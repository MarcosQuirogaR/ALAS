// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The mass method on a published modern narrow-body: Boeing's 765-093
//! "SUGAR Free" (2008-technology, 737-class, 154 passengers) of Bradley and
//! Droney, *Subsonic Ultra Green Aircraft Research: Phase I Final Report*,
//! NASA/CR-2011-216847 (2011).
//!
//! Printed inputs: Fig. 5.12 p. 55 (wing 1,532.2 ft^2, aspect ratio 9.76,
//! taper 0.137, quarter-chord sweep 25.14 deg, span 1,388.44 in; tailplane
//! 357.14 ft^2 taper 0.202; fin 284.56 ft^2 taper 0.271; body length
//! 1,497.9 in), Fig. 5.13 p. 56 (12 first + 142 economy, cabin 1,038.5 in),
//! Table 5.8 p. 57 (t/c 0.1258, Mach 0.78), Table 5.9 p. 61 (take-off gross
//! weight 175,635 lb, usable fuel 45,313 lb), Table 5.10 p. 63 (CFM56-7B dry
//! 5,216 lb, fan 61 in, length 98.7 in, 27,300 lbf).
//!
//! Not printed, and assumed here (each is named in the output): the body
//! section (the 737 section, 12.33 ft wide by 13.17 ft deep), the movable
//! wing area (0.35 of the wing area, the mean of the registered jets), the
//! landing weight (FLOPS default, 0.86 of the gross weight at 3,500 nmi), the
//! gear leg lengths (NASA's Aviary single-aisle deck, 102 and 67 in), the
//! nacelle (7.0 ft by 12.3 ft), the wetted area for paint (8,300 ft^2) and
//! the design range (3,500 nmi).

#![allow(clippy::print_stdout, clippy::expect_used)]

use alas_mass::flops_transport::propulsion::{estimate_flops_propulsion, FlopsPropulsionInputs};
use alas_mass::flops_transport::structure::{
    estimate_flops_structure, FlopsStructureInputs, FlopsWingInputs, WingBendingFactor,
};
use alas_mass::flops_transport::{
    estimate_flops_transport, FlopsTransportInputs, PropulsionSizing,
};
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
fn row(name: &str, model_kg: f64, published_lb: f64) {
    let published_kg = lbm(published_lb);
    println!(
        "{name:<34} {model_kg:>9.0} {published_kg:>9.0} {:>+7.1} %",
        100.0 * (model_kg / published_kg - 1.0)
    );
}

