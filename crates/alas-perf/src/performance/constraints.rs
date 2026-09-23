// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/performance.py
// Reference: alas @ rust-port-baseline.

//! The four matching-chart constraint curves and the assembled chart.

use alas_atmo::Atmosphere;
use alas_config::airports::Airport;
use alas_config::PerformanceConfig;

use super::{far25_engine_count_supported, linspace, M_TO_FT, PA_TO_PSF};

/// Cruise thrust-to-weight constraint, sea-level static: `tw_cruise_constraint`.
///
/// Solves level flight (`L = W`, `T = D`) at the cruise design point and
/// converts the required altitude `T/W` to sea-level static through a fixed
/// thrust lapse `eta`:
///
/// ```text
/// T/W0 = [q*CD0/(W/S) + k*(W/S)/q] / eta
/// ```
///
/// Evaluated for every wing loading in `ws_pa` (Pa); returns one `T/W0` each.
pub fn tw_cruise_constraint(
    ws_pa: &[f64],
    cd0: f64,
    k: f64,
    cruise_mach: f64,
    cruise_altitude_m: f64,
    thrust_lapse: f64,
) -> Vec<f64> {
    let atmo = Atmosphere::new(cruise_altitude_m);
    let speed = cruise_mach * atmo.speed_of_sound();
    let q = 0.5 * atmo.density() * speed * speed;
    ws_pa
        .iter()
        .map(|&ws| (q * cd0 / ws + k * ws / q) / thrust_lapse)
        .collect()
}

/// Engine-out second-segment climb thrust-to-weight, constant in wing loading:
/// `tw_oei_climb_constraint`.
///
/// This compatibility function returns the required *in-flight* all-engine
/// equivalent `T/W` at the specified condition. It does not convert from the
/// thrust available at `V2` and field altitude/temperature to sea-level static
/// thrust. Use [`tw_oei_climb_constraint_at_v2`] when the result is compared
/// with an SLS-rated installed thrust-to-weight.
///
/// FAR 25.121(b) evaluates the second segment at `V2` with the landing gear
/// retracted. `delta_cd_to_config` therefore represents the takeoff
/// high-lift increment with the gear retracted. Asymmetric trim/control drag
/// and inoperative-engine (for example, windmilling-propeller) drag are
/// separate effects and must not be silently folded into this scalar unless
/// their source values are available.
///
/// A single-engine aircraft is not subject to the rule and returns `0.0`.
pub fn tw_oei_climb_constraint(
    cd0: f64,
    k: f64,
    n_engines: i64,
    oei_gradient: f64,
    cl_climb: f64,
    delta_cd_to_config: f64,
) -> f64 {
    if n_engines < 2 {
        return 0.0;
    }
    let cd_to_config = cd0 + delta_cd_to_config + k * cl_climb * cl_climb;
    let ld_to = cl_climb / cd_to_config;
    let factor = n_engines as f64 / (n_engines - 1) as f64;
    factor * (1.0 / ld_to + oei_gradient)
}

/// Additional drag terms for an engine-out climb calculation.
///
/// `high_lift_gear_up_cd` is the takeoff flap/slat increment with the landing
/// gear retracted, matching the configuration in FAR 25.121(b). The two
/// `Option` fields make missing aircraft-specific evidence explicit: a caller
/// must not turn an unknown asymmetric trim or failed-engine drag into a
/// certification-looking zero. A known negligible contribution may be passed
/// as `Some(0.0)` with that assumption recorded by the caller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OeiDragIncrements {
    /// Takeoff high-lift parasite-drag increment, gear retracted (delta CD).
    pub high_lift_gear_up_cd: f64,
    /// Additional asymmetric rudder/aileron trim and control drag (delta CD).
    pub asymmetric_trim_cd: Option<f64>,
    /// Additional drag from the inoperative engine or propeller (delta CD).
    pub windmilling_cd: Option<f64>,
}

