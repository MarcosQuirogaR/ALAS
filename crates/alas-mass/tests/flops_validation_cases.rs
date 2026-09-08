// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The FLOPS transport equations against two FLOPS-run validation cases.
//!
//! NASA's Aviary (github.com/OpenMDAO/Aviary, Apache-2.0) ships the inputs
//! and the FLOPS-produced outputs of `LargeSingleAisle2FLOPS` (simple wing
//! bending factor, every mass scaler at one) and `LargeSingleAisle1FLOPS`
//! (detailed wing, several group scalers). The values are recorded verbatim
//! in `.agent/reports/flops-aviary-validation-data.md`. Every equation here
//! is fed the case's inputs converted to SI and must reproduce the FLOPS
//! output to the precision the data file quotes (mostly 0.1 lb on four to
//! five figures). Where Aviary applies a group scaler the FLOPS output is
//! divided by it, since ALAS carries no scalers.
//!
//! This is implementation verification against the published equations as
//! FLOPS itself evaluates them, not physical validation against weighed
//! aircraft.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_mass::flops_transport::propulsion::{estimate_flops_propulsion, FlopsPropulsionInputs};
use alas_mass::flops_transport::structure::{
    estimate_flops_structure, FlopsStructureInputs, FlopsWingInputs, WingBendingFactor,
};
use alas_mass::flops_transport::wing_bending::{
    detailed_bending_factor, elliptical_load_intensity, WingStation,
};
use alas_mass::flops_transport::{estimate_flops_transport, FlopsTransportInputs};
use alas_units::{FOOT, INCH, POUND_FORCE, POUND_MASS};

fn ft(feet: f64) -> f64 {
    feet * FOOT
}

fn ft2(square_feet: f64) -> f64 {
    square_feet * FOOT * FOOT
}

fn lbm(pounds: f64) -> f64 {
    pounds * POUND_MASS
}

fn lbf(pounds_force: f64) -> f64 {
    pounds_force * POUND_FORCE
}

fn in_lb(kilograms: f64) -> f64 {
    kilograms / POUND_MASS
}

/// Assert `actual_kg` equals `expected_lb` to `relative` (default the
/// quoted precision of the data file, one part in two thousand).
fn check(label: &str, actual_kg: f64, expected_lb: f64, relative: f64) {
    let actual_lb = in_lb(actual_kg);
    let error = (actual_lb - expected_lb).abs() / expected_lb.abs().max(1.0);
    assert!(
        error <= relative,
        "{label}: {actual_lb:.2} lb vs FLOPS {expected_lb:.2} lb ({:.3}% off)",
        error * 100.0
    );
}

/// The data file quotes most masses to 0.1 lb on four to five figures and
/// several to the pound, so one part in a thousand is its precision.
const QUOTED: f64 = 1.0e-3;

/// `LargeSingleAisle2FLOPS`: a 737-800-class twin at 174,200 lb design
/// gross weight, simple wing method, scalers at one.
mod large_single_aisle_2 {
    use super::*;

    const DG_LB: f64 = 174_200.0;
    const SPAN_FT: f64 = 112.57;
    const AREA_FT2: f64 = 1_341.0;
    const THRUST_LBF: f64 = 27_301.0;
    const NACELLE_DIAMETER_FT: f64 = 7.0;
    const MAX_MACH: f64 = 0.82;
    const FUEL_CAPACITY_LB: f64 = 46_063.0;

    fn wing_inputs() -> FlopsWingInputs {
        FlopsWingInputs {
            design_gross_mass_kg: lbm(DG_LB),
            wing_area_m2: ft2(AREA_FT2),
            wing_span_m: ft(SPAN_FT),
            taper_ratio: 0.237_343_146_184_852,
            quarter_chord_sweep_deg: 25.03,
            thickness_to_chord: 0.131_732_727_515_702,
            movable_surface_area_m2: ft2(0.333 * AREA_FT2),
            ultimate_load_factor: 3.75,
            composite_utilization: 0.0,
            aeroelastic_tailoring: 0.0,
            strut_bracing: 0.0,
            wing_load_fraction: 1.0,
            fuselage_count: 1,
            variable_sweep_penalty: 0.0,
            wing_mounted_engine_count: 2,
            bending: WingBendingFactor::Simplified,
        }
    }

