// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission analysis model/Methods/Aerodynamics/Common/Fidelity_Zero/Drag/
//   induced_drag_aircraft.py, compressibility_drag_wing.py,
//   compressibility_drag_wing_total.py and miscellaneous_drag_aircraft_ESDU.py
// Upstream: mission analysis model 2.5.2, LGPL-2.1.
// Reference: alas @ rust-port-baseline.

//! The three contributions parasite drag does not cover: induced,
//! compressibility and excrescence drag.

use super::types::{
    ComponentParasiteDrag, DragVehicle, Freestream, WingCompressibilityDrag, WingParams,
};

/// What the induced-drag buildup reports.
pub struct InducedDrag {
    /// The viscous and inviscid parts, summed.
    pub total: f64,
    /// The lift-dependent viscous part alone.
    pub viscous: f64,
    /// Each wing's viscous contribution *before* the area scaling the total
    /// applies, which is the form upstream reports it in.
    pub viscous_wings: Vec<f64>,
}

/// Induced drag, viscous and inviscid.
///
/// Scoped to `oswald_efficiency_factor = None` and `span_efficiency = None`,
/// which are `Fidelity_Zero`'s defaults and which `mission_builder.py`
/// overrides neither of. Both alternatives are untranslated because nothing
/// this program builds can reach them: a set Oswald factor replaces the whole
/// buildup with `CL^2 / (pi AR e)`, and a set span efficiency replaces the
/// vortex lattice's per-wing inviscid drag with the same closed form. With
/// both `None` the inviscid half is *the vortex lattice's own answer*, which
/// is why [`WingParams::inviscid_induced_drag_coefficient`] is an input.
///
/// `nacelles` is not a parameter because upstream's loop runs over wings
/// alone: this model has no way to attribute induced drag to a body, which
/// its own docstring acknowledges.
pub fn induced_drag_aircraft(
    vehicle: &DragVehicle<'_>,
    wing_parasite: &[ComponentParasiteDrag],
    viscous_lift_dependent_drag_factor: f64,
) -> InducedDrag {
    let mut viscous = 0.0;
    let mut inviscid = 0.0;
    let mut viscous_wings = Vec::with_capacity(vehicle.wings.len());

    for (wing, parasite) in vehicle.wings.iter().zip(wing_parasite) {
        let area_ratio = wing.reference_area_m2 / vehicle.reference_area_m2;
        let lift = wing.inviscid_lift_coefficient;

        // `parasite.parasite_drag_coefficient` has already been rescaled to
        // the vehicle reference area by `parasite_total`, and this reads the
        // rescaled value. See `parasite::scale_to_vehicle_reference`.
        let wing_viscous =
            viscous_lift_dependent_drag_factor * parasite.parasite_drag_coefficient * lift * lift;
        viscous += wing_viscous * area_ratio;
        inviscid += wing.inviscid_induced_drag_coefficient * area_ratio;
        viscous_wings.push(wing_viscous);
    }

    InducedDrag {
        total: viscous + inviscid,
        viscous,
        viscous_wings,
    }
}

/// Compressibility drag of one wing.
///
/// The crest-critical Mach number is a six-term quadratic fit in the
/// sweep-corrected thickness and lift coefficient; the drag rise past it is a
/// power law with an exponent of 14.641, which is what makes this term go
/// from numerically zero in the climb to the largest one in the fixture at
/// Mach 1.
pub fn compressibility_drag_wing(
    freestream: &Freestream,
    wing: &WingParams,
) -> WingCompressibilityDrag {
    let cos_sweep = wing.quarter_chord_sweep_rad.cos();
    let tc = wing.thickness_to_chord / cos_sweep;
    let cl = wing.inviscid_lift_coefficient / (cos_sweep * cos_sweep);

    let mcc_cos_ws = 0.922_321_524_499_352 - 1.153_885_166_170_62 * tc - 0.304_541_067_183_461 * cl
        + 0.332_881_324_404_729 * tc * tc
        + 0.467_317_361_111_105 * tc * cl
        + 0.087_490_431_201_549 * cl * cl;

    let crest_critical = mcc_cos_ws / cos_sweep;
    let divergence_mach = crest_critical * (1.02 + 0.08 * (1.0 - cos_sweep));
    let mach_ratio = freestream.mach / crest_critical;

    let rise = 0.0019 * mach_ratio.powf(14.641);

    WingCompressibilityDrag {
        compressibility_drag: rise * cos_sweep * cos_sweep * cos_sweep,
        crest_critical,
        divergence_mach,
    }
}

