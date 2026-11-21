// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The take-off thrust band on the optimizer rows: the AVE meets the CS 25.121(b) second segment with the
//! Jenkinson take-off charts at its improved-climb speed; at the schedule
//! speed the requirement is inside the band (a flagged Diagnostic row, no hard
//! violation), and a large stated failed-engine drag (a configured windmilling
//! coefficient in place of the derived one) puts it beyond the band (a hard
//! violation).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::AlasConfig;
use alas_opt::assess_product_candidate;

fn second_segment(
    windmilling_cd: Option<f64>,
    improved_climb: bool,
) -> (Vec<(String, f64, f64, f64)>, bool) {
    let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "AVE"})).unwrap();
    config.performance.oei_windmilling_cd = windmilling_cd;
    config.performance.improved_climb_v2 = improved_climb;
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
    // Met. With the
    // capped BPR 8 take-off chart and the CS 25.107 schedule the AVE no longer
    // meets at the schedule speed (0.2528 required, 0.2655 installed);
    // the improved-climb speed that meets it is the lowest one,
    // so the requirement there equals the installed 0.2655.
    let (rows, hard) = second_segment(None, true);
    assert!(!hard);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!((rows[0].1 - 0.2655).abs() < 2e-3, "{rows:?}");
    assert!(rows[0].2 >= rows[0].1 - 1e-9, "{rows:?}");
    // At the schedule speed, without the improved climb, the requirement on
    // the V2 climb-out locus (0.2866) is inside the 8.2 % band (7.3 % short):
    // no violation, a flagged Diagnostic row carrying the numbers.
    let (rows, hard) = second_segment(None, false);
    assert!(!hard);
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[0].3, 0.0);
    assert_eq!(rows[1].0, "oei_second_segment_within_takeoff_band");
    let shortfall = 1.0 - rows[1].2 / rows[1].1;
    assert!((0.0..0.082).contains(&shortfall), "{shortfall}");
    // A stated failed-engine drag replaces the derived one (0.0033). Up to
    // 0.008 the improved-climb speed still meets the requirement; 0.024 is
    // beyond even the best-climb speed and the band (0.3234 required, 17.9 %
    // short): hard.
    let (rows, hard) = second_segment(Some(0.008), true);
    assert!(!hard);
    assert_eq!(rows.len(), 1, "{rows:?}");
    let (rows, hard) = second_segment(Some(0.024), true);
    assert!(hard);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].3 > 0.04, "{rows:?}");
}
