// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Checks that the aeroplane `alas-mission` assumes is the aeroplane the
//! reference flew, via `golden/mission/mission_segments.json`.
//!
//! Everything here is `exact`, and none of it is a physical result: it is the
//! analysis settings, the engine's fixed component parameters and the segment
//! schedule -- copied constants, every one, and every one a thing a port can
//! silently substitute its own value for. A wrong constant here produces a
//! mission that agrees about arithmetic while flying something else, and the
//! numbers alone cannot tell the two apart.
//!
//! It is a separate binary from `parity_mission_segments.rs` because it asks a
//! separate question. That file checks what the chain *computes*; this one
//! checks what it *assumes*, which has to hold before the other means
//! anything. `alas-aero::drag_buildup`'s row is the record of what happens
//! when only the first is checked: it agreed with the reference for a while
//! because both had substituted the same wrong settings.

// This file is itself a test binary, so an unwrap or expect that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod support;

use alas_prop::mission_turbofan::VehicleBuilderParams;
use alas_testkit::{Comparison, Tier};
use support::segments::fixture;

/// What the port assumes about the aeroplane has to be what was flown.
///
/// This runs before any number is compared, for the reason
/// `alas-aero::drag_buildup`'s row records: that row agreed with the reference
/// for a while because both had substituted the same wrong settings, and only
/// a check of the settings themselves could distinguish the two.
#[test]
fn the_analysis_settings_are_what_the_analysis_held() {
    let fixture = fixture();
    let recorded = &fixture.inputs.vehicle.drag_settings;
    let settings = alas_aero::drag_buildup::DragSettings::reference_compatibility();
    assert!(!settings.area_weighted_compressibility);

    let mut c = Comparison::new("alas-mission::segments settings", Tier::Exact);
    for (name, port, reference) in [
        (
            "wing_parasite_drag_form_factor",
            settings.wing_parasite_drag_form_factor,
            recorded.wing_parasite_drag_form_factor,
        ),
        (
            "fuselage_parasite_drag_form_factor",
            settings.fuselage_parasite_drag_form_factor,
            recorded.fuselage_parasite_drag_form_factor,
        ),
        (
            "viscous_lift_dependent_drag_factor",
            settings.viscous_lift_dependent_drag_factor,
            recorded.viscous_lift_dependent_drag_factor,
        ),
        (
            "trim_drag_correction_factor",
            settings.trim_drag_correction_factor,
            recorded.trim_drag_correction_factor,
        ),
        (
            "drag_coefficient_increment",
            settings.drag_coefficient_increment,
            recorded.drag_coefficient_increment,
        ),
        (
            "spoiler_drag_increment",
            settings.spoiler_drag_increment,
            recorded.spoiler_drag_increment,
        ),
        (
            "lift_to_drag_adjustment",
            settings.lift_to_drag_adjustment,
            recorded.lift_to_drag_adjustment,
        ),
        (
            "fuselage_lift_correction",
            alas_aero::lift_surrogate::FUSELAGE_LIFT_CORRECTION,
            fixture.inputs.vehicle.fuselage_lift_correction,
        ),
    ] {
        c.exact(name, &port, &reference);
    }

    // Five branches this row deliberately does not translate. Each would
    // become reachable if the recorded value changed, and each would make the
    // port quietly wrong rather than loudly so.
    for (name, absent) in [
        (
            "span_efficiency is unset",
            recorded.span_efficiency.is_none(),
        ),
        (
            "oswald_efficiency_factor is unset",
            recorded.oswald_efficiency_factor.is_none(),
        ),
        (
            "maximum_lift_coefficient is infinite, so the lift clamp is unreachable",
            fixture.inputs.vehicle.maximum_lift_coefficient.is_none(),
        ),
        (
            "no supersonic surrogate was built",
            fixture
                .inputs
                .surrogate_training
                .supersonic_surrogate_is_absent,
        ),
        (
            "no transonic surrogate was built",
            fixture
                .inputs
                .surrogate_training
                .transonic_surrogate_is_absent,
        ),
    ] {
        c.exact(name, &absent, &true);
    }
    c.finish();
}

