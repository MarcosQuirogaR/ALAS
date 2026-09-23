// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Domain check for the configurable Lock/Korn wave-drag rise.

use crate::AlasConfig;

use super::{Severity, ValidationIssue};

pub(super) fn rise_is_physical(config: &AlasConfig) -> Vec<ValidationIssue> {
    let coefficient = config.drag_model.wave_drag_coefficient;
    if coefficient.is_finite() && coefficient > 0.0 {
        Vec::new()
    } else {
        vec![ValidationIssue {
            field_path: "drag_model.wave_drag_coefficient".to_owned(),
            message: format!(
                "Wave-drag rise coefficient must be finite and positive for the Lock/Korn critical-Mach relation; got {coefficient}."
            ),
            severity: Severity::Error,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coefficient_must_define_a_finite_critical_mach() {
        for coefficient in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let mut config = AlasConfig::default();
            config.drag_model.wave_drag_coefficient = coefficient;
            let issues = rise_is_physical(&config);
            assert!(issues.iter().any(|issue| {
                issue.field_path == "drag_model.wave_drag_coefficient"
                    && issue.severity == Severity::Error
            }));
        }
    }
}
