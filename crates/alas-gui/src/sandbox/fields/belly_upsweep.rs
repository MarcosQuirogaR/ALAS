// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The optional belly-upsweep length field of the aft fuselage (m aft of the
//! nose tip to the start of the rising lower line; unset = tailcone loft).

use super::{cfg, Bounds, Discipline, FieldKind, Spec, STATIONS};

pub(super) const SPEC: Spec = cfg(
    "geometry.fuselage.belly_upsweep_length_m",
    Discipline::Fuselage,
    STATIONS,
    "/geometry/fuselage/belly_upsweep_length_m",
    FieldKind::OptionalFloat,
    Bounds {
        min: 1.0,
        max: 50.0,
        decimals: 2,
    },
);
