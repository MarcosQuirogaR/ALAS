// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/pipeline.py (`StructuralAnalysisResult`, `_run_structural_analysis`).
// Reference: alas @ rust-port-baseline.

//! Structural wingbox sizing, mesh generation, analytical response, and solver execution.
//!
//! [`run_structural_analysis`] sizes the primary wingbox (caps, webs, skin, ribs),
//! generates the NASTRAN finite element mesh deck, calculates analytical deflection
//! and modal estimates, and optionally launches the NASTRAN solver if configured.

use std::path::{Path, PathBuf};

use alas_config::materials::get as get_material;
use alas_config::AlasConfig;
use alas_exec::RunEnvironment;
use alas_geom::wing_structure::WingStructureGeometry;
use alas_struct::analytical::{analyze_structure, StructuralAnalysisReport};
use alas_struct::mesh::{build_wing_mesh_bdf, MeshHealthReport};
use alas_struct::nastran::ResultStatus;
use alas_struct::nastran::{run_nastran_analysis, NastranResults};
use alas_struct::nastran95::run_nastran95_from_config_or_env;
use alas_struct::sizing::{size_wingbox, WingboxSizing};

use crate::full_analysis::AnalysisReport;
use crate::patran::run_patran_export;

/// Complete structural analysis outcomes for a pipeline run.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuralAnalysisResult {
    /// Overall structural status (`"ok"`, `"not_run"`, or `"error"`).
    pub status: String,
    /// Failure message if an error occurred.
    pub error: Option<String>,
    /// The rib/spar wingbox geometry the sizing and mesh were built from.
    pub wsg: Option<WingStructureGeometry>,
    /// Sized wingbox internal structural dimensions and masses.
    pub sizing: Option<WingboxSizing>,
    /// Finite-element mesh geometric health validation metrics.
    pub mesh_health: Option<MeshHealthReport>,
    /// Closed-form analytical deflection, stress, and natural frequency estimates.
    pub analysis: Option<StructuralAnalysisReport>,
    /// NASTRAN finite-element solution results, if executed.
    pub nastran: Option<NastranResults>,
    /// Independent NASA NASTRAN-95 solution results, when run alongside MSC.
    pub nastran95: Option<NastranResults>,
    /// Patran deformation renders, if an external export was performed.
    pub patran: Option<PatranExportResult>,
    /// Empirical Torenbeek wing mass, in kg, for comparison.
    pub torenbeek_wing_mass_kg: f64,
}

/// Output of the optional Patran deformation-render export.
///
/// The Python result stores an insertion-ordered dictionary of load-case names
/// to PNG paths. A vector preserves that order without introducing a map whose
/// iteration order would differ from the render order.
#[derive(Debug, Clone, PartialEq)]
pub struct PatranExportResult {
    /// Export state (`"not_run"`, `"ok"`, or `"error"`).
    pub status: String,
    /// Failure or partial-export detail, when present. Error text preserves
    /// the boundary category (absent, incomplete, invalid, launch, timeout,
    /// or artifact validation) so the GUI can provide recovery guidance.
    pub error: Option<String>,
    /// Load-case names and the corresponding rendered PNG paths.
    pub png_paths: Vec<(String, PathBuf)>,
}

impl Default for PatranExportResult {
    fn default() -> Self {
        Self {
            status: "not_run".to_owned(),
            error: None,
            png_paths: Vec::new(),
        }
    }
}

impl Default for StructuralAnalysisResult {
    fn default() -> Self {
        Self {
            status: "not_run".to_owned(),
            error: None,
            wsg: None,
            sizing: None,
            mesh_health: None,
            analysis: None,
            nastran: None,
            nastran95: None,
            patran: None,
            torenbeek_wing_mass_kg: f64::NAN,
        }
    }
}

/// Execute the wingbox structural sizing and analysis stage.
pub fn run_structural_analysis(
    config: &AlasConfig,
    report: &AnalysisReport,
    work_dir: Option<&Path>,
    environment: &RunEnvironment,
) -> StructuralAnalysisResult {
    run_structural_analysis_with_environment(config, report, work_dir, environment)
}

