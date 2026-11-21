// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Display units that differ from the stored SI unit by a constant factor.
//!
//! A pressure or stress is stored in pascals, which reads as `20684272 Pa`;
//! the form shows it in megapascals. The conversion happens only when the
//! value is drawn or typed: the configuration keeps the SI number.

use alas_config::Field;

use super::number_text::format_number;

/// A field shown in a unit other than its stored one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct DisplayScale {
    /// Displayed value per stored value.
    pub(super) factor: f64,
    /// The displayed unit.
    pub(super) unit: &'static str,
    /// Decimals the displayed value is shown with.
    pub(super) decimals: usize,
}

impl DisplayScale {
    /// The stored value as displayed.
    pub(super) fn to_display(self, stored: f64) -> f64 {
        stored * self.factor
    }

    /// A displayed (typed) value as stored.
    pub(super) fn to_stored(self, displayed: f64) -> f64 {
        displayed / self.factor
    }

    /// The stored value as displayed text.
    pub(super) fn text(self, stored: f64) -> String {
        format_number(self.to_display(stored), self.decimals)
    }
}

/// The display scale of `field`, or `None` when it is shown as stored.
pub(super) fn display_scale(field: &Field) -> Option<DisplayScale> {
    let name = field.name;
    // A fraction of the MAC reads as a percentage, like every other MAC quantity.
    // The reference window is a share of the reference value or of the chord: shown as a percentage.
    if name.starts_with("reference_") {
        let unit = match field.unit {
            "fraction of reference value" => Some("% ref. value"),
            "fraction of chord" => Some("% chord"),
            _ => None,
        };
        if let Some(unit) = unit {
            return Some(DisplayScale {
                factor: 100.0,
                unit,
                decimals: 2,
            });
        }
    }
    if field.unit == "fraction of MAC" {
        return Some(DisplayScale {
            factor: 100.0,
            unit: "% MAC",
            decimals: 1,
        });
    }
    (field.unit == "Pa" && (name.contains("pressure") || name.contains("stress"))).then_some(
        DisplayScale {
            factor: 1.0e-6,
            unit: "MPa",
            decimals: 3,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::DisplayScale;

    #[test]
    fn a_pressure_is_shown_in_megapascals_and_stored_in_pascals() {
        let scale = DisplayScale {
            factor: 1.0e-6,
            unit: "MPa",
            decimals: 3,
        };
        assert_eq!(scale.text(20_684_272.0), "20.684");
        assert!((scale.to_stored(20.684272) - 20_684_272.0).abs() < 1e-6);
        assert_eq!(scale.text(0.0), "0");
    }
}
