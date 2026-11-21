// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The altitude-thrust evidence of a catalogue turbofan.
//!
//! An engine datum (a maximum-cruise or maximum-climb thrust of the engine or
//! of a sibling of its type certificate, at a stated altitude and Mach
//! number) scales Jenkinson's maximum-climb charts onto the engine. An engine
//! without one runs the charts unscaled; its off-design source then carries an
//! explicit evidence-gap statement that says what was searched and names the
//! document that would close the gap.

use super::TurbofanOffDesignSpec;

/// Words that open the evidence-gap statement in an off-design source.
pub const ALTITUDE_DATUM_GAP_MARKER: &str = "EVIDENCE GAP (altitude thrust):";

/// Words that name, inside the statement, the document that closes the gap.
pub const ALTITUDE_DATUM_CLOSING_DOCUMENT: &str = "Closing document:";

impl TurbofanOffDesignSpec {
    /// The evidence-gap statement of an engine whose altitude thrust rests on
    /// the unscaled charts: the source from [`ALTITUDE_DATUM_GAP_MARKER`] on.
    /// `None` for an engine datum, and for a class statistic that states no
    /// gap (a configuration saved before the statement existed).
    pub fn altitude_datum_gap(&self) -> Option<&str> {
        if self.is_engine_datum() {
            return None;
        }
        self.source
            .find(ALTITUDE_DATUM_GAP_MARKER)
            .map(|start| &self.source[start..])
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::super::{database, get};
    use super::*;

    #[test]
    fn every_engine_without_a_datum_states_its_gap_and_the_closing_document() {
        let mut gaps = Vec::new();
        for engine in database() {
            let Some(off_design) = engine.off_design.as_ref() else {
                continue;
            };
            let Some(gap) = off_design.altitude_datum_gap() else {
                // Only an engine datum may run without a gap statement.
                assert!(off_design.is_engine_datum(), "{}", engine.name);
                continue;
            };
            assert!(off_design.is_class_statistic(), "{}", engine.name);
            assert!(gap.contains("searched 2026-11-19"), "{}", engine.name);
            let closing = gap
                .split(ALTITUDE_DATUM_CLOSING_DOCUMENT)
                .nth(1)
                .unwrap_or_else(|| panic!("{}: no closing document", engine.name));
            assert!(closing.contains("BADA"), "{}", engine.name);
            assert!(
                closing.contains("customer performance deck"),
                "{}",
                engine.name
            );
            gaps.push(engine.name.as_str());
        }
        gaps.sort_unstable();
        assert_eq!(
            gaps,
            [
                "GE9X",
                "GEnx-1B",
                "PW1500G",
                "PW1900G",
                "Trent 900",
                "Trent 970-84",
                "Trent XWB-84"
            ]
        );
    }

    #[test]
    fn a_datum_with_the_marker_in_its_text_is_still_no_gap() {
        let spec = TurbofanOffDesignSpec {
            cruise_reference_thrust_n: 24_773.0,
            cruise_reference_altitude_m: 10_668.0,
            cruise_reference_mach: 0.78,
            evidence: "family-proxy".to_owned(),
            source: format!("Family proxy. Closes the earlier {ALTITUDE_DATUM_GAP_MARKER} row"),
            rating: Some("maximum-climb".to_owned()),
        };
        assert!(spec.is_engine_datum());
        assert_eq!(spec.altitude_datum_gap(), None);
        // A class statistic saved without the statement states no gap.
        let saved = TurbofanOffDesignSpec {
            evidence: "openap-static-fallback".to_owned(),
            source: "OpenAP thrust.py Svoboda fallback: 0.2 * rated thrust + 890 N".to_owned(),
            rating: None,
            ..spec
        };
        assert!(!saved.is_engine_datum());
        assert_eq!(saved.altitude_datum_gap(), None);
    }

    #[test]
    fn the_generic_trent_900_row_is_labelled_the_class_statistic() {
        let off_design = get("Trent 900").unwrap().off_design.as_ref().unwrap();
        assert_eq!(off_design.evidence, "openap-static-fallback");
        assert!((off_design.cruise_reference_thrust_n - (0.2 * 374_000.0 + 890.0)).abs() < 0.5);
        // The A380's other engine is quoted as a cross-check, not as a datum.
        let gap = off_design.altitude_datum_gap().unwrap();
        assert!(gap.contains("GP7270") && gap.contains("not a datum"));
    }

    #[test]
    fn the_leap_1c_datum_is_the_leap_1a_maximum_climb_ratio_carried_to_its_rating() {
        // LEAP-1A26: 23,600 N at FL350 (10,668 m), M 0.78, maximum climb, over
        // its 120.636 kN rating: FN/FN* = 0.19563. LEAP-1C (same type
        // certificate, EASA.E.110), deck rating 126.63 kN: 0.19563 x 126,630 N
        // = 24,773 N at the same point.
        let leap_1a = get("LEAP-1A").unwrap().turbofan_spec().unwrap();
        let leap_1c = get("LEAP-1C").unwrap().turbofan_spec().unwrap();
        assert_eq!(leap_1a.rated_thrust_kn, 120.636);
        assert_eq!(leap_1c.rated_thrust_kn, 126.63);
        let ratio: f64 = 23_600.0 / 120_636.0;
        assert!((ratio - 0.19563).abs() < 5.0e-6, "{ratio}");
        let a = &leap_1a.off_design;
        let c = &leap_1c.off_design;
        assert_eq!(a.cruise_reference_thrust_n, 23_600.0);
        assert!((c.cruise_reference_thrust_n - ratio * 126_630.0).abs() < 1.0);
        assert_eq!(c.cruise_reference_thrust_n, 24_773.0);
        // The point of the sibling: altitude, Mach number and rating.
        assert_eq!(c.cruise_reference_altitude_m, a.cruise_reference_altitude_m);
        assert_eq!(c.cruise_reference_mach, a.cruise_reference_mach);
        assert!(c.is_maximum_climb_datum() && a.is_maximum_climb_datum());
        assert_eq!(c.evidence, "family-proxy");
        assert!(c.is_engine_datum());
        assert!(!c.is_class_statistic());
        assert_eq!(c.altitude_datum_gap(), None);
        assert!(c.source.contains("EASA.E.110"));
        assert!(c.source.contains("LEAP-1A26"));
        // Carried at the certified LEAP-1C28 rating instead: 25,427 N, +2.6 %.
        let certified = ratio * 129_976.0;
        assert!((certified - 25_427.0).abs() < 1.0, "{certified}");
        assert!(c.source.contains("25,427 N"));
    }
}
