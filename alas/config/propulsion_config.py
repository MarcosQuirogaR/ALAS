# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Propulsion cycle analysis configuration -- component polytropic efficiencies
and pressure-loss factors for the on-design turbofan cycle model
(:mod:`alas.physics.propulsion`).

Defaults reproduce the exact assumptions
``external tools/suave_runner/vehicle_builder.py`` hardcodes when it builds a
SUAVE turbofan network, so this module's on-design cycle and SUAVE's
mission-simulated engine start from the same component-level physics --
changing a value here does not automatically change SUAVE's (that file is a
separate, frozen SUAVE-side script), but a user auditing "are these
consistent?" finds identical numbers in both places.
"""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class PropulsionCycleConfig:
    """Tunable component efficiencies for the on-design turbofan cycle,
    exposed in Advanced Settings -> Engine Designer."""

    inlet_pressure_recovery: float = field(
        default=0.98,
        metadata={
            "label": "Inlet pressure recovery",
            "unit": "-",
            "help": "Total-pressure recovery through the inlet (ram + duct losses). "
            "Matches SUAVE's inlet_nozzle.pressure_ratio.",
        },
    )
    lpc_pressure_ratio_split: float = field(
        default=1.20,
        metadata={
            "label": "LPC pressure-ratio split",
            "unit": "-",
            "help": "Fixed low-pressure-compressor (booster) pressure ratio; the high-pressure compressor "
            "makes up the rest of the overall (core) pressure ratio (HPC = OPR / this value). "
            "Matches SUAVE's fixed LPC split.",
        },
    )
    lpc_polytropic_efficiency: float = field(
        default=0.91,
        metadata={
            "label": "LPC polytropic efficiency",
            "unit": "-",
        },
    )
    hpc_polytropic_efficiency: float = field(
        default=0.93,
        metadata={
            "label": "HPC polytropic efficiency",
            "unit": "-",
        },
    )
    fan_polytropic_efficiency: float = field(
        default=0.93,
        metadata={
            "label": "Fan polytropic efficiency",
            "unit": "-",
        },
    )
    combustor_pressure_ratio: float = field(
        default=0.95,
        metadata={
            "label": "Combustor pressure ratio",
            "unit": "-",
            "help": "Total-pressure loss fraction across the combustor.",
        },
    )
    combustor_efficiency: float = field(
        default=0.99,
        metadata={
            "label": "Combustor efficiency",
            "unit": "-",
        },
    )
    hpt_polytropic_efficiency: float = field(
        default=0.95,
        metadata={
            "label": "HPT polytropic efficiency",
            "unit": "-",
        },
    )
    lpt_polytropic_efficiency: float = field(
        default=0.95,
        metadata={
            "label": "LPT polytropic efficiency",
            "unit": "-",
        },
    )
    turbine_mechanical_efficiency: float = field(
        default=0.99,
        metadata={
            "label": "Turbine mechanical efficiency",
            "unit": "-",
            "help": "Shaft power-transmission efficiency for both spools (HPT-HPC, LPT-LPC+fan).",
        },
    )
    core_nozzle_pressure_ratio: float = field(
        default=0.99,
        metadata={
            "label": "Core nozzle pressure ratio",
            "unit": "-",
        },
    )
    fan_nozzle_pressure_ratio: float = field(
        default=0.99,
        metadata={
            "label": "Fan nozzle pressure ratio",
            "unit": "-",
        },
    )
    core_nozzle_efficiency: float = field(
        default=0.95,
        metadata={
            "label": "Core nozzle efficiency",
            "unit": "-",
            "help": "Polytropic expansion efficiency of the core exhaust nozzle.",
        },
    )
    fan_nozzle_efficiency: float = field(
        default=0.95,
        metadata={
            "label": "Fan nozzle efficiency",
            "unit": "-",
            "help": "Polytropic expansion efficiency of the fan (bypass) nozzle.",
        },
    )
    fuel_heating_value_kj_kg: float = field(
        default=42_800.0,
        metadata={
            "label": "Fuel heating value",
            "unit": "kJ/kg",
            "help": "Lower heating value of Jet-A/Jet-A1 fuel.",
        },
    )
    cp_cold_j_kgk: float = field(
        default=1004.5,
        metadata={
            "label": "Cold-section specific heat (cp)",
            "unit": "J/(kg.K)",
            "help": "Air-standard specific heat used for the inlet/fan/compressor (unburned-air) stations.",
        },
    )
    gamma_cold: float = field(
        default=1.4,
        metadata={
            "label": "Cold-section ratio of specific heats",
            "unit": "-",
        },
    )
    cp_hot_j_kgk: float = field(
        default=1156.9,
        metadata={
            "label": "Hot-section specific heat (cp)",
            "unit": "J/(kg.K)",
            "help": "Combustion-gas specific heat used for the combustor/turbine/core-nozzle stations.",
        },
    )
    gamma_hot: float = field(
        default=1.33,
        metadata={
            "label": "Hot-section ratio of specific heats",
            "unit": "-",
        },
    )
    fan_face_mach: float = field(
        default=0.55,
        metadata={
            "label": "Assumed fan-face Mach number (static anchor)",
            "unit": "-",
            "help": "Used only to sanity-check the design mass flow implied by anchoring the cycle to the "
            "engine's rated static thrust -- a conceptual-design-level assumption, not a real "
            "corrected-flow schedule.",
        },
    )
