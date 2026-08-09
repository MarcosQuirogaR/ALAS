// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Shared plumbing for the `alas-perf::performance` parity tests, split across
//! two binaries (`parity_performance_constraints` and
//! `parity_performance_speeds`) so neither file exceeds the source-size limit.

// Each parity binary uses a subset of these helpers, so items unused in one
// binary are not dead across the pair; the allow is scoped to this shared file.
#![allow(dead_code)]
// A test asserts on values it loaded from a fixture it controls, so a failed
// unwrap here is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::airports::Airport;
use alas_config::{DesignRequirements, PerformanceConfig};
use alas_perf::performance::VSpeeds;
use alas_testkit::Comparison;
use serde::Deserialize;
use serde_json::{Map, Value};

/// One named section of `golden/perf/performance.json`, deserialized into the
/// caller's case type.
pub fn section<T: for<'de> Deserialize<'de>>(key: &str) -> Vec<T> {
    let root = alas_testkit::load_json("perf", "performance");
    serde_json::from_value(root[key].clone()).unwrap()
}

/// Rebuild a `PerformanceConfig` from a default plus the recorded overrides,
/// the same `setattr`-after-default the generator does. An unhandled key is a
/// fixture the test cannot reproduce, so it panics.
pub fn perf_config_for(overrides: &Map<String, Value>) -> PerformanceConfig {
    let mut pc = PerformanceConfig::default();
    for (key, value) in overrides {
        let f = || value.as_f64().unwrap();
        match key.as_str() {
            "cl_max_to" => pc.cl_max_to = f(),
            "cl_max_land" => pc.cl_max_land = f(),
            "cl_max_clean" => pc.cl_max_clean = f(),
            "cl_min_clean" => pc.cl_min_clean = f(),
            "thrust_lapse" => pc.thrust_lapse = f(),
            "vmc_vstall_factor" => pc.vmc_vstall_factor = f(),
            "vr_vmc_factor" => pc.vr_vmc_factor = f(),
            "vr_vstall_factor" => pc.vr_vstall_factor = f(),
            "v2_vstall_factor" => pc.v2_vstall_factor = f(),
            "v1_vr_factor" => pc.v1_vr_factor = f(),
            "vapp_vstall_land_factor" => pc.vapp_vstall_land_factor = f(),
            "vtd_vstall_land_factor" => pc.vtd_vstall_land_factor = f(),
            other => panic!("fixture set an unhandled PerformanceConfig field: {other}"),
        }
    }
    pc
}

/// Rebuild a `DesignRequirements` from a default plus the recorded overrides.
pub fn requirements_for(overrides: &Map<String, Value>) -> DesignRequirements {
    let mut req = DesignRequirements::default();
    for (key, value) in overrides {
        let f = || value.as_f64().unwrap();
        match key.as_str() {
            "cruise_mach" => req.cruise_mach = f(),
            "mtow_kg" => req.mtow_kg = f(),
            "ultimate_load_factor" => req.ultimate_load_factor = f(),
            "dive_speed_m_s" => req.dive_speed_m_s = f(),
            "limit_load_factor_neg" => req.limit_load_factor_neg = f(),
            other => panic!("fixture set an unhandled DesignRequirements field: {other}"),
        }
    }
    req
}

/// The scalar inputs that identify an aerodrome to the performance functions.
/// `name` is present only where a matching-chart case keys its curves by it.
#[derive(Debug, Deserialize)]
pub struct AirportInput {
    pub elevation_m: f64,
    pub toda_m: f64,
    pub lda_m: f64,
    pub isa_deviation_c: f64,
    #[serde(default)]
    pub name: Option<String>,
}

impl AirportInput {
    pub fn build(&self) -> Airport {
        Airport::custom(
            self.name.clone().unwrap_or_else(|| "Fixture".to_owned()),
            self.elevation_m,
            self.toda_m,
            self.lda_m,
            self.isa_deviation_c,
            0.0,
            0.0,
        )
    }
}

/// The V-speed schedule as the generator serializes it (`VSpeeds.as_ms`).
#[derive(Debug, Deserialize)]
pub struct VSpeedsExpected {
    #[serde(rename = "Vstall_TO")]
    pub vstall_to: f64,
    #[serde(rename = "Vstall_land")]
    pub vstall_land: f64,
    #[serde(rename = "Vmc")]
    pub vmc: f64,
    #[serde(rename = "V1")]
    pub v1: f64,
    #[serde(rename = "VR")]
    pub vr: f64,
    #[serde(rename = "V2")]
    pub v2: f64,
    #[serde(rename = "VAPP")]
    pub vapp: f64,
    #[serde(rename = "VTD")]
    pub vtd: f64,
}

pub fn compare_v_speeds(c: &mut Comparison, label: &str, got: &VSpeeds, want: &VSpeedsExpected) {
    c.scalar(
        &format!("{label}: Vstall_TO"),
        got.v_stall_to_ms,
        want.vstall_to,
    );
    c.scalar(
        &format!("{label}: Vstall_land"),
        got.v_stall_land_ms,
        want.vstall_land,
    );
    c.scalar(&format!("{label}: Vmc"), got.v_mc_ms, want.vmc);
    c.scalar(&format!("{label}: V1"), got.v1_ms, want.v1);
    c.scalar(&format!("{label}: VR"), got.v_r_ms, want.vr);
    c.scalar(&format!("{label}: V2"), got.v2_ms, want.v2);
    c.scalar(&format!("{label}: VAPP"), got.v_app_ms, want.vapp);
    c.scalar(&format!("{label}: VTD"), got.v_td_ms, want.vtd);
}

pub fn compare_arrays(c: &mut Comparison, label: &str, got: &[f64], want: &[f64]) {
    assert_eq!(got.len(), want.len(), "{label}: length");
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        c.scalar(&format!("{label}[{i}]"), *g, *w);
    }
}
