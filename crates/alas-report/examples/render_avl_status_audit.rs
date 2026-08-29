// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Render the normal product-path AVL unavailable state for visual audit.

use std::error::Error;
use std::fs;
use std::io::{Error as IoError, ErrorKind};
use std::path::PathBuf;

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_pipeline::avl::run_avl_analysis;
use alas_pipeline::full_analysis::FullAnalysis;
use alas_report::families::aerodynamics::figure_model_comparison;
use alas_report::svg::render_svg;

fn main() -> Result<(), Box<dyn Error>> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = workspace.join("outputs/avl_runtime_audit");
    let config = AlasConfig::default();
    let report = FullAnalysis::new(config.clone())
        .run(&DesignVector::default(), true)
        .map_err(|error| IoError::new(ErrorKind::InvalidData, error))?;
    let avl = run_avl_analysis(&report, &config, &output, None, 300.0);
    let scene = figure_model_comparison(&report, None, None, None, Some(&avl), Some("dark"));
    fs::create_dir_all(&output)?;
    fs::write(output.join("model_comparison.svg"), render_svg(&scene))?;
    Ok(())
}
