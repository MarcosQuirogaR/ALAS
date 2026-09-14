"""Render Mach and pressure contours from a native ALAS OpenFOAM case.

Usage with the ParaView distribution's ``pvpython``::

    pvpython tools/openfoam_render_fields.py CASE_DIR RHO_KG_M3 TEMPERATURE_K [MARKER]
        [--compressible] [--pressure-reference-pa P_REF]

The optional fourth argument selects the exact ``.foam`` marker created by
``paraFoam -vtk -touch``.  Without it, the renderer looks for the standard
case-name marker and then the ALAS ``case.foam`` fallback.

The input fields are native OpenFOAM results.  In low-Mach mode, Mach is a
derived diagnostic ``|U|/a`` and pressure is ``rho*p`` for the documented
incompressible template.  In ``--compressible`` mode, Mach uses the local
static temperature and pressure is ``p - P_REF`` in Pa.  Both scalar bars use
the ParaView Turbo preset while retaining the data-derived ranges; the two
PNGs are written below the case directory and can be loaded by the Airfoil
CFD Results tab.  Each image also carries the solver status and last finite
iteration/time from ``result.json`` (or ``study.json`` when no result
manifest exists); a rendered image is never presented as physical validation.
"""

import json
import math
import sys
from pathlib import Path

from paraview.simple import (
    Calculator,
    ColorBy,
    CreateView,
    GetColorTransferFunction,
    GetScalarBar,
    Delete,
    OpenFOAMReader,
    Render,
    SaveScreenshot,
    Show,
    Text,
    _DisableFirstRenderCameraReset,
)


case = Path(sys.argv[1]).resolve()
rho = float(sys.argv[2])
temperature_k = float(sys.argv[3])
extra_args = sys.argv[4:]
compressible = "--compressible" in extra_args
pressure_reference_pa = 0.0
reference_index = None
if "--pressure-reference-pa" in extra_args:
    reference_index = extra_args.index("--pressure-reference-pa") + 1
    if reference_index >= len(extra_args):
        raise SystemExit("--pressure-reference-pa requires a value")
    pressure_reference_pa = float(extra_args[reference_index])
marker_arg = None
for index, arg in enumerate(extra_args):
    if arg.startswith("--"):
        continue
    if reference_index is not None and index == reference_index:
        continue
    marker_arg = arg
    break
sound_speed = math.sqrt(1.4 * 287.05287 * temperature_k)
output = case / "postProcessing" / "alas-field-figures"
output.mkdir(parents=True, exist_ok=True)
marker = Path(marker_arg).resolve() if marker_arg else case / f"{case.name}.foam"
if not marker.is_file():
    marker = case / "case.foam"
marker.touch(exist_ok=True)

reader = OpenFOAMReader(FileName=str(marker))
# ParaView defaults to SkipZeroTime=1.  An aborted case can legitimately have
# only its fully finite initial field at time 0, so keep that field available
# for diagnostic rendering instead of producing empty Calculator inputs.
reader.SkipZeroTime = 0
reader.UpdatePipelineInformation()
reader.Adddimensionalunitstoarraynames = 0
reader.CellArrays = ["U", "p"] + (["T"] if compressible else [])
reader.MeshRegions = ["internalMesh", "airfoil"]
time = max(reader.TimestepValues)
reader.UpdatePipeline(time)

_DisableFirstRenderCameraReset()
view = CreateView("RenderView")
view.ViewSize = [1600, 950]
view.ViewTime = time
view.InteractionMode = "2D"
view.CameraPosition = [0.5, 0.0, 10.0]
view.CameraFocalPoint = [0.5, 0.0, 0.0]
view.CameraViewUp = [0.0, 1.0, 0.0]
view.CameraParallelProjection = 1
view.CameraParallelScale = 2.4
rendered_ranges = {}


def _read_json(path):
    """Read a case diagnostic manifest when one is present.

    The renderer also serves cases produced outside the report-study driver,
    so a missing or partially written manifest must not prevent field output.
    """

    if not path.is_file():
        return {}
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError):
        return {}
    return value if isinstance(value, dict) else {}


