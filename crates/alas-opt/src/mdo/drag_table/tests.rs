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
use crate::mdo::trim::trim_and_polar;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;

const PRESETS: [&str; 7] = [
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
        let polar = trim_and_polar(&config, &mut plane, cg[0], &dv, seed_kg)
            .unwrap_or_else(|f| panic!("{name}: {}", f.reason));
        Self {
            name,
            sweep_deg: AeroAnalysis::quarter_chord_sweep_deg(&plane, dv.sweep_deg),
            table: polar
                .drag
                .table()
                .cloned()
                .expect("a native trim carries its table"),
            config,
            plane,
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
fn the_trimmed_span_efficiency_respects_munks_bound() {
    for preset in presets() {
        let table = &preset.table;
        let e = table.effective_oswald(table.design_cl());
        // Munk's minimum-induced-drag theorem: no planar lifting system of
        // the wing's span beats elliptic loading, e = 1; a trimmed
        // wing-tail pair only adds tail-load drag to that.
        assert!(e > 0.0 && e <= 1.0, "{}: e = {e}", preset.name);
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
        let [c0, _, c2] = table.induced;
        let optimum = ((table.cd0(mach, altitude_m) + c0) / c2).sqrt();
        let cl_max = table.cl[table.cl.len() - 1];
        if optimum < cl_max && table.wave_cd(optimum, mach) == 0.0 {
            let found = table.min_drag_cl(mach, altitude_m);
            assert!(
                (found - optimum).abs() < 1e-6,
                "{}: {found} vs {optimum}",
                preset.name
            );
        }
    }
}
