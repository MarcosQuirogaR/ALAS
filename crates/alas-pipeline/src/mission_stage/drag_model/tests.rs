// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;
use crate::full_analysis::FullAnalysis;
use crate::mission_stage::build_analyses;
use alas_opt::mdo::mission_model::ParabolicPolar;

#[test]
fn every_native_preset_mission_reads_the_carried_trimmed_drag_table() {
    for name in [
        "A320-200",
        "A220-300",
        "A340-300",
        "A380-800",
        "B787-9",
        "DC-10",
        "ATR72-600",
        "AVE",
    ] {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": name}))
            .unwrap_or_else(|error| panic!("{name} config: {error}"));
        let design = alas_config::presets::get(name)
            .unwrap_or_else(|error| panic!("{name} preset: {error}"))
            .design_vector;
        let report = FullAnalysis::new(config.clone())
            .run(&design, true)
            .unwrap_or_else(|error| panic!("{name} report: {error}"));
        let artifacts = report
            .fuel
            .artifacts(&config, &report.design)
            .unwrap_or_else(|error| panic!("{name} artifacts: {error}"));
        let table = artifacts
            .drag
            .table()
            .unwrap_or_else(|| panic!("{name} has a native trimmed table"));
        let adapter = CandidateMissionDrag::from_report(&config, &report)
            .unwrap_or_else(|error| panic!("{name} adapter: {error}"));
        let CandidateDrag::Table(shared_table) = &adapter.drag else {
            panic!("{name} adapter changed the drag source");
        };
        assert!(Arc::ptr_eq(table, shared_table), "{name} copied the table");

        let mut analyses = build_analyses(&config, &report)
            .unwrap_or_else(|error| panic!("{name} analyses: {error}"));
        assert!(matches!(
            analyses.drag_source,
            MissionDragSource::SharedCandidate(_)
        ));
        assert_eq!(analyses.reference_area_m2, artifacts.reference_area_m2);

        // Poison the obsolete buildup inputs: a product mission must never
        // read the frozen-parity settings or the surrogate induced drag.
        analyses.drag_settings.drag_coefficient_increment = f64::NAN;
        analyses.induced_drag_lift_correction = f64::NAN;
        analyses.wings.clear();
        analyses.fuselages.clear();
        analyses.nacelles.clear();
        for (altitude_m, temperature_deviation_k) in [(0.0, 0.0), (11_000.0, 17.0)] {
            let atmosphere = analyses.atmosphere(altitude_m, temperature_deviation_k);
            for mach in [0.35, 0.7, config.requirements.cruise_mach] {
                let reynolds_number_per_m =
                    atmosphere.density_kg_m3 * mach * atmosphere.speed_of_sound_m_s
                        / atmosphere.dynamic_viscosity_pa_s;
                for alpha_rad in [0.03, 0.06, 0.09] {
                    let solution = analyses.aerodynamics(
                        alpha_rad,
                        mach,
                        atmosphere.temperature_k,
                        reynolds_number_per_m,
                    );
                    let cl = solution.lift_coefficient;
                    let parasite = table.cd0_at_reynolds_per_m(mach, reynolds_number_per_m);
                    let induced = table.induced_cd(cl);
                    let wave = table.wave_cd(cl, mach);
                    assert_eq!(
                        solution.drag.parasite_total.to_bits(),
                        parasite.to_bits(),
                        "{name} parasite drag diverged at CL {cl}, M {mach}"
                    );
                    assert_eq!(solution.drag.induced_total.to_bits(), induced.to_bits());
                    assert_eq!(solution.drag.compressible_total.to_bits(), wave.to_bits());
                    assert_eq!(
                        solution.drag.total.to_bits(),
                        (parasite + induced + wave).to_bits(),
                        "{name} native drag diverged at CL {cl}, M {mach}"
                    );
                    assert!(solution.drag.total.is_finite() && solution.drag.total > 0.0);
                    assert!(solution.wing_induced_drag_coefficient.is_empty());
                }
            }
        }
    }
}

#[test]
fn externally_measured_candidates_keep_the_same_shared_polar() {
    let polar = Arc::new(ParabolicPolar::new(0.021, 0.045, 0.0034, 0.8426));
    let drag = CandidateDrag::External(polar.clone());
    let adapter = CandidateMissionDrag { drag: drag.clone() };
    let CandidateDrag::External(shared_polar) = &adapter.drag else {
        panic!("external polar source changed");
    };
    assert!(Arc::ptr_eq(&polar, shared_polar));
    for cl in [0.2, 0.5, 0.8] {
        for mach in [0.4, 0.7, 0.8426] {
            let coefficients = adapter.coefficients(cl, mach, 1.0e7);
            assert_eq!(
                (coefficients.parasite + coefficients.induced + coefficients.wave).to_bits(),
                drag.cd(cl, mach, 11_000.0).to_bits()
            );
            assert_eq!(
                coefficients.wave.to_bits(),
                drag.wave_cd(cl, mach).to_bits()
            );
        }
    }
}
