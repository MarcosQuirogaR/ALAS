// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The embedded loft data files and the reference that selects one.
//!
//! Files live in `crates/alas-config/data/lofts/<id>.json` and are compiled in
//! with `include_str!`, as `data/engines.json` is: a data-file diff is
//! reviewable, the fitting tool writes JSON natively, and 12 presets of 60-100
//! floats would not fit the source-size budgets as Rust constants. The registry
//! below lists the converted presets (wave 1: C919, A220-300, A320-200, E195-E2,
//! B787-9; the quality pass added the A340-300 after a refit with `E >= 0` at
//! the tail base; wave 2: B747-400 with its section residual, A380-800, AVE).

use serde::{Deserialize, Serialize};

use super::spec::LoftSpec;
use super::validate::LoftSpecError;

/// `(id, file contents)` of every embedded loft. A converted preset adds one
/// line here (and one in [`PRESET_LOFTS`]):
///
/// ```text
/// ("a320_200", include_str!("../../../data/lofts/a320_200.json")),
/// ```
const EMBEDDED: &[(&str, &str)] = &[
    (
        "a220_300",
        include_str!("../../../data/lofts/a220_300.json"),
    ),
    (
        "a320_200",
        include_str!("../../../data/lofts/a320_200.json"),
    ),
    (
        "a340_300",
        include_str!("../../../data/lofts/a340_300.json"),
    ),
    (
        "a380_800",
        include_str!("../../../data/lofts/a380_800.json"),
    ),
    ("a400m", include_str!("../../../data/lofts/a400m.json")),
    ("ave", include_str!("../../../data/lofts/ave.json")),
    (
        "atr72_600",
        include_str!("../../../data/lofts/atr72_600.json"),
    ),
    (
        "b747_400",
        include_str!("../../../data/lofts/b747_400.json"),
    ),
    ("b787_9", include_str!("../../../data/lofts/b787_9.json")),
    ("c919", include_str!("../../../data/lofts/c919.json")),
    ("dc_10", include_str!("../../../data/lofts/dc_10.json")),
    ("e195_e2", include_str!("../../../data/lofts/e195_e2.json")),
];

/// `(preset name, loft id)`: the loft a preset's fuselage references. Every
/// target must be in [`EMBEDDED`] (a unit test checks it).
const PRESET_LOFTS: &[(&str, &str)] = &[
    ("A220-300", "a220_300"),
    ("A320-200", "a320_200"),
    ("A340-300", "a340_300"),
    ("A380-800", "a380_800"),
    ("A400M", "a400m"),
    ("ATR72-600", "atr72_600"),
    ("AVE", "ave"),
    ("B747-400", "b747_400"),
    ("B787-9", "b787_9"),
    ("C919", "c919"),
    ("DC-10", "dc_10"),
    ("E195-E2", "e195_e2"),
];

/// Hand-made fixtures that exist only to exercise the builder path end to end:
/// compiled under test or the `test-fixtures` feature, never listed by
/// [`embedded_loft_ids`], and never referenced by a preset.
#[cfg(any(test, feature = "test-fixtures"))]
const TEST_FIXTURES: &[(&str, &str)] = &[(
    "test_fuselage",
    include_str!("../../../tests/fixtures/loft_test_fuselage.json"),
)];
#[cfg(not(any(test, feature = "test-fixtures")))]
const TEST_FIXTURES: &[(&str, &str)] = &[];

/// Reference from a fuselage configuration to an embedded loft data file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoftRef {
    /// Identifier of the data file, the stem of `data/lofts/<id>.json`.
    pub id: String,
}

impl LoftRef {
    /// A reference to the embedded loft `id`.
    #[must_use]
    pub fn embedded(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }

    /// The referenced loft.
    ///
    /// # Errors
    ///
    /// [`LoftSpecError::UnknownEmbedded`] when no embedded file has this id
    /// (a typo must not silently fall back to the parametric laws), or the
    /// parse / validation error of an invalid embedded file.
    pub fn resolve(&self) -> Result<LoftSpec, LoftSpecError> {
        resolve_id(&self.id)
    }
}