    fn structure_inputs() -> FlopsStructureInputs {
        FlopsStructureInputs {
            wing: wing_inputs(),
            horizontal_tail_area_m2: ft2(407.335_370_699_457),
            horizontal_tail_taper_ratio: 0.3008,
            vertical_tail_area_m2: ft2(284.499_779_284_585),
            vertical_tail_taper_ratio: 0.210_820_826_388_98,
            vertical_tail_count: 1,
            fuselage_length_m: ft(124.75),
            fuselage_width_m: ft(12.33),
            fuselage_depth_m: ft(13.0208),
            scaled_fuselage_engines: 0.0,
            military_cargo_floor: 0.0,
            design_landing_mass_kg: lbm(146_328.0),
            main_gear_oleo_length_m: 84.0 * INCH,
            nose_gear_oleo_length_m: 58.8 * INCH,
            total_nacelles: 2.0,
            nacelle_diameter_m: ft(NACELLE_DIAMETER_FT),
            nacelle_length_m: ft(11.65),
            rated_thrust_per_engine_n: lbf(THRUST_LBF),
            paint_area_density_kg_m2: lbm(0.07) / (FOOT * FOOT),
            painted_wetted_area_m2: ft2(8_319.07),
        }
    }

    #[test]
    fn the_structural_group_reproduces_the_flops_run() {
        let structure = estimate_flops_structure(&structure_inputs());
        // FLOPS quotes the factor to four decimals and takes the aspect ratio
        // as the 9.45 input rather than span squared over area (9.4499).
        assert!(
            (structure.wing.bending_factor - 8.8294).abs() < 2.0e-3,
            "BT {}",
            structure.wing.bending_factor
        );
        assert!((structure.wing.inertia_relief_factor - 0.94).abs() < 1e-12);
        check(
            "W1 bending material",
            structure.wing.bending_material_kg,
            6_016.9,
            QUOTED,
        );
        check(
            "W2 shear and control",
            structure.wing.shear_and_control_kg,
            7_552.6,
            QUOTED,
        );
        check(
            "W3 miscellaneous",
            structure.wing.miscellaneous_kg,
            1_718.7,
            QUOTED,
        );
        check("wing", structure.wing.total_kg, 15_288.0, QUOTED);
        check(
            "horizontal tail",
            structure.horizontal_tail_kg,
            1_931.8,
            QUOTED,
        );
        check("vertical tail", structure.vertical_tail_kg, 1_035.6, QUOTED);
        check("fuselage", structure.fuselage_kg, 16_790.0, QUOTED);
        check(
            "landing gear",
            structure.main_gear_kg + structure.nose_gear_kg,
            7_148.277_290_864_326,
            1.0e-6,
        );
        check("nacelles", structure.nacelle_kg, 2.0 * 806.0988, QUOTED);
        check("paint", structure.paint_kg, 582.3, QUOTED);
        check("structure", structure.total_kg, 44_389.0, QUOTED);
    }

    #[test]
    fn the_propulsion_group_reproduces_the_flops_run() {
        let propulsion = estimate_flops_propulsion(&FlopsPropulsionInputs {
            engine_count: 2,
            wing_mounted_engine_count: 2,
            fuselage_mounted_engine_count: 0,
            rated_thrust_per_engine_n: lbf(THRUST_LBF),
            baseline_thrust_n: lbf(THRUST_LBF),
            baseline_engine_mass_kg: Some(lbm(8_071.35)),
            scaling_exponent: 1.15,
            thrust_reversers_installed: true,
            maximum_mach: MAX_MACH,
            nacelle_diameter_m: ft(NACELLE_DIAMETER_FT),
            maximum_fuel_capacity_kg: lbm(FUEL_CAPACITY_LB),
            misc_propulsion_mass_kg: 0.0,
        });
        check("engines", propulsion.engines_kg, 16_143.0, QUOTED);
        check(
            "thrust reversers",
            propulsion.thrust_reversers_kg,
            1_856.4,
            QUOTED,
        );
        check(
            "starters",
            propulsion.starters_kg,
            11.0 * 2.0 * MAX_MACH.powf(0.32) * NACELLE_DIAMETER_FT.powf(1.6),
            1.0e-9,
        );
        check("misc propulsion", propulsion.misc_kg, 550.4, QUOTED);
        check("fuel system", propulsion.fuel_system_kg, 682.7, QUOTED);
        check("propulsion", propulsion.total_kg, 19_232.0, QUOTED);
    }