impl OeiDragIncrements {
    /// Construct a complete drag record. Use `Some(0.0)` only when the zero
    /// contribution is an intentional, documented modelling assumption.
    pub const fn new(
        high_lift_gear_up_cd: f64,
        asymmetric_trim_cd: Option<f64>,
        windmilling_cd: Option<f64>,
    ) -> Self {
        Self {
            high_lift_gear_up_cd,
            asymmetric_trim_cd,
            windmilling_cd,
        }
    }

    /// Sum the three drag increments when every required contribution is
    /// known and non-negative.
    pub fn total_cd(self) -> Option<f64> {
        let trim = self.asymmetric_trim_cd?;
        let windmilling = self.windmilling_cd?;
        if !self.high_lift_gear_up_cd.is_finite()
            || self.high_lift_gear_up_cd < 0.0
            || !trim.is_finite()
            || trim < 0.0
            || !windmilling.is_finite()
            || windmilling < 0.0
        {
            return None;
        }
        let total = self.high_lift_gear_up_cd + trim + windmilling;
        total.is_finite().then_some(total)
    }
}

/// Lift coefficient at a selected `V2/VSR` or `V2/VS` speed ratio.
///
/// This is the level-flight schedule `CL = CLmax_TO / (V2/Vs)^2`, provided
/// the supplied stall-speed reference and `CLmax_TO` describe the same
/// takeoff configuration. The regulatory speed selection also includes VMC,
/// rotation, manoeuvring, and takeoff-path conditions (14 CFR 25.107(c)); those
/// floors must be resolved by the caller before passing the selected ratio.
/// A ratio at or below one is outside the schedule and returns `None`.
pub fn oei_cl_at_v2(cl_max_to: f64, v2_over_vstall: f64) -> Option<f64> {
    if !cl_max_to.is_finite()
        || cl_max_to <= 0.0
        || !v2_over_vstall.is_finite()
        || v2_over_vstall <= 1.0
    {
        return None;
    }
    Some(cl_max_to / (v2_over_vstall * v2_over_vstall))
}

/// Convert a required in-flight engine-equivalent `T/W` to the equivalent
/// sea-level-static installed `T/W`.
///
/// `condition_to_sls_thrust_ratio` is the ratio of available installed thrust
/// at the actual certification condition and engine setting (for example, at
/// `V2`, field altitude and temperature, with the remaining engines at
/// takeoff thrust) to the rated installed SLS thrust. It must come from an
/// engine deck, approved performance data, or an explicitly bounded model;
/// this helper does not invent an altitude/Mach thrust lapse.
pub fn sls_tw_for_oei_condition(
    required_inflight_tw: f64,
    condition_to_sls_thrust_ratio: f64,
) -> Option<f64> {
    if !required_inflight_tw.is_finite()
        || required_inflight_tw < 0.0
        || !condition_to_sls_thrust_ratio.is_finite()
        || condition_to_sls_thrust_ratio <= 0.0
    {
        return None;
    }
    Some(required_inflight_tw / condition_to_sls_thrust_ratio)
}

/// Required SLS installed thrust-to-weight for the FAR 25.121(b) condition at
/// the selected `V2` speed ratio.
///
/// Unlike the closed-form [`tw_oei_climb_constraint`], whose golden values it
/// leaves unchanged, every physical input of the condition is explicit here:
///
/// * only the two/three/four-engine Part 25 schedule is accepted;
/// * `CL` is derived from the selected `V2/Vs` ratio;
/// * gear-up high-lift, asymmetric trim/control, and failed-engine drag are
///   separate terms, with missing terms returning `None`; and
/// * the in-flight requirement is converted to SLS `T/W` using a supplied
///   condition-to-SLS thrust ratio.
///
/// `None` means that the requested engine count or one of the physical inputs
/// is outside this helper's documented domain. It is an unsupported/evidence
/// gap for a caller, not a passing zero.
#[allow(clippy::too_many_arguments)] // one per FAR 25.121(b) physical input
pub fn tw_oei_climb_constraint_at_v2(
    cd0: f64,
    k: f64,
    n_engines: i64,
    oei_gradient: f64,
    cl_max_to: f64,
    v2_over_vstall: f64,
    drag: OeiDragIncrements,
    condition_to_sls_thrust_ratio: f64,
) -> Option<f64> {
    let required_inflight_tw = oei_inflight_tw_at_v2(
        cd0,
        k,
        n_engines,
        oei_gradient,
        cl_max_to,
        v2_over_vstall,
        drag,
    )?;
    sls_tw_for_oei_condition(required_inflight_tw, condition_to_sls_thrust_ratio)
}

