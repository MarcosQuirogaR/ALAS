// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Stiffened cover panels of the product deck and their stress along the
//! stringers.
//!
//! The deck idealises each wing-box cover as the native section does: a skin
//! sheet of the sized thickness between the spars and the flanges and
//! stringers lumped into the spar-cap bars (`crate::sizing::panel`). The
//! compressive allowable of such a cover is a panel allowable. It is the
//! failure stress of the skin-stringer section, defined on the end load per
//! unit width spread over the whole smeared section, `f = N / T` with
//! `T = (t b + A_s) / b` (M. C. Y. Niu, *Airframe Structural Design*, Conmilit
//! Press, 1988, Sec. 5.6, p. 148; crippling cut-off of an extruded 7075-T6
//! Z-stringer, Sec. 5.3 example, p. 136; Torenbeek, *Advanced Aircraft
//! Design*, 2013, Eq. 11.71, p. 351). Torenbeek "butters" the stringers and
//! spar flanges into the skin to form that equivalent thickness (Sec. 11.2.2,
//! pp. 323-324).
//!
//! A point of the skin sheet is therefore not what the 380 MPa allowable
//! limits. Each rib bay of each cover is assessed as one panel: the axial
//! load along the stringers of every skin shell of the bay (stress along the
//! stringer direction times thickness times width across the stringers) plus
//! that of the spar caps on the same surface, over the panel's axial
//! stiffness. The result is the panel's mean stress in skin-material terms,
//! `E_skin N / sum(E A)`, which is `N / A` for a single material and keeps the
//! strain basis of a composite cover. The skin sheet's own von Mises is
//! limited separately by its CS 25.305 combined-stress limit
//! ([`crate::allowables::shell_von_mises_allowable_pa`]).
//!
//! The stringers run along the bay's spanwise grid lines, the constant chord
//! fraction lines the spars also follow. Each solver reports shell stresses in
//! its own element axes: MSC Nastran places the CQUAD4 x-axis on the bisector
//! of the diagonals and the CTRIA3 x-axis on G1-G2; NASTRAN-95 uses G1-G2 for
//! both (checked against the two solvers' A320 output: an element whose
//! bisector is 5.5 deg off G1-G2 gives the same tensor rotated by 5.5 deg).

use std::collections::{HashMap, HashSet};

use super::cards::Deck;

/// Element x-axis convention of a solver's shell stress output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellStressAxes {
    /// MSC Nastran: CQUAD4 x on the diagonal bisector, CTRIA3 x on G1-G2.
    MscNastran,
    /// NASTRAN-95: x on G1-G2 for CQUAD4 and CTRIA3.
    Nastran95,
}

/// One skin shell of a cover panel.
#[derive(Debug, Clone, PartialEq)]
pub struct CoverPanelShell {
    /// Shell element identifier.
    pub eid: i64,
    /// Skin thickness, m.
    pub thickness_m: f64,
    /// Width across the stringers: element area over its length along them, m.
    pub width_m: f64,
    /// Young's modulus of the shell material, Pa.
    pub e_pa: f64,
    /// Angle from the MSC element x-axis to the stringers, rad.
    pub msc_angle_rad: f64,
    /// Angle from G1-G2 (the NASTRAN-95 x-axis) to the stringers, rad.
    pub edge_angle_rad: f64,
}

/// One spar-cap bar of a cover panel.
#[derive(Debug, Clone, PartialEq)]
pub struct CoverPanelCap {
    /// Bar element identifier.
    pub eid: i64,
    /// Section area, m^2.
    pub area_m2: f64,
    /// Young's modulus of the cap material, Pa.
    pub e_pa: f64,
}

