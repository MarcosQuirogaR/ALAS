// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Closed-form benchmark of the NASTRAN-95 product path: a clamped
//! thin-walled box beam, solved with the product's own card writers, unit
//! scaling and print readers and compared with beam theory.
//!
//! The wingbox comparisons against MSC Nastran are code-to-code; this one has
//! an analytic answer, so it measures NASTRAN-95's error directly. Run at
//! several deck length units `L` (deck length units per metre, with a
//! coherent mass unit so one force unit stays one newton) it also measures
//! how much that error moves with `L` ([`super::units`]: up to 25 percent in
//! A320 wingbox stress). Three load cases exercise the card families that
//! wingbox uses, one at a time, so a scale sensitivity is attributed to a
//! feature and not to "the solver":
//!
//! 1. `tip`: a transverse force on the free-end ring (`FORCE`, `CQUAD4`,
//!    `PSHELL`, `MAT1`, clamped `SPC1`);
//! 2. `gravity`: self-weight (`GRAV`, `MAT1` density), i.e. the mass-unit path;
//! 3. `spider`: the same tip force applied at a reference grid tied to the
//!    free-end ring by a `CRBE3` (the load-introduction path).
//!
//! # Reference solution
//!
//! A cantilever of length `l`, mid-surface width `w`, height `h` and uniform
//! wall `t` (Timoshenko, *Strength of Materials*, vol. 1):
//!
//! - second moment `I = 2 (w t (h/2)^2 + w t^3/12) + 2 t h^3/12` (flanges with
//!   their own inertia, two webs); shear area `A_s = 2 h t` (webs);
//! - tip load `P` along +z: `w(x) = P x^2 (3 l - x) / (6 E I) + P x / (G A_s)`
//!   and top-flange membrane stress `sigma = -P (l - x) (h/2) / I`. An upward
//!   load deflects the beam concave up, which shortens the top fibre: the top
//!   flange is in compression;
//! - self-weight `q = rho g 2 (w + h) t` per length, downward:
//!   `w(x) = -[q x^2 (6 l^2 - 4 l x + x^2) / (24 E I) + q (l x - x^2/2) /
//!   (G A_s)]` and top-flange stress `sigma = +q (l - x)^2 (h/2) / (2 I)`
//!   (tension).
//!
//! Point forces on the free-end ring leave it without a diaphragm, so the
//! section ovalises at the tip: deflection is compared at mid-length and
//! stress at a quarter length, outside that disturbance and the root's
//! warping restraint. The remaining shell-model error (shear lag, the 1995
//! `CQUAD4` drilling treatment) is part of what is measured, so the gate in
//! [`BenchmarkOutcome::within`] is a tolerance.

use std::path::Path;

use super::deck::{benchmark_bulk, benchmark_card};
use super::deck::{Dialect, UnitScale};
use super::field::{ContinuationTags, Field};
use super::{
    displacement_of, read_displacement_tables, read_static_shell_stress_print, run_nastran95,
    Nastran95Solver, RunOutcome,
};
use crate::loads::LoadCase;
use crate::mesh::{Deck, Mat1, Param, ParamValue, Pshell, Rbe3, Shell, Spc1};

/// Standard gravity, m/s^2 (ISO 80000-3).
const G0_M_S2: f64 = 9.80665;

const TIP_FORCE_SET: i64 = 201;
const GRAVITY_SET: i64 = 102;
const SPIDER_FORCE_SET: i64 = 203;

/// Geometry, material, load and mesh of the benchmark. SI throughout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CantileverBox {
    /// Beam length, m.
    pub length_m: f64,
    /// Mid-surface width (y), m.
    pub width_m: f64,
    /// Mid-surface height (z), m.
    pub height_m: f64,
    /// Wall thickness, m.
    pub wall_m: f64,
    /// Young's modulus, Pa.
    pub e_pa: f64,
    /// Poisson's ratio.
    pub nu: f64,
    /// Density, kg/m^3.
    pub rho_kg_m3: f64,
    /// Tip force along +z, N, shared equally by the tip-ring grids (or
    /// applied at the spider's reference grid).
    pub tip_load_n: f64,
    /// Elements along the length.
    pub nx: usize,
    /// Elements across the width on each flange.
    pub ny: usize,
    /// Elements up each web.
    pub nz: usize,
}

