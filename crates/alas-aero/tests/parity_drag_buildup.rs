// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-aero::drag_buildup` against SUAVE's `Fidelity_Zero` drag
//! chain, via `golden/aero/drag_buildup.json`.
//!
//! Two tiers, and the split is the same one `alas-aero::analysis` and the two
//! payload rows carry. Everything discrete about the comparison is checked at
//! `exact` -- the analysis settings, which are copied constants rather than
//! computed values, and the component tags, so that a fixture whose wings have
//! stopped meaning the same wings is one line of report rather than forty.
//! Every drag coefficient is closed-form `f64` arithmetic over a geometry, a
//! freestream and a lift solution, with no factorization, spline fit or
//! iteration anywhere in it, so all of them are checked at `closed`, the tier
//! `docs/PORTING.md` assigns this row.
//!
//! The freestream and the per-wing lift solution are read *out of* the fixture
//! rather than recomputed. That is the correction that closed this row: a
//! previous version derived the Reynolds number from AeroSandbox's ISA while
//! the fixture had been generated against SUAVE's `US_Standard_1976`, so it
//! was comparing two atmospheres and reporting the difference as a drag
//! disagreement -- about `7e-5` relative on the cruise cases, growing with
//! altitude because that is where the two density models diverge.

// This file is itself a test binary, so an unwrap or expect that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod support;

use alas_aero::drag_buildup::{
    evaluate, DragSettings, DragVehicle, Freestream, FuselageParams, NacelleParams, WingParams,
};
use alas_testkit::{Comparison, Tier};
use support::drag_buildup::{at, Fixture};

/// The settings the port defaults to must be the ones the analysis held.
///
/// This is the check that would have caught the row's second defect on its
/// own: the fixture it replaced had been generated against a hand-built
/// `settings` object carrying a wing form factor of 1.0 and a span efficiency
/// of 0.95, neither of which `Fidelity_Zero.__defaults__` sets, so it recorded
/// a configuration the mission never runs and agreed with a port that made the
/// same substitution.
#[test]
fn the_analysis_settings_are_what_the_port_assumes() {
    let fixture: Fixture = alas_testkit::load("aero", "drag_buildup");
    let settings = DragSettings::default();
    let expected = &fixture.settings;

    let mut c = Comparison::new("alas-aero::drag_buildup settings", Tier::Exact);
    c.exact(
        "wing_parasite_drag_form_factor",
        &settings.wing_parasite_drag_form_factor,
        &expected.wing_parasite_drag_form_factor,
    );
    c.exact(
        "fuselage_parasite_drag_form_factor",
        &settings.fuselage_parasite_drag_form_factor,
        &expected.fuselage_parasite_drag_form_factor,
    );
    c.exact(
        "viscous_lift_dependent_drag_factor",
        &settings.viscous_lift_dependent_drag_factor,
        &expected.viscous_lift_dependent_drag_factor,
    );
    c.exact(
        "trim_drag_correction_factor",
        &settings.trim_drag_correction_factor,
        &expected.trim_drag_correction_factor,
    );
    c.exact(
        "drag_coefficient_increment",
        &settings.drag_coefficient_increment,
        &expected.drag_coefficient_increment,
    );
    c.exact(
        "spoiler_drag_increment",
        &settings.spoiler_drag_increment,
        &expected.spoiler_drag_increment,
    );
    c.exact(
        "lift_to_drag_adjustment",
        &settings.lift_to_drag_adjustment,
        &expected.lift_to_drag_adjustment,
    );

    // Neither efficiency factor is a number here, and that is the whole
    // reason the inviscid induced drag is an input rather than a formula. A
    // fixture in which either had become a number would be exercising a
    // branch this row deliberately does not translate.
    c.exact(
        "oswald_efficiency_factor is unset",
        &expected.oswald_efficiency_factor.is_none(),
        &true,
    );
    c.exact(
        "span_efficiency is unset",
        &expected.span_efficiency.is_none(),
        &true,
    );
    c.finish();
}

