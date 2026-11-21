// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The all-coach passenger capacity of a cabin shell: the `Np` of Beltramo,
//! Trapp, Kimoto and Marsh, *Parametric Study of Transport Aircraft Systems
//! Cost and Weight*, NASA CR-151970 (1977), whose systems weight relations
//! read it (the electrical group in `alas_mass`).
//!
//! The report normalises every cabin to the passengers it "could carry"
//! with "all coach seating at a seat pitch of 34 inches", at the cabin's
//! coach abreast (p. 5-12). Here that is the capacity of the shell itself,
//! not of the class mix or the count a study requests: the auto-sizer
//! ([`crate::build::simulate_passenger_counts`]) seats the whole cabin in
//! economy at 34 in, with the economy seat of the cabin configuration, the
//! same monument bays and exit-derived deck caps the detailed engine applies,
//! and no planning or certified seat cap. A cabin declared below its capacity
//! therefore has the `Np` of the full cabin.
//!
//! A converted freighter keeps the systems of the airliner it was built from
//! (`converted_freighter_seats`), so its `Np` is that airliner's: the same
//! shell with the airliner's declared exit arrangement. A production
//! freighter, a cargo aircraft and a custom payload without seats have none.

use alas_config::AlasConfig;

use crate::geometry::CabinGeometry;

/// The seat pitch NASA CR-151970 normalises every cabin to, m: 34 in.
pub const ALL_COACH_PITCH_M: f64 = 34.0 * 0.0254;

/// The passengers the shell `g` of `config` carries all-coach at 34 in pitch
/// (NASA CR-151970 `Np`), or `None` for an aircraft without a passenger cabin.
/// `g` is the product cabin frame the layout was built on.
pub(crate) fn all_coach_capacity(config: &AlasConfig, g: &CabinGeometry) -> Option<usize> {
    if config.cabin.payload_items.custom_only {
        return None;
    }
    let passenger = config.requirements.aircraft_type == "passenger";
    let exit_layout = if passenger {
        crate::build::registered_source_exit_layout(config, false)
    } else if config.requirements.aircraft_type == "cargo"
        && config.cabin.cargo.converted_freighter_seats.is_some()
    {
        // The freighter frame has no declared passenger doors; the airliner's
        // are those of the same registered aircraft.
        alas_config::presets::get(&config.preset)
            .ok()
            .and_then(|preset| preset.reference.certified_exit_layout)
    } else {
        return None;
    };
    let airliner;
    let shell = if passenger {
        g
    } else {
        airliner = g.clone().with_declared_doors(exit_layout.as_ref());
        &airliner
    };
    let mut coach = config.cabin.passenger.clone();
    coach.economy.pitch_m = ALL_COACH_PITCH_M;
    let mix = [("Economy", 1.0)];
    let seats = crate::build::simulate_passenger_counts_with_exit_semantics(
        shell,
        &coach,
        &mix,
        true,
        None,
        exit_layout,
    )
    .total();
    usize::try_from(seats).ok().filter(|seats| *seats > 0)
}
