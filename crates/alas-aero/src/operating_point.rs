// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from native aerodynamic model/performance/operating_point.py
// Upstream: native aerodynamic model 4.2.8, MIT.
// Reference: alas @ rust-port-baseline.

//! native aerodynamic model's `OperatingPoint`, scoped to the surface this program's
//! Python package and native aerodynamic model's own `VortexLatticeMethod` actually reach.
//!
//! `alas/physics/aerodynamics.py` constructs one
//! (`OperatingPoint(atmosphere=atmo, velocity=v, alpha=a)`) and passes it
//! straight to `the vortex-lattice solver`; it reads nothing off it directly.
//! `vortex_lattice_method.py`'s `run` and `run_with_stability_derivatives`
//! were grepped for every `op_point.<name>` and `self.op_point.<name>`
//! access, which is the whole reached surface: the seven constructor fields
//! (`atmosphere`, `velocity`, `alpha`, `beta`, `p`, `q`, `r` -- `velocity` is
//! also read directly, for the stability-derivative finite-difference step
//! sizes), [`OperatingPoint::dynamic_pressure`],
//! [`OperatingPoint::freestream_velocity_geometry_axes`],
//! [`OperatingPoint::rotation_velocity_geometry_axes`] and
//! [`OperatingPoint::convert_axes`]. `beta`, `p` and `r` are never read as
//! bare attributes by `run` itself -- the one place upstream does that is
//! `# self.op_point.beta == 0 and ...`, commented-out symmetry-detection code
//! that never executes -- but all three still feed
//! `rotation_velocity_geometry_axes` and `convert_axes` internally, so every
//! field is still part of the state this type has to carry.
//!
//! `compute_freestream_direction_geometry_axes` and
//! `compute_rotation_matrix_wind_to_geometry` are translated as private
//! helpers of [`OperatingPoint::freestream_velocity_geometry_axes`], matching
//! how upstream uses them: nothing outside `OperatingPoint` calls either one
//! directly.
//!
//! Left untranslated, because nothing in `vortex_lattice_method.py` or
//! `alas/physics/aerodynamics.py` reaches them: the `state`/
//! `get_new_instance_with_state`/`_set_state`/`unpack_state`/`pack_state`
//! vectorization machinery (nothing in this program's call sites is
//! vectorized across an array of operating points), `__repr__`/
//! `__getitem__`/`__len__`/`__array__`, and `total_pressure`/
//! `total_temperature`/`reynolds`/`mach`/`indicated_airspeed`/
//! `equivalent_airspeed`/`energy_altitude`. `docs/PORTING.md` records the
//! scoping decision.
//!
//! # `convert_axes`'s four branches, two reached
//!
//! [`AxisFrame`] has all four variants upstream's `convert_axes` accepts --
//! `Geometry`, `Body`, `Wind`, `Stability` -- and every branch of the
//! function is translated, since the branch logic is trivial, symmetric
//! algebra and cheap to keep complete. But `vortex_lattice_method.py` calls
//! `convert_axes` in exactly four places (on the near-field force and moment,
//! twice each), and every one of them is `from_axes="geometry", to_axes="body"`
//! or `from_axes="body", to_axes="wind"` -- `"stability"` is never reached
//! from that call site, and the fixture this module is checked against
//! exercises only those two pairs. The `Stability` branches are checked by
//! unit tests on properties that hold everywhere instead: round-tripping
//! through them is the identity, and every conversion preserves vector
//! length.
//!
//! Upstream represents axis frames as a bare `str` and raises `ValueError`
//! for anything else; [`AxisFrame`] makes that state impossible to construct
//! rather than translating a runtime check for it.

use alas_atmo::Atmosphere;

/// The reference frame a vector is expressed in -- geometry, body, wind or
/// stability axes. See the module doc for which pairs
/// [`OperatingPoint::convert_axes`] is actually exercised on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisFrame {
    /// X downstream, Z down, origin at the aircraft's geometric datum.
    Geometry,
    /// X forward, Z down, the frame the equations of motion are usually
    /// written in.
    Body,
    /// X aligned with the freestream, as seen at the current alpha and beta.
    Wind,
    /// Body axes rotated about Y by alpha alone (no beta rotation).
    Stability,
}

