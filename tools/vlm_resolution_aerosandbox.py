# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Run the VLM mesh-resolution sweep in AeroSandbox, on geometry exported
from ALAS.

``alas-aero::vlm`` is a port of AeroSandbox's ``VortexLatticeMethod``. This
script runs the same mesh grid through the original, so a disagreement can be
attributed: identical numbers mean any mesh pathology is inherited physics,
and differing numbers mean the port diverges.

The geometry comes from ``export_airplane_geometry``, not from rebuilding a
preset in Python, so both codes panel exactly the same cross-sections,
chords, twists and airfoil coordinates. Only the solver differs.

Usage::

    .venv/Scripts/python tools/vlm_resolution_aerosandbox.py geom_AVE.json out.csv
"""

from __future__ import annotations

import json
import os
import sys
import time

import numpy as np
from aerosandbox.aerodynamics.aero_3D.vortex_lattice_method import VortexLatticeMethod
from aerosandbox.atmosphere import Atmosphere
from aerosandbox.geometry.airfoil.airfoil import Airfoil
from aerosandbox.geometry.airplane import Airplane
from aerosandbox.geometry.wing import Wing, WingXSec
from aerosandbox.performance.operating_point import OperatingPoint

SPANWISE = [int(v) for v in os.environ.get("ALAS_SWEEP_SPAN", "1,2,3,4,6,10").split(",")]
CHORDWISE = [int(v) for v in os.environ.get("ALAS_SWEEP_CHORD", "1,2,3,4,6,8,12,16,24,32").split(",")]
PANEL_CAP = int(os.environ.get("ALAS_SWEEP_PANEL_CAP", "4000"))
PROBE_ALPHA_DEG = 2.0


def build_airplane(data: dict) -> Airplane:
    """Rebuild the exported aircraft exactly, one AeroSandbox object per
    ALAS object."""
    airfoils = [
        Airfoil(
            name=entry["name"],
            coordinates=np.array(entry["coordinates"], dtype=float),
        )
        for entry in data["airfoils"]
    ]

    wings = []
    for wing in data["wings"]:
        xsecs = [
            WingXSec(
                xyz_le=np.array(xsec["xyz_le"], dtype=float),
                chord=float(xsec["chord"]),
                twist=float(xsec["twist"]),
                airfoil=airfoils[xsec["airfoil"]],
            )
            for xsec in wing["xsecs"]
        ]
        wings.append(
            Wing(name=wing["name"], symmetric=bool(wing["symmetric"]), xsecs=xsecs)
        )

    return Airplane(
        name=data["name"],
        xyz_ref=np.array(data["xyz_ref"], dtype=float),
        wings=wings,
        fuselages=[],
        s_ref=float(data["s_ref"]),
        c_ref=float(data["c_ref"]),
        b_ref=float(data["b_ref"]),
    )


def main() -> int:
    source = sys.argv[1] if len(sys.argv) > 1 else "geom_AVE.json"
    destination = sys.argv[2] if len(sys.argv) > 2 else "asb_sweep.csv"

    with open(source, encoding="utf-8") as handle:
        data = json.load(handle)

    airplane = build_airplane(data)
    atmosphere = Atmosphere(altitude=float(data["cruise_altitude_m"]))
    velocity = float(data["cruise_mach"]) * float(atmosphere.speed_of_sound())

    # One strip per lofted section per side, which is what spanwise_resolution
    # multiplies; used only to skip meshes above the cap before paying for
    # them.
    strips = sum(
        (len(wing["xsecs"]) - 1) * (2 if wing["symmetric"] else 1)
        for wing in data["wings"]
    )

    rows = ["preset,span_res,chord_res,panels,solve_s,cl_lift,cd_drag,cm_pitch"]
    print(rows[0])
    for span in SPANWISE:
        for chord in CHORDWISE:
            if strips * span * chord > PANEL_CAP:
                continue
            op_point = OperatingPoint(
                atmosphere=atmosphere,
                velocity=velocity,
                alpha=PROBE_ALPHA_DEG,
                beta=0.0,
                p=0.0,
                q=0.0,
                r=0.0,
            )
            started = time.perf_counter()
            try:
                method = VortexLatticeMethod(
                    airplane=airplane,
                    op_point=op_point,
                    spanwise_resolution=span,
                    chordwise_resolution=chord,
                    verbose=False,
                )
                result = method.run()
                panels = int(np.size(method.vortex_strengths))
                row = (
                    f"{data['preset']},{span},{chord},{panels},"
                    f"{time.perf_counter() - started:.3f},"
                    f"{float(result['CL']):.6f},{float(result['CD']):.6f},"
                    f"{float(result['Cm']):.6f}"
                )
            except Exception as error:  # noqa: BLE001 - the failure is the datum
                row = f"{data['preset']},{span},{chord},,,ERROR,{type(error).__name__}: {error}"
            print(row, flush=True)
            rows.append(row)

    with open(destination, "w", encoding="utf-8", newline="\n") as handle:
        handle.write("\n".join(rows) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