impl CantileverBox {
    /// The reference case: a 3 m aluminium box, 0.4 m by 0.3 m, 3 mm walls,
    /// 10 kN at the tip (flange stress about 50 MPa, tip deflection about
    /// 19 mm: linear, far below yield).
    pub fn reference() -> Self {
        Self {
            length_m: 3.0,
            width_m: 0.4,
            height_m: 0.3,
            wall_m: 0.003,
            e_pa: 71.7e9,
            nu: 0.33,
            rho_kg_m3: 2810.0,
            tip_load_n: 10_000.0,
            nx: 60,
            ny: 8,
            nz: 6,
        }
    }

    fn shear_modulus_pa(&self) -> f64 {
        self.e_pa / (2.0 * (1.0 + self.nu))
    }

    fn shear_area_m2(&self) -> f64 {
        2.0 * self.height_m * self.wall_m
    }

    fn ring_len(&self) -> usize {
        2 * (self.ny + self.nz)
    }

    /// Second moment of the thin-walled section about the horizontal axis, m^4.
    pub fn second_moment_m4(&self) -> f64 {
        let (w, h, t) = (self.width_m, self.height_m, self.wall_m);
        2.0 * (w * t * (h / 2.0).powi(2) + w * t.powi(3) / 12.0) + 2.0 * t * h.powi(3) / 12.0
    }

    /// Self-weight per unit length, N/m.
    pub fn weight_per_length_n_m(&self) -> f64 {
        self.rho_kg_m3 * G0_M_S2 * 2.0 * (self.width_m + self.height_m) * self.wall_m
    }

    /// Beam-theory deflection along +z under the tip load at station `x_m`
    /// from the root (bending plus web shear), m.
    pub fn beam_tip_load_deflection_m(&self, x_m: f64) -> f64 {
        let bending = self.tip_load_n * x_m * x_m * (3.0 * self.length_m - x_m)
            / (6.0 * self.e_pa * self.second_moment_m4());
        let shear = self.tip_load_n * x_m / (self.shear_modulus_pa() * self.shear_area_m2());
        bending + shear
    }

    /// Beam-theory top-flange membrane stress under the tip load, Pa
    /// (negative: compression for a load along +z).
    pub fn beam_tip_load_stress_pa(&self, x_m: f64) -> f64 {
        -self.tip_load_n * (self.length_m - x_m) * (self.height_m / 2.0) / self.second_moment_m4()
    }

    /// Beam-theory deflection along +z under self-weight (negative: down), m.
    pub fn beam_gravity_deflection_m(&self, x_m: f64) -> f64 {
        let (q, l) = (self.weight_per_length_n_m(), self.length_m);
        let bending = q * x_m * x_m * (6.0 * l * l - 4.0 * l * x_m + x_m * x_m)
            / (24.0 * self.e_pa * self.second_moment_m4());
        let shear =
            q * (l * x_m - 0.5 * x_m * x_m) / (self.shear_modulus_pa() * self.shear_area_m2());
        -(bending + shear)
    }

    /// Beam-theory top-flange membrane stress under self-weight, Pa
    /// (positive: tension for a downward load).
    pub fn beam_gravity_stress_pa(&self, x_m: f64) -> f64 {
        let remaining = self.length_m - x_m;
        self.weight_per_length_n_m() * remaining * remaining * (self.height_m / 2.0)
            / (2.0 * self.second_moment_m4())
    }