/// The instantaneous flight condition a VLM solve (or any other analysis)
/// evaluates: the atmosphere, the true airspeed, the attitude and the three
/// body-axis rotation rates.
///
/// `atmosphere`, `alpha` and `beta` are in the units upstream documents them
/// in: `alpha`/`beta` in degrees, `p`/`q`/`r` in rad/s, `velocity` in m/s.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OperatingPoint {
    /// The air the aircraft is flying through.
    pub atmosphere: Atmosphere,
    /// True airspeed, in m/s.
    pub velocity: f64,
    /// Angle of attack, in degrees.
    pub alpha: f64,
    /// Sideslip angle, in degrees. Positive means the oncoming air comes
    /// from the pilot's right-hand side.
    pub beta: f64,
    /// Roll rate about the body X axis, in rad/s.
    pub p: f64,
    /// Pitch rate about the body Y axis, in rad/s.
    pub q: f64,
    /// Yaw rate about the body Z axis, in rad/s.
    pub r: f64,
}

impl OperatingPoint {
    /// A new operating point. See the field docs for units.
    pub fn new(
        atmosphere: Atmosphere,
        velocity: f64,
        alpha: f64,
        beta: f64,
        p: f64,
        q: f64,
        r: f64,
    ) -> Self {
        Self {
            atmosphere,
            velocity,
            alpha,
            beta,
            p,
            q,
            r,
        }
    }

    /// Dynamic pressure of the working fluid, in Pa.
    pub fn dynamic_pressure(&self) -> f64 {
        0.5 * self.atmosphere.density() * self.velocity * self.velocity
    }

    /// The 3x3 rotation matrix from wind axes to geometry axes.
    ///
    /// `axes_flip @ alpha_rotation @ beta_rotation`, left-associated exactly
    /// as upstream evaluates it (`@` in Python), since a bicubic-free product
    /// of three rotation matrices can still differ in its last bit by
    /// grouping.
    fn rotation_matrix_wind_to_geometry(&self) -> [[f64; 3]; 3] {
        let alpha_rotation = rotate_y((-self.alpha).to_radians());
        let beta_rotation = rotate_z(self.beta.to_radians());
        // Geometry axes put X downstream and Z down, opposite wind axes'
        // upstream/up convention -- a 180-degree flip about Y.
        let axes_flip = rotate_y(std::f64::consts::PI);

        matmul3(matmul3(axes_flip, alpha_rotation), beta_rotation)
    }

    /// The freestream direction (the direction the wind is going *to*), in
    /// geometry axes.
    fn freestream_direction_geometry_axes(&self) -> [f64; 3] {
        matvec3(self.rotation_matrix_wind_to_geometry(), [-1.0, 0.0, 0.0])
    }

    /// The freestream velocity vector, in geometry axes.
    pub fn freestream_velocity_geometry_axes(&self) -> [f64; 3] {
        let direction = self.freestream_direction_geometry_axes();
        [
            direction[0] * self.velocity,
            direction[1] * self.velocity,
            direction[2] * self.velocity,
        ]
    }

    /// The effective velocity-due-to-rotation the aircraft's own rotation
    /// induces at each of `points`, in geometry axes.
    ///
    /// This is the velocity the wing *sees*, not the velocity of the wing --
    /// the sign of a rigid-body rotation's effect on the apparent local
    /// airflow is opposite the rotation itself, which is why upstream negates
    /// the raw cross product before returning it.
    pub fn rotation_velocity_geometry_axes(&self, points: &[[f64; 3]]) -> Vec<[f64; 3]> {
        // Signs convert p, q, r from body axes to geometry axes, matching
        // upstream's `[-p, q, -r]`.
        let angular_velocity = [-self.p, self.q, -self.r];
        points
            .iter()
            .map(|&point| {
                let raw = cross3(angular_velocity, point);
                [-raw[0], -raw[1], -raw[2]]
            })
            .collect()
    }