/// The skin shells and spar caps of one rib bay of one cover.
#[derive(Debug, Clone, PartialEq)]
pub struct CoverPanel {
    /// Whether the panel is in the upper cover.
    pub upper: bool,
    /// Mean span station of the panel's shells, m.
    pub y_mid_m: f64,
    /// Material identifier of the skin.
    pub skin_mid: i64,
    /// Young's modulus of the skin, Pa.
    pub skin_e_pa: f64,
    /// The bay's skin shells.
    pub shells: Vec<CoverPanelShell>,
    /// The spar caps bounding the bay on this surface.
    pub caps: Vec<CoverPanelCap>,
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: [f64; 3], k: f64) -> [f64; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}
fn unit(a: [f64; 3]) -> Option<[f64; 3]> {
    let n = norm(a);
    (n > 0.0 && n.is_finite()).then(|| scale(a, 1.0 / n))
}

/// Signed in-plane angle from `x` to `s` about the element normal `n`, rad.
fn angle(x: [f64; 3], s: [f64; 3], n: [f64; 3]) -> f64 {
    dot(cross(x, s), n).atan2(dot(x, s))
}

/// Geometry of one shell: `(stringer direction, length along it, area,
/// normal, MSC x-axis, G1-G2)`.
fn shell_geometry(p: &[[f64; 3]]) -> Option<CoverGeometry> {
    let (direction, length, area, normal, msc_x) = match p {
        [a, b, c, d] => {
            let (d13, d24) = (sub(*c, *a), sub(*d, *b));
            let normal = unit(cross(d13, d24))?;
            let (e01, e12) = (sub(*b, *a), sub(*c, *b));
            // The spanwise pair of opposite edges: the one with the larger
            // span component.
            let along = if e01[1].abs() >= e12[1].abs() {
                sub(add(*b, *c), add(*a, *d))
            } else {
                sub(add(*c, *d), add(*a, *b))
            };
            let msc_x = unit(sub(unit(d13)?, unit(d24)?))?;
            (
                along,
                0.5 * norm(along),
                0.5 * norm(cross(d13, d24)),
                normal,
                msc_x,
            )
        }
        [a, b, c] => {
            let normal = unit(cross(sub(*b, *a), sub(*c, *a)))?;
            // A fan triangle has two corners on one rib and its apex on the
            // other: the apex is the corner whose span is furthest from the
            // other two.
            let apex = (0..3)
                .max_by(|&i, &j| {
                    let off = |k: usize| {
                        let others: Vec<_> = (0..3).filter(|&m| m != k).collect();
                        (p[k][1] - 0.5 * (p[others[0]][1] + p[others[1]][1])).abs()
                    };
                    off(i).total_cmp(&off(j))
                })
                .unwrap_or(0);
            let others: Vec<_> = (0..3).filter(|&m| m != apex).collect();
            let base_mid = scale(add(p[others[0]], p[others[1]]), 0.5);
            let along = sub(p[apex], base_mid);
            let msc_x = unit(sub(*b, *a))?;
            (
                along,
                norm(along),
                0.5 * norm(cross(sub(*b, *a), sub(*c, *a))),
                normal,
                msc_x,
            )
        }
        _ => return None,
    };
    // In the element plane, oriented outboard.
    let mut s = unit(sub(direction, scale(normal, dot(direction, normal))))?;
    if s[1] < 0.0 {
        s = scale(s, -1.0);
    }
    let g12 = unit(sub(p[1], p[0]))?;
    (length > 0.0 && area > 0.0).then_some(CoverGeometry {
        width_m: area / length,
        msc_angle_rad: angle(msc_x, s, normal),
        edge_angle_rad: angle(g12, s, normal),
    })
}

struct CoverGeometry {
    width_m: f64,
    msc_angle_rad: f64,
    edge_angle_rad: f64,
}

