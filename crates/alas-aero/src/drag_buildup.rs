// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission analysis model/Analyses/Aerodynamics/Fidelity_Zero.py's `compute.drag`
// process chain and the methods it names in
// mission analysis model/Methods/Aerodynamics/Common/Fidelity_Zero/Drag/.
// Upstream: mission analysis model 2.5.2, LGPL-2.1.
// Reference: alas @ rust-port-baseline.

//! mission analysis model's `Fidelity_Zero` drag buildup: the drag polar the mission flies on.
//!
//! Without this, a mission has lift and no drag, so no throttle setting, no
//! fuel burn and no range. It is an empirical buildup rather than a solve:
//! flat-plate skin friction marked up by a component form factor, plus the
//! lift-dependent terms, plus a transonic rise, plus a fixed allowance for
//! everything a component breakdown cannot see.
//!
//! **This is not `alas-aero::analysis`.** That row is this program's *own*
//! Raymer/Korn buildup, over an native aerodynamic model geometry, reached from
//! `alas/physics/aerodynamics.py`. This one is mission analysis model's, over a mission analysis model vehicle,
//! and the only thing in the whole reference that reaches it is the mission
//! runner's `mission_builder.py:85-87`, which attaches
//! `mission analysis model.Analyses.Aerodynamics.Fidelity_Zero()` to an assembled vehicle and
//! sets nothing on it. The two answer the same question with different
//! correlations and are deliberately not unified, exactly as
//! `alas-prop::mission_turbofan` is not unified with `alas-prop::cycle`.
//!
//! # What this row takes as input, and why
//!
//! [`evaluate`] takes a [`Freestream`] rather than an altitude, and each
//! [`WingParams`] carries a lift coefficient and an inviscid induced drag
//! coefficient it did not compute. Both are deliberate.
//!
//! The freestream is the mission segment's, which comes from mission analysis model's
//! `US_Standard_1976`: `alas-atmo::us1976`'s own green row. Deriving it here
//! instead would make every number this module reports a function of an
//! atmosphere model as well as of a drag correlation, and a disagreement in
//! the first would surface as a disagreement in the second. That is not
//! hypothetical: it is exactly what stalled this row, and the ledger records
//! it.
//!
//! The per-wing lift solution is the vortex lattice's. `span_efficiency` and
//! `oswald_efficiency_factor` are both `None` in `Fidelity_Zero`'s defaults,
//! and with `span_efficiency` unset the inviscid induced drag *is*
//! `drag_breakdown.induced.inviscid_wings[tag]`, which
//! `mission analysis model.Analyses.Aerodynamics.Vortex_Lattice` writes; there is no
//! closed-form fallback on the path this program takes. So the lift solution
//! arrives as data, the same arrangement `alas-mass::transport_weight` uses for
//! `sealevel_static_thrust`. `alas-aero::lift_surrogate` is what will supply
//! it; until then the fixture does.
//!
//! # Scope
//!
//! Translated: the whole `compute.drag` chain in the order
//! `Fidelity_Zero.__defaults__` builds it: per-component parasite drag, the
//! pylon allowance, the parasite total (including the in-place rescale the
//! induced buildup then reads back), induced drag, per-wing compressibility
//! drag and its total, ESDU excrescence drag, and the
//! untrimmed/trim/spoiler/total accumulation.
//!
//! Left untranslated, each unreachable from this program's inputs:
//! `parasite_drag_wing`'s segmented-wing branch and its wetted-area
//! recalculation; `induced_drag_aircraft`'s set-Oswald-factor and
//! set-span-efficiency branches; and `parasite_total`'s `fuselage_bwb` skip.
//! `gen_aero_drag_buildup.py` refuses to write a fixture in which any of the
//! first three assumptions has stopped holding, rather than leaving the
//! boundary to a comment.
//!
//! One reported byproduct is not translated: `induced_drag_aircraft` also
//! reports an Oswald efficiency factor, back-solved from the total it just
//! computed. Nothing in mission analysis model or in the mission runner reads it, and it needs
//! the aircraft's total lift coefficient, which nothing else here does, so
//! it is omitted rather than carried as an input used once for an output no
//! caller has. Same reasoning as `alas-stab::modes`' untranslated phugoid
//! byproducts.

mod components;
mod flat_plate;
mod parasite;
mod types;

pub use components::{InducedDrag, MiscellaneousDrag};
pub use flat_plate::{
    compressible_mixed_flat_plate, compressible_turbulent_flat_plate, SkinFriction,
};
pub use parasite::blend_to_sonic;
pub use types::{
    ComponentParasiteDrag, DragBreakdown, DragSettings, DragVehicle, Freestream, FuselageParams,
    NacelleParams, WingCompressibilityDrag, WingParams,
};