    /// Converts a vector `[x_from, y_from, z_from]`, given in `from_axes`, to
    /// its equivalent in `to_axes`.
    ///
    /// Wind axes rotations are taken from Eq. 6.7 in Sect. 6.2.2 of Drela's
    /// *Flight Vehicle Aerodynamics*, with axis corrections to go from
    /// `[D, Y, L]` to true wind axes -- the citation upstream carries.
    pub fn convert_axes(
        &self,
        x_from: f64,
        y_from: f64,
        z_from: f64,
        from_axes: AxisFrame,
        to_axes: AxisFrame,
    ) -> (f64, f64, f64) {
        if from_axes == to_axes {
            return (x_from, y_from, z_from);
        }

        let (x_b, y_b, z_b) = match from_axes {
            AxisFrame::Geometry => (-x_from, y_from, -z_from),
            AxisFrame::Body => (x_from, y_from, z_from),
            AxisFrame::Wind => {
                let (sa, ca) = (sind(self.alpha), cosd(self.alpha));
                let (sb, cb) = (sind(self.beta), cosd(self.beta));
                (
                    (cb * ca) * x_from + (-sb * ca) * y_from + (-sa) * z_from,
                    sb * x_from + cb * y_from, // z term is 0; not forgotten.
                    (cb * sa) * x_from + (-sb * sa) * y_from + ca * z_from,
                )
            }
            AxisFrame::Stability => {
                let (sa, ca) = (sind(self.alpha), cosd(self.alpha));
                (ca * x_from - sa * z_from, y_from, sa * x_from + ca * z_from)
            }
        };

        match to_axes {
            AxisFrame::Geometry => (-x_b, y_b, -z_b),
            AxisFrame::Body => (x_b, y_b, z_b),
            AxisFrame::Wind => {
                let (sa, ca) = (sind(self.alpha), cosd(self.alpha));
                let (sb, cb) = (sind(self.beta), cosd(self.beta));
                (
                    (cb * ca) * x_b + sb * y_b + (cb * sa) * z_b,
                    (-sb * ca) * x_b + cb * y_b + (-sb * sa) * z_b,
                    (-sa) * x_b + ca * z_b, // y term is 0; not forgotten.
                )
            }
            AxisFrame::Stability => {
                let (sa, ca) = (sind(self.alpha), cosd(self.alpha));
                (ca * x_b + sa * z_b, y_b, -sa * x_b + ca * z_b)
            }
        }
    }
}

/// `native aerodynamic model.numpy.sind`: sine of an angle given in degrees.
fn sind(degrees: f64) -> f64 {
    degrees.to_radians().sin()
}

/// `native aerodynamic model.numpy.cosd`: cosine of an angle given in degrees.
fn cosd(degrees: f64) -> f64 {
    degrees.to_radians().cos()
}

/// `rotation_matrix_3D(angle, axis="y")`: right-handed rotation about Y.
fn rotate_y(angle_rad: f64) -> [[f64; 3]; 3] {
    let (s, c) = (angle_rad.sin(), angle_rad.cos());
    [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]]
}

/// `rotation_matrix_3D(angle, axis="z")`: right-handed rotation about Z.
fn rotate_z(angle_rad: f64) -> [[f64; 3]; 3] {
    let (s, c) = (angle_rad.sin(), angle_rad.cos());
    [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]
}