/// Parse and validate one loft data file.
///
/// # Errors
///
/// [`LoftSpecError::Parse`] when the JSON does not match schema v1, or the
/// validation error of [`LoftSpec::validate`].
pub fn parse_loft_spec(json: &str) -> Result<LoftSpec, LoftSpecError> {
    let spec: LoftSpec =
        serde_json::from_str(json).map_err(|e| LoftSpecError::Parse(e.to_string()))?;
    spec.validate()?;
    Ok(spec)
}

/// Embedded files first, then the test fixtures; an id in neither is an error.
fn resolve_id(id: &str) -> Result<LoftSpec, LoftSpecError> {
    if EMBEDDED.iter().any(|(name, _)| *name == id) {
        lookup(EMBEDDED, id)
    } else {
        lookup(TEST_FIXTURES, id)
    }
}

fn lookup(files: &[(&str, &str)], id: &str) -> Result<LoftSpec, LoftSpecError> {
    let (_, json) = files
        .iter()
        .find(|(name, _)| *name == id)
        .ok_or_else(|| LoftSpecError::UnknownEmbedded(id.to_owned()))?;
    parse_loft_spec(json)
}

/// The embedded loft with this id, parsed and validated; `None` if the id is
/// unknown or its file is invalid (a unit test per file guards the latter).
/// Callers that hold a [`LoftRef`] use [`LoftRef::resolve`], which says why.
#[must_use]
pub fn embedded_loft(id: &str) -> Option<LoftSpec> {
    resolve_id(id).ok()
}

/// Ids of every embedded loft.
#[must_use]
pub fn embedded_loft_ids() -> Vec<&'static str> {
    EMBEDDED.iter().map(|(id, _)| *id).collect()
}

