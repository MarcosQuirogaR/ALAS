// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The optional upper-deck hump and upper-deck fields of the fuselage (all
//! unset = constant crown and one deck). Ranges are the sandbox editing
//! domain; `alas_config::FuselageConfig::upper_deck_hump` also requires the
//! stations to be ordered, and ignores an inconsistent set.

use super::{cfg, Bounds, Discipline, FieldKind, Spec};

/// The panel group of the hump and upper-deck fields.
const UPPER_DECK: &str = "Upper deck";

pub(super) const HEIGHT: Spec = cfg(
    "geometry.fuselage.hump_height_m",
    Discipline::Fuselage,
    UPPER_DECK,
    "/geometry/fuselage/hump_height_m",
    FieldKind::OptionalFloat,
    Bounds {
        min: 0.1,
        max: 4.0,
        decimals: 2,
    },
);

pub(super) const START: Spec = cfg(
    "geometry.fuselage.hump_start_x_m",
    Discipline::Fuselage,
    UPPER_DECK,
    "/geometry/fuselage/hump_start_x_m",
    FieldKind::OptionalFloat,
    Bounds {
        min: 0.0,
        max: 60.0,
        decimals: 2,
    },
);

pub(super) const CROWN_START: Spec = cfg(
    "geometry.fuselage.hump_crown_start_x_m",
    Discipline::Fuselage,
    UPPER_DECK,
    "/geometry/fuselage/hump_crown_start_x_m",
    FieldKind::OptionalFloat,
    Bounds {
        min: 0.0,
        max: 60.0,
        decimals: 2,
    },
);

pub(super) const CROWN_END: Spec = cfg(
    "geometry.fuselage.hump_crown_end_x_m",
    Discipline::Fuselage,
    UPPER_DECK,
    "/geometry/fuselage/hump_crown_end_x_m",
    FieldKind::OptionalFloat,
    Bounds {
        min: 0.0,
        max: 60.0,
        decimals: 2,
    },
);

pub(super) const END: Spec = cfg(
    "geometry.fuselage.hump_end_x_m",
    Discipline::Fuselage,
    UPPER_DECK,
    "/geometry/fuselage/hump_end_x_m",
    FieldKind::OptionalFloat,
    Bounds {
        min: 0.0,
        max: 60.0,
        decimals: 2,
    },
);

pub(super) const FAIRING_EXPONENT: Spec = cfg(
    "geometry.fuselage.hump_fairing_exponent",
    Discipline::Fuselage,
    UPPER_DECK,
    "/geometry/fuselage/hump_fairing_exponent",
    FieldKind::OptionalFloat,
    Bounds {
        min: 1.0,
        max: 4.0,
        decimals: 2,
    },
);

pub(super) const FLOOR_HEIGHT: Spec = cfg(
    "geometry.fuselage.upper_deck_floor_height_m",
    Discipline::Fuselage,
    UPPER_DECK,
    "/geometry/fuselage/upper_deck_floor_height_m",
    FieldKind::OptionalFloat,
    Bounds {
        min: 1.5,
        max: 4.0,
        decimals: 2,
    },
);

pub(super) const DECK_START: Spec = cfg(
    "geometry.fuselage.upper_deck_start_x_m",
    Discipline::Fuselage,
    UPPER_DECK,
    "/geometry/fuselage/upper_deck_start_x_m",
    FieldKind::OptionalFloat,
    Bounds {
        min: 0.0,
        max: 60.0,
        decimals: 2,
    },
);

pub(super) const DECK_END: Spec = cfg(
    "geometry.fuselage.upper_deck_end_x_m",
    Discipline::Fuselage,
    UPPER_DECK,
    "/geometry/fuselage/upper_deck_end_x_m",
    FieldKind::OptionalFloat,
    Bounds {
        min: 0.0,
        max: 60.0,
        decimals: 2,
    },
);