/// The wings' compressibility drag, reported on the aircraft reference area.
///
/// [`compressibility_drag_wing`] returns each coefficient on that wing's own
/// reference area.  A coefficient sum is therefore only valid when every
/// wing uses the same area; tails do not.  Convert each term back to force
/// (`q * S_wing * C_D,w`) and normalize the sum by the vehicle reference area
/// so the result is a single aircraft-level coefficient.
pub fn compressibility_drag_total(
    vehicle: &DragVehicle<'_>,
    wings: &[WingCompressibilityDrag],
) -> f64 {
    vehicle
        .wings
        .iter()
        .zip(wings)
        .map(|(wing, drag)| {
            drag.compressibility_drag * wing.reference_area_m2 / vehicle.reference_area_m2
        })
        .sum()
}

/// Historical frozen-fixture aggregation before the area-reference fix.
pub(super) fn compressibility_drag_total_unweighted(wings: &[WingCompressibilityDrag]) -> f64 {
    wings.iter().map(|wing| wing.compressibility_drag).sum()
}

/// What the excrescence buildup reports.
pub struct MiscellaneousDrag {
    /// The total wetted area the fit was evaluated at, including the 10%
    /// allowance. Reported by upstream under this name, so the allowance is
    /// inside the number a reader sees.
    pub total_wetted_area_m2: f64,
    /// The excrescence drag coefficient.
    pub total: f64,
}