/// The geometry both sides run on has to be the same aeroplane before any
/// drag coefficient means anything, so it is checked first and separately.
#[test]
fn the_fixture_describes_the_geometry_the_port_translates() {
    let fixture: Fixture = alas_testkit::load("aero", "drag_buildup");
    let geometry = &fixture.geometry;

    let mut c = Comparison::new("alas-aero::drag_buildup geometry", Tier::Exact);
    c.exact(
        "wing tags",
        &geometry
            .wings
            .iter()
            .map(|wing| wing.tag.as_str())
            .collect::<Vec<_>>(),
        &vec!["main_wing", "horizontal_stabilizer", "vertical_stabilizer"],
    );
    c.exact(
        "fuselage tags",
        &geometry
            .fuselages
            .iter()
            .map(|fuselage| fuselage.tag.as_str())
            .collect::<Vec<_>>(),
        &vec!["fuselage"],
    );
    c.exact(
        "nacelle tags",
        &geometry
            .nacelles
            .iter()
            .map(|nacelle| nacelle.tag.as_str())
            .collect::<Vec<_>>(),
        &vec!["nacelle_1", "nacelle_2"],
    );

    // The unsegmented branch of `parasite_drag_wing` is the only one
    // translated, so a segmented wing would mean the port is silently
    // computing a different quantity rather than a wrong one.
    for wing in &geometry.wings {
        c.exact(
            &format!("{} segment count", wing.tag),
            &wing.segment_count,
            &0,
        );
    }
    c.finish();
}

