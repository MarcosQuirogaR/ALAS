// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The transport equations on each weight statement's own aircraft.
//!
//! Every input is the published one for the aircraft the statement belongs to;
//! no preset geometry enters. A sister type's figure is never compared with
//! another type's result. Inputs no source prints are named in the `assumed`
//! line of the case and listed with the row, so a ratio that leans on one can be
//! told from one that does not.
//!
//! Sources by short name:
//! * Obert: Aerodynamic Design of Transport Aircraft (2009), Chapter 42 figures.
//! * Roskam: Airplane Design Part V (1985), Appendix A tables.
//! * Dababneh: Dababneh and Conway-Smith, Aerospace 12, 142 (2025), Tables 5, A1, A2.
//! * Jane's: Jane's All the World's Aircraft 2008-09.
//! * DC-10 ACAP: Boeing DC-10 Airplane Characteristics for Airport Planning.
//! * A320 AC: Airbus A320 Aircraft Characteristics, Airport and Maintenance Planning.
//! * E.003: EASA TCDS E.003 CFM56-5.

#![allow(clippy::print_stdout, clippy::expect_used)]

use alas_mass::flops_transport::structure::{
    fuselage_kg, horizontal_tail_kg, main_gear_kg, main_gear_oleo_length_m, nacelle_kg,
    nose_gear_kg, nose_gear_oleo_length_m, vertical_tail_kg, wing_mass, FlopsWingInputs,
    WingBendingFactor,
};
use alas_mass::flops_transport::{
    estimate_flops_transport, FlopsTransportInputs, PropulsionSizing,
};

const LB: f64 = 0.453_592_37;
const FT: f64 = 0.3048;
const FT2: f64 = FT * FT;
const LBF: f64 = 4.448_221_615_260_5;

/// A wing with its printed planform.
#[derive(Clone, Copy)]
struct WingData {
    area_m2: f64,
    span_m: f64,
    taper: f64,
    sweep_deg: f64,
    thickness: f64,
    /// `SFLAP`, or `None` for the TM default of 0.333 S.
    movables_m2: Option<f64>,
    engines_on_wing: usize,
}

fn wing_kg(gross_kg: f64, wing: WingData) -> f64 {
    wing_mass(&FlopsWingInputs {
        design_gross_mass_kg: gross_kg,
        wing_area_m2: wing.area_m2,
        wing_span_m: wing.span_m,
        taper_ratio: wing.taper,
        quarter_chord_sweep_deg: wing.sweep_deg,
        thickness_to_chord: wing.thickness,
        movable_surface_area_m2: wing.movables_m2.unwrap_or(0.333 * wing.area_m2),
        ultimate_load_factor: 3.75,
        composite_utilization: 0.0,
        aeroelastic_tailoring: 0.0,
        strut_bracing: 0.0,
        wing_load_fraction: 1.0,
        bending_stress_factor: 1.0,
        fuselage_count: 1,
        variable_sweep_penalty: 0.0,
        wing_mounted_engine_count: wing.engines_on_wing,
        bending: WingBendingFactor::Simplified,
    })
    .total_kg
}

fn row(aircraft: &str, component: &str, published: f64, computed: f64, source: &str) {
    println!(
        "{aircraft} | {component} | {published:.0} | {computed:.0} | {:.3} | {source}",
        computed / published
    );
}

/// The inputs of a whole statement.
struct Case {
    name: &'static str,
    source: &'static str,
    gross_kg: f64,
    landing_kg: f64,
    wing: WingData,
    tail_h_m2: f64,
    tail_v_m2: f64,
    /// Taper of the tailplane and the fin. No source prints them for these aircraft.
    tail_taper: (f64, f64),
    length_m: f64,
    width_m: f64,
    depth_m: f64,
    wing_engines: usize,
    body_engines: usize,
    thrust_n: f64,
    nacelle_d_m: f64,
    nacelle_l_m: f64,
    dihedral_deg: f64,
    outboard_engine_y_m: f64,
    mach: f64,
    range_nmi: f64,
    fuel_kg: f64,
    cabin_m: f64,
    seats: usize,
    attendants: usize,
    flight_crew: usize,
    long_haul: bool,
    inputs: &'static str,
    assumed: &'static str,
}

