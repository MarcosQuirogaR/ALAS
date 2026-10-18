// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! `MasterParams` wing -> lattice airplane and section models.
//!
//! Frame (`alas-geom`): x aft, y right, z up.  Each `WingPanel` is a straight
//! trapezoid from its root station to its tip: chord `c_root` to
//! `taper_ratio * c_root`, leading edge swept aft by `span tan(sweep_le)`,
//! raised by `span tan(dihedral)`, twisted nose-up by `twist_tip` over the
//! panel (twist accumulates from panel to panel).  The next panel starts at
//! the previous tip with the previous tip chord.
//!
//! # Spanwise stations
//!
//! The lattice induced drag of a rectangular wing converges slowly in the
//! number of strips (M2 lane A: `e` of an `A = 8` rectangle goes 0.82 -> 0.94
//! from 8 to 96 strips) because the tip loading falls to zero as a square
//! root.  [`build_wing`] therefore places the cross-sections itself, clustered
//! towards the wing tip like a half cosine (`y = (b/2) sin(pi k / 2n)`, the
//! classic lifting-line collocation), instead of letting the lattice subdivide
//! each panel.  The root is not clustered: the loading is smooth there.  Only
//! the geometry fed to the unchanged production lattice changes, so the
//! transport path is untouched.
//!
//! Winglets are not modelled; their presence is reported in
//! [`WingBuild::notes`].

use std::collections::BTreeMap;

use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::{Wing, WingXSec};
use alas_geom::airfoil_library::AirfoilLibrary;

use crate::aero::section::SectionModel;
use crate::spec::master::{MasterParams, WingPanel};

use super::CoupledError;

/// Blend resolution of intermediate airfoils (points per side).
const BLEND_POINTS: usize = 80;

/// The wing as the viscous VLM wants it.
pub struct WingBuild {
    /// Airplane with the main wing as `wings[0]`.
    pub airplane: Airplane,
    /// One section model per cross-section.
    pub sections: Vec<SectionModel>,
    /// Half-span cross-section stations `y`, m (root first).
    pub stations_y_m: Vec<f64>,
    /// Reference area (planform, both halves), m2.
    pub s_ref_m2: f64,
    /// Span, m.
    pub span_m: f64,
    /// Mean aerodynamic chord, m.
    pub mac_m: f64,
    /// Modelling limits that apply to this spec.
    pub notes: Vec<String>,
}

/// Resolve an airfoil name: UIUC corpus (hyphens ignored), then NACA 4-digit.
///
/// # Errors
///
/// [`CoupledError::Spec`] for an unknown name.
pub fn resolve_airfoil(name: &str) -> Result<Airfoil, CoupledError> {
    let compact: String = name.chars().filter(|c| *c != '-' && *c != ' ').collect();
    AirfoilLibrary::get(name)
        .or_else(|| AirfoilLibrary::get(&compact))
        .ok_or_else(|| CoupledError::Spec(format!("unknown airfoil '{name}'")))
}

struct Node {
    y: f64,
    x_le: f64,
    z: f64,
    chord: f64,
    twist_deg: f64,
    /// Airfoil pair of the host panel and the blend fraction towards the second.
    foil: (String, String, f64),
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + t * (b - a)
}

fn panel_nodes(
    p: &WingPanel,
    start: &Node,
    sub: usize,
    cluster_tip: bool,
) -> Result<Vec<Node>, CoupledError> {
    let ok = p.span_m > 0.0
        && p.taper_ratio > 0.0
        && p.sweep_le_deg.abs() < 80.0
        && p.dihedral_deg.abs() < 80.0;
    if !ok {
        return Err(CoupledError::Spec(format!(
            "wing panel out of range: {p:?}"
        )));
    }
    let tip_chord = start.chord * p.taper_ratio;
    let dx = p.span_m * p.sweep_le_deg.to_radians().tan();
    let dz = p.span_m * p.dihedral_deg.to_radians().tan();
    let mut out = Vec::with_capacity(sub);
    for k in 1..=sub {
        let f = k as f64 / sub as f64;
        let u = if cluster_tip {
            (0.5 * std::f64::consts::PI * f).sin()
        } else {
            f
        };
        out.push(Node {
            y: start.y + u * p.span_m,
            x_le: start.x_le + u * dx,
            z: start.z + u * dz,
            chord: lerp(start.chord, tip_chord, u),
            twist_deg: start.twist_deg + u * p.twist_tip_deg,
            foil: (p.airfoil_root.clone(), p.airfoil_tip.clone(), u),
        });
    }
    Ok(out)
}

