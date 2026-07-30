# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Mission-results CSV export.

``suave_example.py`` calls ``export_data.export_simulation_results(results,
'ave_flight_data.csv')`` but never shipped the module. This is that module --
it walks a SUAVE ``results`` object the same way
``SUAVE.Plots.Performance.Mission_Plots`` does internally and writes a flat
CSV, covering the same breadth ``suave_example.py``'s own ``plot_mission()``
plotted (flight conditions, aerodynamic forces, aerodynamic coefficients,
drag components, altitude/SFC/weight, velocities -- see
``plot_flight_conditions``, ``plot_aerodynamic_forces``,
``plot_aerodynamic_coefficients``, ``plot_drag_components``,
``plot_altitude_sfc_weight``, ``plot_aircraft_velocities`` in
``SUAVE/Plots/Performance/Mission_Plots.py`` for the exact
``segment.conditions...`` access pattern each column mirrors), not just the
altitude/speed/mass subset needed for the route globe.

``Time_s`` and ``TAS_m_s`` are kept under those exact names because
``route_globe.m`` integrates them directly
(``cumtrapz(simData.Time_s, simData.TAS_m_s)``) to synchronise the KML route
distance with aircraft mass.
"""

from __future__ import annotations

import csv
from pathlib import Path

import _compat  # noqa: F401  (must run before `import SUAVE`)
from SUAVE.Core import Units

_RHO_SL = 1.225   # ISA sea-level density [kg/m^3], the EAS reference SUAVE itself uses
_G0 = 9.80665     # standard gravity [m/s^2], for the N -> kgf conversion in SFC

_COLUMNS = [
    "Time_s", "Segment",
    # Flight conditions (plot_flight_conditions)
    "Altitude_m", "TAS_m_s", "EAS_m_s", "Mach", "Density_kg_m3", "Range_m", "Pitch_deg",
    # Aerodynamic coefficients (plot_aerodynamic_coefficients)
    "AoA_deg", "CL", "CD", "L_over_D",
    # Aerodynamic forces (plot_aerodynamic_forces)
    "Throttle", "Lift_N", "Drag_N", "Thrust_N",
    # Drag breakdown (plot_drag_components)
    "CD_parasite", "CD_induced", "CD_compressible", "CD_miscellaneous", "CD_total",
    # Weight / fuel (plot_altitude_sfc_weight)
    "Mass_kg", "MassFlowRate_kg_s", "SFC_kg_kgf_hr",
]


def _col(array_2d, row, col=0):
    """Safe accessor for a SUAVE conditions array row; NaN if unavailable."""
    try:
        return float(array_2d[row, col])
    except Exception:
        return float("nan")


def export_simulation_results(results, filename: str | Path) -> None:
    """Flatten every mission segment's conditions into one chronological CSV."""
    rows = []
    t_offset = 0.0
    for segment in results.segments.values():
        c = segment.conditions
        time = c.frames.inertial.time[:, 0]
        n = len(time)
        tag = segment.tag

        cl = c.aerodynamics.lift_coefficient[:, 0]
        cd = c.aerodynamics.drag_coefficient[:, 0]
        db = c.aerodynamics.drag_breakdown

        # Mission segments report time from zero at their own start; offset
        # each one by the previous segment's final time so Time_s is
        # monotonic across the whole flight.
        seg_start = float(time[0])
        for i in range(n):
            l_over_d = cl[i] / cd[i] if cd[i] not in (0.0, None) else float("nan")
            tas = _col(c.freestream.velocity, i)
            density = _col(c.freestream.density, i)
            eas = tas * (density / _RHO_SL) ** 0.5 if density == density else float("nan")
            thrust = _col(c.frames.body.thrust_force_vector, i)
            mdot = _col(c.weights.vehicle_mass_rate, i)
            # kg fuel / (kgf . hr), the standard TSFC unit -- numerically the
            # same ratio as SUAVE's own imperial lb/lbf/hr (both are mass
            # flow per unit weight-force), via kgf = thrust_N / g0.
            sfc = (mdot * 3600.0) / (thrust / _G0) if thrust not in (0.0, None) else float("nan")
            rows.append([
                t_offset + (float(time[i]) - seg_start), tag,
                _col(c.freestream.altitude, i), tas, eas,
                _col(c.freestream.mach_number, i), density,
                _col(c.frames.inertial.aircraft_range, i),
                _col(c.frames.body.inertial_rotations, i, 1) / Units.deg,
                _col(c.aerodynamics.angle_of_attack, i) / Units.deg,
                float(cl[i]), float(cd[i]), float(l_over_d),
                _col(c.propulsion.throttle, i),
                -_col(c.frames.wind.lift_force_vector, i, 2),
                -_col(c.frames.wind.drag_force_vector, i, 0),
                thrust,
                _col(db.parasite.total, i), _col(db.induced.total, i),
                _col(db.compressible.total, i), _col(db.miscellaneous.total, i),
                _col(db.total, i),
                _col(c.weights.total_mass, i), mdot, sfc,
            ])
        t_offset += float(time[-1]) - seg_start

    path = Path(filename)
    with path.open("w", newline="", encoding="utf-8") as f:
        writer = csv.writer(f)
        writer.writerow(_COLUMNS)
        writer.writerows(rows)
    print(f"  [file] mission data written: {path}")
