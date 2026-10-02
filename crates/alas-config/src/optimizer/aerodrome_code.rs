// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The aerodrome reference code letter and the wingspan it admits.
//!
//! ICAO Annex 14, *Aerodromes*, Volume I, Aerodrome Design and Operations,
//! Chapter 1, Table 1-1 (Aerodrome reference code) gives the code letter
//! from the aeroplane's wingspan:
//!
//! | Code letter | Wingspan |
//! |---|---|
//! | A | up to but not including 15 m |
//! | B | 15 m up to but not including 24 m |
//! | C | 24 m up to but not including 36 m |
//! | D | 36 m up to but not including 52 m |
//! | E | 52 m up to but not including 65 m |
//! | F | 65 m up to but not including 80 m |
//!
//! Every band is half-open: a wingspan equal to the upper figure belongs to
//! the next letter, so the constraint for an aerodrome of code `L` is a
//! span strictly below the band's upper figure.
//!
//! The letter is also set by the outer main gear wheel span where that is
//! the more demanding of the two; this module states the wingspan criterion
//! only, which is the one a planform search moves.

use serde::{Deserialize, Serialize};

use crate::design_variables::DesignVector;
use crate::{AlasConfig, DesignMode, Kind, Leaf, VariableEnvelope};

/// Margin below the band's upper figure that a design span must keep, m.
///
/// Annex 14 Table 1-1 bands are open at the top, which a continuous search
/// variable cannot express exactly. One centimetre is below the two decimals
/// a span is reported to, so it never excludes a span that could be
/// published as inside the band. Engineering choice, not an Annex value.
pub const SPAN_CODE_MARGIN_M: f64 = 0.01;

/// An aerodrome reference code letter, or no wingspan limit at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AerodromeReferenceCode {
    /// No wingspan limit is applied.
    #[serde(rename = "unrestricted")]
    Unrestricted,
    /// Wingspan below 15 m.
    #[serde(rename = "A")]
    A,
    /// Wingspan from 15 m to below 24 m.
    #[serde(rename = "B")]
    B,
    /// Wingspan from 24 m to below 36 m.
    #[serde(rename = "C")]
    C,
    /// Wingspan from 36 m to below 52 m.
    #[serde(rename = "D")]
    D,
    /// Wingspan from 52 m to below 65 m.
    #[serde(rename = "E")]
    E,
    /// Wingspan from 65 m to below 80 m, the largest code in Annex 14.
    #[serde(rename = "F")]
    #[default]
    F,
}

impl AerodromeReferenceCode {
    /// Every letter, smallest to largest.
    pub const LETTERS: [Self; 6] = [Self::A, Self::B, Self::C, Self::D, Self::E, Self::F];

    /// Stable serialized name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unrestricted => "unrestricted",
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
            Self::D => "D",
            Self::E => "E",
            Self::F => "F",
        }
    }

    /// Serialized names, in declaration order.
    pub const NAMES: [&'static str; 7] = ["unrestricted", "A", "B", "C", "D", "E", "F"];

    /// The wingspan the band's letter excludes from above, m (Annex 14
    /// Table 1-1), or `None` without a limit.
    pub const fn span_upper_limit_m(self) -> Option<f64> {
        match self {
            Self::Unrestricted => None,
            Self::A => Some(15.0),
            Self::B => Some(24.0),
            Self::C => Some(36.0),
            Self::D => Some(52.0),
            Self::E => Some(65.0),
            Self::F => Some(80.0),
        }
    }

    /// The largest wingspan a design may have under this letter, m: the
    /// band's upper figure less [`SPAN_CODE_MARGIN_M`].
    pub fn max_design_span_m(self) -> Option<f64> {
        self.span_upper_limit_m()
            .map(|limit| limit - SPAN_CODE_MARGIN_M)
    }

    /// The letter whose band contains `span_m`, or `None` outside A to F.
    pub fn for_span_m(span_m: f64) -> Option<Self> {
        Self::LETTERS.into_iter().find(|code| {
            code.span_upper_limit_m()
                .is_some_and(|limit| span_m < limit)
        })
    }
}

impl Leaf for AerodromeReferenceCode {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Str
    }
}

impl AlasConfig {
    /// The code letter whose span limit applies to this configuration.
    ///
    /// A reference adaptation or baseline sandbox of a registered aircraft
    /// uses that aircraft's own letter, because the aerodrome the type
    /// operates from is part of what it is. A clean-sheet study uses the
    /// user's choice, `optimizer.objective.aerodrome_reference_code`. A
    /// registered aircraft with no letter also falls back to that choice.
    pub fn aerodrome_reference_code(&self) -> AerodromeReferenceCode {
        let registered = match self.optimizer.design_space.mode {
            DesignMode::CleanSheet => None,
            DesignMode::ReferenceAdaptation | DesignMode::BaselineSandbox => {
                crate::presets::get(&self.preset)
                    .ok()
                    .and_then(|preset| preset.reference.aerodrome_reference_code)
            }
        };
        registered.unwrap_or(self.optimizer.objective.aerodrome_reference_code)
    }

