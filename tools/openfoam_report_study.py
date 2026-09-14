"""Run the private report's KC-135 Winglet study with native OpenFOAM.

The report used NeuralFoil/Karman--Tsien rather than a CFD solver.  This
driver creates a separate dimensional, compressible ``rhoSimpleFoam`` case
for the report's three operating points and alpha sweep.  It deliberately
keeps the report's contradictory M=0, U~=50 m/s reference as a named case and
records the resulting physical Mach number instead of silently changing it.

Typical usage (from the ALAS repository):

    python tools/openfoam_report_study.py --alphas 0 --end-time 50 --workers 2
    python tools/openfoam_report_study.py --workers 2 --end-time 300

Every case is independent.  The default worker count is two so another
native OpenFOAM study can run on the same workstation.  Results and all
solver logs are written below ``.agent/openfoam-report-study-20260914``.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import math
import os
import re
import subprocess
import sys
import time
from pathlib import Path
from typing import Iterable, Sequence


REPO = Path(__file__).resolve().parents[1]
REPORT_PATH = Path(r"C:\Users\Marcos\OneDrive\Descargas\informe_tecnico_P1.md")
COORDINATE_PATH = REPO / "crates" / "alas-geom" / "data" / "selig.txt"
OUTPUT_ROOT = REPO / ".agent" / "openfoam-report-study-20260914"
OPENFOAM_ROOT = Path(
    r"C:\Proyectos\OpenFOAM-v2606\msys64\home\ofuser\OpenFOAM\OpenFOAM-v2606"
)
OPENFOAM_BIN = OPENFOAM_ROOT / "platforms" / "win64MingwDPInt32Opt" / "bin"
GMSH = Path(r"C:\Proyectos\.agent\tools\gmsh-4.15.2-Windows64\gmsh.exe")

GAMMA = 1.4
R_AIR = 287.05287
CP_AIR = 1005.0
PR_AIR = 0.71
MOL_WEIGHT = 28.9

# Values transcribed from informe_tecnico_P1.md lines 36--61.  The report
# gives rho and nu to four significant figures; using their product for mu
# preserves its stated Reynolds numbers and is recorded as an assumption.
ALTITUDE_M = 7000.0
T_INF_K = 242.65
P_INF_PA = 41060.35
RHO_INF_KG_M3 = 0.5895
NU_INF_M2_S = 2.65e-5
MU_INF_PA_S = RHO_INF_KG_M3 * NU_INF_M2_S
CHORD_M = 6.0
SPAN_M = 0.01 * CHORD_M
MACH_POINTS = (0.0, 0.6, 0.85)
ALPHA_POINTS_DEG = (-4.0, -2.0, 0.0, 2.0, 4.0, 6.0, 8.0, 10.0, 12.0)

# The report supplies no turbulence/transition model inputs.  These are
# explicit engineering assumptions for a reproducible SST case.
TURBULENCE_INTENSITY = 0.01
TURBULENCE_LENGTH_M = 0.07 * CHORD_M
WALL_MODEL = "noSlip, adiabatic zeroGradient T; kOmegaSST wall functions"
# The report gives no wall-resolution target.  Scale the template's
# 2e-5 m-per-metre chord first-cell setting to c=6 m so the extruded mesh
# remains a valid quasi-2-D mesh under checkMesh's aspect-ratio diagnostic.
FIRST_LAYER_M = 2.0e-5 * CHORD_M


def finite(value: float) -> bool:
    return math.isfinite(value)


def fmt(value: float) -> str:
    return f"{value:.16g}"


def shell_env() -> dict[str, str]:
    env = os.environ.copy()
    path = str(OPENFOAM_BIN)
    if env.get("PATH"):
        path += os.pathsep + env["PATH"]
    env.update(
        {
            "PATH": path,
            "WM_PROJECT_DIR": str(OPENFOAM_ROOT),
            "WM_PROJECT_VERSION": "v2606",
            "FOAM_APPBIN": str(OPENFOAM_BIN),
            "FOAM_USER_APPBIN": str(OPENFOAM_BIN),
        }
    )
    return env


def find_executable(name: str) -> Path:
    if name == "gmsh":
        if not GMSH.is_file():
            raise FileNotFoundError(f"Gmsh executable is missing: {GMSH}")
        return GMSH
    candidate = OPENFOAM_BIN / (name + (".exe" if os.name == "nt" else ""))
    if not candidate.is_file():
        raise FileNotFoundError(f"OpenFOAM executable is missing: {candidate}")
    return candidate


def load_kc135_coordinates() -> list[tuple[float, float]]:
    lines = COORDINATE_PATH.read_text(encoding="utf-8").splitlines()
    start = next(
        index for index, line in enumerate(lines) if line.strip().lower() == "@kc135winglet"
    )
    coordinates: list[tuple[float, float]] = []
    for line in lines[start + 2 :]:
        stripped = line.strip()
        if not stripped or stripped.startswith("@"):
            break
        fields = stripped.split()
        if len(fields) < 2:
            continue
        try:
            coordinates.append((float(fields[0]), float(fields[1])))
        except ValueError:
            break
    if len(coordinates) < 10:
        raise RuntimeError("kc135winglet coordinate block was not found in selig.txt")
    return coordinates


def geo_text(coordinates: Sequence[tuple[float, float]]) -> str:
    """Return a dimensional 2-D airfoil extrusion compatible with gmshToFoam."""

    outer_size = 0.25 * CHORD_M
    min_size = 2.0e-6 * CHORD_M
    surface_size = 0.02 * CHORD_M
    first_layer = FIRST_LAYER_M  # m, an explicit resolution setting, not a y+ claim
    boundary_thickness = 9.439621664406896e-3 * CHORD_M
    lines: list[str] = [
        "// ALAS report-study native Gmsh mesh",
        "// source = informe_tecnico_P1.md lines 5-25; exact kc135winglet snapshot",
        "// coordinates = crates/alas-geom/data/selig.txt @kc135winglet",
        'SetFactory("Built-in");',
        "Mesh.MshFileVersion = 2.2;",
        "Mesh.Algorithm = 5;",
        "Mesh.Algorithm3D = 1;",
        "Mesh.ElementOrder = 1;",
        "Mesh.Optimize = 1;",
        "Mesh.OptimizeNetgen = 1;",
        "Mesh.Smoothing = 5;",
        "Mesh.CharacteristicLengthExtendFromBoundary = 0;",
        "Mesh.CharacteristicLengthFromPoints = 1;",
        "Mesh.CharacteristicLengthFromCurvature = 0;",
        f"Mesh.CharacteristicLengthMax = {fmt(outer_size)};",
        f"Mesh.CharacteristicLengthMin = {fmt(min_size)};",
        "",
        "// Outer rectangle and exact airfoil polygon points",
        f"Point(1) = {{{fmt(-10.0 * CHORD_M)}, {fmt(-10.0 * CHORD_M)}, 0, {fmt(outer_size)}}};",
        f"Point(2) = {{{fmt(20.0 * CHORD_M)}, {fmt(-10.0 * CHORD_M)}, 0, {fmt(outer_size)}}};",
        f"Point(3) = {{{fmt(20.0 * CHORD_M)}, {fmt(10.0 * CHORD_M)}, 0, {fmt(outer_size)}}};",
        f"Point(4) = {{{fmt(-10.0 * CHORD_M)}, {fmt(10.0 * CHORD_M)}, 0, {fmt(outer_size)}}};",
    ]
    for index, (x_over_c, y_over_c) in enumerate(coordinates, start=5):
        lines.append(
            f"Point({index}) = {{{fmt(x_over_c * CHORD_M)}, {fmt(y_over_c * CHORD_M)}, 0, {fmt(surface_size)}}};"
        )
    lines += [
        "",
        "Line(1) = {1, 2};",
        "Line(2) = {2, 3};",
        "Line(3) = {3, 4};",
        "Line(4) = {4, 1};",
    ]
    airfoil_line_ids = list(range(5, 5 + len(coordinates)))
    for line_id, point_id in zip(airfoil_line_ids, range(5, 5 + len(coordinates))):
        next_point = 5 + ((point_id - 5 + 1) % len(coordinates))
        lines.append(f"Line({line_id}) = {{{point_id}, {next_point}}};")
    lines += [
        "Curve Loop(1) = {1, 2, 3, 4};",
        "Curve Loop(2) = {"
        + ", ".join(str(-line_id) for line_id in airfoil_line_ids)
        + "};",
        "Plane Surface(1) = {1, 2};",
        "",
        "Field[2] = Distance;",
        "Field[2].CurvesList = {"
        + ":".join((str(airfoil_line_ids[0]), str(airfoil_line_ids[-1])))
        + "};",
        "Field[2].Sampling = 200;",
        "Field[3] = Threshold;",
        "Field[3].InField = 2;",
        f"Field[3].SizeMin = {fmt(surface_size)};",
        f"Field[3].SizeMax = {fmt(outer_size)};",
        f"Field[3].DistMin = {fmt(boundary_thickness)};",
        f"Field[3].DistMax = {fmt(0.75 * CHORD_M)};",
        "Field[4] = Box;",
        f"Field[4].VIn = {fmt(0.0625 * CHORD_M)};",
        f"Field[4].VOut = {fmt(outer_size)};",
        "Field[4].XMin = 0;",
        f"Field[4].XMax = {fmt(20.0 * CHORD_M)};",
        f"Field[4].YMin = {fmt(-2.0 * CHORD_M)};",
        f"Field[4].YMax = {fmt(2.0 * CHORD_M)};",
        "Field[4].ZMin = 0;",
        f"Field[4].ZMax = {fmt(SPAN_M)};",
        "Field[5] = Min;",
        "Field[5].FieldsList = {3, 4};",
        "Background Field = 5;",
        "",
        "// SI boundary-layer field: Size is first-cell thickness, twice wall-centre distance.",
        "Field[1] = BoundaryLayer;",
        "Field[1].CurvesList = {"
        + ":".join((str(airfoil_line_ids[0]), str(airfoil_line_ids[-1])))
        + "};",
        f"Field[1].Size = {fmt(first_layer)};",
        f"Field[1].Thickness = {fmt(boundary_thickness)};",
        "Field[1].Ratio = 1.2;",
        "Field[1].NbLayers = 25;",
        "Field[1].Quads = 1;",
        "BoundaryLayer Field = 1;",
        "",
        f"out[] = Extrude {{0, 0, {fmt(SPAN_M)}}} {{ Surface{{1}}; Layers{{1}}; Recombine; }};",
        "",
        'Physical Surface("frontAndBack") = {1, out[0]};',
        'Physical Surface("farField") = {out[2], out[4]};',
        'Physical Surface("outlet") = {out[3]};',
        'Physical Surface("inlet") = {out[5]};',
        'Physical Surface("airfoil") = {'
        + ", ".join(f"out[{index}]" for index in range(6, 6 + len(coordinates)))
        + "};",
        'Physical Volume("fluid") = {out[1]};',
        "Mesh.SaveAll = 0;",
        "",
    ]
    return "\n".join(lines)


def alpha_tag(alpha_deg: float) -> str:
    if alpha_deg < 0:
        return f"m{abs(alpha_deg):g}".replace(".", "p")
    return f"p{alpha_deg:g}".replace(".", "p")


def mach_tag(requested_mach: float) -> str:
    return "M0-reference" if requested_mach == 0.0 else f"M{requested_mach:g}".replace(".", "p")


def case_name(requested_mach: float, alpha_deg: float) -> str:
    return f"kc135winglet-{mach_tag(requested_mach)}-alpha-{alpha_tag(alpha_deg)}"


def operating_point(requested_mach: float) -> dict[str, float | str]:
    speed_of_sound = math.sqrt(GAMMA * R_AIR * T_INF_K)
    if requested_mach == 0.0:
        # The report explicitly assigns U~=50 m/s to its M=0 reference.
        velocity = 50.0
        label = "M0-reference"
    else:
        velocity = requested_mach * speed_of_sound
        label = f"M{requested_mach:g}"
    actual_mach = velocity / speed_of_sound
    reynolds = RHO_INF_KG_M3 * velocity * CHORD_M / MU_INF_PA_S
    k_inf = 1.5 * (TURBULENCE_INTENSITY * velocity) ** 2
    omega_inf = math.sqrt(k_inf) / (0.5477225575051661 * TURBULENCE_LENGTH_M)
    return {
        "requested_mach": requested_mach,
        "mach_label": label,
        "actual_mach": actual_mach,
        "velocity_m_s": velocity,
        "speed_of_sound_m_s": speed_of_sound,
        "reynolds_chord": reynolds,
        "k_infinity_m2_s2": k_inf,
        "omega_infinity_s-1": omega_inf,
    }


def header(field_class: str, object_name: str) -> str:
    return f"""/*--------------------------------*- C++ -*----------------------------------*\\
| OpenFOAM v2606 report study                                                   |
\\*---------------------------------------------------------------------------*/
FoamFile
{{
    version     2.0;
    format      ascii;
    class       {field_class};
    object      {object_name};
}}
// ************************************************************************* //
"""


def vector_field(name: str, dimensions: str, internal: str, ux: float, uy: float) -> str:
    return header("volVectorField", name) + f"""
