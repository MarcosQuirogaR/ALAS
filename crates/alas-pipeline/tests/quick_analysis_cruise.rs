// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! The Quick Analysis cruise solve on the default aircraft flies the full
//! analysis' fuel model: its drag coefficient is the report's trimmed drag
//! table at every flight point, and the achievable cruise Mach is a point
//! where maximum-climb thrust equals that drag.

use alas_atmo::Atmosphere;
use alas_config::{AlasConfig, DesignVector};
use alas_pipeline::full_analysis::FullAnalysis;
use alas_pipeline::quick_analysis::{sandbox_config, CruiseSolve};

#[test]
fn the_cruise_solve_flies_the_drag_table_of_the_report_fuel_model() {
    let config = sandbox_config(&AlasConfig::default());
    let report = FullAnalysis::new(config.clone())
        .run(&DesignVector::default(), true)
        .expect("full baseline analysis");
    let mass_kg = config.requirements.mtow_kg;
    let solve = CruiseSolve::new(&config, &report, mass_kg).expect("cruise solve binds");
    let artifacts = report
        .fuel
        .artifacts(&config, &report.design)
        .expect("baseline fuel artifacts");
    let gravity = config.requirements.gravity_m_s2;
    for (mach, altitude_m) in [
        (0.45, 3_000.0),
        (0.78, 10_000.0),
        (
            config.requirements.cruise_mach,
            config.requirements.cruise_altitude_m,
        ),
    ] {
        let drag = solve.drag_coefficients(mach, altitude_m).expect("usable q");
        let atmosphere = Atmosphere::new(altitude_m);
        let tas = mach * atmosphere.speed_of_sound();
        let q = 0.5 * atmosphere.density() * tas * tas;
        let cl = mass_kg * gravity / (q * artifacts.reference_area_m2);
        assert!(
            (drag.cl - cl).abs() <= 1e-12 * cl,
            "Mach {mach}: cl {}",
            drag.cl
        );
        assert_eq!(
            drag.cd.to_bits(),
            artifacts.drag.cd(drag.cl, mach, altitude_m).to_bits(),
            "Mach {mach}, {altitude_m} m"
        );
    }

    let altitude_m = config.requirements.cruise_altitude_m;
    let mach = solve
        .achievable_mach_at_requested_altitude()
        .expect("achievable Mach");
    assert!(
        mach < solve.mach_cap(),
        "precondition: the default aircraft is drag-limited below the scan cap, got {mach}"
    );
    let (excess_n, tas) = solve
        .excess_thrust_n(mach, altitude_m)
        .expect("deck covers");
    let atmosphere = Atmosphere::new(altitude_m);
    let drag_n = 0.5
        * atmosphere.density()
        * tas
        * tas
        * artifacts.reference_area_m2
        * solve
            .drag_coefficients(mach, altitude_m)
            .expect("usable q")
            .cd;
    // Forty bisections of a 0.023 Mach bracket leave a Mach error near
    // 2e-14; the thrust-drag residual there is far below 1e-9 of the drag.
    assert!(
        excess_n.abs() <= 1e-9 * drag_n,
        "thrust minus drag {excess_n} N at Mach {mach} against {drag_n} N of drag"
    );
}
