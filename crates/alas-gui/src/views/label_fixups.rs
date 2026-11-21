// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Display-only touch-ups of a schema field label.
//!
//! A label that the schema derives from its identifier (`target_cg_pct_mac`)
//! reads "Target cg pct mac". The form shows it with its acronyms restored and
//! the odd ones spelled out; the key the translation catalogue looks up and
//! the identifier itself are never touched.

/// Words written upper case wherever they appear in a derived label.
const ACRONYMS: &[&str] = &[
    "cg", "uld", "mac", "flops", "cas", "mses", "mset", "oei", "apu", "mtow", "tsfc", "isa", "avl",
];

/// Labels that a plain acronym pass cannot fix.
const SPELLED: &[(&str, &str)] = &[
    ("Target cg pct mac", "Target CG (% MAC)"),
    ("Fwd door x", "Forward door position"),
    ("Main door x", "Main door position"),
    ("Aft door x", "Aft door position"),
];

/// A schema label as the English form shows it: acronyms upper case and a
/// lone axis letter in lower case after the first word ("Nose-gear x position").
pub(crate) fn english_label(label: &str) -> String {
    if let Some((_, shown)) = SPELLED.iter().find(|(raw, _)| *raw == label) {
        return (*shown).to_owned();
    }
    let mut out = String::with_capacity(label.len());
    for (index, word) in label.split(' ').enumerate() {
        if index > 0 {
            out.push(' ');
        }
        let lower = word.to_ascii_lowercase();
        if ACRONYMS.contains(&lower.as_str()) {
            out.push_str(&lower.to_ascii_uppercase());
        } else if lower == "mach" {
            out.push_str("Mach");
        } else if index > 0 && matches!(word, "X" | "Y" | "Z") {
            out.push_str(&lower);
        } else {
            out.push_str(word);
        }
    }
    out
}

/// A label without the parenthesised symbol written with an underscore
/// ("Ultimate load factor (n_ult)" reads "Ultimate load factor"): the name
/// already says it and a bare underscore is not typography.
pub(crate) fn without_symbol_subscripts(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut rest = label;
    while let Some(open) = rest.find(" (") {
        let after = &rest[open + 2..];
        match after.find(')') {
            Some(close) if after[..close].contains('_') => {
                out.push_str(&rest[..open]);
                rest = &after[close + 1..];
            }
            _ => {
                out.push_str(&rest[..open + 2]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::{english_label, without_symbol_subscripts};

    #[test]
    fn derived_labels_get_their_acronyms_back() {
        assert_eq!(english_label("Main deck uld"), "Main deck ULD");
        assert_eq!(english_label("Cg trim step"), "CG trim step");
        assert_eq!(english_label("Target cg pct mac"), "Target CG (% MAC)");
        assert_eq!(english_label("Fwd door x"), "Forward door position");
        assert_eq!(english_label("Flops transport"), "FLOPS transport");
        assert_eq!(
            english_label("Nose-gear X position"),
            "Nose-gear x position"
        );
        assert_eq!(english_label("Wing area"), "Wing area");
        assert_eq!(english_label("Climb mach"), "Climb Mach");
    }

    #[test]
    fn underscore_symbols_are_dropped_from_a_label() {
        assert_eq!(
            without_symbol_subscripts("Ultimate load factor (n_ult)"),
            "Ultimate load factor"
        );
        assert_eq!(
            without_symbol_subscripts("Limit load factor, negative (n_lim,neg)"),
            "Limit load factor, negative"
        );
        assert_eq!(
            without_symbol_subscripts("Max lift coefficient, clean (CLmax_clean)"),
            "Max lift coefficient, clean"
        );
        assert_eq!(
            without_symbol_subscripts("Wing span (m) here"),
            "Wing span (m) here"
        );
    }
}