dimensions      {dimensions};

internalField   uniform ({fmt(ux)} {fmt(uy)} 0);

boundaryField
{{
    frontAndBack {{ type empty; }}
    farField
    {{
        type            freestreamVelocity;
        freestreamValue uniform ({fmt(ux)} {fmt(uy)} 0);
        value           uniform ({fmt(ux)} {fmt(uy)} 0);
    }}
    outlet
    {{
        type            pressureInletOutletVelocity;
        value           uniform ({fmt(ux)} {fmt(uy)} 0);
    }}
    inlet
    {{
        type            freestreamVelocity;
        freestreamValue uniform ({fmt(ux)} {fmt(uy)} 0);
        value           uniform ({fmt(ux)} {fmt(uy)} 0);
    }}
    airfoil {{ type noSlip; }}
}}
"""


def scalar_field(
    name: str,
    dimensions: str,
    internal: float,
    far_patch: str,
    outlet_patch: str,
    inlet_patch: str,
    wall_patch: str,
) -> str:
    return header("volScalarField", name) + f"""
dimensions      {dimensions};

internalField   uniform {fmt(internal)};

boundaryField
{{
    frontAndBack {{ type empty; }}
    farField
    {{
        {far_patch}
    }}
    outlet
    {{
        {outlet_patch}
    }}
    inlet
    {{
        {inlet_patch}
    }}
    airfoil
    {{
        {wall_patch}
    }}
}}
"""


def thermophysical_properties() -> str:
    return header("dictionary", "thermophysicalProperties") + f"""
