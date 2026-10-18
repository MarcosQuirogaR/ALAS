// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Absolute tail placement and dorsal-fin fields of the empennage.
//!
//! Positions are metres aft of the nose tip (x) or from the centreline (y).
//! Ranges are the sandbox editing domain; the geometry builder validates the
//! station ordering (for example the root station must lie inside the
//! tailplane half-span).

use super::{cfg, Bounds, Discipline, FieldKind, Spec};

/// The panel group shared with the other tail placement fields.
const PLACEMENT: &str = "Placement";

const X_FROM_NOSE_M: Bounds = Bounds {
    min: 0.0,
    max: 120.0,
    decimals: 2,
};

pub(super) const HSTAB_ROOT_LE_X: Spec = cfg(
    "geometry.empennage.hstab_root_le_x_m",
    Discipline::HorizontalTail,
    PLACEMENT,
    "/geometry/empennage/hstab_root_le_x_m",
    FieldKind::OptionalFloat,
    X_FROM_NOSE_M,
);

pub(super) const HSTAB_ROOT_STATION_Y: Spec = cfg(
    "geometry.empennage.hstab_root_station_y_m",
    Discipline::HorizontalTail,
    PLACEMENT,
    "/geometry/empennage/hstab_root_station_y_m",
    FieldKind::OptionalFloat,
    Bounds {
        min: 0.0,
        max: 20.0,
        decimals: 2,
    },
);

pub(super) const VSTAB_ROOT_LE_X: Spec = cfg(
    "geometry.empennage.vstab_root_le_x_m",
    Discipline::VerticalTail,
    PLACEMENT,
    "/geometry/empennage/vstab_root_le_x_m",
    FieldKind::OptionalFloat,
    X_FROM_NOSE_M,
);

pub(super) const DORSAL_X_START: Spec = cfg(
    "geometry.empennage.vstab_dorsal_x_start_m",
    Discipline::VerticalTail,
    PLACEMENT,
    "/geometry/empennage/vstab_dorsal_x_start_m",
    FieldKind::OptionalFloat,
    X_FROM_NOSE_M,
);

pub(super) const DORSAL_HEIGHT: Spec = cfg(
    "geometry.empennage.vstab_dorsal_height_m",
    Discipline::VerticalTail,
    PLACEMENT,
    "/geometry/empennage/vstab_dorsal_height_m",
    FieldKind::OptionalFloat,
    Bounds {
        min: 0.0,
        max: 10.0,
        decimals: 2,
    },
);

pub(super) const DORSAL_IN_AERO: Spec = cfg(
    "geometry.empennage.vstab_dorsal_in_aero",
    Discipline::VerticalTail,
    PLACEMENT,
    "/geometry/empennage/vstab_dorsal_in_aero",
    FieldKind::Bool,
    Bounds {
        min: 0.0,
        max: 1.0,
        decimals: 0,
    },
);
