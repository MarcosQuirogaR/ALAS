// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/sidecar/schema.py
// Reference: alas @ rust-port-baseline.

//! The description a configuration struct gives of its own fields.
//!
//! This is what the settings interface is rendered from, and it is produced
//! by `#[derive(ConfigNode)]` rather than written by hand. The shape follows
//! the reference implementation's: an ordered list of fields, each carrying
//! what to call it, what unit it is in, what it means, and either its current
//! value or -- when the field is itself a configuration group -- the same
//! description one level down.
//!
//! Two things the reference does at this boundary are deliberately not done
//! here. It translates every label and help string on the way out, because
//! that function is its HTTP boundary and the language belongs to the
//! request; here the schema carries the canonical English and
//! [`Node::translated`] applies a language when something is about to show it
//! to a person, which is the arrangement `alas-i18n` already documents. It
//! also resolves the accepted values of the handful of string fields that
//! have them by importing the module that owns each list; those modules sit
//! above this crate in the layering, so a field names its list through
//! [`OptionSource`] and something that can see both resolves it.

use serde::ser::{SerializeMap, Serializer};
use serde::Serialize;

/// One configuration struct's fields, in declaration order.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// The struct's name, as the interface titles the group.
    pub type_name: &'static str,
    /// Its fields, in the order they are declared.
    pub fields: Vec<Field>,
}

/// One field of a configuration struct.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// The field's identifier, which is also its key in a saved file.
    pub name: &'static str,
    /// What to call it, in English.
    pub label: &'static str,
    /// Its unit, or the empty string when it is dimensionless.
    pub unit: &'static str,
    /// What it means, in English.
    pub help: &'static str,
    /// Whether the interface hides it until advanced settings are revealed.
    pub advanced: bool,
    /// Its value, or its own fields when it is a group.
    pub entry: Entry,
}

/// What sits under a field: a nested group, or a value.
#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    /// A nested configuration group.
    Node(Node),
    /// A value the interface edits directly.
    Leaf(LeafField),
}

/// A value-bearing field, with whatever constraints it declared.
#[derive(Debug, Clone, PartialEq)]
pub struct LeafField {
    /// Which editor the interface should use.
    pub kind: Kind,
    /// The current value.
    pub value: serde_json::Value,
    /// The smallest accepted value, if the field states one.
    pub min: Option<Number>,
    /// The largest accepted value, if the field states one.
    pub max: Option<Number>,
    /// Decimal places to show, if the field states a preference.
    pub decimals: Option<u32>,
    /// Column headings, for a field holding a table of numbers.
    pub columns: Option<&'static [&'static str]>,
    /// The sibling field and value that make this one editable.
    pub readonly_unless: Option<ReadonlyUnless>,
    /// Where this field's accepted values come from.
    pub options: Option<OptionSource>,
}

/// A bound, which keeps whether it was written as an integer.
///
/// A seat count's minimum is `0` and a seat pitch's is `0.7112`; serializing
/// the first as `0.0` would make a whole-number field look like a fractional
/// one to whatever renders it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Number {
    /// A whole-number bound.
    Integer(i64),
    /// A fractional bound.
    Real(f64),
}

impl From<i64> for Number {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<f64> for Number {
    fn from(value: f64) -> Self {
        Self::Real(value)
    }
}

/// A field that is editable only while a sibling holds a particular value.
///
/// The sibling is named rather than referenced: the condition is resolved
/// against the values the form currently holds, which upstream resolves at
/// the nearest enclosing level rather than necessarily in the same struct.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ReadonlyUnless {
    /// The sibling field's name.
    pub field: &'static str,
    /// The value it must hold.
    pub value: &'static str,
}

/// Which editor a value gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A checkbox.
    Bool,
    /// A whole number.
    Int,
    /// A real number.
    Float,
    /// A real number the interface offers as a slider, because it is a
    /// relative weight rather than a physical quantity.
    WeightSlider,
    /// Text, possibly with a list of accepted values.
    Str,
    /// A value that is currently unset.
    Optional,
    /// A list of numbers.
    NumberList,
    /// A list of number pairs -- a small table.
    TupleList,
    /// A nested configuration group.
    ///
    /// Spelled as the reference spells it, because the parity fixture is its
    /// output and a renamed variant would mean the comparison is against a
    /// translation of that output rather than against it.
    #[serde(rename = "dataclass")]
    Nested,
    /// A value with no editor. Reproduces the reference's classification,
    /// which reaches this for an empty list because it tests the list's
    /// contents and an empty one has none.
    Unsupported,
}

