// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Physical invariants and error bounds of [`super::TrimmedDragTable`] on
//! the transport presets, each trimmed at its sizing seed mass exactly as
//! `mdo::sizing` trims its first pass.

// The tests build their own configurations, so a failed `expect` is the
// assertion failing rather than a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]

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
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
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
            Some(config.drag_model.clone()),
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
            Some(self.config.drag_model.clone()),
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
                    let direct = parasite + table.induced_cd(cl) + aero.wave_drag(mach, cl, None);
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
            assert!(table.induced_cd(cl) >= 0.0, "{}: CL {cl}", preset.name);
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
fn wave_drag_is_zero_below_the_critical_and_onset_mach() {
    for preset in presets() {
        let (table, aero) = (&preset.table, preset.aero());
        let onset = preset.config.drag_model.wave_drag_onset_mach;
        for n in 1..=30 {
            let cl = 0.05 * f64::from(n);
            let (_, native_critical) = aero.korn_mach_numbers(cl, None);
            assert!((table.critical_mach(cl) - native_critical).abs() < 1e-12);
            let below = native_critical.min(onset) - 1e-4;
            assert_eq!(table.wave_cd(cl, below), 0.0, "{}: CL {cl}", preset.name);
            assert_eq!(aero.wave_drag(below, cl, None), 0.0);
            let above = native_critical.max(onset) + 0.02;
            if above <= table.mach[table.mach.len() - 1] {
                assert!(table.wave_cd(cl, above) > 0.0, "{}: CL {cl}", preset.name);
            }
        }
    }
}

#[test]
fn the_minimum_drag_lift_coefficient_is_the_polar_optimum_below_the_wave_rise() {
    for preset in presets() {
        let table = &preset.table;
        // With no wave drag, CL/CD = CL / (A + c1 CL + c2 CL^2) peaks at
        // CL = sqrt(A / c2), A = CD0 + c0: the linear trim term does not move
        // the optimum.
        let (mach, altitude_m) = (0.3, 1_000.0);
        let cd0 = table.cd0(mach, altitude_m);
        let cl_max = table.cl[table.cl.len() - 1];
        let optimum = table
            .induced
            .ranges()
            .filter_map(|(low, high)| {
                let (low, high) = (low.max(table.cl[0]), high.min(cl_max));
                if low >= high {
                    return None;
                }
                let [c0, _, c2] = table.induced.coefficients_at(0.5 * (low + high));
                Some(((cd0 + c0) / c2).sqrt().clamp(low, high))
            })
            .max_by(|left, right| {
                (left / table.cd(*left, mach, altitude_m))
                    .total_cmp(&(right / table.cd(*right, mach, altitude_m)))
            })
            .expect("a nonempty induced range");
        if table.wave_cd(optimum, mach) == 0.0 {
            let found = table.min_drag_cl(mach, altitude_m);
            assert!(
                (found - optimum).abs() < 1e-6,
                "{}: {found} vs {optimum}",
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
            (table.induced_cd(preset.design.cl) - preset.design.cd_induced).abs() < 1e-12,
            "{}: design induced drag was not retained",
            preset.name
        );
        for (low, high) in table.induced.ranges() {
            for fraction in [0.375, 0.625] {
                let target = low + fraction * (high - low);
                let (cl, direct) = induced::trimmed_node(&aero, &preset.design, target)
                    .unwrap_or_else(|error| panic!("{}: CL {target}: {error}", preset.name));
                let relative = (table.induced_cd(cl) - direct).abs() / direct;
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
fn a_design_point_with_a_loose_moment_residual_is_retrimmed() {
    let preset = Trimmed::new("A320-200");
    let aero = preset.aero();
    let mut loose = preset.design;
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
    assert!((table.induced_cd(cl) - corrected).abs() < 1e-12);
    assert!(table
        .induced
        .pieces
        .iter()
        .any(|piece| piece.nodes[1] == cl));

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
