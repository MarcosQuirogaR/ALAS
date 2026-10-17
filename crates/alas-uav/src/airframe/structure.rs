// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Structural mass estimates from areas and areal densities (UAV-PLAN 3.7).
//!
//! Every item produced here is tagged [`MassBasis::Estimate`]: the areal
//! densities are inputs, not database facts, and the generic preset is an
//! order-of-magnitude figure for foam-core composite RC airframes that has NOT
//! been calibrated.  Calibrate with a team factor ([`MassStatement::calibrated`])
//! or override a part with a weighed mass (`Node::mass_override_kg`).
//!
//! Frame: body axes, x aft (see the [module root](super)).  The structural CG
//! of a lifting surface sits at `chord_fraction_cg` of the local chord
//! (ESTIMATE 0.40: spar and foam core dominate the mass).

use alas_uav_db::parts::LaminateSpec;

use super::geometry::{Planform, TailRole, TailSurfaceGeom};
use super::inertia::{Inertia, Vec3};
use super::mass::{MassBasis, MassGroup, MassItem};
use super::AirframeError;
use crate::spec::master::TailBoom;

/// Mass per unit area, kg/m^2.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArealDensity(pub f64);

impl ArealDensity {
    /// A validated areal density.
    ///
    /// # Errors
    ///
    /// Non-positive or non-finite value.
    pub fn new(kg_m2: f64) -> Result<Self, AirframeError> {
        if kg_m2.is_finite() && kg_m2 > 0.0 {
            Ok(Self(kg_m2))
        } else {
            Err(AirframeError::Invalid(format!(
                "areal density must be > 0, got {kg_m2}"
            )))
        }
    }

    /// Sum of laminate layers (`areal_mass_kg_m2` x ply count) plus a core, kg/m^2.
    ///
    /// # Errors
    ///
    /// A laminate without `areal_mass_kg_m2` (never defaulted) or a negative count.
    pub fn from_layup(
        layers: &[(&LaminateSpec, f64)],
        core_kg_m2: f64,
        what: &str,
    ) -> Result<Self, AirframeError> {
        let mut total = core_kg_m2;
        for (spec, n) in layers {
            let a = spec
                .areal_mass_kg_m2
                .as_ref()
                .ok_or_else(|| AirframeError::Missing {
                    what: what.to_owned(),
                    field: "areal_mass_kg_m2",
                })?;
            total += a.value * n;
        }
        Self::new(total)
    }

    /// Solid sheet of density `rho` (kg/m^3) and thickness `t` (m), kg/m^2.
    ///
    /// # Errors
    ///
    /// Non-positive inputs.
    pub fn from_sheet(rho_kg_m3: f64, thickness_m: f64) -> Result<Self, AirframeError> {
        Self::new(rho_kg_m3 * thickness_m)
    }
}

/// A fuselage modelled as a thin cylindrical shell along x (ESTIMATE shape).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FuselageShell {
    /// Nose end x, m.
    pub x_start_m: f64,
    /// Length, m.
    pub length_m: f64,
    /// Equivalent diameter, m.
    pub diameter_m: f64,
    /// Centre-line y, z, m.
    pub centre_yz_m: [f64; 2],
}

/// Inputs of the structural estimate.
#[derive(Debug, Clone, PartialEq)]
pub struct StructureInputs {
    /// Wing structure per m^2 of projected planform (skins, core, spars, ribs).
    pub wing: ArealDensity,
    /// Horizontal tail or V pair, per m^2 of true surface area.
    pub horizontal_tail: ArealDensity,
    /// Fin, per m^2.
    pub vertical_tail: ArealDensity,
    /// Fuselage shell per m^2 of lateral surface.
    pub fuselage: ArealDensity,
    /// Component build-up of the wing (`None`: use the areal density `wing`).
    /// The build-up carries its own `team_factor`.
    pub wing_buildup: Option<super::wing_buildup::WingBuildup>,
    /// Fuselage shape; `None` omits a fuselage item.
    pub fuselage_shell: Option<FuselageShell>,
    /// Chord fraction of the structural CG of lifting surfaces.
    pub chord_fraction_cg: f64,
    /// Boom tube density, kg/m^3 (CFRP tube ESTIMATE 1600).
    pub boom_density_kg_m3: f64,
    /// Fasteners, glue, covering as a fraction of structure mass (plan 3.7:
    /// 6 %, INFERENCE).
    pub allowance_fraction: f64,
    /// Spanwise strips per panel used to integrate mass and inertia.
    pub strips_per_panel: usize,
}