/// `a @ b` for two 3x3 matrices.
fn matmul3(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut out = [[0.0; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    out
}

/// `m @ v` for a 3x3 matrix and a 3-vector.
fn matvec3(m: [[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/// The cross product `a x b`.
fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(alpha: f64, beta: f64) -> OperatingPoint {
        OperatingPoint::new(Atmosphere::new(1000.0), 100.0, alpha, beta, 0.0, 0.0, 0.0)
    }

    #[test]
    fn from_axes_equal_to_axes_is_the_identity_for_every_frame() {
        let point = op(7.0, -3.0);
        for frame in [
            AxisFrame::Geometry,
            AxisFrame::Body,
            AxisFrame::Wind,
            AxisFrame::Stability,
        ] {
            let (x, y, z) = point.convert_axes(1.0, 2.0, 3.0, frame, frame);
            assert_eq!((x, y, z), (1.0, 2.0, 3.0));
        }
    }

    #[test]
    fn every_axis_pair_round_trips_including_stability() {
        // `convert_axes` has no state of its own beyond alpha/beta, so a
        // round trip through any pair -- including the "stability" branches
        // the fixture never exercises -- should recover the original vector.
        let point = op(12.0, -5.5);
        let pairs = [
            (AxisFrame::Geometry, AxisFrame::Body),
            (AxisFrame::Body, AxisFrame::Wind),
            (AxisFrame::Geometry, AxisFrame::Wind),
            (AxisFrame::Body, AxisFrame::Stability),
            (AxisFrame::Wind, AxisFrame::Stability),
            (AxisFrame::Geometry, AxisFrame::Stability),
        ];
        let original = [3.1, -2.4, 0.9];
        for (from_axes, to_axes) in pairs {
            let (x, y, z) =
                point.convert_axes(original[0], original[1], original[2], from_axes, to_axes);
            let (rx, ry, rz) = point.convert_axes(x, y, z, to_axes, from_axes);
            assert!((rx - original[0]).abs() < 1e-12);
            assert!((ry - original[1]).abs() < 1e-12);
            assert!((rz - original[2]).abs() < 1e-12);
        }
    }

    #[test]
    fn convert_axes_preserves_vector_length_for_every_pair() {
        // Every branch is a rotation (geometry's is a pure sign flip, still
        // length-preserving), so no pair should stretch or shrink a vector.
        let point = op(-8.0, 15.0);
        let original: [f64; 3] = [2.0, -1.0, 4.0];
        let length =
            (original[0] * original[0] + original[1] * original[1] + original[2] * original[2])
                .sqrt();
        let frames = [
            AxisFrame::Geometry,
            AxisFrame::Body,
            AxisFrame::Wind,
            AxisFrame::Stability,
        ];
        for &from_axes in &frames {
            for &to_axes in &frames {
                let (x, y, z) =
                    point.convert_axes(original[0], original[1], original[2], from_axes, to_axes);
                let converted_length = (x * x + y * y + z * z).sqrt();
                assert!(
                    (converted_length - length).abs() < 1e-12,
                    "{from_axes:?} -> {to_axes:?} changed length: {length} -> {converted_length}"
                );
            }
        }
    }

    #[test]
    fn zero_alpha_and_beta_make_wind_axes_equal_body_axes() {
        let point = op(0.0, 0.0);
        let (x, y, z) = point.convert_axes(4.0, -2.0, 1.0, AxisFrame::Body, AxisFrame::Wind);
        assert!((x - 4.0).abs() < 1e-12);
        assert!((y - -2.0).abs() < 1e-12);
        assert!((z - 1.0).abs() < 1e-12);
    }

    #[test]
    fn dynamic_pressure_scales_with_velocity_squared() {
        let atmo = Atmosphere::new(0.0);
        let slow = OperatingPoint::new(atmo, 50.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let fast = OperatingPoint::new(atmo, 100.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert!((fast.dynamic_pressure() - 4.0 * slow.dynamic_pressure()).abs() < 1e-9);
    }

    #[test]
    fn freestream_velocity_has_magnitude_equal_to_true_airspeed() {
        for alpha in [-10.0, 0.0, 5.0, 20.0] {
            for beta in [-8.0, 0.0, 8.0] {
                let point =
                    OperatingPoint::new(Atmosphere::new(0.0), 123.0, alpha, beta, 0.0, 0.0, 0.0);
                let v = point.freestream_velocity_geometry_axes();
                let norm = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                assert!(
                    (norm - 123.0).abs() < 1e-9,
                    "alpha={alpha} beta={beta} norm={norm}"
                );
            }
        }
    }

    #[test]
    fn zero_alpha_and_beta_freestream_points_in_the_positive_x_geometry_direction() {
        // Geometry axes put X downstream, and at zero attitude the freestream
        // (the direction the wind is going *to*) is straight down that axis.
        let point = OperatingPoint::new(Atmosphere::new(0.0), 50.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let v = point.freestream_velocity_geometry_axes();
        assert!((v[0] - 50.0).abs() < 1e-9);
        assert!(v[1].abs() < 1e-9);
        assert!(v[2].abs() < 1e-9);
    }

    #[test]
    fn zero_rotation_rates_give_zero_rotation_velocity_everywhere() {
        let point = op(3.0, -2.0);
        let points = [[0.0, 0.0, 0.0], [10.0, 5.0, -3.0]];
        for v in point.rotation_velocity_geometry_axes(&points) {
            assert_eq!(v, [0.0, 0.0, 0.0]);
        }
    }

    #[test]
    fn rotation_velocity_is_linear_in_the_rotation_rates() {
        // The formula is a cross product of a linear function of p/q/r with
        // the point, so doubling every rate should double the result.
        let atmo = Atmosphere::new(0.0);
        let point = point_with_rates(atmo, 0.01, -0.02, 0.03);
        let doubled = point_with_rates(atmo, 0.02, -0.04, 0.06);
        let points = [[1.0, 2.0, 3.0]];

        let base = point.rotation_velocity_geometry_axes(&points)[0];
        let scaled = doubled.rotation_velocity_geometry_axes(&points)[0];
        for i in 0..3 {
            assert!((scaled[i] - 2.0 * base[i]).abs() < 1e-12);
        }
    }

    fn point_with_rates(atmosphere: Atmosphere, p: f64, q: f64, r: f64) -> OperatingPoint {
        OperatingPoint::new(atmosphere, 100.0, 0.0, 0.0, p, q, r)
    }

    #[test]
    fn rotation_velocity_at_the_origin_is_zero_regardless_of_rates() {
        // The velocity due to rotation about the origin is `omega x r`; at
        // `r = 0` that is zero for any angular rate.
        let point = point_with_rates(Atmosphere::new(0.0), 0.5, -0.3, 0.2);
        let v = point.rotation_velocity_geometry_axes(&[[0.0, 0.0, 0.0]])[0];
        assert_eq!(v, [0.0, 0.0, 0.0]);
    }
}
