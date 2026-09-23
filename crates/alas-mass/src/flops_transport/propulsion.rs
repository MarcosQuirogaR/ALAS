// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! FLOPS transport propulsion-group equations: the engine count and nacelle
//! count (73-74), scaled engine mass (75-76), the separately declared inlet
//! and nozzle (77-79) and the combined form without them (80),
//! distributed-propulsion scaling (81-85), thrust reversers (86), engine
//! controls and starters (87, 89, 91), the fuel system (92), the engine pod
//! (40-41) and the group total (137) of NASA/TM-2017-219627 Vol. I.
//!
//! Equations 77-80 are a branch, not a fold. NASA/TM-2017-219627 Vol. I
//! defines `WENGB` as the baseline engine mass that "includes inlet and
//! nozzle weight if they are not specified separately"; when `WINLB` and
//! `WNOZB` *are* declared, `WENGB` is the bare core and equation 79 adds the
//! separately scaled inlet and nozzle back. Both branches are implemented:
//! with neither declared the result is equation 80, `WENG = WENGP`, which is
//! how an engine catalogue normally quotes a dry mass and remains the
//! default.
//!
//! Alternate engines (95) and alternate energy storage (96) are
//! user-declared masses FLOPS adds without an equation and are not
//! represented here.

use alas_config::{FlopsNozzleScope, FlopsStarterScope, PylonMassMethod};
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

