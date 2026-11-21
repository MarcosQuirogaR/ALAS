// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one number rendering of the forms and the tables beside them.

/// The shortest decimal rendering of `value` that still round-trips within
/// `max_decimals` places.
///
/// `egui` derives a `DragValue`'s minimum decimals from its drag speed and the
/// pointer aim radius, which is scaled by the display's points-per-pixel: on a
/// 2x display that rendered whole-number counts as `16.0` and `0.0`, and the
/// same quantity at two magnitudes with different decimals (`-4.00` beside
/// `10.0`). Formatting explicitly makes the rendering a property of the value
/// and the field, not of the monitor.
pub(crate) fn format_number(value: f64, max_decimals: usize) -> String {
    if !value.is_finite() {
        return format!("{value}");
    }
    // A negative zero (or a value that rounds to one) reads "0", not "-0".
    let value = if value == 0.0 { 0.0 } else { value };
    for decimals in 0..max_decimals {
        let text = format!("{value:.decimals$}");
        let parsed: f64 = text.parse().unwrap_or(f64::NAN);
        if (parsed - value).abs() <= value.abs() * 1e-9 + 1e-12 {
            return text;
        }
    }
    format!("{value:.max_decimals$}")
}
