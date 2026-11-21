// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/physics_config.py

//! Coefficients of the semi-empirical drag buildup.
//!
//! The parasite-drag estimate is Raymer's component method and the transonic
//! rise is the Korn equation, and both are parameterized by a handful of
//! technology and interference factors. In the scripts this program grew out
//! of these were inline literals, `* 1.10`, `0.95`, `radians(32)`, which
//! made the fidelity assumptions invisible and unadjustable. Gathering them
//! here is what makes them either.
//!
//! References for the values themselves are in each field's explanation:
//! D. P. Raymer, *Aircraft Design: A Conceptual Approach*, for the form and
//! interference factors, and the Korn equation as it is usually given for the
//! wave-drag rise.
//!
//! The Korn technology factor is not one of these coefficients: it follows
//! from the section technology the wing is declared with,
//! [`crate::AirfoilClass`] on `geometry.wing.airfoil_class`.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// Tunable coefficients for the semi-empirical drag buildup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct DragModelConfig {
    /// Version of this node's own schema; see [`DRAG_MODEL_SCHEMA_VERSION`].
    ///
    /// A file saved before the viscous margin became the sourced propulsion
    /// class allowance states no version, so it reads back as 1 and a margin
    /// of 1.00 in it is the retired shipped default, not a stated value. The
    /// retired-key migration reads the marker from the raw document, because
    /// the overlay merges onto this build's defaults.
    #[serde(default = "legacy_drag_model_schema_version")]
    #[config(skip)]
    pub schema_version: u32,
    /// Exclude the main-wing center section shielded by the fuselage.
    #[serde(default = "exclude_buried_main_wing_area_default")]
    #[config(
        label = "Exclude buried main-wing area",
        help = "Subtract the tapered centre section inside the local fuselage width at the root quarter chord from the parasite wetted area (NASA NDARC exposed-area convention).",
        long_help = "Subtract the tapered center section inside the local fuselage width at the root quarter chord from the parasite wetted-area estimate (NASA NDARC exposed-area convention). This is a local body approximation, not a surface intersection. Disable to replay the gross-wing-area convention."
    )]
    pub exclude_buried_main_wing_area: bool,
    /// Chordwise location of maximum airfoil thickness.
    #[config(
        label = "Max-thickness chordwise location",
        unit = "x/c",
        help = "Chordwise location of maximum airfoil thickness (as a fraction of chord), used in the wing form factor."
    )]
    pub max_thickness_chordwise_loc: f64,

    /// How much the wing-fuselage junction disturbs the local flow.
    #[config(
        label = "Wing interference factor (Q)",
        help = "How much the wing-fuselage junction disturbs the local flow, multiplying the wing's parasite drag. 1.0 = no interference."
    )]
    pub interference_factor_wing: f64,

    /// How much neighbouring components disturb the fuselage's flow.
    ///
    /// Read only by the frozen reference-compatibility drag buildup; the
    /// product path uses the constant `FUSELAGE_INTERFERENCE` = 1.0 (Raymer
    /// 12.5.5) and ignores this field, so changing it does not change a
    /// product result.
    #[config(
        label = "Fuselage interference factor (Q)",
        help = "How much neighbouring components (wing, tail) disturb the fuselage's flow, multiplying its parasite drag."
    )]
    pub interference_factor_fuselage: f64,

    /// Multiplier on the component parasite-drag sum for leakage and
    /// protuberance drag (Raymer Table 12.8; see [`DEFAULT_VISCOUS_MARGIN`]).
    #[config(
        label = "Viscous drag margin",
        help = "Multiplier on the summed component parasite drag for leakage and protuberances a build-up cannot see (dimensionless).",
        long_help = "Multiplier on the sum of the component parasite drags, for leakage and protuberances that a component build-up cannot see. Leave unset for the default of 1.00 (a stated value always wins): both product build-ups already book these items. A turbofan aircraft takes the jet-transport build-up of Poll and Schumann (The Aeronautical Journal 125, 2021, App. B), whose interference and secondary-drag factors hold the excrescences, leakage, pylons, fairings and afterbody. A turboprop takes the Torenbeek and Raymer component build-up plus the excrescence rows of Obert, Aerodynamic Design of Transport Aircraft (2009), Fig. 40.29, p. 537, as shares of the component sum: control-surface gaps 6.6 %, nacelle intakes and exhausts 7.5 %, other excrescences 3.3 %, excrescences in the slipstream 1.1 %. A stated margin multiplies the whole sum. Raymer, Aircraft Design: A Conceptual Approach, 6th ed., Table 12.8, p. 431, gives 2 to 5 % for jet transports and 5 to 10 % for propeller aircraft for a build-up without those items. The frozen reference configuration uses 1.10."
    )]
    pub viscous_margin: Option<f64>,

    /// Lift-dependent viscous (profile and interference) drag, as a
    /// fraction of the clean zero-lift drag per unit `CL^2`.
    #[serde(default, skip_serializing_if = "is_zero_fraction")]
    #[config(
        label = "Lift-dependent viscous drag fraction",
        unit = "1/CL^2",
        help = "Adds CD_v = fraction * CD0 * CL^2 to the clean drag polar: lift-dependent profile drag an inviscid induced drag cannot see.",
        long_help = "Adds CD_v = fraction * CD0 * CL^2 to the drag polar: the lift-dependent profile and interference drag that an inviscid wake-energy induced drag cannot see, CDp = CD0 (1 + k_L CL^2) (total Oswald factor 1/e = 1/e_inviscid + fraction * CD0 * pi * A). Default 0.25 from Jenkinson, Civil Jet Aircraft Design (1999), Ch. 8, pp. 179-180: dCD/dCL^2 = 0.35 CD0 for older-technology designs (B737-300, B757, B767) and 0.15 CD0 for advanced-technology designs (A330/A340, B777), the A320 in between; the default is the midpoint of that printed range because the product carries no technology class (a rule on the printed range, not a fit). Applies to the clean (cruise and climb) polar only; take-off and landing configurations keep their own drag increments. Jenkinson takes the viscous part as proportional to the aircraft profile drag CD0 (p. 179); here CD0 is the zero-lift drag including the Viscous drag margin, so the term scales with the margin. 0 disables the term."
    )]
    pub lift_dependent_viscous_fraction: f64,

    /// How much the nacelle and pylon disturb the local flow.
    #[config(
        label = "Nacelle/pylon interference factor (Q)",
        help = "How much the engine nacelle and pylon disturb the local flow. Typical value 1.3 for podded under-wing installations (Raymer)."
    )]
    pub interference_factor_nacelle: f64,

    /// Below this Mach number, wave drag is taken as zero.
    #[config(
        label = "Wave-drag onset Mach",
        help = "Below this Mach number, transonic wave drag is assumed zero."
    )]
    pub wave_drag_onset_mach: f64,

    /// Leading constant of the Korn wave-drag rise.
    #[config(
        label = "Wave-drag rise coefficient",
        help = "Leading constant of the Lock/Korn wave-drag rise: CD_wave = coefficient * max(M - M_critical, 0)^4 (dimensionless). Must be positive.",
        long_help = "Leading constant in the Lock/Korn wave-drag rise: CD_wave = coefficient * max(M - M_critical, 0)^4. The critical Mach is M_drag_divergence - (0.1 / (4 * coefficient))^(1/3); coefficient must be positive."
    )]
    pub wave_drag_coefficient: f64,
}

