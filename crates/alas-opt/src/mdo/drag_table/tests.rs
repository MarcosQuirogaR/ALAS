// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Physical invariants and error bounds of [`super::TrimmedDragTable`] on
//! the transport presets, each trimmed at its sizing seed mass exactly as
//! `mdo::sizing` trims its first pass.

// The tests build their own configurations, so a failed `expect` is the
// assertion failing rather than a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]
// The test prints its diagnostic table of measured values.
#![allow(clippy::print_stderr)]

use std::sync::Arc;

use super::*;
use crate::mdo::build::{build_geometry, first_mass_pass};
use alas_atmo::Atmosphere;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;

const PRESETS: [&str; 8] = [
    "AVE",
    "A320-200",
    "A220-300",
    "A340-300",
    "A380-800",
    "B787-9",
    "DC-10",
    "ATR72-600",
];

/// A preset's candidate configuration, trimmed geometry and drag table.
struct Trimmed {
    name: &'static str,
    config: AlasConfig,
    sweep_deg: f64,
    plane: Airplane,
    design: DesignTrim,
    table: Arc<TrimmedDragTable>,
}

impl Trimmed {
    fn new(name: &'static str) -> Self {
        Self::with_viscous_fraction(name, 0.0)
    }

    /// The same candidate with `lift_dependent_viscous_fraction` set.
    fn with_viscous_fraction(name: &'static str, kappa: f64) -> Self {
        let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
        config.drag_model.lift_dependent_viscous_fraction = kappa;
        let x = alas_config::presets::get(name)
            .unwrap()
            .design_vector
            .to_array();
        let (config, dv, mut plane) =
            build_geometry(&config, &x).unwrap_or_else(|f| panic!("{name}: {}", f.reason));
        let (_, _, cg, _, _, _) = first_mass_pass(&config, &dv, &plane)
            .unwrap_or_else(|f| panic!("{name}: {}", f.reason));
        let seed_kg = config.mtow_plan().seed_kg;
        plane.xyz_ref[0] = cg[0];
        let req = &config.requirements;
        let atmo = Atmosphere::new(req.cruise_altitude_m);
        let speed = req.cruise_mach * atmo.speed_of_sound();
        let cl = seed_kg * req.gravity_m_s2 / (0.5 * atmo.density() * speed * speed * plane.s_ref);
        let trim = alas_stab::trim::stability_and_trim(
            &plane,
            &config.analysis,
            cl,
            req.cruise_mach,
            req.cruise_altitude_m,
        )
        .unwrap_or_else(|error| panic!("{name} trim: {error}"));
        assert!(trim.converged, "{name} trim did not converge");
        let aero = AeroAnalysis::new(
            &plane,
            AeroAnalysis::quarter_chord_sweep_deg(&plane, dv.sweep_deg),
            Some(config.geometry.clone()),
            Some(config.resolved_drag_model()),
            Some(config.analysis.clone()),
        );
        let perf = aero
            .trimmed_performance(
                &alas_aero::analysis::TrimPoint {
                    trim_alpha_deg: trim.trim_alpha_deg,
                    trim_ih_deg: trim.trim_ih_deg,
                    cl_alpha: trim.cl_alpha,
                },
                req.cruise_mach,
                req.cruise_altitude_m,
            )
            .unwrap();
        let design = DesignTrim {
            trim,
            cl: perf.cl,
            cd_induced: perf.cd_induced,
            cm_residual: perf.cm_residual,
            mach: req.cruise_mach,
            altitude_m: req.cruise_altitude_m,
            cl_max_clean: config.performance.cl_max_clean,
            cm_tolerance: 1e-3,
            stretched: None,
            incompressible: None,
        };
        let table = TrimmedDragTable::build(&aero, &design)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        Self {
            name,
            sweep_deg: AeroAnalysis::quarter_chord_sweep_deg(&plane, dv.sweep_deg),
            table: Arc::new(table),
            config,
            plane,
            design,
        }
    }

    /// The analysis the table was built from, for direct evaluation.
    fn aero(&self) -> AeroAnalysis<'_> {
        AeroAnalysis::new(
            &self.plane,
            self.sweep_deg,
            Some(self.config.geometry.clone()),
            Some(self.config.resolved_drag_model()),
            Some(self.config.analysis.clone()),
        )
    }
}

fn presets() -> Vec<Trimmed> {
    PRESETS.iter().map(|name| Trimmed::new(name)).collect()
}

