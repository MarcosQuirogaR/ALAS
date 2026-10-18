// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The cruise-altitude thrust residuals from the engine deck (FLD-11, SIZ-08).
//!
//! The flat `performance.thrust_lapse` ratio (0.235 by default, one constant
//! for every engine and cruise point) is replaced by the ratio the deck
//! delivers at the design cruise Mach and altitude, `r = F(h, M) / F_SLS`,
//! through the same [`EngineOutModel`] that gives the engine-out thrust
//! ratio: the maximum-cruise rating for level flight (0.93 of maximum climb,
//! Torenbeek 2013 Sec. 9.2 and Jenkinson Figs. 9.19-9.21) and the
//! maximum-climb rating for the top-of-climb rate. Two residuals, both written on the
//! installed sea-level-static thrust-to-weight axis
//! (`alas_perf::performance::cruise_climb`):
//!
//! - `cruise_thrust`: level flight at the mid-cruise weight (the weight the
//!   design altitude is the representative level of),
//!   `(T/W)_SLS >= (m_mid / m_TO) (CD/CL) / r`;
//! - `toc_rate_of_climb` (Constraint for jets, [`toc_rows`]): the 300 ft/min
//!   residual climb at the top-of-climb mass (0.985 of the plan mass) at the
//!   *initial cruise level the design mission actually selects*, evaluated by
//!   the mission model itself (`improved_climb::top_of_climb`). The mission
//!   lowers the initial level until the criterion holds (step-climb
//!   practice), so the design altitude of the requirements, which is the
//!   mid-cruise altitude of a step-climbing aircraft, is not the test; a miss
//!   means no usable level exists. The closed-form T/W of the same criterion
//!   ([`DeckCruiseRequirements::top_of_climb_tw`], at the design altitude) is
//!   information only.
//!
//! Level flight is evaluated at the mid-cruise mass
//! (`SizedCandidate::mid_cruise_mass_kg`, the same mass the wave-drag gate
//! uses at this point). The flat-lapse residual and the first deck version
//! used the take-off weight, which is the initial-cruise convention
//! (Torenbeek 2013, Eq. 9.3) paired with an altitude that is not the initial
//! level: a conservative choice that failed the long-range aircraft by the
//! burned fuel (3-12 % of the take-off mass) rather than by a thrust deficit
//! (fix-deck-altitude-2026-10-12). The top-of-climb row stays at the
//! top-of-climb mass at maximum climb. A deck that cannot be evaluated is an
//! evidence gap, never a silent return to the flat lapse.

use alas_config::{AlasConfig, DesignRequirements};
use alas_perf::performance::{
    required_sls_tw_level, required_sls_tw_top_of_climb, TOC_MASS_FRACTION, TOC_RESIDUAL_CLIMB_M_S,
};

use super::improved_climb::{top_of_climb, TOC_TOLERANCE_M_S};
use super::oei_deck::{EngineOutCondition, EngineOutModel, EngineOutRating};
pub use super::planned_cruise::{
    highest_cruise_level_m, planned_cruise_requirements, PlannedCruise, PlannedCruiseLevel,
};
use super::thrust_uncertainty::{thrust_scatter, ThrustAssessment, ThrustVerdict};
use crate::mdo::types::{
    CandidateDrag, CandidateFuelArtifacts, ConstraintFamily, ConstraintResidual,
};
use crate::mdo::ResidualRole;

/// The deck-based cruise-altitude requirements of one candidate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeckCruiseRequirements {
    /// Maximum-climb thrust at the cruise point over rated static thrust
    /// (the top-of-climb rating).
    pub thrust_ratio: f64,
    /// Maximum-cruise thrust at the cruise point over rated static thrust
    /// (the level-flight rating).
    pub cruise_thrust_ratio: f64,
    /// Required installed sea-level-static `T/W` (over the take-off weight)
    /// for level flight at the level-flight mass.
    pub level_tw: f64,
    /// Required installed sea-level-static `T/W` for the top-of-climb rate.
    pub top_of_climb_tw: f64,
    /// True airspeed at the cruise point, m/s.
    pub tas_m_s: f64,
}