thermoType
{{
    type            hePsiThermo;
    mixture         pureMixture;
    transport       const;
    thermo          hConst;
    equationOfState perfectGas;
    specie          specie;
    energy          sensibleInternalEnergy;
}}

mixture
{{
    specie
    {{
        molWeight   {fmt(MOL_WEIGHT)};
    }}
    thermodynamics
    {{
        Cp          {fmt(CP_AIR)};
        Hf          0;
    }}
    transport
    {{
        mu          {fmt(MU_INF_PA_S)};
        Pr          {fmt(PR_AIR)};
    }}
}}
"""


def turbulence_properties() -> str:
    return header("dictionary", "turbulenceProperties") + """
simulationType          RAS;

RAS
{
    RASModel            kOmegaSST;
    turbulence          on;
    printCoeffs         on;
}
"""


def fv_schemes() -> str:
    return header("dictionary", "fvSchemes") + """
ddtSchemes
{
    default         steadyState;
}

gradSchemes
{
    default         Gauss linear;
    limited         cellLimited Gauss linear 1;
    grad(U)         $limited;
    grad(k)         $limited;
    grad(omega)     $limited;
}

divSchemes
{
    default         none;
    div(phi,U)      bounded Gauss linearUpwind limited;
    energy          bounded Gauss linearUpwind limited;
    div(phi,e)      $energy;
    div(phi,K)      $energy;
    div(phi,Ekp)    $energy;
    turbulence      bounded Gauss upwind;
    div(phi,k)      $turbulence;
    div(phi,omega)  $turbulence;
    div(phid,p)     Gauss upwind;
    div((phi|interpolate(rho)),p)  bounded Gauss upwind;
    div(((rho*nuEff)*dev2(T(grad(U)))))    Gauss linear;
}

