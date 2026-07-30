# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Central path resolution for bundled data and externally-provisioned tools.

Every earlier caller derived its roots as ``Path(__file__).resolve().parents[N]``.
That is correct in a dev checkout, but silently wrong in a PyInstaller frozen
build: there ``__file__`` points *inside the one-dir bundle's temp extraction
cache* (see ``desktop/sidecar.go``'s ``sidecarCacheRoot``), not next to the real
``ALAS.exe`` where the user drops their ``external tools/`` folder (MSES
binaries, the SUAVE runner + venv). The result was the exact symptom reported:
"MSES executables not found" / SUAVE "not_configured" even though the tools were
present beside the app.

Two distinct roots matter, and they are NOT the same place once frozen:

* :func:`bundle_root` -- read-only data PyInstaller froze *into* the bundle
  (``alas/data/...``: the airfoil database, navdata, textures -- all listed
  in ``scripts/packaging/alas_sidecar.spec``). Lives under ``sys._MEIPASS``.
* :func:`app_root` -- the real install directory the user interacts with, where
  externally-provisioned tools live beside the app. Located via the
  ``ALAS_APP_DIR`` environment variable the Go launcher sets from
  ``os.Executable()`` (see ``desktop/sidecar.go``'s ``buildCommand``), falling
  back to ``sys.executable``'s directory.

In a dev checkout both collapse back to the repo root, so nothing changes there.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

# Repo root in a dev checkout: alas/paths.py -> parents[1] is the repo root
# (the directory holding pyproject.toml and "external tools/").
_REPO_ROOT = Path(__file__).resolve().parents[1]

# Environment variable the desktop Go launcher sets to the real install dir so a
# frozen sidecar (running from a temp extraction cache) can find tools beside
# the app. Keep this string in sync with desktop/sidecar.go.
APP_DIR_ENV = "ALAS_APP_DIR"

# Environment variables the desktop Go launcher sets when it successfully
# extracted a bundled SUAVE mission-analysis runtime (scripts/build_suave_env.py
# via desktop/suave_runtime.go) -- a fingerprinted OS-temp cache dir, NOT next
# to the app exe, so these are handled separately from APP_DIR_ENV/
# resolve_tool_dir's next-to-exe search. Keep these strings in sync with
# desktop/sidecar.go's addSuaveEnv.
SUAVE_VENV_DIR_ENV = "ALAS_SUAVE_VENV_DIR"
SUAVE_RUNNER_DIR_ENV = "ALAS_SUAVE_RUNNER_DIR"
# Set by the launcher instead of the two above when extracting the bundled
# SUAVE runtime FAILED, carrying the reason. Without it a failed extraction is
# indistinguishable from "SUAVE was never provisioned" -- both just produce
# mission status "not_configured" with no clue which.
SUAVE_EXTRACT_ERROR_ENV = "ALAS_SUAVE_EXTRACT_ERROR"


def is_frozen() -> bool:
    """True when running inside a PyInstaller (or similar) frozen bundle."""
    return bool(getattr(sys, "frozen", False))


def bundle_root() -> Path:
    """Root for read-only data frozen into the bundle (``alas/data/...``).

    ``sys._MEIPASS`` when frozen (PyInstaller's extraction dir), else the repo
    root. Most callers should not need this directly -- modules that ship data
    already resolve it ``Path(__file__)``-relative, which stays valid because
    PyInstaller keeps the ``alas/`` package layout inside the bundle.
    """
    meipass = getattr(sys, "_MEIPASS", None)
    if meipass:
        return Path(meipass)
    return _REPO_ROOT


def app_root() -> Path:
    """The real install directory: where externally-provisioned tools live.

    Frozen build: the ``ALAS_APP_DIR`` env var (set by the Go launcher to
    the Wails app-exe directory) if present, else the directory containing
    ``sys.executable``. Dev checkout: the repo root. Never raises.
    """
    env = os.environ.get(APP_DIR_ENV)
    if env:
        p = Path(env)
        if p.exists():
            return p
    if is_frozen():
        try:
            return Path(sys.executable).resolve().parent
        except Exception:
            return _REPO_ROOT
    return _REPO_ROOT


def user_data_root() -> Path:
    """Writable per-user root for data fetched *after* installation.

    Downloadable assets must not live under :func:`bundle_root`. Frozen, that
    is ``sys._MEIPASS`` -- a per-build fingerprinted extraction cache (see
    ``desktop/sidecar.go``) that a new release replaces wholesale, so anything
    downloaded into it is orphaned by the next update and silently re-fetched.

    This is deliberately a *data* directory, not a cache one: the maintenance
    screen offers cache directories for deletion, and a user reclaiming disk
    should not lose a 10 MB navdata download they chose to install.

    A dev checkout returns the repo root, so a developer's downloads sit
    beside the source exactly where the config defaults point.
    """
    if not is_frozen():
        return _REPO_ROOT
    if os.name == "nt":
        base = os.environ.get("LOCALAPPDATA")
        return (
            Path(base) / "ALAS" if base else Path.home() / "AppData" / "Local" / "ALAS"
        )
    if sys.platform == "darwin":
        return Path.home() / "Library" / "Application Support" / "ALAS"
    xdg = os.environ.get("XDG_DATA_HOME")
    return (Path(xdg) if xdg else Path.home() / ".local" / "share") / "ALAS"


def _data_roots() -> list[Path]:
    """Ordered roots to search for a relative data path, duplicates collapsed."""
    ordered = [app_root(), user_data_root(), bundle_root(), _REPO_ROOT]
    seen: set[str] = set()
    unique: list[Path] = []
    for p in ordered:
        key = str(p)
        if key not in seen:
            seen.add(key)
            unique.append(p)
    return unique


def resolve_data_path(configured: str | os.PathLike[str]) -> Path:
    """Resolve a configured data path (the navdata directory, the Earth texture).

    An absolute ``configured`` is honoured verbatim. A relative one is searched
    against, in order: the install directory (a copy the user placed beside the
    app wins), :func:`user_data_root` (where downloads land), the frozen bundle
    (assets shipped inside the executable), and the repo root (dev checkout).

    When nothing exists yet, the *download* location is returned rather than
    the first candidate, so a "missing, fetch it to here" message names the
    path the downloader will actually write to.
    """
    configured_path = Path(configured)
    if configured_path.is_absolute():
        return configured_path
    for root in _data_roots():
        candidate = root / configured_path
        if candidate.exists():
            return candidate
    return user_data_root() / configured_path


def _candidate_roots() -> list[Path]:
    """Ordered roots to search for a relative tool directory.

    Deliberately tolerant of a few plausible install layouts: tools beside the
    app, in a ``bin/`` subfolder, one level up (e.g. a ``bin/ALAS.exe``
    layout with tools at the install root), and the repo root for dev. Duplicates
    are collapsed while preserving order.
    """
    root = app_root()
    ordered = [root, root / "bin", root.parent, _REPO_ROOT]
    seen: set[str] = set()
    unique: list[Path] = []
    for p in ordered:
        key = str(p)
        if key not in seen:
            seen.add(key)
            unique.append(p)
    return unique


def resolve_tool_dir(configured: str | os.PathLike[str]) -> Path:
    """Resolve a configured (possibly repo-root-relative) tool directory.

    An absolute ``configured`` path is honoured verbatim (this is how a
    ``Setup > External Tools`` override pins tools anywhere on disk). A relative
    path (e.g. the ``"external tools/MSES"`` default) is searched against each
    :func:`_candidate_roots` entry; the first that exists wins. If none exists,
    the first candidate is returned so downstream "not found under {dir}" errors
    still report a sensible, user-recognisable path rather than a temp cache dir.

    An empty/whitespace-only ``configured`` is treated as "unset" and resolves
    to the app root itself rather than ``Path("")``'s surprising ``"."``
    normalisation (see :func:`find_tool_dir`'s note on the ``not_configured``
    bug this caused).
    """
    if not str(configured).strip():
        return app_root()
    configured_path = Path(configured)
    if configured_path.is_absolute():
        return configured_path
    candidates = [root / configured_path for root in _candidate_roots()]
    for candidate in candidates:
        if candidate.exists():
            return candidate
    return candidates[0]


def find_tool_dir(configured: str | os.PathLike[str]) -> Path | None:
    """Like :func:`resolve_tool_dir`, but returns ``None`` instead of a best
    guess when nothing actually exists on disk at any candidate location.

    Used to distinguish "the user has (or hasn't) provisioned this tool
    themselves at the conventional location" from "nothing there, fall back to
    something else" -- e.g. a bundled SUAVE runtime extracted to an OS-temp
    cache dir (see :data:`SUAVE_VENV_DIR_ENV`), which should only be used when
    a user-provisioned copy isn't already sitting next to the app.

    An empty/whitespace-only ``configured`` means "not configured" and returns
    None. Without that guard ``Path("")`` normalises to ``"."``, so every
    candidate root "exists" and this returned the APP ROOT itself -- which then
    won over the bundled-runtime env-var fallback and produced a bogus
    ``<app root>/Scripts/python.exe``, surfacing as ``mission:
    not_configured`` on a packaged build that actually had SUAVE bundled
    correctly. The desktop UI persists blank External-Tools fields as "", so
    this was reachable just by opening that page once.
    """
    if not str(configured).strip():
        return None
    configured_path = Path(configured)
    if configured_path.is_absolute():
        return configured_path if configured_path.exists() else None
    for root in _candidate_roots():
        candidate = root / configured_path
        if candidate.exists():
            return candidate
    return None


def resolve_tool_exe(configured: str | os.PathLike[str], root: Path) -> Path | None:
    """Resolve a configured external-tool *executable* against ``root``.

    Returns None when the tool is unset or absent, so callers can report "not
    configured" rather than trying to launch something.

    Two guards, both load-bearing:

    * A blank ``configured`` returns None before any path arithmetic.
      ``Path("")`` normalises to ``"."``, so ``root / ""`` is ``root`` itself
      -- a directory that satisfies ``exists()`` and then reaches
      ``subprocess``, which fails with a permission error naming the
      repository root instead of saying the tool is unconfigured.
    * The result must be a regular file. ``exists()`` is also true for
      directories, which closes the same gap for any other directory-shaped
      value.

    Unlike :func:`find_tool_dir` this searches only ``root``, because the
    callers that need it already carry an explicit repo root through their own
    signatures rather than inferring one.
    """
    if not str(configured).strip():
        return None
    exe = Path(configured)
    if not exe.is_absolute():
        exe = Path(root) / exe
    return exe if exe.is_file() else None
