// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Run one complete native OpenFOAM airfoil study from a fresh case folder.
//!
//! Configure the executable locations through environment variables so the
//! example is usable with both the official OpenCFD native distribution and a
//! WSL2 installation:
//!
//! ```text
//! $env:ALAS_OPENFOAM_BIN = 'C:/.../platforms/win64MingwDPInt32Opt/bin'
//! $env:ALAS_OPENFOAM_PROJECT = 'C:/.../OpenFOAM-v2606'
//! $env:ALAS_GMSH = 'C:/.../gmsh.exe'
//! cargo run -p alas-cfd --example run_airfoil_case -- .agent/airfoil-run naca0012
//! ```

use std::env;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use alas_cfd::{run_study, CfdStage, CfdStudyConfig, MeshPreset};
use alas_exec::openfoam::{OpenFoamAdapter, OpenFoamBackend, OpenFoamPreferences};

fn main() {
    let mut args = env::args_os().skip(1);
    let case_dir = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("outputs/airfoil-cfd/native-example"));
    let airfoil = args
        .next()
        .and_then(|value| value.into_string().ok())
        .unwrap_or_else(|| "SC2-0714".to_owned());

    let mut config = CfdStudyConfig::default();
    config.airfoil_name = airfoil;
    config.mesh.preset = MeshPreset::Coarse;
    let max_iterations_was_set = if let Some(value) = env::var("ALAS_CFD_MAX_ITERATIONS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
    {
        config.solver.max_iterations = value.max(10);
        true
    } else {
        false
    };
    let startup_iterations_was_set = if let Some(value) = env::var("ALAS_CFD_STARTUP_ITERATIONS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
    {
        config.solver.startup_iterations = value.min(config.solver.max_iterations);
        true
    } else {
        false
    };
    if env::var("ALAS_CFD_SCHEME").as_deref() == Ok("upwind") {
        config.solver.convection_scheme = alas_cfd::ConvectionScheme::BoundedUpwind;
        config.solver.startup_iterations = 0;
    }
    if env::var("ALAS_CFD_FAR_FIELD").as_deref() == Ok("freestream") {
        config.boundaries.far_field = alas_cfd::FarFieldCondition::Freestream;
    }
    // Keep a short max-iteration smoke run usable when the caller changes
    // only ALAS_CFD_MAX_ITERATIONS.  An explicitly supplied startup value is
    // left for config validation so an accidental equal-length staged run is
    // reported clearly.
    if max_iterations_was_set
        && !startup_iterations_was_set
        && config.solver.convection_scheme != alas_cfd::ConvectionScheme::BoundedUpwind
        && config.solver.startup_iterations >= config.solver.max_iterations
    {
        config.solver.startup_iterations = config.solver.max_iterations.saturating_sub(1);
    }

    let mut preferences = OpenFoamPreferences {
        backend: OpenFoamBackend::Native,
        ..OpenFoamPreferences::default()
    };
    preferences.native_bin_dir = env::var("ALAS_OPENFOAM_BIN").ok();
    preferences.native_project_dir = env::var("ALAS_OPENFOAM_PROJECT").ok();
    preferences.gmsh_executable = env::var("ALAS_GMSH").ok();
    let adapter = OpenFoamAdapter::resolve(preferences);
    let capabilities = adapter.probe();
    println!(
        "OpenFOAM: {} - {}",
        capabilities.summary(),
        capabilities.detail
    );
    println!("Gmsh available: {}", adapter.probe_gmsh());
    if !capabilities.available || !adapter.probe_gmsh() {
        eprintln!("The configured OpenFOAM/Gmsh tools are not available.");
        std::process::exit(2);
    }

    let cancel = Arc::new(AtomicBool::new(false));
    match run_study(&config, &adapter, &case_dir, &cancel, |event| {
        println!(
            "[{:>7.2}s] {}: {}",
            event.elapsed_seconds,
            stage_name(event.stage),
            event.message
        )
    }) {
        Ok(results) => {
            if let Some(final_force) = results.forces.last() {
                println!(
                    "Outcome: {} ({}) - final Cd={:.8e}, Cl={:.8e}, Cm={:.8e} ({} samples)",
                    results.outcome.as_str(),
                    results.status_detail,
                    final_force.cd,
                    final_force.cl,
                    final_force.cm,
                    results.forces.len()
                );
            } else {
                println!(
                    "Outcome: {} ({}) - no force samples",
                    results.outcome.as_str(),
                    results.status_detail
                );
            }
            println!(
                "Artifacts: {}",
                results.case_dir.join("results.json").display()
            );
            if matches!(
                results.outcome,
                alas_cfd::CfdOutcome::Failed | alas_cfd::CfdOutcome::Cancelled
            ) {
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("Study could not be started: {error}");
            std::process::exit(2);
        }
    }
}

fn stage_name(stage: CfdStage) -> &'static str {
    stage.as_str()
}