fn oei_inflight_tw_at_v2(
    cd0: f64,
    k: f64,
    n_engines: i64,
    oei_gradient: f64,
    cl_max_to: f64,
    v2_over_vstall: f64,
    drag: OeiDragIncrements,
) -> Option<f64> {
    if !far25_engine_count_supported(n_engines)
        || !cd0.is_finite()
        || cd0 < 0.0
        || !k.is_finite()
        || k < 0.0
        || !oei_gradient.is_finite()
        || oei_gradient < 0.0
    {
        return None;
    }
    let cl_v2 = oei_cl_at_v2(cl_max_to, v2_over_vstall)?;
    let delta_cd = drag.total_cd()?;
    let required_inflight_tw =
        tw_oei_climb_constraint(cd0, k, n_engines, oei_gradient, cl_v2, delta_cd);
    (required_inflight_tw.is_finite() && required_inflight_tw >= 0.0)
        .then_some(required_inflight_tw)
}

/// Whether a transport-category engine-out climb assessment has the physical
/// evidence needed to be compared with a sea-level-static thrust-to-weight.
///
/// `SlsEquivalent` means that the selected departure/V2 condition, drag terms,
/// and condition-to-SLS thrust ratio were supplied and passed this helper's
/// dimensional/domain checks. It is an evidence-backed calculation, not a
/// certification or regulatory-compliance finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OeiClimbStatus {
    /// The Part 25 engine-out climb rule is outside scope below two engines.
    NotApplicable,
    /// All required departure/V2 evidence was supplied and is usable.
    SlsEquivalent,
    /// A legacy in-flight estimate is available, but no condition conversion
    /// evidence was supplied; the value is diagnostic only.
    ConceptualInflight,
    /// A condition was requested but one or more required inputs are missing
    /// or invalid, so no SLS floor can be scored.
    EvidenceGap,
    /// The engine count is outside the two/three/four-engine schedule.
    UnsupportedEngineCount,
}

/// Departure and V2 evidence needed to express an OEI requirement on an SLS
/// installed-thrust axis.
///
/// The altitude and ISA deviation establish that a real departure condition is
/// known; they are metadata here because the thrust ratio is expected to come
/// from a condition-specific engine deck or other documented source. A ratio
/// by itself, without this condition context, is not sufficient evidence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OeiV2Condition {
    /// Departure field elevation [m].
    pub departure_elevation_m: f64,
    /// Departure ISA temperature deviation [deg C].
    pub departure_isa_deviation_c: f64,
    /// Selected take-off safety speed divided by the take-off stall-speed
    /// reference [-].
    pub v2_over_vstall: f64,
    /// Available installed thrust at the condition divided by rated installed
    /// sea-level-static thrust [-].
    pub condition_to_sls_thrust_ratio: Option<f64>,
    /// Additional asymmetric trim/control drag increment [CD].
    pub asymmetric_trim_cd: Option<f64>,
    /// Inoperative-engine or propeller drag increment [CD].
    pub windmilling_cd: Option<f64>,
}

/// Assessed OEI climb requirement and its evidence status.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OeiClimbAssessment {
    /// Evidence status governing whether `required_sls_tw` is scoreable.
    pub status: OeiClimbStatus,
    /// Required in-flight equivalent `T/W`, when the aerodynamic inputs are
    /// usable. This is diagnostic unless `status` is `SlsEquivalent`.
    pub required_inflight_tw: Option<f64>,
    /// Required installed SLS `T/W`, only when a complete condition-specific
    /// evidence record is available.
    pub required_sls_tw: Option<f64>,
    /// Short user-facing explanation of the status and scoring consequence.
    pub diagnostic: &'static str,
}