fn transport(case: &Case, lth: bool) -> FlopsTransportInputs {
    FlopsTransportInputs {
        maximum_mach: case.mach,
        design_range_nmi: case.range_nmi,
        design_gross_mass_kg: case.gross_kg,
        wing_area_m2: case.wing.area_m2,
        movable_surface_area_m2: case.wing.movables_m2.unwrap_or(0.333 * case.wing.area_m2),
        wing_span_m: case.wing.span_m,
        quarter_chord_sweep_deg: case.wing.sweep_deg,
        fuselage_length_m: case.length_m,
        fuselage_width_m: case.width_m,
        fuselage_depth_m: case.depth_m,
        fuselage_count: 1,
        passenger_compartment_length_m: case.cabin_m,
        first_class_passenger_count: 0,
        business_class_passenger_count: 0,
        tourist_class_passenger_count: case.seats,
        systems_passenger_count: None,
        all_coach_passenger_count: None,
        flight_crew_count: case.flight_crew,
        flight_attendant_count: case.attendants,
        galley_crew_count: 0,
        wing_mounted_engine_count: case.wing_engines,
        fuselage_mounted_engine_count: case.body_engines,
        engine_count: case.wing_engines + case.body_engines,
        rated_thrust_per_engine_n: case.thrust_n,
        nacelle_diameter_m: case.nacelle_d_m,
        hydraulic_pressure_pa: 20_684_271.879_504,
        variable_sweep_penalty: 0.0,
        maximum_fuel_capacity_kg: case.fuel_kg,
        apu_installed: true,
        bleed_air_system: true,
        fuel_tank_count: 3,
        containerized_cargo_kg: 0.0,
        containerized_baggage_kg: 0.0,
        cargo_loading: alas_config::CargoHoldLoading::Containerized,
        cabin_equipment_method: if lth {
            alas_config::CabinEquipmentMethod::LthCivilTransportV1
        } else {
            alas_config::CabinEquipmentMethod::FlopsTransportV1
        },
        haul_class: if case.long_haul {
            alas_config::OperatingHaulClass::LongHaul
        } else {
            alas_config::OperatingHaulClass::ShortMediumHaul
        },
        propulsion_sizing: PropulsionSizing::RatedThrust,
        military_cargo: None,
        executive_cabin: None,
    }
}

struct Computed {
    wing: f64,
    tail_h: f64,
    tail_v: f64,
    fuselage: f64,
    gear: f64,
    nacelles: f64,
    oleo_m: f64,
}

fn structure(case: &Case) -> Computed {
    let oleo_m = main_gear_oleo_length_m(
        case.nacelle_d_m,
        case.dihedral_deg,
        Some(case.outboard_engine_y_m),
        case.width_m,
        case.length_m,
    );
    let engines = case.wing_engines + case.body_engines;
    let total_nacelles = alas_mass::flops_transport::propulsion::total_nacelles(engines);
    Computed {
        wing: wing_kg(case.gross_kg, case.wing),
        tail_h: horizontal_tail_kg(case.tail_h_m2, case.tail_taper.0, case.gross_kg),
        tail_v: vertical_tail_kg(case.tail_v_m2, case.tail_taper.1, 1, case.gross_kg),
        fuselage: fuselage_kg(
            case.length_m,
            case.width_m,
            case.depth_m,
            case.body_engines as f64,
            0.0,
            1,
        ),
        gear: main_gear_kg(case.landing_kg, oleo_m)
            + nose_gear_kg(case.landing_kg, nose_gear_oleo_length_m(oleo_m)),
        nacelles: nacelle_kg(
            total_nacelles,
            case.nacelle_d_m,
            case.nacelle_l_m,
            case.thrust_n,
        ),
        oleo_m,
    }
}

