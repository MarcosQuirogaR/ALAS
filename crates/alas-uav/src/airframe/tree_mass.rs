// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Bottom-up mass statement from the `UavSpec` component tree and the database.
//!
//! * `Part::Catalog` nodes take their mass (and box or cylinder dimensions)
//!   from the `alas-uav-db` row; a missing mass is an error, never a default,
//!   unless the node carries `mass_override_kg` (a weighed value).
//! * Structure of the wing, tails and boom comes from the geometry and the
//!   areal densities of [`StructureInputs`]; a `Part::Structure` node whose
//!   `role` is `wing`, `htail`, `vtail`, `boom` or `fuselage` replaces the
//!   estimate by `mass_override_kg` (CG stays at the geometric value).
//! * `Part::Payload` nodes take their mass from the [`LoadCase`] (by role),
//!   falling back to `mass_override_kg`, else zero.
//! * A node position is `offset_m` in the frame of its `parent_frame`
//!   publication (point: its position, box: its centre, plane/axis: its
//!   origin), axes parallel to the aircraft axes; `cg_point` replaces the node
//!   origin by that publication.  `parent` only groups; it adds no offset.
//! * Database box dimensions `dims_m` are read as (length, width, height)
//!   along body x, y, z before the node's Euler rotation; motors are cylinders
//!   along x.  Parts without dimensions are point masses (inertia flagged
//!   not modelled).

use std::collections::BTreeMap;

use alas_uav_db::{Body, Component, Database};

use crate::spec::skeleton::{Datum, Node, Part};
use crate::spec::UavSpec;

use super::geometry::{AirframeGeometry, TailShape};
use super::inertia::{distance, rotation_rpy_deg, Inertia, Vec3};
use super::mass::{LoadCase, MassBasis, MassGroup, MassItem, MassStatement};
use super::structure::{
    boom_structure, combine, fuselage_structure, tail_structure, wing_structure, StructureInputs,
};
use super::AirframeError;

/// A wire run between two named nodes.
#[derive(Debug, Clone, PartialEq)]
pub struct WireRun {
    /// Start node name.
    pub from: String,
    /// End node name.
    pub to: String,
    /// Mass per metre, kg/m (see [`wire_mass_per_m`]).
    pub kg_per_m: f64,
    /// Routing slack over the straight distance (ESTIMATE 1.15).
    pub slack_factor: f64,
}

/// Mass per metre of a copper wire of AWG gauge `awg` with insulation
/// (conductor `rho_cu pi d^2 / 4` with `d = 0.127 mm 92^((36 - awg)/39)`, the
/// AWG definition; insulation adds `insulation_factor`, ESTIMATE 1.25 for
/// silicone wire).
#[must_use]
pub fn wire_mass_per_m(awg: f64, insulation_factor: f64) -> f64 {
    const RHO_CU: f64 = 8960.0;
    let d = 0.127e-3 * 92.0_f64.powf((36.0 - awg) / 39.0);
    RHO_CU * std::f64::consts::PI * d * d / 4.0 * insulation_factor
}

/// Inputs of the bottom-up statement.
#[derive(Debug, Clone, PartialEq)]
pub struct MassInputs {
    /// Structural estimate inputs.
    pub structure: StructureInputs,
    /// Tail proportions in `Driven` mode.
    pub tail_shape: TailShape,
    /// Wire runs.
    pub wires: Vec<WireRun>,
}

impl MassInputs {
    /// Generic preset: foam-composite structure, default tail shape, no wires.
    #[must_use]
    pub fn generic() -> Self {
        Self {
            structure: StructureInputs::generic_foam_composite(),
            tail_shape: TailShape::default(),
            wires: Vec::new(),
        }
    }
}

fn publication_point(spec: &UavSpec, name: &str) -> Result<Vec3, AirframeError> {
    for list in spec.subskeletons.values() {
        for p in list {
            if p.name == name {
                return Ok(match &p.datum {
                    Datum::Point { xyz_m } => *xyz_m,
                    Datum::Box { min_m, max_m } => [
                        0.5 * (min_m[0] + max_m[0]),
                        0.5 * (min_m[1] + max_m[1]),
                        0.5 * (min_m[2] + max_m[2]),
                    ],
                    Datum::Plane { origin_m, .. } | Datum::Axis { origin_m, .. } => *origin_m,
                });
            }
        }
    }
    Err(AirframeError::UnknownName(name.to_owned()))
}

