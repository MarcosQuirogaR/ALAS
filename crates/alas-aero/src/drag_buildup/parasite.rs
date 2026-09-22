// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission analysis model/Methods/Aerodynamics/Common/Fidelity_Zero/Drag/
//   parasite_drag_wing.py, parasite_drag_fuselage.py, parasite_drag_nacelle.py,
//   parasite_drag_pylon.py and parasite_total.py
// Upstream: mission analysis model 2.5.2, LGPL-2.1.
// Reference: alas @ rust-port-baseline.

//! Parasite drag, component by component.
//!
//! Each function returns the component's coefficient on *its own* reference
//! area, which is what upstream's per-component methods return.
//! [`scale_to_vehicle_reference`] then rescales it to the vehicle reference
//! area, reproducing the in-place rewrite `parasite_total` performs: a
//! detail that is not cosmetic, because the induced-drag buildup reads the
//! rescaled value back out.

use std::f64::consts::PI;

use super::flat_plate::{compressible_mixed_flat_plate, compressible_turbulent_flat_plate};
use super::types::{
    ComponentParasiteDrag, DragVehicle, Freestream, FuselageParams, NacelleParams, WingParams,
};

/// The cubic Hermite blend `Cubic_Spline_Blender(0.95, 1.0).compute(M)`.
///
/// Written with the interval as `1.0 - 0.95` rather than as `0.05` on
/// purpose: those are different doubles, and the difference is visible in the
/// blend's output for a Mach number inside the band.
pub fn blend_to_sonic(mach: f64) -> f64 {
    let eta = (mach - 0.95) / (1.0 - 0.95);
    if eta < 0.0 {
        1.0
    } else if eta > 1.0 {
        0.0
    } else {
        2.0 * eta * eta * eta - 3.0 * eta * eta + 1.0
    }
}

/// Parasite drag of one wing, on the wing's own reference area.
///
/// Scoped to the unsegmented branch: `parasite_drag_wing` sums over
/// `wing.Segments` when there are any, and `vehicle_builder.py` never appends
/// one, which `gen_aero_drag_buildup.py` refuses to write a fixture without
/// confirming. The wetted-area recalculation is likewise unreached:
/// `recalculate_total_wetted_area` is `false` in `Fidelity_Zero`'s defaults
/// and `simple_sizing` sets every `areas.wetted` to a nonzero value before
/// the analysis runs, so upstream's `or wing.areas.wetted == 0.` fallback
/// cannot fire.
pub fn parasite_drag_wing(
    freestream: &Freestream,
    form_factor: f64,
    wing: &WingParams,
) -> ComponentParasiteDrag {
    let re = freestream.reynolds_number_per_m * wing.mean_aerodynamic_chord_m;
    let upper = compressible_mixed_flat_plate(
        re,
        freestream.mach,
        freestream.temperature_k,
        wing.transition_x_upper,
    );
    let lower = compressible_mixed_flat_plate(
        re,
        freestream.mach,
        freestream.temperature_k,
        wing.transition_x_lower,
    );

    let cos_sweep = wing.quarter_chord_sweep_rad.cos();
    let cos2 = cos_sweep * cos_sweep;
    let mach = freestream.mach;
    let tc = wing.thickness_to_chord;

    // Upstream evaluates this only where `Mc <= 1`, leaving `k_w` at 1
    // elsewhere. Above Mach 1 the radicands go negative, so the branch is
    // load-bearing rather than an optimization.
    let mut form = 1.0;
    if mach <= 1.0 {
        form = 1.0
            + (2.0 * form_factor * (tc * cos2)) / (1.0 - mach * mach * cos2).sqrt()
            + (form_factor * form_factor * cos2 * tc * tc * (1.0 + 5.0 * cos2))
                / (2.0 * (1.0 - (mach * cos_sweep).powi(2)));
    }

    let blend = blend_to_sonic(mach);
    form = form * blend + 1.0 * (1.0 - blend);

    let area_ratio = wing.wetted_area_m2 / wing.reference_area_m2;
    let parasite = form * upper.cf * area_ratio / 2.0 + form * lower.cf * area_ratio / 2.0;

    ComponentParasiteDrag {
        parasite_drag_coefficient: parasite,
        skin_friction_coefficient: (upper.cf + lower.cf) / 2.0,
        form_factor: form,
        compressibility_factor: (upper.k_comp + lower.k_comp) / 2.0,
        reynolds_factor: (upper.k_reyn + lower.k_reyn) / 2.0,
    }
}

