// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The main-gear sponsons of a high wing (m). They are not lofted on the
//! built body; the three lengths give the parasite build-up their wetted
//! area. Unset on a body without sponsons.

use super::{cfg, Bounds, Discipline, FieldKind, Spec, BODY};

pub(super) const LENGTH: Spec = cfg(
    "geometry.fuselage.gear_sponson_length_m",
    Discipline::Fuselage,
    BODY,
    "/geometry/fuselage/gear_sponson_length_m",
    FieldKind::OptionalFloat,
    Bounds {
        min: 0.5,
        max: 20.0,
        decimals: 2,
    },
);

pub(super) const HALF_WIDTH: Spec = cfg(
    "geometry.fuselage.gear_sponson_half_width_m",
    Discipline::Fuselage,
    BODY,
    "/geometry/fuselage/gear_sponson_half_width_m",
    FieldKind::OptionalFloat,
    Bounds {
        min: 0.3,
        max: 6.0,
        decimals: 2,
    },
);

pub(super) const DEPTH: Spec = cfg(
    "geometry.fuselage.gear_sponson_depth_m",
    Discipline::Fuselage,
    BODY,
    "/geometry/fuselage/gear_sponson_depth_m",
    FieldKind::OptionalFloat,
    Bounds {
        min: 0.05,
        max: 3.0,
        decimals: 2,
    },
);