/// The cruise-altitude rows of one candidate: `cruise_thrust` (Constraint)
/// and the `toc_rate_of_climb` Diagnostic from the engine deck, or the flat
/// `thrust_lapse` form when the deck basis is off or does not apply.
///
/// A turboprop takes the same form: its thrust ratio is the unit model's
/// maximum-cruise thrust over the installed static thrust the caller binds
/// into the [`EngineOutModel`] (`propulsion::turboprop_static_thrust_n`), the same figure
/// `available_tw` is built from, so the comparison is exactly the in-flight
/// thrust against the in-flight drag. The reporting findings
/// (`alas_pipeline::feasibility::cruise_deck`) evaluate this one definition.
/// A deck that cannot be evaluated raises `cruise_deck_evidence_gap` and the lapse row stands in,
/// flagged rather than presented as the deck result.
///
/// `(takeoff_mass_kg, level_mass_kg)`: the take-off mass and the mass level
/// flight is evaluated at (mid-cruise).
pub(super) fn cruise_rows(
    engine_out: Option<&EngineOutModel>,
    drag: &CandidateDrag,
    config: &AlasConfig,
    (takeoff_mass_kg, level_mass_kg): (f64, f64),
    (s_ref_m2, available_tw): (f64, f64),
    artifacts: &CandidateFuelArtifacts,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    let req = &config.requirements;
    let perf = &config.performance;
    let deck = perf.cruise_thrust_from_deck.then(|| {
        let model =
            engine_out.ok_or_else(|| "the engine-out deck binding is unavailable".to_owned())?;
        // The levels the mission plan flies; without a usable plan the
        // design level at the mid-cruise mass (the earlier definition).
        match planned_cruise_requirements(model, config, artifacts, takeoff_mass_kg, s_ref_m2) {
            Ok(planned) => Ok((planned.worst, planned.describe())),
            Err(reason) => {
                deck_cruise_requirements(model, drag, req, takeoff_mass_kg, level_mass_kg, s_ref_m2)
                    .map(|deck| {
                        (
                            deck,
                            format!(
                                "the design level at the mid-cruise weight (no plan: {reason})"
                            ),
                        )
                    })
            }
        }
    });
    let row = |id: &'static str, required: f64, role: ResidualRole| {
        ConstraintResidual::scaled(
            id,
            ConstraintFamily::Performance,
            available_tw,
            required,
            "T/W",
            required - available_tw,
            role,
        )
    };
    match deck {
        Some(Ok((deck, basis))) => {
            // An engine without a cruise or climb datum fails hard only
            // beyond the chart scatter; inside it the shortfall is a flagged
            // warning row (never hidden).
            let assessment =
                ThrustAssessment::new(deck.level_tw, available_tw, thrust_scatter(config));
            let mut rows = vec![ConstraintResidual::direct(
                "cruise_thrust",
                ConstraintFamily::Performance,
                available_tw,
                deck.level_tw,
                "T/W",
                deck.level_tw - available_tw,
                assessment.violation(),
                role,
            )
            .with_detail(format!("level cruise at {basis}"))];
            rows.extend(scatter_warning(
                "cruise_thrust_within_scatter",
                &format!("level cruise at {basis}"),
                &assessment,
                "T/W",
            ));
            rows
        }
        other => {
            let mut rows = Vec::new();
            if matches!(other, Some(Err(_))) {
                rows.push(ConstraintResidual::direct(
                    "cruise_deck_evidence_gap",
                    ConstraintFamily::Performance,
                    1.0,
                    0.0,
                    "bool",
                    1.0,
                    1.0,
                    ResidualRole::Diagnostic,
                ));
            }
            // Mass closure changes CL without rebuilding a CG-compatible
            // table: evaluate the drag at that CL, not on a design-CL tangent.
            let atmosphere = alas_atmo::Atmosphere::new(req.cruise_altitude_m);
            let speed_m_s = req.cruise_mach * atmosphere.speed_of_sound();
            let q_pa = 0.5 * atmosphere.density() * speed_m_s * speed_m_s;
            let cl = takeoff_mass_kg * req.gravity_m_s2 / s_ref_m2 / q_pa;
            let required =
                drag.cd(cl, req.cruise_mach, req.cruise_altitude_m) / cl / perf.thrust_lapse;
            rows.push(row("cruise_thrust", required, role));
            rows
        }
    }
}