def _status_overlay(case_dir, render_time=None):
    """Return honest case-status text and its manifest source.

    Status is deliberately kept separate from ParaView's rendering success:
    a screenshot can be generated for a finite but unconverged OpenFOAM
    field, and the image must say so.  ``result.json`` has priority because it
    is written after the solver; ``results.json`` and ``study.json`` support
    the other native case drivers as fallbacks.
    """

    source = None
    metadata = {}
    for candidate in (
        case_dir / "result.json",
        case_dir / "results.json",
        case_dir / "study.json",
    ):
        data = _read_json(candidate)
        if data:
            metadata = data
            source = candidate.name
            break

    raw_status = str(
        metadata.get("status", metadata.get("outcome", ""))
    ).strip().lower()
    if raw_status == "converged":
        status = "CONVERGED (NUMERICAL DIAGNOSTIC ONLY)"
    elif raw_status in {"unconverged/provisional", "unconverged", "provisional"}:
        status = "UNCONVERGED / PROVISIONAL"
    elif raw_status == "no-finite-force-output":
        status = "NO FINITE FORCE OUTPUT"
    elif raw_status in {"failed", "failure"}:
        status = "FAILED / PROVISIONAL"
    elif raw_status in {"success", "ok", "completed"}:
        status = "COMPLETED (NUMERICAL DIAGNOSTIC ONLY)"
    elif raw_status == "prepared":
        status = "PREPARED; NO SOLUTION STATUS"
    elif raw_status:
        status = raw_status.upper()
    else:
        status = "STATUS UNKNOWN"

    history = metadata.get("force_history")
    if not isinstance(history, list):
        history = metadata.get("forces")

    finite = metadata.get("finite_output")
    if not isinstance(finite, bool) and isinstance(history, list):
        # NASA case manifests use ``forces`` and do not repeat the boolean
        # field.  Keep finite rows visible even when the solver later fails.
        finite = False
        for row in history:
            if not isinstance(row, dict):
                continue
            values = []
            for key in ("Cd", "Cl", "Cm", "cd", "cl", "cm"):
                if key in row:
                    try:
                        values.append(float(row[key]))
                    except (TypeError, ValueError):
                        pass
            if values and all(math.isfinite(value) for value in values):
                finite = True
                break
    if isinstance(finite, bool):
        finite_text = "yes" if finite else "no"
    else:
        finite_text = "not recorded"

    solver = str(metadata.get("solver", "")).strip()
    if not solver:
        solver = str(metadata.get("solver_name", "")).strip() or "not recorded"
    if solver == "not recorded":
        provenance = metadata.get("provenance")
        if isinstance(provenance, dict):
            config = provenance.get("config")
            if isinstance(config, dict):
                candidate = config.get("solver_name")
                if isinstance(candidate, str) and candidate.strip():
                    solver = candidate.strip()
    status_detail = metadata.get("status_detail")
    if solver == "not recorded" and isinstance(status_detail, str):
        candidate = status_detail.split(":", 1)[0].strip()
        if candidate:
            solver = candidate

    last_time = None
    if isinstance(history, list):
        for row in reversed(history):
            if not isinstance(row, dict):
                continue
            value = row.get("time", row.get("iteration"))
            try:
                value = float(value)
            except (TypeError, ValueError):
                continue
            if math.isfinite(value):
                last_time = value
                break
    if last_time is None:
        metrics = metadata.get("metrics")
        if isinstance(metrics, dict):
            try:
                candidate = float(metrics.get("time"))
            except (TypeError, ValueError):
                candidate = math.nan
            if math.isfinite(candidate):
                last_time = candidate

    details = [
        f"finite force outputs shown: {finite_text}",
        f"solver: {solver}",
    ]
    if render_time is not None:
        try:
            render_time = float(render_time)
        except (TypeError, ValueError):
            render_time = math.nan
        if math.isfinite(render_time):
            details.append(f"render time: {render_time:g}")
    if last_time is not None:
        details.append(f"last force iteration/time: {last_time:g}")
    render_time_note = metadata.get("render_time_note")
    if isinstance(render_time_note, str) and render_time_note.strip():
        details.append(f"render note: {render_time_note.strip()}")
    source_text = source or "no result/study manifest"
    return f"STATUS: {status}\n" + "\n".join(details), source_text