/// Excrescence drag, from ESDU 94044 figure 1.
///
/// A quadratic in the whole aircraft's wetted area, marked up by 10% for
/// surfaces the component areas do not account for.
pub fn miscellaneous_drag_aircraft_esdu(vehicle: &DragVehicle<'_>) -> MiscellaneousDrag {
    let mut wetted = 0.0;
    for wing in vehicle.wings {
        wetted += wing.wetted_area_m2;
    }
    for fuselage in vehicle.fuselages {
        wetted += fuselage.wetted_area_m2;
    }
    for nacelle in vehicle.nacelles {
        // See `NacelleParams::origin_count`: one component can stand for
        // several installations, and only this sum reads that.
        wetted += nacelle.wetted_area_m2 * nacelle.origin_count as f64;
    }
    wetted *= 1.10;

    let drag_over_q = 0.40 * (0.0184 + 0.000_469 * wetted - 1.13e-7 * wetted * wetted);

    MiscellaneousDrag {
        total_wetted_area_m2: wetted,
        total: drag_over_q / vehicle.reference_area_m2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wing(sweep: f64, tc: f64, cl: f64) -> WingParams {
        WingParams {
            mean_aerodynamic_chord_m: 9.6,
            quarter_chord_sweep_rad: sweep,
            thickness_to_chord: tc,
            reference_area_m2: 529.0,
            wetted_area_m2: 1084.0,
            transition_x_upper: 0.0,
            transition_x_lower: 0.0,
            aspect_ratio: 9.9,
            inviscid_lift_coefficient: cl,
            inviscid_induced_drag_coefficient: 0.028,
        }
    }

    fn freestream(mach: f64) -> Freestream {
        Freestream {
            mach,
            temperature_k: 216.77,
            reynolds_number_per_m: 6.2e6,
        }
    }

    #[test]
    fn compressibility_drag_is_negligible_far_below_the_crest_critical_mach() {
        // The 14.641 exponent is what makes the buildup usable across the
        // whole flight envelope with no switch: at a third of the crest
        // critical Mach the rise is seven decades below a drag count.
        let result = compressibility_drag_wing(&freestream(0.25), &wing(0.593, 0.12, 0.63));
        assert!(result.compressibility_drag < 1e-9);
    }

    #[test]
    fn sweeping_a_wing_raises_its_crest_critical_mach() {
        let straight = compressibility_drag_wing(&freestream(0.8), &wing(0.0, 0.12, 0.5));
        let swept = compressibility_drag_wing(&freestream(0.8), &wing(0.6, 0.12, 0.5));

        assert!(swept.crest_critical > straight.crest_critical);
        assert!(swept.compressibility_drag < straight.compressibility_drag);
    }

    #[test]
    fn the_divergence_mach_sits_above_the_crest_critical_one() {
        let result = compressibility_drag_wing(&freestream(0.82), &wing(0.593, 0.12, 0.63));
        assert!(result.divergence_mach > result.crest_critical);
    }

    #[test]
    fn a_wing_at_zero_lift_still_carries_compressibility_drag() {
        // The fit's constant term keeps the crest critical Mach finite with
        // no lift at all, so the thickness alone produces a rise. A port that
        // had folded the lift coefficient in as a multiplier would report
        // zero here.
        let result = compressibility_drag_wing(&freestream(0.9), &wing(0.593, 0.12, 0.0));
        assert!(result.compressibility_drag > 0.0);
    }

    #[test]
    fn compressibility_total_conserves_area_when_wings_use_different_references() {
        let main = wing(0.593, 0.12, 0.63);
        let tail = WingParams {
            reference_area_m2: 100.0,
            ..wing(0.593, 0.12, 0.63)
        };
        let wings = [main, tail];
        let freestream = freestream(0.84);
        let per_wing = wings
            .iter()
            .map(|wing| compressibility_drag_wing(&freestream, wing))
            .collect::<Vec<_>>();
        let vehicle = DragVehicle {
            reference_area_m2: 529.0,
            wings: &wings,
            fuselages: &[],
            nacelles: &[],
            network_count: 1,
        };

        let expected = per_wing[0].compressibility_drag * main.reference_area_m2 / 529.0
            + per_wing[1].compressibility_drag * tail.reference_area_m2 / 529.0;
        let unweighted = per_wing
            .iter()
            .map(|drag| drag.compressibility_drag)
            .sum::<f64>();

        assert_eq!(compressibility_drag_total(&vehicle, &per_wing), expected);
        assert_ne!(compressibility_drag_total(&vehicle, &per_wing), unweighted);
    }

    #[test]
    fn excrescence_drag_reports_the_marked_up_wetted_area() {
        let wings = [wing(0.593, 0.12, 0.63)];
        let vehicle = DragVehicle {
            reference_area_m2: 529.0,
            wings: &wings,
            fuselages: &[],
            nacelles: &[],
            network_count: 1,
        };
        let result = miscellaneous_drag_aircraft_esdu(&vehicle);
        assert!((result.total_wetted_area_m2 - 1084.0 * 1.10).abs() < 1e-9);
    }

    #[test]
    fn a_nacelle_installed_twice_counts_twice_in_the_wetted_area() {
        use super::super::types::NacelleParams;

        let nacelles = |count: usize| NacelleParams {
            length_m: 7.8,
            diameter_m: 4.2,
            wetted_area_m2: 113.0,
            origin_count: count,
        };
        let one = [nacelles(1)];
        let two = [nacelles(2)];
        let build = |slice: &[NacelleParams]| {
            miscellaneous_drag_aircraft_esdu(&DragVehicle {
                reference_area_m2: 529.0,
                wings: &[],
                fuselages: &[],
                nacelles: slice,
                network_count: 1,
            })
            .total_wetted_area_m2
        };

        assert!((build(&two) - 2.0 * build(&one)).abs() < 1e-9);
    }
}
