"""Export ALAS OpenFOAM contours through the installed ``paraFoam`` route.

``paraFoam`` is an OpenFOAM launcher, rather than a second solver.  With
``-vtk`` it selects ParaView's native OpenFOAM reader; ``-touch`` creates the
case marker without opening an interactive window.  This wrapper runs that
launcher first and then executes the reproducible headless renderer against
the marker it created::

    python tools/openfoam_parafoam_render.py CASE_DIR RHO_KG_M3 TEMPERATURE_K \
        --parafoam C:/.../OpenFOAM-v2606/bin/paraFoam \
        --bash C:/.../OpenFOAM-v2606/msys64/usr/bin/bash.exe \
        --pvpython C:/.../ParaView/bin/pvpython.exe

The renderer writes ``postProcessing/alas-field-figures`` below the case.
The wrapper records both commands and their exit status in
``parafoam-provenance.txt`` so an image cannot be mistaken for a different
reader or a compressible-flow solution.  The renderer's status overlay and
physical scalar ranges are copied into the same provenance record.
"""

from __future__ import annotations

import argparse
import math
import os
import shlex
import subprocess
import sys
from pathlib import Path


def msys_path(path: Path) -> str:
    """Convert an absolute Windows path to the spelling understood by MSYS2."""

    resolved = path.resolve()
    drive = resolved.drive
    if not drive:
        return resolved.as_posix()
    return f"/{drive[0].lower()}{resolved.as_posix()[2:]}"


def default_bash(project: Path) -> Path | None:
    """Find the MSYS2 shell shipped beside the native OpenFOAM tree."""

    if os.name != "nt":
        return None
    # .../msys64/home/ofuser/OpenFOAM/OpenFOAM-v2606
    candidate = project.parents[3] / "usr" / "bin" / "bash.exe"
    return candidate if candidate.is_file() else None


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("case_dir", type=Path)
    result.add_argument("rho_kg_m3", type=float)
    result.add_argument("temperature_k", type=float)
    result.add_argument(
        "--parafoam",
        type=Path,
        required=True,
        help="OpenFOAM bin/paraFoam launcher script",
    )
    result.add_argument(
        "--bash",
        type=Path,
        help="MSYS2 bash used to execute the paraFoam shell script",
    )
    result.add_argument(
        "--pvpython",
        type=Path,
        required=True,
        help="ParaView pvpython executable",
    )
    result.add_argument(
        "--renderer",
        type=Path,
        default=Path(__file__).with_name("openfoam_render_fields.py"),
        help="headless renderer invoked after paraFoam creates the marker",
    )
    result.add_argument(
        "--compressible",
        action="store_true",
        help="render local Mach from T and pressure as p-p_reference (rhoSimpleFoam/rhoCentralFoam)",
    )
    result.add_argument(
        "--pressure-reference-pa",
        type=float,
        default=0.0,
        help="absolute pressure reference for compressible gauge-pressure contours",
    )
    return result