#[test]
fn every_preset_table_meets_its_error_bound_off_the_check_points() {
    for preset in presets() {
        let (table, aero) = (&preset.table, preset.aero());
        assert!(
            table.max_abs_error_cd() <= CD_ERROR_BOUND,
            "{}: {}",
            preset.name,
            table.max_abs_error_cd()
        );
        // Quarter points of every cell, at an altitude the refinement never
        // checked: linear and cubic-Hermite errors peak near cell centres,
        // and the altitude enters only through the exact skin friction.
        for altitude_m in [table.reference_altitude_m(), 5_000.0] {
            for mach in table.mach.windows(2).map(|m| 0.75 * m[0] + 0.25 * m[1]) {
                let parasite = aero.parasite_drag(mach, altitude_m, 0.0, None, None);
                for cl in table.cl.windows(2).map(|c| 0.25 * c[0] + 0.75 * c[1]) {
                    // The upsweep's attitude term is evaluated, not tabulated.
                    let direct = parasite
                        + table.upsweep_attitude_cd(cl, mach)
                        + table.induced_cd(cl)
                        + aero.wave_drag(mach, cl, None);
                    let error = (table.cd(cl, mach, altitude_m) - direct).abs();
                    assert!(
                        error <= CD_ERROR_BOUND,
                        "{}: CL {cl:.3} M {mach:.4} h {altitude_m}: {error:.3e}",
                        preset.name
                    );
                }
            }
        }
    }
}

#[test]
fn the_induced_quadratic_passed_its_fourth_trimmed_point_and_is_nonnegative() {
    for preset in presets() {
        let table = &preset.table;
        let check = table.induced_check();
        assert!(
            check.relative_error() <= INDUCED_CHECK_RELATIVE_TOLERANCE,
            "{}: {check:?}",
            preset.name
        );
        // The checking point is a fourth, distinct lift coefficient.
        let design = table.design_cl();
        for fraction in INDUCED_NODE_FRACTIONS {
            assert!((check.cl - fraction * design).abs() > 0.05 * design);
        }
        for piece in &table.induced.pieces {
            for &node in &piece.nodes {
                assert!(
                    (check.cl - node).abs() > 1e-6,
                    "{}: checking CL {} was a fitting node {node}",
                    preset.name,
                    check.cl
                );
            }
        }
        // Induced drag is a dissipated kinetic energy and cannot be
        // negative anywhere on the lift range the table serves.
        let cl_max = table.cl[table.cl.len() - 1];
        for n in 0..=200 {
            let cl = cl_max * f64::from(n) / 200.0;
            assert!(table.induced_cd(cl) >= -1e-12, "{}: CL {cl}", preset.name);
        }
    }
}

#[test]
fn preset_trimmed_span_efficiency_passes_transport_screening() {
    for preset in presets() {
        let table = &preset.table;
        let e = table.effective_oswald(table.design_cl());
        // These preset wing-tail states remain below the planar elliptic
        // e=1 benchmark. Munk's bound applies to a planar sheet; separated
        // or dihedral sheets need not obey it for every possible loading.
        assert!(e > 0.0 && e <= 1.0, "{}: e = {e}", preset.name);
        // Cantwell, Aircraft and Rocket Propulsion (Stanford, May 2024),
        // ch. 1 pp. 1-6--1-7: ordinary wing span efficiencies 0.7--0.9;
        // optimized planar loading approaches the theoretical e=1 bound.
        // The 0.7 floor screens these transport presets, not every possible
        // wing. This is inviscid wake drag; OpenAP's fitted viscous Oswald
        // factors must not be substituted for it.
        assert!(
            e >= 0.7,
            "{}: wake e = {e} below the typical transport screening range",
            preset.name
        );
    }
}

#[test]
fn the_compressibility_drag_is_the_analysis_law_with_its_creep_down_to_low_mach() {
    for preset in presets() {
        let (table, aero) = (&preset.table, preset.aero());
        let cl_max = table.cl[table.cl.len() - 1];
        for n in 1..=30 {
            let cl = 0.05 * f64::from(n);
            if cl > cl_max {
                break;
            }
            let (_, native_critical) = aero.korn_mach_numbers(cl, None);
            assert!((table.critical_mach(cl) - native_critical).abs() < 1e-12);
            // Shevell's curve has no onset Mach: it is positive at every Mach
            // and the table carries it at its nodes.
            for &mach in &[
                table.mach[0],
                table.mach[table.mach.len() / 2],
                table.design_mach(),
            ] {
                let direct = aero.wave_drag(mach, cl, None);
                assert!(direct > 0.0, "{}: CL {cl} M {mach}", preset.name);
                assert!(
                    (table.wave_cd(cl, mach) - direct).abs() <= CD_ERROR_BOUND,
                    "{}: CL {cl} M {mach}: {} vs {direct}",
                    preset.name,
                    table.wave_cd(cl, mach)
                );
            }
            // More Mach, more compressibility drag.
            assert!(table.wave_cd(cl, table.design_mach()) > table.wave_cd(cl, table.mach[0]));
        }
        // Negligible at low speed and cruise lift: under a tenth of a count.
        assert!(table.wave_cd(0.5, 0.3) < 1.0e-5, "{}", preset.name);
    }
}