fn a320() -> Case {
    Case {
        name: "A320-200",
        source: "Obert Fig 42.25 p581",
        gross_kg: 73_500.0,
        landing_kg: 64_500.0,
        wing: WingData {
            area_m2: 122.57,
            span_m: 34.10,
            taper: 0.21,
            sweep_deg: 24.96,
            thickness: 0.126,
            movables_m2: Some(2.74 + 21.10 + 12.64 + 8.64 + 2.35),
            engines_on_wing: 2,
        },
        tail_h_m2: 31.00,
        tail_v_m2: 21.50,
        tail_taper: (0.45, 0.45),
        length_m: 37.57,
        width_m: 3.95,
        depth_m: 4.14,
        wing_engines: 2,
        body_engines: 0,
        thrust_n: 120_100.0,
        nacelle_d_m: 2.37,
        nacelle_l_m: 4.44,
        dihedral_deg: 5.11,
        outboard_engine_y_m: 5.75,
        mach: 0.82,
        range_nmi: 2_700.0,
        fuel_kg: 19_159.0,
        cabin_m: 27.51,
        seats: 150,
        attendants: 4,
        flight_crew: 2,
        long_haul: false,
        inputs: "wing Dababneh A1 pp21-22; SFLAP, Sh, Sv, MLW, fuel Jane's A320 entry p267; L, W, D, dihedral, thrust Jane's A320 entry; YEE A320 AC 2-2-0",
        assumed: "tail taper Raymer Table 4.3 mid; nacelle D, L; range, cabin",
    }
}

fn dc10_30() -> Case {
    Case {
        name: "DC-10-30",
        source: "Roskam A7.1a p150",
        gross_kg: 555_000.0 * LB,
        landing_kg: 403_000.0 * LB,
        wing: WingData {
            area_m2: 3_958.0 * FT2,
            span_m: 50.40,
            taper: 0.22,
            sweep_deg: 35.0,
            thickness: 0.11,
            movables_m2: None,
            engines_on_wing: 2,
        },
        tail_h_m2: 1_338.0 * FT2,
        tail_v_m2: 605.0 * FT2,
        tail_taper: (0.45, 0.45),
        length_m: 55.35,
        width_m: 6.02,
        depth_m: 6.02,
        wing_engines: 2,
        body_engines: 1,
        thrust_n: 51_000.0 * LBF,
        nacelle_d_m: 2.93,
        nacelle_l_m: 7.0,
        dihedral_deg: 5.0,
        outboard_engine_y_m: 8.50,
        mach: 0.88,
        range_nmi: 5_000.0,
        fuel_kg: 137_509.0 * 0.8,
        cabin_m: 41.0,
        seats: 255,
        attendants: 8,
        flight_crew: 3,
        long_haul: true,
        inputs: "S, Sh, Sv Roskam A7.1b p151; b, taper, sweep, t/c Dababneh A2 pp22-23; L, W, YEE, MLW, fuel, seats DC-10 ACAP 2.1, 2.2",
        assumed: "tail taper Raymer mid; thrust; nacelle D, L; dihedral; range, cabin, crew; SFLAP TM default",
    }
}

