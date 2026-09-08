// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::collections::HashMap;

use alas_config::AlasConfig;
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};
use alas_geom::aircraft::wing::{Wing, WingXSec};
use thiserror::Error;

use super::model::{CpacsDocument, CpacsFuselage, CpacsTransformation, CpacsWing};

/// Failure while converting CPACS geometry into the existing physics input.
#[derive(Debug, Error)]
pub enum CpacsAircraftError {
    /// The CPACS model has no lifting surface.
    #[error("CPACS aircraft contains no wings")]
    NoWings,
    /// A CPACS object has no section that can be represented by the physics core.
    #[error("{kind} {uid:?} contains no sections")]
    NoSections {
        /// CPACS object kind.
        kind: &'static str,
        /// UID of the object without sections.
        uid: String,
    },
    /// A section has no usable profile element.
    #[error("{kind} section {uid:?} contains no elements")]
    NoElements {
        /// CPACS section kind.
        kind: &'static str,
        /// UID of the section without elements.
        uid: String,
    },
    /// A section contains more profile elements than the native geometry can retain.
    #[error("{kind} section {uid:?} contains {count} elements; native geometry accepts one")]
    MultipleElements {
        /// CPACS section kind.
        kind: &'static str,
        /// UID of the section with multiple elements.
        uid: String,
        /// Number of CPACS elements present.
        count: usize,
    },
    /// A referenced CPACS profile is not available in the document.
    #[error("missing CPACS profile {profile_uid:?} referenced by {path}")]
    MissingProfile {
        /// CPACS location containing the unresolved reference.
        path: String,
        /// UID of the missing profile.
        profile_uid: String,
    },
    /// The current physics geometry cannot represent this CPACS symmetry.
    #[error("unsupported CPACS symmetry {symmetry:?} on wing {wing:?}")]
    UnsupportedSymmetry {
        /// Wing name carrying the unsupported declaration.
        wing: String,
        /// CPACS symmetry value.
        symmetry: String,
    },
    /// The current physics geometry cannot preserve this transformation.
    #[error("unsupported CPACS transformation at {path}: {reason}")]
    UnsupportedTransformation {
        /// CPACS location of the unsupported transform.
        path: String,
        /// Reason the native geometry cannot preserve it.
        reason: String,
    },
    /// A required finite, positive reference value is unavailable.
    #[error("invalid CPACS reference {name}: {value}")]
    InvalidReference {
        /// Reference quantity name.
        name: &'static str,
        /// Invalid value supplied by CPACS.
        value: f64,
    },
    /// The CPACS fuselage section cannot be reduced to the native ellipse model.
    #[error("fuselage section {section:?} uses a profile the physics geometry cannot represent")]
    UnsupportedFuselageProfile {
        /// Section UID using a non-elliptic profile.
        section: String,
    },
    /// A native geometry constructor rejected a CPACS section.
    #[error("could not construct native fuselage section {section:?}: {reason}")]
    FuselageSection {
        /// Section UID rejected by the native constructor.
        section: String,
        /// Native constructor error.
        reason: String,
    },
    /// A CPACS engine value cannot be used by the existing propulsion model.
    #[error("invalid CPACS engine value for {field}: {value}")]
    InvalidEngineValue {
        /// CPACS engine quantity name.
        field: &'static str,
        /// Invalid value supplied by CPACS.
        value: f64,
    },
}