#[test]
fn the_minimum_drag_lift_coefficient_is_the_polar_optimum_below_the_wave_rise() {
    for preset in presets() {
        let table = &preset.table;
        // The polar is no longer a quadratic once the upsweep follows the
        // trimmed attitude, and its slope breaks where the body passes the
        // 5 deg curve of the upsweep figure, so the optimum is checked
        // against a dense scan of CL/CD rather than a closed form.
        let (mach, altitude_m) = (0.3, 1_000.0);
        let ratio = |cl: f64| cl / table.cd(cl, mach, altitude_m);
        let (low, high) = (table.cl[0], table.cl[table.cl.len() - 1]);
        let samples = 40_000;
        let step = (high - low) / f64::from(samples);
        let optimum = (0..=samples)
            .map(|index| low + f64::from(index) * step)
            .max_by(|left, right| ratio(*left).total_cmp(&ratio(*right)))
            .expect("a nonempty CL grid");
        if table.wave_cd(optimum, mach) == 0.0 {
            let found = table.min_drag_cl(mach, altitude_m);
            let alpha = table.body_alpha_and_slope(found, mach).0;
            assert!(
                (found - optimum).abs() <= 2.0 * step && ratio(found) >= ratio(optimum) - 1e-9,
                "{}: {found} vs {optimum} (body alpha {alpha:.3} deg)",
                preset.name
            );
        }
    }
}

#[test]
fn induced_cells_reproduce_design_drag_and_pass_independent_trimmed_solves() {
    for preset in presets() {
        let (table, aero) = (&preset.table, preset.aero());
        assert!(
            (table.lattice_induced_cd(preset.design.cl) - preset.design.cd_induced).abs() < 1e-12,
            "{}: design induced drag was not retained",
            preset.name
        );
        for (low, high) in table.induced.ranges() {
            for fraction in [0.375, 0.625] {
                let target = low + fraction * (high - low);
                let (cl, direct) = induced::trimmed_node(&aero, &preset.design, target)
                    .unwrap_or_else(|error| panic!("{}: CL {target}: {error}", preset.name));
                let relative = (table.lattice_induced_cd(cl) - direct).abs() / direct;
                assert!(
                    relative <= INDUCED_CHECK_RELATIVE_TOLERANCE,
                    "{}: off-check CL {cl} error {relative:.6}",
                    preset.name
                );
            }
        }
        let cl = table.design_cl();
        let mach = table.design_mach();
        let altitude = table.reference_altitude_m();
        let (cd0, k) = table.parabolic_equivalent_at(cl, mach, altitude);
        assert!((cd0 + k * cl * cl - table.cd(cl, mach, altitude)).abs() < 1e-12);
        let step = 1e-6;
        let slope = (table.cd(cl + step, mach, altitude) - table.cd(cl - step, mach, altitude))
            / (2.0 * step);
        assert!(
            (2.0 * k * cl - slope).abs() < 1e-7,
            "{}: tangent slope {} vs finite difference {slope}",
            preset.name,
            2.0 * k * cl
        );
    }
}

#[test]
fn screening_induced_fit_retains_trim_and_checked_clean_range() {
    for preset in presets() {
        let aero = preset.aero();
        let table = TrimmedDragTable::build_with_check_set(&aero, &preset.design, true)
            .unwrap_or_else(|error| panic!("{}: {error}", preset.name));
        assert!(
            (table.lattice_induced_cd(preset.design.cl) - preset.design.cd_induced).abs() < 1e-12,
            "{} design trim",
            preset.name
        );
        for target in [0.0, preset.design.cl_max_clean] {
            let (cl, direct) = induced::trimmed_node(&aero, &preset.design, target)
                .unwrap_or_else(|error| panic!("{}: {error}", preset.name));
            assert!(
                (table.lattice_induced_cd(cl) - direct).abs() / direct
                    <= INDUCED_CHECK_RELATIVE_TOLERANCE,
                "{} clean-CL endpoint",
                preset.name
            );
        }
        for target in (0..=20).map(|node| preset.design.cl_max_clean * f64::from(node) / 20.0) {
            assert!(
                table.induced_cd(target) >= -1e-12,
                "{} wake energy",
                preset.name
            );
        }
    }
}

