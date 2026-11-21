// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The cruise buffet-onset margin.
//!
//! A transport is operated so that, at its cruise Mach number, altitude and
//! mass, a load factor of at least 1.3 g is available before buffet onset.
//! CS 25.251(e) (EASA, *Certification Specifications for Large Aeroplanes*,
//! CS-25, "Vibration and buffeting") requires the buffet-onset boundaries to
//! be determined for the cruise configuration; the 0.3 g margin is the
//! operating practice that sets the maximum cruise altitude from those
//! boundaries (Airbus, *Getting to Grips with Aircraft Performance*, Airbus
//! Customer Services, 2002, maximum-altitude discussion). The 1.3 g figure is
//! therefore an operating criterion applied to the certified boundary, not a
//! number CS-25 itself prescribes (Obert, *Aerodynamic Design of Transport
//! Aircraft*, IOS Press, 2009, ch. 28 p. 371, states it the same way).
//!
//! Without the margin the search can raise the cruise lift coefficient (less
//! wing area at a given span, i.e. a higher aspect ratio at a higher wing
//! loading) or thicken the wing at no cost until wave drag rises, while the
//! real aircraft would lose its cruise altitude to buffet first.

use crate::mdo::ResidualRole;

use alas_aero::analysis::AeroAnalysis;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;

use super::sizing::candidate_model::candidate_mission_model;
use super::sizing::SizingOutcome;
use super::types::ConstraintFamily::Performance;
use super::types::ConstraintResidual;

/// Load factor that must be reachable before buffet onset at the cruise
/// point, g: the 0.3 g operating margin (Airbus 2002; see the module doc).
pub(crate) const BUFFET_LOAD_FACTOR: f64 = 1.3;

/// Wing buffet-onset lift coefficient and reference area, m^2, of `plane` at
/// the configured cruise Mach number and altitude.
///
/// The lift coefficient is `AeroAnalysis::buffet_onset`: the certified
/// boundary (0.25 g peak to peak at the pilot's seat), a published
/// definition transfer above the lift at which the local Mach number ahead
/// of the shock on the first outboard strip reaches the 1.38 of Obert (2009,
/// ch. 17 pp. 109-110), read from the offline MSES tables of the strip's
/// section family at the strip's normal Mach number, thickness and camber,
/// with the strip's basic and additional load from a wing-only lattice solve
/// (see that module). It is the physical boundary, not the Korn
/// drag-divergence relation the cruise drag uses, which sits at the cruise
/// lift of a transport by construction.
///
/// `Err` carries the reason no onset lift exists (no wing, a section family
/// without an onset table, a missing table cell), for the row that reports
/// it.
fn wing_basis(
    plane: &Airplane,
    fallback_sweep_deg: f64,
    config: &AlasConfig,
    altitude_m: f64,
) -> Result<(f64, f64), String> {
    if plane.wings.is_empty() {
        return Err("no main wing".to_owned());
    }
    let aero = AeroAnalysis::new(
        plane,
        AeroAnalysis::quarter_chord_sweep_deg(plane, fallback_sweep_deg),
        Some(config.geometry.clone()),
        Some(config.resolved_drag_model()),
        Some(config.analysis.clone()),
    );
    let onset = aero.buffet_onset(config.requirements.cruise_mach, altitude_m);
    match onset.cl_b() {
        Some(cl_b) if cl_b.is_finite() && cl_b > 0.0 => Ok((cl_b, plane.s_ref)),
        _ => Err(format!("{onset:?}")),
    }
}

