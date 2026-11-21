# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Evaluate the reference implementation at the U.S. Standard Atmosphere, 1976.

The reference reads every ambient state through ``aerosandbox.Atmosphere``,
whose default ``method="differentiable"`` is a fitted spline through the ISA.
The product evaluates the U.S. Standard Atmosphere, 1976 (NOAA-S/T 76-1562)
everywhere, so a fixture generated with the fit compares two atmospheres, not
two implementations of the model under test (up to 1.9 % in temperature and
1.7 % in density near 11 km).

``use_us1976()`` replaces ``Atmosphere.pressure`` and ``Atmosphere.temperature``
in this generator process only; the reference tree is not edited, so the
fixture still names a clean reference commit. Everything else in the class
(density, speed of sound, viscosity) is derived from these two exactly as it
was, except that the specific gas constant is the product's single
``GAS_CONSTANT_AIR``, 287.052 874 2 J/(kg K) (ICAO Doc 7488, 3rd edition,
primary constants), instead of AeroSandbox's 8.31432 / 28.9644e-3, which is
6.9e-7 higher.

The table and formulas are those of ``alas-atmo::us1976``: altitude is
geopotential metres, clamped to the table's [-2 km, 84.852 km] band, the
barometric exponent uses standard gravity 9.80665 m/s^2.
"""

from __future__ import annotations

import numpy as np

STANDARD_GRAVITY = 9.80665
GAS_CONSTANT_AIR = 287.0528742

# Geopotential altitude [m], temperature [K], pressure [Pa] at the nine breaks.
_BREAKS = (
    (-2000.0, 301.15, 127_774.0),
    (0.0, 288.15, 101_325.0),
    (11_000.0, 216.65, 22_632.1),
    (20_000.0, 216.65, 5474.89),
    (32_000.0, 228.65, 868.019),
    (47_000.0, 270.65, 110.906),
    (51_000.0, 270.65, 66.9389),
    (71_000.0, 214.65, 3.95642),
    (84_852.0, 186.95, 0.3734),
)


def _segment(h: float):
    for i in range(len(_BREAKS) - 2, -1, -1):
        if h >= _BREAKS[i][0]:
            return i
    return 0


def _state(h: float) -> tuple[float, float]:
    h = min(max(float(h), _BREAKS[0][0]), _BREAKS[-1][0])
    i = _segment(h)
    z0, t0, p0 = _BREAKS[i]
    z1, t1, _ = _BREAKS[i + 1]
    alpha = -(t1 - t0) / (z1 - z0)
    dz = h - z0
    if alpha == 0.0:
        p = p0 * np.exp(-dz * STANDARD_GRAVITY / (GAS_CONSTANT_AIR * t0))
    else:
        p = p0 * (1.0 - alpha * dz / t0) ** (STANDARD_GRAVITY / (alpha * GAS_CONSTANT_AIR))
    return float(p), t0 - dz * alpha


def pressure(altitude):
    if np.ndim(altitude) == 0:
        return _state(altitude)[0]
    return np.vectorize(lambda h: _state(h)[0], otypes=[float])(altitude)


def temperature(altitude):
    if np.ndim(altitude) == 0:
        return _state(altitude)[1]
    return np.vectorize(lambda h: _state(h)[1], otypes=[float])(altitude)


def use_us1976() -> None:
    """Make every ``aerosandbox.Atmosphere`` in this process evaluate US1976."""
    import aerosandbox as asb  # noqa: PLC0415 - patch target
    import importlib  # noqa: PLC0415

    asb_atmosphere = importlib.import_module("aerosandbox.atmosphere.atmosphere")

    asb_atmosphere.gas_constant_air = GAS_CONSTANT_AIR

    def _pressure(self):
        return pressure(self.altitude)

    def _temperature(self):
        return temperature(self.altitude) + self.temperature_deviation

    asb.Atmosphere.pressure = _pressure
    asb.Atmosphere.temperature = _temperature


def use_geopotential_input_in_suave() -> None:
    """Make SUAVE's ``US_Standard_1976`` read its altitude as geopotential.

    SUAVE converts its input from geometric to geopotential altitude
    (``z / (1 + z / R)`` with its mean radius 6 371 km) before consulting the
    table. The product reads every altitude as pressure (geopotential)
    altitude, so the same number means a different ambient state in the two.
    The wrapper hands SUAVE the geometric altitude whose conversion returns the
    requested geopotential one, ``H R / (R - H)``, which leaves every other
    line of SUAVE's evaluation as it was.
    """
    import numpy as _np  # noqa: PLC0415
    from SUAVE.Analyses.Atmospheric.US_Standard_1976 import (  # noqa: PLC0415
        US_Standard_1976,
    )

    original = US_Standard_1976.compute_values
    if getattr(original, "_alas_geopotential", False):
        return

    def compute_values(self, altitude, temperature_deviation=0.0, var_gamma=False):
        radius = self.planet.mean_radius
        h = _np.asarray(altitude, dtype=float)
        geometric = h * radius / (radius - h)
        if _np.ndim(altitude) == 0:
            geometric = float(geometric)
        return original(self, geometric, temperature_deviation, var_gamma)

    compute_values._alas_geopotential = True
    US_Standard_1976.compute_values = compute_values