/// Current version of the drag-model schema: 2 since the viscous margin is
/// unset by default and resolved by propulsion class. Files without it are
/// version 1 (margin always written, shipped 1.00).
pub const DRAG_MODEL_SCHEMA_VERSION: u32 = 2;

/// The version of a drag model that states none (see
/// [`DragModelConfig::schema_version`]).
#[must_use]
pub const fn legacy_drag_model_schema_version() -> u32 {
    1
}

/// Default lift-dependent viscous fraction `k_L`, per unit `CL^2`.
///
/// Jenkinson, *Civil Jet Aircraft Design* (1999), Ch. 8, pp. 179-180 (PDF p.
/// 194-195 of the library copy): the viscous part of the lift-dependent drag
/// is proportional to the profile drag, `dCD/dCL^2 = 0.35 CD0` for older
/// technology designs (B737-300, B757, B767) and `0.15 CD0` for advanced
/// technology designs (A330/A340, B777), the A320 lying in between. The
/// product has no technology-class input, so one global value is used: the
/// midpoint of the printed range, 0.25 (a rule on
/// the printed range, not a fit). It replaces Torenbeek's 0.43 (2013, p. 96,
/// Eq. 4.21, a whole-airplane en-route value). Applied to the clean polar
/// only; take-off and landing configurations keep their own drag increments.
pub const DEFAULT_LIFT_DEPENDENT_VISCOUS_FRACTION: f64 = 0.25;

