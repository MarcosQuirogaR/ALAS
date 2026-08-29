// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from the reference mission turbofan component network and sizing
// routine. The implementation is retained as an independent analysis path.
// Reference: alas @ rust-port-baseline.

//! The mission turbofan cycle at the single flight condition
//! `turbofan_sizing` -- a second, structurally different turbofan model from
//! [`crate::cycle`].
//!
//! [`crate::cycle`] is *this program's own* separate-flow on-design cycle
//! (`alas/physics/propulsion.py`). This module is reached only when the
//! mission runner sizes an engine onto a built vehicle. The network consists
//! of a ram, a compression inlet, low- and high-pressure
//! compressors, a fan, a combustor, high- and low-pressure turbines, core and
//! fan expansion nozzles, and a thrust process, then calls
//! `turbofan_sizing(turbofan, cruise_mach, cruise_altitude)` once. The two
//! models are deliberately *not* unified -- they use different station
//! numbering, different gas-property fits and a different thrust equation --
//! so a disagreement is unambiguous.
//!
//! # Scope: one sizing call, not the per-timestep mission use
//!
//! [`size_turbofan`] reproduces the established sizing sequence: it walks
//! the network once at the cruise design point and runs `Thrust.size` to back
//! the core mass flow out of the design thrust, then replays the network at
//! sea-level-static through the network evaluator -- reusing the
//! core-flow scale factor the cruise pass solved -- to report the
//! sea-level-static thrust. That single-flight-condition call is reachable
//! standalone and is this row's whole scope.
//!
//! [`evaluate_thrust`] is the same network walk at *any* flight condition, and
//! it is what a mission segment calls once per control point: it reads the
//! segment's own freestream rather than a design point, reuses the
//! `compressor_nondimensional_massflow` the sizing pass solved, and multiplies
//! the dimensional thrust by the segment's throttle -- one of the two unknowns
//! the segment solves for. It takes its freestream as data
//! ([`freestream_from_atmosphere`] builds one) rather than deriving an
//! atmosphere of its own, for the reason `alas-aero::drag_buildup`'s row
//! records: a parity test must be handed the inputs the reference used. The
//! vectorization across control points is the caller's; each point is one
//! scalar call.
//!
//! # Component parameters are `vehicle_builder`'s, not the engine spec's
//!
//! The variable engine inputs ([`TurbofanInputs`]) are what a design carries:
//! bypass ratio, overall pressure ratio, fan pressure ratio, turbine inlet
//! temperature, number of engines, the cruise point and the design thrust.
//! Every component *efficiency* and *pressure loss* is a fixed textbook value
//! `vehicle_builder.py` hardcodes when it assembles the network, so those are
//! [`VehicleBuilderParams`] constants here rather than inputs, carrying the
//! line each came from.

use network::{build_freestream, walk_network};

pub mod components;
pub mod network;
pub mod types;

pub use network::freestream_from_atmosphere;
pub use types::{
    CombustorOutput, CompressionNozzleOutput, CompressorOutput, ExpansionNozzleOutput, Freestream,
    RamOutput, StationSet, ThrustOutput, TurbineOutput, TurbofanSizingResult,
};

/// Air's specific gas constant, m^2/(s^2*K).
/// Air's gas-specific constant used by the ram component.
const GAS_CONSTANT_AIR: f64 = 287.052_874_2;

/// Jet-A specific energy (lower heating value), J/kg.
/// Jet-A lower heating value used by the combustor.
const JET_A_SPECIFIC_ENERGY_J_KG: f64 = 43.02e6;

/// Standard sea-level gravity, m/s^2. `Attributes.Planets.Earth.sea_level_gravity`.
const SEA_LEVEL_GRAVITY: f64 = 9.806_65;

/// Earth's mean radius, m. `Attributes.Planets.Earth.mean_radius`.
const EARTH_MEAN_RADIUS_M: f64 = 6.371e6;

/// The throttle both sizing passes run at. `Thrust.size` sets `throttle = 1.0`
/// outright, and `turbofan_sizing`'s sea-level-static replay evaluates the
/// network on a conditions object whose throttle it has just set to one.
const SIZING_THROTTLE: f64 = 1.0;

