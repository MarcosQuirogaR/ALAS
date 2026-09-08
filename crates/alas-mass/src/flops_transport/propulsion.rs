// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! FLOPS transport propulsion-group equations: the engine count and nacelle
//! count (73-74), scaled engine mass (75-76, 80), distributed-propulsion
//! scaling (81-85), thrust reversers (86), engine controls and starters
//! (87, 89, 91), the fuel system (92), the engine pod (40-41) and the group
//! total (137) of NASA/TM-2017-219627 Vol. I.
//!
//! Inlet and nozzle masses (77-79) are folded into the baseline engine mass,
//! which is how a transport engine catalogue quotes its dry mass; alternate
//! engines (95) and alternate energy storage (96) are user-declared masses
//! FLOPS adds without an equation and are not represented here.

use alas_units::{FOOT, POUND_FORCE, POUND_MASS};

fn lb(kilograms: f64) -> f64 {
    kilograms / POUND_MASS
}

fn kg(pounds: f64) -> f64 {
    pounds * POUND_MASS
}

fn lbf(newtons: f64) -> f64 {
    newtons / POUND_FORCE
}

/// Equation 74: the total nacelle count `TNAC`, the engine count plus one
/// half when an odd engine is centre-mounted.
pub fn total_nacelles(engine_count: usize) -> f64 {
    let n = engine_count as f64;
    n + 0.5 * (n - 2.0 * (n / 2.0).floor())
}

/// Equations 81-83: an engine count scaled for distributed propulsion. Four
/// or fewer engines are unchanged; beyond four the count saturates through
/// an arctangent.
pub fn scaled_engine_count(count: usize) -> f64 {
    let n = count as f64;
    if count <= 4 {
        n
    } else {
        4.0 + 2.0 * ((n - 4.0) / 3.0).atan()
    }
}

/// The distributed-propulsion-scaled variables of equations 81-85.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DistributedPropulsionScaling {
    /// `FNENG`: scaled total engine count.
    pub engines: f64,
    /// `FNEW`: scaled wing-mounted engine count.
    pub wing_engines: f64,
    /// `FNEF`: scaled fuselage-mounted engine count.
    pub fuselage_engines: f64,
    /// `FTHRST`: scaled rated thrust per engine, N.
    pub thrust_per_engine_n: f64,
    /// `FNAC`: scaled nacelle diameter, m.
    pub nacelle_diameter_m: f64,
}

/// Equations 81-85 from the installed counts, thrust and nacelle diameter.
pub fn distributed_scaling(
    engine_count: usize,
    wing_engines: usize,
    fuselage_engines: usize,
    rated_thrust_per_engine_n: f64,
    nacelle_diameter_m: f64,
) -> DistributedPropulsionScaling {
    let engines = scaled_engine_count(engine_count);
    let (thrust_per_engine_n, nacelle_diameter) = if engine_count <= 4 {
        (rated_thrust_per_engine_n, nacelle_diameter_m)
    } else {
        (
            engine_count as f64 * rated_thrust_per_engine_n / engines,
            nacelle_diameter_m * (engine_count as f64 / 2.0).sqrt(),
        )
    };
    DistributedPropulsionScaling {
        engines,
        wing_engines: scaled_engine_count(wing_engines),
        fuselage_engines: scaled_engine_count(fuselage_engines),
        thrust_per_engine_n,
        nacelle_diameter_m: nacelle_diameter,
    }
}

/// Equations 75-76: the mass of one scaled engine, kg. The baseline mass is
/// `THRSO / 5.5` in pounds when none is declared (the transport default).
pub fn scaled_engine_kg(
    rated_thrust_per_engine_n: f64,
    baseline_thrust_n: f64,
    baseline_engine_mass_kg: Option<f64>,
    scaling_exponent: f64,
) -> f64 {
    let thrust_lb = lbf(rated_thrust_per_engine_n);
    let baseline_lb = lbf(baseline_thrust_n);
    let wengb = baseline_engine_mass_kg.map_or(baseline_lb / 5.5, lb);
    let wengp = if scaling_exponent >= 0.3 {
        wengb * (thrust_lb / baseline_lb).powf(scaling_exponent)
    } else {
        wengb + (thrust_lb - baseline_lb) * scaling_exponent
    };
    kg(wengp)
}