/// Assess the engine-out second-segment requirement for charting and scoring.
///
/// The legacy scalar inputs (`oei_climb_cl` and `oei_climb_delta_cd`) produce a
/// useful in-flight estimate for compatibility. Passing `Some(condition)` is
/// required before this function can produce `required_sls_tw`: the departure
/// context, selected V2 ratio, condition-specific thrust ratio, and both
/// additional drag terms must all be finite and physically bounded. No generic
/// thrust-lapse setting is used as a substitute for that evidence.
#[allow(clippy::too_many_arguments)] // one per FAR 25.121(b) physical input
pub fn assess_oei_climb(
    cd0: f64,
    k: f64,
    n_engines: i64,
    oei_gradient: f64,
    oei_climb_cl: f64,
    oei_climb_delta_cd: f64,
    cl_max_to: f64,
    condition: Option<OeiV2Condition>,
) -> OeiClimbAssessment {
    if n_engines < 2 {
        return OeiClimbAssessment {
            status: OeiClimbStatus::NotApplicable,
            required_inflight_tw: None,
            required_sls_tw: None,
            diagnostic: "OEI climb not applicable below two engines; no floor scored.",
        };
    }
    if !far25_engine_count_supported(n_engines) {
        return OeiClimbAssessment {
            status: OeiClimbStatus::UnsupportedEngineCount,
            required_inflight_tw: None,
            required_sls_tw: None,
            diagnostic: "OEI climb engine count is outside the supported 2/3/4-engine schedule.",
        };
    }

    let legacy_is_valid = cd0.is_finite()
        && cd0 >= 0.0
        && k.is_finite()
        && k >= 0.0
        && oei_gradient.is_finite()
        && oei_gradient >= 0.0
        && oei_climb_cl.is_finite()
        && oei_climb_cl > 0.0
        && oei_climb_delta_cd.is_finite()
        && oei_climb_delta_cd >= 0.0;
    let legacy_inflight_tw = legacy_is_valid.then(|| {
        tw_oei_climb_constraint(
            cd0,
            k,
            n_engines,
            oei_gradient,
            oei_climb_cl,
            oei_climb_delta_cd,
        )
    });
    let legacy_inflight_tw = legacy_inflight_tw.filter(|value| value.is_finite() && *value >= 0.0);

    let Some(condition) = condition else {
        return OeiClimbAssessment {
            status: if legacy_inflight_tw.is_some() {
                OeiClimbStatus::ConceptualInflight
            } else {
                OeiClimbStatus::EvidenceGap
            },
            required_inflight_tw: legacy_inflight_tw,
            required_sls_tw: None,
            diagnostic: if legacy_inflight_tw.is_some() {
                "OEI climb is a conceptual in-flight estimate; departure/V2 SLS evidence is absent, so no SLS floor is scored."
            } else {
                "OEI climb inputs are invalid; no in-flight or SLS requirement is scored."
            },
        };
    };

    let condition_is_finite = condition.departure_elevation_m.is_finite()
        && condition.departure_isa_deviation_c.is_finite()
        && condition.v2_over_vstall.is_finite()
        && condition.v2_over_vstall > 1.0;
    let Some(condition_to_sls_thrust_ratio) = condition.condition_to_sls_thrust_ratio else {
        return OeiClimbAssessment {
            status: OeiClimbStatus::EvidenceGap,
            required_inflight_tw: legacy_inflight_tw,
            required_sls_tw: None,
            diagnostic: "OEI SLS evidence gap: condition-to-SLS thrust ratio is missing; no SLS floor is scored.",
        };
    };
    let Some(asymmetric_trim_cd) = condition.asymmetric_trim_cd else {
        return OeiClimbAssessment {
            status: OeiClimbStatus::EvidenceGap,
            required_inflight_tw: legacy_inflight_tw,
            required_sls_tw: None,
            diagnostic:
                "OEI SLS evidence gap: asymmetric trim drag is missing; no SLS floor is scored.",
        };
    };
    let Some(windmilling_cd) = condition.windmilling_cd else {
        return OeiClimbAssessment {
            status: OeiClimbStatus::EvidenceGap,
            required_inflight_tw: legacy_inflight_tw,
            required_sls_tw: None,
            diagnostic:
                "OEI SLS evidence gap: failed-engine drag is missing; no SLS floor is scored.",
        };
    };
    if !condition_is_finite
        || !condition_to_sls_thrust_ratio.is_finite()
        || condition_to_sls_thrust_ratio <= 0.0
        || !asymmetric_trim_cd.is_finite()
        || asymmetric_trim_cd < 0.0
        || !windmilling_cd.is_finite()
        || windmilling_cd < 0.0
    {
        return OeiClimbAssessment {
            status: OeiClimbStatus::EvidenceGap,
            required_inflight_tw: legacy_inflight_tw,
            required_sls_tw: None,
            diagnostic: "OEI SLS evidence gap: departure/V2, thrust-ratio, or drag evidence is invalid; no SLS floor is scored.",
        };
    }

    let drag = OeiDragIncrements::new(
        oei_climb_delta_cd,
        Some(asymmetric_trim_cd),
        Some(windmilling_cd),
    );
    let required_inflight_tw = oei_inflight_tw_at_v2(
        cd0,
        k,
        n_engines,
        oei_gradient,
        cl_max_to,
        condition.v2_over_vstall,
        drag,
    );
    let required_sls_tw = required_inflight_tw
        .and_then(|value| sls_tw_for_oei_condition(value, condition_to_sls_thrust_ratio));
    match (required_inflight_tw, required_sls_tw) {
        (Some(required_inflight_tw), Some(required_sls_tw)) => OeiClimbAssessment {
            status: OeiClimbStatus::SlsEquivalent,
            required_inflight_tw: Some(required_inflight_tw),
            required_sls_tw: Some(required_sls_tw),
            diagnostic: "OEI SLS-equivalent requirement uses supplied departure/V2 evidence; this calculation is not a certification finding.",
        },
        _ => OeiClimbAssessment {
            status: OeiClimbStatus::EvidenceGap,
            required_inflight_tw,
            required_sls_tw: None,
            diagnostic: "OEI SLS evidence is incomplete or outside the supported domain; no SLS floor is scored.",
        },
    }
}