/// Freestream Mach the sea-level-static replay uses, from `turbofan_sizing.py`.
///
/// Not zero: the thrust equation divides by `gamma * M0`, so the static rating
/// is evaluated at a small but nonzero Mach (upstream's `np.atleast_2d(0.01)`).
const SEA_LEVEL_STATIC_MACH: f64 = 0.01;

/// `Attributes.Planets.Earth.compute_gravity`: `g0 * (Re / (Re + H))^2`.
fn compute_gravity(altitude_m: f64) -> f64 {
    SEA_LEVEL_GRAVITY * (EARTH_MEAN_RADIUS_M / (EARTH_MEAN_RADIUS_M + altitude_m)).powi(2)
}

/// The fixed component efficiencies and pressure losses `vehicle_builder.py`
/// assembles the `Turbofan` network with, each with its source line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VehicleBuilderParams {
    /// `inlet_nozzle.pressure_ratio = 0.98`.
    pub inlet_pressure_ratio: f64,
    /// `inlet_nozzle.polytropic_efficiency = 0.98`.
    pub inlet_polytropic_efficiency: f64,
    /// `Compression_Nozzle.__defaults__.pressure_recovery = 1.0` (never overridden).
    pub inlet_pressure_recovery: f64,
    /// The fixed low-pressure-compressor pressure ratio, `lpc_pr = 1.20`.
    pub lpc_pressure_ratio: f64,
    /// `low_pressure_compressor.polytropic_efficiency = 0.91`.
    pub lpc_polytropic_efficiency: f64,
    /// `high_pressure_compressor.polytropic_efficiency = 0.93` (its pressure
    /// ratio is backed out of the overall ratio; see [`TurbofanInputs`]).
    pub hpc_polytropic_efficiency: f64,
    /// `fan.polytropic_efficiency = 0.93`.
    pub fan_polytropic_efficiency: f64,
    /// `combustor.pressure_ratio = 0.95`.
    pub combustor_pressure_ratio: f64,
    /// `combustor.efficiency = 0.99`.
    pub combustor_efficiency: f64,
    /// `low/high_pressure_turbine.mechanical_efficiency = 0.99`.
    pub turbine_mechanical_efficiency: f64,
    /// `low/high_pressure_turbine.polytropic_efficiency = 0.95`.
    pub turbine_polytropic_efficiency: f64,
    /// `core_nozzle.pressure_ratio = 0.99`.
    pub core_nozzle_pressure_ratio: f64,
    /// `core_nozzle.polytropic_efficiency = 0.95`.
    pub core_nozzle_polytropic_efficiency: f64,
    /// `fan_nozzle.pressure_ratio = 0.99`.
    pub fan_nozzle_pressure_ratio: f64,
    /// `fan_nozzle.polytropic_efficiency = 0.95`.
    pub fan_nozzle_polytropic_efficiency: f64,
}

impl Default for VehicleBuilderParams {
    /// The exact values `vehicle_builder.build_vehicle` sets on the network.
    fn default() -> Self {
        Self {
            inlet_pressure_ratio: 0.98,
            inlet_polytropic_efficiency: 0.98,
            inlet_pressure_recovery: 1.0,
            lpc_pressure_ratio: 1.20,
            lpc_polytropic_efficiency: 0.91,
            hpc_polytropic_efficiency: 0.93,
            fan_polytropic_efficiency: 0.93,
            combustor_pressure_ratio: 0.95,
            combustor_efficiency: 0.99,
            turbine_mechanical_efficiency: 0.99,
            turbine_polytropic_efficiency: 0.95,
            core_nozzle_pressure_ratio: 0.99,
            core_nozzle_polytropic_efficiency: 0.95,
            fan_nozzle_pressure_ratio: 0.99,
            fan_nozzle_polytropic_efficiency: 0.95,
        }
    }
}