    /// Ring position `p`'s `(y, z)` in metres: bottom flange left to right,
    /// right web up, top flange right to left, left web down.
    fn ring_yz(&self, p: usize) -> (f64, f64) {
        let (w, h) = (self.width_m, self.height_m);
        let (ny, nz) = (self.ny, self.nz);
        let f = |i: usize, n: usize| i as f64 / n as f64;
        if p < ny {
            (-w / 2.0 + w * f(p, ny), -h / 2.0)
        } else if p < ny + nz {
            (w / 2.0, -h / 2.0 + h * f(p - ny, nz))
        } else if p < 2 * ny + nz {
            (w / 2.0 - w * f(p - ny - nz, ny), h / 2.0)
        } else {
            (-w / 2.0, h / 2.0 - h * f(p - 2 * ny - nz, nz))
        }
    }

    fn nid(&self, ix: usize, p: usize) -> i64 {
        (1 + ix * self.ring_len() + p % self.ring_len()) as i64
    }

    /// The spider's reference grid, beyond the last ring grid.
    fn spider_nid(&self) -> i64 {
        ((self.nx + 1) * self.ring_len() + 1) as i64
    }

    /// Whether ring segment `p` (from ring node `p` to `p + 1`) lies on the
    /// top flange.
    fn is_top_flange(&self, p: usize) -> bool {
        (self.ny + self.nz..2 * self.ny + self.nz).contains(&p)
    }

    /// The mesh as a product [`Deck`] (grids, `CQUAD4`, `PSHELL`, `MAT1`, a
    /// clamped root `SPC1`, the tip spider, `PARAM,AUTOSPC`).
    pub fn deck(&self) -> Deck {
        let mut deck = Deck::new();
        let ring = self.ring_len();
        let dx = self.length_m / self.nx as f64;
        for ix in 0..=self.nx {
            for p in 0..ring {
                let (y, z) = self.ring_yz(p);
                deck.add_grid(self.nid(ix, p), [ix as f64 * dx, y, z]);
            }
        }
        deck.add_grid(self.spider_nid(), [self.length_m, 0.0, 0.0]);
        deck.materials.push(Mat1 {
            mid: 1,
            e: self.e_pa,
            g: self.shear_modulus_pa(),
            nu: self.nu,
            rho: self.rho_kg_m3,
        });
        deck.shell_properties.push(Pshell {
            pid: 1,
            mid1: 1,
            t: self.wall_m,
            mid2: 1,
        });
        let mut eid = 1;
        for ix in 0..self.nx {
            for p in 0..ring {
                deck.quads.push(Shell {
                    eid,
                    pid: 1,
                    nodes: vec![
                        self.nid(ix, p),
                        self.nid(ix + 1, p),
                        self.nid(ix + 1, p + 1),
                        self.nid(ix, p + 1),
                    ],
                });
                eid += 1;
            }
        }
        deck.rigid_elements.push(Rbe3 {
            eid: 1_000_000,
            refgrid: self.spider_nid(),
            refc: "123456",
            weight: 1.0,
            comp: "123",
            gijs: (0..ring).map(|p| self.nid(self.nx, p)).collect(),
        });
        deck.constraints.push(Spc1 {
            sid: 1,
            components: "123456",
            nodes: (0..ring).map(|p| self.nid(0, p)).collect(),
        });
        deck.params.push(Param {
            key: "AUTOSPC",
            value: ParamValue::Int(1),
        });
        deck
    }

