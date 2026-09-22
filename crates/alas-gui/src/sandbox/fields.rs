// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The sandbox's inventory of independent geometry parameters.
//!
//! Rendered geometry is driven by two metadata systems: the configuration
//! schema (`geometry.wing`, `geometry.empennage`, `geometry.fuselage` and the
//! engine installation) and the sixteen-variable design vector. This module
//! joins them into one list of editable fields with a shared discipline
//! grouping, so the Parameter Panel, the Discipline Windows and the direct
//! manipulation handles all address the same parameters the same way.
//!
//! Labels, help text and units come from the schema or the design-variable
//! table; the editing range and decimals are the sandbox validity domain
//! declared here, since the shared schema carries no bounds for geometry.
//! Values are read from and written to the same JSON edit buffer and design
//! map the guided workspace uses, so nothing is copied.

use std::collections::BTreeMap;

use alas_config::{presets, ConfigNode, EngineConfig, Entry, Kind, Node, DESIGN_VARIABLE_SPECS};
use serde_json::Value;

/// The name of the reference aircraft every sandbox starts from.
pub const REFERENCE_PRESET: &str = "AVE";

/// A component discipline the Parameter Panel groups fields by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Discipline {
    /// The main wing.
    Wing,
    /// The horizontal stabilizer.
    HorizontalTail,
    /// The vertical stabilizer.
    VerticalTail,
    /// The fuselage body.
    Fuselage,
    /// Engines and nacelles.
    Propulsion,
}

impl Discipline {
    /// Every discipline, in panel order.
    pub const ALL: [Discipline; 5] = [
        Self::Wing,
        Self::HorizontalTail,
        Self::VerticalTail,
        Self::Fuselage,
        Self::Propulsion,
    ];

    /// The English panel title.
    pub fn title(self) -> &'static str {
        match self {
            Self::Wing => "Wing",
            Self::HorizontalTail => "Horizontal tail",
            Self::VerticalTail => "Vertical tail",
            Self::Fuselage => "Fuselage",
            Self::Propulsion => "Propulsion",
        }
    }

    /// Stable identifier for persistence and widget ids.
    pub fn id(self) -> &'static str {
        match self {
            Self::Wing => "wing",
            Self::HorizontalTail => "horizontal_tail",
            Self::VerticalTail => "vertical_tail",
            Self::Fuselage => "fuselage",
            Self::Propulsion => "propulsion",
        }
    }
}

/// Where a field's value lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldTarget {
    /// A JSON pointer into the configuration edit buffer.
    Config(&'static str),
    /// A named design-vector variable.
    Design(&'static str),
}

/// The editor a field needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// A bounded real number.
    Float,
    /// A bounded integer.
    Int,
    /// A real number that may be unset.
    OptionalFloat,
    /// An airfoil name from the library.
    Airfoil,
    /// A catalogue engine name.
    Engine,
    /// Three real coordinates in metres.
    Vec3,
    /// A list of real numbers.
    FloatList,
    /// A list of `(x, r)` pairs.
    PairList,
}

/// One independent geometry parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct SandboxField {
    /// Stable identifier (`design.span_m`, `geometry.wing.root_z_m`).
    pub id: String,
    /// English label from the shared metadata.
    pub label: String,
    /// English help from the shared metadata.
    pub help: String,
    /// Unit as declared by the shared metadata.
    pub unit: String,
    /// The discipline the field belongs to.
    pub discipline: Discipline,
    /// The subgroup title inside the discipline.
    pub group: &'static str,
    /// Where the value lives.
    pub target: FieldTarget,
    /// The editor kind.
    pub kind: FieldKind,
    /// Lower bound of the sandbox validity domain.
    pub min: f64,
    /// Upper bound of the sandbox validity domain.
    pub max: f64,
    /// Displayed decimals.
    pub decimals: usize,
    /// Fields whose meaning depends on this one; a group reset re-checks them.
    pub dependents: &'static [&'static str],
}

struct Spec {
    id: &'static str,
    discipline: Discipline,
    group: &'static str,
    target: FieldTarget,
    kind: FieldKind,
    min: f64,
    max: f64,
    decimals: usize,
    dependents: &'static [&'static str],
}

/// The sandbox validity domain for a field: its editing range and displayed
/// decimals. Grouped because every bounded config field declares all three
/// together (see the module doc comment).
struct Bounds {
    min: f64,
    max: f64,
    decimals: usize,
}

