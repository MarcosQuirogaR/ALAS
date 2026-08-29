// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Render the retained real-MSES pressure fixture through both report figures.

use std::error::Error;
use std::path::PathBuf;

use alas_aero::mses::{MsesPressureResult, MsesStatus};
use alas_report::families::aerodynamics::{
    figure_mses_mach_contours, figure_mses_pressure_distribution,
};
use serde_json::Value;

fn numbers(value: &Value, key: &str) -> Result<Vec<f64>, Box<dyn Error>> {
    let array = value[key]
        .as_array()
        .ok_or_else(|| format!("missing MSES fixture array: {key}"))?;
    array
        .iter()
        .map(|item| {
            item.as_f64()
                .ok_or_else(|| format!("non-numeric MSES fixture value in {key}").into())
        })
        .collect()
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let output = arguments.next().map_or_else(
        || PathBuf::from("outputs/mses_figure_fidelity_audit"),
        PathBuf::from,
    );
    let raw_directory = arguments.next().map(PathBuf::from);
    std::fs::create_dir_all(&output)?;
    let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("golden/aero/mses.json");
    let fixture: Value = serde_json::from_slice(&std::fs::read(fixture_path)?)?;
    let pressure = fixture["pressure"]
        .as_array()
        .and_then(|cases| cases.first())
        .and_then(|case| case.get("result"))
        .ok_or("golden MSES fixture has no pressure result")?;
    let fixture_result = MsesPressureResult {
        status: MsesStatus::Ok,
        error: None,
        alpha_deg: pressure["alpha_deg"]
            .as_f64()
            .ok_or("MSES fixture alpha is not numeric")?,
        x_upper: numbers(pressure, "x_upper")?,
        cp_upper: numbers(pressure, "cp_upper")?,
        mach_upper: numbers(pressure, "mach_upper")?,
        x_lower: numbers(pressure, "x_lower")?,
        cp_lower: numbers(pressure, "cp_lower")?,
        mach_lower: numbers(pressure, "mach_lower")?,
        field_x: numbers(pressure, "field_x")?,
        field_y: numbers(pressure, "field_y")?,
        field_mach: numbers(pressure, "field_mach")?,
        airfoil_x: numbers(pressure, "airfoil_x")?,
        airfoil_y: numbers(pressure, "airfoil_y")?,
        ..MsesPressureResult::default()
    };
    let result = if let Some(raw_directory) = raw_directory {
        let airfoil = fixture_result
            .airfoil_x
            .iter()
            .copied()
            .zip(fixture_result.airfoil_y.iter().copied())
            .collect::<Vec<_>>();
        MsesPressureResult::replay_raw_exports(
            fixture_result.alpha_deg,
            std::fs::read_to_string(raw_directory.join("bl_dump.txt"))?,
            std::fs::read_to_string(raw_directory.join("flowfield.txt"))?,
            &airfoil,
        )?
    } else {
        fixture_result
    };

    for (name, scene) in [
        (
            "pressure",
            figure_mses_pressure_distribution(&result, Some("dark-accessible")),
        ),
        (
            "mach_field",
            figure_mses_mach_contours(&result, None, Some("dark-accessible")),
        ),
    ] {
        std::fs::write(
            output.join(format!("{name}.svg")),
            alas_report::render_svg(&scene),
        )?;
    }
    Ok(())
}