    /// The NASTRAN-95 static deck in deck length units `length_per_m` per
    /// metre (mass unit `1/length_per_m`, so one force unit is one newton).
    pub fn static_deck_text(&self, length_per_m: f64) -> String {
        let units = benchmark_units(length_per_m);
        let deck = self.deck();
        let mut out = String::new();
        out.push_str("ID ALAS,BENCH\nAPP DISPLACEMENT\nSOL 1,1\nTIME 10\nCEND\n");
        out.push_str("MAXLINES = 1000000\nTITLE = CLOSED-FORM CANTILEVER BOX\n");
        out.push_str("  SPC = 1\n  DISPLACEMENT = ALL\n  STRESS = ALL\n");
        for (sid, label) in [(1, "TIP"), (2, "GRAVITY"), (3, "SPIDER")] {
            out.push_str(&format!(
                "SUBCASE {sid}\n  LABEL = {label}\n  LOAD = {sid}\n"
            ));
        }
        out.push_str("BEGIN BULK\n");
        let mut tags = ContinuationTags::new();
        benchmark_bulk(&mut out, &mut tags, &deck, Dialect::Nastran95, units);
        for (sid, set) in [(1, TIP_FORCE_SET), (2, GRAVITY_SET), (3, SPIDER_FORCE_SET)] {
            benchmark_card(
                &mut out,
                &mut tags,
                "LOAD",
                vec![
                    Field::Int(sid),
                    Field::Real(1.0),
                    Field::Real(1.0),
                    Field::Int(set),
                ],
            );
        }
        let ring = self.ring_len();
        let share = self.tip_load_n / ring as f64;
        let force = |set: i64, nid: i64, newtons: f64| {
            vec![
                Field::Int(set),
                Field::Int(nid),
                Field::Int(0),
                Field::Real(newtons * units.force()),
                Field::Real(0.0),
                Field::Real(0.0),
                Field::Real(1.0),
            ]
        };
        for p in 0..ring {
            benchmark_card(
                &mut out,
                &mut tags,
                "FORCE",
                force(TIP_FORCE_SET, self.nid(self.nx, p), share),
            );
        }
        benchmark_card(
            &mut out,
            &mut tags,
            "FORCE",
            force(SPIDER_FORCE_SET, self.spider_nid(), self.tip_load_n),
        );
        benchmark_card(
            &mut out,
            &mut tags,
            "GRAV",
            vec![
                Field::Int(GRAVITY_SET),
                Field::Int(0),
                Field::Real(G0_M_S2 * units.acceleration()),
                Field::Real(0.0),
                Field::Real(0.0),
                Field::Real(-1.0),
            ],
        );
        out.push_str("ENDDATA\n");
        out
    }

    /// Read the print file of [`Self::static_deck_text`] at `length_per_m`.
    pub fn evaluate(&self, print: &str, length_per_m: f64) -> Result<BenchmarkOutcome, String> {
        let units = benchmark_units(length_per_m);
        let deck = self.deck();
        let tables = read_displacement_tables(print);
        let ring = self.ring_len();
        if self.nz % 2 != 0 {
            return Err("the web mid-height grid needs an even nz".to_owned());
        }
        let mid_ix = self.nx / 2;
        let dx = self.length_m / self.nx as f64;
        let mid_x = mid_ix as f64 * dx;
        // Deflection is read on the neutral axis (the two web grids at
        // mid-height): flange grids add local plate-bending sag between the
        // webs, which beam theory lacks (about 0.02 mm under self-weight).
        let web_mid_z = |subcase: usize| -> Result<f64, String> {
            let right = self.ny + self.nz / 2;
            let left = 2 * self.ny + self.nz + self.nz / 2;
            let mut mean = 0.0;
            for p in [right, left] {
                let row = displacement_of(&tables, subcase, self.nid(mid_ix, p))
                    .ok_or("web displacement missing from the print file")?;
                mean += 0.5 * row[2] / units.length;
            }
            Ok(mean)
        };
        let load_cases = ["tip", "gravity", "spider"].map(|name| LoadCase {
            name,
            load_factor: 1.0,
            total_force_n: self.tip_load_n,
        });
        let stress = read_static_shell_stress_print(print, &deck, &load_cases);
        if let Some(error) = stress.error {
            return Err(format!("shell stress output: {error}"));
        }
        // Element row nearest a quarter length from the root: beyond the
        // clamp's warping disturbance, where the moment is still large.
        let row = ((0.25 * self.length_m / dx - 0.5).round().max(0.0) as usize).min(self.nx - 1);
        let x_centroid = (row as f64 + 0.5) * dx;
        let mut cases = Vec::new();
        for (index, name) in ["tip", "gravity", "spider"].into_iter().enumerate() {
            let samples = &stress
                .cases
                .get(index)
                .ok_or("a load case is missing from the stress output")?
                .samples;
            let (mut sum, mut count) = (0.0, 0_usize);
            for p in (0..ring).filter(|&p| self.is_top_flange(p)) {
                let eid = (row * ring + p + 1) as i64;
                let fibers = samples
                    .iter()
                    .filter(|s| s.element_id == eid)
                    .collect::<Vec<_>>();
                if fibers.len() != 2 {
                    return Err(format!("element {eid} lacks its two stress fibers"));
                }
                // Membrane stress: the mean of the two fibers cancels the
                // plate-bending part, which these loads do not excite in the
                // flanges.
                sum += 0.5 * (fibers[0].normal_x_pa + fibers[1].normal_x_pa) / units.stress();
                count += 1;
            }
            let (beam_defl, beam_stress) = if name == "gravity" {
                (
                    self.beam_gravity_deflection_m(mid_x),
                    self.beam_gravity_stress_pa(x_centroid),
                )
            } else {
                (
                    self.beam_tip_load_deflection_m(mid_x),
                    self.beam_tip_load_stress_pa(x_centroid),
                )
            };
            cases.push(LoadCaseOutcome {
                name,
                mid_deflection_m: web_mid_z(index)?,
                beam_mid_deflection_m: beam_defl,
                flange_stress_pa: sum / count.max(1) as f64,
                beam_flange_stress_pa: beam_stress,
            });
        }
        Ok(BenchmarkOutcome {
            length_per_m,
            mid_station_m: mid_x,
            stress_station_m: x_centroid,
            cases,
        })
    }
}

