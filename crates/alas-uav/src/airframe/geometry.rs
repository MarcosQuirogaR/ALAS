// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lifting-surface geometry derived from the master parameters.
//!
//! A [`Planform`] is a chain of trapezoidal panels.  Horizontal surfaces run
//! along `y` (span is the lateral projection; the panel has a dihedral) and are
//! mirrored about the centre line; a fin runs along `z` and is a single
//! surface.  Integrals over a panel (`c`, `c^2`, `x_le c`, `y c`) are exact
//! with Simpson's rule because the integrands are at most quadratic in the
//! span station.

use crate::spec::topology::TailKind;

use super::inertia::Vec3;

/// One trapezoidal panel.  Positions are absolute body-axes coordinates of
/// the panel root leading edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Panel {
    /// Span of this panel along the surface axis (y for horizontal, z for fin), m.
    pub span_m: f64,
    /// Root chord, m.
    pub chord_root_m: f64,
    /// Tip chord, m.
    pub chord_tip_m: f64,
    /// Root leading-edge x, y, z, m.
    pub root_le_m: Vec3,
    /// Leading-edge sweep, rad (positive aft).
    pub sweep_le_rad: f64,
    /// Dihedral, rad (positive tip up); zero for a fin.
    pub dihedral_rad: f64,
}

impl Panel {
    fn chord_at(&self, s: f64) -> f64 {
        self.chord_root_m + (self.chord_tip_m - self.chord_root_m) * s / self.span_m
    }

    fn x_le_at(&self, s: f64) -> f64 {
        self.root_le_m[0] + s * self.sweep_le_rad.tan()
    }

    /// Simpson integral of `f(s)` over the panel span (exact up to quadratics).
    fn integral(&self, f: impl Fn(f64) -> f64) -> f64 {
        self.span_m / 6.0 * (f(0.0) + 4.0 * f(0.5 * self.span_m) + f(self.span_m))
    }
}

/// A chain of panels, mirrored (horizontal) or single (fin).
#[derive(Debug, Clone, PartialEq)]
pub struct Planform {
    /// Panels from the root outwards (one half when mirrored).
    pub panels: Vec<Panel>,
    /// Mirrored about `y = 0`.
    pub mirror: bool,
    /// Runs along `z` (a fin) instead of `y`.
    pub vertical: bool,
}

/// A chordwise-lumped slice of a planform used for mass and inertia.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Strip {
    /// Planform (projected) area of the slice, m^2.
    pub area_m2: f64,
    /// Slice centroid at the requested chord fraction, m.
    pub centroid_m: Vec3,
    /// Local chord, m.
    pub chord_m: f64,
    /// Width of the slice along the span axis, m.
    pub width_m: f64,
}

impl Planform {
    fn k(&self) -> f64 {
        if self.mirror {
            2.0
        } else {
            1.0
        }
    }

    /// Projected planform area (both halves), m^2.
    #[must_use]
    pub fn projected_area_m2(&self) -> f64 {
        self.k()
            * self
                .panels
                .iter()
                .map(|p| p.integral(|s| p.chord_at(s)))
                .sum::<f64>()
    }

    /// True surface area (projected area over cos of the dihedral), m^2.
    #[must_use]
    pub fn true_area_m2(&self) -> f64 {
        self.k()
            * self
                .panels
                .iter()
                .map(|p| p.integral(|s| p.chord_at(s)) / p.dihedral_rad.cos())
                .sum::<f64>()
    }

    /// Span along the span axis (tip to tip for a mirrored surface), m.
    #[must_use]
    pub fn span_m(&self) -> f64 {
        self.k() * self.panels.iter().map(|p| p.span_m).sum::<f64>()
    }

    /// Aspect ratio `b^2 / S` on the projected area.
    #[must_use]
    pub fn aspect_ratio(&self) -> f64 {
        let s = self.projected_area_m2();
        self.span_m().powi(2) / s
    }

    fn weighted(&self, w: impl Fn(&Panel, f64) -> f64) -> f64 {
        self.panels.iter().map(|p| p.integral(|s| w(p, s))).sum()
    }

    /// Mean aerodynamic chord `integral c^2 ds / integral c ds`, m.
    #[must_use]
    pub fn mac_m(&self) -> f64 {
        self.weighted(|p, s| p.chord_at(s).powi(2)) / self.weighted(|p, s| p.chord_at(s))
    }

