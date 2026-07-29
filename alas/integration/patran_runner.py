# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""Patran headless deformation-plot export: builds a PCL session file per
static load case and batch-replays it to produce a PNG (the same
deformation view MSC Patran's own interactive GUI shows), with no GUI
window ever appearing.

Generalizes a *real recorded* interactive session (a Patran ``.jou``
journal produced while inspecting a SOL 101 result by hand) into a
parameterized template, rather than guessing PCL syntax from documentation.
Two batch-mode-specific quirks that journal's own commands hit, both
confirmed directly against a real install and worked around here:

* ``uil_file_rebuild.start(...)`` (the recorded journal's own new-database
  call) raises an interactive "journal name conflicts -- continue?"
  confirmation that batch mode auto-answers NO, silently leaving the
  database never actually opened -- every subsequent PDB call then fails
  with "Database accessed is not open", cascading into a crash.
  ``uil_file_new.go(template_db, new_db_stem)`` (found in Patran's own
  bundled ``tutorial_apps/examples/dynamo/dyn_pcl/dyn_dbase.pcl``) creates
  the same new-database-from-template result without that dialog.
* ``gm_write_image(..., "Increment", ...)`` appends its own ``_1`` suffix
  to the filename given -- the PNG is looked up by glob after the run
  rather than assumed to land at the exact requested path.

Opt-in (``StructuresConfig.run_patran_export``, default off) and
non-fatal: mirrors NASTRAN's own contract (see
:mod:`alas.integration.nastran_runner`'s module docstring) -- any
failure (executable/template.db not found, timeout, a Patran crash) comes
back as a non-"ok" status/error rather than raising, so a normal pipeline
run never fails because Patran isn't installed or one render didn't work.
"""

from __future__ import annotations

import os
import shutil
import subprocess
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional

from .nastran_runner import _kill_process_tree, _tail
from ..paths import resolve_tool_exe
from ..proc import no_window_kwargs

# Isometric-ish default view, taken verbatim from the recorded reference
# session's final camera orientation (see module docstring).
_VIEW_AA = (-57.552925, -8.686753, 111.595818)


def _session_script(
    template_db: Path,
    db_stem: Path,
    bdf_path: Path,
    op2_path: Path,
    h5_path: Path,
    subcase_num: int,
    png_stem: Path,
) -> str:
    """Builds the PCL session text for one load case's deformation plot.

    Direct generalization of the recorded reference journal: same PCL call
    sequence and ``msc_dra_add_param`` block, only the paths and subcase
    number vary per call.
    """
    sc = f"SC{subcase_num}:"
    return (
        "\n".join(
            [
                f'uil_file_new.go( "{template_db}", "{db_stem}" )',
                f'set_current_dir( "{db_stem.parent}" )',
                f'nastran_input_import( "{bdf_path}", "default_group", 11, '
                "[TRUE, TRUE, TRUE, TRUE, TRUE, TRUE, TRUE, TRUE, FALSE, TRUE, TRUE], "
                "[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], "
                "[-2000000000, -2000000000, -2000000000, -2000000000, -2000000000, -2000000000, "
                "-2000000000, -2000000000, 0, 0, 0] )",
                f'op2_to_hdf5_translate( "{op2_path}", FALSE, "{h5_path}", TRUE )',
                "msc_dra_init_stream(  )",
                f'msc_dra_add_param( "DATABASE", "{db_stem}.db" )',
                'msc_dra_add_param( "JOBNAME", "wing_mesh" )',
                f'msc_dra_add_param( "RESULTS FILE", "{h5_path}" )',
                'msc_dra_add_param( "OBJECT", "Result Entities" )',
                'msc_dra_add_param( "ANALYSIS TYPE", "Structural" )',
                'msc_dra_add_param( "DIVISION TOLERANCE", "1.0E-8" )',
                'msc_dra_add_param( "NUMERICAL TOLERANCE", "1.0E-4" )',
                'msc_dra_add_param( "MODEL TOLERANCE", "0.0049999999" )',
                'msc_dra_add_param( "OBJECTIVE FUNCTION", "ON" )',
                'msc_dra_add_param( "DESIGN CONSTRAINTS", "ON" )',
                'msc_dra_add_param( "DESIGN VARIABLES", "ON" )',
                'msc_dra_add_param( "COMBINE RESULTCASES", "ON" )',
                'msc_dra_add_param( "COMBINE MODULES", "OFF" )',
                'msc_dra_add_param( "SPLINE IMPORT DATA", "OFF" )',
                'msc_dra_add_param( "SPLINE POST DATA", "ZERO" )',
                'msc_dra_add_param( "ROTATIONAL NODAL RESULTS", "ON" )',
                'msc_dra_add_param( "STRESS/STRAIN INVARIANTS", "OFF" )',
                'msc_dra_add_param( "PRINCIPAL DIRECTIONS", "OFF" )',
                'msc_dra_add_param( "CREATE P-ORDER FIELD", "OFF" )',
                'msc_dra_add_param( "ELEMENT RESULTS POSITIONS", "Both        " )',
                'msc_dra_add_param( "NASTRAN VERSION", "2026.1" )',
                'msc_dra_add_param( "TITLE DESCRIPTION", "ON" )',
                "msc_dra_finish_stream(  )",
                f'analysis_import( "MSC.Nastran", "wing_mesh", "Attach HDF5 Results File", "{h5_path}", TRUE )',
                f'op2hdf5_import2( "{h5_path}", "ON", "OFF", "OFF", "BOTH", FALSE )',
                "res_dra_detach_file( 1, 150, 25 )",
                'res_display_tool_unpost( "Fringe", "default_Fringe" )',
                f'res_data_load_dbresult( 0, "Nodal", "Vector", "{sc}", "Static subcase", "Displacements", '
                '"Translational", "(NON-LAYERED)", "", "Global", "", "", "" )',
                'res_data_title( 0, "Nodal", "Vector", 1, '
                '["$POFF@@@$PT: @@@$LCN, @@@$SCN, @@@$PRN, @@@$SRN, @@@$DRVL"] )',
                'res_display_deformation_create( "", "Elements", 0, [""], 10, '
                '["DeformedStyle:White,Solid,1,Wireframe", "DeformedScale:Model=0.1", '
                '"UndeformedStyle:ON,Blue,Solid,1,Wireframe", "TitleDisplay:ON", "MinMaxDisplay:ON", '
                '"ScaleFactor:1.", "LabelStyle:Exponential, 12, White, 3", "DeformDisplay:Resultant", '
                '"DeformComps:OFF,OFF,OFF", "RelativeToGeom:ORIG"] )',
                'res_display_deformation_post( "", 0 )',
                f"ga_view_aa_set( {_VIEW_AA[0]}, {_VIEW_AA[1]}, {_VIEW_AA[2]} )",
                f'gm_write_image( "PNG", "{png_stem}.png", "Increment", 0., 0., 1., 1., 10, "Viewport" )',
            ]
        )
        + "\n"
    )


@dataclass
class PatranRunOutcome:
    ok: bool
    detail: str


def _run_patran(
    session_path: Path, exe_path: Path, png_stem: Path, timeout_s: float
) -> PatranRunOutcome:
    """Invokes ``exe_path -b -graphics -sfp <session_path.name>``. Never
    raises. Success is judged by whether a PNG matching ``png_stem`` shows
    up afterward, not by return code: a Patran-internal crash mid-session
    (confirmed directly, e.g. from a database that failed to open) still
    exits 0."""
    cmd = [str(exe_path), "-b", "-graphics", "-sfp", session_path.name]
    popen_kwargs = (
        dict(no_window_kwargs()) if os.name == "nt" else {"start_new_session": True}
    )
    try:
        proc = subprocess.Popen(
            cmd,
            cwd=str(session_path.parent),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            **popen_kwargs,
        )
    except OSError as exc:
        return PatranRunOutcome(False, f"Failed to launch {exe_path}: {exc}")

    try:
        stdout, stderr = proc.communicate(timeout=timeout_s)
    except subprocess.TimeoutExpired:
        _kill_process_tree(proc)
        proc.communicate()
        return PatranRunOutcome(
            False,
            f"Timed out after {timeout_s:.0f}s running: {' '.join(cmd)} "
            "(process tree force-killed)",
        )

    hits = sorted(png_stem.parent.glob(png_stem.name + "*.png"))
    if not hits:
        return PatranRunOutcome(
            False,
            f"Patran exited (code {proc.returncode}) but wrote no {png_stem.name}*.png in "
            f"{png_stem.parent}.\nstdout (tail):\n{_tail(stdout)}\nstderr (tail):\n{_tail(stderr)}",
        )
    return PatranRunOutcome(True, str(hits[0]))


@dataclass
class PatranExportResult:
    status: str = "not_run"  # "ok" | "error" | "not_run"
    error: Optional[str] = None
    png_paths: Dict[str, Path] = field(
        default_factory=dict
    )  # load-case name -> PNG path


def run_patran_export(
    work_dir: Path,
    repo_root: Path,
    patran_exe_path: str,
    load_case_names: List[str],
    timeout_s: float = 300.0,
) -> PatranExportResult:
    """Renders one deformation-plot PNG per name in ``load_case_names``
    (in SOL 101 subcase order -- see :mod:`alas.physics.
    structural_loads`'s ``load_cases``, whose order is where "SC1"/"SC2"/
    "SC3" come from), reading ``work_dir/wing_mesh.bdf`` and
    ``work_dir/sol101/wing_sol101.op2`` (the files
    :func:`alas.integration.nastran_runner.run_nastran_analysis`
    itself writes -- call this only after that succeeds). Each load case
    gets its own subfolder (own Patran database/session/PNG), the same
    per-analysis-subfolder convention the NASTRAN runner uses, so a crashed
    or retried render never contaminates another case's files.
    """
    result = PatranExportResult()
    exe_path = resolve_tool_exe(patran_exe_path, Path(repo_root))
    if exe_path is None:
        result.status = "error"
        result.error = (
            f"Patran executable not found at {Path(patran_exe_path)}"
            if str(patran_exe_path).strip()
            else "Patran executable not configured (Setup > External Tools)"
        )
        return result

    template_db = exe_path.parent.parent / "template.db"
    if not template_db.exists():
        result.status = "error"
        result.error = f"Patran template.db not found at {template_db} (expected next to the Patran install root)"
        return result

    work_dir = Path(work_dir)
    bdf_path = work_dir / "wing_mesh.bdf"
    op2_path = work_dir / "sol101" / "wing_sol101.op2"
    if not bdf_path.exists() or not op2_path.exists():
        result.status = "error"
        result.error = (
            f"Missing {bdf_path.name} or {op2_path.name} -- run NASTRAN SOL 101 first"
        )
        return result

    patran_dir = work_dir / "patran"
    outcomes: Dict[str, PatranRunOutcome] = {}
    for i, name in enumerate(load_case_names):
        case_dir = patran_dir / name
        # Wipe any previous run's contents first: gm_write_image's own
        # "Increment" naming mode appends _1/_2/_3/... rather than
        # overwriting, so a stale case_dir both accumulates PNGs indefinitely
        # AND can make _run_patran's glob-based success check find an OLD
        # image and report success even if THIS run's render actually failed.
        shutil.rmtree(case_dir, ignore_errors=True)
        case_dir.mkdir(parents=True, exist_ok=True)
        db_stem = case_dir / "preview"
        h5_path = case_dir / "wing_sol101.op2.h5"
        png_stem = case_dir / f"deform_{name}"

        session_text = _session_script(
            template_db, db_stem, bdf_path, op2_path, h5_path, i + 1, png_stem
        )
        session_path = case_dir / "render.ses"
        session_path.write_text(session_text, encoding="utf-8")

        outcomes[name] = _run_patran(session_path, exe_path, png_stem, timeout_s)

    failures = [f"{name}: {o.detail}" for name, o in outcomes.items() if not o.ok]
    if failures and len(failures) == len(outcomes):
        result.status = "error"
        result.error = "\n".join(failures)
        return result

    result.status = "ok"
    result.png_paths = {name: Path(o.detail) for name, o in outcomes.items() if o.ok}
    if failures:
        result.error = "Some load cases failed:\n" + "\n".join(failures)
    return result