/// Equation 86: thrust reversers for every nacelle, kg.
pub fn thrust_reversers_kg(rated_thrust_per_engine_n: f64, total_nacelles: f64) -> f64 {
    kg(0.034 * lbf(rated_thrust_per_engine_n) * total_nacelles)
}

/// Equation 87: transport engine controls, kg, from the scaled count and
/// scaled thrust.
pub fn engine_controls_kg(scaled_engines: f64, scaled_thrust_per_engine_n: f64) -> f64 {
    kg(0.26 * scaled_engines * lbf(scaled_thrust_per_engine_n).sqrt())
}

/// Equation 89: transport engine starters, kg, from the scaled count, the
/// maximum Mach number and the scaled nacelle diameter.
pub fn engine_starters_kg(
    scaled_engines: f64,
    maximum_mach: f64,
    scaled_nacelle_diameter_m: f64,
) -> f64 {
    kg(11.0
        * scaled_engines
        * maximum_mach.powf(0.32)
        * (scaled_nacelle_diameter_m / FOOT).powf(1.6))
}

/// Equation 92: transport fuel system, tanks and plumbing, kg.
pub fn fuel_system_kg(
    maximum_fuel_capacity_kg: f64,
    scaled_engines: f64,
    maximum_mach: f64,
) -> f64 {
    kg(1.07
        * lb(maximum_fuel_capacity_kg).powf(0.58)
        * scaled_engines.powf(0.43)
        * maximum_mach.powf(0.34))
}

/// SI inputs to the propulsion group.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlopsPropulsionInputs {
    /// Total engines `NENG`.
    pub engine_count: usize,
    /// Wing-mounted engines `NEW`.
    pub wing_mounted_engine_count: usize,
    /// Fuselage-mounted engines `NEF`.
    pub fuselage_mounted_engine_count: usize,
    /// Rated thrust per installed engine `THRUST`, N.
    pub rated_thrust_per_engine_n: f64,
    /// Rated thrust of the baseline engine `THRSO`, N.
    pub baseline_thrust_n: f64,
    /// Declared baseline engine mass `WENGB`, kg, or the FLOPS estimate.
    pub baseline_engine_mass_kg: Option<f64>,
    /// Engine mass scaling exponent `EEXP`.
    pub scaling_exponent: f64,
    /// Whether thrust reversers are installed.
    pub thrust_reversers_installed: bool,
    /// Maximum Mach number `VMAX`.
    pub maximum_mach: f64,
    /// Average nacelle diameter `DNAC`, m.
    pub nacelle_diameter_m: f64,
    /// Maximum usable fuel capacity `FMXTOT`, kg.
    pub maximum_fuel_capacity_kg: f64,
    /// Declared miscellaneous propulsion mass `WPMISC`, kg.
    pub misc_propulsion_mass_kg: f64,
}

/// The propulsion group, kg.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlopsPropulsionBreakdown {
    /// The scaling applied for distributed propulsion.
    pub scaling: DistributedPropulsionScaling,
    /// Total nacelles `TNAC`.
    pub total_nacelles: f64,
    /// Baseline engine mass `WENGB` actually used, kg.
    pub baseline_engine_mass_kg: f64,
    /// One scaled engine `WENG`, kg.
    pub engine_each_kg: f64,
    /// All engines `WENG x NENG`, kg.
    pub engines_kg: f64,
    /// Thrust reversers `WTHR`, kg.
    pub thrust_reversers_kg: f64,
    /// Engine controls `WEC`, kg.
    pub engine_controls_kg: f64,
    /// Engine starters `WSTART`, kg.
    pub starters_kg: f64,
    /// Miscellaneous propulsion systems `WPMSC` (controls, starters and the
    /// declared miscellaneous mass), kg.
    pub misc_kg: f64,
    /// Fuel system, tanks and plumbing `WFSYS`, kg.
    pub fuel_system_kg: f64,
    /// Equation 137 without alternate engines and energy storage, kg.
    pub total_kg: f64,
}