status_text, status_source = _status_overlay(case, time)
status_source_proxy = Text()
status_source_proxy.Text = status_text
status_display = Show(status_source_proxy, view)
status_display.WindowLocation = "Upper Left Corner"
status_display.Justification = "Left"
status_display.VerticalJustification = "Top"
status_display.FontSize = 18
status_display.Color = [1.0, 1.0, 1.0]
status_display.BackgroundColor = [0.0, 0.0, 0.0, 0.7]
status_display.Opacity = 1.0
status_display.Shadow = 1


def render_calculated(name, expression, label, file_name):
    calculated = Calculator(Input=reader)
    calculated.AttributeType = "Cell Data"
    calculated.ResultArrayName = name
    calculated.Function = expression
    calculated.UpdatePipeline(time)
    representation = Show(calculated, view)
    ColorBy(representation, ("CELLS", name))
    representation.RescaleTransferFunctionToDataRange(True, False)
    lut = GetColorTransferFunction(name)
    # Keep the data-derived range while selecting the user-requested Turbo
    # preset.  Applying a preset with rescale=False preserves the physical
    # range; the explicit second rescale also handles ParaView builds that
    # reset the range while applying a preset.
    # The ParaView 6.1 Python proxy does not expose vtkScalarsToColors
    # ``GetRange()`` as a zero-argument method.  The first and last abscissae
    # of RGBPoints are the active transfer-function range after the data
    # rescale above.
    rgb_points = [float(value) for value in lut.RGBPoints]
    if len(rgb_points) < 8:
        raise RuntimeError(f"transfer function for {name} has no usable range")
    data_range = (rgb_points[0], rgb_points[-4])
    try:
        lut.ApplyPreset("Turbo", False)
    except Exception:
        # Older ParaView builds may not expose the preset under this name.
        # Rendering remains useful, and the provenance records the failure.
        pass
    lut.RescaleTransferFunction(data_range[0], data_range[1])
    rendered_ranges[name] = data_range
    scalar_bar = GetScalarBar(lut, view)
    scalar_bar.Title = label
    scalar_bar.ComponentTitle = ""
    representation.SetScalarBarVisibility(view, True)
    Render(view)
    SaveScreenshot(str(output / file_name), view)
    representation.SetScalarBarVisibility(view, False)
    representation.Visibility = 0

render_calculated(
    "Mach",
    f"mag(U)/{sound_speed:.16e}",
    "Mach [-]",
    "mach-contour.png",
)
render_calculated(
    "pressurePa",
    (f"p-{pressure_reference_pa:.16e}" if compressible else f"p*{rho:.16e}"),
    "Gauge pressure [Pa]",
    "pressure-contour.png",
)

status_display.Visibility = 0
Delete(status_source_proxy)

(output / "render-provenance.txt").write_text(
    "\n".join(
        [
            f"case={case}",
            f"time={time}",
            f"density_kg_m3={rho:.16e}",
            f"static_temperature_k={temperature_k:.16e}",
            f"speed_of_sound_m_s={sound_speed:.16e}",
            "mach_definition=mag(U)/sqrt(1.4*287.05287*T)",
            (
                f"pressure_definition=p_absolute-p_reference; pressure_reference_pa={pressure_reference_pa:.16e}"
                if compressible
                else "pressure_definition=rho*p_kinematic_gauge"
            ),
            f"lut_preset=Turbo",
            "scalar_ranges=preserved_from_rendered_data_after_Turbo_preset",
            f"Mach_range={rendered_ranges.get('Mach', (math.nan, math.nan))[0]:.16e},{rendered_ranges.get('Mach', (math.nan, math.nan))[1]:.16e}",
            f"pressurePa_range={rendered_ranges.get('pressurePa', (math.nan, math.nan))[0]:.16e},{rendered_ranges.get('pressurePa', (math.nan, math.nan))[1]:.16e}",
            f"status_overlay={status_text.replace(chr(10), ' | ')}",
            f"status_source={status_source}",
            (
                "model=compressible steady RANS rhoSimpleFoam; Mach uses local static T"
                if compressible
                else "model=incompressible steady RANS kOmegaSST; Mach is a diagnostic only"
            ),
        ]
    )
    + "\n",
    encoding="utf-8",
)
