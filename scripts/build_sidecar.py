#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Freezes the FastAPI sidecar (alas/sidecar/server.py) into a standalone
PyInstaller one-dir bundle, then stages it under desktop/sidecar_dist/<goos>-
<goarch>/ so the Go shell's `//go:embed` (desktop/embed_sidecar.go) picks it
up on the next `wails build`. See scripts/packaging/alas_sidecar.spec
for what actually goes into the bundle, and desktop/sidecar.go for how the
Go shell locates and runs the frozen result at app startup.

This is `wails.json`'s "*/*" preBuildHooks entry, so `wails build` runs it
automatically -- see that file's comment for why the hook is keyed "*/*"
and invoked as `uv run python ../../../scripts/build_sidecar.py` (a
cwd-relative path is the only addressing Wails' hook system offers; see
that file for the exact directory-nesting assumption this relies on). It
can also be run directly for a standalone rebuild: `uv run python
scripts/build_sidecar.py`.

PyInstaller cannot cross-compile: this must run on the same OS/arch you are
shipping for. A Windows .exe and a Linux binary each need their own native
build machine (or CI runner) -- there is no "build both from one host" mode.
Running `wails build -platform linux/amd64` on a Windows host will still
freeze a *Windows* sidecar via this script (there is no way around that),
so cross-platform releases need one CI job per target OS, each running
`wails build` natively.
"""

from __future__ import annotations

import hashlib
import platform
import shutil
import subprocess
import sys
import tempfile
import time
import zipfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SPEC_FILE = REPO_ROOT / "scripts" / "packaging" / "alas_sidecar.spec"

# Scratch space for PyInstaller's own intermediate output (thousands of
# files, several hundred MB once scipy/aerosandbox/pyvista are in it) --
# deliberately kept outside the repo, in the OS temp dir, not just outside
# desktop/. This repo lives under OneDrive on the reference dev machine, and
# OneDrive's sync engine transiently locks freshly-written files it hasn't
# finished indexing yet; rebuilding this directory in place under the repo
# hit sporadic `PermissionError` on `rmtree` from exactly that. The final
# staged copy (DIST_STAGING below) still has to live inside the repo for
# go:embed to see it -- _rmtree_retrying() below is what makes deleting
# *that* directory on a rebuild robust to the same issue.
PYINSTALLER_DIST_DIR = Path(tempfile.gettempdir()) / "alas-sidecar-build" / "dist"
PYINSTALLER_WORK_DIR = Path(tempfile.gettempdir()) / "alas-sidecar-build" / "work"

DESKTOP_DIR = REPO_ROOT / "desktop"
DIST_STAGING = DESKTOP_DIR / "sidecar_dist"


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
    """shutil.copytree with retries for transient WinError 3/5 that Windows
    Defender or OneDrive can cause by scanning newly-created DLL directories
    (like PyInstaller's _internal/) for the first few seconds after they are
    written.
    """
    for attempt in range(1, attempts + 1):
        # copytree requires the target to not exist; remove a partial copy
        # left from a failed previous attempt.
        if target.exists():
            shutil.rmtree(target, ignore_errors=True)
        try:
            shutil.copytree(source, target)
            return
        except (OSError, shutil.Error) as exc:
            if attempt == attempts:
                raise
            print(
                f"[build_sidecar] copytree attempt {attempt} failed ({exc}); "
                f"retrying in {delay_s:.0f}s (likely AV/OneDrive lock on source)",
                flush=True,
            )
            time.sleep(delay_s)


def _replace_directory(target: Path, source: Path) -> None:
    """Make `target` a fresh copy of `source`, tolerating a `target` that
    OneDrive still has a lock on (this repo is synced through it, and a
    fresh several-hundred-MB directory tree can stay locked well past a
    short retry budget while OneDrive uploads/indexes it). A rename only
    touches target's own directory entry (NTFS doesn't need to close
    handles on its *contents* for that), so it can succeed even when
    deleting target -- which does need every contained file unlocked --
    still can't; moving the stale directory aside instead of blocking the
    build on it trades a few hundred MB of leftover disk space for the
    build actually finishing.
    """
    if target.exists():
        try:
            _rmtree_retrying(target, attempts=20, delay_s=3.0)
        except OSError:
            stale = target.with_name(f"{target.name}.stale-{int(time.time())}")
            _rename_retrying(target, stale)
            print(
                f"[build_sidecar] warning: could not delete old {target} "
                f"(still locked, likely by OneDrive sync) -- moved it aside "
                f"to {stale} instead; safe to delete by hand once OneDrive "
                f"catches up",
                flush=True,
            )
    target.parent.mkdir(parents=True, exist_ok=True)
    _copytree_retrying(source, target)


BINARY_NAME = "alas-core.exe" if sys.platform == "win32" else "alas-core"

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


def _run_pyinstaller() -> None:
    for stale in (PYINSTALLER_DIST_DIR, PYINSTALLER_WORK_DIR):
        if stale.exists():
            _rmtree_retrying(stale)

    command = [
        "uv",
        "run",
        "--extra",
        "sidecar",
        "--extra",
        "package",
        "pyinstaller",
        str(SPEC_FILE),
        "--distpath",
        str(PYINSTALLER_DIST_DIR),
        "--workpath",
        str(PYINSTALLER_WORK_DIR),
        "--noconfirm",
    ]
    print(f"[build_sidecar] {' '.join(command)}", flush=True)
    subprocess.run(command, cwd=REPO_ROOT, check=True)


ARCHIVE_NAME = "alas-core.zip"
FINGERPRINT_NAME = "fingerprint.txt"


def _zip_onedir(onedir: Path, archive_path: Path) -> None:
    """Deflate-compress the PyInstaller one-dir bundle into a single archive.

    Embedding one compressed file instead of ~9,500 raw ones roughly halves
    the shipped ALAS.exe (scipy/VTK/casadi DLLs compress well) and
    speeds `wails build`'s go:embed step. The decompression cost is paid once
    per build, at extraction into the sidecar cache (see desktop/sidecar.go),
    not on every launch. Deflate specifically because Go's stdlib archive/zip
    can read it -- no new Go dependency.
    """
    files = sorted(p for p in onedir.rglob("*") if p.is_file())
    with zipfile.ZipFile(
        archive_path, "w", zipfile.ZIP_DEFLATED, compresslevel=6
    ) as zf:
        for f in files:
            zf.write(f, f.relative_to(onedir).as_posix())


def _stage_output() -> Path:
    onedir = PYINSTALLER_DIST_DIR / "alas-core"
    built_binary = onedir / BINARY_NAME
    if not built_binary.exists():
        raise SystemExit(
            f"[build_sidecar] PyInstaller reported success but "
            f"{built_binary} is missing -- see scripts/packaging/alas_sidecar.spec"
        )

    # Zip in the same temp workspace (outside OneDrive, see the note on
    # PYINSTALLER_DIST_DIR), then copy the two small-ish artifacts into the
    # repo staging dir go:embed reads from.
    archive = PYINSTALLER_DIST_DIR / ARCHIVE_NAME
    print(f"[build_sidecar] compressing bundle -> {archive.name}", flush=True)
    t0 = time.time()
    _zip_onedir(onedir, archive)
    raw_mb = sum(f.stat().st_size for f in onedir.rglob("*") if f.is_file()) / 1e6
    zip_mb = archive.stat().st_size / 1e6
    print(
        f"[build_sidecar] compressed {raw_mb:.0f} MB -> {zip_mb:.0f} MB "
        f"in {time.time() - t0:.0f}s",
        flush=True,
    )

    # Content fingerprint, precomputed here so the app never has to hash a
    # multi-hundred-MB archive at launch just to decide whether its cached
    # extraction is current (desktop/sidecar.go reads this file instead).
    fingerprint = hashlib.sha256(archive.read_bytes()).hexdigest()[:16]

    staging = PYINSTALLER_DIST_DIR / "staging"
    if staging.exists():
        shutil.rmtree(staging)
    staging.mkdir(parents=True)
    shutil.move(str(archive), staging / ARCHIVE_NAME)
    (staging / FINGERPRINT_NAME).write_text(fingerprint, encoding="utf-8")

    target = DIST_STAGING / _target_dir_name()
    _replace_directory(target, staging)
    return target


def _clear_stale_staging_dirs() -> None:
    # Leftovers from a previous run's _replace_directory() fallback. These
    # aren't named <goos>-<goarch>, so sidecar.go's lookup at runtime always
    # ignores them either way -- but go:embed doesn't know that, and would
    # happily bundle their few hundred MB into the compiled Go binary if
    # they're still sitting here when `wails build` runs. Best-effort: if
    # OneDrive is still holding one, it just waits for a future run.
    if not DIST_STAGING.exists():
        return
    for entry in DIST_STAGING.glob("*.stale-*"):
        try:
            # Worth real retries: anything left here IS embedded into the
            # shipped exe by `go:embed all:sidecar_dist` -- a lingering stale
            # raw tree silently added ~800 MB to a build once.
            _rmtree_retrying(entry, attempts=10, delay_s=2.0)
        except OSError:
            raise SystemExit(
                f"[build_sidecar] {entry} is still locked (OneDrive?) and would "
                f"be embedded into the app binary, bloating it by its full "
                f"size. Delete it by hand (or pause OneDrive sync) and re-run."
            )


def main() -> None:
    _clear_stale_staging_dirs()
    _run_pyinstaller()
    target = _stage_output()
    print(f"[build_sidecar] staged frozen sidecar at {target}", flush=True)


if __name__ == "__main__":
    main()
