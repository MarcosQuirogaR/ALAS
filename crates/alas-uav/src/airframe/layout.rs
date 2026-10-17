// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Construction of the lifting-surface geometry from the master parameters:
//! wing panels, explicit tails and tails driven by volume coefficients.

use crate::spec::master::{EmpennageDriving, MasterParams, TailSurface};
use crate::spec::topology::{TailKind, Topology};

use super::geometry::{AirframeGeometry, Panel, Planform, TailRole, TailSurfaceGeom};
use super::inertia::Vec3;
use super::AirframeError;

/// Defaults for tail shape in `Driven` mode: ESTIMATES used only to turn an
/// area into a planform (the area itself comes from the volume coefficient).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TailShape {
    /// Horizontal-tail aspect ratio (Raymer table 6.5 range is 3 to 5).
    pub ar_horizontal: f64,
    /// Horizontal-tail taper ratio.
    pub taper_horizontal: f64,
    /// Fin aspect ratio (single fin).
    pub ar_vertical: f64,
    /// Fin taper ratio.
    pub taper_vertical: f64,
}

impl Default for TailShape {
    fn default() -> Self {
        Self {
            ar_horizontal: 4.0,
            taper_horizontal: 0.7,
            ar_vertical: 1.5,
            taper_vertical: 0.6,
        }
    }
}

fn positive(v: f64, what: &str) -> Result<f64, AirframeError> {
    if v.is_finite() && v > 0.0 {
        Ok(v)
    } else {
        Err(AirframeError::Invalid(format!(
            "{what} must be > 0, got {v}"
        )))
    }
}

/// Planform from an area, aspect ratio and taper ratio (zero sweep unless
/// given), root leading edge at `origin`.
///
/// # Errors
///
/// Non-positive area, aspect ratio or taper.
pub fn planform_from_area(
    area_m2: f64,
    ar: f64,
    taper: f64,
    sweep_le_deg: f64,
    dihedral_deg: f64,
    origin: Vec3,
    vertical: bool,
) -> Result<Planform, AirframeError> {
    positive(area_m2, "tail area")?;
    positive(ar, "tail aspect ratio")?;
    positive(taper, "tail taper ratio")?;
    // A mirrored surface has total span b with S = b^2/AR; a fin has height b.
    let b = (ar * area_m2).sqrt();
    let c_root = 2.0 * area_m2 / (b * (1.0 + taper));
    let half = if vertical { b } else { 0.5 * b };
    let dih = dihedral_deg.to_radians();
    // A V pair is specified by lateral projection; true area is S, so project.
    let half_y = if vertical { half } else { half * dih.cos() };
    Ok(Planform {
        panels: vec![Panel {
            span_m: half_y,
            chord_root_m: c_root,
            chord_tip_m: c_root * taper,
            root_le_m: origin,
            sweep_le_rad: sweep_le_deg.to_radians(),
            dihedral_rad: if vertical { 0.0 } else { dih },
        }],
        mirror: !vertical,
        vertical,
    })
}

fn explicit_planform(
    s: &TailSurface,
    origin: Vec3,
    vertical: bool,
    dihedral_deg: f64,
) -> Result<Planform, AirframeError> {
    positive(s.span_m, "tail span")?;
    positive(s.root_chord_m, "tail root chord")?;
    positive(s.taper_ratio, "tail taper ratio")?;
    Ok(Planform {
        panels: vec![Panel {
            // `TailSurface::span_m` is measured ALONG the surface: the lateral
            // projection of a dihedral pair is smaller by cos(dihedral).
            span_m: if vertical {
                s.span_m
            } else {
                0.5 * s.span_m * dihedral_deg.to_radians().cos()
            },
            chord_root_m: s.root_chord_m,
            chord_tip_m: s.root_chord_m * s.taper_ratio,
            root_le_m: origin,
            sweep_le_rad: s.sweep_le_deg.to_radians(),
            dihedral_rad: if vertical {
                0.0
            } else {
                dihedral_deg.to_radians()
            },
        }],
        mirror: !vertical,
        vertical,
    })
}

/// The wing planform of a master parameter set (projected panels, mirrored).
///
/// # Errors
///
/// A non-positive root chord, panel span or taper ratio.
pub fn wing_planform(master: &MasterParams) -> Result<Planform, AirframeError> {
    positive(master.root_chord_m, "root_chord_m")?;
    let mut panels = Vec::with_capacity(master.panels.len());
    let mut root_le = master.wing_xyz_m;
    let mut c_root = master.root_chord_m;
    for p in &master.panels {
        positive(p.span_m, "panel span")?;
        positive(p.taper_ratio, "panel taper")?;
        let c_tip = c_root * p.taper_ratio;
        let panel = Panel {
            span_m: p.span_m,
            chord_root_m: c_root,
            chord_tip_m: c_tip,
            root_le_m: root_le,
            sweep_le_rad: p.sweep_le_deg.to_radians(),
            dihedral_rad: p.dihedral_deg.to_radians(),
        };
        root_le = [
            root_le[0] + p.span_m * panel.sweep_le_rad.tan(),
            root_le[1] + p.span_m,
            root_le[2] + p.span_m * panel.dihedral_rad.tan(),
        ];
        c_root = c_tip;
        panels.push(panel);
    }
    if panels.is_empty() {
        return Err(AirframeError::Invalid(
            "wing needs at least one panel".into(),
        ));
    }
    Ok(Planform {
        panels,
        mirror: true,
        vertical: false,
    })
}