    /// Leading-edge x of the MAC, `integral x_le c ds / integral c ds`, m.
    #[must_use]
    pub fn x_le_mac_m(&self) -> f64 {
        self.weighted(|p, s| p.x_le_at(s) * p.chord_at(s)) / self.weighted(|p, s| p.chord_at(s))
    }

    /// Span-axis station of the MAC measured from the root, `integral s c ds / integral c ds`, m.
    #[must_use]
    pub fn s_mac_m(&self) -> f64 {
        let mut offset = 0.0;
        let mut num = 0.0;
        for p in &self.panels {
            num += p.integral(|s| (offset + s) * p.chord_at(s));
            offset += p.span_m;
        }
        num / self.weighted(|p, s| p.chord_at(s))
    }

    /// Aerodynamic centre of the surface alone at 25 % MAC: `[x, y, z]`, m
    /// (`y = 0` for a mirrored surface; `z` at the MAC span station).
    #[must_use]
    pub fn aerodynamic_centre_m(&self) -> Vec3 {
        let s_mac = self.s_mac_m();
        let mut offset = 0.0;
        let mut z = self.panels.first().map_or(0.0, |p| p.root_le_m[2]);
        for p in &self.panels {
            if s_mac <= offset + p.span_m {
                let local = s_mac - offset;
                z = if self.vertical {
                    p.root_le_m[2] + local
                } else {
                    p.root_le_m[2] + local * p.dihedral_rad.tan()
                };
                break;
            }
            offset += p.span_m;
        }
        let y = if self.mirror {
            0.0
        } else {
            self.panels.first().map_or(0.0, |p| p.root_le_m[1])
        };
        [self.x_le_mac_m() + 0.25 * self.mac_m(), y, z]
    }

    /// Sweep of the line at chord fraction `p` (root to last tip), rad.
    #[must_use]
    pub fn sweep_at_chord_fraction_rad(&self, p: f64) -> f64 {
        let (Some(first), Some(last)) = (self.panels.first(), self.panels.last()) else {
            return 0.0;
        };
        let total: f64 = self.panels.iter().map(|q| q.span_m).sum();
        let tip_x = last.x_le_at(last.span_m) + p * last.chord_tip_m;
        let root_x = first.root_le_m[0] + p * first.chord_root_m;
        ((tip_x - root_x) / total).atan()
    }

    /// Chord and leading-edge x at the station `s` measured from the root
    /// along the span axis (one half when mirrored), m.  `None` beyond the tip.
    #[must_use]
    pub fn chord_and_le_x_at(&self, s: f64) -> Option<(f64, f64)> {
        let mut offset = 0.0;
        for p in &self.panels {
            if s <= offset + p.span_m + 1e-12 {
                let local = (s - offset).clamp(0.0, p.span_m);
                return Some((p.chord_at(local), p.x_le_at(local)));
            }
            offset += p.span_m;
        }
        None
    }

    /// Length of one half (or the fin height) along the span axis, m.
    #[must_use]
    pub fn half_span_m(&self) -> f64 {
        self.panels.iter().map(|p| p.span_m).sum()
    }

    /// Root leading-edge x of the first panel, m.
    #[must_use]
    pub fn root_le_x_m(&self) -> f64 {
        self.panels.first().map_or(0.0, |p| p.root_le_m[0])
    }

    /// Move the whole planform along x by `dx`, m.
    #[must_use]
    pub fn shifted_x(&self, dx: f64) -> Self {
        let mut out = self.clone();
        for p in &mut out.panels {
            p.root_le_m[0] += dx;
        }
        out
    }

    /// Scale the planform area by `k` keeping its shape (span and chords by
    /// `sqrt(k)`), first-panel root leading edge fixed.
    #[must_use]
    pub fn scaled_area(&self, k: f64) -> Self {
        let f = k.sqrt();
        let mut out = self.clone();
        let mut next_root: Option<Vec3> = None;
        for p in &mut out.panels {
            p.span_m *= f;
            p.chord_root_m *= f;
            p.chord_tip_m *= f;
            if let Some(r) = next_root {
                p.root_le_m = r;
            }
            let tip = if self.vertical {
                [
                    p.root_le_m[0] + p.span_m * p.sweep_le_rad.tan(),
                    p.root_le_m[1],
                    p.root_le_m[2] + p.span_m,
                ]
            } else {
                [
                    p.root_le_m[0] + p.span_m * p.sweep_le_rad.tan(),
                    p.root_le_m[1] + p.span_m,
                    p.root_le_m[2] + p.span_m * p.dihedral_rad.tan(),
                ]
            };
            next_root = Some(tip);
        }
        out
    }

