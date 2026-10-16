// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The optional shaped-aft-body and nose-length fields of the fuselage (all
//! unset = the previous nose and tailcone laws): a geometry-only nose loft
//! length, the finite closing section at the tail end, the belly exponent and
//! the plan-taper and crown-drop lengths and exponents.

use super::{cfg, Bounds, Discipline, FieldKind, Spec, PROFILE, STATIONS};

pub(super) const NOSE_LENGTH: Spec = cfg(
    "geometry.fuselage.nose_length_m",
    Discipline::Fuselage,
    PROFILE,
    "/geometry/fuselage/nose_length_m",
    FieldKind::OptionalFloat,
    Bounds {
        min: 1.0,
        max: 30.0,
        decimals: 2,
    },
);

pub(super) const TAIL_END_Z: Spec = cfg(
    "geometry.fuselage.tail_end_z_m",
    Discipline::Fuselage,
    PROFILE,
    "/geometry/fuselage/tail_end_z_m",
    FieldKind::OptionalFloat,
    Bounds {
        min: -5.0,
        max: 5.0,
        decimals: 2,
    },
);

pub(super) const TAIL_END_WIDTH: Spec = cfg(
    "geometry.fuselage.tail_end_width_m",
    Discipline::Fuselage,
    PROFILE,
    "/geometry/fuselage/tail_end_width_m",
    FieldKind::OptionalFloat,
    Bounds {
        min: 0.0,
        max: 3.0,
        decimals: 2,
    },
);

pub(super) const TAIL_END_HEIGHT: Spec = cfg(
    "geometry.fuselage.tail_end_height_m",
    Discipline::Fuselage,
    PROFILE,
    "/geometry/fuselage/tail_end_height_m",
    FieldKind::OptionalFloat,
    Bounds {
        min: 0.0,
        max: 4.0,
        decimals: 2,
    },
);

pub(super) const BELLY_EXPONENT: Spec = cfg(
    "geometry.fuselage.aft_belly_exponent",
    Discipline::Fuselage,
    STATIONS,
    "/geometry/fuselage/aft_belly_exponent",
    FieldKind::OptionalFloat,
    Bounds {
        min: 1.0,
        max: 3.0,
        decimals: 2,
    },
);

pub(super) const PLAN_LENGTH: Spec = cfg(
    "geometry.fuselage.aft_plan_length_m",
    Discipline::Fuselage,
    STATIONS,
    "/geometry/fuselage/aft_plan_length_m",
    FieldKind::OptionalFloat,
    Bounds {
        min: 1.0,
        max: 50.0,
        decimals: 2,
    },
);

pub(super) const PLAN_EXPONENT: Spec = cfg(
    "geometry.fuselage.aft_plan_exponent",
    Discipline::Fuselage,
    PROFILE,
    "/geometry/fuselage/aft_plan_exponent",
    FieldKind::OptionalFloat,
    Bounds {
        min: 1.0,
        max: 3.0,
        decimals: 2,
    },
);

pub(super) const CROWN_LENGTH: Spec = cfg(
    "geometry.fuselage.aft_crown_length_m",
    Discipline::Fuselage,
    STATIONS,
    "/geometry/fuselage/aft_crown_length_m",
    FieldKind::OptionalFloat,
    Bounds {
        min: 1.0,
        max: 50.0,
        decimals: 2,
    },
);

pub(super) const CROWN_EXPONENT: Spec = cfg(
    "geometry.fuselage.aft_crown_exponent",
    Discipline::Fuselage,
    PROFILE,
    "/geometry/fuselage/aft_crown_exponent",
    FieldKind::OptionalFloat,
    Bounds {
        min: 1.0,
        max: 3.0,
        decimals: 2,
    },
);

pub(super) const TAILCONE_LENGTH: Spec = cfg(
    "geometry.fuselage.tailcone_length_m",
    Discipline::Fuselage,
    STATIONS,
    "/geometry/fuselage/tailcone_length_m",
    FieldKind::Float,
    Bounds {
        min: 1.0,
        max: 50.0,
        decimals: 2,
    },
);

pub(super) const HOLD_END: Spec = cfg(
    "geometry.fuselage.hold_interior_end_x_m",
    Discipline::Fuselage,
    STATIONS,
    "/geometry/fuselage/hold_interior_end_x_m",
    FieldKind::OptionalFloat,
    Bounds {
        min: 1.0,
        max: 100.0,
        decimals: 2,
    },
);
