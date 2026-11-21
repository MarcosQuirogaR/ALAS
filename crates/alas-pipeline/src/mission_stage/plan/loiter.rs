// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The speed of a hold.
//!
//! Endurance is maximised by flying at the lift coefficient of minimum fuel
//! flow: for a jet, whose fuel flow follows thrust, the minimum-drag point
//! `max C_L / C_D`; for a propeller aircraft, whose fuel flow follows power, the
//! minimum-power point `max C_L^1.5 / C_D`. Raymer, *Aircraft Design: A
//! Conceptual Approach*, 6th ed., section 17.2 (Eqs. 17.13-17.21, pp. 640-641,
//! read from the page): at minimum power the speed is about 0.76 of the
//! minimum-drag speed and `L/D` is 0.866 of its maximum, for the parabolic
//! polar; p. 642 notes that programs search the real polar for the best speed,
//! which is what the scan below does over the aircraft's own drag table. The
//! priced mission model holds at the same minimum-drag lift coefficient
//! (`holding_fuel_flow_kg_s`).
//!
//! The scan stays at or above 1.3 `V_SR` of the clean configuration
//! (`cl_max_clean`). Where this floor comes from, stated plainly: CS-25 has no
//! requirement on the speed a hold is flown at. The only place it names a
//! "minimum holding speed" is the icing flight-test programme of AMC to
//! CS 25.21(g) (section 6.9 and Tables 2 and 3: Amendment 28 Easy Access pages
//! 70-80 of 1515, Amendment 27 page 76 of 1495; text as read), where the
//! holding configuration is trimmed at 1.3 `V_SR` and 40 degree banked turns are
//! flown at holding speed. Every use of "holding" in the text of Amendment 28
//! (30 hits, searched in full) is an icing programme, an en-route flap
//! definition or a fire-protection allowance: none sets a speed. The model uses 1.3
//! `V_SR` as the margin that leaves that manoeuvre capability (a 40 degree turn
//! needs a load factor of 1.31, so at least 1.14 `V_SR` before the stall) and the
//! stall warning, not because a rule asks for it. It binds only where the
//! optimum lies below it, and then the hold flies faster than its optimum, so
//! the fuel of the hold is on the safe side. A hold with a stated speed
//! (`speed_m_s` of the stage) never uses it. Open question: an
//! operational source (an FCOM holding-speed rule) could replace it. The scan has no
//! lower bound on the lift coefficient: it runs from that ceiling toward lower
//! lift (higher speed) until the merit has fallen to half of the best found,
//! past the maximum of the unimodal merit.

use alas_atmo::us1976_try_compute_values;
use alas_mission::segments::{MissionAnalyses, MissionDragSource};

/// Margin over the reference stall speed a hold is flown at least with: a stated
/// floor (see the module documentation for its provenance and its limits).
const STALL_MARGIN: f64 = 1.3;

/// Merit, as a fraction of the best found, below which the scan stops.
const STOP_FRACTION: f64 = 0.5;

/// Points of the lift-coefficient scan.
const SCAN_POINTS: usize = 240;

/// The speed of minimum fuel flow, m/s, at `altitude_m` and `mass_kg` on a day
/// `isa_deviation_k` warmer than standard; `None` when the aircraft's drag is
/// not the shared candidate table or the atmosphere is unusable.
#[must_use]
pub fn endurance_speed_m_s(
    analyses: &MissionAnalyses,
    altitude_m: f64,
    mass_kg: f64,
    isa_deviation_k: f64,
    turboprop: bool,
    cl_max_clean: f64,
) -> Option<f64> {
    let MissionDragSource::SharedCandidate(model) = &analyses.drag_source else {
        return None;
    };
    let atmosphere = us1976_try_compute_values(altitude_m, isa_deviation_k).ok()?;
    let weight_n = mass_kg * alas_units::STANDARD_GRAVITY;
    let area = analyses.reference_area_m2;
    let cl_upper = cl_max_clean / (STALL_MARGIN * STALL_MARGIN);
    let mut best: Option<(f64, f64)> = None;
    for step in 0..SCAN_POINTS {
        let cl = cl_upper * (1.0 - step as f64 / SCAN_POINTS as f64);
        if cl <= 0.0 {
            break;
        }
        let speed = (2.0 * weight_n / (atmosphere.density_kg_m3 * area * cl)).sqrt();
        let mach = speed / atmosphere.speed_of_sound_m_s;
        let reynolds_per_m = atmosphere.density_kg_m3 * speed / atmosphere.dynamic_viscosity_pa_s;
        let drag = model.coefficients(cl, mach, reynolds_per_m);
        let cd = drag.parasite + drag.induced + drag.wave;
        if !cd.is_finite() || cd <= 0.0 {
            continue;
        }
        let merit = if turboprop {
            cl.powf(1.5) / cd
        } else {
            cl / cd
        };
        if best.is_none_or(|(_, top)| merit > top) {
            best = Some((speed, merit));
        } else if best.is_some_and(|(_, top)| merit < STOP_FRACTION * top) {
            break;
        }
    }
    best.map(|(speed, _)| speed)
}
