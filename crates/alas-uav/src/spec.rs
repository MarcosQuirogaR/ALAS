// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! `UavSpec`: the single serialisable source of truth of a UAV design.
//!
//! Every discipline reads this object (the team reports show spans and masses
//! drifting between sections of one report because there was no such object).
//! It lives outside `AlasConfig` on purpose (decision D6): no parity ledger,
//! no Spanish-catalogue entry per field and no frozen-module budget churn;
//! `inventory` is its own field inventory with units and ranges.
//!
//! This is the frozen v1 schema. Geometry and loads are derived from it and are
//! never stored. Origin tags (`ParamOrigin`) are the per-parameter provenance:
//! whether a value was typed by the user, imported from a CAD parameter table,
//! calibrated on test data, derived, or left at a default.

pub mod inventory;
pub mod layout;
pub mod master;
pub mod mission;
pub mod score;
pub mod skeleton;
pub mod topology;
mod validate;
mod validate_layout;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use master::MasterParams;
pub use mission::MissionSpec;
pub use skeleton::{Node, Publication, Skeleton};
pub use topology::Topology;

/// Schema version of the serialized `UavSpec`.
pub const SPEC_SCHEMA_VERSION: u32 = 1;

/// Where a parameter value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OriginKind {
    /// Typed by the user.
    User,
    /// Imported from a CAD parameter table.
    CadImport,
    /// Fitted to test or flight data.
    Calibrated,
    /// Computed from other parameters (driven mode).
    Derived,
    /// Left at the template default.
    Default,
}

/// Per-parameter provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamOrigin {
    /// Origin class.
    pub kind: OriginKind,
    /// Free-text note (file, test id, date).
    pub note: Option<String>,
}

/// The design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UavSpec {
    /// Schema version ([`SPEC_SCHEMA_VERSION`]).
    pub schema_version: u32,
    /// Design name.
    pub name: String,
    /// Global skeleton parameters.
    pub master: MasterParams,
    /// Discrete configuration.
    pub topology: Topology,
    /// Publications by owning skeleton.
    pub subskeletons: BTreeMap<Skeleton, Vec<Publication>>,
    /// Manufacturing tolerance parameters, m (for example `printing_tolerance_m`).
    pub tolerances_m: BTreeMap<String, f64>,
    /// Component and structure tree.
    pub tree: Vec<Node>,
    /// Mission and score selection.
    pub mission: MissionSpec,
    /// Origin of parameters, keyed by concrete path (absent = `User`).
    pub origins: BTreeMap<String, ParamOrigin>,
}

impl UavSpec {
    /// Check ranges and structural invariants; returns every problem found.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        validate::validate(self)
    }
}
