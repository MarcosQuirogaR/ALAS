// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Structural stock, laminates and kits.
//!
//! Prices never appear here (rule: price is never inside a spec row); the
//! price per kilogram or per metre of stock is an `Offer` with a price basis.

use serde::{Deserialize, Serialize};

use crate::field::{Field, Num};
use crate::ids::ComponentId;

/// Isotropic or nominal engineering material.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialSpec {
    /// Form text (sheet, tube, rod, foam board, filament, ...).
    pub form: Option<Field<String>>,
    /// Density, kg/m3.
    pub density_kg_m3: Option<Num>,
    /// Young's modulus, Pa.
    pub e_pa: Option<Num>,
    /// Shear modulus, Pa.
    pub g_pa: Option<Num>,
    /// Failure or allowable strength, Pa (state which in `raw`).
    pub strength_pa: Option<Num>,
    /// Purchased stock mass, kg.
    pub stock_mass_kg: Option<Num>,
    /// Purchased stock bounding box length, width, height, m.
    pub stock_dims_m: Option<Field<[f64; 3]>>,
}

/// A composite laminate stock (prepreg, fabric, UD tape) for CLT sizing.
///
/// Ply properties are lamina-level (1 = fibre, 2 = transverse, 12 = in-plane
/// shear). `knockdown` is the design reduction from the coupon programme and
/// stays unset until `coupon_source` exists.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaminateSpec {
    /// Cured ply thickness, m.
    pub ply_thickness_m: Option<Num>,
    /// Areal mass of one dry or cured ply, kg/m2.
    pub areal_mass_kg_m2: Option<Num>,
    /// Fibre-direction modulus E1, Pa.
    pub ply_e1_pa: Option<Num>,
    /// Transverse modulus E2, Pa.
    pub ply_e2_pa: Option<Num>,
    /// In-plane shear modulus G12, Pa.
    pub ply_g12_pa: Option<Num>,
    /// Major Poisson ratio nu12 (1).
    pub ply_nu12: Option<Num>,
    /// Longitudinal tensile strength Xt, Pa.
    pub ply_xt_pa: Option<Num>,
    /// Longitudinal compressive strength Xc, Pa (positive magnitude).
    pub ply_xc_pa: Option<Num>,
    /// Transverse tensile strength Yt, Pa.
    pub ply_yt_pa: Option<Num>,
    /// Transverse compressive strength Yc, Pa (positive magnitude).
    pub ply_yc_pa: Option<Num>,
    /// In-plane shear strength S12, Pa.
    pub ply_s12_pa: Option<Num>,
    /// Design knock-down factor in (0, 1] from the coupon programme.
    pub knockdown: Option<Num>,
    /// Identifier text of the coupon campaign that produced `knockdown`.
    pub coupon_source: Option<Field<String>>,
}

/// A purchased kit (airframe kit or ARF) composed of other components.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KitSpec {
    /// Component ids of the parts the kit contains.
    #[serde(default)]
    pub parts: Vec<ComponentId>,
    /// Kit mass, kg.
    pub mass_kg: Option<Num>,
    /// Wing span, m.
    pub span_m: Option<Num>,
    /// Wing area, m2.
    pub area_m2: Option<Num>,
    /// Intended all-up weight, kg.
    pub intended_auw_kg: Option<Num>,
}
