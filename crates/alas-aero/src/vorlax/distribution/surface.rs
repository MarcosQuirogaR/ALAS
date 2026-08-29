// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission analysis model/Methods/Aerodynamics/Common/Fidelity_Zero/Lift/
// generate_vortex_distribution.py, the body of
// generate_wing_vortex_distribution's strip loop.
// Upstream: mission analysis model 2.5.2, LGPL-2.1.
// Reference: alas @ rust-port-baseline.

//! One side of one wing, while it is being laid out.
//!
//! The strip loop is the arithmetic half of the panelizer and its own file
//! for length: [`super`] decides *which* strips a wing has and where their
//! edges fall, and this decides where each strip's twenty-odd points end up.
//!
//! Everything here is double precision. The single rounding into `f32`
//! happens in [`Surface::append_to`], at the point upstream's
//! `dtype=precision` cast happens, and [`Lines`] says what rounding any
//! earlier would cost.

use super::super::types::VortexDistribution;

/// The spanwise geometry of one strip, in wing-local coordinates.
pub struct Strip {
    pub eta_a: f64,
    pub eta_b: f64,
    pub eta: f64,
    pub y_a: f64,
    pub y_b: f64,
    pub y_c: f64,
}

/// The trapezoid the strip sits in.
pub struct Section {
    pub root_chord_m: f64,
    pub root_twist_rad: f64,
    pub chord_ratio: f64,
    pub twist_ratio: f64,
    pub sweep_le_rad: f64,
    pub dihedral_rad: f64,
    pub x_offset_m: f64,
    pub z_offset_m: f64,
}

/// Ten named triples of coordinate arrays, held in double precision while a
/// surface is being built.
///
/// The rounding to `f32` happens once, in [`Surface::append_to`], and that
/// is where upstream's does too: it computes the whole strip, adds the wing
/// origin, and only then casts on the way into `VD`. Rounding earlier -- for
/// instance on the way out of the twist rotation, before the origin shift --
/// rounds twice, and the second rounding lands about a hundred of a
/// three-hundred-panel wing's corners one ulp away. That is invisible in the
/// corner itself and not invisible in the panel normal, whose streamwise
/// component is a difference of two nearly-parallel edges with almost no
/// significance left to lose.
#[derive(Default)]
struct Lines {
    x: Vec<f64>,
    y: Vec<f64>,
    z: Vec<f64>,
}

impl Lines {
    fn new(n: usize) -> Self {
        Self {
            x: vec![0.0; n],
            y: vec![0.0; n],
            z: vec![0.0; n],
        }
    }

    fn set(&mut self, i: usize, point: (f64, f64, f64)) {
        self.x[i] = point.0;
        self.y[i] = point.1;
        self.z[i] = point.2;
    }

    fn shift(&mut self, dx: f64, dy: f64, dz: f64) {
        for v in &mut self.x {
            *v += dx;
        }
        for v in &mut self.y {
            *v += dy;
        }
        for v in &mut self.z {
            *v += dz;
        }
    }

    fn round(&self) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
        let cast = |v: &Vec<f64>| v.iter().map(|&c| c as f32).collect();
        (cast(&self.x), cast(&self.y), cast(&self.z))
    }
}

/// One side of one wing, filled strip by strip before being appended.
pub struct Surface {
    a1: Lines,
    ah: Lines,
    ac: Lines,
    a2: Lines,
    b1: Lines,
    bh: Lines,
    bc: Lines,
    b2: Lines,
    ch: Lines,
    c: Lines,
    chord_lengths: Vec<f32>,
    tangent_incidence: Vec<f32>,
}

impl Surface {
    pub fn new(n_cw: usize, n_sw: usize) -> Self {
        let n = n_cw * n_sw;
        Self {
            a1: Lines::new(n),
            ah: Lines::new(n),
            ac: Lines::new(n),
            a2: Lines::new(n),
            b1: Lines::new(n),
            bh: Lines::new(n),
            bc: Lines::new(n),
            b2: Lines::new(n),
            ch: Lines::new(n),
            c: Lines::new(n),
            chord_lengths: vec![0.0; n],
            tangent_incidence: vec![0.0; n],
        }
    }