const fn cfg(
    id: &'static str,
    discipline: Discipline,
    group: &'static str,
    pointer: &'static str,
    kind: FieldKind,
    bounds: Bounds,
) -> Spec {
    Spec {
        id,
        discipline,
        group,
        target: FieldTarget::Config(pointer),
        kind,
        min: bounds.min,
        max: bounds.max,
        decimals: bounds.decimals,
        dependents: &[],
    }
}

const fn dv(
    id: &'static str,
    name: &'static str,
    discipline: Discipline,
    group: &'static str,
) -> Spec {
    Spec {
        id,
        discipline,
        group,
        target: FieldTarget::Design(name),
        kind: FieldKind::Float,
        min: 0.0,
        max: 0.0,
        decimals: 0,
        dependents: &[],
    }
}

const PLANFORM: &str = "Planform";
const TWIST: &str = "Twist and dihedral";
const PLACEMENT: &str = "Placement";
const AIRFOILS: &str = "Airfoils";
const MESH: &str = "Mesh";
const BODY: &str = "Body";
const STATIONS: &str = "Stations";
const PROFILE: &str = "Profile";
const INSTALLATION: &str = "Installation";
const NACELLE: &str = "Nacelle";

const SPECS: &[Spec] = &[
    // Wing planform: the design vector owns the chords, span and sweep.
    dv("design.span_m", "span_m", Discipline::Wing, PLANFORM),
    dv(
        "design.root_chord_m",
        "root_chord_m",
        Discipline::Wing,
        PLANFORM,
    ),
    dv(
        "design.break_chord_m",
        "break_chord_m",
        Discipline::Wing,
        PLANFORM,
    ),
    dv(
        "design.tip_chord_m",
        "tip_chord_m",
        Discipline::Wing,
        PLANFORM,
    ),
    Spec {
        dependents: &["geometry.wing.outboard_le_sweep_deg"],
        ..dv("design.sweep_deg", "sweep_deg", Discipline::Wing, PLANFORM)
    },
    Spec {
        dependents: &["geometry.wing.kink_span_fraction"],
        ..cfg(
            "geometry.wing.break_span_fraction",
            Discipline::Wing,
            PLANFORM,
            "/geometry/wing/break_span_fraction",
            FieldKind::Float,
            Bounds {
                min: 0.05,
                max: 0.95,
                decimals: 3,
            },
        )
    },
    cfg(
        "geometry.wing.kink_span_fraction",
        Discipline::Wing,
        PLANFORM,
        "/geometry/wing/kink_span_fraction",
        FieldKind::OptionalFloat,
        Bounds {
            min: 0.05,
            max: 0.95,
            decimals: 3,
        },
    ),
    Spec {
        dependents: &["geometry.wing.side_of_body_chord_ratio"],
        ..cfg(
            "geometry.wing.side_of_body_span_fraction",
            Discipline::Wing,
            PLANFORM,
            "/geometry/wing/side_of_body_span_fraction",
            FieldKind::OptionalFloat,
            Bounds {
                min: 0.0,
                max: 0.9,
                decimals: 3,
            },
        )
    },
    cfg(
        "geometry.wing.side_of_body_chord_ratio",
        Discipline::Wing,
        PLANFORM,
        "/geometry/wing/side_of_body_chord_ratio",
        FieldKind::OptionalFloat,
        Bounds {
            min: 0.2,
            max: 1.5,
            decimals: 3,
        },
    ),
    cfg(
        "geometry.wing.outboard_sweep_decrement_deg",
        Discipline::Wing,
        PLANFORM,
        "/geometry/wing/outboard_sweep_decrement_deg",
        FieldKind::Float,
        Bounds {
            min: -20.0,
            max: 30.0,
            decimals: 2,
        },
    ),
    cfg(
        "geometry.wing.outboard_le_sweep_deg",
        Discipline::Wing,
        PLANFORM,
        "/geometry/wing/outboard_le_sweep_deg",
        FieldKind::OptionalFloat,
        Bounds {
            min: -10.0,
            max: 60.0,
            decimals: 2,
        },
    ),
    // Twist and dihedral.
    cfg(
        "geometry.wing.root_twist_deg",
        Discipline::Wing,
        TWIST,
        "/geometry/wing/root_twist_deg",
        FieldKind::Float,
        Bounds {
            min: -15.0,
            max: 15.0,
            decimals: 2,
        },
    ),
    cfg(
        "geometry.wing.break_twist_deg",
        Discipline::Wing,
        TWIST,
        "/geometry/wing/break_twist_deg",
        FieldKind::Float,
        Bounds {
            min: -15.0,
            max: 15.0,
            decimals: 2,
        },
    ),
    dv(
        "design.tip_twist_deg",
        "tip_twist_deg",
        Discipline::Wing,
        TWIST,
    ),
    cfg(
        "geometry.wing.root_z_m",
        Discipline::Wing,
        TWIST,
        "/geometry/wing/root_z_m",
        FieldKind::Float,
        Bounds {
            min: -15.0,
            max: 15.0,
            decimals: 2,
        },
    ),
    cfg(
        "geometry.wing.break_z_m",
        Discipline::Wing,
        TWIST,
        "/geometry/wing/break_z_m",
        FieldKind::Float,
        Bounds {
            min: -15.0,
            max: 15.0,
            decimals: 2,
        },
    ),
    cfg(
        "geometry.wing.tip_z_m",
        Discipline::Wing,
        TWIST,
        "/geometry/wing/tip_z_m",
        FieldKind::Float,
        Bounds {
            min: -15.0,
            max: 15.0,
            decimals: 2,
        },
    ),
    // Placement.
    cfg(
        "geometry.wing.root_datum_x_m",
        Discipline::Wing,
        PLACEMENT,
        "/geometry/wing/root_datum_x_m",
        FieldKind::Float,
        Bounds {
            min: 0.0,
            max: 150.0,
            decimals: 2,
        },
    ),
    dv(
        "design.wing_x_shift_m",
        "wing_x_shift_m",
        Discipline::Wing,
        PLACEMENT,
    ),
    // Airfoils.
    cfg(
        "geometry.wing.root_airfoil",
        Discipline::Wing,
        AIRFOILS,
        "/geometry/wing/root_airfoil",
        FieldKind::Airfoil,
        Bounds {
            min: 0.0,
            max: 0.0,
            decimals: 0,
        },
    ),
    cfg(
        "geometry.wing.tip_airfoil",
        Discipline::Wing,
        AIRFOILS,
        "/geometry/wing/tip_airfoil",
        FieldKind::Airfoil,
        Bounds {
            min: 0.0,
            max: 0.0,
            decimals: 0,
        },
    ),
    dv(
        "design.airfoil_thickness_scale",
        "airfoil_thickness_scale",
        Discipline::Wing,
        AIRFOILS,
    ),
    dv(
        "design.airfoil_camber_scale",
        "airfoil_camber_scale",
        Discipline::Wing,
        AIRFOILS,
    ),
    dv(
        "design.bump_upper_front",
        "bump_upper_front",
        Discipline::Wing,
        AIRFOILS,
    ),
    dv(
        "design.bump_upper_rear",
        "bump_upper_rear",
        Discipline::Wing,
        AIRFOILS,
    ),
    dv(
        "design.bump_lower_mid",
        "bump_lower_mid",
        Discipline::Wing,
        AIRFOILS,
    ),
    dv(
        "design.bump_lower_rear",
        "bump_lower_rear",
        Discipline::Wing,
        AIRFOILS,
    ),
    cfg(
        "geometry.wing.n_subdivisions",
        Discipline::Wing,
        MESH,
        "/geometry/wing/n_subdivisions",
        FieldKind::Int,
        Bounds {
            min: 1.0,
            max: 64.0,
            decimals: 0,
        },
    ),
    // Horizontal tail.
    cfg(
        "geometry.empennage.hstab_root_chord_m",
        Discipline::HorizontalTail,
        PLANFORM,
        "/geometry/empennage/hstab_root_chord_m",
        FieldKind::Float,
        Bounds {
            min: 0.2,
            max: 30.0,
            decimals: 2,
        },
    ),
    cfg(
        "geometry.empennage.hstab_tip_chord_m",
        Discipline::HorizontalTail,
        PLANFORM,
        "/geometry/empennage/hstab_tip_chord_m",
        FieldKind::Float,
        Bounds {
            min: 0.1,
            max: 20.0,
            decimals: 2,
        },
    ),
    cfg(
        "geometry.empennage.hstab_tip_le_m",
        Discipline::HorizontalTail,
        PLANFORM,
        "/geometry/empennage/hstab_tip_le_m",
        FieldKind::Vec3,
        Bounds {
            min: -20.0,
            max: 40.0,
            decimals: 2,
        },
    ),
    cfg(
        "geometry.empennage.hstab_root_twist_deg",
        Discipline::HorizontalTail,
        TWIST,
        "/geometry/empennage/hstab_root_twist_deg",
        FieldKind::Float,
        Bounds {
            min: -15.0,
            max: 15.0,
            decimals: 2,
        },
    ),
    cfg(
        "geometry.empennage.hstab_tip_twist_deg",
        Discipline::HorizontalTail,
        TWIST,
        "/geometry/empennage/hstab_tip_twist_deg",
        FieldKind::Float,
        Bounds {
            min: -15.0,
            max: 15.0,
            decimals: 2,
        },
    ),
    cfg(
        "geometry.empennage.hstab_offset_from_tail_m",
        Discipline::HorizontalTail,
        PLACEMENT,
        "/geometry/empennage/hstab_offset_from_tail_m",
        FieldKind::Float,
        Bounds {
            min: 0.0,
            max: 60.0,
            decimals: 2,
        },
    ),
    cfg(
        "geometry.empennage.hstab_z_m",
        Discipline::HorizontalTail,
        PLACEMENT,
        "/geometry/empennage/hstab_z_m",
        FieldKind::Float,
        Bounds {
            min: -10.0,
            max: 20.0,
            decimals: 2,
        },
    ),
    dv(
        "design.tail_scale",
        "tail_scale",
        Discipline::HorizontalTail,
        PLACEMENT,
    ),
    dv(
        "design.tail_x_shift_m",
        "tail_x_shift_m",
        Discipline::HorizontalTail,
        PLACEMENT,
    ),
    cfg(
        "geometry.empennage.tail_airfoil",
        Discipline::HorizontalTail,
        AIRFOILS,
        "/geometry/empennage/tail_airfoil",
        FieldKind::Airfoil,
        Bounds {
            min: 0.0,
            max: 0.0,
            decimals: 0,
        },
    ),
    cfg(
        "geometry.empennage.n_subdivisions",
        Discipline::HorizontalTail,
        MESH,
        "/geometry/empennage/n_subdivisions",
        FieldKind::Int,
        Bounds {
            min: 1.0,
            max: 64.0,
            decimals: 0,
        },
    ),
    // Vertical tail.
    cfg(
        "geometry.empennage.vstab_root_chord_m",
        Discipline::VerticalTail,
        PLANFORM,
        "/geometry/empennage/vstab_root_chord_m",
        FieldKind::Float,
        Bounds {
            min: 0.2,
            max: 30.0,
            decimals: 2,
        },
    ),
    cfg(
        "geometry.empennage.vstab_tip_chord_m",
        Discipline::VerticalTail,
        PLANFORM,
        "/geometry/empennage/vstab_tip_chord_m",
        FieldKind::Float,
        Bounds {
            min: 0.1,
            max: 20.0,
            decimals: 2,
        },
    ),
    cfg(
        "geometry.empennage.vstab_tip_le_m",
        Discipline::VerticalTail,
        PLANFORM,
        "/geometry/empennage/vstab_tip_le_m",
        FieldKind::Vec3,
        Bounds {
            min: -20.0,
            max: 40.0,
            decimals: 2,
        },
    ),
    cfg(
        "geometry.empennage.vstab_offset_from_tail_m",
        Discipline::VerticalTail,
        PLACEMENT,
        "/geometry/empennage/vstab_offset_from_tail_m",
        FieldKind::Float,
        Bounds {
            min: 0.0,
            max: 60.0,
            decimals: 2,
        },
    ),
    cfg(
        "geometry.empennage.vstab_z_m",
        Discipline::VerticalTail,
        PLACEMENT,
        "/geometry/empennage/vstab_z_m",
        FieldKind::Float,
        Bounds {
            min: -5.0,
            max: 20.0,
            decimals: 2,
        },
    ),
    // Fuselage.
    dv(
        "design.fuselage_length_m",
        "fuselage_length_m",
        Discipline::Fuselage,
        BODY,
    ),
    Spec {
        dependents: &["geometry.fuselage.height_m"],
        ..cfg(
            "geometry.fuselage.diameter_m",
            Discipline::Fuselage,
            BODY,
            "/geometry/fuselage/diameter_m",
            FieldKind::Float,
            Bounds {
                min: 0.5,
                max: 12.0,
                decimals: 3,
            },
        )
    },
    cfg(
        "geometry.fuselage.height_m",
        Discipline::Fuselage,
        BODY,
        "/geometry/fuselage/height_m",
        FieldKind::OptionalFloat,
        Bounds {
            min: 0.5,
            max: 14.0,
            decimals: 3,
        },
    ),
    cfg(
        "geometry.fuselage.cabin_start_x_m",
        Discipline::Fuselage,
        STATIONS,
        "/geometry/fuselage/cabin_start_x_m",
        FieldKind::Float,
        Bounds {
            min: 0.5,
            max: 40.0,
            decimals: 2,
        },
    ),
    cfg(
        "geometry.fuselage.tailcone_length_m",
        Discipline::Fuselage,
        STATIONS,
        "/geometry/fuselage/tailcone_length_m",
        FieldKind::Float,
        Bounds {
            min: 1.0,
            max: 50.0,
            decimals: 2,
        },
    ),
    cfg(
        "geometry.fuselage.nose_z_m",
        Discipline::Fuselage,
        PROFILE,
        "/geometry/fuselage/nose_z_m",
        FieldKind::Float,
        Bounds {
            min: -5.0,
            max: 5.0,
            decimals: 2,
        },
    ),
    cfg(
        "geometry.fuselage.cabin_z_m",
        Discipline::Fuselage,
        PROFILE,
        "/geometry/fuselage/cabin_z_m",
        FieldKind::Float,
        Bounds {
            min: -5.0,
            max: 5.0,
            decimals: 2,
        },
    ),
    cfg(
        "geometry.fuselage.tail_z_m",
        Discipline::Fuselage,
        PROFILE,
        "/geometry/fuselage/tail_z_m",
        FieldKind::Float,
        Bounds {
            min: -5.0,
            max: 10.0,
            decimals: 2,
        },
    ),
    cfg(
        "geometry.fuselage.n_subdivisions",
        Discipline::Fuselage,
        MESH,
        "/geometry/fuselage/n_subdivisions",
        FieldKind::Int,
        Bounds {
            min: 1.0,
            max: 64.0,
            decimals: 0,
        },
    ),
    // Propulsion installation.
    cfg(
        "geometry.engine.engine_name",
        Discipline::Propulsion,
        INSTALLATION,
        "/geometry/engine/engine_name",
        FieldKind::Engine,
        Bounds {
            min: 0.0,
            max: 0.0,
            decimals: 0,
        },
    ),
    cfg(
        "geometry.engine.spanwise_positions_m",
        Discipline::Propulsion,
        INSTALLATION,
        "/geometry/engine/spanwise_positions_m",
        FieldKind::FloatList,
        Bounds {
            min: -45.0,
            max: 45.0,
            decimals: 2,
        },
    ),
    cfg(
        "geometry.engine.z_m",
        Discipline::Propulsion,
        INSTALLATION,
        "/geometry/engine/z_m",
        FieldKind::Float,
        Bounds {
            min: -10.0,
            max: 10.0,
            decimals: 2,
        },
    ),
    cfg(
        "geometry.engine.inlet_x_offset_m",
        Discipline::Propulsion,
        INSTALLATION,
        "/geometry/engine/inlet_x_offset_m",
        FieldKind::Float,
        Bounds {
            min: -10.0,
            max: 15.0,
            decimals: 2,
        },
    ),
    cfg(
        "geometry.engine.radius_scale_m",
        Discipline::Propulsion,
        NACELLE,
        "/geometry/engine/radius_scale_m",
        FieldKind::Float,
        Bounds {
            min: 0.2,
            max: 4.0,
            decimals: 3,
        },
    ),
    cfg(
        "geometry.engine.nacelle_profile",
        Discipline::Propulsion,
        NACELLE,
        "/geometry/engine/nacelle_profile",
        FieldKind::PairList,
        Bounds {
            min: 0.0,
            max: 20.0,
            decimals: 3,
        },
    ),
];

