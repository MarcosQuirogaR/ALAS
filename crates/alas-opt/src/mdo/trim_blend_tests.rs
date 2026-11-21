// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Mach interpolation of the trimmed induced drag against direct solves.
//!
//! A table built for cruise Mach `Mc` answers `induced_cd_at(cl, M)` by
//! blending its cruise-Mach curve with the incompressible one. A table built
//! for a configuration whose cruise Mach *is* `M` carries the direct solve: the
//! Goethert-stretched lattice at that Mach, re-trimmed at every CL. The two are
//! compared at intermediate Mach and CL multiples of the design lift.
// Evidence tests print the tables they compare (run with `--nocapture`).
#![allow(clippy::print_stdout)]
// The tests build their own candidates; a failed unwrap is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use serde_json::json;

use super::build::{build_geometry, first_mass_pass};
use super::drag_table::TrimmedDragTable;
use super::trim::trim_and_polar;

/// A preset's candidate: the table of its own cruise Mach, and one rebuilt for
/// `mach`.
fn table_at(name: &str, mach: Option<f64>) -> (std::sync::Arc<TrimmedDragTable>, f64, f64) {
    let mut config = AlasConfig::from_value(&json!({"preset": name})).unwrap();
    let x = presets::get(name).unwrap().design_vector.to_array();
    if let Some(mach) = mach {
        config.requirements.cruise_mach = mach;
    }
    let (candidate, dv, mut plane) =
        build_geometry(&config, &x).unwrap_or_else(|f| panic!("{}", f.reason));
    let (_, _, cg0, _, _, _) =
        first_mass_pass(&candidate, &dv, &plane).unwrap_or_else(|f| panic!("{}", f.reason));
    // The mid-cruise mass of the design (the cruise lift the wing can carry),
    // scaled with the dynamic pressure so the design lift coefficient does not
    // change with the Mach under test.
    let reference_mach = AlasConfig::from_value(&json!({"preset": name}))
        .unwrap()
        .requirements
        .cruise_mach;
    let mass = 0.78
        * candidate.requirements.mtow_kg
        * (candidate.requirements.cruise_mach / reference_mach).powi(2);
    let polar = trim_and_polar(&candidate, &mut plane, cg0[0], &dv, mass)
        .unwrap_or_else(|f| panic!("{}", f.reason));
    let table = polar.drag.table().expect("a native trim carries its table");
    (
        table.clone(),
        polar.cl_trim,
        candidate.requirements.cruise_mach,
    )
}

/// Worst relative error of the blend at `machs` for `name`, and the printed
/// table of every point.
fn worst_blend_error(name: &str, machs: &[f64]) -> f64 {
    let (reference, cl_design, cruise_mach) = table_at(name, None);
    let mut worst = 0.0_f64;
    for &mach in machs {
        let (direct, _, _) = table_at(name, Some(mach));
        for factor in [0.6, 1.0, 1.3] {
            let cl = factor * cl_design;
            // The blend interpolates the lattice's curves as solved.
            let solved = direct.lattice_induced_cd(cl);
            let blended = reference.lattice_induced_cd_at(cl, mach);
            let error = (blended - solved).abs() / solved;
            worst = worst.max(error);
            println!(
                "{name}: cruise M {cruise_mach:.3}, M {mach:.2}, CL {cl:.3}: direct {solved:.6} blend {blended:.6} error {:+.2} %",
                100.0 * (blended - solved) / solved
            );
        }
    }
    worst
}

/// Measured worst blend error (relative, induced drag) per preset over Mach
/// 0.05 to cruise + 0.05 and CL 0.6-1.3 times the design lift, with the bound
/// each is held to: the interior error of the interpolation in the stretch
/// factor, larger at the top Mach of the swept B747-400, where the extrapolation
/// above cruise is longest.
#[test]
fn the_mach_blend_stays_within_its_measured_error_of_direct_stretched_solves() {
    for (name, machs, bound) in [
        ("B747-400", vec![0.05, 0.3, 0.5, 0.7, 0.9], 0.025),
        ("A320-200", vec![0.05, 0.3, 0.5, 0.7, 0.83], 0.012),
        ("ATR72-600", vec![0.05, 0.2, 0.35, 0.49], 0.002),
    ] {
        let worst = worst_blend_error(name, &machs);
        println!("{name}: worst {:.2} %", 100.0 * worst);
        assert!(worst <= bound, "{name}: {worst} > {bound}");
    }
}