/// Equations 73-92 and 137: the transport propulsion group.
pub fn estimate_flops_propulsion(inputs: &FlopsPropulsionInputs) -> FlopsPropulsionBreakdown {
    let scaling = distributed_scaling(
        inputs.engine_count,
        inputs.wing_mounted_engine_count,
        inputs.fuselage_mounted_engine_count,
        inputs.rated_thrust_per_engine_n,
        inputs.nacelle_diameter_m,
    );
    let total_nacelles = total_nacelles(inputs.engine_count);
    let baseline_engine_mass_kg = inputs
        .baseline_engine_mass_kg
        .unwrap_or(kg(lbf(inputs.baseline_thrust_n) / 5.5));
    let engine_each_kg = scaled_engine_kg(
        inputs.rated_thrust_per_engine_n,
        inputs.baseline_thrust_n,
        inputs.baseline_engine_mass_kg,
        inputs.scaling_exponent,
    );
    let engines_kg = engine_each_kg * inputs.engine_count as f64;
    let thrust_reversers = if inputs.thrust_reversers_installed {
        thrust_reversers_kg(inputs.rated_thrust_per_engine_n, total_nacelles)
    } else {
        0.0
    };
    let engine_controls = engine_controls_kg(scaling.engines, scaling.thrust_per_engine_n);
    let starters = engine_starters_kg(
        scaling.engines,
        inputs.maximum_mach,
        scaling.nacelle_diameter_m,
    );
    let misc = engine_controls + starters + inputs.misc_propulsion_mass_kg;
    let fuel_system = fuel_system_kg(
        inputs.maximum_fuel_capacity_kg,
        scaling.engines,
        inputs.maximum_mach,
    );
    FlopsPropulsionBreakdown {
        scaling,
        total_nacelles,
        baseline_engine_mass_kg,
        engine_each_kg,
        engines_kg,
        thrust_reversers_kg: thrust_reversers,
        engine_controls_kg: engine_controls,
        starters_kg: starters,
        misc_kg: misc,
        fuel_system_kg: fuel_system,
        total_kg: engines_kg + thrust_reversers + misc + fuel_system,
    }
}