    #[test]
    fn the_systems_and_operating_items_reproduce_the_flops_run() {
        // Cargo containers count baggage with the cargo: 4,077 lb of cargo
        // plus 162 x 35 lb of baggage make eleven 950 lb containers.
        let inputs = FlopsTransportInputs {
            maximum_mach: MAX_MACH,
            design_range_nmi: 2_960.0,
            design_gross_mass_kg: lbm(DG_LB),
            wing_area_m2: ft2(AREA_FT2),
            movable_surface_area_m2: ft2(0.333 * AREA_FT2),
            wing_span_m: ft(SPAN_FT),
            quarter_chord_sweep_deg: 25.03,
            fuselage_length_m: ft(124.75),
            fuselage_width_m: ft(12.33),
            fuselage_depth_m: ft(13.0208),
            fuselage_count: 1,
            passenger_compartment_length_m: ft(98.5),
            first_class_passenger_count: 12,
            business_class_passenger_count: 0,
            tourist_class_passenger_count: 150,
            flight_crew_count: 2,
            flight_attendant_count: 5,
            galley_crew_count: 1,
            wing_mounted_engine_count: 2,
            fuselage_mounted_engine_count: 0,
            engine_count: 2,
            rated_thrust_per_engine_n: lbf(THRUST_LBF),
            nacelle_diameter_m: ft(NACELLE_DIAMETER_FT),
            hydraulic_pressure_pa: 3_000.0 * alas_units::PSI,
            variable_sweep_penalty: 0.0,
            maximum_fuel_capacity_kg: lbm(FUEL_CAPACITY_LB),
            fuel_tank_count: 7,
            containerized_cargo_kg: lbm(4_077.0 + 162.0 * 35.0),
        };
        let breakdown = estimate_flops_transport(&inputs).expect("complete inputs");
        let systems = breakdown.systems;
        check(
            "surface controls",
            systems.surface_controls_kg,
            1_835.0,
            QUOTED,
        );
        check("APU", systems.apu_kg, 1_014.0, QUOTED);
        check("instruments", systems.instruments_kg, 484.0, QUOTED);
        check("hydraulics", systems.hydraulics_kg, 1_075.3, QUOTED);
        check("electrical", systems.electrical_kg, 1_935.6, QUOTED);
        check("avionics", systems.avionics_kg, 1_339.4, QUOTED);
        check("furnishings", systems.furnishings_kg, 14_690.0, QUOTED);
        check(
            "air conditioning",
            systems.air_conditioning_kg,
            1_603.75,
            QUOTED,
        );
        check("anti-icing", systems.anti_ice_kg, 195.93, QUOTED);
        check("systems and equipment", systems.total_kg, 24_174.0, QUOTED);
        let items = breakdown.operating_items;
        check("cabin crew", items.cabin_crew_and_baggage_kg, 975.0, 1e-9);
        check("flight crew", items.flight_crew_and_baggage_kg, 450.0, 1e-9);
        check("unusable fuel", items.unusable_fuel_kg, 497.7, QUOTED);
        check("engine oil", items.engine_oil_kg, 125.42, QUOTED);
        check(
            "passenger service",
            items.passenger_service_kg,
            2_787.302_854_38,
            1e-6,
        );
        check("cargo containers", items.cargo_containers_kg, 1_925.0, 1e-9);
        check("operating items", items.total_kg, 6_760.422_854_38, 1e-5);
    }

