// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Detection of the slope breaks and curvature steps of the legacy laws.
//!
//! The legacy laws are piecewise (windshield start, crown end, belly start,
//! plan taper start, closing section). A fitted C2 spline rings around such a
//! feature unless a knot of multiplicity 3 sits on it, so the features are
//! found on a uniform grid from the jump of the second difference between
//! neighbouring cells of the crown, keel and plan lines.

/// Jump of the curvature (1/m) between neighbouring cells that makes a feature.
const STEP_THRESHOLD: f64 = 0.3;
/// Features closer than this to each other, an exclusion or an end are dropped, m.
const SEPARATION_M: f64 = 0.5;
/// Features closer than this to the nose tip or the tail end are not reported, m
/// (curvature varies too fast there for a step to mean anything).
const END_MARGIN_M: f64 = 0.3;
/// Most features reported.
const MAX_FEATURES: usize = 8;

/// Feature stations (m) of the sampled `lines` on the uniform grid
/// `x_i = i * pitch`, ordered, away from `exclude` stations and the ends.
pub(super) fn detect(lines: &[Vec<f64>], pitch: f64, exclude: &[f64]) -> Vec<f64> {
    let n = lines.first().map_or(0, Vec::len);
    if n < 32 {
        return Vec::new();
    }
    let length = pitch * (n - 1) as f64;
    let mut score = vec![0.0_f64; n];
    for line in lines {
        let d2: Vec<f64> = (0..n)
            .map(|i| {
                if i == 0 || i + 1 == n {
                    0.0
                } else {
                    (line[i + 1] - 2.0 * line[i] + line[i - 1]) / (pitch * pitch)
                }
            })
            .collect();
        for i in 2..(n - 2) {
            let jump = (d2[i + 1] - d2[i]).abs().max((d2[i] - d2[i - 1]).abs());
            score[i] = score[i].max(jump / STEP_THRESHOLD);
        }
    }
    // Cells that cannot hold a feature must not suppress their neighbours.
    for (i, s) in score.iter_mut().enumerate() {
        let x = i as f64 * pitch;
        if x < END_MARGIN_M
            || x > length - END_MARGIN_M
            || exclude.iter().any(|e| (e - x).abs() < SEPARATION_M)
        {
            *s = 0.0;
        }
    }
    let reach = (SEPARATION_M / pitch) as usize;
    let mut peaks: Vec<(f64, f64)> = Vec::new();
    for i in 1..n - 1 {
        let x = i as f64 * pitch;
        if score[i] < 1.0
            || x < END_MARGIN_M
            || x > length - END_MARGIN_M
            || exclude.iter().any(|e| (e - x).abs() < SEPARATION_M)
        {
            continue;
        }
        let lo = i.saturating_sub(reach);
        let hi = (i + reach).min(n - 1);
        if (lo..=hi).all(|j| score[j] <= score[i]) {
            peaks.push((score[i], x));
        }
    }
    peaks.sort_by(|p, q| q.0.total_cmp(&p.0));
    let mut out: Vec<f64> = Vec::new();
    for (_, x) in peaks {
        if out.len() < MAX_FEATURES && out.iter().all(|o| (o - x).abs() >= SEPARATION_M) {
            out.push(x);
        }
    }
    out.sort_by(f64::total_cmp);
    out
}