/// The wing buffet-onset lift coefficient the mission's cruise-level
/// selection is bounded by ([`crate::mdo::SegmentMissionModel::with_buffet_onset_cl`]):
/// [`wing_basis`] at the configured cruise Mach number. The onset lift depends
/// on the Mach number only (the strip load is a lattice solve at that Mach and
/// the onset tables are at one Reynolds number), so it is read once, at the
/// design cruise altitude. `None` where [`buffet_residuals`] emits no margin
/// (a cruise Mach below the wave-drag onset) or the onset lift is unavailable
/// (that row reports why).
pub(crate) fn mission_buffet_onset_cl(
    plane: &Airplane,
    fallback_sweep_deg: f64,
    config: &AlasConfig,
) -> Option<f64> {
    let req = &config.requirements;
    if req.cruise_mach < config.drag_model.wave_drag_onset_mach {
        return None;
    }
    wing_basis(plane, fallback_sweep_deg, config, req.cruise_altitude_m)
        .ok()
        .map(|(cl_b, _)| cl_b)
}

/// The row of a margin whose onset lift is unavailable: an undefined value,
/// so a hard row is infeasible and a diagnostic row says why, never a
/// missing row that would drop the floor without notice.
fn unavailable_row(
    id: &'static str,
    role: ResidualRole,
    reason: &str,
    root_airfoil: &str,
) -> ConstraintResidual {
    ConstraintResidual::scaled(
        id,
        Performance,
        f64::NAN,
        BUFFET_LOAD_FACTOR,
        "g",
        f64::NAN,
        role,
    )
    .with_detail(format!(
        "buffet onset unavailable for wing section family `{root_airfoil}`: {reason}; the \
         {BUFFET_LOAD_FACTOR} g floor cannot be checked (open fix: tabulate the family)"
    ))
}

/// Published buffet-onset boundaries read at the cruise Mach: preset, Mach,
/// wing lift coefficient (Obert 2009, Fig. 28.3 p. 371, Figs. 28.10 and 28.13
/// p. 375; read off the figures, about +-0.02).
const PUBLISHED_CL_B: [(&str, f64, f64); 3] = [
    ("A320-200", 0.78, 0.855),
    ("B747-400", 0.85, 0.72),
    ("DC-10", 0.82, 0.755),
];

/// Accuracy of the model boundary, recorded in the row text and the
/// data-file header.
const MODEL_ACCURACY: &str = "model CL_b is within about 10 % of the certified boundaries of \
     Obert 2009 Figs. 28.3, 28.10 and 28.13 (A320 +6 %, 747-400 0 %, DC-10 -7 %)";

/// Highest altitude, m, at which `load_factor_at` still reaches
/// [`BUFFET_LOAD_FACTOR`]: the buffet-limited cruise ceiling of the mass the
/// closure holds. The load factor falls with altitude at a fixed Mach
/// number, so the ceiling is the root of a monotonic function; `None` when
/// the margin is missing at sea level or still held at 20 km.
fn buffet_ceiling_m(load_factor_at: impl Fn(f64) -> f64) -> Option<f64> {
    let (mut low, mut high) = (0.0_f64, 20_000.0_f64);
    if load_factor_at(low) < BUFFET_LOAD_FACTOR || load_factor_at(high) >= BUFFET_LOAD_FACTOR {
        return None;
    }
    for _ in 0..40 {
        let mid = 0.5 * (low + high);
        if load_factor_at(mid) >= BUFFET_LOAD_FACTOR {
            low = mid;
        } else {
            high = mid;
        }
    }
    Some(low)
}

/// The start-of-cruise condition of the design mission: the initial cruise
/// level the mission flies and the top-of-climb mass, the takeoff mass less
/// the climb-ladder fuel of the flown trip. `None` when the mission model
/// cannot fly the trip.
fn top_of_climb_condition(outcome: &SizingOutcome, config: &AlasConfig) -> Option<(f64, f64)> {
    let sized = &outcome.sized;
    let model = candidate_mission_model(config, &sized.fuel_artifacts).ok()?;
    let (takeoff_kg, range_m) = sized
        .fuel_artifacts
        .frozen_plan
        .as_ref()
        .map_or((sized.takeoff_mass_kg, sized.design_range_m), |plan| {
            (plan.takeoff_mass_kg, plan.range_m)
        });
    let flown = model.fly_trip(takeoff_kg, range_m).ok()?;
    let toc_mass_kg = takeoff_kg - flown.climb_fuel_kg;
    (flown.cruise_altitude_m.is_finite()
        && flown.cruise_altitude_m > 0.0
        && toc_mass_kg.is_finite()
        && toc_mass_kg > 0.0)
        .then_some((flown.cruise_altitude_m, toc_mass_kg))
}