/// The design a `turbofan_sizing` call is handed: the variable engine numbers,
/// the cruise design point, and the design thrust to size against.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurbofanInputs {
    /// `turbofan.number_of_engines`.
    pub number_of_engines: f64,
    /// `turbofan.bypass_ratio`.
    pub bypass_ratio: f64,
    /// The engine's published overall pressure ratio. The high-pressure
    /// compressor ratio is `max(overall / lpc_pressure_ratio, 1.0)`, exactly
    /// as `vehicle_builder.py` backs it out.
    pub overall_pressure_ratio: f64,
    /// `fan.pressure_ratio`.
    pub fan_pressure_ratio: f64,
    /// `combustor.turbine_inlet_temperature`, K.
    pub turbine_inlet_temperature_k: f64,
    /// The cruise design Mach the engine is sized at.
    pub cruise_mach: f64,
    /// The cruise design geometric altitude, m.
    pub cruise_altitude_m: f64,
    /// `thrust.total_design`, N -- the *total* (all-engine) design thrust,
    /// which `vehicle_builder.py` sets to the cruise-required thrust, not the
    /// sea-level-static rating.
    pub design_thrust_total_n: f64,
}

/// Walk the network at one flight condition, as `Turbofan.evaluate_thrust`
/// does once per mission control point.
///
/// This is the same walk [`size_turbofan`] runs twice, with the two things a
/// mission supplies that a sizing call does not: the
/// `compressor_nondimensional_massflow` a previous sizing pass solved (the
/// engine is already sized by the time a mission flies it), and the segment's
/// `throttle`.
///
/// The returned [`ThrustOutput`] is the whole `thrust.outputs` bag, not only
/// the two numbers `update_thrust` packs onto the conditions
/// (`thrust_n` becomes the body-frame thrust force's x component, and
/// `fuel_flow_rate_kg_s` the vehicle mass rate), because a segment that
/// disagrees is easier to diagnose from the station it disagreed at than from
/// the force it produced.
pub fn evaluate_thrust(
    freestream: &Freestream,
    inputs: &TurbofanInputs,
    params: &VehicleBuilderParams,
    compressor_nondimensional_massflow: f64,
    throttle: f64,
) -> ThrustOutput {
    let (_ram, _inlet, lpc, _hpc, _fan, combustor, _hpt, _lpt, core_nozzle, fan_nozzle) =
        walk_network(freestream, inputs, params);

    components::compute_thrust(
        freestream,
        &core_nozzle,
        &fan_nozzle,
        combustor.fuel_to_air_ratio,
        lpc.stagnation_temperature_k,
        lpc.stagnation_pressure_pa,
        inputs.bypass_ratio,
        inputs.number_of_engines,
        compressor_nondimensional_massflow,
        throttle,
    )
}