    /// The largest wingspan a design may have, m, or `None` without a limit.
    pub fn max_design_span_m(&self) -> Option<f64> {
        self.aerodrome_reference_code().max_design_span_m()
    }

    /// [`crate::DesignSpaceConfig::envelope`] with the wingspan window's
    /// upper bound clamped to [`Self::max_design_span_m`].
    ///
    /// A window that starts above the limit (a nominal that already violates
    /// it) is pulled down to the limit as well, so the search is never
    /// handed a span range the hard requirement rejects outright.
    pub fn design_envelope(&self, nominal: &DesignVector) -> Vec<VariableEnvelope> {
        let mut envelope = self.optimizer.design_space.envelope(nominal);
        if let Some(limit) = self.max_design_span_m() {
            for variable in envelope.iter_mut().filter(|v| v.name == "span_m") {
                variable.upper = variable.upper.min(limit);
                variable.lower = variable.lower.min(variable.upper);
                variable.fixed = variable.lower >= variable.upper;
            }
        }
        envelope
    }
}

// A test asserts on values it built here, so a failed unwrap is the assertion
// failing rather than a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_span_on_a_band_edge_belongs_to_the_next_letter() {
        assert_eq!(
            AerodromeReferenceCode::for_span_m(35.99),
            Some(AerodromeReferenceCode::C)
        );
        assert_eq!(
            AerodromeReferenceCode::for_span_m(36.0),
            Some(AerodromeReferenceCode::D)
        );
        assert_eq!(
            AerodromeReferenceCode::for_span_m(79.99),
            Some(AerodromeReferenceCode::F)
        );
        assert_eq!(AerodromeReferenceCode::for_span_m(80.0), None);
    }

    #[test]
    fn the_design_limit_is_strictly_below_the_band_edge() {
        for code in AerodromeReferenceCode::LETTERS {
            let edge = code.span_upper_limit_m().unwrap();
            let design = code.max_design_span_m().unwrap();
            assert!(design < edge);
            assert_eq!(AerodromeReferenceCode::for_span_m(design), Some(code));
        }
        assert_eq!(
            AerodromeReferenceCode::Unrestricted.max_design_span_m(),
            None
        );
    }

    #[test]
    fn serialized_names_match_the_listed_options() {
        let names: Vec<_> = std::iter::once(AerodromeReferenceCode::Unrestricted)
            .chain(AerodromeReferenceCode::LETTERS)
            .map(|code| serde_json::to_value(code).unwrap())
            .collect();
        let listed: Vec<_> = AerodromeReferenceCode::NAMES
            .iter()
            .map(|name| serde_json::json!(name))
            .collect();
        assert_eq!(names, listed);
    }

    #[test]
    fn a_clean_sheet_uses_the_users_letter_and_a_reference_uses_its_own() {
        let mut clean = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
        clean.optimizer.design_space.mode = DesignMode::CleanSheet;
        clean.optimizer.objective.aerodrome_reference_code = AerodromeReferenceCode::E;
        assert!((clean.max_design_span_m().unwrap() - 64.99).abs() < 1e-9);

        let mut reference = clean.clone();
        reference.optimizer.design_space.mode = DesignMode::ReferenceAdaptation;
        assert_eq!(
            reference.aerodrome_reference_code(),
            AerodromeReferenceCode::C
        );
    }

    #[test]
    fn every_registered_preset_fits_its_own_code_letter() {
        // A preset's nominal span must satisfy the very limit its letter
        // imposes, or the registered aircraft would be infeasible by span.
        for preset in crate::presets::registry() {
            let code = preset.reference.aerodrome_reference_code.unwrap();
            let span = preset.design_vector.span_m;
            assert_eq!(
                AerodromeReferenceCode::for_span_m(span),
                Some(code),
                "{}: span {span} m",
                preset.name
            );
            assert!(span <= code.max_design_span_m().unwrap(), "{}", preset.name);
        }
    }

    #[test]
    fn the_span_window_is_clamped_to_the_code_limit() {
        let mut config =
            AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"})).unwrap();
        config.optimizer.design_space.mode = DesignMode::ReferenceAdaptation;
        let nominal = crate::presets::get("A320-200").unwrap().design_vector;
        let span = |config: &AlasConfig| {
            config
                .design_envelope(&nominal)
                .into_iter()
                .find(|v| v.name == "span_m")
                .unwrap()
        };
        assert!(span(&config).upper <= 35.99);
        assert!(span(&config).lower <= span(&config).upper);
        config.optimizer.design_space.mode = DesignMode::CleanSheet;
        config.optimizer.objective.aerodrome_reference_code = AerodromeReferenceCode::Unrestricted;
        assert!(span(&config).upper > 36.0);
    }
}