fn b747_100() -> Case {
    Case {
        name: "747-100",
        source: "Roskam A7.2a p152",
        gross_kg: 710_000.0 * LB,
        landing_kg: 564_000.0 * LB,
        wing: WingData {
            area_m2: 5_500.0 * FT2,
            span_m: 59.64,
            taper: 0.30,
            sweep_deg: 37.5,
            thickness: 0.125,
            movables_m2: None,
            engines_on_wing: 4,
        },
        tail_h_m2: 1_470.0 * FT2,
        tail_v_m2: 830.0 * FT2,
        tail_taper: (0.45, 0.45),
        length_m: 68.63,
        width_m: 6.5,
        depth_m: 7.85,
        wing_engines: 4,
        body_engines: 0,
        thrust_n: 45_500.0 * LBF,
        nacelle_d_m: 2.7,
        nacelle_l_m: 7.0,
        dihedral_deg: 7.0,
        outboard_engine_y_m: 21.0,
        mach: 0.90,
        range_nmi: 5_000.0,
        fuel_kg: 180_000.0,
        cabin_m: 57.0,
        seats: 382,
        attendants: 14,
        flight_crew: 3,
        long_haul: true,
        inputs: "S, Sh, Sv Roskam A7.2b p153; b, taper, sweep, t/c Dababneh A2 pp22-23 (747-200B wing); MLW, thrust, seats Obert Fig 42.20 p578; L Jane's 747-400 entry (same body)",
        assumed: "tail taper Raymer mid; W, D; nacelle D, L; dihedral; YEE; range, fuel, cabin, crew; SFLAP TM default",
    }
}

/// The 747-100 of the Flight statement (Obert Figs 42.20 and 42.21 p578): the
/// same airframe as [`b747_100`] at its own weights, thrust and seats.
fn b747_100_flight() -> Case {
    Case {
        name: "747-100 Flight",
        source: "Obert Fig 42.20 p578 (Flight 1973)",
        gross_kg: 738_000.0 * LB,
        landing_kg: 564_000.0 * LB,
        thrust_n: 46_150.0 * LBF,
        seats: 385,
        inputs: "gross 738,000 lb (taxi), MLW, thrust, seats Obert Fig 42.21 p578; geometry as the Roskam 747-100 case",
        ..b747_100()
    }
}

fn atr42() -> Case {
    Case {
        name: "ATR-42",
        source: "Obert Fig 42.24 p580",
        gross_kg: 16_150.0,
        landing_kg: 16_150.0,
        wing: WingData {
            area_m2: 54.50,
            span_m: 24.57,
            taper: 1.41 / 2.57,
            sweep_deg: 3.0,
            thickness: 0.155,
            movables_m2: Some(3.12 + 11.00),
            engines_on_wing: 2,
        },
        tail_h_m2: 11.73,
        tail_v_m2: 12.48,
        tail_taper: (0.45, 0.8),
        length_m: 22.67,
        width_m: 2.865,
        depth_m: 2.865,
        wing_engines: 2,
        body_engines: 0,
        thrust_n: 20_000.0,
        nacelle_d_m: 1.0,
        nacelle_l_m: 4.0,
        dihedral_deg: 2.0,
        outboard_engine_y_m: 4.05,
        mach: 0.45,
        range_nmi: 800.0,
        fuel_kg: 4_500.0,
        cabin_m: 14.72,
        seats: 42,
        attendants: 1,
        flight_crew: 2,
        long_haul: false,
        inputs: "S, b, chords, Sh, Sv, movables, L, W, cabin, fuel, seats Jane's ATR 42 entry",
        assumed: "tail taper Raymer mid; t/c, sweep, dihedral; turboprop outside the transport equations' thrust inputs: propulsion and systems rows not meaningful",
    }
}

fn whole_statement(case: &Case, published: &[(&str, f64)]) {
    let computed = structure(case);
    let flops =
        estimate_flops_transport(&transport(case, false)).expect("published inputs evaluate");
    let lth = estimate_flops_transport(&transport(case, true)).expect("published inputs evaluate");
    let systems = flops.systems.total_kg - flops.systems.furnishings_kg;
    for (component, value) in published {
        let result = match *component {
            "wing" => computed.wing,
            "htail" => computed.tail_h,
            "vtail" => computed.tail_v,
            "empennage" => computed.tail_h + computed.tail_v,
            "fuselage" => computed.fuselage,
            "landing gear" => computed.gear,
            "nacelles eq 69" => computed.nacelles,
            "surface controls" => flops.systems.surface_controls_kg,
            "systems" => systems,
            "furnishings FLOPS" => flops.systems.furnishings_kg,
            "furnishings LTH" => lth.systems.furnishings_kg,
            "operating FLOPS" => flops.operating_items.total_kg,
            "operating LTH" => lth.operating_items.total_kg,
            "furnishings + operating FLOPS" => {
                flops.systems.furnishings_kg + flops.operating_items.total_kg
            }
            "furnishings + operating LTH" => {
                lth.systems.furnishings_kg + lth.operating_items.total_kg
            }
            other => panic!("unknown component {other}"),
        };
        row(case.name, component, *value, result, case.source);
    }
    println!(
        "{} inputs: {}. Assumed: {}. Eq 66 oleo {:.2} m.",
        case.name, case.inputs, case.assumed, computed.oleo_m
    );
}