/// Default viscous margin of a jet transport: allowance for leakage and
/// protuberance drag, which the component buildup omits.
///
/// `cd0 = viscous_margin * sum(Cf FF Q Swet / Sref)` over the wing, tail,
/// fuselage and nacelle components (`AeroAnalysis::parasite_breakdown`); the
/// lift-dependent viscous term is then `fraction * cd0 * CL^2`, so it carries
/// the margin too. Raymer, *Aircraft Design: A Conceptual Approach*, 6th ed.
/// (2018), Sec. 12.5.7 and Table 12.8, p. 431 (PDF p. 465 of the library
/// copy): leakage is drag from air "inhaled" and "exhaled" through holes and
/// gaps; protuberances are "antennas, lights, door edges, fuel vents, control
/// surface external hinges, actuator fairings, and such manufacturing defects
/// as protruding rivets and rough or misaligned skin panels". Their drag is
/// "estimated as a percentage of the total parasite drag", Table 12.8: jet
/// transports or bombers 2-5 %, propeller aircraft 5-10 %, non-stealth
/// fighters 10-15 %, stealth fighters 3-5 %. The default is the midpoint of
/// the printed row of the propulsion class (a rule on the printed range, not a
/// fit to any preset or result): 3.5 % for jets (this constant), 7.5 % for
/// propeller aircraft ([`DEFAULT_VISCOUS_MARGIN_PROPELLER`]). The margin does
/// NOT cover pylons, wing-body or flap-track fairings, aft-fuselage upsweep
/// or base drag: Raymer treats those as components or separate items (Eqs.
/// 12.36-12.38, p. 428); the product buildup adds the pylons, the wing-body
/// fairing, upsweep and base drag as their own terms
/// (`alas_aero::analysis::parasite_items`), flap-track fairings are not
/// drawn. Jenkinson, *Civil Jet Aircraft Design*
/// (1999), Ch. 8 (vii), p. 175, suggests a broader allowance for secondary
/// items (excrescence, surface imperfections, system installations: wing 6 %,
/// fuselage and empennage 7 %, engine installation 15 %, systems 3 %, "as high
/// as 10 %" of profile drag); it is on another base and scope and is not an
/// upper bound of this one. Raymer's percentage is of the total parasite drag,
/// which in his method also holds the miscellaneous items (upsweep, base
/// drag), so the multiplier applies to the components plus those items,
/// once. The
/// frozen reference configuration keeps 1.10. The value is resolved per
/// propulsion class when unset ([`DragModelConfig::resolved_for`]).
pub const DEFAULT_VISCOUS_MARGIN: f64 = 1.035;

/// Default viscous margin of the product build-ups: 1. A turbofan's is Poll
/// and Schumann's (*The Aeronautical Journal* 125, 2021, App. B), whose
/// interference and secondary-drag factors `FIFS` hold the excrescences,
/// leakage and protuberances, pylons, fairings and afterbody; a turboprop's
/// books the excrescence rows of Obert (*Aerodynamic Design of Transport
/// Aircraft*, 2009, Fig. 40.29) on its component sum. A margin on top would
/// count them twice. [`DEFAULT_VISCOUS_MARGIN`] and
/// [`DEFAULT_VISCOUS_MARGIN_PROPELLER`] stay Raymer's Table 12.8 rows for a
/// build-up without those items: the fallback of an unresolved model.
pub const DEFAULT_VISCOUS_MARGIN_ITEMISED: f64 = 1.0;

/// Default viscous margin of a propeller-driven aircraft: the midpoint of
/// Raymer Table 12.8, p. 431, "Propeller aircraft", 5-10 % (see
/// [`DEFAULT_VISCOUS_MARGIN`]).
pub const DEFAULT_VISCOUS_MARGIN_PROPELLER: f64 = 1.075;

impl Default for DragModelConfig {
    fn default() -> Self {
        Self {
            schema_version: DRAG_MODEL_SCHEMA_VERSION,
            exclude_buried_main_wing_area: true,
            max_thickness_chordwise_loc: 0.35,
            interference_factor_wing: 1.0,
            interference_factor_fuselage: 1.25,
            viscous_margin: None,
            lift_dependent_viscous_fraction: DEFAULT_LIFT_DEPENDENT_VISCOUS_FRACTION,
            interference_factor_nacelle: 1.3,
            wave_drag_onset_mach: 0.6,
            wave_drag_coefficient: 20.0,
        }
    }
}