    #[test]
    fn the_empty_mass_margin_closes_the_flops_empty_mass() {
        // Equation 139 with the case's 0.514 percent margin on the three
        // groups: 44,389 + 19,232 + 24,174 = 87,795 lb, margin 451.3 lb,
        // empty mass 88,246 lb.
        let groups_lb: f64 = 44_389.0 + 19_232.0 + 24_174.0;
        let margin_lb: f64 = 0.00514 * groups_lb;
        assert!((margin_lb - 451.3).abs() < 0.1, "{margin_lb}");
        assert!((groups_lb + margin_lb - 88_246.0).abs() < 0.5);
    }
}

/// `LargeSingleAisle1FLOPS`: a 181,200 lb twin with the detailed wing
/// method, a 20 percent composite wing, a 134 ft^2 glove and no reversers.
mod large_single_aisle_1 {
    use super::*;

    const DG_LB: f64 = 181_200.0;
    const SPAN_FT: f64 = 117.83;
    const THRUST_LBF: f64 = 28_928.1;
    const POD_LB: f64 = 9_000.0;

    /// Aviary's integration stations for the case: fifty target intervals
    /// over the input stations `[0, 0.2759, 0.9367]`, fifteen in the first
    /// section (open end) and thirty-six in the second, with the load path
    /// swept 0 and 22 degrees, chord and thickness interpolated linearly
    /// and an elliptical load intensity (`LOAD_DISTRIBUTION_CONTROL = 2`).
    fn stations() -> Vec<WingStation> {
        let inp = [0.0, 0.2759, 0.9367];
        let chord = [0.31, 0.23, 0.084];
        let tc = [0.145, 0.115, 0.104];
        let sweep = [0.0, 22.0];
        let mut etas = Vec::new();
        let mut sweeps = Vec::new();
        for i in 0..15 {
            etas.push(inp[0] + (inp[1] - inp[0]) * i as f64 / 15.0);
            sweeps.push(sweep[0]);
        }
        for i in 0..36 {
            etas.push(inp[1] + (inp[2] - inp[1]) * i as f64 / 35.0);
            sweeps.push(sweep[1]);
        }
        let interp = |values: [f64; 3], eta: f64| {
            let (i, t) = if eta <= inp[1] {
                (0, (eta - inp[0]) / (inp[1] - inp[0]))
            } else {
                (1, (eta - inp[1]) / (inp[2] - inp[1]))
            };
            values[i] + t.clamp(0.0, 1.0) * (values[i + 1] - values[i])
        };
        etas.iter()
            .zip(&sweeps)
            .map(|(&eta, &load_path_sweep_deg)| WingStation {
                eta,
                chord_per_semispan: interp(chord, eta),
                thickness_to_chord: interp(tc, eta),
                load_intensity: elliptical_load_intensity(eta),
                load_path_sweep_deg,
            })
            .collect()
    }

    #[test]
    fn the_detailed_bending_factor_and_pod_relief_reproduce_the_flops_run() {
        let engine_eta = 15.83 / (SPAN_FT / 2.0);
        let factor = detailed_bending_factor(&stations(), &[engine_eta], 11.22091, 0.0, 0.0)
            .expect("the case integrates");
        assert!(
            (factor.bt - 11.5918).abs() / 11.5918 < 2.0e-3,
            "BT {} vs 11.5918",
            factor.bt
        );
        let caye = 1.0 - factor.bte / factor.bt * POD_LB / DG_LB;
        assert!(
            (caye - 0.967333).abs() < 5.0e-4,
            "CAYE {caye} vs 0.967333 (BTE {})",
            factor.bte
        );
    }