    /// Lay one strip's panels down.
    ///
    /// The three chordwise lines -- inboard edge, outboard edge and the
    /// strip's own centre -- are built independently, each on its own local
    /// chord, then all three are rotated about their own leading edge by the
    /// local twist. That is why the bound vortex of a twisted strip is not
    /// the average of the two edge vortices.
    pub fn lay_strip(
        &mut self,
        idx_y: usize,
        n_cw: usize,
        strip: &Strip,
        section: &Section,
        vertical: bool,
        inverted: f64,
    ) {
        let chord_a = section.root_chord_m + strip.eta_a * section.chord_ratio;
        let chord_b = section.root_chord_m + strip.eta_b * section.chord_ratio;
        let chord_c = section.root_chord_m + strip.eta * section.chord_ratio;

        let twist_a = section.root_twist_rad + strip.eta_a * section.twist_ratio;
        let twist_b = section.root_twist_rad + strip.eta_b * section.twist_ratio;
        let twist_c = section.root_twist_rad + strip.eta * section.twist_ratio;

        let tan_sweep = section.sweep_le_rad.tan();
        let tan_dihedral = section.dihedral_rad.tan();

        // The pivot is the leading edge of each chordwise line, before
        // camber -- which is zero here, so it is also the panel's own leading
        // edge.
        let pivot_x_a = section.x_offset_m + strip.eta_a * tan_sweep;
        let pivot_x_b = section.x_offset_m + strip.eta_b * tan_sweep;
        let pivot_x_c = section.x_offset_m + strip.eta * tan_sweep;
        let pivot_z_a = section.z_offset_m + strip.eta_a * tan_dihedral;
        let pivot_z_b = section.z_offset_m + strip.eta_b * tan_dihedral;
        let pivot_z_c = section.z_offset_m + strip.eta * tan_dihedral;

        let delta_a = chord_a / n_cw as f64;
        let delta_b = chord_b / n_cw as f64;
        let delta_c = chord_c / n_cw as f64;

        // `x_stations` are the panel boundaries along the local chord. With
        // no control-surface cut the nondimensional stations are a plain
        // `linspace(0, 1)`, and the `np.interp` upstream runs them through
        // against `[0, 1] -> [LE_cut, TE_cut]` is the identity.
        //
        // `k * (1 / n_cw)` and `k / n_cw` are not the same double, and the
        // difference is not academic: at `n_cw = 5` the third station comes
        // out one ulp apart between them, and a panel corner rounded to
        // `f32` on the wrong side of that shows up as a part in `1e3` in the
        // panel normal's streamwise component, which is a difference of two
        // nearly-parallel edges and has almost no significance left to lose.
        // NumPy's `linspace` multiplies by the step, so this does too.
        let step = 1.0 / n_cw as f64;
        let station = |k: usize, chord: f64| {
            if k == n_cw {
                chord
            } else {
                k as f64 * step * chord
            }
        };

        // The camber line is identically zero, so every point of a chordwise
        // line shares its leading edge's height and the rotation's second
        // argument is always zero. Upstream is inconsistent about *which*
        // camber height two of its ten rotations read -- `xi_prime_ac` takes
        // the bottom corner's and `xi_prime_bc` the top one's, neither
        // matching their own `zeta_prime_*` -- and with the camber line zero
        // there is no difference between those choices to reproduce. A port
        // that grows a camber line inherits the question.
        const CAMBER: f64 = 0.0;

        let base = idx_y * n_cw;
        for k in 0..n_cw {
            let (xa1, za1) = rotate(pivot_x_a, pivot_z_a, twist_a, station(k, chord_a), CAMBER);
            let (xah, zah) = rotate(
                pivot_x_a,
                pivot_z_a,
                twist_a,
                station(k, chord_a) + delta_a * 0.25,
                CAMBER,
            );
            let (xa2, za2) = rotate(
                pivot_x_a,
                pivot_z_a,
                twist_a,
                station(k + 1, chord_a),
                CAMBER,
            );
            let (xac, zac) = rotate(
                pivot_x_a,
                pivot_z_a,
                twist_a,
                station(k, chord_a) + delta_a * 0.75,
                CAMBER,
            );

            let (xb1, zb1) = rotate(pivot_x_b, pivot_z_b, twist_b, station(k, chord_b), CAMBER);
            let (xbh, zbh) = rotate(
                pivot_x_b,
                pivot_z_b,
                twist_b,
                station(k, chord_b) + delta_b * 0.25,
                CAMBER,
            );
            let (xb2, zb2) = rotate(
                pivot_x_b,
                pivot_z_b,
                twist_b,
                station(k + 1, chord_b),
                CAMBER,
            );
            let (xbc, zbc) = rotate(
                pivot_x_b,
                pivot_z_b,
                twist_b,
                station(k, chord_b) + delta_b * 0.75,
                CAMBER,
            );

            let (xch, zch) = rotate(
                pivot_x_c,
                pivot_z_c,
                twist_c,
                station(k, chord_c) + delta_c * 0.25,
                CAMBER,
            );
            let (xc, zc) = rotate(
                pivot_x_c,
                pivot_z_c,
                twist_c,
                station(k, chord_c) + delta_c * 0.75,
                CAMBER,
            );

            let corners = [
                (xa1, strip.y_a, za1),
                (xah, strip.y_a, zah),
                (xac, strip.y_a, zac),
                (xa2, strip.y_a, za2),
                (xb1, strip.y_b, zb1),
                (xbh, strip.y_b, zbh),
                (xbc, strip.y_b, zbc),
                (xb2, strip.y_b, zb2),
                (xch, strip.y_c, zch),
                (xc, strip.y_c, zc),
            ];
            let placed: Vec<(f64, f64, f64)> = corners
                .iter()
                .map(|&(x, y, z)| {
                    if vertical {
                        (x, z, inverted * y)
                    } else {
                        (x, y, z)
                    }
                })
                .collect();

            let i = base + k;
            self.a1.set(i, placed[0]);
            self.ah.set(i, placed[1]);
            self.ac.set(i, placed[2]);
            self.a2.set(i, placed[3]);
            self.b1.set(i, placed[4]);
            self.bh.set(i, placed[5]);
            self.bc.set(i, placed[6]);
            self.b2.set(i, placed[7]);
            self.ch.set(i, placed[8]);
            self.c.set(i, placed[9]);
        }

        // The strip's own chord and incidence, measured between the midpoint
        // of its leading edge and the midpoint of its trailing edge -- after
        // twist, and after a vertical surface's reflection, which is why a
        // fin reports zero incidence rather than its geometric twist. Both
        // are taken before the origin shift, as upstream takes them, and are
        // rounded to `f32` here because upstream rounds them here.
        let le_x = 0.5 * (self.a1.x[base] + self.b1.x[base]);
        let le_z = 0.5 * (self.a1.z[base] + self.b1.z[base]);
        let te = base + n_cw - 1;
        let te_x = 0.5 * (self.a2.x[te] + self.b2.x[te]);
        let te_z = 0.5 * (self.a2.z[te] + self.b2.z[te]);
        let chord = ((te_x - le_x).powi(2) + (te_z - le_z).powi(2)).sqrt() as f32;
        let incidence = ((le_z - te_z) / (le_x - te_x)) as f32;
        for k in 0..n_cw {
            self.chord_lengths[base + k] = chord;
            self.tangent_incidence[base + k] = incidence;
        }
    }