/// The buffet residuals: `buffet_margin`, the load factor to buffet onset at
/// the mission's start-of-cruise condition, and `buffet_margin_mtow`, the
/// same at the closed takeoff mass and the design cruise altitude.
///
/// # Condition
///
/// The 1.3 g margin selects the cruise altitude (an operating criterion:
/// CS 25.251(e) and AMC 25.251, EASA CS-25, "Vibration and buffeting", require
/// the buffet boundary to be established, and the 1.3 g operating margin is
/// applied to it; Obert 2009, ch. 28 p. 371 and Figs. 28.10-28.13 p. 375
/// give the boundaries). It is therefore evaluated where the mission starts
/// to cruise: the initial cruise level it flies (the flown trip's
/// `cruise_altitude_m`, the frozen plan's initial level) at the top-of-climb
/// mass, takeoff mass less the climb fuel of the flown trip. At that
/// condition `n = CL_b q S / W`, with `CL_b` read at the cruise Mach number
/// and that altitude. The takeoff mass at the design altitude is kept as the
/// reported diagnostic `buffet_margin_mtow`: it is not what a certified
/// aircraft flies (the 747-400 reads about 1.17 there from Fig. 28.10).
///
/// The mission's initial level is the lowest of the thrust-limited one
/// (300 ft/min residual climb), the level at which the top-of-climb mass
/// holds the margin ([`mission_buffet_onset_cl`]) and the certified ceiling,
/// as an operator selects it, so the row holds where the onset lift the
/// mission was bounded with is the row's own; the row text gives the
/// buffet-limited level.
///
/// # Role
///
/// Hard ([`ResidualRole::Constraint`]) in the design modes; diagnostic in the
/// baseline sandbox, where a published aircraft's own buffet boundary is
/// compared, not enforced.
///
/// # Floor
///
/// [`BUFFET_LOAD_FACTOR`], absolute. An answer extrapolated beyond the onset
/// tables is still used (clamped at the table edge). An unavailable one (a
/// section family the tables do not cover, a missing cell) emits the row
/// with an undefined value and the reason: infeasible in the design modes,
/// a diagnostic in the sandbox; the floor is never dropped silently.
///
/// Below the configured wave-drag onset Mach the local-Mach criterion is
/// outside its domain and low-speed buffet is a stall-margin question the
/// field performance family covers, so no residual is emitted.
pub(super) fn buffet_residuals(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    let req = &config.requirements;
    let mach = req.cruise_mach;
    if mach < config.drag_model.wave_drag_onset_mach {
        return Vec::new();
    }
    let sweep = outcome.history.dv.sweep_deg;
    let load_factor = |altitude_m: f64, mass_kg: f64, cl_b: f64| {
        let atmosphere = alas_atmo::Atmosphere::new(altitude_m);
        let v_m_s = mach * atmosphere.speed_of_sound();
        let q_pa = 0.5 * atmosphere.density() * v_m_s.powi(2);
        cl_b * q_pa * outcome.plane.s_ref / (mass_kg * req.gravity_m_s2)
    };
    let mut residuals = Vec::new();
    let baseline = config.optimizer.design_space.mode == alas_config::DesignMode::BaselineSandbox;
    let gate_role = if baseline {
        ResidualRole::Diagnostic
    } else {
        role
    };
    let published = PUBLISHED_CL_B
        .iter()
        .find(|(preset, m, _)| *preset == config.preset && (m - mach).abs() < 0.02);

    if let Some((altitude_m, toc_mass_kg)) = top_of_climb_condition(outcome, config) {
        match wing_basis(&outcome.plane, sweep, config, altitude_m) {
            Err(reason) => residuals.push(unavailable_row(
                "buffet_margin",
                gate_role,
                &reason,
                &config.geometry.wing.root_airfoil,
            )),
            Ok((cl_b, _)) => {
                let n = load_factor(altitude_m, toc_mass_kg, cl_b);
                // The onset lift is a function of the Mach number only, so
                // the level at which this mass still has the margin follows
                // from the dynamic pressure alone.
                let ceiling = buffet_ceiling_m(|h| load_factor(h, toc_mass_kg, cl_b)).map_or_else(
                    || "none below 20 km".to_owned(),
                    |ceiling_m| format!("{ceiling_m:.0} m"),
                );
                let mut detail = format!(
                    "start of cruise: level {altitude_m:.0} m, top-of-climb mass \
                     {toc_mass_kg:.0} kg, CL_b {cl_b:.3}; level at which this mass holds \
                     {BUFFET_LOAD_FACTOR} g: {ceiling}; {MODEL_ACCURACY}"
                );
                if let Some((_, _, cl_pub)) = published {
                    detail.push_str(&format!(
                        "; published CL_b {cl_pub:.3} (Obert 2009 ch. 28), ALAS/published {:.2}",
                        cl_b / cl_pub
                    ));
                }
                residuals.push(
                    ConstraintResidual::scaled(
                        "buffet_margin",
                        Performance,
                        n,
                        BUFFET_LOAD_FACTOR,
                        "g",
                        BUFFET_LOAD_FACTOR - n,
                        gate_role,
                    )
                    .with_detail(detail),
                );
            }
        }
    }
    match wing_basis(&outcome.plane, sweep, config, req.cruise_altitude_m) {
        Err(reason) => residuals.push(unavailable_row(
            "buffet_margin_mtow",
            ResidualRole::Diagnostic,
            &reason,
            &config.geometry.wing.root_airfoil,
        )),
        Ok((cl_b, _)) => {
            let n = load_factor(req.cruise_altitude_m, outcome.sized.takeoff_mass_kg, cl_b);
            residuals.push(
                ConstraintResidual::scaled(
                    "buffet_margin_mtow",
                    Performance,
                    n,
                    BUFFET_LOAD_FACTOR,
                    "g",
                    BUFFET_LOAD_FACTOR - n,
                    ResidualRole::Diagnostic,
                )
                .with_detail(format!(
                    "reported only: closed takeoff mass {:.0} kg at the design cruise altitude \
                 {:.0} m; {MODEL_ACCURACY}",
                    outcome.sized.takeoff_mass_kg, req.cruise_altitude_m
                )),
            );
        }
    }
    residuals
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A missing onset is a row, not an absence: hard rows are infeasible and
    /// every row names the section family.
    #[test]
    fn an_unavailable_onset_is_an_explicit_row_infeasible_when_hard() {
        let hard = unavailable_row(
            "buffet_margin",
            ResidualRole::Constraint,
            "Unavailable(FamilyNotTabulated(\"b737a\"))",
            "b737a",
        );
        assert!(hard.violated());
        assert_eq!(hard.role, ResidualRole::Constraint);
        assert!(hard
            .detail
            .as_deref()
            .unwrap_or_default()
            .contains("`b737a`"));
        let diagnostic = unavailable_row(
            "buffet_margin",
            ResidualRole::Diagnostic,
            "Unavailable(FamilyNotTabulated(\"b737a\"))",
            "b737a",
        );
        assert_eq!(diagnostic.role, ResidualRole::Diagnostic);
        assert!(diagnostic
            .detail
            .as_deref()
            .unwrap_or_default()
            .contains("cannot be checked"));
    }
}