/// The table's off-design trim nodes: at 0.6 and 1.3 times the
/// cruise lift the table's trimmed induced drag against an independent
/// re-trim, a full compressible stability-and-trim solve at that lift followed
/// by the stretched trimmed evaluation, for a high and a low static-margin
/// preset. The table's own nodes are Newton re-trims on the cruise Jacobian;
/// this one starts from nothing.
#[test]
fn the_table_off_design_nodes_equal_an_independent_stretched_retrim() {
    use alas_aero::analysis::{AeroAnalysis, TrimPoint};
    use alas_stab::neutral_point::{beta_for_mach, goethert_stretch};
    use alas_stab::trim::stability_and_trim_with_cache;

    for name in ["B747-400", "ATR72-600"] {
        let config = AlasConfig::from_value(&json!({"preset": name})).unwrap();
        let x = presets::get(name).unwrap().design_vector.to_array();
        let (candidate, dv, mut plane) =
            build_geometry(&config, &x).unwrap_or_else(|f| panic!("{}", f.reason));
        let (_, _, cg0, _, _, _) =
            first_mass_pass(&candidate, &dv, &plane).unwrap_or_else(|f| panic!("{}", f.reason));
        let mass = 0.78 * candidate.requirements.mtow_kg;
        let polar = trim_and_polar(&candidate, &mut plane, cg0[0], &dv, mass)
            .unwrap_or_else(|f| panic!("{}", f.reason));
        let table = polar.drag.table().expect("a native trim carries its table");
        let req = &candidate.requirements;
        let aero = AeroAnalysis::new(
            &plane,
            AeroAnalysis::quarter_chord_sweep_deg(&plane, dv.sweep_deg),
            Some(candidate.geometry.clone()),
            Some(candidate.drag_model.clone()),
            Some(candidate.analysis.clone()),
        );
        let beta = beta_for_mach(req.cruise_mach);
        let stretched = goethert_stretch(&plane, beta);
        for factor in [0.6, 1.3] {
            let target = factor * polar.cl_trim;
            let trim = stability_and_trim_with_cache(
                &plane,
                &candidate.analysis,
                target,
                req.cruise_mach,
                req.cruise_altitude_m,
                &Default::default(),
            )
            .unwrap();
            let flight = trim.attitude();
            assert!(flight.compressible && flight.converged, "{name} {factor}");
            let perf = aero
                .trimmed_stretched_performance(
                    &stretched,
                    &TrimPoint {
                        trim_alpha_deg: flight.alpha_deg,
                        trim_ih_deg: flight.incidence_deg,
                        cl_alpha: flight.cl_alpha,
                    },
                    req.cruise_mach,
                    req.cruise_altitude_m,
                    beta,
                    flight.cm_body_per_deg,
                )
                .unwrap();
            let table_cdi = table.lattice_induced_cd(perf.cl);
            let error = (table_cdi - perf.cd_induced).abs() / perf.cd_induced;
            println!(
                "{name}: CL {:.3} ({factor} x cruise): re-trim CDi {:.6} table {table_cdi:.6} error {:.3} %, residual cm {:.2e}",
                perf.cl,
                perf.cd_induced,
                100.0 * error,
                perf.cm_residual
            );
            assert!(
                error <= super::drag_table::INDUCED_CHECK_RELATIVE_TOLERANCE,
                "{name} at {factor} x cruise CL: {error}"
            );
        }
    }
}

/// Cruise drag decomposition of the table before (incompressible trim basis,
/// `compressible_trim = false`) and after A1G, at the start-of-cruise lift
/// coefficient and at 0.88 of it (mid-cruise), and the induced drag of the
/// stretched path at Mach 0.05 (the airframe moment without compressibility):
/// the evidence behind the per-preset effect table.
/// Run with `--nocapture`.
#[test]
fn cruise_drag_decomposition_before_and_after() {
    for name in [
        "A320-200",
        "A220-300",
        "B747-400",
        "ATR72-600",
        "DC-10",
        "default",
    ] {
        let mut rows = Vec::new();
        for compressible in [false, true] {
            let (mut config, x) = if name == "default" {
                (
                    AlasConfig::default(),
                    alas_config::design_variables::DesignVector::default().to_array(),
                )
            } else {
                (
                    AlasConfig::from_value(&json!({"preset": name})).unwrap(),
                    presets::get(name).unwrap().design_vector.to_array(),
                )
            };
            config.analysis.compressible_trim = compressible;
            let (candidate, dv, mut plane) =
                build_geometry(&config, &x).unwrap_or_else(|f| panic!("{}", f.reason));
            let (_, _, cg0, _, _, _) =
                first_mass_pass(&candidate, &dv, &plane).unwrap_or_else(|f| panic!("{}", f.reason));
            let mass = 0.78 * candidate.requirements.mtow_kg;
            let polar = trim_and_polar(&candidate, &mut plane, cg0[0], &dv, mass)
                .unwrap_or_else(|f| panic!("{}", f.reason));
            let table = polar.drag.table().unwrap().clone();
            let (mach, alt) = (
                candidate.requirements.cruise_mach,
                candidate.requirements.cruise_altitude_m,
            );
            for factor in [1.0, 0.88] {
                let cl = factor * polar.cl_trim;
                let cd0 = table.cd0(mach, alt);
                let viscous = table.viscous_lift_cd_at(cl, mach, alt);
                let induced = table.induced_cd_at(cl, mach);
                let wave = table.wave_cd(cl, mach);
                let total = table.cd(cl, mach, alt);
                let m0 = if compressible {
                    table.induced_cd_at(cl, 0.05)
                } else {
                    f64::NAN
                };
                let margin = (polar.x_np - plane.xyz_ref[0]) / plane.c_ref;
                rows.push(format!(
                    "{name} {} static margin {margin:.3} CL {cl:.3}: CD0 {cd0:.5} viscous-lift {viscous:.5} CDi {induced:.5} (Mach 0.05 {m0:.5}) CDw {wave:.5} CD {total:.5} L/D {:.2}",
                    if compressible { "after " } else { "before" },
                    cl / total
                ));
            }
        }
        for row in rows {
            println!("{row}");
        }
    }
}