/// Build the main wing of `master`, about `strips` lattice strips per half
/// span in total, with NeuralFoil sections at `n_crit`.
///
/// # Errors
///
/// [`CoupledError::Spec`] for out-of-range panels, unknown airfoils or
/// mismatching airfoils at a panel junction; [`CoupledError::Aero`] if a
/// section cannot be fitted.
pub fn build_wing(
    master: &MasterParams,
    n_crit: f64,
    strips: usize,
) -> Result<WingBuild, CoupledError> {
    if master.panels.is_empty() || master.root_chord_m <= 0.0 || strips == 0 {
        return Err(CoupledError::Spec(
            "a wing needs panels, a root chord and strips".into(),
        ));
    }
    let semispan: f64 = master.panels.iter().map(|p| p.span_m).sum();
    let mut notes = Vec::new();
    if master.winglet.is_some() {
        notes.push("winglet present in the spec but not modelled".to_owned());
    }
    let first = &master.panels[0];
    let mut nodes = vec![Node {
        y: 0.0,
        x_le: 0.0,
        z: 0.0,
        chord: master.root_chord_m,
        twist_deg: 0.0,
        foil: (first.airfoil_root.clone(), first.airfoil_root.clone(), 0.0),
    }];
    let last = master.panels.len() - 1;
    for (i, p) in master.panels.iter().enumerate() {
        if i > 0 && master.panels[i - 1].airfoil_tip != p.airfoil_root {
            return Err(CoupledError::Spec(format!(
                "airfoil mismatch at the junction of panels {} and {}: '{}' vs '{}'",
                i - 1,
                i,
                master.panels[i - 1].airfoil_tip,
                p.airfoil_root
            )));
        }
        let sub = ((strips as f64 * p.span_m / semispan).round() as usize).max(2);
        let start = nodes
            .last()
            .ok_or_else(|| CoupledError::Spec("internal: wing node list is empty".to_owned()))?;
        let next = panel_nodes(p, start, sub, i == last)?;
        nodes.extend(next);
    }
    // Airfoils and section models, memoised by label.
    let mut foils: BTreeMap<String, SectionModel> = BTreeMap::new();
    let mut xsecs = Vec::with_capacity(nodes.len());
    let mut sections = Vec::with_capacity(nodes.len());
    for n in &nodes {
        let (a, b, u) = (&n.foil.0, &n.foil.1, n.foil.2);
        let (airfoil, key) = if a == b || u <= 0.0 {
            (resolve_airfoil(a)?, a.clone())
        } else if u >= 1.0 - 1e-12 {
            (resolve_airfoil(b)?, b.clone())
        } else {
            let blended = resolve_airfoil(a)?
                .blend_with_another_airfoil(&resolve_airfoil(b)?, u, BLEND_POINTS)
                .map_err(|e| CoupledError::Spec(format!("airfoil blend {a}/{b}: {e:?}")))?;
            (blended, format!("{a}~{b}~{u:.6}"))
        };
        if !foils.contains_key(&key) {
            foils.insert(key.clone(), SectionModel::neuralfoil(&airfoil, n_crit)?);
        }
        let section = foils
            .get(&key)
            .cloned()
            .ok_or_else(|| CoupledError::Spec("internal: section memo".into()))?;
        let at = master.wing_xyz_m;
        xsecs.push(WingXSec::new(
            [at[0] + n.x_le, at[1] + n.y, at[2] + n.z],
            n.chord,
            n.twist_deg,
            airfoil,
        ));
        sections.push(section);
    }
    let wing = Wing::new("Main Wing", xsecs, true);
    let (s, b, c) = (
        wing.reference_area(),
        wing.reference_span(),
        wing.mean_aerodynamic_chord(),
    );
    let airplane = Airplane {
        name: "uav".into(),
        xyz_ref: [master.wing_xyz_m[0] + 0.25 * c, 0.0, master.wing_xyz_m[2]],
        s_ref: s,
        c_ref: c,
        b_ref: b,
        wings: vec![wing],
        fuselages: Vec::new(),
    };
    Ok(WingBuild {
        airplane,
        sections,
        stations_y_m: nodes.iter().map(|n| n.y).collect(),
        s_ref_m2: s,
        span_m: b,
        mac_m: c,
        notes,
    })
}