def main() -> int:
    args = parser().parse_args()
    case = args.case_dir.resolve()
    parafoam = args.parafoam.resolve()
    pvpython = args.pvpython.resolve()
    renderer = args.renderer.resolve()
    bash = args.bash.resolve() if args.bash else default_bash(parafoam.parent.parent)

    if not case.is_dir():
        raise SystemExit(f"case directory does not exist: {case}")
    if not parafoam.is_file():
        raise SystemExit(f"paraFoam launcher does not exist: {parafoam}")
    if bash is None or not bash.is_file():
        raise SystemExit("an MSYS2 bash executable is required to run the paraFoam shell script")
    if not pvpython.is_file():
        raise SystemExit(f"pvpython executable does not exist: {pvpython}")
    if not renderer.is_file():
        raise SystemExit(f"renderer script does not exist: {renderer}")
    if not math.isfinite(args.rho_kg_m3) or args.rho_kg_m3 <= 0.0:
        raise SystemExit("density must be finite and greater than zero")
    if not math.isfinite(args.temperature_k) or args.temperature_k <= 0.0:
        raise SystemExit("temperature must be finite and greater than zero")

    parafoam_command = (
        "export PATH="
        + shlex.quote(msys_path(pvpython.parent))
        + ":$PATH; exec "
        + shlex.quote(msys_path(parafoam))
        + " -vtk -case "
        + shlex.quote(msys_path(case))
        + " -touch"
    )
    parafoam_result = subprocess.run(
        [str(bash), "-lc", parafoam_command],
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )
    marker = case / f"{case.name}.foam"
    if not marker.is_file():
        # The ALAS GUI uses this stable fallback marker when it opens a case
        # directly in ParaView.  Prefer the paraFoam-generated case-name
        # marker above whenever the launcher created it.
        marker = case / "case.foam"
    if parafoam_result.returncode != 0 or not marker.is_file():
        raise SystemExit(
            "paraFoam did not create the native OpenFOAM marker "
            f"(exit {parafoam_result.returncode}):\n"
            f"{parafoam_result.stderr.strip()}"
        )

    render_command = [
        str(pvpython),
        str(renderer),
        str(case),
        f"{args.rho_kg_m3:.16e}",
        f"{args.temperature_k:.16e}",
        str(marker),
    ]
    if args.compressible:
        render_command.append("--compressible")
        render_command.extend(["--pressure-reference-pa", f"{args.pressure_reference_pa:.16e}"])
    render_result = subprocess.run(
        render_command,
        check=False,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
    )

    output = case / "postProcessing" / "alas-field-figures"
    output.mkdir(parents=True, exist_ok=True)
    (output / "parafoam.stdout.log").write_text(
        parafoam_result.stdout, encoding="utf-8"
    )
    (output / "parafoam.stderr.log").write_text(
        parafoam_result.stderr, encoding="utf-8"
    )
    (output / "renderer.stdout.log").write_text(
        render_result.stdout, encoding="utf-8"
    )
    (output / "renderer.stderr.log").write_text(
        render_result.stderr, encoding="utf-8"
    )
    provenance_lines = [
                f"case={case}",
                f"marker={marker}",
                "reader_route=paraFoam -vtk -> ParaView native OpenFOAM reader",
                f"parafoam_launcher={parafoam}",
                f"parafoam_command=PATH={msys_path(pvpython.parent)}:$PATH {msys_path(parafoam)} -vtk -case {msys_path(case)} -touch",
                f"parafoam_exit_code={parafoam_result.returncode}",
                f"pvpython={pvpython}",
                f"renderer={renderer}",
                f"renderer_exit_code={render_result.returncode}",
                f"density_kg_m3={args.rho_kg_m3:.16e}",
                f"static_temperature_k={args.temperature_k:.16e}",
                "mach_definition=mag(U)/sqrt(1.4*287.05287*T)",
                (
                    f"pressure_definition=p_absolute-p_reference; pressure_reference_pa={args.pressure_reference_pa:.16e}"
                    if args.compressible
                    else "pressure_definition=rho*p_kinematic_gauge"
                ),
                "lut_preset=Turbo",
                "scalar_ranges=preserved_from_rendered_data_after_Turbo_preset",
                (
                    "model=compressible steady RANS rhoSimpleFoam; Mach uses local static T"
                    if args.compressible
                    else "model=incompressible steady RANS kOmegaSST; Mach is a diagnostic only"
                ),
    ]
    renderer_provenance = output / "render-provenance.txt"
    if renderer_provenance.is_file():
        provenance_lines.append(f"renderer_provenance={renderer_provenance}")
        provenance_lines.extend(
            f"renderer_{line}"
            for line in renderer_provenance.read_text(encoding="utf-8").splitlines()
            if line.startswith(
                (
                    "Mach_range=",
                    "pressurePa_range=",
                    "lut_preset=",
                    "scalar_ranges=",
                    "status_overlay=",
                    "status_source=",
                )
            )
        )
    (output / "parafoam-provenance.txt").write_text(
        "\n".join(provenance_lines) + "\n", encoding="utf-8"
    )
    if render_result.returncode != 0:
        print(render_result.stdout, file=sys.stdout, end="")
        print(render_result.stderr, file=sys.stderr, end="")
        return render_result.returncode or 1
    print(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
