// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mass moment of inertia tensors: primitive solids, parallel axis, rotation.
//!
//! Frame and sign convention are those of the [module root](super): `x` aft,
//! `y` starboard, `z` up, `I = integral (|r|^2 E - r r^T) dm`, units kg m^2.

/// A 3-vector (x aft, y starboard, z up), metres unless stated.
pub type Vec3 = [f64; 3];

/// Symmetric inertia tensor, kg m^2.  Entry `[i][j]` is
/// `integral (|r|^2 delta_ij - r_i r_j) dm`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Inertia(pub [[f64; 3]; 3]);

impl Inertia {
    /// The zero tensor.
    pub const ZERO: Self = Self([[0.0; 3]; 3]);

    /// Diagonal tensor.
    #[must_use]
    pub fn diagonal(ixx: f64, iyy: f64, izz: f64) -> Self {
        Self([[ixx, 0.0, 0.0], [0.0, iyy, 0.0], [0.0, 0.0, izz]])
    }

    /// Solid rectangular box of mass `m` with edge lengths `dims` along x, y, z
    /// about its own centre: `I_xx = m (b^2 + c^2) / 12` and cyclic.
    #[must_use]
    pub fn solid_box(m: f64, dims: Vec3) -> Self {
        let [a, b, c] = dims;
        Self::diagonal(
            m * (b * b + c * c) / 12.0,
            m * (a * a + c * c) / 12.0,
            m * (a * a + b * b) / 12.0,
        )
    }

    /// Solid circular cylinder with its axis along x (length `l`, radius `r`).
    #[must_use]
    pub fn solid_cylinder_x(m: f64, r: f64, l: f64) -> Self {
        Self::diagonal(
            0.5 * m * r * r,
            m * (3.0 * r * r + l * l) / 12.0,
            m * (3.0 * r * r + l * l) / 12.0,
        )
    }

    /// Thin cylindrical shell with its axis along x (Sadraey, Table 11.11 and
    /// Example 11.3: `I_xx = m r^2`, `I_yy = I_zz = m (6 r^2 + L^2) / 12`).
    #[must_use]
    pub fn thin_shell_x(m: f64, r: f64, l: f64) -> Self {
        Self::diagonal(
            m * r * r,
            m * (6.0 * r * r + l * l) / 12.0,
            m * (6.0 * r * r + l * l) / 12.0,
        )
    }

    /// Point mass `m` displaced by `d` from the reference point:
    /// `m (|d|^2 E - d d^T)`.
    #[must_use]
    pub fn point_mass(m: f64, d: Vec3) -> Self {
        let d2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
        let mut t = [[0.0; 3]; 3];
        for (i, row) in t.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                let delta = if i == j { d2 } else { 0.0 };
                *v = m * (delta - d[i] * d[j]);
            }
        }
        Self(t)
    }

    /// Parallel-axis theorem: this tensor (about the body's own CG) moved to a
    /// point displaced by `d` from that CG.
    #[must_use]
    pub fn translated(self, m: f64, d: Vec3) -> Self {
        self.plus(Self::point_mass(m, d))
    }

    /// Sum of two tensors about the same point.
    #[must_use]
    pub fn plus(self, other: Self) -> Self {
        let mut t = self.0;
        for (row, orow) in t.iter_mut().zip(other.0) {
            for (v, o) in row.iter_mut().zip(orow) {
                *v += o;
            }
        }
        Self(t)
    }

    /// Scale every entry.
    #[must_use]
    pub fn scaled(self, k: f64) -> Self {
        let mut t = self.0;
        for row in &mut t {
            for v in row {
                *v *= k;
            }
        }
        Self(t)
    }

    /// Express in a frame rotated by `r` (`r` maps local axes to body axes):
    /// `R I R^T`.
    #[must_use]
    pub fn rotated(self, r: &[[f64; 3]; 3]) -> Self {
        let mut ri = [[0.0; 3]; 3];
        for (i, row) in ri.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = (0..3).map(|k| r[i][k] * self.0[k][j]).sum();
            }
        }
        let mut out = [[0.0; 3]; 3];
        for (i, row) in out.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = (0..3).map(|k| ri[i][k] * r[j][k]).sum();
            }
        }
        Self(out)
    }

    /// `I_xx`, kg m^2.
    #[must_use]
    pub fn ixx(&self) -> f64 {
        self.0[0][0]
    }

    /// `I_yy`, kg m^2.
    #[must_use]
    pub fn iyy(&self) -> f64 {
        self.0[1][1]
    }

    /// `I_zz`, kg m^2.
    #[must_use]
    pub fn izz(&self) -> f64 {
        self.0[2][2]
    }

    /// Product of inertia `integral x z dm` (the tensor entry is its negative).
    #[must_use]
    pub fn product_xz(&self) -> f64 {
        -self.0[0][2]
    }

    /// Whether every entry is finite.
    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.0.iter().flatten().all(|v| v.is_finite())
    }
}

/// Rotation matrix from Euler angles `(roll, pitch, yaw)` in degrees, applied
/// as `R = Rz(yaw) Ry(pitch) Rx(roll)` (the `Node::euler_deg` convention).
#[must_use]
pub fn rotation_rpy_deg(euler_deg: Vec3) -> [[f64; 3]; 3] {
    let [r, p, y] = euler_deg.map(f64::to_radians);
    let (sr, cr) = r.sin_cos();
    let (sp, cp) = p.sin_cos();
    let (sy, cy) = y.sin_cos();
    [
        [cy * cp, cy * sp * sr - sy * cr, cy * sp * cr + sy * sr],
        [sy * cp, sy * sp * sr + cy * cr, sy * sp * cr - cy * sr],
        [-sp, cp * sr, cp * cr],
    ]
}

/// Euclidean distance between two points.
#[must_use]
pub fn distance(a: Vec3, b: Vec3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}
