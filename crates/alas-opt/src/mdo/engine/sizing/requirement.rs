// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The thrust requirement of a closed airframe, as a factor on its installed
//! rating, found by bisection over the real performance residual table.

use alas_config::{AlasConfig, ENGINE_SCALE_SEARCH_BRACKET};

use super::super::static_thrust_kn_per_engine;
use crate::mdo::residuals_performance::cruise_deck::deck_cruise_requirements;
use crate::mdo::residuals_performance::oei_deck::EngineOutModel;
use crate::mdo::residuals_performance::performance_residuals_at;
use crate::mdo::sizing::SizingOutcome;
use crate::mdo::ResidualRole;

/// Relative width at which the requirement bisection stops, at most: the
/// caller narrows it to a fifth of the band it aims the engine in
/// (`sizing::AIM_BAND`), because the reported ratio is the bracket's met end
/// and carries up to this width of quantisation, which a narrower band
/// could never be met through.
const BISECTION_TOLERANCE: f64 = 5.0e-4;

/// Rows of the performance table that bound the thrust from below and do not
/// adapt to the thrust (see the module documentation). The declared
/// initial-cruise-altitude row exists only when the brief or preset declares
/// an ICA (`requirements.initial_cruise_altitude_m`); its level is fixed, so
/// unlike the planner's own level it cannot be lowered to meet the thrust.
///
/// The obstacle clearance of the net take-off flight path (CS 25.115,
/// CAT.POL.A.210: 35 ft over every obstacle of the departure survey) is a
/// thrust row like the net-path gradient beside it: a hard row of the design
/// modes that only more thrust meets once the airframe is closed. Left out,
/// the sized engine met every other row and failed it: on the B747-400
/// clean sheet 44 of 53 refinement analyses missed the
/// clearance by about 3 m with the engine at 0.89 of the selected one, and
/// the run retained the registered aircraft. Sized to it, the best of them
/// replayed feasible; the A320-200 delivered design is unchanged (its
/// clearance is met wherever the other rows are).
const THRUST_ROWS: [&str; 8] = [
    "oei_second_segment",
    "oei_final_segment",
    "oei_approach_climb",
    "landing_climb",
    "takeoff_field",
    "takeoff_net_path_gradient",
    "takeoff_obstacle_clearance",
    crate::mdo::residuals_performance::initial_cruise::ICA_ROW,
];

/// The planned-level cruise row, a sizing row of a turboprop: its planner has
/// no thrust ceiling (`cruise_ceiling_band`), so the plan does not lower the
/// level to meet it and it is a requirement the engine must carry.
const TURBOPROP_CRUISE_ROW: &str = "cruise_thrust";

pub(super) fn is_turboprop(config: &AlasConfig) -> bool {
    matches!(
        config.geometry.engine.active_model(),
        Ok(alas_config::ActiveEngineModel::Turboprop(_))
    )
}

/// Identifier reported when the design-level cruise thrust governs.
const CRUISE_ROW: &str = "cruise_thrust_design_level";

/// The requirement on the installed rating of a closed airframe.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Requirement {
    /// Required rating over the installed rating.
    pub(super) ratio: f64,
    pub(super) governing: &'static str,
}

fn probe(outcome: &mut SizingOutcome, config: &AlasConfig, factor: f64) -> Option<Probe> {
    let scaled = config.with_engine_scaled(factor).ok()?;
    let original = outcome.static_thrust_kn;
    outcome.static_thrust_kn = static_thrust_kn_per_engine(&scaled.geometry.engine).ok()?;
    let rows = performance_residuals_at(outcome, &scaled, ResidualRole::Constraint, None);
    outcome.static_thrust_kn = original;
    let turboprop = is_turboprop(&scaled);
    let mut seen = false;
    for row in rows.iter().filter(|row| {
        row.role == ResidualRole::Constraint
            && (THRUST_ROWS.contains(&row.id) || (turboprop && row.id == TURBOPROP_CRUISE_ROW))
    }) {
        seen = true;
        let met = row.limit.is_finite()
            && row.raw_residual.is_finite()
            && row.raw_residual <= 1.0e-5 * row.limit.abs().max(1.0e-9);
        if !met {
            return Some(Probe {
                met: false,
                failing: Some(row.id),
                seen,
                field_m: if row.id == "takeoff_field" {
                    row.limit
                } else {
                    f64::NAN
                },
            });
        }
    }
    Some(Probe {
        met: true,
        failing: None,
        seen,
        field_m: f64::NAN,
    })
}