    /// Aspect ratio on the true (slant) span and true area, which is the
    /// right figure for the lift-curve slope of a V pair.
    #[must_use]
    pub fn true_aspect_ratio(&self) -> f64 {
        let b = self.k()
            * self
                .panels
                .iter()
                .map(|p| p.span_m / p.dihedral_rad.cos())
                .sum::<f64>();
        b * b / self.true_area_m2()
    }

    /// Slice the planform into strips (`n_per_panel` per panel, both halves
    /// when mirrored) with centroids at chord fraction `chord_fraction`.
    #[must_use]
    pub fn strips(&self, n_per_panel: usize, chord_fraction: f64) -> Vec<Strip> {
        let n = n_per_panel.max(1);
        let mut out = Vec::new();
        for p in &self.panels {
            let ds = p.span_m / n as f64;
            for i in 0..n {
                let s = (i as f64 + 0.5) * ds;
                let c = p.chord_at(s);
                let x = p.x_le_at(s) + chord_fraction * c;
                let (y, z) = if self.vertical {
                    (p.root_le_m[1], p.root_le_m[2] + s)
                } else {
                    (
                        p.root_le_m[1] + s,
                        p.root_le_m[2] + s * p.dihedral_rad.tan(),
                    )
                };
                let strip = Strip {
                    area_m2: c * ds,
                    centroid_m: [x, y, z],
                    chord_m: c,
                    width_m: ds,
                };
                out.push(strip);
                if self.mirror {
                    out.push(Strip {
                        centroid_m: [x, -y, z],
                        ..strip
                    });
                }
            }
        }
        out
    }
}

/// Role of a tail surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailRole {
    /// Horizontal stabiliser.
    Horizontal,
    /// V (or inverted V) pair carrying pitch and yaw.
    VPair,
    /// Vertical fin (`count` fins, e.g. 2 on a twin boom).
    Vertical,
}

/// A tail surface with its attitude data.
#[derive(Debug, Clone, PartialEq)]
pub struct TailSurfaceGeom {
    /// Role.
    pub role: TailRole,
    /// Planform (mirrored for horizontal and V pair).
    pub planform: Planform,
    /// Dihedral half-angle of a V pair measured from horizontal, rad
    /// (negative for an inverted V); `0` otherwise.
    pub v_dihedral_rad: f64,
    /// Incidence relative to the wing zero-lift line, rad (positive nose-up).
    pub incidence_rad: f64,
    /// Number of identical surfaces (fins).
    pub count: u32,
}

impl TailSurfaceGeom {
    /// True area of all surfaces of this entry, m^2.
    #[must_use]
    pub fn true_area_m2(&self) -> f64 {
        f64::from(self.count) * self.planform.true_area_m2()
    }

    /// Effective horizontal-tail area for the pitch derivative, `S cos^2(Gamma)`
    /// (zero for a fin), m^2.
    #[must_use]
    pub fn effective_horizontal_area_m2(&self) -> f64 {
        match self.role {
            TailRole::Vertical => 0.0,
            _ => self.true_area_m2() * self.v_dihedral_rad.cos().powi(2),
        }
    }

    /// Effective vertical-tail area, `S sin^2(Gamma)` for a V pair and `S` for a fin, m^2.
    #[must_use]
    pub fn effective_vertical_area_m2(&self) -> f64 {
        match self.role {
            TailRole::Horizontal => 0.0,
            TailRole::VPair => self.true_area_m2() * self.v_dihedral_rad.sin().powi(2),
            TailRole::Vertical => self.true_area_m2(),
        }
    }
}

/// Geometry of the whole lifting system, derived from the spec.
#[derive(Debug, Clone, PartialEq)]
pub struct AirframeGeometry {
    /// Main wing (reference surface).
    pub wing: Planform,
    /// Horizontal tail or V pair; `None` for a flying wing.
    pub horizontal: Option<TailSurfaceGeom>,
    /// Vertical fin(s); `None` for V-tail and flying wing.
    pub vertical: Option<TailSurfaceGeom>,
    /// Tail kind from the topology.
    pub tail_kind: TailKind,
}

pub use super::layout::{planform_from_area, wing_planform, TailShape};