fn node_position(spec: &UavSpec, node: &Node) -> Result<Vec3, AirframeError> {
    if let Some(cg) = &node.cg_point {
        return publication_point(spec, cg);
    }
    let base = match &node.parent_frame {
        Some(f) => publication_point(spec, f)?,
        None => [0.0; 3],
    };
    Ok([
        base[0] + node.offset_m[0],
        base[1] + node.offset_m[1],
        base[2] + node.offset_m[2],
    ])
}

struct Resolved {
    mass: Option<f64>,
    local_inertia_for: fn(f64, &Component) -> Option<Inertia>,
    group: MassGroup,
}

fn dims_box(m: f64, dims: &Option<alas_uav_db::Field<[f64; 3]>>) -> Option<Inertia> {
    dims.as_ref().map(|d| Inertia::solid_box(m, d.value))
}

fn resolve(component: &Component) -> Result<Resolved, AirframeError> {
    let v = |o: &Option<alas_uav_db::Num>| o.as_ref().map(|f| f.value);
    let r = match &component.body {
        Body::Motor(s) => Resolved {
            mass: v(&s.mass_kg),
            local_inertia_for: |m, c| match &c.body {
                Body::Motor(s) => match (&s.diameter_m, &s.length_m) {
                    (Some(d), Some(l)) => {
                        Some(Inertia::solid_cylinder_x(m, 0.5 * d.value, l.value))
                    }
                    _ => None,
                },
                _ => None,
            },
            group: MassGroup::Propulsion,
        },
        Body::Propeller(s) => Resolved {
            mass: v(&s.mass_kg),
            local_inertia_for: |m, c| match &c.body {
                Body::Propeller(p) => {
                    // Spin axis along x: I_xx is the polar inertia, a thin
                    // disc-like rotor has half of it about each transverse axis.
                    let polar =
                        p.inertia_kg_m2.as_ref().map(|f| f.value).or_else(|| {
                            p.diameter_m.as_ref().map(|d| m * d.value * d.value / 12.0)
                        })?;
                    Some(Inertia::diagonal(polar, 0.5 * polar, 0.5 * polar))
                }
                _ => None,
            },
            group: MassGroup::Propulsion,
        },
        Body::Esc(s) => Resolved {
            mass: v(&s.mass_kg),
            local_inertia_for: |m, c| match &c.body {
                Body::Esc(s) => dims_box(m, &s.dims_m),
                _ => None,
            },
            group: MassGroup::Propulsion,
        },
        Body::Battery(s) => Resolved {
            mass: v(&s.mass_kg),
            local_inertia_for: |m, c| match &c.body {
                Body::Battery(s) => dims_box(m, &s.dims_m),
                _ => None,
            },
            group: MassGroup::Propulsion,
        },
        Body::Servo(s) => Resolved {
            mass: v(&s.mass_kg),
            local_inertia_for: |m, c| match &c.body {
                Body::Servo(s) => dims_box(m, &s.dims_m),
                _ => None,
            },
            group: MassGroup::Avionics,
        },
        Body::Receiver(s) | Body::FlightController(s) | Body::Electronics(s) => Resolved {
            mass: v(&s.mass_kg),
            local_inertia_for: |m, c| match &c.body {
                Body::Receiver(s) | Body::FlightController(s) | Body::Electronics(s) => {
                    dims_box(m, &s.dims_m)
                }
                _ => None,
            },
            group: MassGroup::Avionics,
        },
        Body::Wheel(s) => Resolved {
            mass: v(&s.mass_kg),
            local_inertia_for: |_, _| None,
            group: MassGroup::Gear,
        },
        Body::LandingGear(s) => Resolved {
            mass: v(&s.mass_kg),
            local_inertia_for: |m, c| match &c.body {
                Body::LandingGear(s) => dims_box(m, &s.dims_m),
                _ => None,
            },
            group: MassGroup::Gear,
        },
        Body::Connector(s) => Resolved {
            mass: v(&s.mass_kg),
            local_inertia_for: |_, _| None,
            group: MassGroup::Avionics,
        },
        Body::Kit(s) => Resolved {
            mass: v(&s.mass_kg),
            local_inertia_for: |_, _| None,
            group: MassGroup::Other,
        },
        Body::Material(s) => Resolved {
            mass: v(&s.stock_mass_kg),
            local_inertia_for: |m, c| match &c.body {
                Body::Material(s) => dims_box(m, &s.stock_dims_m),
                _ => None,
            },
            group: MassGroup::Other,
        },
        Body::Laminate(_) => {
            return Err(AirframeError::Invalid(format!(
                "{}: a laminate is a material for the structural estimate, not a placed part",
                component.id.as_str()
            )))
        }
    };
    Ok(r)
}