#[derive(Debug, Clone, Copy)]
struct Probe {
    met: bool,
    failing: Option<&'static str>,
    /// At least one thrust row exists to size against.
    seen: bool,
    /// The take-off field length that failed the runway, m, when the field
    /// row is the first row to fail (every climb row before it is met).
    field_m: f64,
}

/// Step of the scan down from an engine with thrust to spare.
const OVERTHRUST_STEP: f64 = 0.87;

/// The factor to continue from when the take-off field fails *because* the
/// engine is too large, not too small: far beyond the need, the failure
/// recognition speed rises above the rotation speed with the acceleration, the
/// balanced field lengthens with thrust (CS 25.107(a)(2), 25.109) and the
/// monotone search upward would find a spurious boundary. A field that gets
/// shorter with less thrust, while every climb row is met, is that regime; the
/// scan returns the first factor below which meets every row, and the
/// closure iteration then searches downward from inside the feasible band.
fn overthrust_scan(
    outcome: &mut SizingOutcome,
    config: &AlasConfig,
    at_one: Probe,
    floor: f64,
    cruise: f64,
) -> Option<Requirement> {
    scan_down(
        &mut |factor| probe(outcome, config, factor),
        at_one,
        floor,
        cruise,
    )
}

/// The scan over an arbitrary probe, so the clamp and the label are testable.
fn scan_down(
    probe: &mut dyn FnMut(f64) -> Option<Probe>,
    at_one: Probe,
    floor: f64,
    cruise: f64,
) -> Option<Requirement> {
    if at_one.failing != Some("takeoff_field") || !at_one.field_m.is_finite() {
        return None;
    }
    let mut factor = OVERTHRUST_STEP;
    let mut at = probe(factor)?;
    let shorter = at.met || (at.failing == Some("takeoff_field") && at.field_m < at_one.field_m);
    if !shorter {
        return None;
    }
    // From here the engine is known to be too large: never fall through to
    // the upward search. Ending the scan at the floor (the cruise requirement
    // or the end of the bracket) without a met point returns the floor, and
    // the next closure judges it.
    while !at.met && factor * OVERTHRUST_STEP >= floor {
        factor *= OVERTHRUST_STEP;
        at = probe(factor)?;
    }
    let ratio = scan_ratio(factor, floor);
    Some(Requirement {
        ratio,
        governing: cruise_governs(
            ratio,
            cruise,
            if at.met {
                Some("takeoff_field_overthrust")
            } else {
                at.failing.or(Some("takeoff_field_overthrust"))
            },
        ),
    })
}