/// Equations 40-41: the mass of one engine pod including its nacelle,
/// `WPOD`, kg, for the detailed wing inertia-relief factor. The systems
/// masses are the FLOPS instruments, electrical and hydraulics groups.
pub fn pod_mass_kg(
    propulsion: &FlopsPropulsionBreakdown,
    nacelle_total_kg: f64,
    instruments_kg: f64,
    electrical_kg: f64,
    hydraulics_kg: f64,
    engine_count: usize,
) -> f64 {
    let wtnfa = propulsion.engines_kg
        + propulsion.thrust_reversers_kg
        + propulsion.starters_kg
        + 0.25 * propulsion.engine_controls_kg
        + 0.11 * instruments_kg
        + 0.13 * electrical_kg
        + 0.13 * hydraulics_kg
        + 0.25 * propulsion.fuel_system_kg;
    wtnfa / engine_count.max(1) as f64 + nacelle_total_kg / propulsion.total_nacelles.max(0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn twin_inputs() -> FlopsPropulsionInputs {
        FlopsPropulsionInputs {
            engine_count: 2,
            wing_mounted_engine_count: 2,
            fuselage_mounted_engine_count: 0,
            rated_thrust_per_engine_n: 120_000.0,
            baseline_thrust_n: 120_000.0,
            baseline_engine_mass_kg: None,
            scaling_exponent: 1.15,
            thrust_reversers_installed: true,
            maximum_mach: 0.82,
            nacelle_diameter_m: 2.0,
            maximum_fuel_capacity_kg: 20_000.0,
            misc_propulsion_mass_kg: 0.0,
        }
    }

    #[test]
    fn nacelle_and_engine_counts_follow_equations_74_and_81() {
        assert_eq!(total_nacelles(2), 2.0);
        assert_eq!(total_nacelles(3), 3.5);
        assert_eq!(total_nacelles(4), 4.0);
        assert_eq!(scaled_engine_count(4), 4.0);
        let eight = scaled_engine_count(8);
        assert!((eight - (4.0 + 2.0 * (4.0_f64 / 3.0).atan())).abs() < 1e-12);
        assert!(scaled_engine_count(64) < 4.0 + std::f64::consts::PI);
    }

    #[test]
    fn distributed_scaling_conserves_total_thrust_and_grows_the_nacelle() {
        let scaled = distributed_scaling(8, 8, 0, 50_000.0, 1.0);
        assert!((scaled.engines * scaled.thrust_per_engine_n - 8.0 * 50_000.0).abs() < 1e-6);
        assert!((scaled.nacelle_diameter_m - 2.0).abs() < 1e-12);
        let twin = distributed_scaling(2, 2, 0, 120_000.0, 2.0);
        assert_eq!(twin.thrust_per_engine_n, 120_000.0);
        assert_eq!(twin.nacelle_diameter_m, 2.0);
    }

    #[test]
    fn the_default_baseline_engine_is_thrust_over_five_and_a_half() {
        let inputs = twin_inputs();
        let breakdown = estimate_flops_propulsion(&inputs);
        let thrust_lb = 120_000.0 / POUND_FORCE;
        assert!((breakdown.baseline_engine_mass_kg / POUND_MASS - thrust_lb / 5.5).abs() < 1e-9);
        assert!((breakdown.engine_each_kg - breakdown.baseline_engine_mass_kg).abs() < 1e-9);
        assert!((breakdown.engines_kg - 2.0 * breakdown.engine_each_kg).abs() < 1e-9);
    }

    #[test]
    fn a_scaled_engine_follows_the_1_15_power_of_the_thrust_ratio() {
        let mut inputs = twin_inputs();
        inputs.baseline_thrust_n = 100_000.0;
        inputs.baseline_engine_mass_kg = Some(2_000.0);
        let breakdown = estimate_flops_propulsion(&inputs);
        assert!((breakdown.engine_each_kg - 2_000.0 * 1.2_f64.powf(1.15)).abs() < 1e-9);

        inputs.scaling_exponent = 0.1;
        let linear = estimate_flops_propulsion(&inputs);
        let expected_lb = 2_000.0 / POUND_MASS + (20_000.0 / POUND_FORCE) * 0.1;
        assert!((linear.engine_each_kg / POUND_MASS - expected_lb).abs() < 1e-9);
    }

    #[test]
    fn reversers_controls_starters_and_fuel_system_match_the_published_forms() {
        let inputs = twin_inputs();
        let breakdown = estimate_flops_propulsion(&inputs);
        let thrust_lb = 120_000.0 / POUND_FORCE;
        assert!(
            (breakdown.thrust_reversers_kg / POUND_MASS - 0.034 * thrust_lb * 2.0).abs() < 1e-9
        );
        assert!(
            (breakdown.engine_controls_kg / POUND_MASS - 0.26 * 2.0 * thrust_lb.sqrt()).abs()
                < 1e-9
        );
        let expected_start = 11.0 * 2.0 * 0.82_f64.powf(0.32) * (2.0 / FOOT).powf(1.6);
        assert!((breakdown.starters_kg / POUND_MASS - expected_start).abs() < 1e-9);
        let expected_fuel =
            1.07 * (20_000.0 / POUND_MASS).powf(0.58) * 2.0_f64.powf(0.43) * 0.82_f64.powf(0.34);
        assert!((breakdown.fuel_system_kg / POUND_MASS - expected_fuel).abs() < 1e-9);
        let sum = breakdown.engines_kg
            + breakdown.thrust_reversers_kg
            + breakdown.misc_kg
            + breakdown.fuel_system_kg;
        assert!((sum - breakdown.total_kg).abs() < 1e-9);

        let mut without = inputs;
        without.thrust_reversers_installed = false;
        assert_eq!(estimate_flops_propulsion(&without).thrust_reversers_kg, 0.0);
    }

    #[test]
    fn the_pod_mass_shares_systems_masses_per_engine_and_nacelle() {
        let breakdown = estimate_flops_propulsion(&twin_inputs());
        let pod = pod_mass_kg(&breakdown, 600.0, 100.0, 400.0, 300.0, 2);
        let expected = (breakdown.engines_kg
            + breakdown.thrust_reversers_kg
            + breakdown.starters_kg
            + 0.25 * breakdown.engine_controls_kg
            + 0.11 * 100.0
            + 0.13 * 400.0
            + 0.13 * 300.0
            + 0.25 * breakdown.fuel_system_kg)
            / 2.0
            + 300.0;
        assert!((pod - expected).abs() < 1e-9);
    }
}