/// The embedded loft id a preset's fuselage references, if the preset has been
/// converted.
#[must_use]
pub fn preset_loft_id(preset_name: &str) -> Option<&'static str> {
    PRESET_LOFTS
        .iter()
        .find(|(preset, _)| *preset == preset_name)
        .map(|(_, id)| *id)
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::super::spec::Closure;
    use super::*;

    /// A unit sphere (radius 1 m) as a one-span cubic body: hand-made fixture.
    const SPHERE: &str = include_str!("../../../tests/fixtures/loft_test_sphere.json");

    fn sphere() -> LoftSpec {
        parse_loft_spec(SPHERE).unwrap()
    }

    #[test]
    fn every_embedded_loft_parses_and_validates() {
        for id in embedded_loft_ids() {
            assert!(embedded_loft(id).is_some(), "embedded loft {id} is invalid");
        }
    }

    #[test]
    fn every_preset_mapping_targets_an_embedded_file() {
        for (preset, id) in PRESET_LOFTS {
            assert!(
                embedded_loft_ids().contains(id),
                "preset {preset} maps to {id}, which is not embedded"
            );
            assert_eq!(preset_loft_id(preset), Some(*id));
        }
        assert_eq!(preset_loft_id("no such preset"), None);
    }

    #[test]
    fn the_test_fixture_resolves_but_is_not_listed_as_embedded_data() {
        let spec = LoftRef::embedded("test_fuselage").resolve().unwrap();
        assert_eq!(spec.reference_length_m, 37.5);
        assert!(embedded_loft("test_fuselage").is_some());
        assert!(!embedded_loft_ids().contains(&"test_fuselage"));
        assert_eq!(preset_loft_id("test_fuselage"), None);
    }

    #[test]
    fn a_drawing_page_may_be_a_number_or_free_text() {
        let text = |page: &str| -> String {
            let json = SPHERE.replace(
                "\"provenance\": [",
                &format!("\"provenance\": [{{\"group\": \"B\", \"kind\": \"measured\", \"drawing\": {{\"page\": {page}}}}},"),
            );
            parse_loft_spec(&json).unwrap().provenance[0]
                .drawing
                .page
                .clone()
        };
        assert_eq!(text("\"2-2-0 p2 (side)\""), "2-2-0 p2 (side)");
        assert_eq!(text("25"), "25");
        assert_eq!(text("0"), "");
    }

    #[test]
    fn the_registry_lists_all_twelve_presets() {
        let mut ids = embedded_loft_ids();
        ids.sort_unstable();
        assert_eq!(
            ids,
            [
                "a220_300",
                "a320_200",
                "a340_300",
                "a380_800",
                "a400m",
                "atr72_600",
                "ave",
                "b747_400",
                "b787_9",
                "c919",
                "dc_10",
                "e195_e2"
            ],
            "all twelve registered presets (waves 1 and 2, integration step A)"
        );
        assert_eq!(preset_loft_id("A340-300"), Some("a340_300"));
        assert!(embedded_loft("a340_300").is_some());
        assert_eq!(preset_loft_id("A380-800"), Some("a380_800"));
        assert_eq!(preset_loft_id("DC-10"), Some("dc_10"));
        assert!(matches!(
            LoftRef::embedded("no_such_loft").resolve(),
            Err(LoftSpecError::UnknownEmbedded(_))
        ));
    }

    #[test]
    fn the_fixture_matches_the_frozen_schema_and_round_trips() {
        let spec = sphere();
        assert_eq!(spec.degree, 3);
        assert_eq!(spec.front, Closure::Point { tip_radius_m: 1.0 });
        assert_eq!(spec.rear, Closure::Point { tip_radius_m: 1.0 });
        assert_eq!(spec.curves.a.knots.len(), spec.curves.a.coeffs.len() + 4);
        let json = serde_json::to_string(&spec).unwrap();
        assert_eq!(parse_loft_spec(&json).unwrap(), spec);
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(value["curves"].get("A").is_some() && value["curves"].get("E").is_some());
        assert_eq!(value["front"]["kind"], "point");
    }

    #[test]
    fn lookup_finds_a_listed_file_and_rejects_an_invalid_one() {
        let files = [("sphere", SPHERE), ("broken", "{\"schema\": 1}")];
        assert!(lookup(&files, "sphere").is_ok());
        assert!(lookup(&files, "broken").is_err());
        assert!(matches!(
            lookup(&files, "missing"),
            Err(LoftSpecError::UnknownEmbedded(_))
        ));
    }

    #[test]
    fn an_unknown_field_in_a_structural_block_is_rejected() {
        let json = SPHERE.replace("\"tv_ratio_max\"", "\"tv_ratio_maxx\"");
        assert!(matches!(
            parse_loft_spec(&json),
            Err(LoftSpecError::Parse(_))
        ));
    }

    #[test]
    fn a_closure_with_an_unknown_field_is_rejected_like_the_schema_says() {
        let json = SPHERE.replace(
            "\"kind\": \"point\", \"tip_radius_m\": 1.0}",
            "\"kind\": \"point\", \"tip_radius_m\": 1.0, \"bogus\": 1}",
        );
        assert_ne!(json, SPHERE);
        assert!(matches!(
            parse_loft_spec(&json),
            Err(LoftSpecError::Parse(_))
        ));
    }

    #[test]
    fn loft_ref_serialises_as_a_bare_id_record() {
        let value = serde_json::to_value(LoftRef::embedded("a320_200")).unwrap();
        assert_eq!(value, serde_json::json!({"id": "a320_200"}));
    }

    #[test]
    fn a_fuselage_without_a_loft_serialises_without_the_key_and_reads_back_as_none() {
        let fuselage = crate::FuselageConfig::default();
        assert!(fuselage.loft.is_none());
        let mut value = serde_json::to_value(&fuselage).unwrap();
        assert!(value.get("loft").is_none());
        let back: crate::FuselageConfig = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(back, fuselage);
        value["loft"] = serde_json::json!({"id": "a320_200"});
        let with: crate::FuselageConfig = serde_json::from_value(value).unwrap();
        assert_eq!(with.loft, Some(LoftRef::embedded("a320_200")));
    }
}