fn benchmark_units(length_per_m: f64) -> UnitScale {
    UnitScale {
        length: length_per_m,
        mass: 1.0 / length_per_m,
    }
}

/// One load case against beam theory.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LoadCaseOutcome {
    /// `"tip"`, `"gravity"` or `"spider"`.
    pub name: &'static str,
    /// Mid-length neutral-axis (web mid-height) deflection along +z, m (SI).
    pub mid_deflection_m: f64,
    /// Beam-theory deflection at that station, m.
    pub beam_mid_deflection_m: f64,
    /// Mean top-flange membrane stress at the stress station, Pa (SI).
    pub flange_stress_pa: f64,
    /// Beam-theory top-flange stress at that station, Pa.
    pub beam_flange_stress_pa: f64,
}

impl LoadCaseOutcome {
    /// Signed relative error of the mid-length deflection against beam theory.
    pub fn deflection_error(&self) -> f64 {
        self.mid_deflection_m / self.beam_mid_deflection_m - 1.0
    }

    /// Signed relative error of the flange stress against beam theory.
    pub fn stress_error(&self) -> f64 {
        self.flange_stress_pa / self.beam_flange_stress_pa - 1.0
    }
}

/// One solve: all three load cases.
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkOutcome {
    /// Deck length units per metre of the solve.
    pub length_per_m: f64,
    /// Deflection station, m from the root.
    pub mid_station_m: f64,
    /// Stress (element centroid) station, m from the root.
    pub stress_station_m: f64,
    /// The three load cases.
    pub cases: Vec<LoadCaseOutcome>,
}

impl BenchmarkOutcome {
    /// Whether every case's deflection and stress are inside the relative
    /// tolerances.
    pub fn within(&self, deflection_tolerance: f64, stress_tolerance: f64) -> bool {
        self.cases.iter().all(|case| {
            case.deflection_error().abs() <= deflection_tolerance
                && case.stress_error().abs() <= stress_tolerance
        })
    }
}

