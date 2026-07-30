#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Bundles a self-contained SUAVE mission-analysis runtime -- the isolated
Python 3.10 + old numpy/scipy/scikit-learn/matplotlib venv SUAVE 2.5.2 needs,
plus the vendored SUAVE-2.5.2 source (LGPL-2.1, ``external tools/SUAVE-2.5.2/
LICENSE``) and this repo's own ``external tools/suave_runner`` wrapper -- into
a single archive staged under ``desktop/suave_dist/<goos>-<goarch>/`` so the
Go shell's `//go:embed` (``desktop/embed_suave.go``) picks it up on the next
`wails build`. This is what lets a packaged ALAS.exe run SUAVE mission
analysis with ZERO manual setup: today, without this, mission analysis reports
"not_configured" on a fresh install because nobody has run
``scripts/provision_suave_venv.py`` there.

Mirrors ``scripts/build_sidecar.py``'s structure closely (temp-dir build ->
zip+fingerprint -> OneDrive-lock-tolerant staging into a repo dir go:embed
reads) but needs no PyInstaller step: this bundle is just an interpreter, its
site-packages, and pure-Python source -- run directly via
``<venv>/Scripts/python.exe external tools/suave_runner/run_mission.py`` at
launch, exactly like the dev path already does.

Archive layout (flat at the root, so ``suave_runner/_compat.py``'s own
``Path(__file__).resolve().parents[1] / "SUAVE-2.5.2"`` sibling-lookup keeps
working unmodified after extraction):
    suave-venv/...           (the provisioned Python 3.10 + deps)
    suave_runner/...         (external tools/suave_runner, verbatim)
    SUAVE-2.5.2/...          (external tools/SUAVE-2.5.2, verbatim, incl. LICENSE)