impl Deck {
    /// The stiffened cover panels of the deck: one per rib bay and surface,
    /// with the spar-cap bars whose two grids both lie on that bay's skin.
    ///
    /// Empty for a deck without cover strips. A bay whose shells cannot be
    /// resolved (missing grid, property or material) is left out.
    pub fn cover_panels(&self) -> Vec<CoverPanel> {
        let materials: HashMap<_, _> = self.materials.iter().map(|m| (m.mid, m.e)).collect();
        let thickness: HashMap<_, _> = self
            .shell_properties
            .iter()
            .map(|p| (p.pid, (p.t, p.mid1)))
            .collect();
        let shells: HashMap<_, _> = self
            .quads
            .iter()
            .chain(&self.trias)
            .map(|s| (s.eid, s))
            .collect();
        let caps: Vec<_> = self
            .bars
            .iter()
            .filter_map(|bar| {
                let property = self.bar_properties.iter().find(|p| p.pid == bar.pid)?;
                (property.section == "BAR" && property.dim.len() >= 2).then(|| {
                    (
                        bar,
                        property.dim[0] * property.dim[1],
                        materials.get(&property.mid).copied(),
                    )
                })
            })
            .collect();
        let mut panels = Vec::new();
        'strip: for strip in &self.cover_strips {
            let mut members = Vec::new();
            let mut nodes = HashSet::new();
            let (mut y_sum, mut skin) = (0.0, None);
            for eid in &strip.shells {
                let Some(shell) = shells.get(eid) else {
                    continue 'strip;
                };
                let Some(&(t, mid)) = thickness.get(&shell.pid) else {
                    continue 'strip;
                };
                let Some(&e) = materials.get(&mid) else {
                    continue 'strip;
                };
                let points: Option<Vec<_>> =
                    shell.nodes.iter().map(|&n| self.grid_xyz(n)).collect();
                let Some(points) = points else {
                    continue 'strip;
                };
                let Some(geometry) = shell_geometry(&points) else {
                    continue 'strip;
                };
                y_sum += points.iter().map(|p| p[1]).sum::<f64>() / points.len() as f64;
                skin.get_or_insert((mid, e));
                nodes.extend(shell.nodes.iter().copied());
                members.push(CoverPanelShell {
                    eid: *eid,
                    thickness_m: t,
                    width_m: geometry.width_m,
                    e_pa: e,
                    msc_angle_rad: geometry.msc_angle_rad,
                    edge_angle_rad: geometry.edge_angle_rad,
                });
            }
            let Some((skin_mid, skin_e_pa)) = skin else {
                continue;
            };
            let bay_caps = caps
                .iter()
                .filter(|(bar, _, _)| nodes.contains(&bar.ga) && nodes.contains(&bar.gb))
                .filter_map(|(bar, area, e)| {
                    Some(CoverPanelCap {
                        eid: bar.eid,
                        area_m2: *area,
                        e_pa: (*e)?,
                    })
                })
                .collect();
            panels.push(CoverPanel {
                upper: strip.upper,
                y_mid_m: y_sum / members.len() as f64,
                skin_mid,
                skin_e_pa,
                shells: members,
                caps: bay_caps,
            });
        }
        panels
    }
}

/// Stress along a direction at `angle_rad` from the element x-axis, from the
/// in-plane tensor in element axes.
pub fn stress_along(sx: f64, sy: f64, txy: f64, angle_rad: f64) -> f64 {
    let (s, c) = angle_rad.sin_cos();
    sx * c * c + sy * s * s + 2.0 * txy * s * c
}

impl CoverPanel {
    /// The panel's mean stress along the stringers in skin-material terms,
    /// `E_skin N / sum(E A)`, Pa (see the module documentation).
    ///
    /// `membrane` gives a shell's mid-surface tensor `[sx, sy, txy]` in the
    /// solver's element axes, `axial` a cap bar's axial stress. `None` when
    /// any member's stress is missing or the panel has no stiffness.
    pub fn stringer_stress_pa(
        &self,
        axes: ShellStressAxes,
        membrane: impl Fn(i64) -> Option<[f64; 3]>,
        axial: impl Fn(i64) -> Option<f64>,
    ) -> Option<f64> {
        let mut load = 0.0;
        let mut stiffness = 0.0;
        for shell in &self.shells {
            let [sx, sy, txy] = membrane(shell.eid)?;
            let angle = match axes {
                ShellStressAxes::MscNastran => shell.msc_angle_rad,
                ShellStressAxes::Nastran95 => shell.edge_angle_rad,
            };
            let area = shell.thickness_m * shell.width_m;
            load += stress_along(sx, sy, txy, angle) * area;
            stiffness += shell.e_pa * area;
        }
        for cap in &self.caps {
            load += axial(cap.eid)? * cap.area_m2;
            stiffness += cap.e_pa * cap.area_m2;
        }
        let stress = self.skin_e_pa * load / stiffness;
        (stiffness > 0.0 && stress.is_finite()).then_some(stress)
    }
}