/// Take-off field-length thrust-to-weight constraint: `tw_takeoff_constraint`
/// (Raymer Ch. 17, empirical).
///
/// ```text
/// T/W = 37.7 * (W/S [psf]) / (sigma * CL_max_TO * TODA [ft])
/// ```
///
/// The `37.7` is Raymer's regression coefficient for jet transports. Evaluated
/// for every wing loading in `ws_pa` (Pa) against one take-off distance
/// available `toda_m` and density ratio `sigma`.
pub fn tw_takeoff_constraint(ws_pa: &[f64], toda_m: f64, sigma: f64, cl_max_to: f64) -> Vec<f64> {
    let toda_ft = toda_m * M_TO_FT;
    ws_pa
        .iter()
        .map(|&ws| 37.7 * (ws * PA_TO_PSF) / (sigma * cl_max_to * toda_ft))
        .collect()
}

/// Maximum wing loading [Pa] the landing-distance constraint allows:
/// `ws_landing_limit`.
///
/// ```text
/// (W/S)_max = LDA * sigma * CL_max_land / K
/// ```
pub fn ws_landing_limit(lda_m: f64, sigma: f64, cl_max_land: f64, k_factor: f64) -> f64 {
    lda_m * sigma * cl_max_land / k_factor
}

/// Pre-computed constraint curves ready for plotting: `MatchingChartData`.
///
/// `tw_takeoff` and `ws_land_limits` are keyed by aerodrome name in the order
/// the aerodromes were supplied (upstream's insertion-ordered `dict`); a
/// `Vec` of pairs keeps that order without an ordered-map dependency.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchingChartData {
    /// Wing-loading axis, Pa.
    pub ws_pa: Vec<f64>,
    /// Cruise `T/W0` curve.
    pub tw_cruise: Vec<f64>,
    /// Engine-out climb `T/W0` (constant).
    pub tw_oei_climb: f64,
    /// Evidence-gated engine-out assessment. The legacy scalar above remains
    /// available for parity/diagnostics; consumers must use this assessment
    /// to decide whether an SLS floor is scoreable.
    pub oei_climb_assessment: OeiClimbAssessment,
    /// Take-off `T/W0` curve per aerodrome, in supplied order.
    pub tw_takeoff: Vec<(String, Vec<f64>)>,
    /// Maximum `W/S` [Pa] per aerodrome, in supplied order.
    pub ws_land_limits: Vec<(String, f64)>,
    /// Design wing loading [Pa], if the aircraft weight and area are known.
    pub design_ws_pa: Option<f64>,
    /// Design `T/W0`, if supplied.
    pub design_tw: Option<f64>,
}

