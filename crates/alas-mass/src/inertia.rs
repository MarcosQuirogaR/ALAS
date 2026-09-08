// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Centroidal inertia tensors of the solids a conceptual airframe is made of.
//!
//! A component ledger needs each item's own tensor before the parallel-axis
//! theorem can carry it to the aircraft centre of gravity. At conceptual
//! fidelity the items are well represented by a handful of solids whose
//! tensors are closed-form: a wing panel as a thin trapezoidal plate, a
//! fuselage as a thin-walled cylinder with hemispherical-cap correction
//! ignored, an engine as a solid cylinder, a fuel tank as a rectangular
//! prism, and anything without extent as a point. The same solids appear in
//! NASA TM-78681's rigid-structure program and in RCAIDE's moment-of-inertia
//! modules, and Raymer's radii-of-gyration table is retained as the
//! aircraft-level check the ledger result is compared with.
//!
//! Every function returns a tensor about the solid's own centroid in axes
//! parallel to the geometry frame.

use crate::ledger::InertiaTensor;

/// A thin flat plate of uniform areal density lying in the x-y plane.
///
/// `length_x_m` and `width_y_m` are the plate's extents along the two body
/// axes; the plate has no thickness, so the moment about z is the sum of the
/// other two. A sweep or taper is represented by the caller by splitting the
/// surface into panels and placing each at its own centroid.
pub fn thin_plate_xy(mass_kg: f64, length_x_m: f64, width_y_m: f64) -> InertiaTensor {
    let ixx = mass_kg * width_y_m * width_y_m / 12.0;
    let iyy = mass_kg * length_x_m * length_x_m / 12.0;
    InertiaTensor::diagonal(ixx, iyy, ixx + iyy)
}

/// A thin flat plate of uniform areal density lying in the x-z plane, which
/// is the orientation of a vertical tail.
pub fn thin_plate_xz(mass_kg: f64, length_x_m: f64, height_z_m: f64) -> InertiaTensor {
    let ixx = mass_kg * height_z_m * height_z_m / 12.0;
    let izz = mass_kg * length_x_m * length_x_m / 12.0;
    InertiaTensor::diagonal(ixx, ixx + izz, izz)
}

/// A thin-walled circular cylinder with its axis along x.
///
/// This is the shell of a pressurised fuselage: all its mass sits at the
/// radius, so the moment about the axis is `m r^2` and the transverse
/// moments carry the `m L^2 / 12` term of a rod plus half the hoop term.
pub fn thin_cylinder_shell_x(mass_kg: f64, radius_m: f64, length_m: f64) -> InertiaTensor {
    let ixx = mass_kg * radius_m * radius_m;
    let transverse = mass_kg * (radius_m * radius_m / 2.0 + length_m * length_m / 12.0);
    InertiaTensor::diagonal(ixx, transverse, transverse)
}

/// A solid circular cylinder with its axis along x, the shape of an engine.
pub fn solid_cylinder_x(mass_kg: f64, radius_m: f64, length_m: f64) -> InertiaTensor {
    let ixx = mass_kg * radius_m * radius_m / 2.0;
    let transverse = mass_kg * (radius_m * radius_m / 4.0 + length_m * length_m / 12.0);
    InertiaTensor::diagonal(ixx, transverse, transverse)
}

/// A rectangular prism of uniform density with edges along the body axes.
///
/// A fuel tank between two spars and two ribs, or a cargo container, is
/// this solid to conceptual accuracy.
pub fn rectangular_prism(
    mass_kg: f64,
    length_x_m: f64,
    width_y_m: f64,
    height_z_m: f64,
) -> InertiaTensor {
    let sq = |value: f64| value * value;
    InertiaTensor::diagonal(
        mass_kg * (sq(width_y_m) + sq(height_z_m)) / 12.0,
        mass_kg * (sq(length_x_m) + sq(height_z_m)) / 12.0,
        mass_kg * (sq(length_x_m) + sq(width_y_m)) / 12.0,
    )
}

/// A uniform rod along y, which stands in for a slender item such as a
/// landing-gear axle or a distributed system run across the span.
pub fn thin_rod_y(mass_kg: f64, length_m: f64) -> InertiaTensor {
    let transverse = mass_kg * length_m * length_m / 12.0;
    InertiaTensor::diagonal(transverse, 0.0, transverse)
}

/// Non-dimensional radii of gyration of a whole aircraft.
///
/// Raymer (*Aircraft Design: A Conceptual Approach*, table 16.1, after
/// Roskam Part V) tabulates these per aircraft class from measured aircraft
/// and defines them against the half dimensions:
///
/// ```text
/// I_xx = m (R_x b / 2)^2
/// I_yy = m (R_y L / 2)^2
/// I_zz = m (R_z e / 2)^2,   e = (b + L) / 2
/// ```
///
/// The frozen `alas-stab` estimate applies the same fractions to the full
/// span and full length, which is 2.6 to 4.6 times the measured B747-100
/// tensor (NASA CR-2144) and is retained there only for parity; here the
/// published definition is used. The radii are a cross-check on the ledger
/// result and a fallback when no ledger exists, never a replacement for it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadiiOfGyration {
    /// Roll radius as a fraction of the half-span.
    pub roll_span_fraction: f64,
    /// Pitch radius as a fraction of the half-length.
    pub pitch_length_fraction: f64,
    /// Yaw radius as a fraction of half the mean of span and length.
    pub yaw_length_fraction: f64,
}