laplacianSchemes
{
    default         Gauss linear corrected;
}
interpolationSchemes { default linear; }
snGradSchemes { default corrected; }
wallDist { method meshWave; }
"""


def fv_solution() -> str:
    return header("dictionary", "fvSolution") + """
solvers
{
    Phi
    {
        solver          PCG;
        preconditioner  DIC;
        tolerance       1e-8;
        relTol          0;
    }
    p
    {
        solver          GAMG;
        smoother        GaussSeidel;
        tolerance       1e-6;
        relTol          0.01;
    }
    "(U|k|omega|e)"
    {
        solver          PBiCGStab;
        preconditioner  DILU;
        tolerance       1e-6;
        relTol          0.1;
    }
}

SIMPLE
{
    residualControl
    {
        p               1e-4;
        U               1e-4;
        "(k|omega|e)"   1e-4;
    }
    nNonOrthogonalCorrectors 0;
    consistent          yes;
    pMinFactor      0.1;
    pMaxFactor      2;
}

potentialFlow
{
    nNonOrthogonalCorrectors 20;
}

relaxationFactors
{
    fields
    {
        p               0.7;
        rho             0.01;
    }
    equations
    {
        U               0.3;
        e               0.7;
        "(k|omega)"     0.7;
    }
}
"""


def control_dict(
    velocity: float, alpha_deg: float, end_time: int, write_interval: int
) -> str:
    alpha = math.radians(alpha_deg)
    drag_x, drag_y = math.cos(alpha), math.sin(alpha)
    lift_x, lift_y = -math.sin(alpha), math.cos(alpha)
    return header("dictionary", "controlDict") + f"""
application     rhoSimpleFoam;
startFrom       startTime;
startTime       0;
stopAt          endTime;
endTime         {int(end_time)};
deltaT          1;
writeControl    timeStep;
writeInterval   {int(write_interval)};
purgeWrite      0;
writeFormat     ascii;
writePrecision  10;
writeCompression off;
timeFormat      general;
timePrecision   8;
runTimeModifiable true;