#[test]
fn a_design_point_with_a_loose_moment_residual_is_retrimmed() {
    let preset = Trimmed::new("A320-200");
    let aero = preset.aero();
    let mut loose = preset.design.clone();
    loose.trim.trim_ih_deg += 5e-4 / loose.trim.cm_ih;
    let displaced = aero
        .trimmed_performance(
            &alas_aero::analysis::TrimPoint {
                trim_alpha_deg: loose.trim.trim_alpha_deg,
                trim_ih_deg: loose.trim.trim_ih_deg,
                cl_alpha: loose.trim.cl_alpha,
            },
            loose.mach,
            loose.altitude_m,
        )
        .unwrap();
    loose.cl = displaced.cl;
    loose.cd_induced = displaced.cd_induced;
    loose.cm_residual = displaced.cm_residual;
    assert!(loose.cm_residual.abs() > 1e-7);
    assert!(loose.cm_residual.abs() < loose.cm_tolerance);
    let (cl, corrected) = induced::trimmed_node(&aero, &loose, loose.cl).unwrap();
    assert!((cl - loose.cl).abs() <= 1e-7);
    assert!((corrected - loose.cd_induced).abs() > 1e-8);
    let table = TrimmedDragTable::build(&aero, &loose).unwrap();
    assert!((table.lattice_induced_cd(cl) - corrected).abs() < 1e-12);
    // The lattice factorization update (Schur complement) differs from a
    // fresh LU by rounding, so the retrimmed node is the same to 1e-9, not
    // the same bits.
    assert!(table
        .induced
        .pieces
        .iter()
        .any(|piece| (piece.nodes[1] - cl).abs() <= 1e-9));

    // A separate trim from the standard initial state verifies the same
    // physical target. A 1e-7 CL/Cm stopping error resolves CDi much more
    // closely than the table's 0.3% representation bound.
    let independent = alas_stab::trim::stability_and_trim(
        &preset.plane,
        &preset.config.analysis,
        loose.cl,
        loose.mach,
        loose.altitude_m,
    )
    .unwrap();
    let converged = aero
        .trimmed_performance(
            &alas_aero::analysis::TrimPoint {
                trim_alpha_deg: independent.trim_alpha_deg,
                trim_ih_deg: independent.trim_ih_deg,
                cl_alpha: independent.cl_alpha,
            },
            loose.mach,
            loose.altitude_m,
        )
        .unwrap();
    assert!(converged.cm_residual.abs() <= 1e-7);
    assert!((corrected - converged.cd_induced).abs() / converged.cd_induced < 1e-5);
}

#[test]
fn lift_dependent_viscous_drag_adds_exactly_kappa_cd0_cl_squared() {
    // AERL-04 / DRG-01 on the A320 table: CD(CL) - CD_off(CL) = kappa CD0 CL^2
    // at fixed M and h, CD0 the whole profile drag (Jenkinson 1999,
    // pp. 179-180), and the inviscid terms are untouched.
    let kappa = 0.30;
    let off = Trimmed::new("A320-200");
    let on = Trimmed::with_viscous_fraction("A320-200", kappa);
    let (mach, altitude_m) = (off.design.mach, off.design.altitude_m);
    assert_eq!(
        on.table.cd0(mach, altitude_m).to_bits(),
        off.table.cd0(mach, altitude_m).to_bits()
    );
    let cd0 = on.table.cd0(mach, altitude_m);
    for cl in [0.2, 0.35, 0.5, 0.8, 1.1] {
        let added = on.table.cd(cl, mach, altitude_m) - off.table.cd(cl, mach, altitude_m);
        assert!(
            (added - kappa * cd0 * cl * cl).abs() < 1e-12,
            "CL {cl}: {added}"
        );
        assert_eq!(
            on.table.induced_cd(cl).to_bits(),
            off.table.induced_cd(cl).to_bits()
        );
        assert_eq!(
            off.table.viscous_lift_cd_at(cl, mach, altitude_m),
            0.0,
            "disabled term is exactly zero"
        );
    }
}

#[test]
fn the_total_oswald_factor_follows_the_viscous_decomposition() {
    // 1/e_total = 1/e_inviscid(trim) + kappa CD0 pi A at fixed CL.
    let kappa = 0.30;
    let on = Trimmed::with_viscous_fraction("A320-200", kappa);
    let (mach, altitude_m) = (on.design.mach, on.design.altitude_m);
    let aspect = on.plane.b_ref * on.plane.b_ref / on.plane.s_ref;
    let cd0 = on.table.cd0(mach, altitude_m);
    for cl in [0.4, 0.5, 0.7] {
        let e_inv = on.table.effective_oswald(cl);
        let e_total = on.table.effective_oswald_total(cl, mach, altitude_m);
        let expected = 1.0 / (1.0 / e_inv + kappa * cd0 * std::f64::consts::PI * aspect);
        assert!((e_total - expected).abs() < 1e-12, "CL {cl}");
        assert!(e_total < e_inv);
    }
}