/// Size the turbofan onto its design point, as `turbofan_sizing` does.
///
/// `params` supplies the fixed component efficiencies and losses; callers
/// reproduce `vehicle_builder.py` with [`VehicleBuilderParams::default`].
pub fn size_turbofan(
    inputs: &TurbofanInputs,
    params: &VehicleBuilderParams,
) -> TurbofanSizingResult {
    // -- cruise design-point pass --------------------------------------------
    let cruise_fs = build_freestream(
        inputs.cruise_altitude_m,
        inputs.cruise_mach,
        compute_gravity(inputs.cruise_altitude_m),
    );
    let (
        cruise_ram,
        cruise_inlet,
        cruise_lpc,
        cruise_hpc,
        cruise_fan,
        cruise_comb,
        cruise_hpt,
        cruise_lpt,
        cruise_core_nozzle,
        cruise_fan_nozzle,
    ) = walk_network(&cruise_fs, inputs, params);

    // `Thrust.size` runs `compute` first, with the scale factor still zero
    // (so the reported cruise thrust/mass-flow/power are zero), then backs the
    // core flow out of the design thrust.
    let cruise_thrust = components::compute_thrust(
        &cruise_fs,
        &cruise_core_nozzle,
        &cruise_fan_nozzle,
        cruise_comb.fuel_to_air_ratio,
        cruise_lpc.stagnation_temperature_k,
        cruise_lpc.stagnation_pressure_pa,
        inputs.bypass_ratio,
        inputs.number_of_engines,
        0.0,
        SIZING_THROTTLE,
    );
    let (mass_flow_rate_design_kg_s, compressor_nondimensional_massflow) =
        components::size_core_flow(
            inputs.design_thrust_total_n,
            cruise_thrust.non_dimensional_thrust,
            cruise_fs.speed_of_sound_m_s,
            inputs.bypass_ratio,
            inputs.number_of_engines,
            cruise_lpc.stagnation_temperature_k,
            cruise_lpc.stagnation_pressure_pa,
        );

    let cruise = StationSet {
        freestream: cruise_fs,
        ram: cruise_ram,
        inlet_nozzle: cruise_inlet,
        low_pressure_compressor: cruise_lpc,
        high_pressure_compressor: cruise_hpc,
        fan: cruise_fan,
        combustor: cruise_comb,
        high_pressure_turbine: cruise_hpt,
        low_pressure_turbine: cruise_lpt,
        core_nozzle: cruise_core_nozzle,
        fan_nozzle: cruise_fan_nozzle,
        thrust: cruise_thrust,
    };

    // -- sea-level-static replay ---------------------------------------------
    let sls_fs = build_freestream(0.0, SEA_LEVEL_STATIC_MACH, SEA_LEVEL_GRAVITY);
    let (
        sls_ram,
        sls_inlet,
        sls_lpc,
        sls_hpc,
        sls_fan,
        sls_comb,
        sls_hpt,
        sls_lpt,
        sls_core_nozzle,
        sls_fan_nozzle,
    ) = walk_network(&sls_fs, inputs, params);

    // The replay reuses the core-flow scale factor the cruise pass solved.
    let sls_thrust = components::compute_thrust(
        &sls_fs,
        &sls_core_nozzle,
        &sls_fan_nozzle,
        sls_comb.fuel_to_air_ratio,
        sls_lpc.stagnation_temperature_k,
        sls_lpc.stagnation_pressure_pa,
        inputs.bypass_ratio,
        inputs.number_of_engines,
        compressor_nondimensional_massflow,
        SIZING_THROTTLE,
    );

    let sea_level_static = StationSet {
        freestream: sls_fs,
        ram: sls_ram,
        inlet_nozzle: sls_inlet,
        low_pressure_compressor: sls_lpc,
        high_pressure_compressor: sls_hpc,
        fan: sls_fan,
        combustor: sls_comb,
        high_pressure_turbine: sls_hpt,
        low_pressure_turbine: sls_lpt,
        core_nozzle: sls_core_nozzle,
        fan_nozzle: sls_fan_nozzle,
        thrust: sls_thrust,
    };

    TurbofanSizingResult {
        design_thrust_n: inputs.design_thrust_total_n,
        mass_flow_rate_design_kg_s,
        compressor_nondimensional_massflow,
        sealevel_static_thrust_n_per_engine: sls_thrust.thrust_n / inputs.number_of_engines,
        cruise,
        sea_level_static,
        sea_level_static_thrust_force_n: sls_thrust.thrust_n,
        sea_level_static_vehicle_mass_rate_kg_s: sls_thrust.fuel_flow_rate_kg_s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ave_inputs() -> TurbofanInputs {
        TurbofanInputs {
            number_of_engines: 2.0,
            bypass_ratio: 10.0,
            overall_pressure_ratio: 60.0,
            fan_pressure_ratio: 1.45,
            turbine_inlet_temperature_k: 1670.0,
            cruise_mach: 0.84,
            cruise_altitude_m: 11887.2,
            design_thrust_total_n: 197136.97010079,
        }
    }

    #[test]
    fn station_temperatures_rise_through_compression_and_the_burner_then_fall() {
        let result = size_turbofan(&ave_inputs(), &VehicleBuilderParams::default());
        let c = &result.cruise;
        // Compression heats the flow; the burner pins Tt4; the turbines cool it.
        assert!(
            c.inlet_nozzle.stagnation_temperature_k
                < c.low_pressure_compressor.stagnation_temperature_k
        );
        assert!(
            c.low_pressure_compressor.stagnation_temperature_k
                < c.high_pressure_compressor.stagnation_temperature_k
        );
        assert!(
            c.high_pressure_compressor.stagnation_temperature_k
                < c.combustor.stagnation_temperature_k
        );
        assert!(
            c.high_pressure_turbine.stagnation_temperature_k < c.combustor.stagnation_temperature_k
        );
        assert!(
            c.low_pressure_turbine.stagnation_temperature_k
                < c.high_pressure_turbine.stagnation_temperature_k
        );
    }

    #[test]
    fn the_cruise_sizing_pass_reports_zero_dimensional_thrust_but_positive_specific_thrust() {
        // `Thrust.size` runs `compute` before the scale factor is solved, so
        // dimensional quantities are zero while the specific thrust is not.
        let result = size_turbofan(&ave_inputs(), &VehicleBuilderParams::default());
        assert_eq!(result.cruise.thrust.thrust_n, 0.0);
        assert_eq!(result.cruise.thrust.core_mass_flow_rate_kg_s, 0.0);
        assert_eq!(result.cruise.thrust.power_w, 0.0);
        assert!(result.cruise.thrust.non_dimensional_thrust > 0.0);
        assert!(result.cruise.thrust.thrust_specific_fuel_consumption > 0.0);
    }

    #[test]
    fn the_sea_level_static_rating_exceeds_the_per_engine_cruise_design_thrust() {
        // The engine is sized on cruise thrust; its static rating is larger.
        let inputs = ave_inputs();
        let result = size_turbofan(&inputs, &VehicleBuilderParams::default());
        let per_engine_cruise = inputs.design_thrust_total_n / inputs.number_of_engines;
        assert!(result.sealevel_static_thrust_n_per_engine > per_engine_cruise);
        // The two ways of reading the static thrust are consistent.
        assert!(
            (result.sea_level_static_thrust_force_n
                - result.sealevel_static_thrust_n_per_engine * inputs.number_of_engines)
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn both_nozzles_choke_at_the_cruise_design_point() {
        // At M0.84 the pressure ratios push both nozzles to Mach 1.
        let result = size_turbofan(&ave_inputs(), &VehicleBuilderParams::default());
        assert_eq!(result.cruise.core_nozzle.mach, 1.0);
        assert_eq!(result.cruise.fan_nozzle.mach, 1.0);
    }

    // The mission entry has to agree with the sizing entry where the two
    // overlap: replaying the sea-level-static point through `evaluate_thrust`
    // with the solved scale factor and a throttle of one is exactly what
    // `turbofan_sizing`'s second pass is.
    #[test]
    fn evaluate_thrust_at_the_sizing_point_reproduces_the_static_rating() {
        let inputs = ave_inputs();
        let params = VehicleBuilderParams::default();
        let sized = size_turbofan(&inputs, &params);
        let replay = evaluate_thrust(
            &sized.sea_level_static.freestream,
            &inputs,
            &params,
            sized.compressor_nondimensional_massflow,
            1.0,
        );
        assert_eq!(replay.thrust_n, sized.sea_level_static_thrust_force_n);
        assert_eq!(
            replay.fuel_flow_rate_kg_s,
            sized.sea_level_static_vehicle_mass_rate_kg_s
        );
    }

    // Throttle multiplies the dimensional thrust and leaves every specific
    // quantity alone, which is the whole of what the segment unknown does.
    #[test]
    fn throttle_scales_the_dimensional_thrust_and_not_the_specific_quantities() {
        let inputs = ave_inputs();
        let params = VehicleBuilderParams::default();
        let sized = size_turbofan(&inputs, &params);
        let mdhc = sized.compressor_nondimensional_massflow;
        let full = evaluate_thrust(&sized.cruise.freestream, &inputs, &params, mdhc, 1.0);
        let half = evaluate_thrust(&sized.cruise.freestream, &inputs, &params, mdhc, 0.5);
        assert_eq!(half.thrust_n, full.thrust_n * 0.5);
        assert_eq!(
            half.thrust_specific_fuel_consumption,
            full.thrust_specific_fuel_consumption
        );
        assert_eq!(half.non_dimensional_thrust, full.non_dimensional_thrust);
        assert_eq!(half.core_mass_flow_rate_kg_s, full.core_mass_flow_rate_kg_s);
    }

    #[test]
    fn the_high_pressure_compressor_ratio_is_backed_out_of_the_overall_ratio() {
        // OPR 60 split against a fixed LPC ratio of 1.20 leaves the HPC at 50,
        // so the HPC discharge pressure is the LPC discharge times 50.
        let result = size_turbofan(&ave_inputs(), &VehicleBuilderParams::default());
        let c = &result.cruise;
        let ratio = c.high_pressure_compressor.stagnation_pressure_pa
            / c.low_pressure_compressor.stagnation_pressure_pa;
        assert!((ratio - 50.0).abs() < 1e-9);
    }
}
