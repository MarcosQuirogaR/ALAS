// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Export field-method diagnostics over every CADO row and every registered preset.
//! CADO field lengths and approach speeds are outputs for comparison only.

use std::path::PathBuf;

use serde_json::json;

mod cado;
mod predictions;
mod preset_references;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args().nth(1).map_or_else(
        || std::env::temp_dir().join("field_fleet.json"),
        PathBuf::from,
    );
    let aircraft =
        cado::read(PathBuf::from("docs/data/cado/CADO_airplane_database_v1.0.csv").as_path())?;
    let fleet: Vec<_> = aircraft.iter().map(predictions::evaluate).collect();
    let mut result = json!({
        "dataset": "Monrolin et al., CADO airplane database v1.3, DOI 10.57745/LLRJO0; ODbL 1.0",
        "dataset_sha256": "6b986349811e66f50039347f7c3450636f8700b1ed0ad5ee7e523d89b8777180",
        "row_count": aircraft.len(),
        "fleet": fleet,
        "presets": [],
        "preset_status": "pending",
        "scope": "Secondary field comparison; no CADO target used to set a model coefficient or CLmax",
    });
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&output, serde_json::to_string_pretty(&result)?)?;
    result["presets"] = json!(preset_references::evaluate()?);
    result["preset_status"] = json!("complete");
    std::fs::write(output, serde_json::to_string_pretty(&result)?)?;
    Ok(())
}