    /// Move every coordinate from wing-local to aircraft coordinates.
    pub fn shift_to_origin(&mut self, x: f64, y: f64, z: f64) {
        for line in [
            &mut self.ah,
            &mut self.bh,
            &mut self.ch,
            &mut self.a1,
            &mut self.a2,
            &mut self.b1,
            &mut self.b2,
            &mut self.ac,
            &mut self.bc,
            &mut self.c,
        ] {
            line.shift(x, y, z);
        }
    }

    /// Fill the trailing-edge arrays and hand the surface to the
    /// distribution.
    pub fn append_to(self, vd: &mut VortexDistribution, n_cw: usize, n_sw: usize) {
        let first_panel = vd.n_cp;
        let first_strip = vd.chordwise_breaks.len();
        vd.spanwise_breaks.push(first_strip);
        for idx_y in 0..n_sw {
            vd.chordwise_breaks.push(first_panel + idx_y * n_cw);
            for k in 0..n_cw {
                vd.leading_edge_indices.push(k == 0);
                vd.trailing_edge_indices.push(k == n_cw - 1);
                vd.panels_per_strip.push(n_cw);
                vd.chordwise_panel_number.push(k + 1);
            }
            // Always 1: the flag only drops to 0 on a strip whose leading
            // edge is covered by a non-slat control surface, and no control
            // surface is discretized.
            vd.exposed_leading_edge_flag.push(1);
        }

        vd.n_w += 1;
        vd.n_cp += n_cw * n_sw;
        vd.n_sw.push(n_sw);
        vd.n_cw.push(n_cw);
        vd.chord_lengths_m.extend_from_slice(&self.chord_lengths);
        vd.tangent_incidence_angle
            .extend_from_slice(&self.tangent_incidence);

        // The single rounding: every coordinate crosses from double to
        // single precision here and nowhere earlier.
        let d = &mut vd.panels;
        for (line, (xs, ys, zs)) in [
            (&self.ah, (&mut d.xah, &mut d.yah, &mut d.zah)),
            (&self.bh, (&mut d.xbh, &mut d.ybh, &mut d.zbh)),
            (&self.ch, (&mut d.xch, &mut d.ych, &mut d.zch)),
            (&self.a1, (&mut d.xa1, &mut d.ya1, &mut d.za1)),
            (&self.a2, (&mut d.xa2, &mut d.ya2, &mut d.za2)),
            (&self.b1, (&mut d.xb1, &mut d.yb1, &mut d.zb1)),
            (&self.b2, (&mut d.xb2, &mut d.yb2, &mut d.zb2)),
            (&self.ac, (&mut d.xac, &mut d.yac, &mut d.zac)),
            (&self.bc, (&mut d.xbc, &mut d.ybc, &mut d.zbc)),
            (&self.c, (&mut d.xc, &mut d.yc, &mut d.zc)),
        ] {
            let (x, y, z) = line.round();
            xs.extend_from_slice(&x);
            ys.extend_from_slice(&y);
            zs.extend_from_slice(&z);
        }

        // Each panel carries its own strip's trailing-edge corners, so that
        // the load integration can read them without an index lookup. They
        // are a copy of the strip's last panel's bottom corners, taken after
        // the origin shift as upstream takes them.
        for idx_y in 0..n_sw {
            let te = first_panel + idx_y * n_cw + n_cw - 1;
            for _ in 0..n_cw {
                d.xa_te.push(d.xa2[te]);
                d.ya_te.push(d.ya2[te]);
                d.za_te.push(d.za2[te]);
                d.xb_te.push(d.xb2[te]);
                d.yb_te.push(d.yb2[te]);
                d.zb_te.push(d.zb2[te]);
            }
        }
    }
}

/// Rotate one point of a chordwise line about that line's leading edge.
///
/// `pivot_x`/`pivot_z` are the leading edge; `offset` is the point's distance
/// aft of it along the unrotated chord line and `camber` its height above
/// it. Those two are upstream's `xi_* - pivot_x` and `zeta_* - pivot_z`,
/// which is the only form the rotation ever sees them in.
fn rotate(pivot_x: f64, pivot_z: f64, twist: f64, offset: f64, camber: f64) -> (f64, f64) {
    let (sin, cos) = twist.sin_cos();
    (
        pivot_x + cos * offset + sin * camber,
        pivot_z - sin * offset + cos * camber,
    )
}
