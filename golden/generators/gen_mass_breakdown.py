# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-mass::breakdown``: the weight-and-balance buildup end to end.

Exercises ``alas/physics/mass.py``'s four public functions --
``calculate_component_masses``, ``define_mass_coordinates``,
``calculate_physical_cg`` and the ``run_mass_analysis`` that composes them --
on the actual nominal aircraft ``AircraftBuilder(GeometryConfig()).build`` (the
same plane ``gen_geom_builder.py`` already checks, with engines so the nacelle
branch of ``define_mass_coordinates`` is reached) rather than a synthetic probe.

Every case runs ``run_mass_analysis`` and records all three of its outputs
(the component masses, the component coordinates, and the global CG), which
covers the other three functions transitively. The plane and the
``GeometryConfig`` are held at their defaults throughout so the fixture reads
back against one built aircraft; the spread instead comes from
``DesignRequirements``/``MassModelConfig`` overrides applied by ``setattr``
after a default construction (so no dataclass ``__post_init__`` re-derives a
field out from under the recorded value), plus one case exercising the
``payload_layout`` override branch:

* ``nominal``: pure defaults, no payload layout -- the pipeline's own call.
* ``low_mtow``: a lighter, denser-seated design, moving every mass fraction's
  product and the fuel remainder.
* ``cargo``: ``aircraft_type='cargo'``, so ``payload_kg`` takes its freighter
  branch and the payload centroid moves with ``cargo_payload_kg`` instead of
  ``num_passengers``.
* ``tuned_mass_model``: non-default empirical fractions and a lower cabin
  payload density, moving both the masses and the occupied-cabin payload
  centroid.
* ``payload_layout``: a detailed-layout override with positive mass, replacing
  the lumped Payload mass/coordinate and recomputing the Fuel remainder.
"""

from __future__ import annotations

from types import SimpleNamespace

import _framework

_framework.add_alas_to_path()

from alas.config.geometry_config import GeometryConfig  # noqa: E402
from alas.config.mass_config import MassModelConfig  # noqa: E402
from alas.config.requirements import DesignRequirements  # noqa: E402
from alas.geometry.aircraft_builder import AircraftBuilder  # noqa: E402
from alas.physics.mass import run_mass_analysis  # noqa: E402


def _requirements(overrides: dict) -> DesignRequirements:
    req = DesignRequirements()
    for key, value in overrides.items():
        setattr(req, key, value)
    return req


def _mass_model(overrides: dict) -> MassModelConfig:
    mm = MassModelConfig()
    for key, value in overrides.items():
        setattr(mm, key, value)
    return mm


def _vec3(v) -> list[float]:
    return [float(v[0]), float(v[1]), float(v[2])]


def _masses_record(masses: dict) -> dict:
    return {key: float(value) for key, value in masses.items()}


def _coords_record(coords: dict) -> dict:
    return {key: _vec3(value) for key, value in coords.items()}


# Each case: DesignRequirements overrides, MassModelConfig overrides, and an
# optional payload-layout stub (the three attributes run_mass_analysis reads
# off a PayloadLayout: total_mass, cg_x, cg_y).
_CASES = [
    {
        "name": "nominal",
        "requirements": {},
        "mass_model": {},
        "payload_layout": None,
    },
    {
        "name": "low_mtow",
        "requirements": {"mtow_kg": 220_000.0, "num_passengers": 300},
        "mass_model": {},
        "payload_layout": None,
    },
    {
        "name": "cargo",
        "requirements": {"aircraft_type": "cargo", "cargo_payload_kg": 52_000.0},
        "mass_model": {},
        "payload_layout": None,
    },
    {
        "name": "tuned_mass_model",
        "requirements": {},
        "mass_model": {
            "suspended_mass_fraction": 0.80,
            "systems_mass_fraction": 0.09,
            "furnishings_mass_fraction": 0.12,
            "cabin_payload_density_kg_m": 650.0,
        },
        "payload_layout": None,
    },
    {
        "name": "payload_layout",
        "requirements": {},
        "mass_model": {},
        "payload_layout": {"total_mass": 46_000.0, "cg_x": 34.5, "cg_y": 0.3},
    },
]


def main() -> None:
    builder = AircraftBuilder(GeometryConfig())
    plane = builder.build(dv=None, include_engines=True)
    geometry_config = builder.geometry

    if not any("Nacelle" in f.name for f in plane.fuselages):
        raise SystemExit(
            "the default build produced no nacelle fuselages; this fixture's "
            "nominal case would not exercise define_mass_coordinates's engine "
            "branch"
        )

    results = []
    for case in _CASES:
        requirements = _requirements(case["requirements"])
        mass_model = _mass_model(case["mass_model"])
        layout = case["payload_layout"]
        payload_layout = (
            SimpleNamespace(
                total_mass=layout["total_mass"],
                cg_x=layout["cg_x"],
                cg_y=layout["cg_y"],
            )
            if layout is not None
            else None
        )

        masses, coords, cg = run_mass_analysis(
            plane,
            requirements,
            geometry_config,
            mass_model,
            payload_layout=payload_layout,
        )

        results.append(
            {
                "name": case["name"],
                "requirements": case["requirements"],
                "mass_model": case["mass_model"],
                "payload_layout": layout,
                "masses": _masses_record(masses),
                "coords": _coords_record(coords),
                "cg": _vec3(cg),
            }
        )

    _framework.write(
        "mass",
        "breakdown",
        {"cases": results},
        description=(
            "alas.physics.mass.run_mass_analysis on the nominal "
            "AircraftBuilder(GeometryConfig()).build aircraft (with engines), "
            "across DesignRequirements/MassModelConfig overrides and the "
            "payload-layout branch -- component masses, coordinates and global CG"
        ),
    )


if __name__ == "__main__":
    main()