/// Where a string field's accepted values come from.
///
/// Most of these lists are owned by crates that sit above this one -- the
/// airfoil library, the engine deck, the material database -- so this names
/// the list and something that can see both resolves it. The lists that
/// depend on nothing are resolved by [`OptionSource::options`] here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OptionSource {
    /// The airfoil library, which also accepts names it does not list: any
    /// NACA 4-digit code resolves without being in it.
    Airfoil,
    /// The engine registry.
    Engine,
    /// The material database.
    Material,
    /// The landing-gear strut materials, which are their own list and not
    /// the general material database.
    StrutMaterial,
    /// The tire database.
    TireClass,
    /// Which trailing-edge ribs the structural mesh generates.
    TeRibMode,
    /// The differential-evolution strategy names.
    Strategy,
    /// The top-level aircraft optimization algorithms.
    OptimizerMethod,
    /// Whether the aircraft carries passengers or freight.
    AircraftType,
    /// The cabin layout presets, which differ by aircraft type.
    CabinPreset,
    /// The one method that owns every production mass group.
    MassArchitecture,
    /// What kind of knowledge a family of declared FLOPS inputs rests on.
    FlopsInputEvidence,
    /// Versioned systems-and-equipment mass method.
    SystemsMassMethod,
    /// Versioned structural-group mass method.
    StructuralMassMethod,
    /// Versioned propulsion-group mass method.
    PropulsionMassMethod,
    /// Which FLOPS wing bending-material factor is evaluated.
    FlopsWingBendingMethod,
    /// The operating rule a design mission's reserves are sized under.
    FuelScheme,
    /// The scalar the mission-sized design search minimises.
    ObjectiveKind,
    /// Whether the takeoff mass is a fixed input, closed by the mission up
    /// to it, or closed by the mission with it used only to seed the first
    /// pass.
    MtowSizing,
    /// How a family of requirements takes part in the ranking.
    ConstraintPolicy,
    /// How the optimizer treats the aircraft geometry it starts from.
    DesignMode,
}

impl OptionSource {
    /// The accepted values, when this crate can name them.
    ///
    /// `None` means the list is owned elsewhere and has to be resolved by a
    /// crate that can see its owner.
    pub fn options(self) -> Option<&'static [&'static str]> {
        match self {
            Self::TeRibMode => Some(&["all", "none", "alternate", "inboard", "outboard"]),
            Self::Strategy => Some(&[
                "best1bin",
                "best1exp",
                "rand1bin",
                "rand1exp",
                "best2bin",
                "best2exp",
                "rand2bin",
                "rand2exp",
                "randtobest1bin",
                "randtobest1exp",
                "currenttobest1bin",
                "currenttobest1exp",
            ]),
            Self::OptimizerMethod => Some(&[
                "differential_evolution",
                "feasibility_first_de",
                "nsga2",
                "turbo_1",
                "cma_es",
                "sqp",
            ]),
            Self::AircraftType => Some(&["passenger", "cargo"]),
            Self::MassArchitecture => Some(&[
                "pure_flops_transport_v1",
                "legacy_reference_compatible_comparison",
            ]),
            Self::FlopsInputEvidence => Some(&[
                "source_backed",
                "user_declared",
                "published_flops_default",
                "uncertain_engineering_estimate",
            ]),
            Self::SystemsMassMethod => {
                Some(&["reference_compatible_fractions", "flops_transport_v1"])
            }
            Self::StructuralMassMethod | Self::PropulsionMassMethod => {
                Some(&["reference_compatible", "flops_transport_v1"])
            }
            Self::FlopsWingBendingMethod => Some(&["simplified", "detailed"]),
            Self::FuelScheme => Some(&[
                "easa_basic",
                "faa_domestic",
                "faa_flag_supplemental",
                "study_convention",
                "trip_fuel_only",
            ]),
            Self::ObjectiveKind => Some(&[
                "block_fuel",
                "takeoff_mass",
                "operating_empty_mass",
                "fuel_per_seat_kilometre",
            ]),
            Self::MtowSizing => Some(&["fixed_requirement", "sized_by_mission", "unconstrained"]),
            Self::ConstraintPolicy => Some(&["hard", "soft", "diagnostic", "off"]),
            Self::DesignMode => Some(&["clean_sheet", "reference_adaptation", "baseline_sandbox"]),
            _ => None,
        }
    }

    /// Whether a value outside the list is still accepted.
    ///
    /// Only the airfoil field is: the geometry layer resolves names the
    /// library does not carry, so a strict list would reject valid input.
    /// Everywhere else the list is the valid set, and free text could only
    /// produce a lookup failure later.
    pub fn editable(self) -> bool {
        self == Self::Airfoil
    }
}