functions
{{
    #includeFunc MachNo
    #includeFunc solverInfo

    forces
    {{
        type            forceCoeffs;
        libs            (forces);
        writeControl    timeStep;
        writeInterval   {int(write_interval)};
        patches         (airfoil);
        rhoInf          {fmt(RHO_INF_KG_M3)};
        CofR            ({fmt(0.25 * CHORD_M)} 0 {fmt(0.5 * SPAN_M)});
        liftDir         ({fmt(lift_x)} {fmt(lift_y)} 0);
        dragDir         ({fmt(drag_x)} {fmt(drag_y)} 0);
        pitchAxis       (0 0 1);
        magUInf         {fmt(velocity)};
        lRef            {fmt(CHORD_M)};
        Aref            {fmt(CHORD_M * SPAN_M)};
    }}

    surfacePressure
    {{
        type            surfaces;
        libs            (sampling);
        executeControl  timeStep;
        executeInterval {int(write_interval)};
        writeControl    timeStep;
        writeInterval   {int(write_interval)};
        surfaceFormat   raw;
        fields          (p T U);
        surfaces
        {{
            airfoil
            {{
                type            patch;
                patches         (airfoil);
                interpolate      false;
            }}
        }}
    }}
}}
"""


def write_case(case: Path, requested_mach: float, alpha_deg: float, end_time: int, write_interval: int) -> dict:
    point = operating_point(requested_mach)
    velocity = float(point["velocity_m_s"])
    alpha = math.radians(alpha_deg)
    ux, uy = velocity * math.cos(alpha), velocity * math.sin(alpha)
    k_inf = float(point["k_infinity_m2_s2"])
    omega_inf = float(point["omega_infinity_s-1"])
    for directory in (case / "0", case / "constant" / "triSurface", case / "system", case / "logs"):
        directory.mkdir(parents=True, exist_ok=True)
    (case / "system" / "airfoil.geo").write_text(geo_text(load_kc135_coordinates()), encoding="utf-8")
    (case / "constant" / "thermophysicalProperties").write_text(thermophysical_properties(), encoding="utf-8")
    (case / "constant" / "turbulenceProperties").write_text(turbulence_properties(), encoding="utf-8")
    (case / "system" / "fvSchemes").write_text(fv_schemes(), encoding="utf-8")
    (case / "system" / "fvSolution").write_text(fv_solution(), encoding="utf-8")
    (case / "system" / "controlDict").write_text(
        control_dict(velocity, alpha_deg, end_time, write_interval), encoding="utf-8"
    )
    (case / "0" / "U").write_text(vector_field("U", "[0 1 -1 0 0 0 0]", "", ux, uy), encoding="utf-8")
    (case / "0" / "p").write_text(
        scalar_field(
            "p", "[1 -1 -2 0 0 0 0]", P_INF_PA,
            f"type freestreamPressure; freestreamValue uniform {fmt(P_INF_PA)};",
            f"type freestreamPressure; freestreamValue uniform {fmt(P_INF_PA)};",
            f"type freestreamPressure; freestreamValue uniform {fmt(P_INF_PA)};",
            "type zeroGradient;",
        ), encoding="utf-8"
    )
    (case / "0" / "T").write_text(
        scalar_field(
            "T", "[0 0 0 1 0 0 0]", T_INF_K,
            f"type inletOutlet; inletValue uniform {fmt(T_INF_K)}; value uniform {fmt(T_INF_K)};",
            f"type inletOutlet; inletValue uniform {fmt(T_INF_K)}; value uniform {fmt(T_INF_K)};",
            f"type inletOutlet; inletValue uniform {fmt(T_INF_K)}; value uniform {fmt(T_INF_K)};",
            "type zeroGradient;",
        ), encoding="utf-8"
    )
    (case / "0" / "k").write_text(
        scalar_field(
            "k", "[0 2 -2 0 0 0 0]", k_inf,
            f"type inletOutlet; inletValue uniform {fmt(k_inf)}; value uniform {fmt(k_inf)};",
            f"type inletOutlet; inletValue uniform {fmt(k_inf)}; value uniform {fmt(k_inf)};",
            f"type inletOutlet; inletValue uniform {fmt(k_inf)}; value uniform {fmt(k_inf)};",
            f"type kqRWallFunction; value uniform {fmt(k_inf)};",
        ), encoding="utf-8"
    )
    (case / "0" / "omega").write_text(
        scalar_field(
            "omega", "[0 0 -1 0 0 0 0]", omega_inf,
            f"type inletOutlet; inletValue uniform {fmt(omega_inf)}; value uniform {fmt(omega_inf)};",
            f"type inletOutlet; inletValue uniform {fmt(omega_inf)}; value uniform {fmt(omega_inf)};",
            f"type inletOutlet; inletValue uniform {fmt(omega_inf)}; value uniform {fmt(omega_inf)};",
            f"type omegaWallFunction; value uniform {fmt(omega_inf)};",
        ), encoding="utf-8"
    )
    (case / "0" / "nut").write_text(
        scalar_field(
            "nut", "[0 2 -1 0 0 0 0]", 0.0,
            "type calculated; value uniform 0;", "type calculated; value uniform 0;",
            "type calculated; value uniform 0;", "type nutkWallFunction; value uniform 0;",
        ), encoding="utf-8"
    )
    (case / "0" / "alphat").write_text(
        scalar_field(
            "alphat", "[1 -1 -1 0 0 0 0]", 0.0,
            "type calculated; value uniform 0;", "type calculated; value uniform 0;",
            "type calculated; value uniform 0;", "type compressible::alphatWallFunction; value uniform 0;",
        ), encoding="utf-8"
    )
    metadata = {
        "case": case.name,
        "airfoil": "kc135winglet",
        "coordinate_source": str(COORDINATE_PATH),
        "coordinate_hash": "fnv1a64-9d705bfafc0bb4e",
        "report_source": str(REPORT_PATH),
        "report_line_refs": {
            "objective_and_points": "5-13",
            "coordinates_and_geometry": "17-25",
            "isa_state": "36-51",
            "chord_velocity_reynolds": "55-61",
            "method_limitations": "82-85,113-127,263-285",
        },
        "altitude_m": ALTITUDE_M,
        "temperature_K": T_INF_K,
        "pressure_Pa": P_INF_PA,
        "density_kg_m3": RHO_INF_KG_M3,
        "kinematic_viscosity_m2_s": NU_INF_M2_S,
        "dynamic_viscosity_Pa_s": MU_INF_PA_S,
        "chord_m": CHORD_M,
        "span_m": SPAN_M,
        "alpha_deg": alpha_deg,
        "alpha_frame": "positive nose-up; freestream U=(U cos(alpha), U sin(alpha), 0), x/c chord frame",
        "force_frame": "drag along freestream, lift +90 deg; pitch axis +z; CofR quarter chord",
        "solver": "rhoSimpleFoam",
        "flow_model": "compressible steady RANS, perfectGas, hePsiThermo, hConst, kOmegaSST",
        "operating_point": point,
        "turbulence_assumptions": {
            "intensity_fraction": TURBULENCE_INTENSITY,
            "length_scale_m": TURBULENCE_LENGTH_M,
            "transition_model": "none; fully turbulent SST assumption",
            "wall_and_thermal_bc": WALL_MODEL,
        },
        "mesh_settings": {
            "domain_x_over_c": [-10.0, 20.0],
            "domain_y_over_c": [-10.0, 10.0],
            "extrusion_span_m": SPAN_M,
            "first_layer_m": FIRST_LAYER_M,
            "boundary_layer_thickness_m": 9.439621664406896e-3 * CHORD_M,
            "boundary_layer_layers": 25,
            "growth_ratio": 1.2,
            "mesh_note": "first-cell setting is dimensional; y+ is measured after solution",
        },
        "requested_end_time_iterations": end_time,
        "write_interval_iterations": write_interval,
        "status": "prepared",
    }
    (case / "study.json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    return metadata


def run_command(case: Path, name: str, args: Sequence[str]) -> tuple[int, str, str]:
    executable = find_executable(name)
    command = [str(executable), *args]
    started = time.time()
    result = subprocess.run(
        command,
        cwd=case,
        env=shell_env(),
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=False,
    )
    elapsed = time.time() - started
    log = case / "logs" / f"{name}.log"
    log.write_text(
        "command=" + " ".join(command) + f"\nexit_code={result.returncode}\nelapsed_s={elapsed:.3f}\n\n"
        + result.stdout + ("\n--- STDERR ---\n" + result.stderr if result.stderr else ""),
        encoding="utf-8",
    )
    return result.returncode, result.stdout, result.stderr


def set_empty_front_and_back(case: Path) -> None:
    """gmshToFoam imports physical surfaces as generic patches.

    The extrusion represents a 2-D solution, so both z-normal faces must be
    converted to OpenFOAM's ``empty`` constraint after import.  Keeping this
    explicit in the driver avoids relying on a GUI or on a project-specific
    post-import mutation.
    """

    boundary = case / "constant" / "polyMesh" / "boundary"
    text = boundary.read_text(encoding="utf-8")
    pattern = r"(frontAndBack\s*\{.*?\btype\s+)patch(;)"
    updated, count = re.subn(pattern, r"\1empty\2", text, count=1, flags=re.S)
    if count != 1:
        raise RuntimeError("gmshToFoam boundary did not contain frontAndBack patch")
    wall_pattern = r"(airfoil\s*\{.*?\btype\s+)patch(;)"
    updated, wall_count = re.subn(wall_pattern, r"\1wall\2", updated, count=1, flags=re.S)
    if wall_count != 1:
        raise RuntimeError("gmshToFoam boundary did not contain airfoil patch")
    boundary.write_text(updated, encoding="utf-8")


def parse_mesh_quality(text: str) -> dict[str, float | int | bool | None]:
    def number(pattern: str) -> float | None:
        match = re.search(pattern, text, flags=re.IGNORECASE)
        return float(match.group(1).rstrip(".")) if match else None

    cells_match = re.search(r"\bcells:\s*(\d+)", text, flags=re.IGNORECASE)
    return {
        "cells": int(cells_match.group(1)) if cells_match else None,
        "max_non_orthogonality_deg": number(r"Mesh non-orthogonality Max:\s*([0-9.eE+-]+)"),
        "average_non_orthogonality_deg": number(r"average:\s*([0-9.eE+-]+)"),
        "max_skewness": number(r"Max skewness\s*=\s*([0-9.eE+-]+)"),
        "min_volume_m3": number(r"Min volume\s*=\s*([0-9.eE+-]+)"),
        "mesh_ok": bool(re.search(r"\bMesh OK\.", text)),
        "failed_checks_reported": bool(re.search(r"failed\s+checks|failed\s*:\s*[1-9]", text, re.I)),
    }


def parse_residual_history(text: str) -> list[dict[str, float | int | str]]:
    history: list[dict[str, float | int | str]] = []
    iteration = 0
    for line in text.splitlines():
        if re.match(r"^Time\s*=", line.strip()):
            iteration += 1
        match = re.search(
            r"Solving for\s+([^,]+),\s*Initial residual\s*=\s*([0-9.eE+-]+)", line
        )
        if match:
            try:
                value = float(match.group(2))
            except ValueError:
                continue
            if finite(value):
                history.append({"iteration": iteration, "field": match.group(1).strip(), "initial_residual": value})
    return history


def parse_force_history(case: Path) -> list[dict[str, float]]:
    # The function object is named ``forces`` in controlDict; OpenFOAM writes
    # its forceCoeffs data below postProcessing/forces/.
    files = list((case / "postProcessing" / "forces").glob("*/coefficient.dat"))
    if not files:
        return []
    selected = max(files, key=lambda path: path.stat().st_mtime)
    rows: list[dict[str, float]] = []
    for line in selected.read_text(encoding="utf-8", errors="replace").splitlines():
        if line.lstrip().startswith("#"):
            continue
        values: list[float] = []
        for token in line.split():
            try:
                values.append(float(token))
            except ValueError:
                values = []
                break
        # OpenFOAM v2606 forceCoeffs order: time Cd Cd(f) Cd(r) Cl Cl(f) Cl(r) CmPitch ...
        if len(values) >= 8 and all(finite(value) for value in values[:8]):
            rows.append({"time": values[0], "Cd": values[1], "Cl": values[4], "Cm": values[7]})
    return rows


def write_result(case: Path, solver_code: int, solver_stdout: str, check_text: str, mesh_code: int) -> dict:
    residuals = parse_residual_history(solver_stdout)
    forces = parse_force_history(case)
    latest_force = forces[-1] if forces else {}
    residual_values = [float(row["initial_residual"]) for row in residuals]
    finite_output = bool(forces) and all(
        finite(float(value)) for row in forces for value in row.values()
    )
    converged = (
        solver_code == 0
        and finite_output
        and bool(residual_values)
        and max(residual_values[-20:]) <= 1.0e-4
    )
    status = "converged" if converged else ("unconverged/provisional" if finite_output else "no-finite-force-output")
    result = {
        "case": case.name,
        "solver": "rhoSimpleFoam",
        "solver_exit_code": solver_code,
        "mesh_exit_code": mesh_code,
        "mesh_quality": parse_mesh_quality(check_text),
        "force_history": forces,
        "residual_history": residuals,
        "metrics": latest_force,
        "finite_output": finite_output,
        "converged_by_residual_diagnostic": converged,
        "status": status,
        "validation": "numerical output only; no physical validation claim",
        "output_files": {
            "solver_log": "logs/rhoSimpleFoam.log",
            "mesh_log": "logs/checkMesh.log",
            "force_coefficients": "postProcessing/forces/*/coefficient.dat",
            "surface_pressure": "postProcessing/surfacePressure/*/airfoil.raw",
        },
    }
    (case / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    metadata_path = case / "study.json"
    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    metadata["status"] = status
    metadata["mesh_quality"] = result["mesh_quality"]
    metadata["metrics"] = latest_force
    metadata["finite_output"] = finite_output
    metadata_path.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    return result


def run_case(
    requested_mach: float,
    alpha_deg: float,
    output_root: Path,
    end_time: int,
    write_interval: int,
    force_rebuild: bool,
    prepare_only: bool,
) -> dict:
    case = output_root / case_name(requested_mach, alpha_deg)
    if force_rebuild and case.exists():
        # Only remove this script's own case directory.  No shared production
        # case is touched by this driver.
        import shutil

        shutil.rmtree(case)
    metadata = write_case(case, requested_mach, alpha_deg, end_time, write_interval)
    if prepare_only:
        result = {"case": case.name, "status": "prepared", "study": metadata}
        (case / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        return result
    gmsh_code, gmsh_out, _ = run_command(
        case,
        "gmsh",
        ["-3", "-format", "msh2", "-o", "constant/triSurface/airfoil.msh", "system/airfoil.geo"],
    )
    if gmsh_code != 0:
        result = {"case": case.name, "status": "mesh-generation-failed", "gmsh_exit_code": gmsh_code}
        (case / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        return result
    gmsh_to_foam_code, gmsh_to_foam_out, _ = run_command(
        case, "gmshToFoam", ["-case", str(case), "constant/triSurface/airfoil.msh"]
    )
    if gmsh_to_foam_code != 0:
        result = {"case": case.name, "status": "gmshToFoam-failed", "gmshToFoam_exit_code": gmsh_to_foam_code}
        (case / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        return result
    set_empty_front_and_back(case)
    mesh_code, mesh_out, mesh_err = run_command(case, "checkMesh", ["-case", str(case), "-writeAllFields"])
    mesh_text = mesh_out + ("\n" + mesh_err if mesh_err else "")
    if mesh_code != 0 or not parse_mesh_quality(mesh_text)["mesh_ok"]:
        result = {
            "case": case.name,
            "status": "mesh-quality-failed",
            "mesh_exit_code": mesh_code,
            "mesh_quality": parse_mesh_quality(mesh_text),
        }
        (case / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
        return result
    potential_code, potential_out, potential_err = run_command(
        case, "potentialFoam", ["-case", str(case), "-writePhi"]
    )
    potential_text = potential_out + ("\n" + potential_err if potential_err else "")
    (case / "logs" / "initialization.txt").write_text(
        f"potentialFoam_exit_code={potential_code}\n{potential_text}", encoding="utf-8"
    )
    # Windows uses a case-insensitive filesystem, so potentialFoam's ``Phi``
    # can alias rhoSimpleFoam's ``phi`` lookup.  The compressible solver
    # reconstructs its own flux; remove only the initializer's auxiliary
    # field before launching it.
    potential_field = case / "0" / "Phi"
    if potential_field.exists():
        potential_field.unlink()
    solver_code, solver_out, solver_err = run_command(case, "rhoSimpleFoam", ["-case", str(case)])
    solver_text = solver_out + ("\n" + solver_err if solver_err else "")
    return write_result(case, solver_code, solver_text, mesh_text, mesh_code)


def parse_float_list(text: str) -> list[float]:
    values: list[float] = []
    for token in text.split(","):
        value = float(token.strip())
        if not finite(value):
            raise ValueError("alpha and Mach values must be finite")
        values.append(value)
    return values


def write_source_note(output_root: Path) -> None:
    """Persist the extracted private-report evidence beside the cases."""

    note = f"""# Report boundary-condition extraction