#[test]
fn the_drag_buildup_agrees_with_suave() {
    let fixture: Fixture = alas_testkit::load("aero", "drag_buildup");
    let geometry = &fixture.geometry;

    let fuselages: Vec<FuselageParams> = geometry
        .fuselages
        .iter()
        .map(|fuselage| FuselageParams {
            length_m: fuselage.length_m,
            effective_diameter_m: fuselage.effective_diameter_m,
            front_projected_area_m2: fuselage.front_projected_area_m2,
            wetted_area_m2: fuselage.wetted_area_m2,
        })
        .collect();

    let nacelles: Vec<NacelleParams> = geometry
        .nacelles
        .iter()
        .map(|nacelle| NacelleParams {
            length_m: nacelle.length_m,
            diameter_m: nacelle.diameter_m,
            wetted_area_m2: nacelle.wetted_area_m2,
            origin_count: nacelle.origin_count,
        })
        .collect();

    let mut c = Comparison::new("alas-aero::drag_buildup", Tier::Closed);

    for case in &fixture.cases {
        // The lift solution is the vortex lattice's, taken from the fixture.
        // `compressible_wings` and `inviscid_wings` are the same array
        // upstream -- `Vortex_Lattice.evaluate` assigns one to the other --
        // so the port carries one field, and the equality is asserted here
        // rather than assumed, since a future producer could break it.
        let wings: Vec<WingParams> = geometry
            .wings
            .iter()
            .map(|wing| {
                let lift = at(&case.lift.inviscid_wings, &wing.tag);
                assert_eq!(
                    lift,
                    at(&case.lift.compressible_wings, &wing.tag),
                    "case {}: the vortex lattice reported different inviscid and \
                     compressible lift for {}, which the port's single field cannot \
                     represent",
                    case.tag,
                    wing.tag,
                );
                WingParams {
                    mean_aerodynamic_chord_m: wing.mean_aerodynamic_chord_m,
                    quarter_chord_sweep_rad: wing.quarter_chord_sweep_rad,
                    thickness_to_chord: wing.thickness_to_chord,
                    reference_area_m2: wing.reference_area_m2,
                    wetted_area_m2: wing.wetted_area_m2,
                    transition_x_upper: wing.transition_x_upper,
                    transition_x_lower: wing.transition_x_lower,
                    aspect_ratio: wing.aspect_ratio,
                    inviscid_lift_coefficient: lift,
                    inviscid_induced_drag_coefficient: at(
                        &case.lift.inviscid_induced_wings,
                        &wing.tag,
                    ),
                }
            })
            .collect();

        let vehicle = DragVehicle {
            reference_area_m2: geometry.reference_area_m2,
            wings: &wings,
            fuselages: &fuselages,
            nacelles: &nacelles,
            network_count: geometry.network_count,
        };
        let freestream = Freestream {
            mach: case.mach,
            temperature_k: case.freestream.temperature_k,
            reynolds_number_per_m: case.freestream.reynolds_number_per_m,
        };

        let result = evaluate(&DragSettings::default(), &freestream, &vehicle);
        let case_tag = &case.tag;

        for (wing, (drag, compressible)) in geometry
            .wings
            .iter()
            .zip(result.parasite_wings.iter().zip(&result.compressible_wings))
        {
            let tag = &wing.tag;
            let name = |quantity: &str| format!("{case_tag}.{tag}.{quantity}");

            c.scalar(
                &name("parasite"),
                drag.parasite_drag_coefficient,
                at(&case.parasite.components, tag),
            );
            c.scalar(
                &name("skin_friction"),
                drag.skin_friction_coefficient,
                at(&case.parasite.skin_friction, tag),
            );
            c.scalar(
                &name("form_factor"),
                drag.form_factor,
                at(&case.parasite.form_factor, tag),
            );
            c.scalar(
                &name("compressibility_factor"),
                drag.compressibility_factor,
                at(&case.parasite.compressibility_factor, tag),
            );
            c.scalar(
                &name("reynolds_factor"),
                drag.reynolds_factor,
                at(&case.parasite.reynolds_factor, tag),
            );
            c.scalar(
                &name("compressibility_drag"),
                compressible.compressibility_drag,
                at(&case.compressible.wings, tag),
            );
            c.scalar(
                &name("crest_critical"),
                compressible.crest_critical,
                at(&case.compressible.crest_critical, tag),
            );
            c.scalar(
                &name("divergence_mach"),
                compressible.divergence_mach,
                at(&case.compressible.divergence_mach, tag),
            );
        }

        for (wing, viscous) in geometry.wings.iter().zip(&result.induced_viscous_wings) {
            c.scalar(
                &format!("{case_tag}.{}.induced_viscous", wing.tag),
                *viscous,
                at(&case.induced.viscous_wings, &wing.tag),
            );
        }

        for (component, drag) in geometry
            .fuselages
            .iter()
            .map(|fuselage| fuselage.tag.as_str())
            .zip(&result.parasite_fuselages)
            .chain(
                geometry
                    .nacelles
                    .iter()
                    .map(|nacelle| nacelle.tag.as_str())
                    .zip(&result.parasite_nacelles),
            )
            .chain(std::iter::once(("pylon", &result.parasite_pylon)))
        {
            let name = |quantity: &str| format!("{case_tag}.{component}.{quantity}");
            c.scalar(
                &name("parasite"),
                drag.parasite_drag_coefficient,
                at(&case.parasite.components, component),
            );
            c.scalar(
                &name("skin_friction"),
                drag.skin_friction_coefficient,
                at(&case.parasite.skin_friction, component),
            );
            c.scalar(
                &name("form_factor"),
                drag.form_factor,
                at(&case.parasite.form_factor, component),
            );
            c.scalar(
                &name("compressibility_factor"),
                drag.compressibility_factor,
                at(&case.parasite.compressibility_factor, component),
            );
            c.scalar(
                &name("reynolds_factor"),
                drag.reynolds_factor,
                at(&case.parasite.reynolds_factor, component),
            );
        }

        c.scalar(
            &format!("{case_tag}.parasite.total"),
            result.parasite_total,
            case.parasite.total,
        );
        c.scalar(
            &format!("{case_tag}.induced.total"),
            result.induced_total,
            case.induced.total,
        );
        c.scalar(
            &format!("{case_tag}.induced.viscous"),
            result.induced_viscous,
            case.induced.viscous,
        );
        c.scalar(
            &format!("{case_tag}.compressible.total"),
            result.compressible_total,
            case.compressible.total,
        );
        c.scalar(
            &format!("{case_tag}.miscellaneous.total_wetted_area"),
            result.miscellaneous_total_wetted_area_m2,
            case.miscellaneous.total_wetted_area_m2,
        );
        c.scalar(
            &format!("{case_tag}.miscellaneous.total"),
            result.miscellaneous_total,
            case.miscellaneous.total,
        );
        c.scalar(
            &format!("{case_tag}.untrimmed"),
            result.untrimmed,
            case.untrimmed,
        );
        c.scalar(
            &format!("{case_tag}.trim_corrected"),
            result.trim_corrected,
            case.trim_corrected,
        );
        c.scalar(&format!("{case_tag}.spoiler"), result.spoiler, case.spoiler);
        c.scalar(&format!("{case_tag}.total"), result.total, case.total);
    }

    c.finish();
}
