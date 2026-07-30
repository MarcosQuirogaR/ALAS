# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""Performance analysis configuration -- all values exposed in Advanced Settings."""

from __future__ import annotations
from dataclasses import dataclass, field


@dataclass
class PerformanceConfig:
    """High-lift and field performance parameters for matching chart analysis.

    Defaults from Raymer (5th ed.) and FAR Part 25.
    All values are exposed in Advanced Settings -> Performance so the user
    can tune them without touching code. The "Performance preset" selector on
    that tab bundles the physical/empirical assumption fields below (CLmax,
    thrust lapse, OEI climb factors, BFL factor) into named profiles -- see
    :mod:`alas.config.performance_presets`.
    """

    cl_max_to: float = field(
        default=1.80,
        metadata={
            "label": "Max lift coefficient, take-off (CLmax_TO)",
            "help": "Maximum lift coefficient achievable in the take-off flap/slat configuration. "
            "Drives take-off field length via the matching chart.",
        },
    )
    cl_max_land: float = field(
        default=2.60,
        metadata={
            "label": "Max lift coefficient, landing (CLmax_L)",
            "help": "Maximum lift coefficient achievable in the landing flap/slat configuration. Drives landing distance.",
        },
    )
    cl_max_clean: float = field(
        default=1.50,
        metadata={
            "label": "Max lift coefficient, clean (CLmax_clean)",
            "help": "Maximum lift coefficient in clean (flaps/slats up) configuration -- the true aerodynamic stall "
            "limit used for the V-n diagram's stall boundary, distinct from the flaps-down CLmax_TO/CLmax_L above.",
        },
    )
    cl_min_clean: float = field(
        default=-1.00,
        metadata={
            "label": "Min lift coefficient, clean (CLmin_clean)",
            "help": "Most negative (inverted-flight) lift coefficient in clean configuration -- the negative stall "
            "boundary on the V-n diagram.",
        },
    )
    thrust_lapse: float = field(
        default=0.235,
        metadata={
            "label": "Thrust lapse ratio",
            "help": "Fraction by which static sea-level thrust falls off during the take-off ground roll / initial climb, "
            "used in the matching-chart take-off constraint.",
        },
    )
    oei_gradient: float = field(
        default=0.024,
        metadata={
            "label": "OEI 2nd-segment climb gradient (fallback)",
            "unit": "fraction",
            "help": "FAR 25.121 minimum second-segment climb gradient with one engine inoperative (OEI). The matching "
            "chart auto-selects 0.024 (twin) / 0.027 (tri-jet) / 0.030 (quad) from the actual engine count; "
            "this value is only the fallback for any other engine count.",
        },
    )
    k_land: float = field(
        default=0.60,
        metadata={
            "label": "Landing distance factor (k_land)",
            "help": "Empirical constant relating approach speed / wing loading to landing ground-roll + air distance.",
        },
    )
    oei_climb_cl: float = field(
        default=1.2,
        metadata={
            "label": "OEI climb configuration CL",
            "help": "Lift coefficient assumed in the take-off configuration when evaluating OEI second-segment climb L/D (Raymer Ch.17).",
        },
    )
    oei_climb_delta_cd: float = field(
        default=0.025,
        metadata={
            "label": "OEI climb flap/gear drag increment",
            "help": "Parasite-drag increment added to the clean CD0 for the flap/gear-down OEI second-segment climb configuration.",
        },
    )

    ws_min_pa: float = field(
        default=2000.0,
        metadata={
            "label": "Matching chart wing-loading axis: minimum",
            "unit": "Pa",
            "help": "Lower wing-loading (W/S) axis limit on the matching chart plot. Widen for very light (GA) aircraft. (~204 kg/m^2)",
        },
    )
    ws_max_pa: float = field(
        default=10000.0,
        metadata={
            "label": "Matching chart wing-loading axis: maximum",
            "unit": "Pa",
            "help": "Upper wing-loading (W/S) axis limit on the matching chart plot. Widen for very heavy (freighter) aircraft. (~1020 kg/m^2)",
        },
    )

    bfl_factor: float = field(
        default=1.15,
        metadata={
            "label": "Balanced field length factor",
            "help": "BFL = bfl_factor x TODR (take-off distance required). Raymer Table 17.1: 1.15 for twin jets, ~1.18 for quads.",
        },
    )

    # -- FAR-25 V-speed factors (multiples of the take-off stall speed VS_TO,
    #    except V1 which is a fraction of VR and VAPP/VTD which are on VS_land).
    #    Exposed here per the project's no-hardcoded-values principle so they
    #    can be calibrated to a real type.
    vmc_vstall_factor: float = field(
        default=1.13,
        metadata={
            "label": "VMC / VS_TO",
            "help": "Minimum-control speed as a multiple of take-off stall speed. "
            "FAR 25.149 caps VMC at 1.13*VSR; that ceiling is used as the default.",
        },
    )
    vr_vmc_factor: float = field(
        default=1.05,
        metadata={
            "label": "VR / VMC floor",
            "help": "FAR 25.107: rotation speed must be at least 1.05*VMC.",
        },
    )
    vr_vstall_factor: float = field(
        default=1.10,
        metadata={
            "label": "VR / VS_TO floor",
            "help": "FAR 25.107: rotation speed must also be at least 1.10*VS_TO. "
            "VR = max(vr_vmc_factor*VMC, vr_vstall_factor*VS_TO).",
        },
    )
    v2_vstall_factor: float = field(
        default=1.20,
        metadata={
            "label": "V2 / VS_TO",
            "help": "Take-off safety speed as a multiple of take-off stall speed (FAR 25.107). "
            "V2 = max(v2_vstall_factor*VS_TO, VR).",
        },
    )
    v1_vr_factor: float = field(
        default=0.95,
        metadata={
            "label": "V1 / VR",
            "help": "Decision speed as a fraction of rotation speed. On a dry balanced field V1 sits "
            "just below VR (~0.95-0.98); the earlier 0.90 default put V1 unrealistically far below VR (e.g. ~152 kt "
            "against a real 787-9 V1 of 160-165 kt).",
        },
    )
    vapp_vstall_land_factor: float = field(
        default=1.30,
        metadata={
            "label": "VAPP / VS_land",
            "help": "Approach speed as a multiple of landing stall speed (FAR 25.125).",
        },
    )
    vtd_vstall_land_factor: float = field(
        default=1.15,
        metadata={
            "label": "VTD / VS_land",
            "help": "Touchdown speed as a multiple of landing stall speed.",
        },
    )

    matching_chart_resolution: int = field(
        default=150,
        metadata={
            "label": "Matching chart plot resolution",
            "help": "Number of wing-loading points swept when drawing the matching-chart constraint curves "
            "(Results -> Matching Chart). Purely a plotting resolution knob -- higher gives smoother curves "
            "at extra compute cost. Not part of the Performance preset (it isn't a physical assumption).",
        },
    )