impl RadiiOfGyration {
    /// Raymer's jet-transport row.
    pub const JET_TRANSPORT: Self = Self {
        roll_span_fraction: 0.25,
        pitch_length_fraction: 0.38,
        yaw_length_fraction: 0.39,
    };

    /// Raymer's twin-turboprop row.
    pub const TWIN_TURBOPROP: Self = Self {
        roll_span_fraction: 0.22,
        pitch_length_fraction: 0.34,
        yaw_length_fraction: 0.38,
    };

    /// The measured B747-100 (NASA CR-2144, Heffley and Jewell 1972) reduced
    /// to the same definition: `I_xx 2.468e7`, `I_yy 4.488e7`,
    /// `I_zz 6.738e7` kg m^2 at 288,760 kg, 59.64 m span and 70.66 m length.
    /// A validation anchor for a large four-engine transport.
    pub const B747_100_MEASURED: Self = Self {
        roll_span_fraction: 0.310,
        pitch_length_fraction: 0.353,
        yaw_length_fraction: 0.469,
    };

    /// The dimensional radii `[r_x, r_y, r_z]` in metres for the given span
    /// and fuselage length.
    pub fn radii_m(&self, span_m: f64, fuselage_length_m: f64) -> [f64; 3] {
        let e = 0.5 * (span_m + fuselage_length_m);
        [
            self.roll_span_fraction * 0.5 * span_m,
            self.pitch_length_fraction * 0.5 * fuselage_length_m,
            self.yaw_length_fraction * 0.5 * e,
        ]
    }

    /// The diagonal tensor the fractions imply for `mass_kg` at the given
    /// span and fuselage length.
    pub fn tensor(&self, mass_kg: f64, span_m: f64, fuselage_length_m: f64) -> InertiaTensor {
        let [rx, ry, rz] = self.radii_m(span_m, fuselage_length_m);
        InertiaTensor::diagonal(mass_kg * rx * rx, mass_kg * ry * ry, mass_kg * rz * rz)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(left: f64, right: f64) -> bool {
        (left - right).abs() <= 1.0e-12 * left.abs().max(right.abs()).max(1.0)
    }

    #[test]
    fn a_square_plate_has_equal_in_plane_moments_and_their_sum_normal_to_it() {
        let plate = thin_plate_xy(12.0, 2.0, 2.0);
        assert!(close(plate.ixx, 4.0));
        assert!(close(plate.iyy, 4.0));
        assert!(close(plate.izz, 8.0));
        assert!(plate.is_physical());
        let fin = thin_plate_xz(12.0, 2.0, 2.0);
        assert!(close(fin.iyy, 8.0));
    }

    #[test]
    fn a_thin_shell_carries_all_its_mass_at_the_radius() {
        let shell = thin_cylinder_shell_x(10.0, 3.0, 0.0);
        assert!(close(shell.ixx, 90.0));
        assert!(close(shell.iyy, 45.0));
        let solid = solid_cylinder_x(10.0, 3.0, 0.0);
        assert!(close(solid.ixx, 45.0));
        assert!(shell.is_physical() && solid.is_physical());
    }

    #[test]
    fn a_long_cylinder_approaches_a_rod_transversely() {
        let long = solid_cylinder_x(12.0, 0.0, 6.0);
        assert!(close(long.iyy, 36.0));
        assert!(close(long.ixx, 0.0));
        let rod = thin_rod_y(12.0, 6.0);
        assert!(close(rod.ixx, 36.0));
        assert!(close(rod.iyy, 0.0));
    }

    #[test]
    fn a_cube_has_equal_moments_about_every_axis() {
        let cube = rectangular_prism(6.0, 1.0, 1.0, 1.0);
        assert!(close(cube.ixx, 1.0));
        assert!(close(cube.iyy, 1.0));
        assert!(close(cube.izz, 1.0));
    }

    #[test]
    fn the_radii_use_raymers_half_dimension_definition() {
        let tensor = RadiiOfGyration::JET_TRANSPORT.tensor(1000.0, 10.0, 20.0);
        assert!(close(tensor.ixx, 1000.0 * (0.25_f64 * 5.0).powi(2)));
        assert!(close(tensor.iyy, 1000.0 * (0.38_f64 * 10.0).powi(2)));
        assert!(close(tensor.izz, 1000.0 * (0.39_f64 * 7.5).powi(2)));
    }

    #[test]
    fn the_b747_anchor_reproduces_the_measured_tensor() {
        // NASA CR-2144 values in kg m^2, at the mass and dimensions the
        // anchor row was reduced from; a 1 percent band covers the rounding
        // of the published slug-ft^2 figures.
        let tensor = RadiiOfGyration::B747_100_MEASURED.tensor(288_760.0, 59.64, 70.66);
        let within = |value: f64, expected: f64| (value / expected - 1.0).abs() < 0.01;
        assert!(within(tensor.ixx, 2.468e7), "ixx {}", tensor.ixx);
        assert!(within(tensor.iyy, 4.488e7), "iyy {}", tensor.iyy);
        assert!(within(tensor.izz, 6.738e7), "izz {}", tensor.izz);
    }
}