    #[test]
    fn the_wing_terms_reproduce_the_flops_run_at_the_published_factors() {
        // The bending factor and relief factor FLOPS reported are supplied,
        // so this isolates equations 33-37 and 45 from the integration.
        let wing = alas_mass::flops_transport::structure::wing_mass(&FlopsWingInputs {
            design_gross_mass_kg: lbm(DG_LB),
            wing_area_m2: ft2(1_370.0),
            wing_span_m: ft(SPAN_FT),
            taper_ratio: 0.278,
            quarter_chord_sweep_deg: 25.0,
            thickness_to_chord: 0.13,
            movable_surface_area_m2: ft2(137.0),
            ultimate_load_factor: 3.75,
            composite_utilization: 0.2,
            aeroelastic_tailoring: 0.0,
            strut_bracing: 0.0,
            wing_load_fraction: 1.0,
            fuselage_count: 1,
            variable_sweep_penalty: 0.0,
            wing_mounted_engine_count: 2,
            bending: WingBendingFactor::Detailed {
                bt: 11.5918,
                bte: 11.5918 * (1.0 - 0.967333) * DG_LB / POD_LB,
                pod_mass_kg: lbm(POD_LB),
            },
        });
        check(
            "W1 bending material",
            wing.bending_material_kg,
            8_184.8,
            1.0e-3,
        );
        check(
            "W2 shear and control",
            wing.shear_and_control_kg,
            4_998.8,
            QUOTED,
        );
        check("W3 miscellaneous", wing.miscellaneous_kg, 1_668.3, QUOTED);
        // Aviary applies a 1.23 wing mass scaler to reach 18,268 lb.
        check("wing before scaler", wing.total_kg, 18_268.0 / 1.23, 1.0e-3);
    }

    #[test]
    fn the_remaining_structure_and_propulsion_reproduce_the_flops_run_without_scalers() {
        let structure = estimate_flops_structure(&FlopsStructureInputs {
            wing: FlopsWingInputs {
                design_gross_mass_kg: lbm(DG_LB),
                wing_area_m2: ft2(1_370.0),
                wing_span_m: ft(SPAN_FT),
                taper_ratio: 0.278,
                quarter_chord_sweep_deg: 25.0,
                thickness_to_chord: 0.13,
                movable_surface_area_m2: ft2(137.0),
                ultimate_load_factor: 3.75,
                composite_utilization: 0.2,
                aeroelastic_tailoring: 0.0,
                strut_bracing: 0.0,
                wing_load_fraction: 1.0,
                fuselage_count: 1,
                variable_sweep_penalty: 0.0,
                wing_mounted_engine_count: 2,
                bending: WingBendingFactor::Simplified,
            },
            horizontal_tail_area_m2: ft2(355.0),
            horizontal_tail_taper_ratio: 0.22,
            vertical_tail_area_m2: ft2(284.0),
            vertical_tail_taper_ratio: 0.33,
            vertical_tail_count: 1,
            fuselage_length_m: ft(128.0),
            fuselage_width_m: ft(12.33),
            fuselage_depth_m: ft(13.17),
            scaled_fuselage_engines: 0.0,
            military_cargo_floor: 0.0,
            design_landing_mass_kg: lbm(152_800.0),
            main_gear_oleo_length_m: 102.0 * INCH,
            nose_gear_oleo_length_m: 67.0 * INCH,
            total_nacelles: 2.0,
            nacelle_diameter_m: ft(7.94),
            nacelle_length_m: ft(12.30),
            rated_thrust_per_engine_n: lbf(THRUST_LBF),
            paint_area_density_kg_m2: lbm(0.037) / (FOOT * FOOT),
            painted_wetted_area_m2: ft2(8_275.86),
        });
        check(
            "horizontal tail / 1.2",
            structure.horizontal_tail_kg,
            1_831.0 / 1.2,
            QUOTED,
        );
        check("vertical tail", structure.vertical_tail_kg, 1_221.8, QUOTED);
        check(
            "fuselage / 1.05",
            structure.fuselage_kg,
            18_357.0 / 1.05,
            QUOTED,
        );
        check(
            "main gear / 1.1",
            structure.main_gear_kg,
            7_910.32 / 1.1,
            QUOTED,
        );
        check("nose gear", structure.nose_gear_kg, 870.59, QUOTED);
        check("nacelles", structure.nacelle_kg, 2.0 * 985.691, QUOTED);
        check("paint", structure.paint_kg, 306.2, QUOTED);

        let propulsion = estimate_flops_propulsion(&FlopsPropulsionInputs {
            engine_count: 2,
            wing_mounted_engine_count: 2,
            fuselage_mounted_engine_count: 0,
            rated_thrust_per_engine_n: lbf(THRUST_LBF),
            baseline_thrust_n: lbf(THRUST_LBF),
            baseline_engine_mass_kg: Some(lbm(7_400.0)),
            scaling_exponent: 1.15,
            thrust_reversers_installed: false,
            maximum_mach: 0.785,
            nacelle_diameter_m: ft(7.94),
            maximum_fuel_capacity_kg: lbm(45_694.0),
            misc_propulsion_mass_kg: 0.0,
        });
        check("engines", propulsion.engines_kg, 14_800.0, 1e-9);
        assert_eq!(propulsion.thrust_reversers_kg, 0.0);
        check("starters", propulsion.starters_kg, 560.39, QUOTED);
        check("misc propulsion", propulsion.misc_kg, 648.83, QUOTED);
        check("fuel system", propulsion.fuel_system_kg, 669.58, QUOTED);
        check("propulsion", propulsion.total_kg, 16_118.0, QUOTED);
    }