#[test]
fn the_tangent_parabola_and_the_min_drag_cl_see_the_viscous_curvature() {
    let kappa = 0.30;
    let on = Trimmed::with_viscous_fraction("A320-200", kappa);
    let (mach, altitude_m) = (0.60, on.design.altitude_m);
    // Slope of the tangent parabola equals the finite-difference dCD/dCL
    // / (2 CL), viscous term included.
    let cl = 0.55;
    let h = 1e-4;
    let (cd0_eq, k) = on.table.parabolic_equivalent_at(cl, mach, altitude_m);
    let fd =
        (on.table.cd(cl + h, mach, altitude_m) - on.table.cd(cl - h, mach, altitude_m)) / (2.0 * h);
    assert!(
        (2.0 * k * cl - fd).abs() < 2e-4 * fd.abs(),
        "{} vs {fd}",
        2.0 * k * cl
    );
    assert!((cd0_eq + k * cl * cl - on.table.cd(cl, mach, altitude_m)).abs() < 1e-12);
    // Below the wave onset CD = CD0 + (k_i + kappa CD0) CL^2 locally, so a
    // larger kappa moves the best-L/D CL down by the analytic factor
    // sqrt(k_off / k_on) on a purely quadratic polar; check the direction
    // and the closed form of the tangent curvature.
    let off = Trimmed::new("A320-200");
    let cl_best_off = off.table.min_drag_cl(mach, altitude_m);
    let cl_best_on = on.table.min_drag_cl(mach, altitude_m);
    assert!(cl_best_on < cl_best_off, "{cl_best_on} vs {cl_best_off}");
    let (_, k_off) = off.table.parabolic_equivalent_at(cl, mach, altitude_m);
    let cd0_here = on.table.cd0(mach, altitude_m);
    assert!((k - k_off - kappa * cd0_here).abs() < 1e-9);
}

/// D8 evidence for the geometric body-alpha window: the incompressible and
/// compressible cruise trim angle of every registered transport preset at its
/// nominal design. Run with
/// `cargo test -p alas-opt --release --lib body_alpha_window_probe -- --ignored --nocapture`
/// and derive `geometric_body_alpha_min/max_deg` from the compressible column.
#[test]
#[ignore = "evidence probe; prints a table"]
// A probe prints its numbers; it asserts nothing.
#[allow(clippy::print_stdout)]
fn body_alpha_window_probe() {
    for preset in presets() {
        let attitude = preset.design.trim.attitude();
        println!(
            "ALPHA {:<10} incompressible {:7.3} deg  compressible {:7.3} deg  ih {:6.3} -> {:6.3}",
            preset.name,
            preset.design.trim.trim_alpha_deg,
            attitude.alpha_deg,
            preset.design.trim.trim_ih_deg,
            attitude.incidence_deg
        );
    }
}

#[test]
fn the_high_lift_base_polar_carries_no_lift_dependent_viscous_term() {
    // Torenbeek (2013) p. 96 states k_L for the en-route configuration: a
    // high-lift configuration adds its own increment to the polar without it.
    let on = Trimmed::with_viscous_fraction("A320-200", 0.43);
    let off = Trimmed::new("A320-200");
    let (mach, altitude_m) = (on.design.mach, on.design.altitude_m);
    for cl in [0.5, 1.0, 1.5] {
        let base = on.table.cd_without_lift_viscous(cl, mach, altitude_m, 0.0);
        let clean = on.table.cd(cl, mach, altitude_m);
        let term = on.table.viscous_lift_cd_at(cl, mach, altitude_m);
        assert!(term > 0.0);
        // The term scales the whole zero-lift profile drag (Jenkinson 1999,
        // pp. 179-180); the product books no separate upsweep item (Poll and
        // Schumann's fuselage FIFS holds the afterbody).
        let viscous_cd0 = on.table.cd0(mach, altitude_m);
        assert_eq!(on.table.upsweep_cd0(mach), 0.0);
        assert!(
            (term - 0.43 * viscous_cd0 * cl * cl).abs() < 1e-15,
            "CL {cl}"
        );
        assert!((clean - base - term).abs() < 1e-15, "CL {cl}");
        assert_eq!(
            base.to_bits(),
            off.table.cd(cl, mach, altitude_m).to_bits(),
            "CL {cl}: the base equals the polar with the term switched off"
        );
    }
}