// A failed unwrap here is the assertion failing.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stress_along_rotates_the_tensor() {
        assert!((stress_along(100.0, 20.0, 0.0, 0.0) - 100.0).abs() < 1e-12);
        assert!((stress_along(100.0, 20.0, 0.0, std::f64::consts::FRAC_PI_2) - 20.0).abs() < 1e-9);
        // Pure shear at 45 deg is a normal stress.
        assert!((stress_along(0.0, 0.0, 50.0, std::f64::consts::FRAC_PI_4) - 50.0).abs() < 1e-9);
    }

    #[test]
    fn rectangle_geometry_measures_width_across_the_spanwise_edges() {
        // Chordwise 0.2 m (x), spanwise 1.0 m (y); G1-G2 runs spanwise.
        let p = [
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.2, 1.0, 0.0],
            [0.2, 0.0, 0.0],
        ];
        let g = shell_geometry(&p).expect("valid quad");
        assert!((g.width_m - 0.2).abs() < 1e-12);
        assert!(g.edge_angle_rad.abs() < 1e-12);
        assert!(g.msc_angle_rad.abs() < 1e-12);
        // Same quad numbered from a chordwise edge: the stringers are 90 deg
        // from G1-G2.
        let q = [p[0], p[3], p[2], p[1]];
        let h = shell_geometry(&q).expect("valid quad");
        assert!((h.edge_angle_rad.abs() - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert!((h.width_m - 0.2).abs() < 1e-12);
    }

    #[test]
    fn fan_triangle_width_is_half_its_rib_edge() {
        let p = [[0.0, 0.0, 0.0], [0.4, 0.0, 0.0], [0.2, 1.0, 0.0]];
        let g = shell_geometry(&p).expect("valid tria");
        assert!((g.width_m - 0.2).abs() < 1e-12);
    }

    #[test]
    fn panel_stress_is_the_mean_over_skin_and_caps() {
        let panel = CoverPanel {
            upper: true,
            y_mid_m: 1.0,
            skin_mid: 1,
            skin_e_pa: 70e9,
            shells: vec![CoverPanelShell {
                eid: 1,
                thickness_m: 0.005,
                width_m: 2.0,
                e_pa: 70e9,
                msc_angle_rad: 0.0,
                edge_angle_rad: std::f64::consts::FRAC_PI_2,
            }],
            caps: vec![CoverPanelCap {
                eid: 9,
                area_m2: 0.01,
                e_pa: 70e9,
            }],
        };
        // Skin 0.01 m^2 at -400 MPa along the stringers, cap 0.01 m^2 at
        // -300 MPa: mean -350 MPa.
        let msc = panel
            .stringer_stress_pa(
                ShellStressAxes::MscNastran,
                |_| Some([-400e6, -30e6, 0.0]),
                |_| Some(-300e6),
            )
            .expect("complete");
        assert!((msc + 350e6).abs() < 1.0);
        let n95 = panel
            .stringer_stress_pa(
                ShellStressAxes::Nastran95,
                |_| Some([-30e6, -400e6, 0.0]),
                |_| Some(-300e6),
            )
            .expect("complete");
        assert!((n95 + 350e6).abs() < 1.0);
        assert!(panel
            .stringer_stress_pa(ShellStressAxes::MscNastran, |_| None, |_| Some(0.0))
            .is_none());
    }
}
