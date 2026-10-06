// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The vertical offsets of the fuselage centreline at the nose tip, the
//! cabin and the tail tip.

use super::{cfg, Bounds, Discipline, FieldKind, Spec, PROFILE};

pub(super) const NOSE_Z: Spec = cfg(
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
);

pub(super) const CABIN_Z: Spec = cfg(
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
);

pub(super) const TAIL_Z: Spec = cfg(
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
);
