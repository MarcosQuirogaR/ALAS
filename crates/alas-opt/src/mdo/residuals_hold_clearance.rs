// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The wing-body junction of a redesigned registered aircraft against the
//! lower holds its preset declares by station.
//!
//! The holds stop at the centre wing box and the main-gear bay behind it,
//! the stretch `alas_payload` `CabinGeometry::lower_hold_gap_x_range` keeps
//! clear: from the front spar (the first entry of the default spar list) to
//! the side-of-body trailing edge. A declared hold sits at its printed
//! stations whatever the wing does, so a wing that moves or grows aft would
//! put its box or gear bay inside a printed compartment while the payload
//! layout still loads it.
//!
//! The registered aircraft's own drawn stretch already reaches into some
//! printed compartments (A380-800: 1.45 m into the forward and 3.38 m into
//! the aft hold, B787-9 0.43 m and A350-900 0.86 m into the aft hold):
//! the side-of-body trailing edge is the end of the fairing plan, not of the
//! bay, and the hold starts carry about 0.3 m of read uncertainty. The row
//! therefore holds every candidate to the registered aircraft's intrusion
//! per compartment, `max(0, registered)`, and counts only what the
//! candidate adds to it.
//!
//! Frame: body axes, x aft from the nose tip, m. Feasible iff the raw
//! residual is at most zero.

use std::sync::OnceLock;

use alas_config::{AlasConfig, HoldDeck};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;

use super::types::ConstraintFamily::Geometry;
use super::types::{ConstraintResidual, ResidualRole, NUMERICAL_SLACK};

/// A lower-hold gap `(start, end)`, m.
type GapStations = (f64, f64);

/// The lower-hold gap `(start, end)` of the built aircraft, m.
fn gap(plane: &Airplane) -> Option<GapStations> {
    let (le, chord) = plane.wings.first()?.carried_root();
    let front_spar = alas_config::StructuresConfig::default()
        .spar_chord_fractions
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
    let front_spar = if (0.0..1.0).contains(&front_spar) {
        front_spar
    } else {
        0.0
    };
    (le[0].is_finite() && chord > 0.0).then(|| (le[0] + front_spar * chord, le[0] + chord))
}

/// The registered aircraft's gap, built once per preset.
fn registered_gap(preset: &str) -> Option<GapStations> {
    static GAPS: OnceLock<Vec<(&'static str, Option<GapStations>)>> = OnceLock::new();
    GAPS.get_or_init(|| {
        alas_config::presets::registry()
            .iter()
            .map(|registered| {
                let plane = AircraftBuilder::new(Some(registered.geometry.clone()))
                    .build(Some(&registered.design_vector), false)
                    .ok();
                (registered.name, plane.as_ref().and_then(gap))
            })
            .collect()
    })
    .iter()
    .find(|(name, _)| *name == preset)
    .and_then(|(_, gap)| *gap)
}

/// How far `gap` reaches into the compartment `[start, end]`, m (negative:
/// the clearance between them).
fn intrusion((gap_start, gap_end): (f64, f64), (start, end): (f64, f64)) -> f64 {
    gap_end.min(end) - gap_start.max(start)
}

/// `wing_box_clear_of_lower_holds`: the largest intrusion of the candidate's
/// box and gear bay into a declared lower hold beyond the registered
/// aircraft's own, m. Empty for a configuration without a registered wing
/// position to hold (`AlasConfig::derives_wing_position`) or without
/// declared lower holds.
pub(super) fn hold_clearance_residuals(
    plane: &Airplane,
    config: &AlasConfig,
    role: ResidualRole,
) -> Vec<ConstraintResidual> {
    if !config.derives_wing_position() {
        return Vec::new();
    }
    let holds: Vec<(f64, f64)> = config
        .cabin
        .cargo
        .hold_compartments
        .iter()
        .filter(|hold| hold.is_valid() && hold.deck == HoldDeck::Lower)
        .map(|hold| (hold.x_start_m, hold.x_end_m))
        .collect();
    let (Some(candidate), Some(registered)) = (gap(plane), registered_gap(&config.preset)) else {
        return Vec::new();
    };
    let Some(added) = holds
        .iter()
        .map(|&hold| intrusion(candidate, hold) - intrusion(registered, hold).max(0.0))
        .reduce(f64::max)
    else {
        return Vec::new();
    };
    // Normalised by the registered box-and-bay length: the limit is zero.
    let length = (registered.1 - registered.0).max(f64::MIN_POSITIVE);
    vec![ConstraintResidual::direct(
        "wing_box_clear_of_lower_holds",
        Geometry,
        added,
        0.0,
        "m",
        added,
        (added / length - NUMERICAL_SLACK).max(0.0),
        role,
    )]
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::{hold_clearance_residuals, intrusion, ResidualRole};
    use alas_config::AlasConfig;
    use alas_geom::builder::AircraftBuilder;

    #[test]
    fn every_registered_aircraft_meets_its_own_hold_clearance() {
        let mut checked = 0;
        for registered in alas_config::presets::registry() {
            let config =
                AlasConfig::from_value(&serde_json::json!({ "preset": registered.name })).unwrap();
            let plane = AircraftBuilder::new(Some(config.geometry.clone()))
                .build(Some(&registered.design_vector), false)
                .unwrap();
            for row in hold_clearance_residuals(&plane, &config, ResidualRole::Constraint) {
                assert!(
                    row.raw_residual <= 1.0e-9,
                    "{}: {}",
                    registered.name,
                    row.raw_residual
                );
                checked += 1;
            }
        }
        assert!(checked >= 6, "{checked} presets declare lower holds");
    }

    #[test]
    fn an_intrusion_is_the_overlap_and_a_clearance_is_negative() {
        assert!((intrusion((10.0, 14.0), (13.0, 20.0)) - 1.0).abs() < 1e-12);
        assert!((intrusion((10.0, 14.0), (15.5, 20.0)) + 1.5).abs() < 1e-12);
        assert!((intrusion((10.0, 14.0), (2.0, 9.0)) + 1.0).abs() < 1e-12);
    }
}
