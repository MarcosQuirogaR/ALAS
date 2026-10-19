// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The take-off thrust band on the optimizer rows (owner decision
//! 2026-10-14): the AVE meets the CS 25.121(b) second segment with the
//! Jenkinson take-off charts; a larger failed-engine drag (a configured
//! windmilling coefficient) puts the requirement inside the band (a flagged
//! Diagnostic row, no hard violation) and then beyond it (a hard violation).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::AlasConfig;
use alas_opt::assess_product_candidate;

fn second_segment(windmilling_cd: Option<f64>) -> (Vec<(String, f64, f64, f64)>, bool) {
    let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "AVE"})).unwrap();
    config.performance.oei_windmilling_cd = windmilling_cd;
    let design = alas_config::presets::get("AVE").unwrap().design_vector;
    let assessment = assess_product_candidate(&config, &design).unwrap();
    let rows = assessment
        .residuals
        .iter()
        .filter(|row| row.id.starts_with("oei_second_segment"))
        .map(|row| {
            (
                row.id.to_owned(),
                row.limit,
                row.actual,
                row.normalized_violation,
            )
        })
        .collect();
    (
        rows,
        assessment
            .violated_hard_ids()
            .contains(&"oei_second_segment"),
    )
}

#[test]
fn the_ave_second_segment_is_met_inside_the_band_flagged_and_beyond_it_hard() {
    // Met: re-pinned by lane field lengths (2026-10-17, by probe). With the
    // capped BPR 8 take-off chart and the CS 25.107 schedule the AVE no longer
    // meets at the schedule speed (0.2528 required, 0.2655 installed on
    // 2026-10-14); the improved-climb speed that meets it is the lowest one,
    // so the requirement there equals the installed 0.2655.
    let (rows, hard) = second_segment(None);
    assert!(!hard);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!((rows[0].1 - 0.2655).abs() < 2e-3, "{rows:?}");
    assert!(rows[0].2 >= rows[0].1 - 1e-9, "{rows:?}");
    // Inside the 8.2 % band (0.2791 required, 4.9 % short): no violation, a
    // flagged Diagnostic row carrying the numbers.
    let (rows, hard) = second_segment(Some(0.010));
    assert!(!hard);
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[0].3, 0.0);
    assert_eq!(rows[1].0, "oei_second_segment_within_takeoff_band");
    let shortfall = 1.0 - rows[1].2 / rows[1].1;
    assert!((0.0..0.082).contains(&shortfall), "{shortfall}");
    // Beyond it (0.3064 required, 13.3 % short): hard.
    let (rows, hard) = second_segment(Some(0.024));
    assert!(hard);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].3 > 0.04, "{rows:?}");
}