#[test]
fn the_upsweep_flies_at_the_trimmed_body_attitude() {
    use alas_aero::analysis::upsweep_frontal_cd;
    for preset in presets() {
        let table = &preset.table;
        let (mach, altitude_m) = (table.design_mach(), table.reference_altitude_m());
        // The design node is the design trim itself.
        let (alpha, cl_trim, _) = table.design_attitude;
        let (at_design, slope) = table.body_alpha_and_slope(cl_trim, mach);
        assert!(
            (at_design - alpha).abs() < 1e-9,
            "{}: {at_design} vs {alpha}",
            preset.name
        );
        assert!(slope > 0.0, "{}: dalpha/dCL {slope}", preset.name);
        let Some(upsweep) = table.upsweep else {
            continue;
        };
        for cl in [0.3, 0.5, 0.7] {
            let (body_alpha, _) = table.body_alpha_and_slope(cl, mach);
            // The afterbody flies the body attitude less the wing's downwash
            // at the body trailing edge, on the DATCOM ramp.
            let flow = upsweep.flow;
            assert!(
                flow.ramp_mean > 0.1 && flow.ramp_mean <= 1.0,
                "{}",
                preset.name
            );
            assert!(flow.eps_per_cl_deg > 0.5, "{}: {flow:?}", preset.name);
            let afterbody_alpha = flow.alpha_deg(body_alpha, cl);
            let (front, _) = upsweep_frontal_cd(upsweep.upsweep_rad, afterbody_alpha);
            let (front_0, _) = upsweep_frontal_cd(upsweep.upsweep_rad, 0.0);
            let column = table.drag_area_cd(upsweep.column, mach);
            let increment = table.upsweep_attitude_cd(cl, mach);
            if front_0 > 0.0 {
                assert!((increment - column * (front / front_0 - 1.0)).abs() < 1e-15);
            }
            // Afterbody nose up, the measured penalty is smaller than at
            // alpha 0.
            if afterbody_alpha >= 0.0 {
                assert!(increment <= 0.0, "{}: CL {cl}: {increment}", preset.name);
            }
            // The analytic slope the tangent parabola uses is the derivative.
            let h = 1e-6;
            let numeric = (table.upsweep_attitude_cd(cl + h, mach)
                - table.upsweep_attitude_cd(cl - h, mach))
                / (2.0 * h);
            let analytic = table.upsweep_attitude_value_and_slope(cl, mach).1;
            assert!(
                (numeric - analytic).abs() < 1e-8,
                "{}: {numeric} vs {analytic}",
                preset.name
            );
            // A high-lift configuration flies the attitude of the clean
            // polar shifted by the lift its devices add.
            // The downwash is that of its full lift.
            for shift in [0.2, 0.6] {
                let high_lift = table.upsweep_attitude_cd_high_lift(cl + shift, mach, shift);
                let afterbody = flow.alpha_deg(body_alpha, cl + shift);
                let front = upsweep_frontal_cd(upsweep.upsweep_rad, afterbody).0;
                assert!((high_lift - column * (front / front_0 - 1.0)).abs() < 1e-15);
            }
            // Every clean sum carries it once.
            let cd0 = table.cd0(mach, altitude_m);
            let parts = cd0
                + table.viscous_lift_cd(cl, cd0)
                + increment
                + table.induced_cd_at(cl, mach)
                + table.wave_cd(cl, mach);
            assert!((table.cd(cl, mach, altitude_m) - parts).abs() < 1e-15);
        }
    }
}

#[test]
fn a_node_started_from_solved_neighbours_meets_the_cold_start_node_tolerance() {
    for preset in presets() {
        let aero = preset.aero();
        let design = &preset.design;
        let cold = |target: f64| {
            induced::solve_node(&aero, design, target, &[])
                .unwrap_or_else(|error| panic!("{}: CL {target}: {error}", preset.name))
        };
        // The cruise curve's own order: both sides of the cruise node first,
        // then a target between them and one beyond the outer node.
        let lower = cold(0.35 * design.cl).1;
        let upper = cold(1.4 * design.cl).1;
        for target in [
            0.7 * design.cl,
            1.2 * design.cl,
            0.1 * design.cl,
            1.6 * design.cl,
        ] {
            let ((cl_cold, cd_cold), state_cold) = cold(target);
            let ((cl_warm, cd_warm), state_warm) =
                induced::solve_node(&aero, design, target, &[lower, upper])
                    .unwrap_or_else(|error| panic!("{}: CL {target}: {error}", preset.name));
            // Both are roots of the same lattice residual to its 1e-7
            // tolerance: the node values agree to that, not bit for bit.
            assert!((cl_warm - cl_cold).abs() < 5.0e-7, "{} CL", preset.name);
            assert!(
                (cd_warm - cd_cold).abs() < 1.0e-5 * cd_cold,
                "{}: CL {target}: induced drag {cd_warm} vs {cd_cold}",
                preset.name
            );
            assert!(
                (state_warm.alpha_deg - state_cold.alpha_deg).abs() < 1.0e-4,
                "{} attitude",
                preset.name
            );
        }
    }
}

