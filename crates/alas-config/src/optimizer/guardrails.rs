// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The guardrails of the design variables, per aircraft class.
//!
//! The table in [`crate::design_variables`] declares one guardrail pair per
//! variable (`preset_lower`, `preset_upper`) for the transport fleet. They are
//! a safety envelope, not a search width: a reference adaptation of a
//! registered aircraft searches the configured window around the aircraft's
//! own value, and the guardrail only stops a wider window from leaving the
//! domain its class occupies. The transport pair cut every window of a
//! business jet (the 20 m span floor against the 13.35 m Learjet 60 span
//! collapsed its span window to one value), so a business jet is bounded by
//! its own pair, [`BUSINESS_JET`], for the variables whose transport pair
//! excludes the class; every other variable keeps the transport pair.
//!
//! The business-jet pairs span the smallest and the largest purpose-built
//! business jets of Jane's All the World's Aircraft 2008-2009 (airliner
//! derivatives excluded), from the dimensions printed there
//! (the `BusinessJetRecord`s the tests check them against); none is derived
//! from the registered aircraft, so the registry can be checked against them.

use crate::design_variables::DesignVariableSpec;
use crate::presets::AircraftClass;

/// Dimensions of one business jet as Jane's All the World's Aircraft
/// 2008-2009 prints them, m and m^2 (`None`: not printed). The evidence the
/// tests derive [`BUSINESS_JET`] from.
#[cfg(test)]
#[derive(Debug, Clone, Copy)]
struct BusinessJetRecord {
    /// Wing span (over winglets where printed so).
    span_m: f64,
    /// Gross (reference) wing area.
    wing_area_m2: Option<f64>,
    /// Wing chord at root and at tip.
    chords_m: Option<(f64, f64)>,
    /// Length overall.
    length_m: f64,
}

/// Eclipse 500, p. 742 (the page marks its data provisional): the smallest
/// span and length overall of the edition's business jets.
#[cfg(test)]
const ECLIPSE_500: BusinessJetRecord = BusinessJetRecord {
    span_m: 11.40,
    wing_area_m2: None,
    chords_m: None,
    length_m: 10.08,
};

/// Cessna 510 Citation Mustang, p. 715: the smallest printed wing area (span
/// without tip lights).
#[cfg(test)]
const CITATION_MUSTANG: BusinessJetRecord = BusinessJetRecord {
    span_m: 13.03,
    wing_area_m2: Some(19.51),
    chords_m: None,
    length_m: 12.37,
};

/// Learjet 60, pp. 790-791.
#[cfg(test)]
const LEARJET_60: BusinessJetRecord = BusinessJetRecord {
    span_m: 13.34,
    wing_area_m2: Some(24.57),
    chords_m: Some((2.74, 1.12)),
    length_m: 17.89,
};

/// Gulfstream G550, p. 764 (span without winglets, to go with the gross
/// area): the largest printed wing area.
#[cfg(test)]
const GULFSTREAM_G550: BusinessJetRecord = BusinessJetRecord {
    span_m: 27.69,
    wing_area_m2: Some(105.63),
    chords_m: None,
    length_m: 29.39,
};

/// Bombardier Global Express XRS, p. 64: the largest span, root chord and
/// length overall.
#[cfg(test)]
const GLOBAL_EXPRESS: BusinessJetRecord = BusinessJetRecord {
    span_m: 28.65,
    wing_area_m2: Some(94.95),
    chords_m: Some((6.43, 1.24)),
    length_m: 30.30,
};

#[cfg(test)]
const RECORDS: [BusinessJetRecord; 5] = [
    ECLIPSE_500,
    CITATION_MUSTANG,
    LEARJET_60,
    GULFSTREAM_G550,
    GLOBAL_EXPRESS,
];

/// The business-jet guardrails that differ from the transport pair,
/// `(variable, lower, upper)` in the variable's units, from the printed
/// records (`RECORDS`, checked by the tests):
///
/// - span and fuselage length: the smallest and largest printed span and
///   length overall (a fuselage is no longer than the aircraft);
/// - chords: the printed root and tip chords, widened to the chords a
///   straight-tapered wing of a printed area and span has
///   (`c_root = 2 S / (b (1 + taper))`, `c_tip = taper c_root`) for every
///   taper ratio between the printed ones (Global Express 0.193, Learjet 60
///   0.409), rounded outward to 0.01 m; the break chord lies between the
///   tip and root chords.
const BUSINESS_JET: &[(&str, f64, f64)] = &[
    ("span_m", 11.40, 28.65),
    ("root_chord_m", 2.12, 6.43),
    ("break_chord_m", 0.48, 6.43),
    ("tip_chord_m", 0.48, 2.22),
    ("fuselage_length_m", 10.08, 30.30),
];

