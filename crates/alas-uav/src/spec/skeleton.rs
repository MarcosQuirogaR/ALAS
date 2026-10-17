// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Subskeletons, publications and the component tree.
//!
//! Mirrors the CAD practice of a global skeleton publishing named datums that
//! sub-skeletons and parts reference. A part never holds absolute coordinates:
//! it names a publication (`parent_frame`) and an offset in that frame.

use alas_uav_db::ComponentId;
use serde::{Deserialize, Serialize};

/// The skeleton that owns a publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Skeleton {
    /// Wing: rib planes, spar x-positions.
    Wing,
    /// Fuselage: frame x-positions, bay boxes.
    Fuselage,
    /// Empennage.
    Empennage,
    /// Wing mechanisation: hinge lines, servo seats.
    WingMech,
    /// Empennage mechanisation.
    EmpennageMech,
    /// Landing gear.
    Gear,
    /// Propulsion: motor mount, propeller plane.
    Propulsion,
}

/// A named datum.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Datum {
    /// A point, m.
    Point {
        /// Position in the aircraft frame, m.
        xyz_m: [f64; 3],
    },
    /// A plane.
    Plane {
        /// A point on the plane, m.
        origin_m: [f64; 3],
        /// Unit normal.
        normal: [f64; 3],
    },
    /// An axis (hinge line, boom axis).
    Axis {
        /// A point on the axis, m.
        origin_m: [f64; 3],
        /// Unit direction.
        direction: [f64; 3],
    },
    /// An axis-aligned box (tank or bay), m.
    Box {
        /// Minimum corner.
        min_m: [f64; 3],
        /// Maximum corner.
        max_m: [f64; 3],
    },
}

/// A published datum.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Publication {
    /// Unique name (for example `WS_Spar_Front_Plane`).
    pub name: String,
    /// Owning skeleton.
    pub owner: Skeleton,
    /// The datum.
    pub datum: Datum,
}

/// What a tree leaf is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Part {
    /// A grouping node.
    Assembly,
    /// A purchased component of the database.
    Catalog {
        /// Database id.
        component: ComponentId,
    },
    /// Structure sized by the structures module.
    Structure {
        /// Structure role (spar, skin, rib, boom, joint, ...).
        role: String,
    },
    /// Payload (cans, bottles, rice bags, water).
    Payload {
        /// Payload role text.
        role: String,
    },
    /// Trim ballast.
    Ballast {
        /// Mass, kg.
        mass_kg: f64,
    },
    /// Mass allowance (wiring, glue, covering).
    Allowance {
        /// Mass, kg.
        mass_kg: f64,
    },
}

/// A tree node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    /// Unique name.
    pub name: String,
    /// Parent node name; `None` for a root.
    pub parent: Option<String>,
    /// Publication whose frame the offset is expressed in; `None` = aircraft frame.
    pub parent_frame: Option<String>,
    /// Offset in that frame, m.
    pub offset_m: [f64; 3],
    /// Euler angles (roll, pitch, yaw), deg.
    pub euler_deg: [f64; 3],
    /// Publication marking the node's centre of gravity, if not its origin.
    pub cg_point: Option<String>,
    /// Mass override, kg, for parts whose mass is measured rather than computed.
    pub mass_override_kg: Option<f64>,
    /// What the node is.
    pub part: Part,
}
