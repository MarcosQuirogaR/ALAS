# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-geom::builder``: the full aircraft assembly, end to end.

This is Phase 3's centerpiece fixture: ``AircraftBuilder(GeometryConfig()).
build(dv=None, include_engines=True)``, the actual nominal aircraft
``alas``'s own pipeline builds -- not a synthetic probe geometry, unlike
``gen_geom_asb_wing.py``/``gen_geom_asb_fuselage.py``, which is what lets
this fixture exercise all three of ``AirfoilLibrary.get``'s branches
together on real input (``docs/PORTING.md``'s Geometry section names them:
``naca2410`` from the Selig corpus at the wing tip, ``SC2-0714`` from the
built-in named coordinates at the wing root, ``naca0012`` from AeroSandbox's
NACA fallback at the tail).

Cases:

* ``with_engines``: the default build with ``include_engines=True``. Every
  wing's full structure (name, symmetric, every xsec's xyz_le/chord/twist
  and its airfoil's name + full coordinate array, plus
  area/span/mean_aerodynamic_chord/aerodynamic_center/taper_ratio), every
  fuselage's name and xsec list (xyz_c/width/height), and the returned
  ``Airplane``'s name/xyz_ref/s_ref/c_ref/b_ref.

* ``without_engines``: the same build with ``include_engines=False``, to
  confirm the fuselage list is exactly the main fuselage.

The default ``EngineConfig.spanwise_positions_m`` is ``[9.8, -9.8]`` -- two
wing-mounted engines, no centerline one -- so this fixture exercises only
the wing-mounted branch of ``_build_engines``. The centerline
(``y_pos == 0.0``) branch has no default-configuration input that reaches
it and is instead covered by an ``alas-geom::builder`` unit test built on a
synthetic engine position.
"""

from __future__ import annotations

import numpy as np

import _framework

_framework.add_alas_to_path()

from alas.config.design_variables import DesignVector  # noqa: E402
from alas.config.geometry_config import GeometryConfig  # noqa: E402
from alas.geometry.aircraft_builder import AircraftBuilder  # noqa: E402


def _vec3(v) -> list[float]:
    return [float(v[0]), float(v[1]), float(v[2])]


def _coords(coordinates) -> list[list[float]]:
    return [[float(x), float(y)] for x, y in np.asarray(coordinates, dtype=float).tolist()]


def _xsec_record(xsec) -> dict:
    return {
        "xyz_le": _vec3(np.asarray(xsec.xyz_le, dtype=float)),
        "chord": float(xsec.chord),
        "twist": float(xsec.twist),
        "airfoil_name": xsec.airfoil.name,
        "airfoil_coordinates": _coords(xsec.airfoil.coordinates),
    }


def _wing_record(wing) -> dict:
    return {
        "name": wing.name,
        "symmetric": bool(wing.symmetric),
        "xsecs": [_xsec_record(xsec) for xsec in wing.xsecs],
        "area": float(wing.area()),
        "span": float(wing.span()),
        "mean_aerodynamic_chord": float(wing.mean_aerodynamic_chord()),
        "aerodynamic_center": _vec3(wing.aerodynamic_center()),
        "taper_ratio": float(wing.taper_ratio()),
    }


def _fuselage_xsec_record(xsec) -> dict:
    return {
        "xyz_c": _vec3(np.asarray(xsec.xyz_c, dtype=float)),
        "width": float(xsec.width),
        "height": float(xsec.height),
    }


def _fuselage_record(fuselage) -> dict:
    return {
        "name": fuselage.name,
        "xsecs": [_fuselage_xsec_record(xsec) for xsec in fuselage.xsecs],
    }


def _airplane_record(airplane) -> dict:
    return {
        "name": airplane.name,
        "xyz_ref": _vec3(np.asarray(airplane.xyz_ref, dtype=float)),
        "s_ref": float(airplane.s_ref),
        "c_ref": float(airplane.c_ref),
        "b_ref": float(airplane.b_ref),
        "wings": [_wing_record(wing) for wing in airplane.wings],
        "fuselages": [_fuselage_record(fuselage) for fuselage in airplane.fuselages],
    }


def main() -> None:
    builder = AircraftBuilder(GeometryConfig())
    dv = DesignVector()

    with_engines = builder.build(dv=dv, include_engines=True)
    without_engines = builder.build(dv=dv, include_engines=False)

    if len(without_engines.fuselages) != 1:
        raise SystemExit(
            "expected include_engines=False to produce exactly one fuselage, "
            f"got {len(without_engines.fuselages)}"
        )
    expected_engine_count = len(builder.geometry.engine.spanwise_positions_m)
    if len(with_engines.fuselages) != 1 + expected_engine_count:
        raise SystemExit(
            "expected include_engines=True to append one nacelle per "
            f"spanwise position ({expected_engine_count}), got "
            f"{len(with_engines.fuselages) - 1}"
        )
    if any(pos == 0.0 for pos in builder.geometry.engine.spanwise_positions_m):
        raise SystemExit(
            "the default engine layout now includes a centerline position; "
            "this fixture's docstring claim that only the wing-mounted "
            "branch is exercised is stale"
        )

    _framework.write(
        "geom",
        "builder",
        {
            "with_engines": _airplane_record(with_engines),
            "without_engines": _airplane_record(without_engines),
        },
        description=(
            "alas.geometry.aircraft_builder.AircraftBuilder(GeometryConfig())"
            ".build(dv=None, include_engines=...) -- the actual nominal "
            "aircraft, with every wing's full xsec/airfoil structure, every "
            "fuselage's xsec list, and the returned Airplane's reference "
            "quantities, for both include_engines=True and False"
        ),
    )


if __name__ == "__main__":
    main()