fn main() {
    println!("aircraft | component | published kg | computed kg | ratio | statement");
    whole_statement(
        &a320(),
        &[
            ("wing", 8_801.0),
            ("htail", 625.0),
            ("vtail", 463.0),
            ("fuselage", 8_938.0),
            ("landing gear", 2_275.0),
            ("surface controls", 772.0),
            ("systems", 4_788.0),
            ("furnishings FLOPS", 2_813.0),
            ("furnishings LTH", 2_813.0),
            ("operating FLOPS", 3_215.0),
            ("operating LTH", 3_215.0),
            ("furnishings + operating FLOPS", 2_813.0 + 3_215.0),
            ("furnishings + operating LTH", 2_813.0 + 3_215.0),
        ],
    );
    whole_statement(
        &dc10_30(),
        &[
            ("wing", 58_859.0 * LB),
            ("empennage", 14_676.0 * LB),
            ("fuselage", 47_270.0 * LB),
            ("nacelles eq 69", 9_127.0 * LB),
            ("landing gear", 25_761.0 * LB),
            ("surface controls", 6_010.0 * LB),
            (
                "systems",
                (4_274.0
                    + 6_010.0
                    + 2_587.0
                    + 1_920.0
                    + 5_912.0
                    + 1_643.0
                    + 256.0
                    + 2_723.0
                    + 471.0)
                    * LB,
            ),
            ("furnishings FLOPS", 34_124.0 * LB),
            ("furnishings LTH", 34_124.0 * LB),
            ("operating FLOPS", 16_274.0 * LB),
            ("operating LTH", 16_274.0 * LB),
            ("furnishings + operating FLOPS", (34_124.0 + 16_274.0) * LB),
            ("furnishings + operating LTH", (34_124.0 + 16_274.0) * LB),
        ],
    );
    whole_statement(
        &b747_100(),
        &[
            ("wing", 86_402.0 * LB),
            ("empennage", 11_850.0 * LB),
            ("fuselage", 71_845.0 * LB),
            ("nacelles eq 69", 10_031.0 * LB),
            ("landing gear", 31_427.0 * LB),
            ("furnishings FLOPS", 37_245.0 * LB),
            ("furnishings LTH", 37_245.0 * LB),
        ],
    );
    // Interior is the furnishings without seats; the seats sit in the operating
    // items, as in the LTH split (the interior equals LTH furnishings to 1.4 %).
    whole_statement(
        &b747_100_flight(),
        &[
            ("wing", 88_537.0 * LB),
            ("htail", 8_000.0 * LB),
            ("vtail", 3_937.0 * LB),
            ("fuselage", 70_684.0 * LB),
            ("landing gear", (27_464.0 + 3_180.0) * LB),
            ("nacelles eq 69", 10_444.0 * LB),
            ("systems", 35_162.0 * LB),
            ("furnishings FLOPS", 29_381.0 * LB),
            ("furnishings LTH", 29_381.0 * LB),
            ("operating FLOPS", 34_100.0 * LB),
            ("operating LTH", 34_100.0 * LB),
            ("furnishings + operating FLOPS", (29_381.0 + 34_100.0) * LB),
            ("furnishings + operating LTH", (29_381.0 + 34_100.0) * LB),
        ],
    );
    whole_statement(
        &atr42(),
        &[
            ("wing", 1_565.0),
            ("htail", 220.0),
            ("vtail", 322.0),
            ("fuselage", 2_587.0),
            ("landing gear", 534.0),
            ("surface controls", 195.0),
            ("furnishings FLOPS", 977.0),
            ("furnishings LTH", 977.0),
            ("operating FLOPS", 727.0),
            ("operating LTH", 727.0),
        ],
    );

    // Wing and empennage only: the sources print no fuselage or engine data for these.
    let roskam = "Roskam Appendix A7, areas A7.xb; b, taper, sweep, t/c Dababneh A1/A2; tail taper Raymer mid";
    for (name, lb, wing, s_h, s_v, published_wing, published_emp) in [
        (
            "737-200",
            115_500.0,
            (980.0, 28.35, 0.27, 25.0, 0.12, 2),
            321.0,
            233.0,
            10_613.0,
            2_718.0,
        ),
        (
            "A300 B2",
            302_000.0,
            (2_799.0, 44.83, 0.30, 28.0, 0.105, 2),
            748.0,
            487.0,
            44_131.0,
            5_941.0,
        ),
        (
            "DC-10-10",
            430_000.0,
            (3_861.0, 47.35, 0.22, 35.0, 0.11, 2),
            1_338.0,
            605.0,
            48_990.0,
            13_660.0,
        ),
    ] {
        let gross = lb * LB;
        let data = WingData {
            area_m2: wing.0 * FT2,
            span_m: wing.1,
            taper: wing.2,
            sweep_deg: wing.3,
            thickness: wing.4,
            movables_m2: None,
            engines_on_wing: wing.5,
        };
        row(
            name,
            "wing",
            published_wing * LB,
            wing_kg(gross, data),
            roskam,
        );
        let tails = horizontal_tail_kg(s_h * FT2, 0.45, gross)
            + vertical_tail_kg(s_v * FT2, 0.45, 1, gross);
        row(name, "empennage", published_emp * LB, tails, roskam);
    }

    // Learjet 25D, T-tail: Roskam Table A5.1b p139 prints GW 15,000 lb and the
    // tail areas but no span or wing section, so only the empennage is checked.
    let gross = 15_000.0 * LB;
    let tails =
        horizontal_tail_kg(54.0 * FT2, 0.45, gross) + vertical_tail_kg(37.4 * FT2, 0.8, 1, gross);
    row(
        "Learjet 25D",
        "empennage",
        361.0 * LB,
        tails,
        "Roskam A5.1a p138, areas A5.1b p139; tail taper Raymer mid (T-tail fin 0.8)",
    );

    // Wing masses printed with their planforms in the same paper. Basis unstated.
    let dababneh = "Dababneh Table 5 p16, inputs A1/A2 pp21-23; t/c average, SFLAP TM default";
    for (name, gross, area, span, taper, sweep, thickness, engines, published) in [
        (
            "A320-200", 73_500.0, 122.57, 34.10, 0.21, 24.96, 0.126, 2, 8_811.0,
        ),
        (
            "A321-200", 89_000.0, 122.57, 34.10, 0.21, 24.96, 0.126, 2, 7_332.0,
        ),
        (
            "A300B2-100",
            137_892.0,
            260.0,
            44.83,
            0.30,
            28.0,
            0.105,
            2,
            20_017.0,
        ),
        (
            "A330-300", 233_000.0, 363.11, 58.00, 0.22, 29.74, 0.124, 2, 28_402.0,
        ),
        (
            "A340-200", 253_500.0, 363.10, 60.30, 0.25, 29.7, 0.1283, 4, 34_757.0,
        ),
        (
            "A340-300", 276_500.0, 363.11, 58.00, 0.22, 29.74, 0.124, 4, 34_747.0,
        ),
        (
            "747-200B", 377_842.0, 511.0, 59.64, 0.30, 37.5, 0.125, 4, 41_696.0,
        ),
        (
            "747-400", 396_893.0, 532.74, 62.30, 0.22, 38.0, 0.0995, 4, 45_030.0,
        ),
        (
            "DC-10/30", 251_760.0, 367.70, 50.40, 0.22, 35.0, 0.11, 2, 26_712.0,
        ),
    ] {
        let data = WingData {
            area_m2: area,
            span_m: span,
            taper,
            sweep_deg: sweep,
            thickness,
            movables_m2: None,
            engines_on_wing: engines,
        };
        row(name, "wing", published, wing_kg(gross, data), dababneh);
    }

    // Military transports: wing weight, gross weight, trapezoidal area, aspect
    // ratio, taper, average t/c, quarter-chord sweep and ultimate load factor
    // are all printed in one table, with the tail weights and areas beside
    // them. Span is sqrt(AR S); SFLAP is the TM default; tail taper Raymer mid.
    let beltramo = "Beltramo et al. NASA CR-151970 (1977) Table 5.2 p5-6; tail taper Raymer mid";
    for (
        name,
        gross_lb,
        s_ft2,
        ar,
        taper,
        thickness,
        sweep,
        n_ult,
        engines,
        wing_lb,
        s_h,
        s_v,
        h_lb,
        v_lb,
    ) in [
        (
            "C-130A", 108_000.0, 1_808.0, 9.7, 0.440, 0.147, 0.0, 4.5, 4, 10_593.0, 545.0, 300.0,
            2_106.0, 1_084.0,
        ),
        (
            "C-130E", 155_000.0, 1_808.0, 9.7, 0.440, 0.147, 0.0, 3.75, 4, 11_647.0, 545.0, 300.0,
            2_250.0, 1_159.0,
        ),
        (
            "KC-135A", 275_000.0, 2_453.0, 7.0, 0.331, 0.148, 35.0, 3.0, 4, 24_719.0, 500.0, 312.0,
            3_192.0, 1_766.0,
        ),
        (
            "C-133B", 286_000.0, 2_673.0, 12.1, 0.226, 0.163, 0.0, 3.75, 4, 27_064.0, 0.0, 0.0,
            0.0, 0.0,
        ),
        (
            "C-141A", 316_100.0, 3_000.0, 8.5, 0.335, 0.121, 25.0, 3.75, 4, 34_262.0, 483.0, 416.0,
            3_155.0, 2_590.0,
        ),
        (
            "C-5A", 728_000.0, 6_200.0, 7.7, 0.338, 0.119, 24.5, 3.75, 4, 81_782.0, 966.0, 961.0,
            6_788.0, 5_556.0,
        ),
    ] {
        let gross = gross_lb * LB;
        let area = s_ft2 * FT2;
        let computed = wing_mass(&FlopsWingInputs {
            design_gross_mass_kg: gross,
            wing_area_m2: area,
            wing_span_m: (ar * area).sqrt(),
            taper_ratio: taper,
            quarter_chord_sweep_deg: sweep,
            thickness_to_chord: thickness,
            movable_surface_area_m2: 0.333 * area,
            ultimate_load_factor: n_ult,
            composite_utilization: 0.0,
            aeroelastic_tailoring: 0.0,
            strut_bracing: 0.0,
            wing_load_fraction: 1.0,
            bending_stress_factor: 1.0,
            fuselage_count: 1,
            variable_sweep_penalty: 0.0,
            wing_mounted_engine_count: engines,
            bending: WingBendingFactor::Simplified,
        })
        .total_kg;
        row(name, "wing", wing_lb * LB, computed, beltramo);
        // The C-133B tail digits are illegible in the scan; its tails are not checked.
        if s_h > 0.0 {
            row(
                name,
                "htail",
                h_lb * LB,
                horizontal_tail_kg(s_h * FT2, 0.45, gross),
                beltramo,
            );
            row(
                name,
                "vtail",
                v_lb * LB,
                vertical_tail_kg(s_v * FT2, 0.45, 1, gross),
                beltramo,
            );
        }
    }
}
