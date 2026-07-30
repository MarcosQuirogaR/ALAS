# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""Landing gear configuration -- wheel/tire sizing and strut material assumptions.

Exposed in Advanced Settings -> Landing Gear so the user can tune the sizing
without touching code. Unlike ``MassModelConfig``'s ``pct_load_nlg_max``/
``pct_load_mlg_max`` (fixed fractions of MTOW an earlier version of the CG
envelope assumed the gear could bear), this config drives a REAL wheel/tire
sizing calculation (:mod:`alas.physics.landing_gear`) that derives those
load limits from an actual number-of-wheels-and-rated-tire-load buildup, so
the CG envelope's NLG/MLG strength boundaries reflect the gear the aircraft
would actually be built with.
"""

from __future__ import annotations
from dataclasses import dataclass, field


@dataclass
class LandingGearConfig:
    """Tunable landing-gear sizing assumptions."""

    tire_safety_factor: float = field(
        default=1.07,
        metadata={
            "label": "Tire load safety factor",
            "help": "Margin applied to the static reaction load when selecting/verifying tire count -- real gear is "
            "sized so the rated tire load is never fully consumed by static load alone, leaving margin for "
            "dynamic (braking, turning, rough-field) loads. Raymer: ~1.07 typical for a preliminary sizing pass.",
        },
    )
    n_nlg_wheels: int = field(
        default=0,
        metadata={
            "label": "Nose-gear wheel count (0 = auto)",
            "help": "Wheels on the nose gear strut. 0 = auto: 1 for light aircraft, 2 (the near-universal choice for "
            "CS-25/FAR-25 transports) once MTOW exceeds nlg_dual_wheel_mtow_kg.",
        },
    )
    nlg_dual_wheel_mtow_kg: float = field(
        default=15_000.0,
        metadata={
            "label": "MTOW threshold for dual nose wheels",
            "unit": "kg",
            "help": "Auto-sizing switches from a single to a dual (twin) nose wheel above this MTOW -- below it, "
            "transport-category aircraft still commonly fly single nose wheels.",
        },
    )
    n_mlg_struts: int = field(
        default=0,
        metadata={
            "label": "Main-gear strut count (0 = auto)",
            "help": "Number of main-gear legs (each with its own wheel bogie), left+right combined. 0 = auto: 2 "
            "(one per side) below mlg_body_gear_mtow_kg, 4 (adds centreline body gear, e.g. A380/747-class) "
            "above it -- real widebodies above roughly 300 t add body gear because a two-leg bogie would need "
            "an impractically large tire count/track width to carry the load within tire-pressure limits.",
        },
    )
    mlg_body_gear_mtow_kg: float = field(
        default=300_000.0,
        metadata={
            "label": "MTOW threshold for body (centreline) main gear",
            "unit": "kg",
            "help": "Auto-sizing adds two centreline body-gear legs (4 main legs total) above this MTOW.",
        },
    )
    wheels_per_mlg_strut: int = field(
        default=0,
        metadata={
            "label": "Wheels per main-gear strut (0 = auto)",
            "help": "0 = auto: the smallest of {2, 4, 6} standard bogie sizes whose rated capacity "
            "(tire_safety_factor-derated) covers this strut's static reaction load at the aft CG limit.",
        },
    )
    track_diameter_factor: float = field(
        default=1.85,
        metadata={
            "label": "Main-gear track / fuselage-diameter factor",
            "help": "Main-gear lateral track width, as a multiple of fuselage diameter. Real transports with "
            "wing-root-mounted main gear run track/diameter ~1.75-2.0 (777-300ER 2.03, 787-9 1.90, "
            "A340-300 1.91, A380-800 2.00, A320-200 1.92, DC-10-30 1.77) -- 1.85 is the fleet-average "
            "calibration. An earlier default (1.15) understated real track width by roughly a factor of "
            "1.6, which fed directly into the lateral-turnover check (physics.landing_gear) reading "
            "artificially safe.",
        },
    )
    tire_class: str = field(
        default="auto",
        metadata={
            "label": "Tire class",
            "help": "Which reference tire (see physics.landing_gear.TIRE_DATABASE) to size with -- 'auto' picks the "
            "smallest class whose rated load, combined with a realistic wheel count (<=6/strut), covers the "
            "aircraft's static gear loads. Options: auto, light, narrowbody, widebody, heavy.",
        },
    )
    strut_material: str = field(
        default="auto",
        metadata={
            "label": "Strut material",
            "help": "Landing-gear strut/piston material, shown on the planform diagram and in the design report. "
            "'auto' selects by MTOW class (see physics.landing_gear.STRUT_MATERIALS): high-strength steel "
            "(300M-class) for larger transports, an aluminium/steel combination for light aircraft. "
            "Informational/labelling only -- this preliminary-design tool does not run a structural (FEA) "
            "stress analysis of the strut itself.",
        },
    )
    turnover_angle_limit_deg: float = field(
        default=63.0,
        metadata={
            "label": "Max lateral turnover angle",
            "unit": "deg",
            "help": "Lateral tip-over (overturn) criterion, Raymer Ch.11 / Currey convention: the angle from the "
            "vertical whose tangent is CG height over the CG's perpendicular distance to the nose-gear-to-"
            "main-gear ground line must not exceed this (evaluated at the forward CG limit, the worst case), "
            "or the aircraft risks tipping over in a tight turn. 63 deg is the standard transport-category "
            "limit; a higher CG, narrower track, or more forward CG all push the angle up toward it.",
        },
    )