/// Where the A1G table differs from the old one along a mission:
/// the induced drag of both tables over the lift coefficients and Mach numbers
/// the climb, cruise and descent legs fly, as new / old - 1 in percent.
/// Run with `--nocapture`.
#[test]
fn induced_drag_change_over_the_mission_envelope() {
    for name in ["A220-300", "A320-200", "B747-400"] {
        let mut tables = Vec::new();
        let mut cruise_mach = 0.0;
        for compressible in [false, true] {
            let mut config = AlasConfig::from_value(&json!({"preset": name})).unwrap();
            config.analysis.compressible_trim = compressible;
            let x = presets::get(name).unwrap().design_vector.to_array();
            let (candidate, dv, mut plane) =
                build_geometry(&config, &x).unwrap_or_else(|f| panic!("{}", f.reason));
            let (_, _, cg0, _, _, _) =
                first_mass_pass(&candidate, &dv, &plane).unwrap_or_else(|f| panic!("{}", f.reason));
            let mass = 0.78 * candidate.requirements.mtow_kg;
            let polar = trim_and_polar(&candidate, &mut plane, cg0[0], &dv, mass)
                .unwrap_or_else(|f| panic!("{}", f.reason));
            cruise_mach = candidate.requirements.cruise_mach;
            tables.push((polar.drag.table().unwrap().clone(), polar.cl_trim));
        }
        let (old, cl_design) = &tables[0];
        let (new, _) = &tables[1];
        println!("{name}: cruise Mach {cruise_mach:.3}, design CL {cl_design:.3}; new/old induced drag - 1 (%)");
        let machs = [0.25, 0.4, 0.55, 0.7, cruise_mach];
        print!("  CL over M ");
        for m in machs {
            print!("{m:>8.2}");
        }
        println!();
        for cl in [0.2, 0.3, 0.4, 0.5, 0.6, 0.8, 1.0] {
            print!("  {cl:>5.2}    ");
            for m in machs {
                let (o, n) = (old.induced_cd_at(cl, m), new.induced_cd_at(cl, m));
                print!("{:>8.1}", 100.0 * (n / o - 1.0));
            }
            println!();
        }
    }
}

/// What the tail's dynamic-pressure ratio does to the cruise drag of the
/// default widebody at its design lift: the trimmed drag table
/// with the tail in the free stream (`tail_efficiency` 1) and at the ratio the
/// geometry gives it (the wing wake). Run with `--nocapture`.
#[test]
fn the_tail_ratio_effect_on_the_default_widebody_cruise_drag() {
    use alas_config::design_variables::DesignVector;
    let mut rows = Vec::new();
    for stated in [Some(1.0), None] {
        let mut config = AlasConfig::default();
        config.analysis.tail_efficiency = stated;
        let x = DesignVector::default().to_array();
        let (candidate, dv, mut plane) =
            build_geometry(&config, &x).unwrap_or_else(|f| panic!("{}", f.reason));
        let efficiency = plane.horizontal_tail_dynamic_pressure_ratio();
        let (_, _, cg0, _, _, _) =
            first_mass_pass(&candidate, &dv, &plane).unwrap_or_else(|f| panic!("{}", f.reason));
        let mass = 0.78 * candidate.requirements.mtow_kg;
        let polar = trim_and_polar(&candidate, &mut plane, cg0[0], &dv, mass)
            .unwrap_or_else(|f| panic!("{}", f.reason));
        let table = polar.drag.table().unwrap().clone();
        let (mach, alt) = (
            candidate.requirements.cruise_mach,
            candidate.requirements.cruise_altitude_m,
        );
        let cl = polar.cl_trim;
        let (cdi, cd) = (table.induced_cd_at(cl, mach), table.cd(cl, mach, alt));
        rows.push((efficiency, cl, cdi, cd, polar.incidence_deg));
    }
    for (efficiency, cl, cdi, cd, incidence) in &rows {
        println!(
            "default widebody, tail ratio {efficiency:.3}: CL {cl:.4} CDi {cdi:.6} CD {cd:.6} L/D {:.3} stabilizer incidence {incidence:+.3} deg",
            cl / cd
        );
    }
    println!(
        "effect of the ratio: CDi {:+.2} counts, CD {:+.2} counts",
        1.0e4 * (rows[1].2 - rows[0].2),
        1.0e4 * (rows[1].3 - rows[0].3)
    );
}
