// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The operating-empty CG against its published reference.

use alas_config::oew_reference::{LightCgBound, OewCgReference};

use super::{Findings, RuleInfo, Severity};

pub(super) const W_OEW_CG_REFERENCE: RuleInfo = RuleInfo {
    id: "CG-OEW-REFERENCE",
    statement: "Where the manufacturer draws a light-weight state at a forward or aft CG, the model OEW CG is not ahead of the forward one nor behind the aft one (HIGH); every other aircraft is held to Torenbeek's OEW CG band for its engine arrangement (MEDIUM outside it, HIGH more than one band width, 5 % MAC, outside it)",
    source: "Airbus A320 / A340 / A380 aircraft characteristics 2-3-0 and 10-0-0 light-weight clearance states and Torenbeek 1982 Sec. 8.5.3 pp. 299-300, as recorded in alas-config oew_reference::cg",
};

/// Judge the model OEW CG (`oew_cg_pct_mac`, % MAC in the preset's published
/// reference-wing frame) against the aircraft's record.
pub fn check_oew_cg_reference(out: &mut Findings, oew_cg_pct_mac: f64, reference: &OewCgReference) {
    if !reference.published.is_empty() {
        for state in reference.published {
            let beyond = match state.bound {
                LightCgBound::Forward => state.cg_pct_mac - oew_cg_pct_mac,
                LightCgBound::Aft => oew_cg_pct_mac - state.cg_pct_mac,
            };
            if beyond > 0.0 {
                out.add_measured(
                    W_OEW_CG_REFERENCE.id,
                    Severity::High,
                    "",
                    Some(beyond),
                    format!(
                        "OEW CG {oew_cg_pct_mac:.1} % MAC is {beyond:.1} % MAC {} the published {} at {:.0} kg, {:.1} % MAC",
                        if state.bound == LightCgBound::Aft { "aft of" } else { "ahead of" },
                        state.state,
                        state.mass_kg,
                        state.cg_pct_mac
                    ),
                );
            }
        }
        return;
    }
    let (lower, upper, page) = reference.arrangement.class_band();
    let outside = (lower - oew_cg_pct_mac).max(oew_cg_pct_mac - upper);
    if outside > 0.0 {
        let severity = if outside > upper - lower {
            Severity::High
        } else {
            Severity::Medium
        };
        out.add_measured(
            W_OEW_CG_REFERENCE.id,
            severity,
            "",
            Some(outside),
            format!(
                "OEW CG {oew_cg_pct_mac:.1} % MAC is {outside:.1} % MAC outside the class band {lower:.0}-{upper:.0} % MAC ({page}; no published state)"
            ),
        );
    }
}

#[cfg(test)]
// Every registered preset has a record; a missing one is the assertion failing.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use alas_config::oew_reference::oew_cg_reference;

    fn severity(preset: &str, cg: f64) -> Option<Severity> {
        let mut out = Findings::new("synthetic");
        check_oew_cg_reference(&mut out, cg, oew_cg_reference(preset).expect("record"));
        out.severity_of(W_OEW_CG_REFERENCE.id)
    }

    #[test]
    fn published_states_bound_the_empty_cg_and_the_band_grades_the_rest() {
        // A320: forward 17 % at 45 t, aft 26.5 % at 41 t.
        assert_eq!(severity("A320-200", 22.0), None);
        assert_eq!(severity("A320-200", 26.6), Some(Severity::High));
        assert_eq!(severity("A320-200", 16.9), Some(Severity::High));
        // B787-9 has no published state: wing-podded band 25-30 % MAC.
        assert_eq!(severity("B787-9", 27.0), None);
        assert_eq!(severity("B787-9", 21.0), Some(Severity::Medium));
        assert_eq!(severity("B787-9", 19.9), Some(Severity::High));
        // ATR 72-600: propellers forward, 20-25 % MAC.
        assert_eq!(severity("ATR72-600", 24.0), None);
        assert_eq!(severity("ATR72-600", 19.0), Some(Severity::Medium));
    }
}