impl DesignVariableSpec {
    /// The guardrails `(lower, upper)` of this variable for an aircraft of
    /// `class` (see the module documentation).
    pub fn guardrails_for(&self, class: AircraftClass) -> (f64, f64) {
        let declared = (self.preset_lower, self.preset_upper);
        match class {
            AircraftClass::Transport => declared,
            AircraftClass::BusinessJet => BUSINESS_JET
                .iter()
                .find(|(name, _, _)| *name == self.name)
                .map_or(declared, |&(_, lower, upper)| (lower, upper)),
        }
    }
}

impl crate::AlasConfig {
    /// The class of the configured aircraft: the registered preset's own,
    /// else the class a clean-sheet brief declares
    /// (`optimizer.design_space.aircraft_class`, a transport unless stated).
    pub fn aircraft_class(&self) -> AircraftClass {
        crate::presets::get(&self.preset)
            .map_or(self.optimizer.design_space.aircraft_class, |preset| {
                preset.class
            })
    }
}

/// Every declared business-jet override names a design variable.
#[cfg(test)]
fn every_override_names_a_variable() -> bool {
    BUSINESS_JET.iter().all(|(name, lower, upper)| {
        crate::design_variables::SPECS
            .iter()
            .any(|spec| spec.name == *name)
            && lower < upper
    })
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use crate::design_variables::SPECS;
    use crate::optimizer::{DesignMode, DesignSpaceConfig};
    use crate::presets::{registry, AircraftClass};

    /// Each registered aircraft's own value lies inside its class's declared
    /// guardrails: an independent check, since no pair is derived from the
    /// registry.
    #[test]
    fn every_registered_value_lies_inside_its_class_guardrails() {
        assert!(super::every_override_names_a_variable());
        for preset in registry() {
            for (value, spec) in preset.design_vector.to_array().iter().zip(SPECS) {
                let (lower, upper) = spec.guardrails_for(preset.class);
                assert!(
                    (lower..=upper).contains(value),
                    "{} {}={value} outside its {:?} guardrails [{lower}, {upper}]",
                    preset.name,
                    spec.name,
                    preset.class
                );
            }
        }
    }

    /// No registered aircraft's default reference window collapses onto its
    /// value, and the Learjet 60 keeps its whole span window, which the
    /// transport floor cut to one value.
    #[test]
    fn no_registered_reference_window_collapses() {
        let space = DesignSpaceConfig {
            mode: DesignMode::ReferenceAdaptation,
            ..DesignSpaceConfig::default()
        };
        for preset in registry() {
            for variable in space.envelope_for(&preset.design_vector, preset.class) {
                if variable.fixed {
                    continue;
                }
                assert!(
                    variable.lower < variable.nominal && variable.nominal < variable.upper,
                    "{} {}: [{}, {}] around {}",
                    preset.name,
                    variable.name,
                    variable.lower,
                    variable.upper,
                    variable.nominal
                );
            }
        }
        let lj60 = crate::presets::get("LJ60").unwrap();
        assert_eq!(lj60.class, AircraftClass::BusinessJet);
        let span = space
            .envelope_for(&lj60.design_vector, lj60.class)
            .into_iter()
            .find(|variable| variable.name == "span_m")
            .unwrap();
        assert!((span.lower - 0.9 * 13.35).abs() < 1e-9 && (span.upper - 1.1 * 13.35).abs() < 1e-9);
    }

    /// The business-jet pairs follow from the printed records as the
    /// [`super::BUSINESS_JET`] documentation states.
    #[test]
    fn the_business_jet_pairs_follow_from_the_printed_records() {
        use super::RECORDS;
        let printed_tapers: Vec<f64> = RECORDS
            .iter()
            .filter_map(|r| r.chords_m.map(|(root, tip)| tip / root))
            .collect();
        let taper =
            |pick: fn(f64, f64) -> f64| printed_tapers.iter().copied().reduce(pick).unwrap();
        let (low_taper, high_taper) = (taper(f64::min), taper(f64::max));
        let mut roots: Vec<f64> = Vec::new();
        let mut tips: Vec<f64> = Vec::new();
        for record in RECORDS {
            if let Some((root, tip)) = record.chords_m {
                roots.push(root);
                tips.push(tip);
            }
            if let Some(area) = record.wing_area_m2 {
                for taper in [low_taper, high_taper] {
                    let root = 2.0 * area / (record.span_m * (1.0 + taper));
                    roots.push(root);
                    tips.push(taper * root);
                }
            }
        }
        let low = |values: &[f64]| {
            (values.iter().copied().fold(f64::INFINITY, f64::min) * 100.0 + 1e-6).floor() / 100.0
        };
        let high = |values: &[f64]| {
            (values.iter().copied().fold(f64::NEG_INFINITY, f64::max) * 100.0 - 1e-6).ceil() / 100.0
        };
        let spans: Vec<f64> = RECORDS.iter().map(|r| r.span_m).collect();
        let lengths: Vec<f64> = RECORDS.iter().map(|r| r.length_m).collect();
        let expected = [
            ("span_m", low(&spans[..]), high(&spans[..])),
            ("root_chord_m", low(&roots[..]), high(&roots[..])),
            ("break_chord_m", low(&tips[..]), high(&roots[..])),
            ("tip_chord_m", low(&tips[..]), high(&tips[..])),
            ("fuselage_length_m", low(&lengths[..]), high(&lengths[..])),
        ];
        for ((name, lower, upper), (declared, declared_lower, declared_upper)) in
            expected.iter().zip(super::BUSINESS_JET)
        {
            assert_eq!(name, declared);
            assert!(
                (lower - declared_lower).abs() < 1e-9 && (upper - declared_upper).abs() < 1e-9,
                "{name}: records give [{lower}, {upper}], declared [{declared_lower}, {declared_upper}]"
            );
        }
    }

    /// A clean-sheet brief that declares itself a business jet is held to the
    /// business-jet guardrails; a registered aircraft keeps its preset's class.
    #[test]
    fn a_brief_declares_its_class_and_a_preset_keeps_its_own() {
        let brief = |extra: serde_json::Value| {
            let mut value = serde_json::json!({
                "requirements": {"mtow_kg": 10_000.0, "num_passengers": 7},
                "optimizer": {"design_space": {"mode": "clean_sheet"}}
            });
            if let Some(class) = extra.as_str() {
                value["optimizer"]["design_space"]["aircraft_class"] = serde_json::json!(class);
            }
            crate::AlasConfig::from_value(&value).unwrap()
        };
        assert_eq!(
            brief(serde_json::Value::Null).aircraft_class(),
            AircraftClass::Transport
        );
        let jet = brief(serde_json::json!("business_jet"));
        assert_eq!(jet.aircraft_class(), AircraftClass::BusinessJet);
        let span = SPECS.iter().find(|spec| spec.name == "span_m").unwrap();
        assert_eq!(span.guardrails_for(jet.aircraft_class()).0, 11.40);
        let saved = serde_json::to_value(&jet).unwrap();
        assert_eq!(
            saved["optimizer"]["design_space"]["aircraft_class"],
            "business_jet"
        );
        let preset = crate::AlasConfig::from_value(&serde_json::json!({
            "preset": "A320-200",
            "optimizer": {"design_space": {"aircraft_class": "business_jet"}}
        }))
        .unwrap();
        assert_eq!(preset.aircraft_class(), AircraftClass::Transport);
    }

    /// A transport keeps the declared pair: the business-jet envelope does
    /// not lower its floors.
    #[test]
    fn a_transport_is_bounded_by_the_declared_pair() {
        for spec in SPECS {
            assert_eq!(
                spec.guardrails_for(AircraftClass::Transport),
                (spec.preset_lower, spec.preset_upper),
                "{}",
                spec.name
            );
        }
        let span = SPECS.iter().find(|spec| spec.name == "span_m").unwrap();
        assert_eq!(span.guardrails_for(AircraftClass::Transport).0, 20.0);
        assert_eq!(
            span.guardrails_for(AircraftClass::BusinessJet),
            (11.40, 28.65)
        );
    }
}
