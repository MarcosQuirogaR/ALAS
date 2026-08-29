// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Write a CPACS 3.5 aircraft-data artifact for the default design.

use std::env;
use std::io;
use std::path::PathBuf;

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_pipeline::{export_cpacs_with_analysis, FullAnalysis};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args_os().nth(1).map(PathBuf::from).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: cargo run -p alas-pipeline --example export_cpacs -- <output.cpacs.xml>",
        )
    })?;
    let config = AlasConfig::default();
    let report = FullAnalysis::new(config.clone())
        .run(&DesignVector::default(), true)
        .map_err(io::Error::other)?;
    export_cpacs_with_analysis(&report, &config, None, None, &path)?;
    Ok(())
}
