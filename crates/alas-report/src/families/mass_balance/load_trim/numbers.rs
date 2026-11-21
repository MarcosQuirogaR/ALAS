// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Number text of the load-and-trim sheet.

/// Group thousands with a space: 78000 -> "78 000".
pub(super) fn kg(value: f64) -> String {
    let digits = format!("{:.0}", value.abs());
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(' ');
        }
        out.push(c);
    }
    if value < 0.0 {
        format!("-{out}")
    } else {
        out
    }
}

/// A signed value to one decimal, with no sign on a value that rounds to
/// zero ("-0.0" would read as outside a limit the state sits on).
pub(super) fn signed_tenths(value: f64) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if rounded == 0.0 {
        "0.0".to_owned()
    } else {
        format!("{rounded:+.1}")
    }
}