/// Equation 85: the nacelle diameter scaled for distributed propulsion,
/// `FNAC`, m. Four or fewer engines keep the installed average diameter;
/// beyond four the diameter grows as half the diameter times the square root
/// of the total engine count.
///
/// This is the form NASA Aviary evaluates in
/// `aviary/subsystems/mass/flops_based/distributed_prop.py`
/// (`distributed_nacelle_diam_factor`: `0.5 * diam_avg * total_num_eng**0.5`),
/// the reference implementation of the same FLOPS source. The two-engine
/// validation cases do not exercise this branch, so it is verified against
/// that source rather than against a FLOPS run.
pub fn scaled_nacelle_diameter_m(nacelle_diameter_m: f64, engine_count: usize) -> f64 {
    if engine_count <= 4 {
        nacelle_diameter_m
    } else {
        0.5 * nacelle_diameter_m * (engine_count as f64).sqrt()
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
    DistributedPropulsionScaling {
        engines,
        wing_engines: scaled_engine_count(wing_engines),
        fuselage_engines: scaled_engine_count(fuselage_engines),
        // Equation 84: the installed total thrust divided by the scaled
        // count, which is the installed per-engine thrust for four or fewer.
        thrust_per_engine_n: engine_count as f64 * rated_thrust_per_engine_n / engines,
        nacelle_diameter_m: scaled_nacelle_diameter_m(nacelle_diameter_m, engine_count),
    }
}

/// Equations 75-76: `WENGP`, the mass of one scaled engine before any
/// separately declared inlet and nozzle, kg. The baseline mass is
/// `THRSO / 5.5` in pounds when none is declared (the transport default of
/// equation 76), which is an all-in baseline for the FLOPS engine term and
/// already contains the inlet and nozzle.  "All-in" stops at that FLOPS term:
/// it is not a claim for a whole installed pod.  Nacelle, pylon, mounts,
/// starters, reversers, controls, fuel-system mass and fluids remain separate
/// terms or unresolved installation scope unless their inputs are declared.
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

/// Equations 77 and 78: a baseline inlet (`WINLB`) or nozzle (`WNOZB`)
/// scaled by the thrust ratio raised to its own exponent (`EINL`, `ENOZ`),
/// kg. Both equations have the identical form; only the declared baseline
/// mass and exponent differ.
///
/// Returns `0.0` when no baseline mass is declared, which is the equation 80
/// branch: the inlet and nozzle are then already inside `WENGB`.
pub fn scaled_inlet_or_nozzle_kg(
    baseline_component_mass_kg: Option<f64>,
    rated_thrust_per_engine_n: f64,
    baseline_thrust_n: f64,
    scaling_exponent: f64,
) -> f64 {
    let Some(baseline_kg) = baseline_component_mass_kg else {
        return 0.0;
    };
    let baseline_lb = lbf(baseline_thrust_n);
    if !baseline_lb.is_finite()
        || baseline_lb <= 0.0
        || !baseline_kg.is_finite()
        || baseline_kg < 0.0
    {
        return 0.0;
    }
    let ratio = lbf(rated_thrust_per_engine_n) / baseline_lb;
    kg(lb(baseline_kg) * ratio.powf(scaling_exponent))
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

/// Engine pylons for a podded installation, kg.
///
/// FLOPS has no pylon equation, so this is a declared addition to the
/// published boundary rather than one of its terms; see
/// [`PylonMassMethod`] for the source and the validity domain. The count is
/// the wing-mounted engine count: the relation was fitted on wing pylons, and
/// a tail-mounted centre engine's mounting structure is fuselage and fin
/// structure it was not fitted on.
pub fn pylon_mass_kg(
    method: PylonMassMethod,
    wing_mounted_engine_count: usize,
    rated_thrust_per_engine_n: f64,
) -> f64 {
    match method {
        PylonMassMethod::None => 0.0,
        PylonMassMethod::LthBoxBeamV1 => {
            if !rated_thrust_per_engine_n.is_finite() || rated_thrust_per_engine_n <= 0.0 {
                return 0.0;
            }
            // LTH MA 401 12-01 B: `m = n x 0.2648 x SLST^0.6517`, thrust in
            // newtons and mass in kilograms, so no unit conversion applies.
            wing_mounted_engine_count as f64 * 0.2648 * rated_thrust_per_engine_n.powf(0.6517)
        }
    }
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
    ///
    /// When no inlet or nozzle is declared separately this is the complete
    /// FLOPS engine term, inlet and nozzle included; when either is declared
    /// it is the bare core, and equation 79 adds them back.  It does not mean
    /// a whole installed pod: the nacelle, pylon, mounts, fluids and other
    /// installation equipment are separate or unresolved.
    pub baseline_engine_mass_kg: Option<f64>,
    /// Engine mass scaling exponent `EEXP`.
    pub scaling_exponent: f64,
    /// Source scope of the starter relative to FLOPS equation 89.
    pub starter_scope: FlopsStarterScope,
    /// Separately declared baseline inlet mass `WINLB`, kg. `None` selects
    /// equation 80: the inlet is inside `WENGB`.
    pub baseline_inlet_mass_kg: Option<f64>,
    /// Inlet mass scaling exponent `EINL`; the FLOPS default is 1.
    pub inlet_scaling_exponent: f64,
    /// Separately declared baseline nozzle mass `WNOZB`, kg. `None` selects
    /// equation 80: the nozzle is inside `WENGB`.
    pub baseline_nozzle_mass_kg: Option<f64>,
    /// Nozzle mass scaling exponent `ENOZ`; the FLOPS default is 1.
    pub nozzle_scaling_exponent: f64,
    /// Source scope of the nozzle relative to FLOPS equations 77-80.
    pub nozzle_scope: FlopsNozzleScope,
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
    /// Which method prices the engine pylons, which the published FLOPS
    /// transport equations do not contain at all.
    pub pylon_mass_method: PylonMassMethod,
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
    /// One scaled engine term `WENGP` of equations 75-76, kg: complete within
    /// the FLOPS engine boundary when no inlet or nozzle is declared
    /// separately; this remains narrower than a whole installed pod.
    pub engine_core_each_kg: f64,
    /// One scaled inlet `WINL` of equation 77, kg; zero on the equation 80
    /// branch where the inlet sits inside `WENGB`.
    pub inlet_each_kg: f64,
    /// One scaled nozzle `WNOZ` of equation 78, kg; zero on the equation 80
    /// branch where the nozzle sits inside `WENGB`.
    pub nozzle_each_kg: f64,
    /// One complete scaled engine `WENG`: equation 79 when an inlet or
    /// nozzle is declared separately, equation 80 otherwise, kg.
    pub engine_each_kg: f64,
    /// All engine cores `WENGP x NENG`, kg.
    pub engine_cores_kg: f64,
    /// All inlets `WINL x NENG`, kg.
    pub inlets_kg: f64,
    /// All nozzles `WNOZ x NENG`, kg.
    pub nozzles_kg: f64,
    /// All complete engines `WENG x NENG`, kg.
    pub engines_kg: f64,
    /// Thrust reversers `WTHR`, kg.
    pub thrust_reversers_kg: f64,
    /// Engine controls `WEC`, kg.
    pub engine_controls_kg: f64,
    /// Engine starters `WSTART`, kg.
    pub starters_kg: f64,
    /// Resolved source scope for the starter term.
    pub starter_scope: FlopsStarterScope,
    /// Resolved source scope for the nozzle term.
    pub nozzle_scope: FlopsNozzleScope,
    /// Miscellaneous propulsion systems `WPMSC` (controls, starters and the
    /// declared miscellaneous mass), kg.
    pub misc_kg: f64,
    /// Fuel system, tanks and plumbing `WFSYS`, kg.
    pub fuel_system_kg: f64,
    /// Engine pylons, kg, which are **outside** the published FLOPS
    /// propulsion group and are zero unless a method is declared.
    pub pylons_kg: f64,
    /// Equation 137 without alternate engines and energy storage, plus any
    /// declared pylon mass, kg.
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
    let engine_core_each_kg = scaled_engine_kg(
        inputs.rated_thrust_per_engine_n,
        inputs.baseline_thrust_n,
        inputs.baseline_engine_mass_kg,
        inputs.scaling_exponent,
    );
    let inlet_each_kg = scaled_inlet_or_nozzle_kg(
        inputs.baseline_inlet_mass_kg,
        inputs.rated_thrust_per_engine_n,
        inputs.baseline_thrust_n,
        inputs.inlet_scaling_exponent,
    );
    let nozzle_each_kg = if matches!(inputs.nozzle_scope, FlopsNozzleScope::SeparateEquation78) {
        scaled_inlet_or_nozzle_kg(
            inputs.baseline_nozzle_mass_kg,
            inputs.rated_thrust_per_engine_n,
            inputs.baseline_thrust_n,
            inputs.nozzle_scaling_exponent,
        )
    } else {
        0.0
    };
    // Equation 79 when either is declared separately, equation 80 otherwise
    // (both zero terms leave `WENG = WENGP`).
    let engine_each_kg = engine_core_each_kg + inlet_each_kg + nozzle_each_kg;
    let count = inputs.engine_count as f64;
    let engine_cores_kg = engine_core_each_kg * count;
    let inlets_kg = inlet_each_kg * count;
    let nozzles_kg = nozzle_each_kg * count;
    let engines_kg = engine_each_kg * count;
    let thrust_reversers = if inputs.thrust_reversers_installed {
        thrust_reversers_kg(inputs.rated_thrust_per_engine_n, total_nacelles)
    } else {
        0.0
    };
    let engine_controls = engine_controls_kg(scaling.engines, scaling.thrust_per_engine_n);
    let starters = if inputs.starter_scope.includes_equation_89() {
        engine_starters_kg(
            scaling.engines,
            inputs.maximum_mach,
            scaling.nacelle_diameter_m,
        )
    } else {
        0.0
    };
    let misc = engine_controls + starters + inputs.misc_propulsion_mass_kg;
    let fuel_system = fuel_system_kg(
        inputs.maximum_fuel_capacity_kg,
        scaling.engines,
        inputs.maximum_mach,
    );
    let pylons = pylon_mass_kg(
        inputs.pylon_mass_method,
        inputs.wing_mounted_engine_count,
        inputs.rated_thrust_per_engine_n,
    );
    FlopsPropulsionBreakdown {
        scaling,
        total_nacelles,
        baseline_engine_mass_kg,
        engine_core_each_kg,
        inlet_each_kg,
        nozzle_each_kg,
        engine_each_kg,
        engine_cores_kg,
        inlets_kg,
        nozzles_kg,
        engines_kg,
        thrust_reversers_kg: thrust_reversers,
        engine_controls_kg: engine_controls,
        starters_kg: starters,
        starter_scope: inputs.starter_scope,
        nozzle_scope: inputs.nozzle_scope,
        misc_kg: misc,
        fuel_system_kg: fuel_system,
        pylons_kg: pylons,
        // Equation 137 is `WENG x NENG + WTHR + WPMSC + WFSYS`. The pylon is
        // not one of its terms and is zero unless a method is declared, so
        // the published sum is still reproducible by leaving it off.
        total_kg: engines_kg + thrust_reversers + misc + fuel_system + pylons,
    }
}

/// Equations 40-41: the mass of one engine pod including its nacelle,
/// `WPOD`, kg, for the detailed wing inertia-relief factor. The systems
/// masses are the FLOPS instruments, electrical and hydraulics groups.
///
/// Equation 41's leading term is `WENG x NENG`, the **complete** scaled
/// engine, so this reads [`FlopsPropulsionBreakdown::engines_kg`] and
/// therefore carries any separately declared inlet and nozzle into the pod
/// relief as well.
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
            starter_scope: FlopsStarterScope::SeparateEquation89,
            baseline_inlet_mass_kg: None,
            inlet_scaling_exponent: 1.0,
            baseline_nozzle_mass_kg: None,
            nozzle_scaling_exponent: 1.0,
            nozzle_scope: FlopsNozzleScope::IncludedInBaseline,
            thrust_reversers_installed: true,
            maximum_mach: 0.82,
            nacelle_diameter_m: 2.0,
            maximum_fuel_capacity_kg: 20_000.0,
            misc_propulsion_mass_kg: 0.0,
            pylon_mass_method: PylonMassMethod::None,
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
        // Equation 85 as Aviary's `distributed_nacelle_diam_factor` evaluates
        // it: 0.5 D sqrt(N), which is sqrt(2) for eight one-metre nacelles,
        // not D sqrt(N/2).
        assert!((scaled.nacelle_diameter_m - 0.5 * 8.0_f64.sqrt()).abs() < 1e-12);
        assert!((scaled.nacelle_diameter_m - std::f64::consts::SQRT_2).abs() < 1e-12);
        let twin = distributed_scaling(2, 2, 0, 120_000.0, 2.0);
        assert_eq!(twin.thrust_per_engine_n, 120_000.0);
        assert_eq!(twin.nacelle_diameter_m, 2.0);
        // The branch *is* continuous at four engines in both the scaled
        // engine count (`FNENG(4) = 4 + 2 atan(0) = 4`) and the nacelle
        // diameter (`FNAC(4) = 0.5 D sqrt(4) = D`, matching the <=4 branch's
        // constant D exactly): treated as a function of a continuous engine
        // count, the two pieces meet with no jump at the boundary. No
        // registered preset exceeds four engines, so this is unreached in
        // practice either way.
        assert_eq!(scaled_nacelle_diameter_m(3.0, 4), 3.0);
        assert!((scaled_nacelle_diameter_m(3.0, 5) - 1.5 * 5.0_f64.sqrt()).abs() < 1e-12);
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
    fn starter_scope_is_explicit_and_unknown_is_conservative() {
        let separate = estimate_flops_propulsion(&twin_inputs());
        assert!(separate.starters_kg > 0.0);
        assert_eq!(
            separate.starter_scope,
            FlopsStarterScope::SeparateEquation89
        );

        let mut included_inputs = twin_inputs();
        included_inputs.starter_scope = FlopsStarterScope::IncludedInBaseline;
        let included = estimate_flops_propulsion(&included_inputs);
        assert_eq!(included.starters_kg, 0.0);
        assert_eq!(
            included.starter_scope,
            FlopsStarterScope::IncludedInBaseline
        );
        assert!((separate.misc_kg - included.misc_kg - separate.starters_kg).abs() < 1e-9);

        let mut unknown_inputs = twin_inputs();
        unknown_inputs.starter_scope = FlopsStarterScope::UnknownConservativeSeparate;
        let unknown = estimate_flops_propulsion(&unknown_inputs);
        assert!((unknown.starters_kg - separate.starters_kg).abs() < 1e-12);
        assert_eq!(
            unknown.starter_scope,
            FlopsStarterScope::UnknownConservativeSeparate
        );
    }

    #[test]
    fn a_separately_declared_inlet_and_nozzle_follow_equations_77_to_79() {
        // WENGB is the bare core when WINLB and WNOZB are declared, so
        // WENG = WENGP + WINL + WNOZ with each term scaled by its own
        // exponent on the same thrust ratio r = THRUST / THRSO = 1.25.
        let mut inputs = twin_inputs();
        inputs.baseline_thrust_n = 96_000.0;
        inputs.rated_thrust_per_engine_n = 120_000.0;
        inputs.baseline_engine_mass_kg = Some(2_400.0);
        inputs.scaling_exponent = 1.15;
        inputs.baseline_inlet_mass_kg = Some(180.0);
        inputs.inlet_scaling_exponent = 1.0;
        inputs.baseline_nozzle_mass_kg = Some(120.0);
        inputs.nozzle_scaling_exponent = 0.8;
        inputs.nozzle_scope = FlopsNozzleScope::SeparateEquation78;
        let breakdown = estimate_flops_propulsion(&inputs);

        let ratio = 120_000.0 / 96_000.0_f64;
        assert!((ratio - 1.25).abs() < 1e-12);
        let core = 2_400.0 * ratio.powf(1.15);
        let inlet = 180.0 * ratio;
        let nozzle = 120.0 * ratio.powf(0.8);
        assert!((breakdown.engine_core_each_kg - core).abs() < 1e-9);
        assert!((breakdown.inlet_each_kg - inlet).abs() < 1e-9);
        assert!((breakdown.nozzle_each_kg - nozzle).abs() < 1e-9);
        // Equation 79.
        assert!((breakdown.engine_each_kg - (core + inlet + nozzle)).abs() < 1e-9);
        // Group totals: two engines, each reported separately and summed.
        assert!((breakdown.engine_cores_kg - 2.0 * core).abs() < 1e-9);
        assert!((breakdown.inlets_kg - 2.0 * inlet).abs() < 1e-9);
        assert!((breakdown.nozzles_kg - 2.0 * nozzle).abs() < 1e-9);
        assert!((breakdown.engines_kg - 2.0 * (core + inlet + nozzle)).abs() < 1e-9);
        assert!(
            (breakdown.engines_kg
                - (breakdown.engine_cores_kg + breakdown.inlets_kg + breakdown.nozzles_kg))
                .abs()
                < 1e-9
        );
        // Equation 137: the group total carries the complete engines.
        let expected_total = breakdown.engines_kg
            + breakdown.thrust_reversers_kg
            + breakdown.misc_kg
            + breakdown.fuel_system_kg;
        assert!((breakdown.total_kg - expected_total).abs() < 1e-9);
        // Equation 41 uses WENG x NENG, so the pod carries the inlet and
        // nozzle too: exactly the separate items divided over the engines.
        let complete = pod_mass_kg(&breakdown, 600.0, 100.0, 400.0, 300.0, 2);
        let mut core_only = inputs;
        core_only.baseline_inlet_mass_kg = None;
        core_only.baseline_nozzle_mass_kg = None;
        let bare = estimate_flops_propulsion(&core_only);
        let bare_pod = pod_mass_kg(&bare, 600.0, 100.0, 400.0, 300.0, 2);
        assert!((complete - bare_pod - (inlet + nozzle)).abs() < 1e-9);
    }

    #[test]
    fn equation_80_remains_the_default_with_no_separate_inlet_or_nozzle() {
        // The existing catalogue-dry-mass path must be bit-for-bit unchanged.
        let breakdown = estimate_flops_propulsion(&twin_inputs());
        assert_eq!(breakdown.inlet_each_kg, 0.0);
        assert_eq!(breakdown.nozzle_each_kg, 0.0);
        assert_eq!(breakdown.inlets_kg, 0.0);
        assert_eq!(breakdown.nozzles_kg, 0.0);
        assert_eq!(breakdown.engine_each_kg, breakdown.engine_core_each_kg);
        assert_eq!(breakdown.engines_kg, breakdown.engine_cores_kg);
        // A declared baseline of zero is still the equation 79 branch and
        // adds nothing, rather than being confused with "not declared".
        let mut zero_items = twin_inputs();
        zero_items.baseline_engine_mass_kg = Some(2_000.0);
        zero_items.baseline_inlet_mass_kg = Some(0.0);
        let zeroed = estimate_flops_propulsion(&zero_items);
        assert_eq!(zeroed.inlet_each_kg, 0.0);
        assert_eq!(zeroed.engine_each_kg, zeroed.engine_core_each_kg);
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
