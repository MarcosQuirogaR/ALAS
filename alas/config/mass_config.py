# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""Mass model configuration -- empirical mass fractions for the Torenbeek weight buildup."""

from __future__ import annotations
from dataclasses import dataclass, field


@dataclass
class MassModelConfig:
    """Tunable mass fractions and Torenbeek structural parameters.

    Exposed in Advanced Settings -> Mass model so the user can calibrate the
    weight estimate without touching code.  Default values follow Torenbeek
    (1982) and Raymer (5th ed.) for CS-25/FAR-25 class transports.
    """

    suspended_mass_fraction: float = field(
        default=0.75,
        metadata={
            "label": "Wing suspended-mass fraction",
            "help": "Fraction of MTOW treated as 'suspended' mass in the Torenbeek wing structural formula "
            "(everything the wing structure must carry other than itself). Typical commercial transport: 0.70-0.78.",
        },
    )

    max_airspeed_for_flaps_ms: float = field(
        default=90.0,
        metadata={
            "label": "Max airspeed with flaps extended",
            "unit": "m/s",
            "help": "Design airspeed with flaps extended, fed into the Torenbeek wing-mass formula.",
        },
    )
    flap_deflection_angle_deg: float = field(
        default=40.0,
        metadata={
            "label": "Max take-off flap deflection",
            "unit": "deg",
            "help": "Maximum flap deflection angle, fed into the Torenbeek wing-mass formula.",
        },
    )

    landing_gear_mass_fraction: float = field(
        default=0.04,
        metadata={
            "label": "Landing-gear mass fraction",
            "help": "Landing gear mass as a fraction of MTOW. Raymer Table 15.2: ~4% for commercial jet transports.",
        },
    )

    propulsion_twr_factor: float = field(
        default=6.0,
        metadata={
            "label": "Engine thrust-to-weight factor",
            "help": "Dry engine mass is estimated as thrust / (this factor * g). Historical engine thrust-to-weight "
            "ratios are ~5-7, so this factor is typically ~6.",
        },
    )
    propulsion_installation_factor: float = field(
        default=1.30,
        metadata={
            "label": "Propulsion installation overhead",
            "help": "Multiplier on dry engine mass accounting for pylon, cowling, fire suppression and other installed accessories.",
        },
    )
    propulsion_mass_fallback_fraction: float = field(
        default=0.07,
        metadata={
            "label": "Propulsion mass fallback fraction",
            "help": "Fallback propulsion mass as a fraction of MTOW, used only if the selected engine isn't found in the database.",
        },
    )

    systems_mass_fraction: float = field(
        default=0.11,
        metadata={
            "label": "Systems & equipment mass fraction",
            "help": "Avionics, electrical, ECS, APU, etc. as a fraction of MTOW. Raymer Table 15.2: 9-13% for commercial transports.",
        },
    )

    furnishings_mass_fraction: float = field(
        default=0.10,
        metadata={
            "label": "Furnishings & operations mass fraction",
            "help": "Passenger seats, galleys, lavatories, insulation, crew, paint, and operational empty items as a fraction of MTOW. Typically 10-14% for passenger transports.",
        },
    )

    cabin_payload_density_kg_m: float = field(
        default=800.0,
        metadata={
            "label": "Payload linear density",
            "unit": "kg/m",
            "help": "How much payload mass occupies one metre of cabin length. Used only to derive the payload/systems CG "
            "position (the occupied cabin length), not the payload mass itself -- so stretching the fuselage beyond "
            "what the payload needs doesn't shift the CG aft 'for free'.",
        },
    )

    nlg_x_fraction: float = field(
        default=0.10,
        metadata={
            "label": "Nose-gear X position",
            "unit": "fraction of fuselage length",
            "help": "Nose landing gear longitudinal position, as a fraction of total fuselage length from the nose.",
        },
    )
    mlg_x_fraction_mac: float = field(
        default=0.50,
        metadata={
            "label": "Main-gear X position",
            "unit": "fraction of MAC aft of MAC LE",
            "help": "Main landing gear longitudinal position, as a fraction of the mean aerodynamic chord aft of the MAC leading edge.",
        },
    )
    pct_load_nlg_max: float = field(
        default=0.10,
        metadata={
            "label": "Max nose-gear load fraction",
            "help": "Maximum fraction of total aircraft weight the nose gear is rated to carry -- sets the 'NLG Max Strength' CG-envelope boundary.",
        },
    )
    pct_load_mlg_max: float = field(
        default=0.93,
        metadata={
            "label": "Max main-gear load fraction",
            "help": "Maximum fraction of total aircraft weight the main gear is rated to carry -- sets the 'MLG Max Strength' CG-envelope boundary.",
        },
    )
    pct_load_nlg_min: float = field(
        default=0.02,
        metadata={
            "label": "Min nose-gear load fraction",
            "help": "Minimum fraction of weight that must be on the nose gear for adequate steering authority -- "
            "sets the 'Min Nose Load' CG-envelope boundary (the aft-most safe CG at each weight).",
        },
    )
    mlw_fraction_mtow: float = field(
        default=0.92,
        metadata={
            "label": "Max landing weight fraction of MTOW",
            "help": "Maximum Landing Weight (MLW) as a fraction of MTOW, shown as a reference line on the CG envelope.",
        },
    )

    fuel_density_kg_m3: float = field(
        default=804.0,
        metadata={
            "label": "Fuel density",
            "unit": "kg/m^3",
            "help": "Jet-A/Jet-A1 density at 15C (~804 kg/m^3). Converts wing tank volume to a fuel-mass capacity "
            "for the payload-range diagram and the wing fuel-volume check.",
        },
    )
    fuel_tank_usable_fraction: float = field(
        default=0.85,
        metadata={
            "label": "Usable fuel-tank volume fraction",
            "help": "Fraction of the wing's geometric (Torenbeek) fuel volume that's actually usable tank capacity, "
            "after structure, ribs, systems and unusable-fuel allowance. Typical preliminary-design value: 0.85-0.95.",
        },
    )