/// Solve at `length_per_m` deck units per metre; `work_dir` must satisfy
/// [`run_nastran95`]'s path limits.
pub fn run_cantilever_benchmark(
    solver: &Nastran95Solver,
    case: &CantileverBox,
    length_per_m: f64,
    work_dir: &Path,
    timeout_seconds: f64,
) -> Result<BenchmarkOutcome, String> {
    let text = case.static_deck_text(length_per_m);
    match run_nastran95(solver, &text, work_dir, timeout_seconds) {
        RunOutcome::Print(print) => case.evaluate(&print, length_per_m),
        RunOutcome::Failed(detail) => Err(detail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beam_reference_values_are_the_hand_calculation() {
        let case = CantileverBox::reference();
        // I = 2 (0.4*0.003*0.15^2 + 0.4*0.003^3/12) + 2*0.003*0.3^3/12
        let inertia =
            2.0 * (0.4 * 0.003 * 0.0225 + 0.4 * 2.7e-8 / 12.0) + 2.0 * 0.003 * 0.027 / 12.0;
        assert!((case.second_moment_m4() / inertia - 1.0).abs() < 1e-12);
        let bending = 1.0e4 * 27.0 / (3.0 * 71.7e9 * inertia);
        let shear = 1.0e4 * 3.0 / (71.7e9 / 2.66 * 2.0 * 0.3 * 0.003);
        assert!((case.beam_tip_load_deflection_m(3.0) / (bending + shear) - 1.0).abs() < 1e-9);
        // Root flange stress is M h / (2 I) with M = P l: compression for an
        // upward load.
        let root = case.beam_tip_load_stress_pa(0.0);
        assert!((root / (-1.0e4 * 3.0 * 0.15 / inertia) - 1.0).abs() < 1e-12);
        assert!(root.abs() < 3.0e8, "linear and elastic");
        // Self-weight: q = 2810 g 2 (0.7) 0.003 = 115.8 N/m; tip sag
        // q l^4 / (8 E I) plus shear q l^2 / (2 G A_s), downward.
        let q = 2810.0 * 9.80665 * 1.4 * 0.003;
        assert!((case.weight_per_length_n_m() / q - 1.0).abs() < 1e-12);
        let sag = q * 81.0 / (8.0 * 71.7e9 * inertia) + q * 9.0 / (2.0 * 71.7e9 / 2.66 * 0.0018);
        assert!((case.beam_gravity_deflection_m(3.0) / -sag - 1.0).abs() < 1e-9);
        // Root moment q l^2 / 2 puts the top flange in tension.
        let root_gravity = case.beam_gravity_stress_pa(0.0);
        assert!((root_gravity / (q * 9.0 / 2.0 * 0.15 / inertia) - 1.0).abs() < 1e-12);
        // Both deflection curves are zero at the clamp and monotone in x.
        for curve in [
            CantileverBox::beam_tip_load_deflection_m,
            CantileverBox::beam_gravity_deflection_m,
        ] {
            assert_eq!(curve(&case, 0.0), 0.0);
            assert!(curve(&case, 1.5).abs() < curve(&case, 3.0).abs());
        }
    }

    #[test]
    fn the_deck_is_closed_clamped_and_fits_the_fixed_field_in_every_unit_system() {
        let case = CantileverBox::reference();
        let deck = case.deck();
        let ring = 2 * (case.ny + case.nz);
        assert_eq!(deck.grids().len(), (case.nx + 1) * ring + 1);
        assert_eq!(deck.quads().len(), case.nx * ring);
        // The ring closes: the last element of a row shares its second edge
        // with the first.
        let last = &deck.quads()[ring - 1];
        assert_eq!(last.nodes[3], deck.quads()[0].nodes[0]);
        for length_per_m in [1.0, 10.0, 100.0, 1000.0] {
            let text = case.static_deck_text(length_per_m);
            assert!(text.starts_with("ID ALAS,BENCH"));
            assert!(text.contains("CQUAD4") && text.contains("SPC1") && text.contains("CRBE3"));
            assert_eq!(text.matches("\nFORCE").count(), ring + 1);
            assert_eq!(text.matches("\nGRAV").count(), 1);
            assert!(text.lines().all(|line| line.len() <= 80), "{length_per_m}");
        }
    }

    #[test]
    fn the_force_per_grid_is_unit_independent() {
        let case = CantileverBox::reference();
        let share = case.tip_load_n / (2.0 * (case.ny + case.nz) as f64);
        for length_per_m in [1.0, 10.0, 1000.0] {
            let units = benchmark_units(length_per_m);
            assert!((share * units.force() - share).abs() < 1e-9 * share);
        }
    }
}
