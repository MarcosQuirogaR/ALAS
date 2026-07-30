# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Subprocess bridge to the isolated SUAVE environment.

SUAVE 2.5.2 needs an old numpy/scipy/scikit-learn/matplotlib stack under
Python 3.10 (see ``scripts/provision_suave_venv.py``), incompatible with
ALAS's own Python 3.10+/numpy 2.x environment. :func:`run_mission` shells
out to that isolated venv's interpreter, running
``external tools/suave_runner/run_mission.py`` as a subprocess, and parses the
CSV + summary JSON it writes back into a :class:`MissionResult`.
"""

from __future__ import annotations

import csv
import io
import json
import os
import shutil
import subprocess
import tempfile
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

from ..paths import app_root, resolve_tool_dir
from ..proc import no_window_kwargs


def _default_runner_dir() -> Path:
    # Frozen-build-aware: resolves "external tools/suave_runner" next to the
    # real app exe, not inside the PyInstaller extraction cache (see
    # alas/paths.py). In a dev checkout this is the repo-root copy.
    return resolve_tool_dir(Path("external tools") / "suave_runner")


def _default_venv_dir() -> Path:
    return app_root() / ".suave-venv"


def venv_python(venv_dir: Optional[Path] = None) -> Path:
    """Path to the isolated SUAVE venv's interpreter.

    ``venv_dir`` is normally ``config.mission.suave_venv_dir`` (repo-root-
    relative, editable in Advanced Settings -> Mission Analysis) resolved to
    an absolute path by the caller; defaults to ``<app root>/.suave-venv``.
    """
    return (venv_dir or _default_venv_dir()) / "Scripts" / "python.exe"


@dataclass
class MissionResult:
    """Outcome of one SUAVE mission run.

    ``columns`` carries every column ``export_data.py`` writes (flight
    conditions, aerodynamic coefficients/forces, drag breakdown, weight/fuel
    -- the same breadth ``suave_example.py``'s own ``plot_mission()`` plotted,
    not just altitude/speed/mass) keyed by CSV header name, e.g.
    ``mission.columns["CL"]``, ``mission.columns["Thrust_N"]``. The
    properties below are convenience accessors for the columns the route
    globe / mission-profile plot need directly.
    """

    status: str  # "ok" | "not_configured" | "error"
    error: Optional[str] = None
    columns: Dict[str, List] = field(default_factory=dict)
    summary: Dict[str, Any] = field(default_factory=dict)
    # Where the flight-data CSV lives on disk, if anywhere: a caller-supplied
    # work_dir keeps its copy (and this points into it); a run_mission-owned
    # temp dir is deleted before returning (every earlier version of this
    # module leaked one temp dir per successful mission run), so this is None
    # until a caller (e.g. DesignPipeline._export_mission_csv) re-writes
    # csv_text somewhere durable and updates it.
    csv_path: Optional[Path] = None
    # The CSV's full raw text, always populated on status="ok" -- the
    # in-memory source of truth `columns` was parsed from, kept so the file
    # can be re-materialized (outputs/ export) without depending on a temp
    # file outliving this call.
    csv_text: Optional[str] = None

    @property
    def time_s(self) -> List[float]:
        return self.columns.get("Time_s", [])

    @property
    def altitude_m(self) -> List[float]:
        return self.columns.get("Altitude_m", [])

    @property
    def tas_m_s(self) -> List[float]:
        return self.columns.get("TAS_m_s", [])

    @property
    def mass_kg(self) -> List[float]:
        return self.columns.get("Mass_kg", [])

    @property
    def segment(self) -> List[str]:
        return self.columns.get("Segment", [])


def _runner_dir(runner_dir: Optional[Path] = None) -> Path:
    return runner_dir or _default_runner_dir()


def _runner_script(runner_dir: Optional[Path] = None) -> Path:
    return _runner_dir(runner_dir) / "run_mission.py"


def suave_env_available(
    venv_dir: Optional[Path] = None, runner_dir: Optional[Path] = None
) -> bool:
    """Whether the isolated SUAVE venv (and the runner script it invokes)
    have been provisioned."""
    return venv_python(venv_dir).exists() and _runner_script(runner_dir).exists()


def venv_is_valid(venv_dir: Path) -> bool:
    """A venv directory is only useful if its interpreter is actually there."""
    try:
        return venv_python(venv_dir).exists()
    except OSError:
        return False


def runner_is_valid(runner_dir: Path) -> bool:
    try:
        return _runner_script(runner_dir).exists()
    except OSError:
        return False


def _suave_cache_roots() -> List[Path]:
    """Where ``desktop/suave_runtime.go`` extracts bundled SUAVE runtimes.
    Mirrors its ``suaveCacheRoot()``; kept in sync by hand (one small path
    expression, versus threading another env var through the launcher)."""
    roots: List[Path] = []
    if os.name == "nt":
        local = os.environ.get("LOCALAPPDATA")
        if local:
            roots.append(Path(local) / "ALAS" / "suave-cache")
    else:
        xdg = os.environ.get("XDG_CACHE_HOME")
        base = Path(xdg) if xdg else Path.home() / ".cache"
        roots.append(base / "ALAS" / "suave-cache")
    return roots


def discover_extracted_runtimes() -> List[Tuple[Path, Path]]:
    """Every *usable* (venv, runner) pair already extracted under the launcher's
    SUAVE cache, newest first.

    Last-resort tier for resolution. The launcher normally hands the sidecar the
    current build's extracted runtime via ``ALAS_SUAVE_VENV_DIR``, but if
    that extraction failed (antivirus, disk pressure, a killed first launch) the
    env var is simply absent and mission analysis reports "not configured" even
    though a perfectly good runtime from a previous build is still sitting in
    the cache. These runtimes are self-contained and pinned by us, so reusing
    one is strictly better than refusing to run.
    """
    found: List[Tuple[Path, Path]] = []
    for root in _suave_cache_roots():
        if not root.is_dir():
            continue
        try:
            entries = sorted(
                root.iterdir(), key=lambda p: p.stat().st_mtime, reverse=True
            )
        except OSError:
            continue
        for entry in entries:
            if not entry.is_dir():
                continue
            venv, runner = entry / "suave-venv", entry / "suave_runner"
            if venv_is_valid(venv) and runner_is_valid(runner):
                found.append((venv, runner))
    return found


def run_mission(
    vehicle_request: Dict[str, Any],
    mission_request: Dict[str, Any],
    *,
    venv_dir: Optional[Path] = None,
    runner_dir: Optional[Path] = None,
    work_dir: Optional[Path] = None,
    timeout_s: float = 900.0,
) -> MissionResult:
    """Run a SUAVE mission for the given vehicle/mission request dicts.

    Returns ``status="not_configured"`` (rather than raising) if the SUAVE
    venv hasn't been set up yet, so callers (e.g. the GUI) can degrade
    gracefully instead of crashing the whole results view. ``venv_dir``
    overrides the default ``<app root>/.suave-venv``; ``runner_dir``
    overrides the default ``<app root>/external tools/suave_runner`` (both
    normally ``config.mission.suave_venv_dir``/``suave_runner_dir``, resolved
    to absolute paths by the caller).

    Both defaults now resolve via :func:`alas.paths.app_root` /
    :func:`alas.paths.resolve_tool_dir`, which locate the real install
    directory even in a frozen/packaged build (the Go launcher exports
    ``ALAS_APP_DIR``). So a packaged ALAS.exe with the SUAVE venv +
    ``external tools/suave_runner`` sitting beside it is auto-located; an
    explicit absolute override (Setup > External Tools > SUAVE) still wins for
    tools kept elsewhere on disk.
    """
    python_exe = venv_python(venv_dir)
    runner_script = _runner_script(runner_dir)
    if not suave_env_available(venv_dir, runner_dir):
        from ..paths import SUAVE_EXTRACT_ERROR_ENV

        detail = (
            f"SUAVE environment not found.\n"
            f"  interpreter : {python_exe} ({'present' if python_exe.exists() else 'MISSING'})\n"
            f"  runner      : {runner_script} ({'present' if runner_script.exists() else 'MISSING'})"
        )
        extract_error = os.environ.get(SUAVE_EXTRACT_ERROR_ENV)
        if extract_error:
            # The launcher had a bundled runtime but could not unpack it -- a
            # very different problem from "never provisioned", and the only
            # place that reason exists.
            detail += (
                f"\n  The bundled SUAVE runtime failed to extract at startup: {extract_error}\n"
                f"  This is usually antivirus blocking the unpack, low disk space, or a "
                f"permissions problem on the app cache directory."
            )
        else:
            detail += (
                "\n  Run scripts/provision_suave_venv.py, or set both paths explicitly under "
                "Setup > External Tools > SUAVE. (A packaged build normally bundles this "
                "automatically.)"
            )
        return MissionResult(status="not_configured", error=detail)

    own_temp_dir = work_dir is None
    work_dir = work_dir or Path(tempfile.mkdtemp(prefix="alas_suave_"))
    work_dir.mkdir(parents=True, exist_ok=True)

    def _cleanup_dir() -> None:
        # Only ever removes a directory this call created itself via
        # tempfile.mkdtemp -- a caller-supplied work_dir is left untouched.
        if own_temp_dir:
            shutil.rmtree(work_dir, ignore_errors=True)

    request_path = work_dir / "request.json"
    csv_path = work_dir / "flight_data.csv"
    summary_path = work_dir / "summary.json"
    request_path.write_text(
        json.dumps({"vehicle": vehicle_request, "mission": mission_request}, indent=2),
        encoding="utf-8",
    )

    try:
        proc = subprocess.run(
            [
                str(python_exe),
                str(runner_script),
                "--request",
                str(request_path),
                "--output-csv",
                str(csv_path),
                "--output-summary",
                str(summary_path),
            ],
            cwd=str(_runner_dir(runner_dir)),
            capture_output=True,
            text=True,
            timeout=timeout_s,
            **no_window_kwargs(),  # no flashing console under the windowed GUI
        )
    except subprocess.TimeoutExpired as exc:
        _cleanup_dir()
        return MissionResult(
            status="error", error=f"SUAVE mission run timed out: {exc}"
        )

    if proc.returncode != 0:
        _cleanup_dir()
        return MissionResult(
            status="error",
            error=f"SUAVE mission run failed (exit {proc.returncode}):\n{proc.stderr or proc.stdout}",
        )

    try:
        summary = (
            json.loads(summary_path.read_text(encoding="utf-8"))
            if summary_path.exists()
            else {}
        )
        csv_text = csv_path.read_text(encoding="utf-8")
        result = MissionResult(status="ok", summary=summary, csv_text=csv_text)
        reader = csv.DictReader(io.StringIO(csv_text))
        for name in reader.fieldnames or []:
            result.columns[name] = []
        for row in reader:
            for name, value in row.items():
                result.columns[name].append(
                    value if name == "Segment" else float(value)
                )
    except Exception as exc:
        # Parsing the subprocess's own output must never be able to crash the
        # caller -- degrade to an error status instead (matches the
        # subprocess-failure paths above), preserving the "run_mission never
        # raises" contract documented in architecture.md.
        _cleanup_dir()
        return MissionResult(
            status="error", error=f"Failed to parse SUAVE mission output: {exc}"
        )

    if own_temp_dir:
        # Everything the caller needs is now in `result` (columns + csv_text
        # + summary), so the whole scratch dir goes -- leaving the CSV behind
        # "in case" leaked one %TEMP% directory per successful mission run,
        # accumulating indefinitely across sessions.
        _cleanup_dir()
    else:
        result.csv_path = csv_path

    return result