#[test]
fn a_query_outside_the_solved_nodes_is_flagged_and_keeps_its_value() {
    let trimmed = Trimmed::new("A320-200");
    let table = &trimmed.table;
    let range = table.solved_range();
    let (cl, mach) = (table.design_cl(), table.design_mach());
    let altitude_m = table.reference_altitude_m();
    assert!(range.induced_cl.0 < cl && cl < range.induced_cl.1);
    assert_eq!(table.out_of_range(cl, mach), None);

    let high = 1.2 * range.induced_cl.1;
    let flagged = table.out_of_range(high, mach).unwrap();
    assert_eq!(flagged.induced_cl, Some(RangeSide::Above));
    assert_eq!(flagged.mach, None);
    // The induced curve is solved through zero lift (its lowest node is at
    // CL = 0), so a query below it lies at a margin under that node, not at
    // a fraction of it.
    let lowest = range.induced_cl.0;
    let low = table
        .out_of_range(lowest - (0.1 * lowest.abs()).max(0.1), mach)
        .unwrap();
    assert_eq!(low.induced_cl, Some(RangeSide::Below));
    let fast = table.out_of_range(cl, range.mach.1 + 0.05).unwrap();
    assert_eq!(fast.mach, Some(RangeSide::Above));
    assert_eq!(fast.induced_cl, None);
    let undefined = table.out_of_range(f64::NAN, mach).unwrap();
    assert_eq!(undefined.induced_cl, Some(RangeSide::NotANumber));

    // The flag accompanies the value; it does not change it.
    let (value, state) = table.cd_checked(high, mach, altitude_m);
    assert_eq!(value.to_bits(), table.cd(high, mach, altitude_m).to_bits());
    assert_eq!(state, Some(flagged));
    let drag: &dyn crate::mdo::mission_model::CruiseDrag = table.as_ref();
    assert_eq!(drag.out_of_range(high, mach), Some(flagged));
}

#[test]
fn the_twist_allowance_replaces_the_basic_loading_and_keeps_the_elliptic_bound() {
    use alas_aero::analysis::NON_OPTIMUM_TWIST_DRAG_FACTOR;
    for preset in presets() {
        let table = &preset.table;
        let basic = table.basic_loading_cd();
        // The declared twist leaves the trimmed lattice a zero-lift loading.
        assert!(basic > 0.0, "{}: basic loading {basic}", preset.name);
        let elliptic = 1.0 / (std::f64::consts::PI * table.span_bound_aspect_ratio);
        assert!(
            table.span_bound_aspect_ratio >= table.aspect_ratio,
            "{}",
            preset.name
        );
        let cl_max = table.cl[table.cl.len() - 1];
        for n in 0..=40 {
            let cl = cl_max * f64::from(n) / 40.0;
            let replaced =
                table.lattice_induced_cd(cl) - basic + NON_OPTIMUM_TWIST_DRAG_FACTOR * cl * cl;
            let expected = replaced.max(elliptic * cl * cl);
            assert!(
                (table.induced_cd(cl) - expected).abs() < 1e-12,
                "{}: CL {cl}: {} vs {expected}",
                preset.name,
                table.induced_cd(cl)
            );
        }
    }
}