impl DragModelConfig {
    /// The viscous margin in use: the stated value, else the jet-transport
    /// default. Analyses built from a full configuration receive it already
    /// resolved by the propulsion class ([`Self::resolved_for`]).
    #[must_use]
    pub fn margin(&self) -> f64 {
        self.viscous_margin.unwrap_or(DEFAULT_VISCOUS_MARGIN)
    }

    /// This model with the viscous margin settled for the propulsion class
    /// of the aircraft: a stated margin always wins, otherwise the class row
    /// of Raymer Table 12.8. The class is read where it is final (the
    /// analysis), not where a file or a preset is loaded, so an engine swap,
    /// an overlay, an import or a clean-sheet brief all follow it.
    #[must_use]
    pub fn resolved_for(&self, technology: crate::engines::PropulsionTechnology) -> Self {
        Self {
            viscous_margin: Some(
                self.viscous_margin
                    .unwrap_or_else(|| Self::default_viscous_margin(technology)),
            ),
            ..self.clone()
        }
    }

    /// The default viscous margin of a propulsion class: 1 for both, whose
    /// build-ups book those items themselves ([`DEFAULT_VISCOUS_MARGIN_ITEMISED`]).
    pub fn default_viscous_margin(technology: crate::engines::PropulsionTechnology) -> f64 {
        match technology {
            crate::engines::PropulsionTechnology::Turboprop
            | crate::engines::PropulsionTechnology::Turbofan => DEFAULT_VISCOUS_MARGIN_ITEMISED,
        }
    }
}

fn exclude_buried_main_wing_area_default() -> bool {
    true
}