    #[test]
    fn the_systems_reproduce_the_flops_run_without_scalers() {
        let inputs = FlopsTransportInputs {
            maximum_mach: 0.785,
            design_range_nmi: 3_500.0,
            design_gross_mass_kg: lbm(DG_LB),
            wing_area_m2: ft2(1_370.0),
            movable_surface_area_m2: ft2(137.0),
            wing_span_m: ft(SPAN_FT),
            quarter_chord_sweep_deg: 25.0,
            fuselage_length_m: ft(128.0),
            fuselage_width_m: ft(12.33),
            fuselage_depth_m: ft(13.17),
            fuselage_count: 1,
            passenger_compartment_length_m: ft(85.5),
            first_class_passenger_count: 11,
            business_class_passenger_count: 0,
            tourist_class_passenger_count: 158,
            flight_crew_count: 2,
            flight_attendant_count: 3,
            galley_crew_count: 0,
            wing_mounted_engine_count: 2,
            fuselage_mounted_engine_count: 0,
            engine_count: 2,
            rated_thrust_per_engine_n: lbf(THRUST_LBF),
            nacelle_diameter_m: ft(7.94),
            hydraulic_pressure_pa: 3_000.0 * alas_units::PSI,
            variable_sweep_penalty: 0.0,
            maximum_fuel_capacity_kg: lbm(45_694.0),
            fuel_tank_count: 7,
            containerized_cargo_kg: lbm(7_436.0),
        };
        let breakdown = estimate_flops_transport(&inputs).expect("complete inputs");
        let systems = breakdown.systems;
        check(
            "surface controls",
            systems.surface_controls_kg,
            894.0,
            1.0e-3,
        );
        check("APU / 1.1", systems.apu_kg, 1_142.0 / 1.1, QUOTED);
        check(
            "instruments / 1.25",
            systems.instruments_kg,
            601.0 / 1.25,
            1.0e-3,
        );
        check("hydraulics", systems.hydraulics_kg, 1_086.7, QUOTED);
        check(
            "electrical / 1.25",
            systems.electrical_kg,
            2_464.0 / 1.25,
            QUOTED,
        );
        check("avionics / 1.2", systems.avionics_kg, 1_652.6 / 1.2, QUOTED);
        check(
            "furnishings / 1.1",
            systems.furnishings_kg,
            15_517.0 / 1.1,
            QUOTED,
        );
        // Aviary feeds the scaled avionics mass (1.2 x) into the air
        // conditioning term, so the unscaled expectation removes that share.
        let avionics_scaler_share_lb = 0.075 * (1_652.6 - 1_652.6 / 1.2);
        check(
            "air conditioning",
            systems.air_conditioning_kg,
            1_602.0 - avionics_scaler_share_lb,
            1.0e-3,
        );
        check("anti-icing", systems.anti_ice_kg, 208.85, QUOTED);
        let items = breakdown.operating_items;
        check("cabin crew", items.cabin_crew_and_baggage_kg, 465.0, 1e-9);
        check("unusable fuel", items.unusable_fuel_kg, 501.3, QUOTED);
        check("engine oil", items.engine_oil_kg, 130.23, QUOTED);
        check(
            "passenger service",
            items.passenger_service_kg,
            3_022.748_058_09,
            1e-6,
        );
        check("cargo containers", items.cargo_containers_kg, 1_400.0, 1e-9);
    }
}
