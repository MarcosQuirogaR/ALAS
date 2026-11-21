// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Stiffness of the empennage support for the static aeroelastic correction of
//! the neutral point: the aft fuselage as a bending beam and the horizontal
//! tail as a box.
//!
//! Neither has a strength-sized structure in the library (the fuselage and the
//! tail are weighed by statistical equations, not sized), so both stiffnesses
//! are derived from the component mass the aircraft's own mass analysis
//! reports, by smearing it over the lofted surface:
//!
//! - **Fuselage.** The structural mass `m_f` spread over the wetted area `S`
//!   at the skin material density gives an equivalent shell thickness
//!   `t = m_f / (rho S)`; a thin circular shell of the local equal-area radius
//!   `R` has `EI_f = E pi R^3 t`. All the mass is credited as bending shell
//!   (frames, floors, doors and bulkheads included), so this is an **upper
//!   bound** of the fuselage stiffness and a lower bound of its flexibility.
//! - **Horizontal tail.** The semi-span mass spread as a uniform-gauge closed
//!   box (covers and two webs) between the outermost spar fractions of the
//!   wing box, depth `t/c` of the local chord; `EI` and `G J` follow as for the
//!   wing box (covers `cos L` as wide on the normal cut). Again every kilogram
//!   is credited to the box, an upper bound.

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;

use super::sizing_cause::{material, StructuralMaterialRole as Role};
use super::{main_wing, StructuralSizingCause as Cause, WingReconciliationError};

/// Fuselage and horizontal-tail stiffness tables.
#[derive(Debug, Clone, PartialEq)]
pub struct TailFlexibilityTable {
    /// Stations from the wing-body junction to the tail root, m.
    pub fuselage_x_m: Vec<f64>,
    /// Fuselage bending stiffness at those stations, N m^2.
    pub fuselage_ei_nm2: Vec<f64>,
    /// Horizontal-tail stations from the root, m.
    pub stabilizer_y_m: Vec<f64>,
    /// Spanwise-frame bending stiffness of the tail box, N m^2.
    pub stabilizer_ei_nm2: Vec<f64>,
    /// Torsional rigidity of the tail box (normal cut), N m^2.
    pub stabilizer_gj_nm2: Vec<f64>,
    /// Chord fraction of the tail elastic axis.
    pub stabilizer_axis_fraction: f64,
}

fn interp(xs: &[f64], ys: &[f64], at: f64) -> f64 {
    if at <= xs[0] {
        return ys[0];
    }
    let last = xs.len() - 1;
    if at >= xs[last] {
        return ys[last];
    }
    let upper = xs.partition_point(|&x| x <= at).clamp(1, last);
    let t = (at - xs[upper - 1]) / (xs[upper] - xs[upper - 1]);
    ys[upper - 1] + t * (ys[upper] - ys[upper - 1])
}

