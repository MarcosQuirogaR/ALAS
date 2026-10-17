// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! From a flown mission to the named inputs of the score engine.
//!
//! Segment conventions (by segment *name*, as written in the preset
//! templates): `takeoff`, `climb`, `distance` (a timed cruise: gives
//! `distance_<N>s` for its `Until::Time` N), `efficiency` (timed cruise:
//! `distance_eff`, `energy_eff`), `circuit` (laps: `time_1km`, `lap_count`),
//! `glide` (`glide_time`), `charter`/`m2`/`m3` laps. Everything a simulation
//! cannot know (payload count, bottles, passengers, declared values, cost,
//! pack ratings, static margins) is added by the caller.

use crate::score::{Dim, RoundInputs, Series};

use super::airframe::Airframe;
use super::integrate::Sample;
use super::mission::MissionResult;

/// The engine assigns the declared dimension of each signal name on input.
fn series(samples: &[Sample], f: impl Fn(&Sample) -> f64) -> Series {
    Series {
        t: samples.iter().map(|s| s.t).collect(),
        y: samples.iter().map(f).collect(),
        dim: Dim::ONE,
    }
}

/// Value of a sampled quantity `dt` seconds after `t0` (linear interpolation).
fn at_time(samples: &[Sample], time: f64, f: impl Fn(&Sample) -> f64) -> Option<f64> {
    let last = samples.last()?;
    if time > last.t + 1e-9 {
        return None;
    }
    let k = samples.partition_point(|s| s.t < time);
    if k == 0 {
        return samples.first().map(&f);
    }
    let (a, b) = (&samples[k - 1], &samples[k.min(samples.len() - 1)]);
    if b.t <= a.t {
        return Some(f(b));
    }
    Some(f(a) + (f(b) - f(a)) * (time - a.t) / (b.t - a.t))
}

/// Score-engine inputs derived from a mission and its airframe, SI.
pub fn score_inputs(result: &MissionResult, af: &Airframe) -> RoundInputs {
    let mut values = std::collections::BTreeMap::new();
    let mut put = |name: &str, v: f64| {
        values.insert(name.to_string(), v);
    };
    put("mtow", af.mass_kg);
    put("payload_mass", af.payload_kg);
    put("empty_mass", af.mass_kg - af.payload_kg);
    put("span", af.span_m);
    put("wing_area", af.wing_area_m2);
    put("loading_time", result.loading_time_s);
    let s = &result.samples;
    put("peak_current", result.peak_current_a());
    put(
        "peak_voltage",
        s.iter()
            .map(|x| x.voltage_v)
            .fold(f64::NEG_INFINITY, f64::max),
    );
    put(
        "peak_power",
        s.iter().map(|x| x.power_w).fold(0.0, f64::max),
    );
    let energy = s.last().map_or(0.0, |x| x.energy_j);
    put("energy", energy);
    let distance = s.last().map_or(0.0, |x| x.x);
    if distance > 0.0 {
        put("energy_per_distance", energy / distance);
    }
    let mut laps = 0.0;
    for seg in &result.segments {
        let span_s = seg.t_end - seg.t_start;
        let dist = seg.x_end - seg.x_start;
        match (seg.kind, seg.name.as_str()) {
            ("takeoff", _) => {
                if let Some(d) = seg.detail.get("takeoff_distance_m") {
                    put("takeoff_distance", *d);
                }
            }
            ("climb", _) => put("climb_time", span_s),
            ("glide", _) => put("glide_time", span_s),
            ("cruise", "distance") => {
                if let Some(n) = seg.until_seconds {
                    put(&format!("distance_{}s", n.round() as i64), dist);
                }
            }
            ("cruise", "efficiency") => {
                put("distance_eff", dist);
                put("energy_eff", seg.energy_j);
            }
            ("laps", name) => {
                laps += seg.detail.get("laps").copied().unwrap_or(0.0);
                if name == "circuit" {
                    put("time_1km", span_s);
                }
                if name == "charter" || name == "m2" {
                    put("laps_m2", seg.detail.get("laps").copied().unwrap_or(0.0));
                }
                if name == "banner" || name == "m3" {
                    put("laps_m3", seg.detail.get("laps").copied().unwrap_or(0.0));
                }
            }
            _ => {}
        }
    }
    put("lap_count", laps);
    if let Some(t_lo) = result.liftoff_time_s {
        if let Some(h) = at_time(s, t_lo + 60.0, |x| x.h) {
            put("altitude_at_60s", h);
        }
    }
    let mut out = RoundInputs {
        values,
        series: Default::default(),
    };
    out.series
        .insert("battery_current".into(), series(s, |x| x.current_a));
    out.series
        .insert("battery_voltage".into(), series(s, |x| x.voltage_v));
    out.series
        .insert("battery_power".into(), series(s, |x| x.power_w));
    out
}
