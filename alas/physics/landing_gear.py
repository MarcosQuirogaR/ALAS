# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Landing gear sizing -- wheel count, tire selection, position, and the
CS-25.147-style lateral turnover check.

Ported from first principles (Raymer Ch.11 / Torenbeek gear-load method), not
a hardcoded guess: the nose- and main-gear static reaction loads are computed
from a real two-point ground-reaction equation at the aircraft's forward and
aft AERODYNAMIC CG limits (the stability-derived envelope, computed
independently of gear -- see ``optimization.objective._check_cg_envelope``),
then enough wheels are added per strut to carry that load (with a safety
margin) using a small reference tire database. The resulting fleet-scale gear
capacity is converted back to ``pct_load_nlg_max``/``pct_load_mlg_max``
(fraction of MTOW each gear group can structurally bear) -- the same
quantities the CG envelope already uses to clip its aerodynamic limits, but
now DERIVED from an actual wheel/tire buildup instead of a fixed guess. This
is what "facilitates compliance with the CG envelope": a design is only
gear-constrained if its real wheel/tire load-bearing capacity, sized to
comfortably cover the aerodynamic envelope with margin, still falls short --
not an arbitrary fraction.

No circularity: gear sizing consumes the AERODYNAMIC limits (from NP +
static-margin target + CG range, no gear involved) as its worst-case design
loads, then hands back a strength limit that can only ever tighten (never
widen) the aerodynamic envelope -- physically correct, since a real gear
failure load is a hard floor/ceiling, not something that expands the
aerodynamically safe range.
"""

from __future__ import annotations

import math
from dataclasses import dataclass, field
from typing import List, Tuple


# ---------------------------------------------------------------------------
# Reference tire database
#
# Representative rated static loads / dimensions for common transport-category
# tire classes, at the fidelity appropriate for conceptual/preliminary design
# (not a specific certified tire p/n) -- comparable in spirit to real classes
# such as the 46x17 (narrowbody, e.g. 737/A320) through 54x21 (heavy widebody,
# e.g. 747/A380) families published by tire manufacturers (Michelin/Bridgestone/
# Goodyear) and standard gear-sizing references (Raymer Table 11.1-class data).
# ---------------------------------------------------------------------------
@dataclass
class TireSpec:
    code: str
    name: str
    rated_load_kg: float  # max rated static load per tire [kg-force]
    diameter_m: float
    width_m: float


TIRE_DATABASE = {
    "light": TireSpec("light", "Light transport (~24x7.7 class)", 3_500.0, 0.61, 0.20),
    "narrowbody": TireSpec(
        "narrowbody", "Narrowbody (~46x17 class, A320/737)", 13_000.0, 1.17, 0.44
    ),
    "widebody": TireSpec(
        "widebody", "Widebody (~52x21 class, 787/A330)", 24_000.0, 1.32, 0.53
    ),
    "heavy": TireSpec(
        "heavy", "Heavy widebody (~54x21 class, A380/747)", 34_000.0, 1.40, 0.56
    ),
}
_TIRE_ORDER = (
    "light",
    "narrowbody",
    "widebody",
    "heavy",
)  # ascending capacity, for auto-select

# Standard main-gear bogie sizes (wheels per strut) a preliminary design would
# actually choose between -- odd counts and anything above 6/strut are not
# realistic for a twin/quad-leg configuration at this design stage.
_STANDARD_BOGIE_SIZES = (2, 4, 6)

# Representative strut material by MTOW class -- informational/labelling only
# (this tool does not run a structural/FEA stress analysis of the strut).
STRUT_MATERIALS = {
    "light": "7075-T6 aluminium",
    "narrowbody": "300M high-strength steel",
    "widebody": "300M high-strength steel",
    "heavy": "300M high-strength steel (titanium truck beam)",
}


@dataclass
class Wheel:
    """One physical wheel, positioned for the planform figure."""

    x: float
    y: float
    group: str  # "NLG" or "MLG"
    strut_label: str  # e.g. "MLG-L", "MLG-R", "MLG-Body-L", "NLG"
    diameter_m: float
    width_m: float


@dataclass
class LandingGearLayout:
    """Sized landing gear: wheel counts, positions, and derived load limits."""

    n_nlg_wheels: int
    n_mlg_struts: int
    wheels_per_mlg_strut: int
    nlg_tire: TireSpec
    mlg_tire: TireSpec
    strut_material: str

    x_nlg: float
    x_mlg: float
    track_width_m: float
    wheelbase_m: float
    wheels: List[Wheel] = field(default_factory=list)

    # Design (worst-case) static reaction loads used to size the gear [kg]
    r_nlg_design_kg: float = 0.0
    r_mlg_total_design_kg: float = 0.0

    # Derived strength limits, as a fraction of MTOW -- feed directly into
    # MassModelConfig.pct_load_nlg_max/pct_load_mlg_max's role in the CG
    # envelope check.
    pct_load_nlg_max: float = 0.0
    pct_load_mlg_max: float = 0.0

    turnover_angle_deg: float = 0.0
    turnover_ok: bool = True


def _select_tire(design_load_per_wheel_kg: float, tire_class: str) -> TireSpec:
    """Pick a tire class: explicit choice, or the smallest whose rating covers the load."""
    if tire_class in TIRE_DATABASE:
        return TIRE_DATABASE[tire_class]
    for name in _TIRE_ORDER:
        spec = TIRE_DATABASE[name]
        if spec.rated_load_kg >= design_load_per_wheel_kg:
            return spec
    return TIRE_DATABASE[
        _TIRE_ORDER[-1]
    ]  # heaviest available, may still be under-rated


def _size_bogie(
    strut_load_kg: float, safety_factor: float, tire_class: str, forced_count: int = 0
) -> Tuple[int, TireSpec]:
    """Smallest standard bogie size + matching tire whose capacity covers ``strut_load_kg``.

    Self-consistent by construction: the tire is (re)selected FOR each
    candidate wheel count (the per-wheel load it would actually see), and the
    same tire is what's checked against the design load and returned -- unlike
    picking a wheel count from one tire assumption and then reselecting a
    different (weaker) tire afterward, which would silently understate the
    load each wheel actually needs to carry.
    """
    design_load = strut_load_kg * safety_factor
    counts = (forced_count,) if forced_count > 0 else _STANDARD_BOGIE_SIZES
    for n in counts:
        tire = _select_tire(strut_load_kg / n, tire_class)
        if forced_count > 0 or n * tire.rated_load_kg >= design_load:
            return n, tire
    # Nothing in the standard ladder covers it -- return the largest bogie
    # with the tire sized for its actual per-wheel load (may still be
    # under-rated; that's a legitimate finding, not silently hidden).
    n = _STANDARD_BOGIE_SIZES[-1]
    return n, _select_tire(strut_load_kg / n, tire_class)


def size_landing_gear(
    mtow_kg: float,
    x_nlg: float,
    x_mlg: float,
    aero_fwd_lim_x: float,
    aero_aft_lim_x: float,
    fuselage_diameter_m: float,
    cg_height_estimate_m: float,
    gear_config,
) -> LandingGearLayout:
    """Size the landing gear from real static reaction loads at the aerodynamic CG limits.

    Parameters
    ----------
    x_nlg, x_mlg : physical fuselage-station X [m] of the nose/main gear (from
        ``MassModelConfig.nlg_x_fraction``/``mlg_x_fraction_mac`` geometry,
        unchanged by this function).
    aero_fwd_lim_x, aero_aft_lim_x : physical X [m] of the aerodynamic
        (stability-derived, gear-independent) forward/aft CG limits -- the
        worst-case loading conditions the gear must be sized to carry.
    cg_height_estimate_m : height of the loaded CG above the ground plane,
        for the lateral turnover check (a preliminary-design estimate, not a
        precise CG-height computation).
    """
    wheelbase = max(0.5, x_mlg - x_nlg)

    # Two-point static reaction: R_nlg = W*(x_mlg - x_cg)/wheelbase, R_mlg = W - R_nlg.
    # Max NLG load occurs at the FORWARD CG limit (x_cg small -> R_nlg large);
    # max total MLG load occurs at the AFT CG limit (x_cg large -> R_nlg small).
    def r_nlg(x_cg: float) -> float:
        return mtow_kg * (x_mlg - x_cg) / wheelbase

    r_nlg_design = max(0.0, r_nlg(aero_fwd_lim_x))
    r_mlg_total_design = max(0.0, mtow_kg - r_nlg(aero_aft_lim_x))

    # -- Nose gear --------------------------------------------------------
    n_nlg_wheels = gear_config.n_nlg_wheels or (
        2 if mtow_kg >= gear_config.nlg_dual_wheel_mtow_kg else 1
    )
    nlg_load_per_wheel = r_nlg_design / max(1, n_nlg_wheels)
    nlg_tire = _select_tire(nlg_load_per_wheel, gear_config.tire_class)

    # -- Main gear --------------------------------------------------------
    n_mlg_struts = gear_config.n_mlg_struts or (
        4 if mtow_kg >= gear_config.mlg_body_gear_mtow_kg else 2
    )
    load_per_strut = r_mlg_total_design / max(1, n_mlg_struts)
    wheels_per_strut, mlg_tire = _size_bogie(
        load_per_strut,
        gear_config.tire_safety_factor,
        gear_config.tire_class,
        forced_count=gear_config.wheels_per_mlg_strut,
    )

    # -- Derived strength limits (fraction of MTOW) ------------------------
    nlg_capacity_kg = n_nlg_wheels * nlg_tire.rated_load_kg
    mlg_capacity_kg = n_mlg_struts * wheels_per_strut * mlg_tire.rated_load_kg
    pct_load_nlg_max = nlg_capacity_kg / max(1.0, mtow_kg)
    pct_load_mlg_max = mlg_capacity_kg / max(1.0, mtow_kg)

    # -- Strut material label ----------------------------------------------
    if gear_config.strut_material != "auto":
        strut_material = gear_config.strut_material
    else:
        strut_material = STRUT_MATERIALS.get(
            mlg_tire.code, STRUT_MATERIALS["narrowbody"]
        )

    # -- Lateral track width: main gear sits outboard of the fuselage, wide
    # enough to clear it with a realistic clearance margin. Calibrated
    # against real transports (LandingGearConfig.track_diameter_factor,
    # default 1.85 -- a previous default of 1.15 underestimated real track
    # width by ~1.6x, e.g. a 787-9-scale fuselage computed a 7.3 m track
    # against a real ~11.3 m, which fed directly into the turnover check
    # below reading artificially safe). The wheels_per_strut term is a small
    # additive allowance for the bogie's own footprint width.
    track_width_m = (
        fuselage_diameter_m * gear_config.track_diameter_factor
        + wheels_per_strut * 0.05
    )

    # -- Lateral turnover angle (Raymer Ch.11 / Currey overturn criterion).
    # The tip-over axis is the ground line from the nose-gear contact point to
    # a main-gear contact point; in plan view it makes angle delta with the
    # centerline (tan(delta) = half_track / wheelbase). The lateral lever arm
    # is the perpendicular distance from the CG to that axis, l_n*sin(delta)
    # with l_n the CG's distance aft of the nose gear -- smallest (worst case)
    # at the FORWARD CG limit. The overturn angle is measured from the
    # vertical: tan(theta) = h_cg / lever, so a higher CG, a narrower track,
    # or a more forward CG all RAISE theta toward the tip-over limit
    # (<= ~63 deg). An earlier version computed atan(half_track / h_cg) --
    # the complement -- which shrank as the CG rose or the track narrowed,
    # so the check passed exactly the tippy designs it was meant to flag.
    half_track = track_width_m / 2.0
    delta = math.atan2(half_track, wheelbase)
    l_n_fwd = max(0.1, aero_fwd_lim_x - x_nlg)
    lever = max(1e-3, l_n_fwd * math.sin(delta))
    turnover_angle_deg = math.degrees(math.atan2(max(0.1, cg_height_estimate_m), lever))
    turnover_ok = turnover_angle_deg <= gear_config.turnover_angle_limit_deg

    # -- Wheel positions for the planform figure -----------------------------
    wheels: List[Wheel] = []
    nlg_spacing = nlg_tire.width_m * 1.6
    for i in range(n_nlg_wheels):
        y = (i - (n_nlg_wheels - 1) / 2.0) * nlg_spacing
        wheels.append(
            Wheel(
                x=x_nlg,
                y=y,
                group="NLG",
                strut_label="NLG",
                diameter_m=nlg_tire.diameter_m,
                width_m=nlg_tire.width_m,
            )
        )

    strut_sides: List[Tuple[str, float]] = [("L", -half_track), ("R", +half_track)]
    if n_mlg_struts >= 4:
        body_offset = half_track * 0.45
        strut_sides += [("Body-L", -body_offset), ("Body-R", +body_offset)]
    strut_sides = strut_sides[: max(2, n_mlg_struts)]

    bogie_spacing = mlg_tire.width_m * 1.6
    for label, y_center in strut_sides:
        for i in range(wheels_per_strut):
            # Even wheel counts pair up fore/aft in a bogie; odd (shouldn't
            # occur given _STANDARD_BOGIE_SIZES) centres the extra wheel.
            row = i // 2 if wheels_per_strut > 1 else 0
            side = -1 if i % 2 == 0 else 1
            y = (
                y_center + side * bogie_spacing / 2.0
                if wheels_per_strut > 1
                else y_center
            )
            x = x_mlg + (row - (math.ceil(wheels_per_strut / 2) - 1) / 2.0) * (
                mlg_tire.diameter_m * 1.3
            )
            wheels.append(
                Wheel(
                    x=x,
                    y=y,
                    group="MLG",
                    strut_label=f"MLG-{label}",
                    diameter_m=mlg_tire.diameter_m,
                    width_m=mlg_tire.width_m,
                )
            )

    return LandingGearLayout(
        n_nlg_wheels=n_nlg_wheels,
        n_mlg_struts=n_mlg_struts,
        wheels_per_mlg_strut=wheels_per_strut,
        nlg_tire=nlg_tire,
        mlg_tire=mlg_tire,
        strut_material=strut_material,
        x_nlg=x_nlg,
        x_mlg=x_mlg,
        track_width_m=track_width_m,
        wheelbase_m=wheelbase,
        wheels=wheels,
        r_nlg_design_kg=r_nlg_design,
        r_mlg_total_design_kg=r_mlg_total_design,
        pct_load_nlg_max=pct_load_nlg_max,
        pct_load_mlg_max=pct_load_mlg_max,
        turnover_angle_deg=turnover_angle_deg,
        turnover_ok=turnover_ok,
    )
