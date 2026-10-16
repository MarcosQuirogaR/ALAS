// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Topology descriptors: enums carrying data, not code branches.
//!
//! These replace the four-value `UavTopology` of the legacy optimiser for the
//! new module. The legacy enum maps onto them (`ConventionalTail`, `TTail`,
//! `VTail`, `FlyingWing`) but cannot express panels, motors or gear.

use serde::{Deserialize, Serialize};

/// Wing arrangement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WingKind {
    /// Straight multi-panel wing described by `MasterParams::panels`.
    Straight,
    /// Swept tailless wing.
    Flying {
        /// Quarter-chord sweep, deg.
        sweep_deg: f64,
        /// Tip washout (positive nose-down at the tip), deg.
        washout_deg: f64,
        /// Whether the airfoil is a reflexed, self-stabilising section.
        reflex_airfoil: bool,
    },
}

/// Empennage arrangement.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TailKind {
    /// Horizontal tail on the fuselage or boom, fin above.
    Conventional,
    /// Horizontal tail on top of the fin.
    T,
    /// V-tail (two surfaces, ruddervators).
    V {
        /// Half-angle of each surface from horizontal, deg.
        dihedral_deg: f64,
    },
    /// Inverted V-tail.
    InvV {
        /// Half-angle of each surface from horizontal, deg (anhedral magnitude).
        dihedral_deg: f64,
    },
    /// Twin booms or H-tail (generic surfaces).
    TwinBoom,
    /// No empennage (flying wing).
    None,
}

/// Rotation sense of the two propellers of a twin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rotation {
    /// Both turn the same way.
    Co,
    /// Opposite senses.
    Counter,
}

/// Motor arrangement.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MotorLayout {
    /// One tractor motor on the nose.
    SingleTractor,
    /// One pusher motor at the tail.
    SinglePusher,
    /// Two wing-mounted tractors.
    Twin {
        /// Motor y-position as a fraction of the half-span, (0, 1).
        span_fraction: f64,
        /// Propeller rotation sense.
        rotation: Rotation,
    },
}

/// Landing-gear arrangement.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GearKind {
    /// Two mains and a tail wheel.
    Taildragger,
    /// Two mains and a nose wheel.
    Tricycle,
    /// Belly landing or skid.
    Belly,
    /// Retractable gear (mass and drag only).
    Retract {
        /// Extra mass of the retract mechanism, kg.
        mechanism_mass_kg: f64,
        /// Retracted-drag reduction as a fraction of the extended gear drag, [0, 1].
        drag_reduction: f64,
    },
}

/// Fuselage arrangement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FuselageKind {
    /// Pod only.
    Pod,
    /// Pod plus tail boom.
    Boom,
    /// No fuselage (flying wing with an embedded bay).
    None,
}

/// The discrete configuration choices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Topology {
    /// Wing arrangement.
    pub wing: WingKind,
    /// Empennage arrangement.
    pub tail: TailKind,
    /// Motor arrangement.
    pub motors: MotorLayout,
    /// Landing gear.
    pub gear: GearKind,
    /// Fuselage.
    pub fuselage: FuselageKind,
}