/// The fuselage and horizontal-tail stiffness of `plane`; see the module
/// documentation for what they are derived from.
///
/// # Errors
///
/// [`WingReconciliationError::StructuralSizing`] when the aircraft has no
/// fuselage, main wing or horizontal tail to describe, or its mass analysis
/// fails.
pub fn tail_flexibility(
    config: &AlasConfig,
    _design: &DesignVector,
    plane: &Airplane,
) -> Result<TailFlexibilityTable, WingReconciliationError> {
    let fail =
        |reason: &'static str| WingReconciliationError::from(Cause::TailFlexibility { reason });
    let model = config.analysis_mass_model(config.requirements.mtow_kg);
    let (masses, _, _) = crate::breakdown::run_mass_analysis_with_model_checked_product_with_gear(
        plane,
        &config.requirements,
        &config.geometry,
        &config.cabin,
        &config.control_surfaces,
        Some(&model),
        None,
        crate::breakdown::MassCoordinateModel::ReferenceCompatibility,
        &config.landing_gear,
    )
    .map_err(|_| fail("the aircraft mass buildup did not resolve"))?;
    let skin = material(&config.structures, Role::Skin)?;
    let (spar_fractions, _) = config.structures.resolved_spars();
    let front = spar_fractions.iter().copied().fold(f64::INFINITY, f64::min);
    let rear = spar_fractions
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    if !(front.is_finite() && rear.is_finite() && rear > front) {
        return Err(fail("the configured spar fractions do not bound a box"));
    }
    let axis = 0.5 * (front + rear);

    // Horizontal tail.
    let hstab = plane
        .wings
        .iter()
        .find(|w| w.name == "Horizontal Stabilizer")
        .filter(|w| w.xsecs.len() >= 2)
        .ok_or_else(|| fail("no horizontal stabilizer with two or more sections"))?;
    let root = &hstab.xsecs[0];
    let tip = &hstab.xsecs[hstab.xsecs.len() - 1];
    let (y_root, y_tip) = (root.xyz_le[1].abs(), tip.xyz_le[1].abs());
    if y_tip <= y_root || masses.h_stab <= 0.0 {
        return Err(fail(
            "the horizontal stabilizer has no positive span or mass",
        ));
    }
    let sample: Vec<f64> = (0..=20).map(|i| f64::from(i) / 20.0).collect();
    let thickness: Vec<f64> = hstab
        .xsecs
        .iter()
        .map(|xsec| xsec.airfoil.max_thickness(&sample))
        .collect();
    let xsec_y: Vec<f64> = hstab.xsecs.iter().map(|x| x.xyz_le[1].abs()).collect();
    let xsec_c: Vec<f64> = hstab.xsecs.iter().map(|x| x.chord).collect();
    let cos_l = hstab.mean_sweep_angle(axis).to_radians().cos().max(1.0e-3);
    const STATIONS: usize = 9;
    let y: Vec<f64> = (0..STATIONS)
        .map(|i| y_root + (y_tip - y_root) * i as f64 / (STATIONS - 1) as f64)
        .collect();
    let width = |y: f64| (rear - front) * interp(&xsec_y, &xsec_c, y) * cos_l;
    let depth = |y: f64| interp(&xsec_y, &thickness, y) * interp(&xsec_y, &xsec_c, y);
    // Uniform gauge `t`: semi-span mass = rho t int (2 w + 2 h) / cos L dy.
    let wetted_per_span: Vec<f64> = y
        .iter()
        .map(|&s| (2.0 * width(s) + 2.0 * depth(s)) / cos_l)
        .collect();
    let integral: f64 = (1..STATIONS)
        .map(|k| 0.5 * (wetted_per_span[k - 1] + wetted_per_span[k]) * (y[k] - y[k - 1]))
        .sum();
    let gauge = 0.5 * masses.h_stab / (skin.rho_kg_m3 * integral);
    let ei: Vec<f64> = y
        .iter()
        .map(|&s| {
            let (w, h) = (width(s), depth(s));
            let normal = skin.e_pa * gauge * (w * h * h / 2.0 + h * h * h / 6.0);
            normal * cos_l.powi(3)
        })
        .collect();
    let gj: Vec<f64> = y
        .iter()
        .map(|&s| {
            let (w, h) = (width(s), depth(s));
            4.0 * (w * h).powi(2) * skin.g_pa() * gauge / (2.0 * w + 2.0 * h)
        })
        .collect();

    // Aft fuselage between the wing-body junction (rear spar of the wing root)
    // and the tail root (its elastic axis).
    let wing = main_wing(plane).ok_or(Cause::MissingMainWing)?;
    let count = wing.xsecs.len();
    let wing_root = wing
        .xsecs
        .first()
        .ok_or(Cause::TooFewCrossSections { count })?;
    let x_junction = wing_root.xyz_le[0] + rear * wing_root.chord;
    let x_tail = root.xyz_le[0] + axis * root.chord;
    let fuselage = plane
        .fuselages
        .first()
        .ok_or_else(|| fail("the aircraft has no fuselage"))?;
    if x_tail <= x_junction || fuselage.xsecs.len() < 2 || masses.fuselage <= 0.0 {
        return Err(fail(
            "the tail root is not aft of the wing-body junction, or the fuselage has \
             fewer than two sections or no mass",
        ));
    }
    let fx: Vec<f64> = fuselage.xsecs.iter().map(|s| s.xyz_c[0]).collect();
    let radius: Vec<f64> = fuselage
        .xsecs
        .iter()
        .map(|s| 0.5 * (s.width * s.height).max(0.0).sqrt())
        .collect();
    if !fx.windows(2).all(|p| p[1] > p[0]) {
        return Err(fail("the fuselage sections do not advance aft"));
    }
    let shell = masses.fuselage / (skin.rho_kg_m3 * fuselage.area_wetted());
    const FUSELAGE_STATIONS: usize = 49;
    let fuselage_x_m: Vec<f64> = (0..FUSELAGE_STATIONS)
        .map(|i| x_junction + (x_tail - x_junction) * i as f64 / (FUSELAGE_STATIONS - 1) as f64)
        .collect();
    let fuselage_ei_nm2: Vec<f64> = fuselage_x_m
        .iter()
        .map(|&x| skin.e_pa * std::f64::consts::PI * interp(&fx, &radius, x).powi(3) * shell)
        .collect();
    Ok(TailFlexibilityTable {
        fuselage_x_m,
        fuselage_ei_nm2,
        stabilizer_y_m: y,
        stabilizer_ei_nm2: ei,
        stabilizer_gj_nm2: gj,
        stabilizer_axis_fraction: axis,
    })
}
