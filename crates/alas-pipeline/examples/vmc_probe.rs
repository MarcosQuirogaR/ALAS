// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The computed sea-level `VMCA` at MTOW of registered presets and the terms
//! of its yaw balance (fin, rudder, engine arm, asymmetric force).
//!
//! ```text
//! cargo run -p alas-pipeline --profile harness --example vmc_probe -- ATR72-600 A320-200
//! ```
#![allow(clippy::print_stdout, clippy::unwrap_used)]

use alas_config::{presets, AlasConfig};
use alas_opt::mdo::{EngineOutModel, YawAirframe};
use alas_pipeline::FullAnalysis;
use serde_json::json;

fn main() {
    for preset in std::env::args().skip(1) {
        let config = AlasConfig::from_value(&json!({ "preset": preset })).unwrap();
        let design = presets::get(&preset).unwrap().design_vector;
        let report = match FullAnalysis::new(config.clone()).run(&design, true) {
            Ok(report) => report,
            Err(error) => {
                println!("{}", json!({ "preset": preset, "error": error.to_string() }));
                continue;
            }
        };
        let mtow = config.requirements.mtow_kg;
        let s_ref = report.airplane.s_ref;
        let resolution =
            alas_pipeline::report_config::resolve_report(&config, &report, mtow, mtow, s_ref);
        let outcome = &resolution.vmc_at_mtow;
        let geometry = EngineOutModel::new(
            &config,
            Some(YawAirframe {
                plane: &report.airplane,
                cg_x_m: report.physical_cg[0],
            }),
            s_ref,
            Some(1.0),
        )
        .ok()
        .and_then(|model| model.yaw_control());
        let sea = outcome.at_sea_level.as_ref().ok();
        println!(
            "{}",
            json!({
                "preset": preset,
                "vmca_m_s": sea.map(|a| a.vmca_m_s),
                "vmca_kt": sea.map(|a| a.vmca_m_s / 0.514_444),
                "vsr_m_s": sea.map(|a| a.vsr_m_s),
                "vmc_over_vsr": sea.map(|a| a.vmc_over_vsr),
                "vmcg_kt": sea.and_then(|a| a.vmcg_m_s).map(|v| v / 0.514_444),
                "rudder_deg": sea.map(|a| a.rudder_deg),
                "sideslip_deg": sea.map(|a| a.sideslip_deg),
                "bank_deg": sea.map(|a| a.bank_deg),
                "limit": sea.map(|a| format!("{:?}", a.limit)),
                "asym_force_n": sea.map(|a| a.asymmetric_force_n),
                "weight_n": mtow * config.requirements.gravity_m_s2,
                "rudder_max_deg": config.control_surfaces.rudder_max_deflection_deg,
                "geometry": geometry.map(|g| format!("{g:?}")),
                "cn_delta_r": geometry.and_then(|g| g.cn_delta_r_per_rad(0.2)),
                "cn_beta": geometry.and_then(|g| g.cn_beta_total_per_rad(0.2)),
                "crosswind": outcome.crosswind.as_ref().map(|c| format!("{c:?}")),
                "failure": outcome.at_sea_level.as_ref().err().map(|e| format!("{e:?}")),
            })
        );
    }
}