/// The geometry schema leaves the sandbox must cover: every leaf of the
/// wing, empennage and fuselage groups plus the engine installation fields.
pub const ENGINE_INSTALLATION_FIELDS: [&str; 6] = [
    "engine_name",
    "nacelle_profile",
    "radius_scale_m",
    "spanwise_positions_m",
    "z_m",
    "inlet_x_offset_m",
];

fn schema_leaf<'a>(node: &'a Node, path: &[&str]) -> Option<&'a alas_config::Field> {
    let (first, rest) = path.split_first()?;
    let field = node.fields.iter().find(|f| f.name == *first)?;
    if rest.is_empty() {
        return Some(field);
    }
    match &field.entry {
        Entry::Node(child) => schema_leaf(child, rest),
        Entry::Leaf(_) => None,
    }
}

/// Build the inventory from the shared schema metadata.
pub fn inventory(schema: &Node) -> Vec<SandboxField> {
    let engine_schema = EngineConfig::default().schema();
    SPECS
        .iter()
        .map(|spec| {
            let (label, help, unit) = match &spec.target {
                FieldTarget::Design(name) => {
                    let variable = DESIGN_VARIABLE_SPECS.iter().find(|v| v.name == *name);
                    let label = variable
                        .map(|v| design_variable_label(v.name, v.unit))
                        .unwrap_or_else(|| name.to_string());
                    (
                        label,
                        variable
                            .map(|v| v.description.to_owned())
                            .unwrap_or_default(),
                        variable.map(|v| v.unit.to_owned()).unwrap_or_default(),
                    )
                }
                FieldTarget::Config(pointer) => {
                    let segments: Vec<&str> = pointer.trim_start_matches('/').split('/').collect();
                    let field = if segments.get(1) == Some(&"engine") {
                        schema_leaf(&engine_schema, &segments[2..])
                    } else {
                        schema_leaf(schema, &segments)
                    };
                    (
                        field
                            .map(|f| f.label.to_owned())
                            .unwrap_or_else(|| spec.id.to_owned()),
                        field.map(|f| f.help.to_owned()).unwrap_or_default(),
                        field.map(|f| f.unit.to_owned()).unwrap_or_default(),
                    )
                }
            };
            let (min, max, decimals) = match &spec.target {
                FieldTarget::Design(name) => DESIGN_VARIABLE_SPECS
                    .iter()
                    .find(|v| v.name == *name)
                    .map(|v| (v.preset_lower, v.preset_upper, v.decimals.max(0) as usize))
                    .unwrap_or((spec.min, spec.max, spec.decimals)),
                FieldTarget::Config(_) => (spec.min, spec.max, spec.decimals),
            };
            SandboxField {
                id: spec.id.to_owned(),
                label,
                help,
                unit,
                discipline: spec.discipline,
                group: spec.group,
                target: spec.target.clone(),
                kind: spec.kind,
                min,
                max,
                decimals,
                dependents: spec.dependents,
            }
        })
        .collect()
}