fn catalog_item(
    spec: &UavSpec,
    db: &Database,
    node: &Node,
    component: &alas_uav_db::ComponentId,
) -> Result<MassItem, AirframeError> {
    let row = db
        .current(component)
        .ok_or_else(|| AirframeError::UnknownComponent(component.as_str().to_owned()))?;
    let r = resolve(row)?;
    let (mass, basis) = match (node.mass_override_kg, r.mass) {
        (Some(m), _) => (m, MassBasis::Weighed),
        (None, Some(m)) => (m, MassBasis::Database),
        (None, None) => {
            return Err(AirframeError::Missing {
                what: component.as_str().to_owned(),
                field: "mass_kg",
            })
        }
    };
    let local = (r.local_inertia_for)(mass, row);
    let rot = rotation_rpy_deg(node.euler_deg);
    Ok(MassItem {
        name: node.name.clone(),
        group: r.group,
        mass_kg: mass,
        cg_m: node_position(spec, node)?,
        inertia_cg: local.unwrap_or(Inertia::ZERO).rotated(&rot),
        basis,
        inertia_modelled: local.is_some(),
    })
}

fn overridden(item: MassItem, role: &str, tree: &[Node]) -> MassItem {
    let ov = tree.iter().find_map(|n| match &n.part {
        Part::Structure { role: r } if r == role => n.mass_override_kg,
        _ => None,
    });
    match ov {
        Some(m) if item.mass_kg > 0.0 => {
            let k = m / item.mass_kg;
            MassItem {
                mass_kg: m,
                inertia_cg: item.inertia_cg.scaled(k),
                basis: MassBasis::Weighed,
                ..item
            }
        }
        _ => item,
    }
}

const STRUCTURE_ROLES: [&str; 5] = ["wing", "htail", "vtail", "boom", "fuselage"];