/// Assemble every matching-chart constraint curve for a set of aerodromes:
/// `build_matching_chart`.
///
/// Each `Option` argument falls back to a fresh [`PerformanceConfig`]'s field
/// of the same name, exactly as upstream's `None`-defaulted keywords do, one
/// place the defaults live, so an omitted argument cannot drift from the
/// configuration. `tw_design` alone has no configuration counterpart and is
/// passed straight through. `n_ws_points` sets the wing-loading resolution;
/// `ws_min_pa`/`ws_max_pa` set its range.
#[allow(clippy::too_many_arguments)] // mirrors upstream's own keyword signature
pub fn build_matching_chart(
    cd0: f64,
    k: f64,
    cruise_mach: f64,
    cruise_altitude_m: f64,
    mtow_kg: f64,
    wing_area_m2: f64,
    n_engines: i64,
    airports: &[Airport],
    cl_max_to: Option<f64>,
    cl_max_land: Option<f64>,
    thrust_lapse: Option<f64>,
    oei_gradient: Option<f64>,
    k_land: Option<f64>,
    oei_climb_cl: Option<f64>,
    oei_climb_delta_cd: Option<f64>,
    tw_design: Option<f64>,
    n_ws_points: i64,
    ws_min_pa: Option<f64>,
    ws_max_pa: Option<f64>,
) -> MatchingChartData {
    let defaults = PerformanceConfig::default();
    let cl_max_to = cl_max_to.unwrap_or(defaults.cl_max_to);
    let cl_max_land = cl_max_land.unwrap_or(defaults.cl_max_land);
    let thrust_lapse = thrust_lapse.unwrap_or(defaults.thrust_lapse);
    let oei_gradient = oei_gradient.unwrap_or(defaults.oei_gradient);
    let k_land = k_land.unwrap_or(defaults.k_land);
    let oei_climb_cl = oei_climb_cl.unwrap_or(defaults.oei_climb_cl);
    let oei_climb_delta_cd = oei_climb_delta_cd.unwrap_or(defaults.oei_climb_delta_cd);
    let ws_min_pa = ws_min_pa.unwrap_or(defaults.ws_min_pa);
    let ws_max_pa = ws_max_pa.unwrap_or(defaults.ws_max_pa);

    let ws_pa = linspace(ws_min_pa, ws_max_pa, n_ws_points);

    let tw_cruise =
        tw_cruise_constraint(&ws_pa, cd0, k, cruise_mach, cruise_altitude_m, thrust_lapse);
    let tw_oei_climb = tw_oei_climb_constraint(
        cd0,
        k,
        n_engines,
        oei_gradient,
        oei_climb_cl,
        oei_climb_delta_cd,
    );

    let mut tw_takeoff: Vec<(String, Vec<f64>)> = Vec::with_capacity(airports.len());
    let mut ws_land_limits: Vec<(String, f64)> = Vec::with_capacity(airports.len());
    for airport in airports {
        let sigma = super::density_ratio(airport.elevation_m, airport.isa_deviation_c);
        tw_takeoff.push((
            airport.name.clone(),
            tw_takeoff_constraint(&ws_pa, airport.toda_m, sigma, cl_max_to),
        ));
        ws_land_limits.push((
            airport.name.clone(),
            ws_landing_limit(airport.lda_m, sigma, cl_max_land, k_land),
        ));
    }

    // Upstream's `mtow_kg and wing_area_m2` is a truthiness test: a zero on
    // either side leaves the design point undefined rather than dividing.
    let design_ws_pa = if mtow_kg != 0.0 && wing_area_m2 != 0.0 {
        Some(mtow_kg * super::G / wing_area_m2)
    } else {
        None
    };

    MatchingChartData {
        ws_pa,
        tw_cruise,
        tw_oei_climb,
        oei_climb_assessment: assess_oei_climb(
            cd0,
            k,
            n_engines,
            oei_gradient,
            oei_climb_cl,
            oei_climb_delta_cd,
            cl_max_to,
            None,
        ),
        tw_takeoff,
        ws_land_limits,
        design_ws_pa,
        design_tw: tw_design,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_single_engine_aircraft_has_no_oei_climb_constraint() {
        // FAR 25.121 does not apply below two engines, so the curve collapses
        // to zero rather than dividing by `N - 1 = 0`.
        assert_eq!(
            tw_oei_climb_constraint(0.02, 0.045, 1, 0.024, 1.2, 0.025),
            0.0
        );
    }

    #[test]
    fn the_v2_lift_schedule_decreases_with_speed_and_rejects_invalid_ratios() {
        let at_120 = oei_cl_at_v2(1.8, 1.20).unwrap();
        let at_130 = oei_cl_at_v2(1.8, 1.30).unwrap();
        assert!(at_120 > at_130);
        assert!((at_120 - 1.25).abs() < 1.0e-12);
        assert_eq!(oei_cl_at_v2(1.8, 1.0), None);
        assert_eq!(oei_cl_at_v2(1.8, f64::NAN), None);
        assert_eq!(oei_cl_at_v2(0.0, 1.2), None);
    }

    #[test]
    fn sls_conversion_requires_a_positive_condition_thrust_ratio() {
        assert_eq!(sls_tw_for_oei_condition(0.20, 0.80), Some(0.25));
        assert_eq!(sls_tw_for_oei_condition(0.20, 0.0), None);
        assert_eq!(sls_tw_for_oei_condition(0.20, f64::NAN), None);
        assert_eq!(sls_tw_for_oei_condition(-0.20, 0.8), None);
    }

    #[test]
    fn v2_oei_helper_requires_supported_count_and_complete_drag_evidence() {
        let drag = OeiDragIncrements::new(0.025, Some(0.003), Some(0.004));
        let at_sls =
            tw_oei_climb_constraint_at_v2(0.02, 0.045, 2, 0.024, 1.8, 1.20, drag, 1.0).unwrap();
        let lapsed =
            tw_oei_climb_constraint_at_v2(0.02, 0.045, 2, 0.024, 1.8, 1.20, drag, 0.80).unwrap();
        assert!(lapsed > at_sls);
        assert_eq!(
            tw_oei_climb_constraint_at_v2(0.02, 0.045, 5, 0.030, 1.8, 1.20, drag, 1.0,),
            None
        );
        assert_eq!(
            tw_oei_climb_constraint_at_v2(
                0.02,
                0.045,
                2,
                0.024,
                1.8,
                1.20,
                OeiDragIncrements::new(0.025, None, Some(0.004)),
                1.0,
            ),
            None
        );
        assert_eq!(
            tw_oei_climb_constraint_at_v2(
                0.02,
                0.045,
                2,
                0.024,
                1.8,
                1.20,
                OeiDragIncrements::new(-0.001, Some(0.0), Some(0.0)),
                1.0,
            ),
            None
        );
        assert_eq!(
            tw_oei_climb_constraint_at_v2(0.02, 0.045, 2, 0.024, 1.8, 1.20, drag, -0.1,),
            None
        );
    }

    fn complete_v2_condition() -> OeiV2Condition {
        OeiV2Condition {
            departure_elevation_m: 250.0,
            departure_isa_deviation_c: 15.0,
            v2_over_vstall: 1.20,
            condition_to_sls_thrust_ratio: Some(0.80),
            asymmetric_trim_cd: Some(0.003),
            windmilling_cd: Some(0.004),
        }
    }

    #[test]
    fn oei_assessment_requires_departure_v2_evidence_before_sls_scoring() {
        let conceptual = assess_oei_climb(0.02, 0.045, 2, 0.024, 1.2, 0.025, 1.8, None);
        assert_eq!(conceptual.status, OeiClimbStatus::ConceptualInflight);
        assert!(conceptual.required_inflight_tw.is_some());
        assert_eq!(conceptual.required_sls_tw, None);
        assert!(conceptual.diagnostic.contains("SLS evidence is absent"));

        let complete = assess_oei_climb(
            0.02,
            0.045,
            2,
            0.024,
            1.2,
            0.025,
            1.8,
            Some(complete_v2_condition()),
        );
        assert_eq!(complete.status, OeiClimbStatus::SlsEquivalent);
        assert!(complete.required_inflight_tw.is_some());
        assert!(complete.required_sls_tw.unwrap() > complete.required_inflight_tw.unwrap());

        let missing_ratio = assess_oei_climb(
            0.02,
            0.045,
            2,
            0.024,
            1.2,
            0.025,
            1.8,
            Some(OeiV2Condition {
                condition_to_sls_thrust_ratio: None,
                ..complete_v2_condition()
            }),
        );
        assert_eq!(missing_ratio.status, OeiClimbStatus::EvidenceGap);
        assert_eq!(missing_ratio.required_sls_tw, None);
        assert!(missing_ratio.diagnostic.contains("ratio is missing"));
    }

    #[test]
    fn oei_assessment_rejects_invalid_condition_metadata_and_engine_counts() {
        let invalid_context = assess_oei_climb(
            0.02,
            0.045,
            2,
            0.024,
            1.2,
            0.025,
            1.8,
            Some(OeiV2Condition {
                departure_elevation_m: f64::NAN,
                ..complete_v2_condition()
            }),
        );
        assert_eq!(invalid_context.status, OeiClimbStatus::EvidenceGap);
        assert_eq!(invalid_context.required_sls_tw, None);

        let not_applicable = assess_oei_climb(0.02, 0.045, 1, 0.024, 1.2, 0.025, 1.8, None);
        assert_eq!(not_applicable.status, OeiClimbStatus::NotApplicable);
        assert_eq!(not_applicable.required_inflight_tw, None);
        assert_eq!(not_applicable.required_sls_tw, None);

        let unsupported = assess_oei_climb(0.02, 0.045, 5, 0.024, 1.2, 0.025, 1.8, None);
        assert_eq!(unsupported.status, OeiClimbStatus::UnsupportedEngineCount);
        assert_eq!(unsupported.required_inflight_tw, None);
        assert_eq!(unsupported.required_sls_tw, None);
    }

    #[test]
    fn drag_increment_total_rejects_nonfinite_sums() {
        assert_eq!(
            OeiDragIncrements::new(f64::MAX, Some(f64::MAX), Some(0.0)).total_cd(),
            None
        );
    }

    #[test]
    fn the_takeoff_constraint_rises_with_wing_loading() {
        // A more heavily loaded wing needs more thrust off the same runway.
        let tw = tw_takeoff_constraint(&[3000.0, 6000.0], 3500.0, 1.0, 1.8);
        assert!(tw[1] > tw[0]);
    }

    #[test]
    fn omitted_keywords_fall_back_to_the_performance_config_defaults() {
        // Passing None for every config-backed keyword must reproduce passing
        // PerformanceConfig::default()'s own fields explicitly.
        let airports: [Airport; 0] = [];
        let d = PerformanceConfig::default();
        let with_none = build_matching_chart(
            0.02, 0.045, 0.78, 10668.0, 79_000.0, 122.0, 2, &airports, None, None, None, None,
            None, None, None, None, 5, None, None,
        );
        let with_explicit = build_matching_chart(
            0.02,
            0.045,
            0.78,
            10668.0,
            79_000.0,
            122.0,
            2,
            &airports,
            Some(d.cl_max_to),
            Some(d.cl_max_land),
            Some(d.thrust_lapse),
            Some(d.oei_gradient),
            Some(d.k_land),
            Some(d.oei_climb_cl),
            Some(d.oei_climb_delta_cd),
            None,
            5,
            Some(d.ws_min_pa),
            Some(d.ws_max_pa),
        );
        assert_eq!(with_none, with_explicit);
    }
}
