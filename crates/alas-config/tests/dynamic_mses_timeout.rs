// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Public configuration-boundary probes for MSES process deadlines.

use alas_config::{validate, AlasConfig, Severity};

#[test]
fn public_validation_rejects_every_nonpositive_or_nonfinite_mses_timeout() {
    for field in ["mset", "mses"] {
        for seconds in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let mut config = AlasConfig::default();
            if field == "mset" {
                config.mses.timeout_mset_s = seconds;
            } else {
                config.mses.timeout_mses_s = seconds;
            }

            let issues = validate(&config);
            let expected_path = format!("mses.timeout_{field}_s");
            let issue = issues
                .iter()
                .find(|issue| issue.field_path == expected_path)
                .unwrap_or_else(|| {
                    panic!("value {seconds:?} for {expected_path} must be rejected; got {issues:?}")
                });
            assert_eq!(issue.severity, Severity::Error, "field={field}");
            assert!(
                issue.message.contains("finite and greater than zero"),
                "field={field} seconds={seconds:?}: {}",
                issue.message
            );
        }
    }
}