impl AirframeGeometry {
    /// Build the geometry from the master parameters and topology.  In `Driven`
    /// mode the tail areas follow the volume coefficients and arm; `shape`
    /// supplies the planform proportions.
    ///
    /// # Errors
    ///
    /// Non-physical dimensions.
    pub fn from_master(
        master: &MasterParams,
        topology: &Topology,
        shape: &TailShape,
    ) -> Result<Self, AirframeError> {
        let wing = wing_planform(master)?;
        let s_w = wing.projected_area_m2();
        let c_w = wing.mac_m();
        let b_w = wing.span_m();
        let x_acw = wing.x_le_mac_m() + 0.25 * c_w;
        let kind = topology.tail;
        let gamma_deg = match kind {
            TailKind::V { dihedral_deg } => dihedral_deg,
            TailKind::InvV { dihedral_deg } => -dihedral_deg.abs(),
            _ => 0.0,
        };
        let origin = master.tail_xyz_m;
        let (horizontal, vertical) = match (&master.empennage, kind) {
            (_, TailKind::None) => (None, None),
            (
                EmpennageDriving::Driven {
                    horizontal_volume,
                    vertical_volume,
                    arm_m,
                },
                _,
            ) => {
                positive(*arm_m, "tail arm")?;
                let s_h = horizontal_volume * c_w * s_w / arm_m;
                let s_v = vertical_volume * b_w * s_w / arm_m;
                let x_target = x_acw + arm_m;
                driven_surfaces(kind, gamma_deg, s_h, s_v, shape, origin, x_target)?
            }
            (
                EmpennageDriving::Explicit {
                    horizontal,
                    vertical,
                },
                _,
            ) => {
                let h = horizontal
                    .as_ref()
                    .map(|s| {
                        let pf = explicit_planform(s, origin, false, gamma_deg)?;
                        Ok::<_, AirframeError>(surface(
                            pf,
                            kind,
                            gamma_deg,
                            s.incidence_deg.to_radians(),
                            1,
                        ))
                    })
                    .transpose()?;
                let v = vertical
                    .as_ref()
                    .map(|s| {
                        let count = if matches!(kind, TailKind::TwinBoom) {
                            2
                        } else {
                            1
                        };
                        let pf = explicit_planform(s, origin, true, 0.0)?;
                        Ok::<_, AirframeError>(TailSurfaceGeom {
                            role: TailRole::Vertical,
                            planform: pf,
                            v_dihedral_rad: 0.0,
                            incidence_rad: 0.0,
                            count,
                        })
                    })
                    .transpose()?;
                (h, v)
            }
        };
        Ok(Self {
            wing,
            horizontal,
            vertical,
            tail_kind: kind,
        })
    }
}

fn surface(
    planform: Planform,
    kind: TailKind,
    gamma_deg: f64,
    incidence_rad: f64,
    count: u32,
) -> TailSurfaceGeom {
    let v = matches!(kind, TailKind::V { .. } | TailKind::InvV { .. });
    TailSurfaceGeom {
        role: if v {
            TailRole::VPair
        } else {
            TailRole::Horizontal
        },
        planform,
        v_dihedral_rad: if v { gamma_deg.to_radians() } else { 0.0 },
        incidence_rad,
        count,
    }
}

fn driven_surfaces(
    kind: TailKind,
    gamma_deg: f64,
    s_h: f64,
    s_v: f64,
    shape: &TailShape,
    origin: Vec3,
    x_ac_target: f64,
) -> Result<(Option<TailSurfaceGeom>, Option<TailSurfaceGeom>), AirframeError> {
    let place = |pf: Planform| {
        let ac = pf.aerodynamic_centre_m();
        pf.shifted_x(x_ac_target - ac[0])
    };
    let v_pair = matches!(kind, TailKind::V { .. } | TailKind::InvV { .. });
    if v_pair {
        // Required effective areas S cos^2(G) >= s_h and S sin^2(G) >= s_v.
        let g = gamma_deg.to_radians();
        let need_h = s_h / g.cos().powi(2);
        let need_v = if g.sin().abs() > 1e-6 {
            s_v / g.sin().powi(2)
        } else {
            0.0
        };
        let area = need_h.max(need_v);
        let pf = planform_from_area(
            area,
            shape.ar_horizontal,
            shape.taper_horizontal,
            0.0,
            gamma_deg,
            origin,
            false,
        )?;
        return Ok((Some(surface(place(pf), kind, gamma_deg, 0.0, 1)), None));
    }
    let h = planform_from_area(
        s_h,
        shape.ar_horizontal,
        shape.taper_horizontal,
        0.0,
        0.0,
        origin,
        false,
    )?;
    let count = if matches!(kind, TailKind::TwinBoom) {
        2
    } else {
        1
    };
    let fin_area = s_v / f64::from(count);
    let v = planform_from_area(
        fin_area,
        shape.ar_vertical,
        shape.taper_vertical,
        0.0,
        0.0,
        origin,
        true,
    )?;
    let v = TailSurfaceGeom {
        role: TailRole::Vertical,
        planform: place(v),
        v_dihedral_rad: 0.0,
        incidence_rad: 0.0,
        count,
    };
    Ok((Some(surface(place(h), kind, 0.0, 0.0, 1)), Some(v)))
}