impl StructureInputs {
    /// Generic foam-core composite RC airframe preset.  ESTIMATE, uncalibrated:
    /// wing 0.9, tails 0.6, fuselage 0.5 kg/m^2.  Replace with laminate rows
    /// ([`ArealDensity::from_layup`]) or a weighed override as soon as data exist.
    #[must_use]
    pub fn generic_foam_composite() -> Self {
        Self {
            wing: ArealDensity(0.9),
            horizontal_tail: ArealDensity(0.6),
            vertical_tail: ArealDensity(0.6),
            fuselage: ArealDensity(0.5),
            wing_buildup: None,
            fuselage_shell: None,
            chord_fraction_cg: 0.40,
            boom_density_kg_m3: 1600.0,
            allowance_fraction: 0.06,
            strips_per_panel: 12,
        }
    }
}

/// Combine lumped items into one (mass, CG, inertia about the combined CG).
///
/// # Errors
///
/// Zero total mass.
pub fn combine(
    name: &str,
    group: MassGroup,
    basis: MassBasis,
    parts: &[MassItem],
) -> Result<MassItem, AirframeError> {
    let m: f64 = parts.iter().map(|p| p.mass_kg).sum();
    if !(m.is_finite() && m > 0.0) {
        return Err(AirframeError::Invalid(format!("{name}: zero mass")));
    }
    let mut cg = [0.0; 3];
    for p in parts {
        for (a, x) in cg.iter_mut().zip(p.cg_m) {
            *a += p.mass_kg * x / m;
        }
    }
    let inertia = parts.iter().fold(Inertia::ZERO, |acc, p| {
        let d = [p.cg_m[0] - cg[0], p.cg_m[1] - cg[1], p.cg_m[2] - cg[2]];
        acc.plus(p.inertia_cg.translated(p.mass_kg, d))
    });
    Ok(MassItem {
        name: name.to_owned(),
        group,
        mass_kg: m,
        cg_m: cg,
        inertia_cg: inertia,
        basis,
        inertia_modelled: parts.iter().all(|p| p.inertia_modelled),
    })
}

/// Structure item of a lifting surface: mass `= areal x area`, strip-wise
/// distribution, thin-plate inertia per strip.  `area_m2` is the area the
/// areal density applies to (projected for the wing, true for tails).
fn surface_item(
    name: &str,
    planform: &Planform,
    area_m2: f64,
    areal: ArealDensity,
    count: u32,
    inputs: &StructureInputs,
) -> Result<MassItem, AirframeError> {
    let strips = planform.strips(inputs.strips_per_panel, inputs.chord_fraction_cg);
    let projected = planform.projected_area_m2();
    let mut parts = Vec::with_capacity(strips.len());
    for s in &strips {
        let m = areal.0 * area_m2 * s.area_m2 / projected;
        let (c, w) = (s.chord_m, s.width_m);
        let i = if planform.vertical {
            Inertia::diagonal(
                m * w * w / 12.0,
                m * (c * c + w * w) / 12.0,
                m * c * c / 12.0,
            )
        } else {
            Inertia::diagonal(
                m * w * w / 12.0,
                m * c * c / 12.0,
                m * (c * c + w * w) / 12.0,
            )
        };
        parts.push(MassItem {
            name: name.to_owned(),
            group: MassGroup::Structure,
            mass_kg: m,
            cg_m: s.centroid_m,
            inertia_cg: i,
            basis: MassBasis::Estimate,
            inertia_modelled: true,
        });
    }
    let mut one = combine(name, MassGroup::Structure, MassBasis::Estimate, &parts)?;
    if count > 1 {
        // Identical fins on the planform's own y (the spec carries no twin-boom
        // spacing): mass and inertia scale with the count.  LIMITATION: roll
        // and yaw inertia of a twin-boom layout are under-estimated.
        let n = f64::from(count);
        one.mass_kg *= n;
        one.inertia_cg = one.inertia_cg.scaled(n);
    }
    Ok(one)
}