Source: `{REPORT_PATH}` (read locally; no external upload).

Relevant line references from the source report:

- Lines 5–13 request KC-135 Winglet cases at M=0, 0.6, and 0.85, with alpha from -4 to 12 degrees and an alpha=0 Cp distribution.
- Lines 17–25 identify the `seligdatfile.dat` coordinate source and the KC-135 Winglet geometry. The unavailable report coordinate path was replaced by the repository’s exact `@kc135winglet` snapshot (`{COORDINATE_PATH}`; hash `fnv1a64-9d705bfafc0bb4e`).
- Lines 36–51 give ISA at 7000 m: T={T_INF_K:g} K, p={P_INF_PA:g} Pa, rho={RHO_INF_KG_M3:g} kg/m^3, a={math.sqrt(GAMMA * R_AIR * T_INF_K):.5f} m/s, nu={NU_INF_M2_S:g} m^2/s.
- Lines 55–61 assume c={CHORD_M:g} m and assign U~=50 m/s to the M=0 reference, U={0.6 * math.sqrt(GAMMA * R_AIR * T_INF_K):.5f} m/s at M=0.6, and U={0.85 * math.sqrt(GAMMA * R_AIR * T_INF_K):.5f} m/s at M=0.85.
- Lines 82–85 and 113–127 state that the report used AeroSandbox NeuralFoil and Karman–Tsien, not OpenFOAM RANS. Lines 140–148 give the report’s alpha=0 reference CL/CD/CM/L/D values.
- Lines 190–201 request the -4,-2,0,2,4,6,8,10,12 degree polar sweep. Lines 263–285 discuss shock/wave-drag interpretations as model-dependent recommendations for Euler/RANS follow-up.

