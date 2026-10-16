// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Packaging of database parts into the fuselage bays.
//!
//! Each part is its bounding box (a motor is a cylinder along `x`, bounded by
//! its box). A part is placed by `pack` with its largest-fit orientation
//! (rotations by multiples of 90 deg about the body axes, carried as the node's
//! Euler angles so mass and inertia stay consistent), centred in `y`, resting
//! on the bay floor and stacked along `x` with a clearance gap. Missing
//! dimensions are an issue, never a default.
//!
//! The bay is the inscribed rectangular box of the pod (see `spec::layout`);
//! a rounded real section holds less, so a fit here is necessary, not
//! sufficient.

use alas_uav_db::{Body, Category, Component, ComponentId, Database};

use crate::airframe::inertia::{rotation_rpy_deg, Vec3};
use crate::spec::skeleton::{Datum, Node, Part};
use crate::spec::UavSpec;

use super::publish::find;
use super::StudioError;

/// Default gap between packed parts, m (ESTIMATE: wiring and foam; override
/// with `tolerances_m["packaging_clearance"]`).
pub const DEFAULT_CLEARANCE_M: f64 = 0.003;

/// Axis-aligned box, m.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Box3 {
    /// Minimum corner.
    pub min: Vec3,
    /// Maximum corner.
    pub max: Vec3,
}

impl Box3 {
    /// Box from a centre and a full size.
    pub fn from_centre(c: Vec3, size: Vec3) -> Self {
        Self {
            min: [
                c[0] - 0.5 * size[0],
                c[1] - 0.5 * size[1],
                c[2] - 0.5 * size[2],
            ],
            max: [
                c[0] + 0.5 * size[0],
                c[1] + 0.5 * size[1],
                c[2] + 0.5 * size[2],
            ],
        }
    }

    /// Volume, m^3.
    pub fn volume(&self) -> f64 {
        (0..3).map(|i| self.max[i] - self.min[i]).product()
    }

    /// Centre.
    pub fn centre(&self) -> Vec3 {
        [0, 1, 2].map(|i| 0.5 * (self.min[i] + self.max[i]))
    }

    /// Interference volume with `o` (zero when disjoint), m^3.
    pub fn overlap_volume(&self, o: &Self) -> f64 {
        (0..3)
            .map(|i| (self.max[i].min(o.max[i]) - self.min[i].max(o.min[i])).max(0.0))
            .product()
    }

    /// Whether `self` lies inside `outer` within `tol`.
    pub fn inside(&self, outer: &Self, tol: f64) -> bool {
        (0..3).all(|i| self.min[i] >= outer.min[i] - tol && self.max[i] <= outer.max[i] + tol)
    }
}

/// Bounding dimensions of a part before rotation: length, width, height along
/// body `x`, `y`, `z`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PartDims {
    /// Size along x, y, z, m.
    pub size_m: Vec3,
    /// Cylinder along x (a motor) rather than a solid box.
    pub cylinder_x: bool,
}

/// Dimensions of a placed component; `None` for a part that has none in the
/// database (and for propellers, which are discs).
pub fn part_dims(component: &Component) -> Option<PartDims> {
    let bx = |d: &Option<alas_uav_db::Field<[f64; 3]>>| {
        d.as_ref().map(|f| PartDims {
            size_m: f.value,
            cylinder_x: false,
        })
    };
    match &component.body {
        Body::Motor(m) => match (&m.length_m, &m.diameter_m) {
            (Some(l), Some(d)) => Some(PartDims {
                size_m: [l.value, d.value, d.value],
                cylinder_x: true,
            }),
            _ => None,
        },
        Body::Esc(s) => bx(&s.dims_m),
        Body::Battery(s) => bx(&s.dims_m),
        Body::Servo(s) => bx(&s.dims_m),
        Body::Receiver(s) | Body::FlightController(s) | Body::Electronics(s) => bx(&s.dims_m),
        Body::LandingGear(s) => bx(&s.dims_m),
        Body::Wheel(w) => match (&w.diameter_m, &w.width_m) {
            (Some(d), Some(t)) => Some(PartDims {
                size_m: [d.value, t.value, d.value],
                cylinder_x: false,
            }),
            _ => None,
        },
        _ => None,
    }
}

/// Size of the axis-aligned box of `dims` rotated by `euler_deg` (exact for
/// multiples of 90 deg, a bound otherwise).
pub fn rotated_size(dims: Vec3, euler_deg: Vec3) -> Vec3 {
    let r = rotation_rpy_deg(euler_deg);
    [0, 1, 2].map(|i| (0..3).map(|j| r[i][j].abs() * dims[j]).sum())
}

/// Centre of a datum (point, box centre, or origin).
pub fn datum_centre(d: &Datum) -> Vec3 {
    match d {
        Datum::Point { xyz_m } => *xyz_m,
        Datum::Box { min_m, max_m } => [0, 1, 2].map(|i| 0.5 * (min_m[i] + max_m[i])),
        Datum::Plane { origin_m, .. } | Datum::Axis { origin_m, .. } => *origin_m,
    }
}

/// Position of a tree node: `cg_point` publication, else the frame
/// publication plus the offset (the rule of the mass statement).
///
/// # Errors
///
/// [`StudioError::UnknownName`] for an unresolved publication.
pub fn node_centre(spec: &UavSpec, node: &Node) -> Result<Vec3, StudioError> {
    let get = |n: &str| {
        find(spec, n)
            .map(|p| datum_centre(&p.datum))
            .ok_or_else(|| StudioError::UnknownName(n.to_owned()))
    };
    if let Some(cg) = &node.cg_point {
        return get(cg);
    }
    let base = match &node.parent_frame {
        Some(f) => get(f)?,
        None => [0.0; 3],
    };
    Ok([0, 1, 2].map(|i| base[i] + node.offset_m[i]))
}