/// A readable label for a design variable, in the style the Design Space
/// page uses (`span_m` becomes `Span`).
pub fn design_variable_label(name: &str, unit: &str) -> String {
    let stem = if unit == "m" {
        name.strip_suffix("_m").unwrap_or(name)
    } else if unit == "deg" {
        name.strip_suffix("_deg").unwrap_or(name)
    } else {
        name
    };
    stem.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            let first = chars
                .next()
                .map(|c| c.to_ascii_uppercase())
                .unwrap_or_default();
            format!("{first}{}", chars.as_str())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Read a field's current value.
pub fn read_value(
    field: &SandboxField,
    config_values: &Value,
    design_values: &BTreeMap<String, f64>,
) -> Value {
    match &field.target {
        FieldTarget::Config(pointer) => config_values
            .pointer(pointer)
            .cloned()
            .unwrap_or(Value::Null),
        FieldTarget::Design(name) => design_values
            .get(*name)
            .map(|v| Value::from(*v))
            .unwrap_or(Value::Null),
    }
}

/// Write a field's value into the edit buffers.
pub fn write_value(
    field: &SandboxField,
    value: Value,
    config_values: &mut Value,
    design_values: &mut BTreeMap<String, f64>,
) {
    match &field.target {
        FieldTarget::Config(pointer) => {
            if let Some(slot) = config_values.pointer_mut(pointer) {
                *slot = value;
            } else if let Some((parent, key)) = pointer.rsplit_once('/') {
                if let Some(map) = config_values
                    .pointer_mut(parent)
                    .and_then(Value::as_object_mut)
                {
                    map.insert(key.to_owned(), value);
                }
            }
        }
        FieldTarget::Design(name) => {
            if let Some(v) = value.as_f64() {
                design_values.insert((*name).to_owned(), v);
            }
        }
    }
}

/// The AVE reference values every reset returns to.
pub struct ReferenceValues {
    /// The AVE geometry group serialized like the edit buffer.
    pub geometry: Value,
    /// The AVE design vector by variable name.
    pub design: BTreeMap<String, f64>,
}

/// Load the AVE reference values.
pub fn reference_values() -> Option<ReferenceValues> {
    let preset = presets::get(REFERENCE_PRESET).ok()?;
    let mut geometry = preset.geometry.clone();
    geometry.engine.apply_engine_spec();
    let geometry = serde_json::to_value(geometry).ok()?;
    let design = serde_json::to_value(preset.design_vector)
        .ok()?
        .as_object()?
        .iter()
        .filter_map(|(k, v)| v.as_f64().map(|v| (k.clone(), v)))
        .collect();
    Some(ReferenceValues {
        geometry: serde_json::json!({ "geometry": geometry }),
        design,
    })
}

impl ReferenceValues {
    /// The reference value of one field.
    pub fn value_of(&self, field: &SandboxField) -> Value {
        read_value(field, &self.geometry, &self.design)
    }
}

/// Whether `value` lies inside a field's validity domain (or is a non-numeric
/// kind the domain does not apply to).
pub fn value_in_domain(field: &SandboxField, value: &Value) -> bool {
    match field.kind {
        FieldKind::Float | FieldKind::Int => value
            .as_f64()
            .is_some_and(|v| v.is_finite() && v >= field.min && v <= field.max),
        FieldKind::OptionalFloat => {
            value.is_null()
                || value
                    .as_f64()
                    .is_some_and(|v| v.is_finite() && v >= field.min && v <= field.max)
        }
        FieldKind::Vec3 | FieldKind::FloatList => value.as_array().is_some_and(|items| {
            items.iter().all(|item| {
                item.as_f64()
                    .is_some_and(|v| v.is_finite() && v >= field.min && v <= field.max)
            })
        }),
        FieldKind::PairList => value.as_array().is_some_and(|rows| {
            rows.iter().all(|row| {
                row.as_array().is_some_and(|pair| {
                    pair.len() == 2 && pair.iter().all(|v| v.as_f64().is_some_and(f64::is_finite))
                })
            })
        }),
        FieldKind::Airfoil | FieldKind::Engine => {
            value.as_str().is_some_and(|s| !s.trim().is_empty())
        }
    }
}

/// Fields of one discipline, grouped and in declaration order.
pub fn grouped(
    fields: &[SandboxField],
    discipline: Discipline,
) -> Vec<(&'static str, Vec<&SandboxField>)> {
    let mut groups: Vec<(&'static str, Vec<&SandboxField>)> = Vec::new();
    for field in fields.iter().filter(|f| f.discipline == discipline) {
        match groups.iter_mut().find(|(title, _)| *title == field.group) {
            Some((_, members)) => members.push(field),
            None => groups.push((field.group, vec![field])),
        }
    }
    groups
}

/// Whether the schema kind of a leaf is one the sandbox editors accept.
pub fn schema_kind_is_supported(kind: Kind) -> bool {
    !matches!(kind, Kind::Nested | Kind::Unsupported)
}

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::AlasConfig;

    #[test]
    fn every_spec_resolves_to_shared_metadata_with_a_unit_and_range() {
        let schema = AlasConfig::default().schema();
        let fields = inventory(&schema);
        assert_eq!(fields.len(), SPECS.len());
        for field in &fields {
            assert_ne!(field.label, field.id, "{} has no shared label", field.id);
            assert!(!field.help.is_empty(), "{} has no help", field.id);
            match field.kind {
                FieldKind::Float
                | FieldKind::Int
                | FieldKind::OptionalFloat
                | FieldKind::Vec3
                | FieldKind::FloatList => {
                    assert!(field.min < field.max, "{} has an empty range", field.id);
                    assert!(
                        !field.unit.is_empty()
                            || field.id.contains("scale")
                            || field.id.contains("bump")
                            || field.id.contains("fraction")
                            || field.id.contains("ratio")
                            || field.id.contains("n_subdivisions"),
                        "{} has no unit",
                        field.id
                    );
                }
                _ => {}
            }
        }
    }

    #[test]
    fn the_inventory_covers_every_geometry_leaf_and_design_variable() {
        let schema = AlasConfig::default().schema();
        let fields = inventory(&schema);
        let geometry = schema.field("geometry").expect("geometry group");
        let Entry::Node(geometry) = &geometry.entry else {
            panic!("geometry is a group")
        };
        let mut missing = Vec::new();
        for group in ["wing", "empennage", "fuselage"] {
            let Some(node) = geometry.fields.iter().find(|f| f.name == group) else {
                continue;
            };
            let Entry::Node(node) = &node.entry else {
                continue;
            };
            for leaf in &node.fields {
                let id = format!("geometry.{group}.{}", leaf.name);
                if !fields.iter().any(|f| f.id == id) {
                    missing.push(id);
                }
            }
        }
        for name in ENGINE_INSTALLATION_FIELDS {
            let id = format!("geometry.engine.{name}");
            if !fields.iter().any(|f| f.id == id) {
                missing.push(id);
            }
        }
        for variable in DESIGN_VARIABLE_SPECS {
            if !fields
                .iter()
                .any(|f| f.target == FieldTarget::Design(variable.name))
            {
                missing.push(format!("design.{}", variable.name));
            }
        }
        assert!(
            missing.is_empty(),
            "uncovered geometry parameters: {missing:?}"
        );
    }

    #[test]
    fn reference_values_read_back_through_the_same_targets() {
        let schema = AlasConfig::default().schema();
        let fields = inventory(&schema);
        let reference = reference_values().expect("AVE preset");
        let span = fields
            .iter()
            .find(|f| f.id == "design.span_m")
            .expect("span");
        assert_eq!(reference.value_of(span).as_f64(), Some(71.75));
        let root_x = fields
            .iter()
            .find(|f| f.id == "geometry.wing.root_datum_x_m")
            .expect("root datum");
        assert!(reference.value_of(root_x).as_f64().is_some());
        for field in &fields {
            assert!(
                value_in_domain(field, &reference.value_of(field)),
                "{} reference value {:?} outside its domain [{}, {}]",
                field.id,
                reference.value_of(field),
                field.min,
                field.max
            );
        }
    }

    #[test]
    fn writes_round_trip_through_both_targets() {
        let schema = AlasConfig::default().schema();
        let fields = inventory(&schema);
        let mut config_values = serde_json::to_value(AlasConfig::default()).expect("config");
        let mut design_values = BTreeMap::new();
        let span = fields
            .iter()
            .find(|f| f.id == "design.span_m")
            .expect("span");
        write_value(
            span,
            Value::from(66.0),
            &mut config_values,
            &mut design_values,
        );
        assert_eq!(
            read_value(span, &config_values, &design_values).as_f64(),
            Some(66.0)
        );
        let root_z = fields
            .iter()
            .find(|f| f.id == "geometry.wing.root_z_m")
            .expect("root z");
        write_value(
            root_z,
            Value::from(-1.5),
            &mut config_values,
            &mut design_values,
        );
        assert_eq!(
            read_value(root_z, &config_values, &design_values).as_f64(),
            Some(-1.5)
        );
        let kink = fields
            .iter()
            .find(|f| f.id == "geometry.wing.kink_span_fraction")
            .expect("kink");
        write_value(kink, Value::Null, &mut config_values, &mut design_values);
        assert!(read_value(kink, &config_values, &design_values).is_null());
    }

    #[test]
    fn grouping_keeps_declaration_order_and_dependents_are_known_fields() {
        let schema = AlasConfig::default().schema();
        let fields = inventory(&schema);
        let wing = grouped(&fields, Discipline::Wing);
        assert_eq!(wing[0].0, PLANFORM);
        assert!(wing.iter().any(|(title, _)| *title == AIRFOILS));
        for field in &fields {
            for dependent in field.dependents {
                assert!(
                    fields.iter().any(|f| f.id == *dependent),
                    "{dependent} unknown"
                );
            }
        }
    }
}