Implementation assumptions not supplied by the report: compressible perfect-gas `rhoSimpleFoam` v2606, SST RANS, fully turbulent TI=1%, length scale 0.07c, adiabatic no-slip wall, and first-cell setting {FIRST_LAYER_M:g} m. Forces use the chord frame (+x chord, +y normal, +z extrusion), drag along the freestream, lift +90 degrees, quarter-chord reference, Aref=c*{SPAN_M:g} m={CHORD_M * SPAN_M:g} m^2.

The M=0/U~=50 m/s inconsistency is preserved as `M0-reference`; its actual Mach at the report temperature is {50.0 / math.sqrt(GAMMA * R_AIR * T_INF_K):.8f}. Every output status is numerical/provisional unless residual evidence supports convergence; no result is labelled physically validated.
"""
    (output_root / "report-source-and-assumptions.md").write_text(note, encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mach", default="0,0.6,0.85", help="comma-separated report Mach points")
    parser.add_argument("--alphas", default=",".join(str(value) for value in ALPHA_POINTS_DEG))
    parser.add_argument("--workers", type=int, default=2)
    parser.add_argument("--end-time", type=int, default=300, help="rhoSimpleFoam SIMPLE iterations per case")
    # Write force and surface samples every iteration.  High-Mach cases can
    # terminate before a long write interval when pressure/temperature become
    # nonphysical; retaining the last finite diagnostic is intentional.
    parser.add_argument("--write-interval", type=int, default=1)
    parser.add_argument("--output-root", type=Path, default=OUTPUT_ROOT)
    parser.add_argument("--force", action="store_true", help="rebuild this driver's case directories")
    parser.add_argument("--prepare-only", action="store_true", help="write dictionaries and meshes inputs without running")
    args = parser.parse_args()
    if args.workers < 1:
        parser.error("--workers must be positive")
    if args.end_time < 1 or args.write_interval < 1:
        parser.error("iteration settings must be positive")
    requested_machs = parse_float_list(args.mach)
    alphas = parse_float_list(args.alphas)
    if not requested_machs or not alphas:
        parser.error("at least one Mach point and alpha are required")
    if any(value < 0.0 or value > 1.2 for value in requested_machs):
        parser.error("Mach points must be within 0 <= M <= 1.2")
    output_root = args.output_root.resolve()
    output_root.mkdir(parents=True, exist_ok=True)
    write_source_note(output_root)
    jobs = [(mach, alpha) for mach in requested_machs for alpha in alphas]
    rows: list[dict] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as executor:
        futures = {
            executor.submit(
                run_case,
                mach,
                alpha,
                output_root,
                args.end_time,
                args.write_interval,
                args.force,
                args.prepare_only,
            ): (mach, alpha)
            for mach, alpha in jobs
        }
        for future in concurrent.futures.as_completed(futures):
            mach, alpha = futures[future]
            try:
                row = future.result()
            except Exception as error:  # keep other independent cases running
                row = {
                    "case": case_name(mach, alpha),
                    "requested_mach": mach,
                    "alpha_deg": alpha,
                    "status": "driver-error",
                    "error": repr(error),
                }
            row["requested_mach"] = mach
            row["alpha_deg"] = alpha
            rows.append(row)
            print(f"{row.get('case')} status={row.get('status')}", flush=True)
    rows.sort(key=lambda row: (float(row.get("requested_mach", 0.0)), float(row.get("alpha_deg", 0.0))))
    manifest = {
        "study": "KC-135 Winglet report boundary-condition OpenFOAM study",
        "report_source": str(REPORT_PATH),
        "report_line_refs": {
            "conditions": "informe_tecnico_P1.md:36-61",
            "requested_points": "informe_tecnico_P1.md:5-13,190-201",
            "source_method_limits": "informe_tecnico_P1.md:82-85,113-127,263-285",
        },
        "solver": "rhoSimpleFoam v2606",
        "parallel_workers": args.workers,
        "cases_requested": len(jobs),
        "cases": rows,
        "created_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }
    (output_root / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return 0 if all(row.get("status") not in {"driver-error", "mesh-generation-failed", "gmshToFoam-failed"} for row in rows) else 1


if __name__ == "__main__":
    raise SystemExit(main())
