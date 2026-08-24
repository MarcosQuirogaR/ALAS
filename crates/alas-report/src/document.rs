// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/design_report.py
// Reference: alas @ rust-port-baseline.

//! Design database export to JSON, Selig `.dat` airfoil coordinate files, and summary reports.
//!
//! Exposes serialization of optimized aircraft analysis reports to standard JSON
//! for external simulation handoffs and Selig format `.dat` files for CFD analysis.

pub use alas_pipeline::export::{
    export_airfoil_dat, export_json, format_summary, report_to_database, DesignDatabase,
};

#[cfg(test)]
mod tests {
    use super::*;
    use alas_aero::analysis::PolarSweep;
    use alas_config::design_variables::DesignVector;
    use alas_config::AlasConfig;
    use alas_geom::aircraft::airplane::Airplane;
    use alas_pipeline::full_analysis::{AnalysisReport, DesignPoint, PolarFit};
    use std::collections::HashMap;

    #[test]
    fn summary_formatting_produces_non_empty_string() {
        let mut report = AnalysisReport {
            design: DesignVector::default(),
            airplane: Airplane {
                name: "TestAirplane".to_owned(),
                xyz_ref: [0.0, 0.0, 0.0],
                wings: Vec::new(),
                fuselages: Vec::new(),
                s_ref: 120.0,
                c_ref: 4.0,
                b_ref: 35.0,
            },
            polar: PolarSweep {
                alpha_deg: Vec::new(),
                cl: Vec::new(),
                cd: Vec::new(),
                cd_induced: Vec::new(),
                cd_wave: Vec::new(),
                cd_parasite: Vec::new(),
                cm: Vec::new(),
                l_over_d: Vec::new(),
            },
            design_point: DesignPoint {
                alpha_deg: 2.5,
                cl: 0.52,
                cd: 0.028,
                l_over_d: 18.57,
            },
            polar_fit: PolarFit {
                cd0: 0.018,
                k: 0.045,
                oswald_e: 0.85,
                aspect_ratio: 9.5,
            },
            static_margin: 0.12,
            x_neutral_point: 16.2,
            trimmed_design_point: None,
            component_masses: HashMap::new(),
            mass_coordinates: HashMap::new(),
            physical_cg: [15.0, 0.0, 0.0],
            geometry_summary: HashMap::new(),
            payload_layout: None,
            cg_envelope_ok: Some(true),
        };
        report.component_masses.insert("Wing".to_owned(), 5000.0);
        report.geometry_summary.insert("span_m".to_owned(), 35.0);

        let config = AlasConfig::default();
        let summary = format_summary(&report, Some(&config));
        assert!(summary.contains("ALAS Design Summary"));
        assert!(summary.contains("Cruise alpha"));
        assert!(summary.contains("MTOW"));
    }
}