impl Node {
    /// This node with every label and help string looked up in `lang`.
    ///
    /// Untranslated strings stay as they are, so a partial catalog degrades
    /// to English rather than to a missing-key placeholder. Returns owned
    /// strings, since a translation is not a `&'static str`.
    pub fn translated(&self, lang: Option<&str>) -> TranslatedNode {
        TranslatedNode {
            type_name: self.type_name,
            fields: self
                .fields
                .iter()
                .map(|field| TranslatedField {
                    name: field.name,
                    label: alas_i18n::t(Some(field.label), lang).into_owned(),
                    unit: field.unit,
                    help: alas_i18n::t(Some(field.help), lang).into_owned(),
                    advanced: field.advanced,
                    entry: match &field.entry {
                        Entry::Node(node) => TranslatedEntry::Node(node.translated(lang)),
                        Entry::Leaf(leaf) => TranslatedEntry::Leaf(leaf.clone()),
                    },
                })
                .collect(),
        }
    }

    /// The field of this node with the given name, if it has one.
    pub fn field(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|field| field.name == name)
    }
}

/// A [`Node`] whose prose has been looked up in a language.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TranslatedNode {
    /// The struct's name.
    pub type_name: &'static str,
    /// Its fields, in declaration order.
    pub fields: Vec<TranslatedField>,
}

/// A [`Field`] whose prose has been looked up in a language.
#[derive(Debug, Clone, PartialEq)]
pub struct TranslatedField {
    /// The field's identifier.
    pub name: &'static str,
    /// What to call it, in the active language.
    pub label: String,
    /// Its unit.
    pub unit: &'static str,
    /// What it means, in the active language.
    pub help: String,
    /// Whether it is hidden until advanced settings are revealed.
    pub advanced: bool,
    /// Its value, or its own fields.
    pub entry: TranslatedEntry,
}

/// What sits under a [`TranslatedField`].
#[derive(Debug, Clone, PartialEq)]
pub enum TranslatedEntry {
    /// A nested group.
    Node(TranslatedNode),
    /// A value.
    Leaf(LeafField),
}

impl Serialize for Node {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("type", self.type_name)?;
        map.serialize_entry("fields", &self.fields)?;
        map.end()
    }
}

impl Serialize for Field {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("name", self.name)?;
        map.serialize_entry("label", self.label)?;
        map.serialize_entry("unit", self.unit)?;
        map.serialize_entry("help", self.help)?;
        map.serialize_entry("advanced", &self.advanced)?;
        match &self.entry {
            Entry::Node(node) => {
                map.serialize_entry("kind", &Kind::Nested)?;
                map.serialize_entry("fields", &node.fields)?;
            }
            Entry::Leaf(leaf) => leaf.serialize_into(&mut map)?,
        }
        map.end()
    }
}

impl Serialize for TranslatedField {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("name", self.name)?;
        map.serialize_entry("label", &self.label)?;
        map.serialize_entry("unit", self.unit)?;
        map.serialize_entry("help", &self.help)?;
        map.serialize_entry("advanced", &self.advanced)?;
        match &self.entry {
            TranslatedEntry::Node(node) => {
                map.serialize_entry("kind", &Kind::Nested)?;
                map.serialize_entry("fields", &node.fields)?;
            }
            TranslatedEntry::Leaf(leaf) => leaf.serialize_into(&mut map)?,
        }
        map.end()
    }
}

impl LeafField {
    /// Write this value's keys into an already-open field map.
    ///
    /// A constraint the field did not declare is an absent key rather than a
    /// null, which is how the reference emits it: the interface tells "no
    /// minimum" from "a minimum of nothing" by the key's presence.
    fn serialize_into<M: SerializeMap>(&self, map: &mut M) -> Result<(), M::Error> {
        map.serialize_entry("kind", &self.kind)?;
        map.serialize_entry("value", &self.value)?;
        if let Some(min) = &self.min {
            map.serialize_entry("min", min)?;
        }
        if let Some(max) = &self.max {
            map.serialize_entry("max", max)?;
        }
        if let Some(decimals) = &self.decimals {
            map.serialize_entry("decimals", decimals)?;
        }
        if let Some(columns) = &self.columns {
            map.serialize_entry("columns", columns)?;
        }
        if let Some(readonly_unless) = &self.readonly_unless {
            map.serialize_entry("readonly_unless", readonly_unless)?;
        }
        if let Some(options) = &self.options {
            map.serialize_entry("options_from", options)?;
            map.serialize_entry("editable", &options.editable())?;
            if let Some(values) = options.options() {
                map.serialize_entry("options", values)?;
            }
        }
        Ok(())
    }
}