/// Parasite drag of one fuselage, on its front projected area.
pub fn parasite_drag_fuselage(
    freestream: &Freestream,
    form_factor: f64,
    fuselage: &FuselageParams,
) -> ComponentParasiteDrag {
    let re = freestream.reynolds_number_per_m * fuselage.length_m;
    let friction = compressible_turbulent_flat_plate(re, freestream.mach, freestream.temperature_k);

    let mach = freestream.mach;
    let fineness = fuselage.effective_diameter_m / fuselage.length_m;

    // The peak velocity increment over an equivalent ellipsoid. Upstream
    // splits at Mach 0.95 rather than at 1: above the split the
    // Prandtl-Glauert factor is dropped entirely instead of going imaginary.
    let du_max_u = if mach < 0.95 {
        let beta2 = 1.0 - mach * mach;
        let d = (1.0 - beta2 * fineness * fineness).sqrt();
        let a = 2.0 * beta2 * (fineness * fineness) * (d.atanh() - d) / (d * d * d);
        a / ((2.0 - a) * beta2.sqrt())
    } else {
        let d = (1.0 - fineness * fineness).sqrt();
        let a = 2.0 * (fineness * fineness) * (d.atanh() - d) / (d * d * d);
        a / (2.0 - a)
    };

    let form = (1.0 + form_factor * du_max_u).powi(2);

    ComponentParasiteDrag {
        parasite_drag_coefficient: form * friction.cf * fuselage.wetted_area_m2
            / fuselage.front_projected_area_m2,
        skin_friction_coefficient: friction.cf,
        form_factor: form,
        compressibility_factor: friction.k_comp,
        reynolds_factor: friction.k_reyn,
    }
}

/// The frontal area a nacelle's coefficient is expressed on.
pub fn nacelle_reference_area(nacelle: &NacelleParams) -> f64 {
    nacelle.diameter_m * nacelle.diameter_m / 4.0 * PI
}

/// Parasite drag of one nacelle, on its frontal area.
pub fn parasite_drag_nacelle(
    freestream: &Freestream,
    nacelle: &NacelleParams,
) -> ComponentParasiteDrag {
    let re = freestream.reynolds_number_per_m * nacelle.length_m;
    let friction = compressible_turbulent_flat_plate(re, freestream.mach, freestream.temperature_k);

    let form = 1.0 + 0.35 / (nacelle.length_m / nacelle.diameter_m);

    ComponentParasiteDrag {
        parasite_drag_coefficient: form * friction.cf * nacelle.wetted_area_m2
            / nacelle_reference_area(nacelle),
        skin_friction_coefficient: friction.cf,
        form_factor: form,
        compressibility_factor: friction.k_comp,
        reynolds_factor: friction.k_reyn,
    }
}

/// The pylons' parasite drag, as a flat 20% of the nacelles'.
///
/// Already on the *vehicle* reference area, unlike every other component
/// here: upstream scales each nacelle's contribution as it accumulates, and
/// `parasite_total` then adds this entry without scaling it again.
///
/// Its four reported factors are sums over the nacelles divided by the
/// *network* count, not by the nacelle count, so on a two-engine single-network
/// aircraft each reads twice a nacelle's. That is upstream's arithmetic and
/// nothing consumes it; reproduced rather than averaged, and the fixture
/// records it so the choice is visible rather than inferred.
pub fn parasite_drag_pylon(
    vehicle: &DragVehicle<'_>,
    nacelles: &[ComponentParasiteDrag],
) -> ComponentParasiteDrag {
    const PYLON_FACTOR: f64 = 0.20;

    let mut parasite = 0.0;
    let mut skin_friction = 0.0;
    let mut form = 0.0;
    let mut compressibility = 0.0;
    let mut reynolds = 0.0;

    for (nacelle, drag) in vehicle.nacelles.iter().zip(nacelles) {
        let area_ratio = nacelle_reference_area(nacelle) / vehicle.reference_area_m2;
        parasite += PYLON_FACTOR * drag.parasite_drag_coefficient * area_ratio;
        skin_friction += drag.skin_friction_coefficient;
        form += drag.form_factor;
        compressibility += drag.compressibility_factor;
        reynolds += drag.reynolds_factor;
    }

    let networks = vehicle.network_count as f64;

    ComponentParasiteDrag {
        parasite_drag_coefficient: parasite,
        skin_friction_coefficient: skin_friction / networks,
        form_factor: form / networks,
        compressibility_factor: compressibility / networks,
        reynolds_factor: reynolds / networks,
    }
}

