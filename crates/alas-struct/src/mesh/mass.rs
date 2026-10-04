// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Structural material mass of one semi-wing FE deck, kg; excludes CONM2.
//! Uses current MAT1/PSHELL/PBARL subset only. All GRID coordinates are basic.
//! Quad area uses the same 2x2 bilinear Jacobian integration as NASA NASTRAN-95
//! mis/quad4s.f:948-1015. Bar length includes offsets, as mis/bars.f:167-180,
//! 580,747 does for actual material mass. The caller doubles for both wings.
use super::Deck;
use std::collections::{HashMap, HashSet};

impl Deck {
    /// Mass of all shell and bar structural material, excluding concentrated
    /// fuel/engine masses and massless rigid links. None means invalid model.
    pub fn primary_structural_mass_kg(&self) -> Option<f64> {
        self.primary_structural_mass_kg_with_diagnostics().ok()
    }

    /// Material mass, with the first invalid card or geometric cause retained.
    ///
    /// # Errors
    /// Reports invalid material/property references, element topology or mass.
    pub fn primary_structural_mass_kg_with_diagnostics(&self) -> Result<f64, String> {
        let materials: HashMap<_, _> = self.materials.iter().map(|m| (m.mid, m)).collect();
        let shells: HashMap<_, _> = self.shell_properties.iter().map(|p| (p.pid, p)).collect();
        let bars: HashMap<_, _> = self.bar_properties.iter().map(|p| (p.pid, p)).collect();
        if materials.len() != self.materials.len()
            || shells.len() != self.shell_properties.len()
            || bars.len() != self.bar_properties.len()
        {
            return Err("duplicate material or property identifiers".to_owned());
        }
        let mut ids = HashSet::new();
        let mut total = 0.0;
        for shell in self.quads.iter().chain(&self.trias) {
            if !ids.insert(shell.eid) {
                return Err(format!(
                    "shell element {}: duplicate element identifier",
                    shell.eid
                ));
            }
            let p = *shells.get(&shell.pid).ok_or_else(|| {
                format!("shell element {}: missing PSHELL {}", shell.eid, shell.pid)
            })?;
            let rho = materials
                .get(&p.mid1)
                .ok_or_else(|| format!("PSHELL {}: missing MAT1 {}", p.pid, p.mid1))?
                .rho;
            if !positive(p.t) || !positive(rho) {
                return Err(format!(
                    "shell element {}: invalid thickness {} or density {}",
                    shell.eid, p.t, rho
                ));
            }
            let xyz: Option<Vec<_>> = shell
                .nodes
                .iter()
                .map(|&nid| {
                    self.grid_xyz(nid)
                        .filter(|v| v.iter().all(|x| x.is_finite()))
                })
                .collect();
            let unique: HashSet<_> = shell.nodes.iter().collect();
            if unique.len() != shell.nodes.len() {
                return Err(format!(
                    "shell element {}: repeated GRID identifiers {:?}",
                    shell.eid, shell.nodes
                ));
            }
            let xyz = xyz.ok_or_else(|| {
                format!(
                    "shell element {}: missing or nonfinite GRID coordinates",
                    shell.eid
                )
            })?;
            total += shell_area(&xyz).map_err(|cause| {
                format!(
                    "shell element {} (GRID {:?}): {cause}; coordinates_m={xyz:?}",
                    shell.eid, shell.nodes
                )
            })? * p.t
                * rho;
        }
        for bar in &self.bars {
            if !ids.insert(bar.eid) || bar.offt != "GGG" {
                return Err(format!(
                    "bar element {}: duplicate identifier or unsupported OFFT {}",
                    bar.eid, bar.offt
                ));
            }
            let property = *bars
                .get(&bar.pid)
                .ok_or_else(|| format!("bar element {}: missing PBARL {}", bar.eid, bar.pid))?;
            let rho = materials
                .get(&property.mid)
                .ok_or_else(|| format!("PBARL {}: missing MAT1 {}", property.pid, property.mid))?
                .rho;
            let a = self
                .grid_xyz(bar.ga)
                .ok_or_else(|| format!("bar element {}: missing GRID {}", bar.eid, bar.ga))?;
            let b = self
                .grid_xyz(bar.gb)
                .ok_or_else(|| format!("bar element {}: missing GRID {}", bar.eid, bar.gb))?;
            if !positive(rho)
                || a.iter()
                    .chain(&b)
                    .chain(&bar.offset_a)
                    .chain(&bar.offset_b)
                    .any(|x| !x.is_finite())
            {
                return Err(format!(
                    "bar element {}: invalid density, GRID coordinates or offsets",
                    bar.eid
                ));
            }
            let delta = std::array::from_fn(|i| b[i] + bar.offset_b[i] - a[i] - bar.offset_a[i]);
            let length = norm(delta);
            if !positive(length) {
                return Err(format!(
                    "bar element {}: nonpositive or nonfinite offset length {length} m",
                    bar.eid
                ));
            }
            total += bar_area(property.section, &property.dim).ok_or_else(|| {
                format!(
                    "bar element {}: invalid {} section dimensions {:?}",
                    bar.eid, property.section, property.dim
                )
            })? * rho
                * length;
        }
        positive(total)
            .then_some(total)
            .ok_or_else(|| format!("nonpositive or nonfinite material mass {total} kg"))
    }
}
fn positive(v: f64) -> bool {
    v.is_finite() && v > 0.0
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(x, y)| x * y).sum()
}
fn norm(a: [f64; 3]) -> f64 {
    a[0].hypot(a[1]).hypot(a[2])
}
fn bar_area(section: &str, dimensions: &[f64]) -> Option<f64> {
    if dimensions.iter().any(|&v| !positive(v)) {
        return None;
    }
    let area = match (section, dimensions) {
        ("BAR", [w, t]) => w * t,
        ("I", [h, wb, wt, tw, tb, tt]) if h > &(tb + tt) && tw <= wb && tw <= wt => {
            wb * tb + wt * tt + tw * (h - tb - tt)
        }
        _ => return None,
    };
    positive(area).then_some(area)
}
fn shell_area(xyz: &[[f64; 3]]) -> Result<f64, &'static str> {
    let area = match xyz {
        [a, b, c] => 0.5 * norm(cross(sub(*b, *a), sub(*c, *a))),
        [_, _, _, _] => {
            let g = 1.0 / 3.0_f64.sqrt();
            let mut area = 0.0;
            let mut reference = None;
            for xi in [-g, g] {
                for eta in [-g, g] {
                    let dx = [
                        -0.25 * (1.0 - eta),
                        0.25 * (1.0 - eta),
                        0.25 * (1.0 + eta),
                        -0.25 * (1.0 + eta),
                    ];
                    let de = [
                        -0.25 * (1.0 - xi),
                        -0.25 * (1.0 + xi),
                        0.25 * (1.0 + xi),
                        0.25 * (1.0 - xi),
                    ];
                    let a =
                        std::array::from_fn(|j| xyz.iter().zip(dx).map(|(p, d)| p[j] * d).sum());
                    let b =
                        std::array::from_fn(|j| xyz.iter().zip(de).map(|(p, d)| p[j] * d).sum());
                    let jac = cross(a, b);
                    let magnitude = norm(jac);
                    if !positive(magnitude) {
                        return Err("quadrilateral has a nonpositive or nonfinite Gauss-point Jacobian magnitude");
                    }
                    let normal = jac.map(|v| v / magnitude);
                    if let Some(first) = reference {
                        if dot(first, normal) <= 0.0 {
                            return Err(
                                "quadrilateral Jacobian changes orientation between Gauss points",
                            );
                        }
                    } else {
                        reference = Some(normal);
                    }
                    area += magnitude;
                }
            }
            area
        }
        _ => return Err("shell must have three or four nodes"),
    };
    positive(area)
        .then_some(area)
        .ok_or("shell area is nonpositive or nonfinite")
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::{Cbar, Conm2, Mat1, Pbarl, Pshell, Shell};
    fn deck() -> Deck {
        let mut d = Deck::new();
        for (i, xyz) in [
            [0., 0., 0.],
            [2., 0., 0.],
            [2., 3., 0.],
            [0., 3., 0.],
            [3., 4., 0.],
        ]
        .into_iter()
        .enumerate()
        {
            d.add_grid(i as i64 + 1, xyz);
        }
        d.materials.push(Mat1 {
            mid: 1,
            e: 1.,
            g: 1.,
            nu: 0.3,
            rho: 1000.,
        });
        d.shell_properties.push(Pshell {
            pid: 1,
            mid1: 1,
            t: 0.01,
            mid2: 1,
        });
        d.quads.push(Shell {
            eid: 1,
            pid: 1,
            nodes: vec![1, 2, 3, 4],
        });
        d.trias.push(Shell {
            eid: 2,
            pid: 1,
            nodes: vec![1, 2, 4],
        });
        d.bar_properties.push(Pbarl {
            pid: 2,
            mid: 1,
            section: "BAR",
            dim: vec![0.1, 0.2],
        });
        d.bars.push(Cbar {
            eid: 3,
            pid: 2,
            ga: 1,
            gb: 5,
            x: [0., 0., 1.],
            offt: "GGG",
            offset_a: [0.; 3],
            offset_b: [0.; 3],
        });
        d
    }
    #[test]
    fn known_shell_bar_mass_ignores_concentrated_mass() {
        let mut d = deck();
        assert!((d.primary_structural_mass_kg().unwrap() - 190.).abs() < 1e-10);
        assert_eq!(
            d.primary_structural_mass_kg_with_diagnostics()
                .unwrap()
                .to_bits(),
            d.primary_structural_mass_kg().unwrap().to_bits()
        );
        d.masses.push(Conm2 {
            eid: 4,
            nid: 1,
            cid: 0,
            mass: 9999.,
            offset: [0.; 3],
        });
        assert!((d.primary_structural_mass_kg().unwrap() - 190.).abs() < 1e-10);
    }
    #[test]
    fn centroid_offsets_change_length_and_missing_material_fails() {
        let mut d = deck();
        d.bars[0].offset_b = [0., 1., 0.];
        assert!(
            (d.primary_structural_mass_kg().unwrap() - (90. + 20. * 34.0_f64.sqrt())).abs() < 1e-10
        );
        d.materials.clear();
        assert!(d.primary_structural_mass_kg().is_none());
        assert!(d
            .primary_structural_mass_kg_with_diagnostics()
            .unwrap_err()
            .contains("missing MAT1"));
    }
    #[test]
    fn invalid_values_and_folded_quad_fail_closed() {
        let mut d = deck();
        d.shell_properties[0].t = f64::NAN;
        assert!(d.primary_structural_mass_kg().is_none());
        let mut d = deck();
        d.quads[0].nodes = vec![1, 3, 2, 4];
        assert!(d.primary_structural_mass_kg().is_none());
        let detail = d.primary_structural_mass_kg_with_diagnostics().unwrap_err();
        assert!(detail.contains(&format!("shell element {}", d.quads[0].eid)));
        assert!(detail.contains("quadrilateral Jacobian changes orientation between Gauss points"));
    }
}
