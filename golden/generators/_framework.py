# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Shared plumbing for the golden-data generators.

Every fixture in ``golden/`` is produced by a generator in this directory,
run against the Python implementation. The generators live here rather than in
the reference repository so that repository stays exactly as it was published:
nothing about validating this port should require editing the thing being
validated.

What a fixture has to carry, beyond the numbers, is enough provenance to be
reproducible. A fixture generated from a working tree that has since moved on
is not evidence of anything, so the manifest records the commit it came from
and refuses to pretend a dirty tree is that commit.

Floating-point values go through ``json.dump`` unchanged. Python's float
repr is the shortest string that round-trips exactly, which is precisely the
property needed here -- the fixture holds the same IEEE-754 doubles the
reference produced, not a decimal approximation of them.
"""

from __future__ import annotations

import json
import os
import platform
import subprocess
import sys
from pathlib import Path
from typing import Any

GOLDEN_DIR = Path(__file__).resolve().parent.parent

# Location of the Python reference implementation these generators read. It is
# not part of this repository, so it is taken from the environment and only
# falls back to a sibling checkout.
ALAS_ROOT = Path(
    os.environ.get("ALAS_PYTHON_REFERENCE", str(GOLDEN_DIR.parent.parent / "ALAS-python"))
)

# The reference uses two virtual environments. A family manifest used to put
# this fact at family scope, which made a later SUAVE fixture overwrite the
# environment of an earlier AeroSandbox fixture. Keep the active runtime in
# this process so each write records the fixture that actually ran.
_ACTIVE_RUNTIME = "alas"
_SUAVE_VERSION = "2.5.2"
_SUAVE_TREE = "external tools/SUAVE-2.5.2/trunk"
# The atmosphere the reference ran on. ``add_alas_to_path`` substitutes the
# U.S. Standard Atmosphere, 1976 for AeroSandbox's fitted model (see
# ``_us1976.py``), because that is what the product evaluates; the fixture
# records which one produced it.
_ATMOSPHERE = "aerosandbox-default"


def alas_baseline() -> str:
    """The reference implementation commit these fixtures describe.

    Raises when the tree is dirty. A fixture is a claim that the reference
    produced these numbers at a named revision; if uncommitted edits are in
    play, that claim cannot be checked later and should not be recorded.
    """
    def git(*args: str) -> str:
        return subprocess.run(
            ["git", *args],
            cwd=ALAS_ROOT,
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()

    if git("status", "--porcelain"):
        raise SystemExit(
            f"{ALAS_ROOT} has uncommitted changes; commit them (or run the "
            "generator from a clean checkout of the reference) so the "
            "fixtures name a revision that can be checked out again"
        )
    return git("rev-parse", "HEAD")


def environment() -> dict[str, Any]:
    """Versions that could plausibly move a number."""
    versions = {"python": sys.version.split()[0], "platform": platform.platform()}
    for module in ("numpy", "scipy", "aerosandbox", "shapely", "matplotlib", "neuralfoil", "PIL"):
        try:
            versions[module] = __import__(module).__version__
        except Exception:  # noqa: BLE001 - absence is the information wanted
            versions[module] = "absent"
    # The BLAS/LAPACK build can move the last digits of a double.
    try:
        import numpy

        config = numpy.show_config(mode="dicts")
        blas = config.get("Build Dependencies", {}).get("blas", {})
        versions["numpy_blas"] = {
            key: blas.get(key) for key in ("name", "version", "openblas configuration") if key in blas
        }
    except Exception:  # noqa: BLE001 - absence is the information wanted
        versions["numpy_blas"] = "unknown"
    return versions


def _write_atomic(path: Path, text: str) -> None:
    """Replace ``path`` with ``text`` through a temporary file in the same folder."""
    temporary = path.with_name(path.name + ".tmp")
    with temporary.open("w", encoding="utf-8", newline="\n") as handle:
        handle.write(text)
    os.replace(temporary, path)


def _fixture_environment(runtime: str) -> dict[str, Any]:
    """Return environment metadata, including the selected runtime identity."""
    if runtime not in {"alas", "suave"}:
        raise ValueError(f"unknown generator runtime: {runtime}")
    versions = environment()
    versions["atmosphere"] = _ATMOSPHERE
    if runtime == "suave":
        versions["suave"] = {"version": _SUAVE_VERSION, "tree": _SUAVE_TREE}
    return versions


def write(family: str, name: str, payload: Any, *, description: str) -> Path:
    """Write one fixture and refresh its family manifest."""
    directory = GOLDEN_DIR / family
    directory.mkdir(parents=True, exist_ok=True)

    path = directory / f"{name}.json"
    # Provenance first: a dirty reference tree aborts before any file changes,
    # so a fixture is never replaced while its manifest still names the old run.
    commit = alas_baseline()
    runtime_environment = _fixture_environment(_ACTIVE_RUNTIME)
    fixture_text = json.dumps(payload, indent=2, sort_keys=True) + "\n"

    manifest_path = directory / "manifest.json"
    manifest: dict[str, Any] = {"fixtures": {}}
    if manifest_path.exists():
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))

    # Family-level provenance was ambiguous when one directory mixed the two
    # virtual environments. Remove those legacy fields as each family is
    # migrated, and keep the claim beside the fixture it describes.
    manifest.pop("alas_commit", None)
    manifest.pop("environment", None)
    manifest.setdefault("fixtures", {})[name] = {
        "generator": Path(sys.argv[0]).name,
        "description": description,
        "runtime": _ACTIVE_RUNTIME,
        "alas_commit": commit,
        "environment": runtime_environment,
    }

    _write_atomic(path, fixture_text)
    _write_atomic(manifest_path, json.dumps(manifest, indent=2, sort_keys=True) + "\n")

    print(f"wrote {path.relative_to(GOLDEN_DIR)}")
    return path


def add_suave_to_path() -> None:
    """Put the vendored SUAVE tree on ``sys.path``.

    SUAVE 2.5.2 predates the removal of the ABC aliases from ``collections``,
    which its vendored copy of pint still imports, and that pint also wants
    ``pkg_resources``. The reference implementation already solves both in the
    shim its mission runner imports; that shim is executed here rather than
    reproduced, so the generators cannot drift from the environment the
    reference actually runs SUAVE in.

    Must therefore run under the interpreter in ``.suave-venv``, which is where
    the pinned numpy and setuptools live.
    """
    global _ACTIVE_RUNTIME
    _ACTIVE_RUNTIME = "suave"
    runner = ALAS_ROOT / "external tools" / "suave_runner"
    if str(runner) not in sys.path:
        sys.path.insert(0, str(runner))

    import _compat  # noqa: F401 - imported for its side effects on sys.path


def add_alas_to_path() -> None:
    """Put the reference implementation on ``sys.path``."""
    global _ACTIVE_RUNTIME, _ATMOSPHERE
    _ACTIVE_RUNTIME = "alas"
    if str(ALAS_ROOT) not in sys.path:
        sys.path.insert(0, str(ALAS_ROOT))
    import _us1976  # noqa: PLC0415 - generator-side substitution

    _us1976.use_us1976()
    _ATMOSPHERE = "us1976 (generator substitution for aerosandbox.Atmosphere)"


def use_suave_geopotential_altitude() -> None:
    """Have SUAVE's US 1976 atmosphere read altitudes as geopotential.

    For generators whose fixture feeds an altitude into the product's mission
    or engine sizing, which take pressure (geopotential) altitude. See
    ``_us1976.use_geopotential_input_in_suave``.
    """
    global _ATMOSPHERE
    import _us1976  # noqa: PLC0415

    _us1976.use_geopotential_input_in_suave()
    _ATMOSPHERE = "us1976 (SUAVE, altitude read as geopotential)"