/// The row that governs a requirement of `ratio`: the design-level cruise
/// thrust when `ratio` is at it (within the bisection tolerance), else the
/// failing row.
fn cruise_governs(ratio: f64, cruise: f64, failing: Option<&'static str>) -> &'static str {
    if cruise > 0.0 && ratio <= cruise * (1.0 + BISECTION_TOLERANCE) {
        CRUISE_ROW
    } else {
        failing.unwrap_or("none")
    }
}

/// The factor the over-thrust scan reports: never below the floor (the
/// design-level cruise requirement or the end of the bracket), which the
/// scan's own step can overshoot when the floor is above the first step.
fn scan_ratio(factor: f64, floor: f64) -> f64 {
    factor.max(floor)
}

/// The design-level cruise requirement over the installed rating: level
/// flight at the design Mach and altitude at the mid-cruise mass on the
/// maximum-cruise rating.
fn cruise_ratio(outcome: &SizingOutcome, config: &AlasConfig) -> Option<f64> {
    let s_ref = outcome.plane.s_ref;
    let model = EngineOutModel::new(
        config,
        None,
        s_ref,
        Some(outcome.static_thrust_kn * 1_000.0),
    )
    .ok()?;
    let sized = &outcome.sized;
    let requirement = deck_cruise_requirements(
        &model,
        &sized.fuel_artifacts.drag,
        &config.requirements,
        sized.takeoff_mass_kg,
        sized.mid_cruise_mass_kg(),
        s_ref,
    )
    .ok()?;
    let available_tw = outcome.n_engines as f64 * outcome.static_thrust_kn * 1_000.0
        / (sized.takeoff_mass_kg * config.requirements.gravity_m_s2);
    let ratio = requirement.level_tw / available_tw;
    (ratio.is_finite() && ratio > 0.0).then_some(ratio)
}

/// The factor on the installed rating at which every sizing row is met.
pub(super) fn required_ratio(
    outcome: &mut SizingOutcome,
    config: &AlasConfig,
    band: f64,
) -> Option<Requirement> {
    let resolution = (0.2 * band).min(BISECTION_TOLERANCE);
    let (low, high) = ENGINE_SCALE_SEARCH_BRACKET;
    let at_one = probe(outcome, config, 1.0)?;
    if !at_one.seen {
        // No thrust row can be evaluated (airport or evidence missing): the
        // existing rows already refuse such a candidate; nothing to size to.
        return None;
    }
    // A jet's planner lowers the cruise level until the thrust holds, so the
    // requirement it cannot escape is the design level at the mid-cruise mass.
    let cruise = if is_turboprop(config) {
        0.0
    } else {
        cruise_ratio(outcome, config).unwrap_or(0.0)
    };
    let floor = low.max(cruise);
    let (mut bad, mut good, mut failing);
    if at_one.met {
        if floor >= 1.0 {
            return Some(Requirement {
                ratio: floor,
                governing: cruise_governs(floor, cruise, None),
            });
        }
        let at_floor = probe(outcome, config, floor)?;
        if at_floor.met {
            return Some(Requirement {
                ratio: floor,
                governing: cruise_governs(floor, cruise, None),
            });
        }
        (bad, good, failing) = (floor, 1.0, at_floor.failing);
    } else {
        if let Some(down) = overthrust_scan(outcome, config, at_one, floor, cruise) {
            return Some(down);
        }
        let at_high = probe(outcome, config, high)?;
        if !at_high.met {
            return Some(Requirement {
                ratio: high,
                governing: at_high.failing.unwrap_or("none"),
            });
        }
        (bad, good, failing) = (1.0, high, at_one.failing);
    }
    while good / bad - 1.0 > resolution {
        let middle = (bad * good).sqrt();
        let at_middle = probe(outcome, config, middle)?;
        if at_middle.met {
            good = middle;
        } else {
            bad = middle;
            failing = at_middle.failing;
        }
    }
    let ratio = good.max(cruise);
    Some(Requirement {
        ratio,
        governing: cruise_governs(ratio, cruise, failing),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_over_thrust_scan_never_reports_a_factor_below_the_floor() {
        // A cruise floor of 0.95 is above the first step of 0.87.
        assert_eq!(scan_ratio(OVERTHRUST_STEP, 0.95), 0.95);
        // Above the floor the scan's own factor stands.
        assert_eq!(scan_ratio(0.60, 0.40), 0.60);
        assert_eq!(scan_ratio(0.40, 0.40), 0.40);
    }

    fn failing_field(field_m: f64) -> Probe {
        Probe {
            met: false,
            failing: Some("takeoff_field"),
            seen: true,
            field_m,
        }
    }

    #[test]
    fn the_over_thrust_scan_labels_the_cruise_requirement_when_the_clamp_binds() {
        // Every factor still fails the (shorter) field; the floor of 0.95 is
        // above the first step of 0.87, so the clamp binds.
        let mut shorter = |factor: f64| Some(failing_field(2_800.0 * factor));
        let down = scan_down(&mut shorter, failing_field(3_000.0), 0.95, 0.95).unwrap();
        assert_eq!(down.ratio, 0.95);
        assert_eq!(down.governing, CRUISE_ROW);
        // The same scan with a floor that is only the end of the bracket keeps
        // the failing row.
        let mut shorter = |factor: f64| Some(failing_field(2_800.0 * factor));
        let down = scan_down(&mut shorter, failing_field(3_000.0), 0.40, 0.0).unwrap();
        assert!(down.ratio >= 0.40 && down.ratio < 0.40 / OVERTHRUST_STEP);
        assert_eq!(down.governing, "takeoff_field");
    }
}
