// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What a required rotation (tail-strike) attitude rests on: the source and
//! the main-gear state it was published for. It decides which model attitude
//! the tail-scrape check compares with and whether a miss rejects the design.

use serde::{Deserialize, Serialize};

use crate::{Kind, Leaf};

/// The basis of `LandingGearConfig::required_rotation_angle_deg`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RotationAngleBasis {
    /// No published value: a configured design-practice assumption. A miss
    /// is reported as a warning, never a rejection.
    #[default]
    Assumed,
    /// A published attitude with the main-gear struts compressed (static):
    /// compared with the model's static attitude; a miss rejects the design.
    PublishedStatic,
    /// A published attitude with the struts extended (wheels on the runway at
    /// lift-off): compared with the model's gear-extended attitude; a miss
    /// rejects the design.
    PublishedGearExtended,
    /// A published attitude whose gear state the source does not give:
    /// compared with the model's gear-extended attitude, the larger of the
    /// two, and a miss is a warning, since the states may not match.
    PublishedStateUnspecified,
}

impl RotationAngleBasis {
    /// Stable serialized names.
    pub const OPTIONS: &'static [&'static str] = &[
        "assumed",
        "published_static",
        "published_gear_extended",
        "published_state_unspecified",
    ];

    /// Stable serialized name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Assumed => "assumed",
            Self::PublishedStatic => "published_static",
            Self::PublishedGearExtended => "published_gear_extended",
            Self::PublishedStateUnspecified => "published_state_unspecified",
        }
    }

    /// Whether the model attitude is measured with the main-gear oleos at
    /// their lift-off extension rather than static.
    pub const fn gear_extended(self) -> bool {
        matches!(
            self,
            Self::PublishedGearExtended | Self::PublishedStateUnspecified
        )
    }

    /// Whether a miss rejects the design: only a published value compared in
    /// its own gear state.
    pub const fn rejects(self) -> bool {
        matches!(self, Self::PublishedStatic | Self::PublishedGearExtended)
    }
}

impl RotationAngleBasis {
    /// Serde skip predicate: the default is not written out.
    pub fn is_assumed(&self) -> bool {
        *self == Self::Assumed
    }
}

impl Leaf for RotationAngleBasis {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Str
    }
}