/// Rescale a component's coefficient from its own reference area to the
/// vehicle's, as `parasite_total` does in place.
///
/// Split out and named because the rewrite is observable: `induced_drag_aircraft`
/// runs after `parasite_total` and reads `parasite[tag].parasite_drag_coefficient`,
/// so the viscous induced drag is built on the *rescaled* value. A port that
/// kept the two coefficients separate and passed the unscaled one on would
/// agree on parasite drag and disagree on induced.
pub fn scale_to_vehicle_reference(
    drag: &mut ComponentParasiteDrag,
    component_reference_area_m2: f64,
    vehicle_reference_area_m2: f64,
) {
    drag.parasite_drag_coefficient *= component_reference_area_m2 / vehicle_reference_area_m2;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_blend_is_one_below_the_band_and_zero_above_it() {
        assert_eq!(blend_to_sonic(0.0), 1.0);
        assert_eq!(blend_to_sonic(0.94), 1.0);
        assert_eq!(blend_to_sonic(0.95), 1.0);
        assert_eq!(blend_to_sonic(1.0), 0.0);
        assert_eq!(blend_to_sonic(1.5), 0.0);
    }

    #[test]
    fn the_blend_is_monotone_and_flat_at_both_ends_of_the_band() {
        // A cubic Hermite patch, so its first derivative vanishes at each
        // end, which is the whole reason upstream uses one instead of a
        // straight line, and what stops the form factor kinking at Mach 0.95.
        let mut previous = 1.0;
        for step in 0..=50 {
            let mach = 0.95 + (1.0 - 0.95) * f64::from(step) / 50.0;
            let value = blend_to_sonic(mach);
            assert!(value <= previous + 1e-15);
            previous = value;
        }
        assert!((blend_to_sonic(0.975) - 0.5).abs() < 1e-15);
    }

    #[test]
    fn the_band_interval_is_not_five_hundredths() {
        // `1.0 - 0.95` is not the double nearest 0.05, and the blend divides
        // by it. Stated so that a later simplification to `/ 0.05` is
        // recognized as a change rather than a tidy-up.
        let band_end = 1.0_f64;
        let band_start = 0.95_f64;
        assert_ne!(band_end - band_start, 0.05_f64);
    }

    #[test]
    fn a_nacelle_reference_area_is_its_frontal_disc() {
        let nacelle = NacelleParams {
            length_m: 7.8,
            diameter_m: 4.0,
            wetted_area_m2: 100.0,
            origin_count: 1,
        };
        assert!((nacelle_reference_area(&nacelle) - 4.0 * PI).abs() < 1e-12);
    }

    #[test]
    fn rescaling_to_the_vehicle_reference_leaves_the_reference_wing_alone() {
        // The main wing's reference area *is* the vehicle's, so the rewrite
        // is the identity there and only the surfaces contribute a ratio.
        let mut drag = ComponentParasiteDrag {
            parasite_drag_coefficient: 0.006,
            skin_friction_coefficient: 0.0024,
            form_factor: 1.21,
            compressibility_factor: 0.995,
            reynolds_factor: 0.998,
        };
        scale_to_vehicle_reference(&mut drag, 529.0, 529.0);
        assert_eq!(drag.parasite_drag_coefficient, 0.006);
    }
}