/// One part to pack.
#[derive(Debug, Clone, PartialEq)]
pub struct PartRequest {
    /// Node name.
    pub name: String,
    /// Database component.
    pub component: ComponentId,
    /// Bay name (`Nose`, `Battery`, ...); `None` picks the default of the category.
    pub bay: Option<String>,
}

/// Default bay of a category.
pub fn default_bay(category: Category) -> &'static str {
    match category {
        Category::Battery => "Battery",
        Category::Esc | Category::Motor => "Nose",
        Category::Receiver | Category::FlightController | Category::Electronics => "Aft",
        _ => "Payload",
    }
}

fn bay_box(spec: &UavSpec, bay: &str) -> Result<Box3, StudioError> {
    let name = format!("FS_Bay_{bay}");
    match find(spec, &name).map(|p| &p.datum) {
        Some(Datum::Box { min_m, max_m }) => Ok(Box3 {
            min: *min_m,
            max: *max_m,
        }),
        _ => Err(StudioError::UnknownName(name)),
    }
}

/// Pack `requests` into the bays and return the spec with the new nodes
/// (existing nodes of the same names are replaced). Parts that do not fit are
/// still placed (overflowing) so the problem is visible; run
/// [`super::check::check`] for the issues.
///
/// # Errors
///
/// Unknown component or bay; a part without dimensions.
pub fn pack(
    spec: &UavSpec,
    db: &Database,
    requests: &[PartRequest],
) -> Result<UavSpec, StudioError> {
    let gap = spec
        .tolerances_m
        .get("packaging_clearance")
        .copied()
        .unwrap_or(DEFAULT_CLEARANCE_M);
    let mut out = spec.clone();
    out.tree
        .retain(|n| !requests.iter().any(|r| r.name == n.name));
    let mut cursors: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    for r in requests {
        let row = db
            .current(&r.component)
            .ok_or_else(|| StudioError::UnknownName(r.component.as_str().to_owned()))?;
        let dims = part_dims(row).ok_or_else(|| {
            StudioError::Invalid(format!("{}: the database row has no dimensions", r.name))
        })?;
        let bay = r
            .bay
            .clone()
            .unwrap_or_else(|| default_bay(row.category()).to_owned());
        let b = bay_box(spec, &bay)?;
        let (ey, ez) = (b.max[1] - b.min[1], b.max[2] - b.min[2]);
        // Orientation: smallest x extent among those fitting the section; else
        // the smallest cross-section overflow.
        let mut best: Option<(f64, Vec3, Vec3)> = None;
        for roll in [0.0, 90.0] {
            for pitch in [0.0, 90.0] {
                for yaw in [0.0, 90.0] {
                    let e = [roll, pitch, yaw];
                    let s = rotated_size(dims.size_m, e);
                    let over = (s[1] - ey).max(0.0) + (s[2] - ez).max(0.0);
                    let key = over * 1.0e3 + s[0];
                    if best.is_none_or(|(k, _, _)| key < k - 1e-12) {
                        best = Some((key, s, e));
                    }
                }
            }
        }
        let (_, size, euler) = best.unwrap_or((0.0, dims.size_m, [0.0; 3]));
        let cursor = cursors.entry(bay.clone()).or_insert(b.min[0]);
        let cx = *cursor + 0.5 * size[0];
        *cursor += size[0] + gap;
        let centre = [cx, 0.5 * (b.min[1] + b.max[1]), b.min[2] + 0.5 * size[2]];
        let bc = b.centre();
        out.tree.push(Node {
            name: r.name.clone(),
            parent: None,
            parent_frame: Some(format!("FS_Bay_{bay}")),
            offset_m: [0, 1, 2].map(|i| centre[i] - bc[i]),
            euler_deg: euler,
            cg_point: None,
            mass_override_kg: None,
            part: Part::Catalog {
                component: r.component.clone(),
            },
        });
    }
    Ok(out)
}

/// A node mounted directly on a publication (servo on its seat, wheel on its
/// axle point, motor behind its propeller plane).
pub fn mount(
    name: &str,
    component: &ComponentId,
    frame: &str,
    offset_m: Vec3,
    euler_deg: Vec3,
) -> Node {
    Node {
        name: name.to_owned(),
        parent: None,
        parent_frame: Some(frame.to_owned()),
        offset_m,
        euler_deg,
        cg_point: None,
        mass_override_kg: None,
        part: Part::Catalog {
            component: component.clone(),
        },
    }
}

/// Mount a motor on a propeller-plane publication: the can sits behind the
/// plane for a tractor and ahead of it for a pusher.
///
/// # Errors
///
/// Unknown component or one without dimensions.
pub fn mount_motor(
    db: &Database,
    name: &str,
    component: &ComponentId,
    frame: &str,
    pusher: bool,
) -> Result<Node, StudioError> {
    let row = db
        .current(component)
        .ok_or_else(|| StudioError::UnknownName(component.as_str().to_owned()))?;
    let d = part_dims(row)
        .filter(|d| d.cylinder_x)
        .ok_or_else(|| StudioError::Invalid(format!("{name}: not a motor with dimensions")))?;
    let sign = if pusher { -1.0 } else { 1.0 };
    Ok(mount(
        name,
        component,
        frame,
        [sign * 0.5 * d.size_m[0], 0.0, 0.0],
        [0.0; 3],
    ))
}