/// Two redesigned A220-300 wings 0.03 m of wing position apart (the first is
/// the design a 418-evaluation search delivered as its best) have the same
/// trimmed wake energy. On the first a wing trailing leg fell 0.5 mm from
/// the start of the tailplane's tip fairing and cut a strip of that width
/// (`alas_aero::vlm` strip boundaries): the Goethert-stretched lattice then
/// returned 109 counts of induced drag at zero lift against 24, the table's
/// basic-loading term took 94 counts out and the cruise induced drag fell to
/// the elliptic bound, 2 percent of block fuel the design did not earn.
#[test]
fn a_wing_leg_beside_the_tail_fairing_does_not_move_the_trimmed_wake_energy() {
    let designs: [[f64; 16]; 2] = [
        [
            33.396805241577155,
            7.367921124423383,
            3.6798745058175837,
            1.2757243354831973,
            31.504143517734413,
            -1.5,
            -0.16267281203372247,
            1.0,
            38.7,
            0.0,
            0.9878619525778268,
            1.0838264214443805,
            -0.00026299946847206527,
            0.0001739699631025908,
            -0.0013185018443230962,
            0.0,
        ],
        [
            33.396805241577155,
            7.367921124423383,
            3.6798745058175837,
            1.2757243354831973,
            31.504143517734413,
            -1.5,
            -0.19267281203372247,
            1.0,
            38.7,
            0.0,
            0.9878619525778268,
            1.0838264214443805,
            -0.00026299946847206527,
            0.0001739699631025908,
            -0.0013185018443230962,
            0.0,
        ],
    ];
    let drag: Vec<[f64; 2]> = designs
        .iter()
        .map(|x| {
            let config =
                AlasConfig::from_value(&serde_json::json!({ "preset": "A220-300" })).unwrap();
            let (config, dv, mut plane) =
                build_geometry(&config, x).unwrap_or_else(|f| panic!("{}", f.reason));
            let (_, _, cg, _, _, _) =
                first_mass_pass(&config, &dv, &plane).unwrap_or_else(|f| panic!("{}", f.reason));
            plane.xyz_ref[0] = cg[0];
            let req = &config.requirements;
            let beta = alas_stab::neutral_point::beta_for_mach(req.cruise_mach);
            let stretched = alas_stab::neutral_point::goethert_stretch(&plane, beta);
            let aero = AeroAnalysis::new(
                &plane,
                AeroAnalysis::quarter_chord_sweep_deg(&plane, dv.sweep_deg),
                Some(config.geometry.clone()),
                Some(config.resolved_drag_model()),
                Some(config.analysis.clone()),
            );
            // The zero-lift node of the cruise curve at its stabilizer
            // incidence, and the same attitude a degree of incidence below.
            [5.0, 4.0].map(|incidence_deg| {
                let point = alas_aero::analysis::TrimPoint {
                    trim_alpha_deg: -6.27,
                    trim_ih_deg: incidence_deg,
                    cl_alpha: 0.1,
                };
                aero.trimmed_inviscid_stretched(
                    &stretched,
                    &point,
                    None,
                    req.cruise_mach,
                    req.cruise_altitude_m,
                    beta,
                    0.0,
                )
                .unwrap()
                .cd_induced
            })
        })
        .collect();
    for (first, second) in drag[0].iter().copied().zip(drag[1].iter().copied()).take(2) {
        assert!(
            (first / second - 1.0).abs() < 0.02 && first < 0.004,
            "induced drag {first} against {second} 0.03 m of wing position away"
        );
    }
}

/// The search models' table (`AnalysisConfig::rotated_tail_nodes`: the
/// nodes of a curve on the design trim's lattice, the incidence as a rotation
/// of the tail normals) against the exact-lattice table of the same
/// candidate, over the whole tabulated lift range. The difference is the
/// tail's vortex displacement the rotation leaves out, second order in the
/// incidence change; the reporting analysis never uses this table.
#[test]
fn the_rotated_tail_search_table_tracks_the_exact_lattice_table() {
    for preset in presets() {
        let mut analysis = preset.config.analysis.clone();
        analysis.rotated_tail_nodes = true;
        let aero = AeroAnalysis::new(
            &preset.plane,
            preset.sweep_deg,
            Some(preset.config.geometry.clone()),
            Some(preset.config.resolved_drag_model()),
            Some(analysis),
        );
        let rotated = TrimmedDragTable::build(&aero, &preset.design)
            .unwrap_or_else(|error| panic!("{}: {error}", preset.name));
        let exact = &preset.table;
        let scale = exact.lattice_induced_cd(preset.design.cl);
        let (low, high) = (exact.cl[0], exact.cl[exact.cl.len() - 1]);
        let worst = (0..=40)
            .map(|n| {
                let cl = low + (high - low) * f64::from(n) / 40.0;
                (rotated.lattice_induced_cd(cl) - exact.lattice_induced_cd(cl)).abs() / scale
            })
            .fold(0.0_f64, f64::max);
        let design = (rotated.lattice_induced_cd(preset.design.cl) - scale).abs() / scale;
        assert!(
            worst <= ROTATED_TAIL_TABLE_TOLERANCE,
            "{}: rotated-tail induced drag off by {worst:.2e} of the design value \
             ({design:.2e} at the design CL)",
            preset.name
        );
    }
}

/// Bound on the rotated-tail table's induced-drag difference from the exact
/// table, as a fraction of the design-point induced drag.
const ROTATED_TAIL_TABLE_TOLERANCE: f64 = 5.0e-3;