This is `wails.json`'s `*/*` preBuildHooks entry via ``scripts/prebuild.py``
(which runs this after ``build_sidecar.py``), so `wails build` does it
automatically. Can also be run standalone: `uv run python scripts/build_suave_env.py`.
"""

from __future__ import annotations

import hashlib
import platform
import shutil
import sys
import tempfile
import time
import zipfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(Path(__file__).resolve().parent))
from provision_suave_venv import provision  # noqa: E402

SUAVE_SOURCE_DIR = REPO_ROOT / "external tools" / "SUAVE-2.5.2"
SUAVE_RUNNER_SOURCE_DIR = REPO_ROOT / "external tools" / "suave_runner"

BUILD_ROOT = Path(tempfile.gettempdir()) / "alas-suave-build"
BUILD_VENV_DIR = BUILD_ROOT / "suave-venv"

DESKTOP_DIR = REPO_ROOT / "desktop"
DIST_STAGING = DESKTOP_DIR / "suave_dist"

ARCHIVE_NAME = "alas-suave.zip"
FINGERPRINT_NAME = "fingerprint.txt"


def _rmtree_retrying(target: Path, attempts: int = 5, delay_s: float = 1.0) -> None:
    for attempt in range(1, attempts + 1):
        try:
            shutil.rmtree(target)
            return
        except OSError:
            if attempt == attempts:
                raise
            time.sleep(delay_s)


def _rename_retrying(
    source: Path, dest: Path, attempts: int = 10, delay_s: float = 2.0
) -> None:
    for attempt in range(1, attempts + 1):
        try:
            source.rename(dest)
            return
        except OSError:
            if attempt == attempts:
                raise
            time.sleep(delay_s)


def _copytree_retrying(
    source: Path, target: Path, attempts: int = 10, delay_s: float = 3.0
) -> None:
    """See ``build_sidecar.py``'s identical helper -- tolerates transient
    WinError 3/5 from Defender/OneDrive scanning a freshly-written tree."""
    for attempt in range(1, attempts + 1):
        if target.exists():
            shutil.rmtree(target, ignore_errors=True)
        try:
            shutil.copytree(source, target)
            return
        except (OSError, shutil.Error) as exc:
            if attempt == attempts:
                raise
            print(
                f"[build_suave_env] copytree attempt {attempt} failed ({exc}); "
                f"retrying in {delay_s:.0f}s (likely AV/OneDrive lock on source)",
                flush=True,
            )
            time.sleep(delay_s)


def _replace_directory(target: Path, source: Path) -> None:
    """See ``build_sidecar.py``'s identical helper for the full rationale
    (this repo lives under OneDrive on the reference dev machine)."""
    if target.exists():
        try:
            _rmtree_retrying(target, attempts=20, delay_s=3.0)
        except OSError:
            stale = target.with_name(f"{target.name}.stale-{int(time.time())}")
            _rename_retrying(target, stale)
            print(
                f"[build_suave_env] warning: could not delete old {target} "
                f"(still locked, likely by OneDrive sync) -- moved it aside "
                f"to {stale} instead; safe to delete by hand once OneDrive "
                f"catches up",
                flush=True,
            )
    target.parent.mkdir(parents=True, exist_ok=True)
    _copytree_retrying(source, target)


_GOOS_BY_PLATFORM = {"win32": "windows", "linux": "linux", "darwin": "darwin"}
_GOARCH_BY_MACHINE = {
    "AMD64": "amd64",
    "x86_64": "amd64",
    "amd64": "amd64",
    "arm64": "arm64",
    "aarch64": "arm64",
}


def _target_dir_name() -> str:
    goos = _GOOS_BY_PLATFORM.get(sys.platform, sys.platform)
    goarch = _GOARCH_BY_MACHINE.get(platform.machine(), platform.machine().lower())
    return f"{goos}-{goarch}"


# Top-level directories of the vendored SUAVE tree that are never imported at
# runtime. `regression/` is SUAVE's own test suite and alone accounts for
# ~300 MB of the shipped bundle (it carries large reference result sets);
# `doc/` is generated documentation. Only `trunk/` -- the actual SUAVE package
# `external tools/suave_runner/_compat.py` puts on sys.path -- plus the licence
# and small top-level metadata files are needed to run a mission.
_SUAVE_SKIP_TOP_LEVEL = {"regression", "doc"}


def _skip_in_bundle(label: str, rel: Path) -> bool:
    """Whether ``rel`` (relative to its bundle root) should be left out."""
    # Bytecode caches: pure bloat, regenerated on first import.
    if "__pycache__" in rel.parts:
        return True
    if label == "SUAVE-2.5.2" and rel.parts and rel.parts[0] in _SUAVE_SKIP_TOP_LEVEL:
        return True
    return False


def _zip_bundle(archive_path: Path) -> None:
    """Deflate-compress venv + SUAVE source + runner into one archive, flat at
    the root (see module docstring for why the layout matters).

    Skips the parts of the vendored SUAVE tree that are never imported (see
    :data:`_SUAVE_SKIP_TOP_LEVEL`) -- they were being embedded into every
    shipped executable purely because the whole directory was walked."""
    with zipfile.ZipFile(
        archive_path, "w", zipfile.ZIP_DEFLATED, compresslevel=6
    ) as zf:
        for label, root in (
            ("suave-venv", BUILD_VENV_DIR),
            ("suave_runner", SUAVE_RUNNER_SOURCE_DIR),
            ("SUAVE-2.5.2", SUAVE_SOURCE_DIR),
        ):
            for f in sorted(p for p in root.rglob("*") if p.is_file()):
                rel = f.relative_to(root)
                if _skip_in_bundle(label, rel):
                    continue
                zf.write(f, (Path(label) / rel).as_posix())


def _clear_stale_staging_dirs() -> None:
    """Same rationale as build_sidecar.py's identical function: anything left
    here from a previous run's OneDrive-locked fallback would otherwise be
    silently embedded into the shipped exe by `go:embed all:suave_dist`."""
    if not DIST_STAGING.exists():
        return
    for entry in DIST_STAGING.glob("*.stale-*"):
        try:
            _rmtree_retrying(entry, attempts=10, delay_s=2.0)
        except OSError:
            raise SystemExit(
                f"[build_suave_env] {entry} is still locked (OneDrive?) and would "
                f"be embedded into the app binary, bloating it by its full size. "
                f"Delete it by hand (or pause OneDrive sync) and re-run."
            )


def main() -> None:
    if not SUAVE_SOURCE_DIR.exists():
        raise SystemExit(
            f"[build_suave_env] {SUAVE_SOURCE_DIR} is missing -- nothing to bundle."
        )
    if not SUAVE_RUNNER_SOURCE_DIR.exists():
        raise SystemExit(
            f"[build_suave_env] {SUAVE_RUNNER_SOURCE_DIR} is missing -- nothing to bundle."
        )

    _clear_stale_staging_dirs()

    if BUILD_ROOT.exists():
        _rmtree_retrying(BUILD_ROOT)
    BUILD_ROOT.mkdir(parents=True)

    print(
        "[build_suave_env] provisioning a fresh SUAVE venv for bundling...", flush=True
    )
    provision(BUILD_VENV_DIR)

    archive = BUILD_ROOT / ARCHIVE_NAME
    print(f"[build_suave_env] compressing bundle -> {archive.name}", flush=True)
    t0 = time.time()
    _zip_bundle(archive)
    zip_mb = archive.stat().st_size / 1e6
    print(
        f"[build_suave_env] compressed to {zip_mb:.0f} MB in {time.time() - t0:.0f}s",
        flush=True,
    )

    fingerprint = hashlib.sha256(archive.read_bytes()).hexdigest()[:16]

    staging = BUILD_ROOT / "staging"
    if staging.exists():
        shutil.rmtree(staging)
    staging.mkdir(parents=True)
    shutil.move(str(archive), staging / ARCHIVE_NAME)
    (staging / FINGERPRINT_NAME).write_text(fingerprint, encoding="utf-8")

    target = DIST_STAGING / _target_dir_name()
    _replace_directory(target, staging)
    print(f"[build_suave_env] staged bundled SUAVE runtime at {target}", flush=True)


if __name__ == "__main__":
    main()