/// The `toc_rate_of_climb` row: the residual climb of the design mission at
/// the initial cruise level it selects must hold the 300 ft/min criterion
/// (initial cruise altitude capability), evaluated by the mission model that
/// selects the level (step-climb practice: it lowers the level until the
/// criterion holds, so a miss means no usable level exists). A model that
/// cannot be built or flown raises `toc_evidence_gap`.
pub(super) fn toc_rows(
    config: &AlasConfig,
    artifacts: &CandidateFuelArtifacts,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    if !config.performance.cruise_thrust_from_deck {
        return Vec::new();
    }
    // A turboprop's maximum-climb thrust rests on the unit model scaled to the
    // installed engine (an ATR stand-in for the A400M) and its level search is
    // not validated: its row is reported, not enforced.
    let role = if matches!(
        config.geometry.engine.active_model(),
        Ok(alas_config::ActiveEngineModel::Turboprop(_))
    ) {
        ResidualRole::Diagnostic
    } else {
        role
    };
    match top_of_climb(config, artifacts) {
        Ok(toc) => {
            let scatter = thrust_scatter(config);
            // A datum engine keeps the rate-margin check exactly; otherwise
            // the miss is a thrust shortfall judged against the scatter.
            let assessment = toc.thrust_assessment(scatter);
            let miss = -toc.margin_m_s() - TOC_TOLERANCE_M_S;
            let violation = if scatter > 0.0 {
                assessment.violation()
            } else {
                (miss / TOC_RESIDUAL_CLIMB_M_S).max(0.0)
            };
            let mut rows = vec![ConstraintResidual::direct(
                "toc_rate_of_climb",
                ConstraintFamily::Performance,
                toc.rate_m_s,
                TOC_RESIDUAL_CLIMB_M_S,
                "m/s",
                -toc.margin_m_s(),
                violation,
                role,
            )];
            if scatter > 0.0 && toc.margin_m_s() < -TOC_TOLERANCE_M_S {
                rows.extend(scatter_warning(
                    "toc_thrust_within_scatter",
                    "300 ft/min residual climb at the initial cruise level",
                    &assessment,
                    "N",
                ));
            }
            rows
        }
        Err(_) => vec![ConstraintResidual::direct(
            "toc_evidence_gap",
            ConstraintFamily::Performance,
            1.0,
            0.0,
            "bool",
            1.0,
            1.0,
            ResidualRole::Diagnostic,
        )],
    }
}
/// The flagged warning row of a shortfall inside the scatter band: a
/// Diagnostic (it does not gate) carrying required, available and scatter.
fn scatter_warning(
    id: &'static str,
    what: &str,
    assessment: &ThrustAssessment,
    unit: &'static str,
) -> Option<ConstraintResidual> {
    (assessment.verdict() == ThrustVerdict::Warning).then(|| {
        ConstraintResidual::direct(
            id,
            ConstraintFamily::Performance,
            assessment.available,
            assessment.required,
            unit,
            assessment.required - assessment.available,
            0.0,
            ResidualRole::Diagnostic,
        )
        .with_detail(assessment.describe(what, unit))
    })
}

/// Evaluate the deck-based cruise requirements at the design cruise point for
/// `takeoff_mass_kg` on `s_ref_m2` of wing; level flight is evaluated at
/// `level_mass_kg` (the mid-cruise mass, or the take-off mass when none is
/// known), the top-of-climb row at 0.985 of `takeoff_mass_kg`.
///
/// # Errors
///
/// The deck rejection or an unusable intermediate, as text.
pub fn deck_cruise_requirements(
    model: &EngineOutModel,
    drag: &CandidateDrag,
    req: &DesignRequirements,
    takeoff_mass_kg: f64,
    level_mass_kg: f64,
    s_ref_m2: f64,
) -> Result<DeckCruiseRequirements, String> {
    deck_cruise_at(
        model,
        drag,
        req.gravity_m_s2,
        (req.cruise_altitude_m, req.cruise_mach, 0.0),
        (takeoff_mass_kg, level_mass_kg),
        s_ref_m2,
    )
}