/// Build the mass statement of the spec for one load case.
///
/// # Errors
///
/// Unknown components, publications or structure roles; a catalogue part
/// without a mass and without an override; non-physical geometry.
pub fn build_mass_statement(
    spec: &UavSpec,
    db: &Database,
    inputs: &MassInputs,
    case: &LoadCase,
) -> Result<MassStatement, AirframeError> {
    let geometry = AirframeGeometry::from_master(&spec.master, &spec.topology, &inputs.tail_shape)?;
    let mut items: Vec<MassItem> = Vec::new();
    let mut structure: Vec<MassItem> = Vec::new();
    for node in &spec.tree {
        if let Part::Structure { role } = &node.part {
            if !STRUCTURE_ROLES.contains(&role.as_str()) {
                return Err(AirframeError::UnknownName(format!(
                    "structure role `{role}`"
                )));
            }
            let present = match role.as_str() {
                "htail" => geometry.horizontal.is_some(),
                "vtail" => geometry.vertical.is_some(),
                "boom" => spec.master.tail_boom.is_some(),
                "fuselage" => {
                    inputs.structure.fuselage_shell.is_some() || node.mass_override_kg.is_some()
                }
                _ => true,
            };
            if !present {
                return Err(AirframeError::Invalid(format!(
                    "structure node `{}` (role `{role}`) has no matching geometry or weighed mass",
                    node.name
                )));
            }
        }
    }
    structure.push(overridden(
        wing_structure(&geometry.wing, &inputs.structure)?,
        "wing",
        &spec.tree,
    ));
    if let Some(h) = &geometry.horizontal {
        structure.push(overridden(
            tail_structure(h, &inputs.structure)?,
            "htail",
            &spec.tree,
        ));
    }
    if let Some(v) = &geometry.vertical {
        structure.push(overridden(
            tail_structure(v, &inputs.structure)?,
            "vtail",
            &spec.tree,
        ));
    }
    if let Some(b) = &spec.master.tail_boom {
        let x_end = spec.master.tail_xyz_m[0];
        let yz = [spec.master.tail_xyz_m[1], spec.master.tail_xyz_m[2]];
        structure.push(overridden(
            boom_structure(b, x_end, yz, &inputs.structure)?,
            "boom",
            &spec.tree,
        ));
    }
    if let Some(shell) = &inputs.structure.fuselage_shell {
        structure.push(overridden(
            fuselage_structure(shell, &inputs.structure)?,
            "fuselage",
            &spec.tree,
        ));
    }
    let structure_mass: f64 = structure.iter().map(|i| i.mass_kg).sum();
    if inputs.structure.allowance_fraction > 0.0 && structure_mass > 0.0 {
        let all = combine(
            "structure",
            MassGroup::Structure,
            MassBasis::Estimate,
            &structure,
        )?;
        items.push(MassItem {
            name: "fasteners, glue, covering".into(),
            group: MassGroup::Allowance,
            mass_kg: inputs.structure.allowance_fraction * structure_mass,
            cg_m: all.cg_m,
            inertia_cg: all.inertia_cg.scaled(inputs.structure.allowance_fraction),
            basis: MassBasis::Estimate,
            inertia_modelled: true,
        });
    }
    items.extend(structure);

    let mut positions: BTreeMap<String, Vec3> = BTreeMap::new();
    for node in &spec.tree {
        let pos = node_position(spec, node)?;
        positions.insert(node.name.clone(), pos);
        match &node.part {
            Part::Assembly => {}
            Part::Structure { role } => {
                // A fuselage with a weighed mass but no shell geometry is a point mass.
                if role == "fuselage" && inputs.structure.fuselage_shell.is_none() {
                    if let Some(m) = node.mass_override_kg {
                        items.push(point_like(
                            node,
                            MassGroup::Structure,
                            m,
                            MassBasis::Weighed,
                            pos,
                        ));
                    }
                }
            }
            Part::Catalog { component } => items.push(catalog_item(spec, db, node, component)?),
            Part::Payload { role } => {
                let (m, basis) = match (case.payload_kg.get(role), node.mass_override_kg) {
                    (Some(m), _) => (*m, MassBasis::UserInput),
                    (None, Some(m)) => (m, MassBasis::Weighed),
                    (None, None) => (0.0, MassBasis::UserInput),
                };
                items.push(point_like(node, MassGroup::Payload, m, basis, pos));
            }
            Part::Ballast { mass_kg } => {
                let m = node.mass_override_kg.unwrap_or(*mass_kg);
                items.push(point_like(
                    node,
                    MassGroup::Ballast,
                    m,
                    MassBasis::UserInput,
                    pos,
                ));
            }
            Part::Allowance { mass_kg } => {
                let m = node.mass_override_kg.unwrap_or(*mass_kg);
                items.push(point_like(
                    node,
                    MassGroup::Allowance,
                    m,
                    MassBasis::UserInput,
                    pos,
                ));
            }
        }
    }
    for w in &inputs.wires {
        let a = positions
            .get(&w.from)
            .ok_or_else(|| AirframeError::UnknownName(w.from.clone()))?;
        let b = positions
            .get(&w.to)
            .ok_or_else(|| AirframeError::UnknownName(w.to.clone()))?;
        let len = distance(*a, *b) * w.slack_factor;
        items.push(MassItem {
            name: format!("wire {} - {}", w.from, w.to),
            group: MassGroup::Avionics,
            mass_kg: w.kg_per_m * len,
            cg_m: [
                0.5 * (a[0] + b[0]),
                0.5 * (a[1] + b[1]),
                0.5 * (a[2] + b[2]),
            ],
            inertia_cg: Inertia::ZERO,
            basis: MassBasis::Estimate,
            inertia_modelled: false,
        });
    }
    for (k, (m, pos)) in case.ballast.iter().enumerate() {
        items.push(MassItem {
            name: format!("{} ballast {k}", case.name),
            group: MassGroup::Ballast,
            mass_kg: *m,
            cg_m: *pos,
            inertia_cg: Inertia::ZERO,
            basis: MassBasis::UserInput,
            inertia_modelled: false,
        });
    }
    items.retain(|i| i.mass_kg != 0.0);
    if items
        .iter()
        .any(|i| !i.mass_kg.is_finite() || i.mass_kg < 0.0)
    {
        return Err(AirframeError::Invalid(
            "a mass is negative or not finite".into(),
        ));
    }
    Ok(MassStatement { items })
}

fn point_like(node: &Node, group: MassGroup, m: f64, basis: MassBasis, pos: Vec3) -> MassItem {
    MassItem {
        name: node.name.clone(),
        group,
        mass_kg: m,
        cg_m: pos,
        inertia_cg: Inertia::ZERO,
        basis,
        inertia_modelled: false,
    }
}