/// Wing structure item (projected-area basis).
///
/// # Errors
///
/// Degenerate planform.
pub fn wing_structure(
    planform: &Planform,
    inputs: &StructureInputs,
) -> Result<MassItem, AirframeError> {
    let area = planform.projected_area_m2();
    let areal = match &inputs.wing_buildup {
        Some(b) => ArealDensity::new(b.breakdown(planform)?.total_kg() / area)?,
        None => inputs.wing,
    };
    surface_item("wing structure", planform, area, areal, 1, inputs)
}

/// Tail structure item (true-area basis, all `count` surfaces).
///
/// # Errors
///
/// Degenerate planform.
pub fn tail_structure(
    tail: &TailSurfaceGeom,
    inputs: &StructureInputs,
) -> Result<MassItem, AirframeError> {
    let (name, areal) = match tail.role {
        TailRole::Vertical => ("vertical tail structure", inputs.vertical_tail),
        TailRole::Horizontal => ("horizontal tail structure", inputs.horizontal_tail),
        TailRole::VPair => ("v-tail structure", inputs.horizontal_tail),
    };
    surface_item(
        name,
        &tail.planform,
        tail.planform.true_area_m2(),
        areal,
        tail.count,
        inputs,
    )
}

/// Boom tube item: `rho pi (r_o^2 - r_i^2) L`, along x ending at the
/// empennage mount `x_end_m`, on `(y, z)`.  Inertia of a thin tube.
///
/// # Errors
///
/// Non-physical tube dimensions.
pub fn boom_structure(
    boom: &TailBoom,
    x_end_m: f64,
    yz_m: [f64; 2],
    inputs: &StructureInputs,
) -> Result<MassItem, AirframeError> {
    let (ro, t, l) = (0.5 * boom.outer_diameter_m, boom.wall_m, boom.length_m);
    if !(ro > 0.0 && t > 0.0 && t < ro && l > 0.0) {
        return Err(AirframeError::Invalid(
            "boom needs 0 < wall < outer radius and length > 0".into(),
        ));
    }
    let ri = ro - t;
    let m = inputs.boom_density_kg_m3 * std::f64::consts::PI * (ro * ro - ri * ri) * l;
    let rr = ro * ro + ri * ri;
    let inertia = Inertia::diagonal(
        0.5 * m * rr,
        m * (3.0 * rr + l * l) / 12.0,
        m * (3.0 * rr + l * l) / 12.0,
    );
    Ok(MassItem {
        name: "tail boom".into(),
        group: MassGroup::Structure,
        mass_kg: m,
        cg_m: [x_end_m - 0.5 * l, yz_m[0], yz_m[1]],
        inertia_cg: inertia,
        basis: MassBasis::Estimate,
        inertia_modelled: true,
    })
}

/// Fuselage shell item: lateral area `pi D L` times the areal density, thin
/// cylindrical shell inertia (Sadraey Table 11.11, Example 11.3).
///
/// # Errors
///
/// Non-positive dimensions.
pub fn fuselage_structure(
    shell: &FuselageShell,
    inputs: &StructureInputs,
) -> Result<MassItem, AirframeError> {
    if !(shell.length_m > 0.0 && shell.diameter_m > 0.0) {
        return Err(AirframeError::Invalid(
            "fuselage needs positive length and diameter".into(),
        ));
    }
    let area = std::f64::consts::PI * shell.diameter_m * shell.length_m;
    let m = inputs.fuselage.0 * area;
    let cg: Vec3 = [
        shell.x_start_m + 0.5 * shell.length_m,
        shell.centre_yz_m[0],
        shell.centre_yz_m[1],
    ];
    Ok(MassItem {
        name: "fuselage structure".into(),
        group: MassGroup::Structure,
        mass_kg: m,
        cg_m: cg,
        inertia_cg: Inertia::thin_shell_x(m, 0.5 * shell.diameter_m, shell.length_m),
        basis: MassBasis::Estimate,
        inertia_modelled: true,
    })
}
