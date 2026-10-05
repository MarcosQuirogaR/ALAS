// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The optional shaped-nose fields of the forward fuselage (all unset = the
//! single-ellipsoid nose). Ranges are the valid ranges of
//! `alas_config::NoseShape`.

use super::{cfg, Bounds, Discipline, FieldKind, Spec, PROFILE};

pub(super) const WINDSHIELD_ANGLE: Spec = cfg(
    "geometry.fuselage.nose_windshield_angle_deg",
    Discipline::Fuselage,
    PROFILE,
    "/geometry/fuselage/nose_windshield_angle_deg",
    FieldKind::OptionalFloat,
    Bounds {
        min: 15.0,
        max: 60.0,
        decimals: 1,
    },
);

pub(super) const CROWN_END: Spec = cfg(
    "geometry.fuselage.nose_crown_end_fraction",
    Discipline::Fuselage,
    PROFILE,
    "/geometry/fuselage/nose_crown_end_fraction",
    FieldKind::OptionalFloat,
    Bounds {
        min: 0.55,
        max: 0.95,
        decimals: 2,
    },
);

pub(super) const RADOME_LENGTH: Spec = cfg(
    "geometry.fuselage.nose_radome_length_fraction",
    Discipline::Fuselage,
    PROFILE,
    "/geometry/fuselage/nose_radome_length_fraction",
    FieldKind::OptionalFloat,
    Bounds {
        min: 0.1,
        max: 0.45,
        decimals: 2,
    },
);

pub(super) const KEEL_EXPONENT: Spec = cfg(
    "geometry.fuselage.nose_keel_exponent",
    Discipline::Fuselage,
    PROFILE,
    "/geometry/fuselage/nose_keel_exponent",
    FieldKind::OptionalFloat,
    Bounds {
        min: 1.5,
        max: 4.0,
        decimals: 2,
    },
);

pub(super) const PLAN_EXPONENT: Spec = cfg(
    "geometry.fuselage.nose_plan_exponent",
    Discipline::Fuselage,
    PROFILE,
    "/geometry/fuselage/nose_plan_exponent",
    FieldKind::OptionalFloat,
    Bounds {
        min: 1.6,
        max: 2.6,
        decimals: 2,
    },
);

pub(super) const SECTION_EXPONENT: Spec = cfg(
    "geometry.fuselage.nose_section_exponent",
    Discipline::Fuselage,
    PROFILE,
    "/geometry/fuselage/nose_section_exponent",
    FieldKind::OptionalFloat,
    Bounds {
        min: 2.0,
        max: 3.5,
        decimals: 2,
    },
);
