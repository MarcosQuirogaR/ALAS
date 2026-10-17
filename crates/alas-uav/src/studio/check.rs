// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Collision and volume checks of the placed parts.
//!
//! Checks (all geometric, bounding-box based, so they are conservative for
//! rounded parts and for the real bay section):
//! * missing dimensions of a placed part;
//! * containment of a part in the bay it is referenced to;
//! * pairwise box interference (propellers are discs, handled separately);
//! * bay fill (sum of part boxes over bay volume) against `MAX_BAY_FILL`;
//! * propeller disc against the pod envelope, the ground plane, the other
//!   disc and any box crossing the disc plane.

use alas_uav_db::{Body, Database};

use crate::airframe::inertia::Vec3;
use crate::spec::skeleton::{Datum, Part};
use crate::spec::UavSpec;

use super::pack::{node_centre, part_dims, rotated_size, Box3};
use super::publish::find;
use super::StudioError;

/// Practical upper bound on bay fill for wiring, connectors and access
/// (ESTIMATE, a planning rule of thumb, not a derived limit).
pub const MAX_BAY_FILL: f64 = 0.6;
/// Minimum propeller-tip ground clearance that is not flagged, m (ESTIMATE:
/// grass and attitude margin on a rotation).
pub const MIN_PROP_GROUND_CLEARANCE_M: f64 = 0.05;
const TOL: f64 = 1e-9;

/// Severity of an issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Worth a look.
    Warning,
    /// The design cannot be built as it stands.
    Error,
}

/// What went wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueKind {
    /// The database row has no dimensions.
    MissingDims,
    /// A part leaves its bay.
    OutsideBay,
    /// Two parts occupy the same volume.
    Collision,
    /// A bay is fuller than [`MAX_BAY_FILL`].
    BayFill,
    /// A propeller disc strikes something.
    PropellerStrike,
    /// A propeller tip is close to the ground.
    PropellerGround,
}

/// One finding.
#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    /// Severity.
    pub severity: Severity,
    /// Kind.
    pub kind: IssueKind,
    /// Parts concerned.
    pub parts: Vec<String>,
    /// Text for the user.
    pub message: String,
}

/// A placed part.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    /// Node name.
    pub name: String,
    /// Category key of the component.
    pub category: &'static str,
    /// Bounding box in body axes, m.
    pub aabb: Box3,
    /// Cylinder along x (motor).
    pub cylinder_x: bool,
    /// Bay the node is referenced to.
    pub bay: Option<String>,
}

/// Propeller disc.
#[derive(Debug, Clone, PartialEq)]
pub struct Disc {
    /// Node name.
    pub name: String,
    /// Centre, m.
    pub centre_m: Vec3,
    /// Radius, m.
    pub radius_m: f64,
}

/// Fill of one bay.
#[derive(Debug, Clone, PartialEq)]
pub struct BayFill {
    /// Bay name.
    pub bay: String,
    /// Bay volume, m^3.
    pub volume_m3: f64,
    /// Sum of the part boxes inside it, m^3.
    pub used_m3: f64,
}

impl BayFill {
    /// Fill fraction, 1.
    pub fn fraction(&self) -> f64 {
        self.used_m3 / self.volume_m3
    }
}

/// Result of [`check`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PackReport {
    /// Placed boxes.
    pub parts: Vec<Placed>,
    /// Propeller discs.
    pub discs: Vec<Disc>,
    /// Bay fills.
    pub fills: Vec<BayFill>,
    /// Findings, errors first.
    pub issues: Vec<Issue>,
}

impl PackReport {
    /// No errors (warnings allowed).
    pub fn is_ok(&self) -> bool {
        !self.issues.iter().any(|i| i.severity == Severity::Error)
    }
}

fn disc_hits_box(d: &Disc, b: &Box3) -> bool {
    let slab = 5.0e-4;
    if b.max[0] < d.centre_m[0] - slab || b.min[0] > d.centre_m[0] + slab {
        return false;
    }
    let dy = (b.min[1] - d.centre_m[1])
        .max(0.0)
        .max(d.centre_m[1] - b.max[1]);
    let dz = (b.min[2] - d.centre_m[2])
        .max(0.0)
        .max(d.centre_m[2] - b.max[2]);
    dy.hypot(dz) < d.radius_m - TOL
}

/// Check every catalogue node of the tree.
///
/// # Errors
///
/// Unresolved publications or components.
pub fn check(spec: &UavSpec, db: &Database) -> Result<PackReport, StudioError> {
    let mut rep = PackReport::default();
    for node in &spec.tree {
        let Part::Catalog { component } = &node.part else {
            continue;
        };
        let row = db
            .current(component)
            .ok_or_else(|| StudioError::UnknownName(component.as_str().to_owned()))?;
        let c = node_centre(spec, node)?;
        if let Body::Propeller(p) = &row.body {
            let d = p.diameter_m.as_ref().map(|f| f.value).ok_or_else(|| {
                StudioError::Invalid(format!("{}: propeller without diameter", node.name))
            })?;
            rep.discs.push(Disc {
                name: node.name.clone(),
                centre_m: c,
                radius_m: 0.5 * d,
            });
            continue;
        }
        let Some(dims) = part_dims(row) else {
            rep.issues.push(Issue {
                severity: Severity::Error,
                kind: IssueKind::MissingDims,
                parts: vec![node.name.clone()],
                message: format!("{}: the database row has no dimensions", node.name),
            });
            continue;
        };
        let size = rotated_size(dims.size_m, node.euler_deg);
        rep.parts.push(Placed {
            name: node.name.clone(),
            category: row.category().key(),
            aabb: Box3::from_centre(c, size),
            cylinder_x: dims.cylinder_x,
            bay: node
                .parent_frame
                .as_deref()
                .and_then(|f| f.strip_prefix("FS_Bay_"))
                .map(str::to_owned),
        });
    }
    containment(spec, &mut rep);
    interference(&mut rep);
    fills(spec, &mut rep);
    discs(spec, &mut rep);
    rep.issues.sort_by_key(|i| std::cmp::Reverse(i.severity));
    Ok(rep)
}

