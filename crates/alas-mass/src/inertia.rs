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
//! modules, and Raymer's radii-of-gyration table is the
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
/// Applying the same fractions to the full span and full length overstates
/// the measured B747-100 tensor (NASA CR-2144) by a factor of 2.6 to 4.6, so
/// the published half-dimension definition is used here. The radii are a
/// cross-check on the ledger result and a fallback when no ledger exists,
/// never a replacement for it.
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
    fn a_long_cylinder_has_no_moment_about_its_own_axis() {
        let long = solid_cylinder_x(12.0, 0.0, 6.0);
        assert!(close(long.iyy, 36.0));
        assert!(close(long.ixx, 0.0));
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
        let [rx, ry, rz] = RadiiOfGyration::JET_TRANSPORT.radii_m(10.0, 20.0);
        assert!(close(rx, 0.25 * 5.0));
        assert!(close(ry, 0.38 * 10.0));
        assert!(close(rz, 0.39 * 7.5));
    }
}