fn is_zero_fraction(value: &f64) -> bool {
    *value == 0.0
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Entry, Kind};

    #[test]
    fn every_field_reaches_the_schema_in_declaration_order() {
        let schema = DragModelConfig::default().schema();
        let names: Vec<&str> = schema.fields.iter().map(|field| field.name).collect();
        assert_eq!(
            names,
            vec![
                "exclude_buried_main_wing_area",
                "max_thickness_chordwise_loc",
                "interference_factor_wing",
                "interference_factor_fuselage",
                "viscous_margin",
                "lift_dependent_viscous_fraction",
                "interference_factor_nacelle",
                "wave_drag_onset_mach",
                "wave_drag_coefficient",
            ]
        );
    }

    #[test]
    fn a_field_with_no_declared_unit_takes_one_from_its_name() {
        // None of these names end in a unit suffix, so all of them are
        // dimensionless, which is the point: the derived unit is the empty
        // string rather than a guess.
        let schema = DragModelConfig::default().schema();
        let viscous = schema.field("viscous_margin").unwrap();
        assert_eq!(viscous.unit, "");
        assert_eq!(viscous.label, "Viscous drag margin");
    }

    #[test]
    fn the_viscous_margin_default_follows_the_propulsion_class_rows_of_raymer_table_12_8() {
        // Midpoints of the printed rows: jet transports 2-5 %, propeller 5-10 %.
        assert_eq!(DEFAULT_VISCOUS_MARGIN, 1.0 + (0.02 + 0.05) / 2.0);
        assert_eq!(DEFAULT_VISCOUS_MARGIN_PROPELLER, 1.0 + (0.05 + 0.10) / 2.0);
        // The class is read where it is final: every way a configuration is made.
        let margin_of = |document: serde_json::Value| {
            crate::AlasConfig::from_value(&document)
                .unwrap()
                .resolved_drag_model()
                .margin()
        };
        let jet = DEFAULT_VISCOUS_MARGIN_ITEMISED;
        let propeller = DEFAULT_VISCOUS_MARGIN_ITEMISED;
        // Registered aircraft follow their engine class.
        assert_eq!(margin_of(serde_json::json!({ "preset": "A320-200" })), jet);
        assert_eq!(
            margin_of(serde_json::json!({ "preset": "ATR72-600" })),
            propeller
        );
        assert_eq!(
            margin_of(serde_json::json!({ "preset": "A400M" })),
            propeller
        );
        // An engine swapped by the file overlay (the sandbox does the same through
        // the GUI) follows the new class, in both directions.
        assert_eq!(
            margin_of(serde_json::json!({
                "preset": "A320-200",
                "geometry": { "engine": { "propulsion_technology": "turboprop", "engine_name": "PW127M" } }
            })),
            propeller
        );
        assert_eq!(
            margin_of(serde_json::json!({
                "preset": "ATR72-600",
                "geometry": { "engine": { "propulsion_technology": "turbofan", "engine_name": "CFM56-5B4" } }
            })),
            jet
        );
        // A preset-less brief (clean sheet or not) follows its engine class.
        let brief = |engine: &str| {
            serde_json::json!({
                "requirements": { "mtow_kg": 23_000.0, "num_passengers": 70, "cruise_mach": 0.45, "cruise_altitude_m": 7_000.0 },
                "geometry": { "engine": { "engine_name": engine } }
            })
        };
        assert_eq!(margin_of(brief("PW127M")), propeller);
        assert_eq!(margin_of(brief("CFM56-5B4")), jet);
        // A stated margin always wins, whatever the class.
        for preset in ["A320-200", "ATR72-600"] {
            assert_eq!(
                margin_of(
                    serde_json::json!({ "preset": preset, "drag_model": { "viscous_margin": 1.02 } })
                ),
                1.02
            );
        }
    }

    #[test]
    fn a_workspace_saved_before_the_class_default_does_not_keep_the_retired_fitted_margin() {
        // Builds before the sourced class default always wrote the margin and
        // shipped 1.00 (v1.4.x with fraction 0.25); they wrote no drag-model
        // schema version, every later build does.
        let old = |preset: &str, margin: f64, fraction: f64| {
            serde_json::json!({
                "preset": preset,
                "drag_model": { "viscous_margin": margin, "lift_dependent_viscous_fraction": fraction }
            })
        };
        let load = |document: serde_json::Value| {
            let (config, notes) = crate::AlasConfig::from_value_with_notes(&document).unwrap();
            (
                config.resolved_drag_model().margin(),
                notes.retired_keys.fitted_viscous_margin,
            )
        };
        // Unmarked, whatever the fraction: reset to the class default, with a note.
        assert_eq!(
            load(old("ATR72-600", 1.0, 0.25)),
            (DEFAULT_VISCOUS_MARGIN_ITEMISED, true)
        );
        assert_eq!(
            load(old("A320-200", 1.0, 0.25)),
            (DEFAULT_VISCOUS_MARGIN_ITEMISED, true)
        );
        assert_eq!(
            load(old("ATR72-600", 1.0, 0.30)),
            (DEFAULT_VISCOUS_MARGIN_ITEMISED, true)
        );
        // An explicit version 1 (a direct serde round trip of a legacy drag model) is
        // as unmarked as an absent one; version 2 keeps the value, also in a hand-written
        // partial overlay.
        let marked = |preset: &str, margin: f64, version: u32| {
            serde_json::json!({
                "preset": preset,
                "drag_model": { "schema_version": version, "viscous_margin": margin }
            })
        };
        assert_eq!(
            load(marked("A320-200", 1.0, 1)),
            (DEFAULT_VISCOUS_MARGIN_ITEMISED, true)
        );
        assert_eq!(
            load(marked("ATR72-600", 1.10, 1)),
            (DEFAULT_VISCOUS_MARGIN_ITEMISED, true)
        );
        assert_eq!(load(marked("A320-200", 1.0, 2)), (1.0, false));
        assert_eq!(load(marked("ATR72-600", 1.10, 2)), (1.10, false));
        // Another stated value stays.
        assert_eq!(load(old("ATR72-600", 1.02, 0.25)), (1.02, false));
        // The v1.3 default pair (1.10, no fraction) is the same stale default.
        assert_eq!(
            load(old("A320-200", 1.10, 0.0)),
            (DEFAULT_VISCOUS_MARGIN_ITEMISED, true)
        );
        assert_eq!(
            load(old("ATR72-600", 1.10, 0.0)),
            (DEFAULT_VISCOUS_MARGIN_ITEMISED, true)
        );
        let v13 = |preset: &str| serde_json::json!({ "preset": preset, "drag_model": { "viscous_margin": 1.10 } });
        assert_eq!(
            load(v13("A320-200")),
            (DEFAULT_VISCOUS_MARGIN_ITEMISED, true)
        );
        // 1.10 with a non-zero fraction is a stated value.
        assert_eq!(load(old("A320-200", 1.10, 0.25)), (1.10, false));
        // A document that never carried the margin is unchanged.
        assert_eq!(
            load(serde_json::json!({ "preset": "A320-200" })),
            (DEFAULT_VISCOUS_MARGIN_ITEMISED, false)
        );
        let note = crate::RetiredKeysDropped {
            fitted_viscous_margin: true,
            ..Default::default()
        }
        .messages();
        assert!(note[0].contains("1.00"));
    }

    #[test]
    fn a_stated_margin_of_one_survives_a_save_and_load_since_the_schema_marker() {
        // The same 1.00, stated on a current build, is a stated value: the
        // saved document carries the schema version and is never migrated.
        let mut config =
            crate::AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).unwrap();
        config.drag_model.viscous_margin = Some(1.0);
        let saved = serde_json::to_value(&config).unwrap();
        assert_eq!(
            saved["drag_model"]["schema_version"],
            DRAG_MODEL_SCHEMA_VERSION
        );
        assert_eq!(saved["drag_model"]["viscous_margin"], 1.0);
        let mut saved = saved;
        for _ in 0..2 {
            let (loaded, notes) = crate::AlasConfig::from_value_with_notes(&saved).unwrap();
            assert_eq!(loaded.drag_model.viscous_margin, Some(1.0));
            assert!(!notes.retired_keys.fitted_viscous_margin);
            // Re-save the loaded configuration: the marker and the value persist.
            saved = serde_json::to_value(&loaded).unwrap();
            assert_eq!(
                saved["drag_model"]["schema_version"],
                DRAG_MODEL_SCHEMA_VERSION
            );
            assert_eq!(saved["drag_model"]["viscous_margin"], 1.0);
        }
        // The v1.3 default stated on a current build is a stated value too.
        config.drag_model.viscous_margin = Some(1.10);
        let saved = serde_json::to_value(&config).unwrap();
        let (loaded, notes) = crate::AlasConfig::from_value_with_notes(&saved).unwrap();
        assert_eq!(loaded.drag_model.viscous_margin, Some(1.10));
        assert!(!notes.retired_keys.fitted_viscous_margin);
        // Left unset it stays unset, and resolves per class.
        let unset =
            crate::AlasConfig::from_value(&serde_json::json!({ "preset": "ATR72-600" })).unwrap();
        let saved = serde_json::to_value(&unset).unwrap();
        let loaded = crate::AlasConfig::from_value(&saved).unwrap();
        assert_eq!(loaded.drag_model.viscous_margin, None);
        assert_eq!(
            loaded.resolved_drag_model().margin(),
            DEFAULT_VISCOUS_MARGIN_ITEMISED
        );
    }

    #[test]
    fn the_default_fraction_is_the_midpoint_of_jenkinsons_range_and_zero_is_not_serialized() {
        // Jenkinson (1999) p. 180: 0.15 CD0 (advanced) to 0.35 CD0 (older).
        assert_eq!(DEFAULT_LIFT_DEPENDENT_VISCOUS_FRACTION, (0.15 + 0.35) / 2.0);
        let defaults = DragModelConfig::default();
        assert_eq!(defaults.lift_dependent_viscous_fraction, 0.25);
        // Unset: the class default is resolved at analysis time.
        assert_eq!(defaults.viscous_margin, None);
        assert_eq!(defaults.margin(), DEFAULT_VISCOUS_MARGIN);
        let value = serde_json::to_value(&defaults).unwrap();
        assert_eq!(value["lift_dependent_viscous_fraction"], 0.25);
        // A saved configuration that carries no fraction keeps the frozen
        // lumped-margin convention (fraction 0), and zero is not written back.
        let off = DragModelConfig {
            lift_dependent_viscous_fraction: 0.0,
            ..Default::default()
        };
        let value = serde_json::to_value(off).unwrap();
        assert!(value.get("lift_dependent_viscous_fraction").is_none());
        let mut legacy = serde_json::to_value(DragModelConfig::default()).unwrap();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("lift_dependent_viscous_fraction");
        let loaded: DragModelConfig = serde_json::from_value(legacy).unwrap();
        assert_eq!(loaded.lift_dependent_viscous_fraction, 0.0);
    }

    #[test]
    fn a_declared_unit_survives_into_the_schema() {
        let schema = DragModelConfig::default().schema();
        assert_eq!(
            schema.field("max_thickness_chordwise_loc").unwrap().unit,
            "x/c"
        );
    }

    #[test]
    fn the_schema_carries_the_current_values_not_the_defaults() {
        let config = DragModelConfig {
            viscous_margin: Some(1.5),
            ..Default::default()
        };
        let schema = config.schema();
        match &schema.field("viscous_margin").unwrap().entry {
            Entry::Leaf(leaf) => {
                assert_eq!(leaf.kind, Kind::Float);
                assert_eq!(leaf.value, serde_json::json!(1.5));
            }
            Entry::Node(_) => panic!("a real number is not a group"),
        }
    }
}