fn main() {
    const DG_LB: f64 = 175_635.0;
    const THRUST_LBF: f64 = 27_300.0;
    let wing_area = ft2(1_532.2);
    let span = 1_388.44 * INCH;
    let structure = estimate_flops_structure(&FlopsStructureInputs {
        wing: FlopsWingInputs {
            design_gross_mass_kg: lbm(DG_LB),
            wing_area_m2: wing_area,
            wing_span_m: span,
            taper_ratio: 0.137,
            quarter_chord_sweep_deg: 25.14,
            thickness_to_chord: 0.1258,
            movable_surface_area_m2: 0.35 * wing_area,
            ultimate_load_factor: 3.75,
            composite_utilization: 0.0,
            aeroelastic_tailoring: 0.0,
            strut_bracing: 0.0,
            wing_load_fraction: 1.0,
            // The equation as published: the statement is a FLOPS run.
            bending_stress_factor: 1.0,
            fuselage_count: 1,
            variable_sweep_penalty: 0.0,
            wing_mounted_engine_count: 2,
            bending: WingBendingFactor::Simplified,
        },
        horizontal_tail_area_m2: ft2(357.14),
        horizontal_tail_taper_ratio: 0.202,
        vertical_tail_area_m2: ft2(284.56),
        vertical_tail_taper_ratio: 0.271,
        vertical_tail_count: 1,
        fin_mounted_tailplane_factor: 1.0,
        fuselage_length_m: 1_497.9 * INCH,
        fuselage_width_m: ft(12.33),
        fuselage_depth_m: ft(13.17),
        scaled_fuselage_engines: 0.0,
        military_cargo_floor: 0.0,
        main_deck_cargo_door_fraction: 0.0,
        design_landing_mass_kg: lbm(0.86 * DG_LB),
        main_gear_oleo_length_m: 102.0 * INCH,
        nose_gear_oleo_length_m: 67.0 * INCH,
        total_nacelles: 2.0,
        nacelle_diameter_m: ft(7.0),
        nacelle_length_m: ft(12.3),
        rated_thrust_per_engine_n: THRUST_LBF * POUND_FORCE,
        paint_area_density_kg_m2: lbm(0.037) / (FOOT * FOOT),
        nacelle_mass_override_kg: None,
        pressurized_fuselage: None,
        painted_wetted_area_m2: ft2(8_300.0),
    });
    let propulsion = estimate_flops_propulsion(&FlopsPropulsionInputs {
        engine_count: 2,
        wing_mounted_engine_count: 2,
        fuselage_mounted_engine_count: 0,
        rated_thrust_per_engine_n: THRUST_LBF * POUND_FORCE,
        baseline_thrust_n: THRUST_LBF * POUND_FORCE,
        baseline_engine_mass_kg: Some(lbm(5_216.0)),
        scaling_exponent: 1.15,
        starter_scope: alas_config::FlopsStarterScope::SeparateEquation89,
        baseline_inlet_mass_kg: None,
        inlet_scaling_exponent: 1.0,
        baseline_nozzle_mass_kg: None,
        nozzle_scaling_exponent: 1.0,
        nozzle_scope: alas_config::FlopsNozzleScope::IncludedInBaseline,
        thrust_reversers_installed: true,
        thrust_reverser_engine_count: None,
        maximum_mach: 0.82,
        nacelle_diameter_m: ft(7.0),
        maximum_fuel_capacity_kg: lbm(45_313.0),
        misc_propulsion_mass_kg: 0.0,
        pylon_mass_method: alas_config::PylonMassMethod::LthBoxBeamV1,
    });
    let cabin = |method, bleed| {
        estimate_flops_transport(&FlopsTransportInputs {
            maximum_mach: 0.82,
            design_range_nmi: 3_500.0,
            design_gross_mass_kg: lbm(DG_LB),
            wing_area_m2: wing_area,
            movable_surface_area_m2: 0.35 * wing_area,
            wing_span_m: span,
            quarter_chord_sweep_deg: 25.14,
            fuselage_length_m: 1_497.9 * INCH,
            fuselage_width_m: ft(12.33),
            fuselage_depth_m: ft(13.17),
            fuselage_count: 1,
            passenger_compartment_length_m: 1_038.5 * INCH,
            first_class_passenger_count: 12,
            business_class_passenger_count: 0,
            tourist_class_passenger_count: 142,
            systems_passenger_count: None,
            all_coach_passenger_count: None,
            flight_crew_count: 2,
            flight_attendant_count: 4,
            galley_crew_count: 0,
            wing_mounted_engine_count: 2,
            fuselage_mounted_engine_count: 0,
            engine_count: 2,
            rated_thrust_per_engine_n: THRUST_LBF * POUND_FORCE,
            nacelle_diameter_m: ft(7.0),
            hydraulic_pressure_pa: 3_000.0 * alas_units::PSI,
            variable_sweep_penalty: 0.0,
            maximum_fuel_capacity_kg: lbm(45_313.0),
            fuel_tank_count: 3,
            containerized_cargo_kg: 0.0,
            apu_installed: true,
            bleed_air_system: bleed,
            containerized_baggage_kg: 0.0,
            cargo_loading: alas_config::CargoHoldLoading::Bulk,
            cabin_equipment_method: method,
            haul_class: alas_config::OperatingHaulClass::ShortMediumHaul,
            propulsion_sizing: PropulsionSizing::RatedThrust,
            military_cargo: None,
            executive_cabin: None,
        })
        .expect("complete inputs")
    };
    let lth = cabin(alas_config::CabinEquipmentMethod::LthCivilTransportV1, true);
    let flops = cabin(alas_config::CabinEquipmentMethod::FlopsTransportV1, true);
    let s = &lth.systems;
    let o = &lth.operating_items;

    println!("765-093 SUGAR Free, NASA/CR-2011-216847 Table 5.9 p. 61: model kg, published kg, model/published");
    row("wing", structure.wing.total_kg, 18_728.0);
    row(
        "tail",
        structure.horizontal_tail_kg + structure.vertical_tail_kg,
        3_779.0,
    );
    row("fuselage", structure.fuselage_kg, 18_392.0);
    row(
        "landing gear",
        structure.main_gear_kg + structure.nose_gear_kg,
        6_712.0,
    );
    println!(
        "propulsion parts kg: engines {:.0} reversers {:.0} starters {:.0} misc {:.0} fuel system {:.0} nacelles {:.0} total {:.0}",
        propulsion.engines_kg,
        propulsion.thrust_reversers_kg,
        propulsion.starters_kg,
        propulsion.misc_kg,
        propulsion.fuel_system_kg,
        structure.nacelle_kg,
        propulsion.total_kg
    );
    row(
        "propulsion + pylon (incl. nacelle)",
        propulsion.total_kg + structure.nacelle_kg,
        14_874.0 + 1_858.0,
    );
    row("flight controls", s.surface_controls_kg, 3_084.0);
    row("APU", s.apu_kg, 1_032.0);
    row("hydraulics", s.hydraulics_kg, 894.0);
    row("electrical", s.electrical_kg, 2_557.0);
    row("instruments", s.instruments_kg, 686.0);
    row("avionics", s.avionics_kg, 1_533.0);
    row(
        "air conditioning + pneumatic",
        s.air_conditioning_kg + s.pneumatic_kg,
        1_678.0,
    );
    row("anti-icing", s.anti_ice_kg, 118.0);
    row("paint (no published line)", structure.paint_kg, 1.0);
    row("furnishings, LTH", s.furnishings_kg, 10_866.0);
    row(
        "furnishings, FLOPS eq. 110",
        flops.systems.furnishings_kg,
        10_866.0,
    );
    row("operating items, component", o.total_kg, 7_342.0);
    row(
        "operating items, FLOPS",
        flops.operating_items.total_kg,
        7_342.0,
    );
    row(
        "furnishings + operating, LTH",
        s.furnishings_kg + o.total_kg,
        10_866.0 + 7_342.0,
    );
    row(
        "furnishings + operating, FLOPS",
        flops.systems.furnishings_kg + flops.operating_items.total_kg,
        10_866.0 + 7_342.0,
    );
    let mew = structure.total_kg + propulsion.total_kg + s.total_kg;
    row("MEW (LTH cabin)", mew, 86_790.0);
    row("OEW (LTH cabin)", mew + o.total_kg, 94_132.0);
    let mew_flops = structure.total_kg + propulsion.total_kg + flops.systems.total_kg;
    row(
        "OEW (FLOPS cabin)",
        mew_flops + flops.operating_items.total_kg,
        94_132.0,
    );
    println!(
        "operating items kg (component): passenger service {:.0} cabin crew {:.0} flight crew {:.0} oil {:.0} unusable fuel {:.0}",
        o.passenger_service_kg,
        o.cabin_crew_and_baggage_kg,
        o.flight_crew_and_baggage_kg,
        o.engine_oil_kg,
        o.unusable_fuel_kg
    );
}