/// Run the whole `Fidelity_Zero` drag chain at one flight condition.
///
/// The order below is `Fidelity_Zero.__defaults__`'s, and it is load-bearing
/// twice over. `parasite_drag_pylon` runs before `parasite_total` and reads
/// each nacelle's coefficient on the nacelle's *own* reference area;
/// `induced_drag_aircraft` runs after it and reads each wing's coefficient
/// *rescaled* to the vehicle's. Reordering either would change the answer
/// while leaving every formula correct.
pub fn evaluate(
    settings: &DragSettings,
    freestream: &Freestream,
    vehicle: &DragVehicle<'_>,
) -> DragBreakdown {
    let mut parasite_wings: Vec<ComponentParasiteDrag> = vehicle
        .wings
        .iter()
        .map(|wing| {
            parasite::parasite_drag_wing(freestream, settings.wing_parasite_drag_form_factor, wing)
        })
        .collect();

    let mut parasite_fuselages: Vec<ComponentParasiteDrag> = vehicle
        .fuselages
        .iter()
        .map(|fuselage| {
            parasite::parasite_drag_fuselage(
                freestream,
                settings.fuselage_parasite_drag_form_factor,
                fuselage,
            )
        })
        .collect();

    let mut parasite_nacelles: Vec<ComponentParasiteDrag> = vehicle
        .nacelles
        .iter()
        .map(|nacelle| parasite::parasite_drag_nacelle(freestream, nacelle))
        .collect();

    // Before the rescale below: the pylon allowance is a fraction of the
    // nacelle coefficients as the nacelle methods returned them, and it comes
    // out already on the vehicle reference area.
    let parasite_pylon = parasite::parasite_drag_pylon(vehicle, &parasite_nacelles);

    let mut parasite_total = 0.0;
    for (wing, drag) in vehicle.wings.iter().zip(&mut parasite_wings) {
        parasite::scale_to_vehicle_reference(
            drag,
            wing.reference_area_m2,
            vehicle.reference_area_m2,
        );
        parasite_total += drag.parasite_drag_coefficient;
    }
    for (fuselage, drag) in vehicle.fuselages.iter().zip(&mut parasite_fuselages) {
        parasite::scale_to_vehicle_reference(
            drag,
            fuselage.front_projected_area_m2,
            vehicle.reference_area_m2,
        );
        parasite_total += drag.parasite_drag_coefficient;
    }
    for (nacelle, drag) in vehicle.nacelles.iter().zip(&mut parasite_nacelles) {
        parasite::scale_to_vehicle_reference(
            drag,
            parasite::nacelle_reference_area(nacelle),
            vehicle.reference_area_m2,
        );
        parasite_total += drag.parasite_drag_coefficient;
    }
    parasite_total += parasite_pylon.parasite_drag_coefficient;

    let induced = components::induced_drag_aircraft(
        vehicle,
        &parasite_wings,
        settings.viscous_lift_dependent_drag_factor,
    );

    let compressible_wings: Vec<WingCompressibilityDrag> = vehicle
        .wings
        .iter()
        .map(|wing| components::compressibility_drag_wing(freestream, wing))
        .collect();
    let compressible_total = if settings.area_weighted_compressibility {
        components::compressibility_drag_total(vehicle, &compressible_wings)
    } else {
        components::compressibility_drag_total_unweighted(&compressible_wings)
    };

    let miscellaneous = components::miscellaneous_drag_aircraft_esdu(vehicle);

    let untrimmed = parasite_total + induced.total + compressible_total + miscellaneous.total;
    let trim_corrected = settings.trim_drag_correction_factor * untrimmed;
    let spoiler = settings.spoiler_drag_increment;
    let total = (trim_corrected + settings.drag_coefficient_increment + spoiler)
        / (1.0 + settings.lift_to_drag_adjustment);

    DragBreakdown {
        parasite_wings,
        parasite_fuselages,
        parasite_nacelles,
        parasite_pylon,
        parasite_total,
        induced_total: induced.total,
        induced_viscous: induced.viscous,
        induced_viscous_wings: induced.viscous_wings,
        compressible_wings,
        compressible_total,
        miscellaneous_total_wetted_area_m2: miscellaneous.total_wetted_area_m2,
        miscellaneous_total: miscellaneous.total,
        untrimmed,
        trim_corrected,
        spoiler,
        total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vehicle_parts() -> (Vec<WingParams>, Vec<FuselageParams>, Vec<NacelleParams>) {
        let wings = vec![WingParams {
            mean_aerodynamic_chord_m: 9.628_258_954_922_85,
            quarter_chord_sweep_rad: 0.593_411_945_678_072_1,
            thickness_to_chord: 0.12,
            reference_area_m2: 529.035_559_696_941,
            wetted_area_m2: 1_084.522_897_378_729,
            transition_x_upper: 0.0,
            transition_x_lower: 0.0,
            aspect_ratio: 9.892_496_078_559_54,
            inviscid_lift_coefficient: 0.63,
            inviscid_induced_drag_coefficient: 0.0277,
        }];
        let fuselages = vec![FuselageParams {
            length_m: 76.72,
            effective_diameter_m: 6.2,
            front_projected_area_m2: 30.190_705_400_997_917,
            wetted_area_m2: 1_494.342_527_977_135_4,
        }];
        let nacelles = vec![
            NacelleParams {
                length_m: 7.8,
                diameter_m: 4.2,
                wetted_area_m2: 113.210_432_864_761_81,
                origin_count: 1,
            },
            NacelleParams {
                length_m: 7.8,
                diameter_m: 4.2,
                wetted_area_m2: 113.210_432_864_761_81,
                origin_count: 1,
            },
        ];
        (wings, fuselages, nacelles)
    }

    fn run(mach: f64) -> DragBreakdown {
        let (wings, fuselages, nacelles) = vehicle_parts();
        let vehicle = DragVehicle {
            reference_area_m2: 529.035_559_696_941,
            wings: &wings,
            fuselages: &fuselages,
            nacelles: &nacelles,
            network_count: 1,
        };
        let freestream = Freestream {
            mach,
            temperature_k: 216.773_237_229_709,
            reynolds_number_per_m: 6.21e6,
        };
        evaluate(&DragSettings::default(), &freestream, &vehicle)
    }

    #[test]
    fn the_default_settings_are_the_analysis_defaults() {
        // These are compared against the fixture too, which is what makes
        // them a claim about mission analysis model rather than about this file. Stated here
        // as well because the two that are not 1.0 are the two a reader is
        // most likely to assume are.
        let settings = DragSettings::default();
        assert_eq!(settings.wing_parasite_drag_form_factor, 1.1);
        assert_eq!(settings.fuselage_parasite_drag_form_factor, 2.3);
        assert_eq!(settings.trim_drag_correction_factor, 1.02);
        assert_eq!(settings.viscous_lift_dependent_drag_factor, 0.38);
        assert!(settings.area_weighted_compressibility);
        assert!(!DragSettings::reference_compatibility().area_weighted_compressibility);
    }

    #[test]
    fn the_parasite_total_is_its_components_plus_the_pylon() {
        let result = run(0.82);
        let summed: f64 = result
            .parasite_wings
            .iter()
            .chain(&result.parasite_fuselages)
            .chain(&result.parasite_nacelles)
            .map(|component| component.parasite_drag_coefficient)
            .sum::<f64>()
            + result.parasite_pylon.parasite_drag_coefficient;

        assert!((result.parasite_total - summed).abs() < 1e-15);
    }

    #[test]
    fn the_pylon_carries_a_fifth_of_the_nacelles() {
        let result = run(0.82);
        let nacelles: f64 = result
            .parasite_nacelles
            .iter()
            .map(|component| component.parasite_drag_coefficient)
            .sum();

        // The nacelle entries have been rescaled to the vehicle reference
        // area by this point, and the pylon was built from the unscaled ones
        // times the same ratio, so the fifth holds against the scaled sum.
        assert!((result.parasite_pylon.parasite_drag_coefficient - 0.2 * nacelles).abs() < 1e-15);
    }

    #[test]
    fn the_trim_correction_marks_the_whole_buildup_up_by_two_percent() {
        let result = run(0.82);
        assert!((result.trim_corrected - 1.02 * result.untrimmed).abs() < 1e-15);
        // With no drag increment, no spoiler and no lift-to-drag adjustment,
        // the reported total is the trim-corrected drag unchanged. Every
        // aircraft this program builds is in that state, which is why the
        // three settings are easy to drop by accident.
        assert_eq!(result.total, result.trim_corrected);
    }

    #[test]
    fn the_untrimmed_drag_is_the_four_contributions() {
        let result = run(0.82);
        let summed = result.parasite_total
            + result.induced_total
            + result.compressible_total
            + result.miscellaneous_total;
        assert!((result.untrimmed - summed).abs() < 1e-15);
    }

    #[test]
    fn excrescence_drag_does_not_move_with_the_flight_condition() {
        // It is a function of wetted area and reference area alone. Worth
        // pinning: it is the one term that would look plausible if it had
        // silently been made to depend on Mach.
        let low = run(0.2).miscellaneous_total;
        let high = run(0.9).miscellaneous_total;
        assert_eq!(low, high);
    }

    #[test]
    fn the_wing_form_factor_collapses_to_one_at_mach_one() {
        // The cubic blend is fully applied there, so the compressibility
        // markup on skin friction disappears exactly rather than asymptotically.
        let result = run(1.0);
        assert_eq!(result.parasite_wings[0].form_factor, 1.0);
    }

    #[test]
    fn drag_rises_through_the_transonic_band() {
        let cruise = run(0.82).total;
        let transonic = run(0.95).total;
        assert!(transonic > cruise);
    }
}