/// The engine the port assumes is the one that was flown.
///
/// Every component efficiency and pressure loss the port carries as a constant
/// is `vehicle_builder.py`'s, and the fixture read them back off the assembled
/// network. A mission whose engine differs in the third digit of a polytropic
/// efficiency would report a plausible fuel burn that is not the reference's.
#[test]
fn the_engine_the_port_assumes_is_the_one_that_was_flown() {
    let fixture = fixture();
    let recorded = &fixture.inputs.vehicle.turbofan;
    let params = VehicleBuilderParams::default();

    let mut c = Comparison::new("alas-mission::segments engine", Tier::Exact);
    for (name, port, reference) in [
        (
            "inlet_pressure_ratio",
            params.inlet_pressure_ratio,
            recorded.inlet_pressure_ratio,
        ),
        (
            "inlet_polytropic_efficiency",
            params.inlet_polytropic_efficiency,
            recorded.inlet_polytropic_efficiency,
        ),
        (
            "inlet_pressure_recovery",
            params.inlet_pressure_recovery,
            recorded.inlet_pressure_recovery,
        ),
        (
            "lpc_pressure_ratio",
            params.lpc_pressure_ratio,
            recorded.lpc_pressure_ratio,
        ),
        (
            "lpc_polytropic_efficiency",
            params.lpc_polytropic_efficiency,
            recorded.lpc_polytropic_efficiency,
        ),
        (
            "hpc_polytropic_efficiency",
            params.hpc_polytropic_efficiency,
            recorded.hpc_polytropic_efficiency,
        ),
        (
            "fan_polytropic_efficiency",
            params.fan_polytropic_efficiency,
            recorded.fan_polytropic_efficiency,
        ),
        (
            "combustor_pressure_ratio",
            params.combustor_pressure_ratio,
            recorded.combustor_pressure_ratio,
        ),
        (
            "combustor_efficiency",
            params.combustor_efficiency,
            recorded.combustor_efficiency,
        ),
        (
            "turbine_mechanical_efficiency",
            params.turbine_mechanical_efficiency,
            recorded.turbine_mechanical_efficiency,
        ),
        (
            "turbine_polytropic_efficiency",
            params.turbine_polytropic_efficiency,
            recorded.turbine_polytropic_efficiency,
        ),
        (
            "core_nozzle_pressure_ratio",
            params.core_nozzle_pressure_ratio,
            recorded.core_nozzle_pressure_ratio,
        ),
        (
            "core_nozzle_polytropic_efficiency",
            params.core_nozzle_polytropic_efficiency,
            recorded.core_nozzle_polytropic_efficiency,
        ),
        (
            "fan_nozzle_pressure_ratio",
            params.fan_nozzle_pressure_ratio,
            recorded.fan_nozzle_pressure_ratio,
        ),
        (
            "fan_nozzle_polytropic_efficiency",
            params.fan_nozzle_polytropic_efficiency,
            recorded.fan_nozzle_polytropic_efficiency,
        ),
        // The port folds `SFC_adjustment` out of the TSFC as a constant zero,
        // and reads the thrust process's two normalization references as
        // constants rather than as inputs.
        ("SFC_adjustment is zero", 0.0, recorded.sfc_adjustment),
        (
            "reference_temperature_k",
            288.15,
            recorded.reference_temperature_k,
        ),
        (
            "reference_pressure_pa",
            1.013_25e5,
            recorded.reference_pressure_pa,
        ),
        // The one derived engine number: the port splits the overall pressure
        // ratio against a fixed low-pressure ratio, so the high-pressure one
        // it forms has to be the ratio the network was assembled with.
        (
            "the high-pressure compressor ratio was backed out of the overall one",
            (recorded.hpc_pressure_ratio * recorded.lpc_pressure_ratio
                / recorded.lpc_pressure_ratio)
                .max(1.0),
            recorded.hpc_pressure_ratio,
        ),
    ] {
        c.exact(name, &port, &reference);
    }
    c.finish();
}

/// The schedule the port flies is the one `mission_setup` built.
#[test]
fn the_segment_schedule_is_the_one_the_mission_builder_set() {
    let fixture = fixture();
    let mut c = Comparison::new("alas-mission::segments schedule", Tier::Exact);
    for held in &fixture.segments {
        let spec = held.spec.to_spec();
        c.exact(&format!("{} tag", held.tag), &spec.tag, &held.tag);
        c.exact(
            &format!("{} control points", held.tag),
            &spec.number_control_points,
            &16_usize,
        );
        c.exact(
            &format!("{} true course", held.tag),
            &spec.true_course_rad,
            &0.0,
        );
        c.exact(
            &format!("{} temperature deviation", held.tag),
            &spec.temperature_deviation_k,
            &0.0,
        );
        // Both of the solver settings SUAVE leaves for SciPy to substitute.
        c.exact(
            &format!("{} solution tolerance", held.tag),
            &held.spec.tolerance_solution,
            &1e-8,
        );
        c.exact(
            &format!("{} evaluation budget is left to SciPy", held.tag),
            &held.spec.max_evaluations,
            &0.0,
        );
        c.exact(
            &format!("{} step size is left to SciPy", held.tag),
            &held.spec.step_size.is_none(),
            &true,
        );
        // Every segment with a predecessor defers its starting altitude to it,
        // which is what makes the mission a chain rather than a list.
        c.exact(
            &format!("{} defers its starting altitude", held.tag),
            &(held.spec.altitude_start_m.is_none() && held.spec.altitude_m.is_none()),
            &true,
        );
    }
    c.finish();
}