/// Execute structural analysis with resolved tools from the public run environment.
pub fn run_structural_analysis_with_environment(
    config: &AlasConfig,
    report: &AnalysisReport,
    work_dir: Option<&Path>,
    environment: &RunEnvironment,
) -> StructuralAnalysisResult {
    let scfg = &config.structures;
    let dv = &report.design;
    let req = &config.requirements;

    let torenbeek_mass = report
        .component_masses
        .get("Wing")
        .copied()
        .unwrap_or(f64::NAN);

    if !scfg.enabled {
        return StructuralAnalysisResult {
            status: "not_run".to_owned(),
            error: None,
            wsg: None,
            sizing: None,
            mesh_health: None,
            analysis: None,
            nastran: None,
            nastran95: None,
            patran: None,
            torenbeek_wing_mass_kg: torenbeek_mass,
        };
    }

    let skin_mat = match get_material(&scfg.skin_material) {
        Ok(m) => m,
        Err(e) => return structural_error(format!("invalid skin material: {e:?}"), torenbeek_mass),
    };
    let web_mat = match get_material(&scfg.spar_web_material) {
        Ok(m) => m,
        Err(e) => return structural_error(format!("invalid web material: {e:?}"), torenbeek_mass),
    };
    let cap_mat = match get_material(&scfg.spar_cap_material) {
        Ok(m) => m,
        Err(e) => return structural_error(format!("invalid cap material: {e:?}"), torenbeek_mass),
    };
    let rib_mat = match get_material(&scfg.rib_material) {
        Ok(m) => m,
        Err(e) => return structural_error(format!("invalid rib material: {e:?}"), torenbeek_mass),
    };

    let (spar_fracs, spar_full_span) = scfg.resolved_spars();

    let wing = match report.airplane.wings.first() {
        Some(w) => w,
        None => {
            return structural_error(
                "no wings found on aircraft geometry".to_owned(),
                torenbeek_mass,
            )
        }
    };

    let root_airfoil = match wing.xsecs.first() {
        Some(x) => &x.airfoil,
        None => return structural_error("wing has no cross sections".to_owned(), torenbeek_mass),
    };
    let tip_airfoil = match wing.xsecs.last() {
        Some(x) => &x.airfoil,
        None => return structural_error("wing has no cross sections".to_owned(), torenbeek_mass),
    };

    let wsg = match WingStructureGeometry::new(
        dv,
        &config.geometry.wing,
        root_airfoil,
        tip_airfoil,
        &spar_fracs,
        Some(&spar_full_span),
    ) {
        Ok(w) => w,
        Err(e) => {
            return structural_error(
                format!("failed to build wing structure geometry: {e:?}"),
                torenbeek_mass,
            )
        }
    };

    let sizing = size_wingbox(&wsg, scfg, req, skin_mat, web_mat, cap_mat, rib_mat);

    let analytical_report = analyze_structure(
        &wsg,
        &sizing,
        scfg,
        req,
        &config.geometry.engine,
        &config.mass_model,
        skin_mat,
        web_mat,
        cap_mat,
    );

    // Sizing output is a physical acceptance result, not merely evidence
    // that the numerical stage executed. Never continue to mesh/deck/export
    // a wingbox with an over-wide rib layout or a negative/non-finite
    // strength margin.
    if let Some(detail) = sizing_failure_detail(&sizing) {
        return StructuralAnalysisResult {
            status: "error".to_owned(),
            error: Some(detail),
            wsg: Some(wsg),
            sizing: Some(sizing),
            mesh_health: None,
            analysis: Some(analytical_report),
            nastran: None,
            nastran95: None,
            patran: None,
            torenbeek_wing_mass_kg: torenbeek_mass,
        };
    }

    let (mesh_deck, mesh_health, node_index) = match build_wing_mesh_bdf(
        &wsg,
        &sizing,
        scfg,
        &config.geometry.engine,
        &config.mass_model,
        req,
        skin_mat,
        web_mat,
        cap_mat,
        rib_mat,
    ) {
        Ok(res) => res,
        Err(e) => {
            return StructuralAnalysisResult {
                status: "ok".to_owned(),
                error: Some(format!("mesh generation warning: {e:?}")),
                wsg: Some(wsg),
                sizing: Some(sizing),
                mesh_health: None,
                analysis: Some(analytical_report),
                nastran: None,
                nastran95: None,
                patran: None,
                torenbeek_wing_mass_kg: torenbeek_mass,
            };
        }
    };

    let nastran_results = work_dir.map(|dir| {
        run_nastran_analysis(
            &mesh_deck,
            &node_index,
            scfg,
            req,
            dir,
            environment.nastran_exe.as_deref(),
        )
    });
    // When MSC is available, retain its result in `nastran` and run the
    // independent NASA dialect beside it. The MSC-absent path above already
    // uses NASTRAN-95 as the primary fallback, so this branch only creates a
    // second result when the GUI can genuinely compare both solvers.
    let nastran95_results = if scfg.run_nastran && environment.nastran_exe.is_some() {
        work_dir.and_then(|dir| {
            run_nastran95_from_config_or_env(
                &mesh_deck,
                &node_index,
                scfg,
                req,
                &dir.join("nastran95"),
            )
        })
    } else {
        None
    };

    let patran = Some(if !scfg.run_patran_export {
        PatranExportResult {
            status: "not_run".to_owned(),
            error: Some("Patran export is disabled in Structural Analysis settings".to_owned()),
            png_paths: Vec::new(),
        }
    } else {
        match (
            work_dir,
            environment.patran_exe.as_deref(),
            nastran_results.as_ref(),
        ) {
            (Some(dir), Some(executable), Some(nastran))
                if nastran.static_solve.status == ResultStatus::Ok =>
            {
                let cases = alas_struct::loads::load_cases(req, scfg.additional_safety_factor);
                let names = cases.iter().map(|case| case.name).collect::<Vec<_>>();
                run_patran_export(dir, executable, &names, scfg.timeout_s)
            }
            (_, None, _) => missing_patran_result(&scfg.patran_exe_path),
            _ => PatranExportResult {
                status: "not_run".to_owned(),
                error: Some("Patran export requires a successful NASTRAN SOL 101 solve".to_owned()),
                png_paths: Vec::new(),
            },
        }
    });

    StructuralAnalysisResult {
        status: "ok".to_owned(),
        error: None,
        wsg: Some(wsg),
        sizing: Some(sizing),
        mesh_health: Some(mesh_health),
        analysis: Some(analytical_report),
        nastran: nastran_results,
        nastran95: nastran95_results,
        patran,
        torenbeek_wing_mass_kg: torenbeek_mass,
    }
}

