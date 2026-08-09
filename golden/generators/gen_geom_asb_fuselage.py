# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-geom::asb::fuselage``: AeroSandbox's ``Fuselage``/``FuselageXSec``,
scoped surface.

Covers what `docs/PORTING.md` names for this row: ``FuselageXSec``
construction from either ``radius`` or explicit ``width``/``height``,
``.translate()`` on both classes, and the ``xyz_c``/``width``/``height``
fields -- built on stations shaped like
``alas/geometry/aircraft_builder.py``'s ``_build_fuselage``/``_build_engines``,
with the literal default values ``alas.config.geometry_config.GeometryConfig``
and ``alas.config.design_variables.DesignVector`` carry (recorded next to
each number below rather than imported, for the same reason
``gen_geom_asb_wing.py`` records its own: this generator exercises
AeroSandbox's ``Fuselage`` directly and has no other reason to depend on
``alas-config``'s Python counterpart).

Cases:

* ``main_fuselage``: the circular (non-ovoid) branch of ``_build_fuselage``,
  at ``FuselageConfig``/``DesignVector`` defaults -- ``height_m`` is left
  ``None``, so every station is ``radius=``-constructed. 9 nose stations
  (``sinspace(0, 1, 10)[:-1]``), 2 cabin stations, 9 tailcone stations
  (``linspace(0, 1, 10)[1:]``): 20 stations total.

* ``ovoid_fuselage``: the same 20 stations' ``x``/``z``/equivalent-radius
  values, but built through the ``is_ovoid`` branch instead -- an explicit,
  non-default ``height_m`` (7.5 m, versus the 6.2 m ``diameter_m``) scales
  width and height independently at ``shape=2.0``. Exercises the
  ``width=``/``height=`` constructor path `docs/PORTING.md` names, which the
  circular case above never reaches.

* ``nacelle``: a small ``radius=``-constructed ``Fuselage`` built from
  ``EngineConfig.nacelle_profile``'s default (x-station, radius-fraction)
  pairs scaled by ``EngineConfig.radius_scale_m``, then ``.translate()``'d --
  shaped like ``_build_engines``'s per-nacelle ``Fuselage``. The translation
  vector is representative of that method's magnitudes rather than
  reproduced from its full wing-interpolation formula, which belongs to
  ``alas-geom::builder`` (a later, out-of-scope module) and not to this one.
"""

from __future__ import annotations

import aerosandbox.numpy as anp
import numpy as np

import _framework
from aerosandbox.geometry.fuselage import Fuselage, FuselageXSec


def _vec3(v) -> list[float]:
    return [float(v[0]), float(v[1]), float(v[2])]


def _xsec_record(xsec: FuselageXSec) -> dict:
    return {
        "xyz_c": _vec3(np.asarray(xsec.xyz_c, dtype=float)),
        "width": float(xsec.width),
        "height": float(xsec.height),
        "shape": float(xsec.shape),
    }


def _fuselage_record(fuselage: Fuselage) -> dict:
    return {
        "name": fuselage.name,
        "xsecs": [_xsec_record(xsec) for xsec in fuselage.xsecs],
    }


# alas.config.geometry_config.FuselageConfig defaults.
DIAMETER_M = 6.2
NOSE_Z_M = -0.5
CABIN_START_X_M = 6.0
CABIN_Z_M = 0.2
TAILCONE_LENGTH_M = 14.0
TAIL_Z_M = 1.8

# alas.config.design_variables.DesignVector default.
FUSELAGE_LENGTH_M = 76.72

RADIUS = DIAMETER_M / 2
CABIN_END = FUSELAGE_LENGTH_M - TAILCONE_LENGTH_M


def _stations():
    """The 20 (x, z, r) triples ``_build_fuselage`` computes, independent of
    the circular/ovoid choice -- that choice only affects how each triple is
    turned into a ``FuselageXSec``."""
    stations = []

    x_nose = anp.sinspace(0, 1, 10)
    for xi in x_nose[:-1]:
        x_val = xi * CABIN_START_X_M
        z_val = CABIN_Z_M + (NOSE_Z_M - CABIN_Z_M) * (1 - xi) ** 2
        r_val = RADIUS * (1 - (1 - xi) ** 2) ** 0.5
        stations.append((float(x_val), float(z_val), float(r_val)))

    stations.append((CABIN_START_X_M, CABIN_Z_M, RADIUS))
    stations.append((CABIN_END, CABIN_Z_M, RADIUS))

    x_tail = anp.linspace(0, 1, 10)
    for xi in x_tail[1:]:
        x_val = CABIN_END + xi * TAILCONE_LENGTH_M
        z_val = CABIN_Z_M + (TAIL_Z_M - CABIN_Z_M) * xi**1.5
        r_val = RADIUS * (1 - xi**1.5)
        stations.append((float(x_val), float(z_val), float(r_val)))

    return stations


def _build_main_fuselage() -> Fuselage:
    xsecs = [
        FuselageXSec(xyz_c=[x, 0, z], radius=r) for x, z, r in _stations()
    ]
    return Fuselage(name="Fuselage", xsecs=xsecs)


def _build_ovoid_fuselage(height_m: float) -> Fuselage:
    # Mirrors `_build_fuselage.make_xsec`'s `is_ovoid` branch: width/height
    # scaled from the same equivalent-circular radius, at shape=2.0.
    xsecs = []
    for x, z, r in _stations():
        local_width = r * 2
        local_height = r * 2 * (height_m / DIAMETER_M)
        xsecs.append(
            FuselageXSec(
                xyz_c=[x, 0, z],
                width=local_width,
                height=local_height,
                shape=2.0,
            )
        )
    return Fuselage(name="Fuselage", xsecs=xsecs)


def _build_nacelle() -> Fuselage:
    # alas.config.geometry_config.EngineConfig defaults.
    nacelle_profile = [
        (0.0, 0.40),
        (0.6, 0.92),
        (1.2, 1.0),
        (4.3, 1.0),
        (5.9, 0.82),
        (7.8, 0.45),
    ]
    radius_scale_m = 2.1

    return Fuselage(
        name="Nacelle R",
        xsecs=[
            FuselageXSec(xyz_c=[x, 0, 0], radius=radius_scale_m * r)
            for x, r in nacelle_profile
        ],
    )


def main() -> None:
    main_fuselage = _build_main_fuselage()
    ovoid_fuselage = _build_ovoid_fuselage(height_m=7.5)

    nacelle = _build_nacelle()
    # Representative of _build_engines's per-nacelle translation magnitude
    # (x_inlet near the wing LE, y_pos a spanwise engine station, z_nacelle
    # below the wing) -- not reproduced from its full wing-interpolation
    # formula, which is out of this module's scope; see module doc.
    nacelle_shift = [28.65, 9.8, -3.59]
    translated_nacelle = nacelle.translate(nacelle_shift)

    _framework.write(
        "geom",
        "asb_fuselage",
        {
            "main_fuselage": _fuselage_record(main_fuselage),
            "ovoid_fuselage": _fuselage_record(ovoid_fuselage),
            "nacelle": {
                "untranslated": _fuselage_record(nacelle),
                "shift": [float(v) for v in nacelle_shift],
                "translated": _fuselage_record(translated_nacelle),
            },
        },
        description=(
            "aerosandbox.geometry.fuselage.Fuselage/FuselageXSec, scoped to "
            "construction from radius or width/height and translate, on "
            "stations shaped like aircraft_builder.py's _build_fuselage "
            "(circular and ovoid branches) and _build_engines"
        ),
    )


if __name__ == "__main__":
    main()
