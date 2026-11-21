// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Narrowest lattice strip of recorded design vectors at one mesh level.
//!
//! A strip a fraction of a millimetre wide (a trailing leg of one surface
//! falling beside a loft kink of another) leaves the lattice a circulation
//! it cannot determine and the Trefftz drag jumps. The defect is one of the
//! mesh, so every mesh level of a fidelity ladder has to be tested for it
//! directly rather than inferred from the finest one. For every design the
//! candidate's aircraft is built with its sized tail, meshed and solved
//! once, and the width of every spanwise strip (the bound-vortex span of its
//! trailing-edge panel, m) is compared with the median strip width of its
//! surface.
//!
//! ```text
//! strip_check CONFIG.json DESIGNS.json [--chord N] [--wing N] [--emp N]
//!     [--limit N] [--ratio R]
//! ```
//!
//! Prints the narrowest strip over all designs (width, ratio to its
//! surface's median, surface, design index) and the number of designs with a
//! strip below `R` (default 0.02: the tip strip of a 24-strip cosine spacing is 0.045 of the median) of its surface's median; exits with status
//! 2 when there is one, so a batch can assert the level is free of slivers.

// An example is a console tool: it owns stdout.
#![allow(clippy::print_stdout)]

use alas_aero::operating_point::OperatingPoint;
use alas_aero::vlm::VlmSystem;
use alas_atmo::Atmosphere;
use alas_config::DesignVector;
use alas_opt::ScreeningFidelity;
use serde_json::Value;
use std::{error::Error, fs, process::ExitCode};

fn main() -> Result<ExitCode, Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        return Err("usage: strip_check CONFIG.json DESIGNS.json [flags]".into());
    }
    let document: Value = serde_json::from_str(&fs::read_to_string(&args[0])?)?;
    let mut designs: Vec<Vec<f64>> = serde_json::from_str(&fs::read_to_string(&args[1])?)?;
    let mut fidelity = ScreeningFidelity::full();
    let mut ratio_limit = 0.02;
    let mut index = 2;
    while index < args.len() {
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("{} requires a value", args[index]))?;
        match args[index].as_str() {
            "--chord" => fidelity.chordwise_resolution = Some(value.parse()?),
            "--wing" => fidelity.wing_subdivisions = Some(value.parse()?),
            "--emp" => fidelity.empennage_subdivisions = Some(value.parse()?),
            "--limit" => designs.truncate(value.parse()?),
            "--ratio" => ratio_limit = value.parse()?,
            other => return Err(format!("unknown flag {other}").into()),
        }
        index += 2;
    }
    let (config, _) = alas_mission::preset_load::load_document(&document)
        .map_err(|error| format!("invalid configuration: {error:?}"))?;
    let config = fidelity.configure(&config);
    let req = &config.requirements;
    let atmosphere = Atmosphere::new(req.cruise_altitude_m);
    let velocity = req.cruise_mach * atmosphere.speed_of_sound();
    let op = OperatingPoint::new(atmosphere, velocity, 2.0, 0.0, 0.0, 0.0, 0.0);
    let (spanwise, chordwise) = (
        config.analysis.spanwise_resolution as usize,
        config.analysis.chordwise_resolution as usize,
    );
    // (width m, ratio, surface, design index, inboard span station m)
    let mut narrowest = (f64::INFINITY, f64::INFINITY, String::new(), 0usize, 0.0_f64);
    let (mut slivers, mut built, mut panels) = (0usize, 0usize, 0usize);
    let mut flagged: Vec<String> = Vec::new();
    for (number, values) in designs.iter().enumerate() {
        let Ok(design) = DesignVector::from_array(values) else {
            continue;
        };
        // The aircraft the evaluator builds: its sized tail included.
        let Ok((mut resolved, sizing)) = alas_opt::resolve_tail_sizing(&config, &design, false)
        else {
            continue;
        };
        let mut candidate = config.clone();
        let Ok(plane) = alas_opt::mdo::rebuild_airplane(&mut candidate, &mut resolved, &sizing)
        else {
            continue;
        };
        let Ok(result) =
            VlmSystem::assemble(&plane, spanwise, chordwise).and_then(|system| system.solve(&op))
        else {
            continue;
        };
        built += 1;
        panels = result.panels.len();
        let mut sliver = false;
        for (wing_index, wing) in plane.wings.iter().enumerate() {
            // (width, inboard |y| or |z| of the strip)
            let mut widths: Vec<(f64, f64)> = result
                .panels
                .iter()
                .filter(|panel| panel.wing_index == wing_index && panel.is_trailing_edge)
                .map(|panel| {
                    let (a, b) = (panel.left_vortex_vertex, panel.right_vortex_vertex);
                    let width = ((b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt();
                    let station = a[1].hypot(a[2]).min(b[1].hypot(b[2]));
                    (width, station)
                })
                .collect();
            if widths.len() < 2 {
                continue;
            }
            widths.sort_by(|left, right| left.0.total_cmp(&right.0));
            let median = widths[widths.len() / 2].0;
            let ratio = widths[0].0 / median;
            sliver |= ratio < ratio_limit;
            if ratio < narrowest.1 {
                narrowest = (widths[0].0, ratio, wing.name.clone(), number, widths[0].1);
            }
        }
        slivers += usize::from(sliver);
        if sliver {
            flagged.push(number.to_string());
        }
    }
    println!(
        "{built} of {} designs meshed ({panels} panels): narrowest strip {:.5} m = {:.4} of its surface's median ({}, design {}, at span station {:.4} m); designs with a strip below {ratio_limit} of the median: {slivers}",
        designs.len(), narrowest.0, narrowest.1, narrowest.2, narrowest.3, narrowest.4
    );
    if !flagged.is_empty() {
        println!("designs with a sliver strip: {}", flagged.join(" "));
    }
    Ok(if slivers > 0 {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    })
}
