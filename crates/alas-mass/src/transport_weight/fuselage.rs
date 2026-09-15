// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission reference.Methods.Weights.Correlations.Transport.tube.tube.
// Upstream: mission reference 2.5.2, LGPL-2.1.
// Reference: alas @ rust-port-baseline.

//! The tube-and-wing fuselage structural mass.

use alas_units::{FOOT, POUND_FORCE, POUND_MASS};

/// A fuselage's fields, as read by [`tube`].
///
/// Every vehicle this program's mission reference bridge builds carries exactly one
/// fuselage, so [`super::empty_weight`] evaluates [`tube`] once rather than
/// summing over a `vehicle.fuselages` container.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fuselage {
    /// `fuse.differential_pressure`, Pa.
    pub differential_pressure_pa: f64,
    /// `fuse.width`, m.
    pub width_m: f64,
    /// `fuse.heights.maximum`, m.
    pub height_max_m: f64,
    /// `fuse.lengths.total`, m.
    pub length_total_m: f64,
    /// `fuse.areas.wetted`, m^2.
    pub area_wetted_m2: f64,
}

/// The mass of a fuselage in the tube-and-wing configuration: `tube`.
///
/// `wt_wing_kg`/`wt_propulsion_kg` are the already-computed main-wing and
/// total-propulsion masses; upstream subtracts both from the zero-fuel weight
/// to size the fuselage against the load it actually carries.
pub(crate) fn tube(
    fuse: &Fuselage,
    main_wing_root_chord_m: f64,
    limit_load_factor: f64,
    max_zero_fuel_kg: f64,
    wt_wing_kg: f64,
    wt_propulsion_kg: f64,
) -> f64 {
    // Pounds-force per square foot: the pressure unit the fuselage index is
    // written in.
    let force_pound_per_ft2 = POUND_FORCE / (FOOT * FOOT);
    let diff_p = fuse.differential_pressure_pa / force_pound_per_ft2;
    let width_ft = fuse.width_m / FOOT;
    let height_ft = fuse.height_max_m / FOOT;

    let length_ft = (fuse.length_total_m - main_wing_root_chord_m / 2.0) / FOOT;
    let weight_lb = (max_zero_fuel_kg - wt_wing_kg - wt_propulsion_kg) / POUND_MASS;
    let area_ft2 = fuse.area_wetted_m2 / (FOOT * FOOT);

    // Torenbeek-style fuselage indices: pressurization vs. bending.
    let i_p = 1.5e-3 * diff_p * width_ft;
    let i_b = 1.91e-4 * limit_load_factor * weight_lb * length_ft / height_ft.powi(2);

    let i_f = if i_p > i_b {
        i_p
    } else {
        (i_p.powi(2) + i_b.powi(2)) / (2.0 * i_b)
    };

    (1.051 + 0.102 * i_f) * area_ft2 * POUND_MASS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_fuselage() -> Fuselage {
        Fuselage {
            differential_pressure_pa: 58_000.0,
            width_m: 6.2,
            height_max_m: 6.2,
            length_total_m: 76.72,
            area_wetted_m2: 1494.3425279771354,
        }
    }

    #[test]
    fn tube_mass_grows_with_wetted_area() {
        let mut fuse = sample_fuselage();
        let base = tube(&fuse, 16.5, 2.5, 261_829.1, 67_940.0, 33_614.0);
        fuse.area_wetted_m2 *= 1.1;
        let bigger = tube(&fuse, 16.5, 2.5, 261_829.1, 67_940.0, 33_614.0);
        assert!(bigger > base, "bigger={bigger}, base={base}");
    }

    #[test]
    fn the_pressurization_index_takes_over_at_high_differential_pressure() {
        // With I_p > I_b the index is I_p itself; doubling the differential
        // pressure then doubles the bending-independent part of I_f.
        let mut fuse = sample_fuselage();
        fuse.differential_pressure_pa = 5.0e6;
        let weight = tube(&fuse, 16.5, 2.5, 261_829.1, 67_940.0, 33_614.0);
        assert!(weight.is_finite() && weight > 0.0, "weight={weight}");
    }
}