/// Density and speed of sound at `altitude_m` on a day `isa_deviation_c` from
/// ISA (the plain ISA atmosphere at zero deviation).
pub(super) fn ambient(altitude_m: f64, isa_deviation_c: f64) -> (f64, f64) {
    if isa_deviation_c == 0.0 {
        let atmosphere = alas_atmo::Atmosphere::new(altitude_m);
        (atmosphere.density(), atmosphere.speed_of_sound())
    } else {
        let day = alas_atmo::us1976_compute_values(altitude_m, isa_deviation_c);
        (day.density_kg_m3, day.speed_of_sound_m_s)
    }
}

/// [`deck_cruise_requirements`] at the cruise point `(altitude_m, mach,
/// isa_deviation_c)`: the day the mission flies (ISA plus the departure
/// airport's deviation), for the density and speed of sound behind the lift
/// coefficient, the Mach of the drag and the deck's thrust.
pub(super) fn deck_cruise_at(
    model: &EngineOutModel,
    drag: &CandidateDrag,
    gravity_m_s2: f64,
    (altitude_m, mach, isa_deviation_c): (f64, f64, f64),
    (takeoff_mass_kg, level_mass_kg): (f64, f64),
    s_ref_m2: f64,
) -> Result<DeckCruiseRequirements, String> {
    // A mass that is not usable falls back to the take-off weight (the
    // earlier, conservative convention) rather than to no requirement.
    let level_mass_kg = if level_mass_kg.is_finite() && level_mass_kg > 0.0 {
        level_mass_kg.min(takeoff_mass_kg)
    } else {
        takeoff_mass_kg
    };
    let (density_kg_m3, speed_of_sound_m_s) = ambient(altitude_m, isa_deviation_c);
    let tas_m_s = mach * speed_of_sound_m_s;
    let q_pa = 0.5 * density_kg_m3 * tas_m_s * tas_m_s;
    let weight_n = takeoff_mass_kg * gravity_m_s2;
    let cl = weight_n / (s_ref_m2 * q_pa);
    if !(cl.is_finite() && cl > 0.0) {
        return Err("cruise lift coefficient is not usable".to_owned());
    }
    let condition = EngineOutCondition {
        altitude_m,
        isa_deviation_c,
        tas_m_s,
    };
    let (_, thrust_ratio) = model.thrust(condition, EngineOutRating::MaximumClimb, false)?;
    let (_, cruise_thrust_ratio) =
        model.thrust(condition, EngineOutRating::MaximumCruise, false)?;
    // Mass closure changes CL without rebuilding a CG-compatible table:
    // evaluate the drag at each lift coefficient, not on a tangent.
    let cl_toc = TOC_MASS_FRACTION * cl;
    let cl_level = level_mass_kg / takeoff_mass_kg * cl;
    let drag_over_weight = drag.cd(cl_level, mach, altitude_m) / cl_level;
    let drag_over_weight_toc = drag.cd(cl_toc, mach, altitude_m) / cl_toc;
    let level_tw = required_sls_tw_level(
        drag_over_weight,
        level_mass_kg / takeoff_mass_kg,
        cruise_thrust_ratio,
    )
    .ok_or_else(|| "level-flight requirement is not usable".to_owned())?;
    let top_of_climb_tw = required_sls_tw_top_of_climb(
        drag_over_weight_toc,
        TOC_RESIDUAL_CLIMB_M_S,
        tas_m_s,
        TOC_MASS_FRACTION,
        thrust_ratio,
    )
    .ok_or_else(|| "top-of-climb requirement is not usable".to_owned())?;
    Ok(DeckCruiseRequirements {
        thrust_ratio,
        cruise_thrust_ratio,
        level_tw,
        top_of_climb_tw,
        tas_m_s,
    })
}