fn sizing_failure_detail(sizing: &WingboxSizing) -> Option<String> {
    let mut failures = Vec::new();
    if !sizing.rib_spacing_pass() {
        failures.push(format!(
            "installed rib spacing {:.6} m exceeds allowable {:.6} m",
            sizing.installed_rib_spacing_m(),
            sizing.rib_spacing_m
        ));
    }
    if !sizing.strength_margins_pass() {
        let minimum = sizing.minimum_margin_of_safety();
        failures.push(if minimum.is_finite() {
            format!("wingbox strength sizing is infeasible (minimum margin {minimum:.6})")
        } else {
            "wingbox strength sizing produced a non-finite margin".to_owned()
        });
    }
    (!failures.is_empty()).then(|| failures.join("; "))
}

fn missing_patran_result(configured: &str) -> PatranExportResult {
    let detail = if configured.trim().is_empty() {
        "Patran executable absent: configure it under Setup > External Tools".to_owned()
    } else if Path::new(configured).is_dir() {
        format!("Patran installation incomplete at {configured}: executable launcher is missing")
    } else if Path::new(configured).exists()
        || Path::new(configured).parent().is_some_and(Path::is_dir)
    {
        format!("Patran executable path is invalid: {configured} is not a regular executable file")
    } else {
        format!("Patran executable absent at {configured}")
    };
    PatranExportResult {
        status: "error".to_owned(),
        error: Some(detail),
        png_paths: Vec::new(),
    }
}

fn structural_error(msg: String, torenbeek: f64) -> StructuralAnalysisResult {
    StructuralAnalysisResult {
        status: "error".to_owned(),
        error: Some(msg),
        wsg: None,
        sizing: None,
        mesh_health: None,
        analysis: None,
        nastran: None,
        nastran95: None,
        patran: None,
        torenbeek_wing_mass_kg: torenbeek,
    }
}

#[cfg(test)]
mod tests {
    use super::{missing_patran_result, sizing_failure_detail};
    use alas_struct::sizing::{MassBreakdown, SparSizing, WingboxSizing};

    fn sample_sizing() -> WingboxSizing {
        WingboxSizing {
            y_stations: vec![0.0, 10.0],
            eta_stations: vec![0.0, 1.0],
            chord: vec![1.0, 1.0],
            spar_fracs: vec![0.25, 0.75],
            spars: vec![SparSizing {
                chord_fraction: 0.25,
                h: vec![1.0, 1.0],
                w_cap: vec![1.0, 1.0],
                t_cap: vec![1.0, 1.0],
                a_cap: vec![1.0, 1.0],
                t_web: 0.1,
                frac_moment: vec![1.0, 1.0],
                margin_of_safety: vec![-0.1, 0.5],
            }],
            t_skin: 0.01,
            num_ribs: 2,
            rib_spacing_m: 4.0,
            mass_breakdown_kg: MassBreakdown {
                spar_caps: 0.0,
                spar_webs: 0.0,
                skin: 0.0,
                ribs: 0.0,
            },
            total_mass_kg: 0.0,
            sizing_load_case: "test",
        }
    }

    #[test]
    fn sizing_gate_rejects_overwide_ribs_and_negative_margins() {
        let detail = match sizing_failure_detail(&sample_sizing()) {
            Some(detail) => detail,
            None => panic!("sizing must fail"),
        };
        assert!(detail.contains("installed rib spacing"));
        assert!(detail.contains("minimum margin -0.100000"));
    }

    #[test]
    fn missing_patran_configuration_is_reported_as_absent() {
        let result = missing_patran_result("");
        assert_eq!(result.status, "error");
        assert!(result
            .error
            .as_deref()
            .is_some_and(|detail| detail.contains("absent")));
    }

    #[test]
    fn a_patran_directory_without_a_launcher_is_reported_as_incomplete() {
        let root = std::env::temp_dir().join(format!("alas-patran-dir-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        let result = missing_patran_result(&root.display().to_string());
        assert!(result
            .error
            .as_deref()
            .is_some_and(|detail| detail.contains("incomplete")));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_patran_path_in_an_existing_directory_is_reported_as_invalid() {
        let root =
            std::env::temp_dir().join(format!("alas-patran-invalid-path-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::create_dir_all(&root);
        let configured = root.join("not-patran.exe");
        let result = missing_patran_result(&configured.display().to_string());
        assert!(result
            .error
            .as_deref()
            .is_some_and(|detail| detail.contains("path is invalid")));
        let _ = std::fs::remove_dir_all(root);
    }
}
