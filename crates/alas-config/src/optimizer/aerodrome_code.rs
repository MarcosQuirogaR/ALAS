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
    /// The letter whose span band holds the class span a clean-sheet brief
    /// implies, re-derived whenever the brief is committed
    /// (`ObjectiveConfig::derived_aerodrome_code`). The default for a
    /// preset-less clean-sheet brief only.
    #[serde(rename = "auto")]
    Auto,
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
            Self::Auto => "auto",
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
    pub const NAMES: [&'static str; 8] = ["auto", "unrestricted", "A", "B", "C", "D", "E", "F"];

    /// The wingspan the band's letter excludes from above, m (Annex 14
    /// Table 1-1), or `None` without a limit. `Auto` has no band of its own;
    /// [`AlasConfig::aerodrome_reference_code`] resolves it to a letter.
    pub const fn span_upper_limit_m(self) -> Option<f64> {
        match self {
            Self::Auto | Self::Unrestricted => None,
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
        let objective = &self.optimizer.objective;
        registered.unwrap_or(match objective.aerodrome_reference_code {
            // The letter derived when the brief was last committed, so the
            // limit cannot drift while an analysis rewrites brief fields.
            AerodromeReferenceCode::Auto => objective
                .derived_aerodrome_code
                .unwrap_or_else(|| self.class_aerodrome_code()),
            code => code,
        })
    }

    /// The largest wingspan a design may have, m, or `None` without a limit.
    pub fn max_design_span_m(&self) -> Option<f64> {
        self.aerodrome_reference_code().max_design_span_m()
    }

    /// [`crate::DesignSpaceConfig::envelope`], with a clean sheet's derived
    /// and explicit boxes applied (see [`crate::clean_sheet`]) and the
    /// wingspan window's upper bound clamped to [`Self::max_design_span_m`].
    ///
    /// A window that starts above the limit (a nominal that already violates
    /// it) is pulled down to the limit as well, so the search is never
    /// handed a span range the hard requirement rejects outright.
    ///
    /// The wing position of a redesigned registered aircraft is not searched
    /// ([`Self::derives_wing_position`]) and the sweep of one that cruises
    /// below the compressibility-drag onset is held
    /// ([`Self::holds_reference_sweep`]).
    pub fn design_envelope(&self, nominal: &DesignVector) -> Vec<VariableEnvelope> {
        let mut envelope = self
            .optimizer
            .design_space
            .envelope_for(nominal, self.aircraft_class());
        self.apply_clean_sheet_bounds(&mut envelope);
        let hold = |envelope: &mut Vec<VariableEnvelope>, name: &str| {
            for variable in envelope.iter_mut().filter(|v| v.name == name) {
                variable.lower = variable.nominal;
                variable.upper = variable.nominal;
                variable.fixed = true;
            }
        };
        if self.derives_wing_position() {
            hold(&mut envelope, WING_POSITION_VARIABLE);
        }
        if self.holds_reference_sweep() {
            hold(&mut envelope, "sweep_deg");
        }
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

/// The design variable that places the wing along the fuselage.
pub const WING_POSITION_VARIABLE: &str = "wing_x_shift_m";

impl AlasConfig {
    /// Whether the wing position of a candidate is derived from its balance
    /// instead of searched: every redesign of a registered aircraft (a
    /// reference adaptation or a clean sheet started from a preset), not a
    /// fixed-aircraft sandbox and not a brief without a registered aircraft.
    ///
    /// A designer places the wing where the loaded aircraft balances
    /// (Torenbeek, *Synthesis of Subsonic Airplane Design*, 1982, ch. 8,
    /// the wing location along the fuselage from the loading diagram;
    /// Raymer, *Aircraft Design: A Conceptual Approach*, ch. 15 and 16). The
    /// evaluator therefore moves the wing until the take-off centre of gravity
    /// sits at the registered aircraft's fraction of the mean aerodynamic
    /// chord, and reports the position it found in the design vector. Left
    /// free, the search moved the wing up to 1.7 m under a fixed fuselage,
    /// cabin and gear, and the centre-of-gravity rows followed the chord
    /// instead of the loading.
    #[must_use]
    pub fn derives_wing_position(&self) -> bool {
        self.optimizer.design_space.mode != crate::DesignMode::BaselineSandbox
            && crate::presets::get(&self.preset).is_ok()
    }

    /// Whether a reference adaptation holds the sweep at the registered
    /// value: the design cruise Mach is below `drag_model.wave_drag_onset_mach`
    /// (0.6 by default), the Mach below which no transport section has a
    /// drag rise. The product drag law (Shevell-Kroo, `Auto`) is not gated
    /// by that Mach and has a crest-critical creep at every Mach, but at these
    /// speeds it is negligible: at cruise the ATR 72-600 (Mach 0.44) carries
    /// 0.17 counts and the C295 (Mach 0.42) 0.09 counts of it, with a model
    /// crest-critical Mach of about 0.61 for their conventional sections, and
    /// a degree of sweep changes it by about 0.002 counts. Sweep therefore
    /// cannot pay for its wing mass and low-speed lift through drag, and a
    /// change of it is a change of the mean-chord station alone. Both are
    /// held; every jet and the A400M (Mach 0.72, 17.5 counts at cruise) keep
    /// the sweep window.
    #[must_use]
    pub fn holds_reference_sweep(&self) -> bool {
        self.optimizer.design_space.mode == crate::DesignMode::ReferenceAdaptation
            && self.requirements.cruise_mach < self.resolved_drag_model().wave_drag_onset_mach
    }
}

// A test asserts on values it built here, so a failed unwrap is the assertion
// failing rather than a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_redesigned_registered_aircraft_does_not_search_its_wing_position() {
        let variable = |config: &AlasConfig, name: &str| {
            let nominal = config.configured_nominal_design();
            config
                .design_envelope(&nominal)
                .into_iter()
                .find(|variable| variable.name == name)
                .unwrap()
        };
        for mode in ["reference_adaptation", "clean_sheet"] {
            let config = AlasConfig::from_value(&serde_json::json!({
                "preset": "A320-200",
                "optimizer": { "design_space": { "mode": mode } },
            }))
            .unwrap();
            assert!(config.derives_wing_position(), "{mode}");
            let position = variable(&config, WING_POSITION_VARIABLE);
            assert!(position.fixed && position.lower == position.upper, "{mode}");
            assert_eq!(position.lower, position.nominal, "{mode}");
            // The jet keeps its sweep window.
            assert!(!config.holds_reference_sweep(), "{mode}");
            assert!(!variable(&config, "sweep_deg").fixed, "{mode}");
        }
        // A fixed-aircraft sandbox and a brief without a registered aircraft
        // derive nothing.
        let sandbox = AlasConfig::from_value(&serde_json::json!({
            "preset": "A320-200",
            "optimizer": { "design_space": { "mode": "baseline_sandbox" } },
        }))
        .unwrap();
        assert!(!sandbox.derives_wing_position());
        assert!(!AlasConfig::default().derives_wing_position());
    }

    #[test]
    fn a_reference_adaptation_below_the_wave_drag_onset_holds_its_sweep() {
        for (preset, held) in [
            ("ATR72-600", true),
            ("C295", true),
            ("A400M", false),
            ("LJ60", false),
            ("A320-200", false),
        ] {
            let config = AlasConfig::from_value(&serde_json::json!({
                "preset": preset,
                "optimizer": { "design_space": { "mode": "reference_adaptation" } },
            }))
            .unwrap();
            assert_eq!(config.holds_reference_sweep(), held, "{preset}");
            let nominal = config.configured_nominal_design();
            let sweep = config
                .design_envelope(&nominal)
                .into_iter()
                .find(|variable| variable.name == "sweep_deg")
                .unwrap();
            assert_eq!(sweep.fixed, held, "{preset}");
            if held {
                assert_eq!((sweep.lower, sweep.upper), (sweep.nominal, sweep.nominal));
            }
        }
        // A clean sheet searches the sweep whatever the Mach.
        let clean = AlasConfig::from_value(&serde_json::json!({
            "preset": "ATR72-600",
            "optimizer": { "design_space": { "mode": "clean_sheet" } },
        }))
        .unwrap();
        assert!(!clean.holds_reference_sweep());
    }

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
        let names: Vec<_> = [
            AerodromeReferenceCode::Auto,
            AerodromeReferenceCode::Unrestricted,
        ]
        .into_iter()
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