fn containment(spec: &UavSpec, rep: &mut PackReport) {
    for p in &rep.parts {
        let Some(bay) = &p.bay else { continue };
        if let Some(Datum::Box { min_m, max_m }) =
            find(spec, &format!("FS_Bay_{bay}")).map(|q| &q.datum)
        {
            let outer = Box3 {
                min: *min_m,
                max: *max_m,
            };
            if !p.aabb.inside(&outer, TOL) {
                rep.issues.push(Issue {
                    severity: Severity::Error,
                    kind: IssueKind::OutsideBay,
                    parts: vec![p.name.clone()],
                    message: format!("{} does not fit inside bay {bay}", p.name),
                });
            }
        }
    }
}

fn interference(rep: &mut PackReport) {
    for (i, a) in rep.parts.iter().enumerate() {
        for b in &rep.parts[i + 1..] {
            let v = a.aabb.overlap_volume(&b.aabb);
            if v > 1e-9 {
                rep.issues.push(Issue {
                    severity: Severity::Error,
                    kind: IssueKind::Collision,
                    parts: vec![a.name.clone(), b.name.clone()],
                    message: format!("{} and {} overlap by {:.1} cm3", a.name, b.name, v * 1.0e6),
                });
            }
        }
    }
}

fn fills(spec: &UavSpec, rep: &mut PackReport) {
    let Some(f) = &spec.master.layout.fuselage else {
        return;
    };
    for b in &f.bays {
        let Some(Datum::Box { min_m, max_m }) =
            find(spec, &format!("FS_Bay_{}", b.name)).map(|q| &q.datum)
        else {
            continue;
        };
        let bay = Box3 {
            min: *min_m,
            max: *max_m,
        };
        let used: f64 = rep
            .parts
            .iter()
            .filter(|p| p.bay.as_deref() == Some(b.name.as_str()))
            .map(|p| p.aabb.overlap_volume(&bay))
            .sum();
        let fill = BayFill {
            bay: b.name.clone(),
            volume_m3: bay.volume(),
            used_m3: used,
        };
        if fill.fraction() > MAX_BAY_FILL {
            rep.issues.push(Issue {
                severity: if fill.fraction() > 1.0 {
                    Severity::Error
                } else {
                    Severity::Warning
                },
                kind: IssueKind::BayFill,
                parts: Vec::new(),
                message: format!(
                    "bay {} is {:.0} % full (practical limit {:.0} %)",
                    b.name,
                    100.0 * fill.fraction(),
                    100.0 * MAX_BAY_FILL
                ),
            });
        }
        rep.fills.push(fill);
    }
}

fn discs(spec: &UavSpec, rep: &mut PackReport) {
    let envelope = match find(spec, "FS_Envelope").map(|p| &p.datum) {
        Some(Datum::Box { min_m, max_m }) => Some(Box3 {
            min: *min_m,
            max: *max_m,
        }),
        _ => None,
    };
    let ground = spec.master.layout.gear.as_ref().map(|g| g.ground_z_m);
    let discs = rep.discs.clone();
    let mut found = Vec::new();
    for (i, d) in discs.iter().enumerate() {
        if let Some(env) = &envelope {
            if disc_hits_box(d, env) {
                found.push(strike(d, "the fuselage"));
            }
        }
        for p in &rep.parts {
            // The motor behind its own disc touches the plane; skip coaxial cans.
            let coaxial = p.cylinder_x
                && (p.aabb.centre()[1] - d.centre_m[1]).abs() < 1e-3
                && (p.aabb.centre()[2] - d.centre_m[2]).abs() < 1e-3;
            if !coaxial && disc_hits_box(d, &p.aabb) {
                found.push(strike(d, &p.name));
            }
        }
        for o in &discs[i + 1..] {
            let dist = (d.centre_m[1] - o.centre_m[1]).hypot(d.centre_m[2] - o.centre_m[2]);
            if (d.centre_m[0] - o.centre_m[0]).abs() < 1e-3 && dist < d.radius_m + o.radius_m {
                found.push(strike(d, &o.name));
            }
        }
        if let Some(gz) = ground {
            let clearance = d.centre_m[2] - d.radius_m - gz;
            if clearance < MIN_PROP_GROUND_CLEARANCE_M {
                found.push(Issue {
                    severity: if clearance <= 0.0 {
                        Severity::Error
                    } else {
                        Severity::Warning
                    },
                    kind: IssueKind::PropellerGround,
                    parts: vec![d.name.clone()],
                    message: format!(
                        "{}: tip clearance to the ground {:.0} mm (flagged below {:.0} mm)",
                        d.name,
                        1000.0 * clearance,
                        1000.0 * MIN_PROP_GROUND_CLEARANCE_M
                    ),
                });
            }
        }
    }
    rep.issues.extend(found);
}

fn strike(d: &Disc, what: &str) -> Issue {
    Issue {
        severity: Severity::Error,
        kind: IssueKind::PropellerStrike,
        parts: vec![d.name.clone()],
        message: format!("propeller {} strikes {what}", d.name),
    }
}
