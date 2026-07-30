#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Provisions the isolated Python 3.10 venv SUAVE 2.5.2 needs (old numpy/scipy/
scikit-learn/matplotlib, incompatible with ALAS's own numpy 2.x sidecar
environment -- see ``alas/integration/suave_bridge.py``'s module
docstring). Recreates the one-time dev setup step this repo used to document
as ``scripts/setup_suave_env.ps1`` (that script no longer exists in the repo;
this is its replacement, and is also reused by ``scripts/build_suave_env.py``
to provision the venv that gets bundled into a packaged ALAS.exe).

SUAVE itself is NOT installed into this venv -- ``external tools/suave_runner/
_compat.py`` already inserts ``external tools/SUAVE-2.5.2/trunk`` onto
``sys.path`` directly, so this venv only ever needs the four scientific
packages below.

Usage (dev, one-time, from the repo root):
    uv run python scripts/provision_suave_venv.py

Or to provision an arbitrary location (used by build_suave_env.py):
    uv run python scripts/provision_suave_venv.py --venv-dir <path>
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_VENV_DIR = REPO_ROOT / ".suave-venv"

# Exact versions already validated against SUAVE 2.5.2's vendored pint plugin
# and the vehicle_builder.py/mission_builder.py/export_data.py wrapper scripts
# in external tools/suave_runner/ (matches the working dev .suave-venv this
# was reverse-engineered from). Do not bump casually -- SUAVE 2.5.2 was last
# tested against precisely this stack; a newer numpy/scipy could silently
# change SUAVE's own numerics or break its (unmaintained) 2.5.2-era API usage.
#
# setuptools is NOT one of SUAVE's own scientific dependencies -- it's needed
# because `uv venv` (unlike stdlib `venv`/`virtualenv`) does not seed
# pip/setuptools/wheel into new environments by default, and SUAVE's vendored
# `Plugins/pint/__init__.py` does `import pkg_resources` (part of setuptools)
# unconditionally. Without it: "ModuleNotFoundError: No module named
# 'pkg_resources'" the moment `import SUAVE` runs. MUST be pinned <81:
# setuptools began actually REMOVING pkg_resources starting around v81/82 (its
# own deprecation warning says so verbatim: "pin to Setuptools<81") -- an
# unpinned "setuptools" resolves to the latest (83.x as of this writing, which
# has already dropped it), reproducing the exact same ModuleNotFoundError even
# with setuptools nominally installed. <81 matches the working dev
# .suave-venv's own setuptools (80.10.2), confirmed directly to still work.
REQUIREMENTS = [
    "numpy==1.26.4",
    "scipy==1.11.4",
    "scikit-learn==1.7.2",
    "matplotlib==3.7.5",
    "setuptools<81",
    # SUAVE's own SUAVE/Plots/Performance/Mission_Plots.py does an
    # unconditional `import plotly.graph_objects` -- triggered merely by
    # `import SUAVE` (via SUAVE/__init__.py's `from . import Plots`), even
    # though ALAS's own suave_runner never calls into SUAVE's plotting
    # utilities itself. Without it: "ModuleNotFoundError: No module named
    # 'plotly'" the moment `import SUAVE` runs.
    "plotly==6.8.0",
]


def _venv_python(venv_dir: Path) -> Path:
    if sys.platform == "win32":
        return venv_dir / "Scripts" / "python.exe"
    return venv_dir / "bin" / "python"


def provision(venv_dir: Path) -> None:
    print(f"[provision_suave_venv] creating Python 3.10 venv at {venv_dir}", flush=True)
    subprocess.run(["uv", "venv", "--python", "3.10", str(venv_dir)], check=True)

    python_exe = _venv_python(venv_dir)
    print(f"[provision_suave_venv] installing {', '.join(REQUIREMENTS)}", flush=True)
    # --no-config: CRITICAL. ALAS's own pyproject.toml sets
    # `[tool.uv] override-dependencies = ["numpy>=2,<3"]` for the MAIN
    # project (a deliberate override so aerosandbox's numpy 2.x requirement
    # wins over pyNastran's own numpy<2 pin -- see that comment). `uv pip
    # install` auto-discovers the nearest pyproject.toml from the CURRENT
    # WORKING DIRECTORY and applies its [tool.uv] settings even when
    # installing into a completely unrelated --python target -- confirmed
    # directly: without --no-config, running this from the repo root (which
    # `uv run python scripts/...` and `wails build`'s preBuildHooks both do)
    # silently installed numpy 2.2.6 into THIS venv despite the explicit
    # numpy==1.26.4 pin below, which would have shipped a completely broken
    # bundled SUAVE runtime (SUAVE 2.5.2 has real numpy<2 API
    # incompatibilities -- that's the entire reason this isolated venv
    # exists). --no-config makes this venv's resolution independent of
    # whatever directory it happens to be invoked from.
    # --link-mode copy: uv defaults to hardlinking from its cache, which fails
    # ("cloud operation ... incompatible hard links") when the cache and the
    # target venv sit on different sides of a OneDrive/cloud-sync boundary --
    # observed directly on the reference dev machine (repo under OneDrive).
    # Copying is slightly slower but always correct, same trade-off
    # build_sidecar.py already accepts elsewhere in this packaging pipeline.
    subprocess.run(
        [
            "uv",
            "pip",
            "install",
            "--python",
            str(python_exe),
            "--no-config",
            "--link-mode",
            "copy",
            *REQUIREMENTS,
        ],
        check=True,
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--venv-dir",
        type=Path,
        default=DEFAULT_VENV_DIR,
        help="Where to create the venv (default: <repo root>/.suave-venv).",
    )
    args = parser.parse_args()
    provision(args.venv_dir)
    print(f"[provision_suave_venv] done -> {args.venv_dir}", flush=True)


if __name__ == "__main__":
    main()
