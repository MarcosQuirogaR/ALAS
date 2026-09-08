// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

fn structural_failure() -> CandidateFailure {
    CandidateFailure {
        reason: "structural_sizing",
    }
}

/// Run the checked product mass buildup and then replace the empirical total
/// wing item with the reconciled structural box inventory.  The replacement
/// is followed by the same payload/fuel/CG closure used by the mass crate;
/// leaving the old fuel remainder in place would make a heavier wing appear
/// to have the same takeoff mass.
pub(crate) fn mass_analysis_with_structural_feedback(
    config: &AlasConfig,
    dv: &DesignVector,
    plane: &Airplane,
    payload_summary: Option<&PayloadLayoutSummary>,
    reference: Option<ReferenceWingMass>,
) -> Result<StructuralMassAnalysis, CandidateFailure> {
    let analysis_mass_model = config.analysis_mass_model(config.requirements.mtow_kg);
    let (mut masses, mut coords, _) = run_mass_analysis_with_model_checked_product_with_gear(
        plane,
        &config.requirements,
        &config.geometry,
        &config.cabin,
        &config.control_surfaces,
        Some(&analysis_mass_model),
        payload_summary,
        MassCoordinateModel::ReferenceCompatibility,
        &config.landing_gear,
    )
    .map_err(|_| CandidateFailure {
        reason: "mass_coordinates",
    })?;

    let (feedback, reference, inventory) = reconcile_structural_wing(config, dv, plane, reference)?;
    masses.wing = feedback.total_wing_mass_kg;
    coords.wing = feedback.centroid_m;
    let (masses, coords, cg) = reclose_mass(masses, coords, &config.requirements, payload_summary);
    Ok((masses, coords, cg, feedback, reference, inventory))
}

/// Build the strength-sized primary box and reconcile it with either a frozen
/// empirical reference or the explicitly modelled clean-sheet movable items.
fn reconcile_structural_wing(
    config: &AlasConfig,
    dv: &DesignVector,
    plane: &Airplane,
    reference: Option<ReferenceWingMass>,
) -> Result<
    (
        WingboxFeedback,
        Option<ReferenceWingMass>,
        StructuralInventory,
    ),
    CandidateFailure,
> {
    let (candidate_primary, _) = sized_primary_wing(config, dv, plane, &config.requirements)?;
    match config.optimizer.design_space.mode {
        DesignMode::CleanSheet => {
            let wing = main_wing(plane).ok_or_else(structural_failure)?;
            let inventory = clean_sheet_secondary(config, wing, candidate_primary)?;
            let feedback =
                reconcile_clean_sheet_wing(candidate_primary, inventory.secondary_wing_mass())
                    .map_err(|_| structural_failure())?;
            // The inventory is always additive to the sized box, so the
            // candidate is still evaluable when a plausibility gate fails; the
            // gate only removes the right to present the wing as complete.
            Ok((
                feedback,
                None,
                StructuralInventory::CleanSheet(Box::new(inventory)),
            ))
        }
        DesignMode::ReferenceAdaptation | DesignMode::BaselineSandbox => {
            let reference = match reference {
                Some(value) => value,
                None => reference_wing(config)?,
            };
            let feedback = reconcile_reference_wing(reference, candidate_primary)
                .map_err(|_| structural_failure())?;
            Ok((
                feedback,
                Some(reference),
                StructuralInventory::FrozenReference,
            ))
        }
    }
}

/// Strength-size the main-wing box using the actual built wing and resolve a
/// structural centroid from the same geometry/material/load inputs. The
/// sizing crate reports one symmetric semispan; the feedback boundary makes
/// that extent explicit before any total-wing mass is formed.
fn sized_primary_wing(
    config: &AlasConfig,
    dv: &DesignVector,
    plane: &Airplane,
    requirements: &alas_config::DesignRequirements,
) -> Result<(SizedWingboxMass, WingboxSizing), CandidateFailure> {
    // `structures.enabled` gates the optional downstream NASTRAN/report
    // solve only (see `alas_config::StructuresConfig::enabled`'s own help
    // text: "the configured spars, materials, and gauges still define the
    // main-wing mass centroid used by weight and balance, without replacing
    // the Torenbeek total wing mass"). Sizing the box here is that mass/CG
    // input, not the downstream solve, so it must run regardless of the
    // flag; `crates/alas-pipeline/src/pipeline.rs` is what actually skips
    // the NASTRAN/report stage when it is false.
    let wing = main_wing(plane).ok_or_else(structural_failure)?;
    if !wing.symmetric || wing.xsecs.len() < 2 || config.structures.spanwise_stations < 2 {
        return Err(structural_failure());
    }
    let (spar_fractions, spar_full_span) = config.structures.resolved_spars();
    let root = wing.xsecs.first().ok_or_else(structural_failure)?;
    let tip = wing.xsecs.last().ok_or_else(structural_failure)?;
    let geometry = alas_geom::wing_structure::WingStructureGeometry::new(
        dv,
        &config.geometry.wing,
        &root.airfoil,
        &tip.airfoil,
        &spar_fractions,
        Some(&spar_full_span),
    )
    .map_err(|_| structural_failure())?;
    let skin = alas_config::materials::get(&config.structures.skin_material)
        .map_err(|_| structural_failure())?;
    let web = alas_config::materials::get(&config.structures.spar_web_material)
        .map_err(|_| structural_failure())?;
    let cap = alas_config::materials::get(&config.structures.spar_cap_material)
        .map_err(|_| structural_failure())?;
    let rib = alas_config::materials::get(&config.structures.rib_material)
        .map_err(|_| structural_failure())?;
    let sizing = size_wingbox(
        &geometry,
        &config.structures,
        requirements,
        skin,
        web,
        cap,
        rib,
    );
    // A sized station can land one ulp below zero after the closed-form
    // section-property arithmetic (the default design observed
    // -1.11e-16).  Keep a small, explicit numerical tolerance at this
    // production seam while still rejecting NaN and any material strength
    // deficit.  The structural crate retains its exact predicate for its
    // own reporting/tests; this caller is deciding whether round-off alone
    // invalidates an otherwise finite candidate.
    let strength_margins_ok = sizing
        .spars
        .iter()
        .flat_map(|spar| spar.margin_of_safety.iter())
        .all(|margin| !margin.is_nan() && *margin >= -1.0e-10);
    if !sizing.total_mass_kg.is_finite()
        || sizing.total_mass_kg <= 0.0
        || !strength_margins_ok
        || !sizing.rib_spacing_pass()
        || [
            sizing.mass_breakdown_kg.spar_caps,
            sizing.mass_breakdown_kg.spar_webs,
            sizing.mass_breakdown_kg.skin,
            sizing.mass_breakdown_kg.ribs,
        ]
        .iter()
        .any(|mass| !mass.is_finite() || *mass < 0.0)
    {
        return Err(structural_failure());
    }
    let centroid =
        alas_mass::wing_centroid::wing_structural_centroid(wing, requirements, &config.structures)
            .map_err(|_| structural_failure())?;
    let primary = SizedWingboxMass::symmetric_semiwing(sizing.total_mass_kg, centroid.xyz_m);
    Ok((primary, sizing))
}

fn main_wing(plane: &Airplane) -> Option<&Wing> {
    plane
        .wings
        .iter()
        .find(|wing| wing.name == "Main Wing")
        .or_else(|| plane.wings.first())
}

/// Obtain the complete empirical wing item used as the frozen reference in
/// reference-adaptation and baseline-sandbox modes. The reference vector is
/// the registered preset's vector when one is named; a blank configuration has
/// the canonical default vector.
fn reference_wing(config: &AlasConfig) -> Result<ReferenceWingMass, CandidateFailure> {
    let reference_dv = if config.preset.is_empty() {
        DesignVector::default()
    } else {
        alas_config::presets::get(&config.preset)
            .map_err(|_| structural_failure())?
            .design_vector
    };
    let reference_plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&reference_dv), false)
        .map_err(|_| structural_failure())?;
    let (reference_primary, _) = sized_primary_wing(
        config,
        &reference_dv,
        &reference_plane,
        &config.requirements,
    )?;
    let analysis_mass_model = config.analysis_mass_model(config.requirements.mtow_kg);
    let (masses, coords, _) = run_mass_analysis_with_model_checked_product_with_gear(
        &reference_plane,
        &config.requirements,
        &config.geometry,
        &config.cabin,
        &config.control_surfaces,
        Some(&analysis_mass_model),
        None,
        MassCoordinateModel::ReferenceCompatibility,
        &config.landing_gear,
    )
    .map_err(|_| CandidateFailure {
        reason: "mass_coordinates",
    })?;
    if !masses.wing.is_finite() || masses.wing <= 0.0 || !coords.wing.iter().all(|v| v.is_finite())
    {
        return Err(structural_failure());
    }
    Ok(ReferenceWingMass {
        total_mass_kg: masses.wing,
        centroid_m: coords.wing,
        sized_box: reference_primary,
    })
}

/// Build the enumerated clean-sheet non-box wing inventory.
///
/// The Torenbeek movable terms and the complete Torenbeek wing group come from
/// the same call arguments, so the group's basic structure -- the empirical
/// counterpart of the analytically sized box -- follows as their difference
/// without duplicating any private Appendix C coefficient. The FLOPS inputs
/// describe the same built wing; `alas-config` carries no FLOPS composite
/// utilisation, aeroelastic tailoring or strut-bracing datum, so those are
/// zero here, which is the metallic cantilever case and is the conservative
/// (heaviest) branch of Eqs. 33-38.
fn clean_sheet_secondary(
    config: &AlasConfig,
    wing: &Wing,
    sized_box: SizedWingboxMass,
) -> Result<WingNonBoxInventory, CandidateFailure> {
    let mm = &config.mass_model;
    let control = &config.control_surfaces;
    let flap_area = configured_surface_area(
        wing,
        control.flap_span_start_frac,
        control.flap_span_end_frac,
        control.flap_chord_fraction,
    );
    let mounted_to_wing =
        config.landing_gear.n_mlg_struts == 0 || config.landing_gear.n_mlg_struts >= 2;
    let torenbeek_arguments = (
        config.requirements.mtow_kg,
        config.requirements.ultimate_load_factor,
        config.requirements.mtow_kg * mm.suspended_mass_fraction,
        config.requirements.dive_speed_m_s,
        mm.max_airspeed_for_flaps_ms,
        mounted_to_wing,
        mm.flap_deflection_angle_deg,
        flap_area,
    );
    let breakdown = wing_secondary_mass_breakdown_with_control_surface_area(
        wing,
        torenbeek_arguments.0,
        torenbeek_arguments.1,
        torenbeek_arguments.2,
        torenbeek_arguments.3,
        torenbeek_arguments.4,
        torenbeek_arguments.5,
        torenbeek_arguments.6,
        None,
        torenbeek_arguments.7,
    );
    validate_secondary_breakdown(breakdown)?;
    let group_total_kg = mass_wing_with_control_surface_area(
        wing,
        torenbeek_arguments.0,
        torenbeek_arguments.1,
        torenbeek_arguments.2,
        torenbeek_arguments.3,
        torenbeek_arguments.4,
        torenbeek_arguments.5,
        torenbeek_arguments.6,
        None,
        torenbeek_arguments.7,
    );

    let surfaces = WingMovableSurfaces {
        trailing_edge_flaps: MovableSurface {
            area_m2: flap_area,
            centroid_m: surface_centroid(
                wing,
                control.flap_span_start_frac,
                control.flap_span_end_frac,
                (1.0 - 0.5 * control.flap_chord_fraction).clamp(0.0, 1.0),
            ),
        },
        leading_edge_devices: MovableSurface {
            area_m2: configured_surface_area(
                wing,
                control.slat_span_start_frac,
                control.slat_span_end_frac,
                control.slat_chord_fraction,
            ),
            centroid_m: surface_centroid(
                wing,
                control.slat_span_start_frac,
                control.slat_span_end_frac,
                (0.5 * control.slat_chord_fraction).clamp(0.0, 1.0),
            ),
        },
        ailerons: MovableSurface {
            area_m2: configured_surface_area(
                wing,
                control.aileron_span_start_frac,
                control.aileron_span_end_frac,
                control.aileron_chord_fraction,
            ),
            centroid_m: surface_centroid(
                wing,
                control.aileron_span_start_frac,
                control.aileron_span_end_frac,
                (1.0 - 0.5 * control.aileron_chord_fraction).clamp(0.0, 1.0),
            ),
        },
        spoilers: MovableSurface {
            area_m2: configured_surface_area(
                wing,
                control.spoiler_span_start_frac,
                control.spoiler_span_end_frac,
                control.spoiler_chord_fraction,
            ),
            centroid_m: surface_centroid(
                wing,
                control.spoiler_span_start_frac,
                control.spoiler_span_end_frac,
                (0.5 * control.spoiler_chord_fraction).clamp(0.0, 1.0),
            ),
        },
    };

    let inputs = WingInventoryInputs {
        flops: flops_wing_inputs(config, wing),
        torenbeek: TorenbeekWingGroup {
            group_total_kg,
            high_lift_devices_kg: breakdown.high_lift_devices_kg,
            spoilers_and_speedbrakes_kg: breakdown.spoilers_and_speedbrakes_kg,
        },
        sized_box,
        surfaces,
        fixed_structure: fixed_non_box_structure(config, wing),
    };
    build_wing_inventory(&inputs).map_err(|_| structural_failure())
}

#[cfg(test)]
mod structural_tests {
    use super::*;
    use alas_config::design_variables::DesignVector;

    /// The default configuration is a clean-sheet design space, so this
    /// exercises the enumerated inventory rather than a frozen reference.
    fn clean_sheet_candidate() -> (AlasConfig, DesignVector, Airplane) {
        let config = AlasConfig::default();
        assert_eq!(config.optimizer.design_space.mode, DesignMode::CleanSheet);
        build_geometry(&config, &DesignVector::default().to_array())
            .unwrap_or_else(|failure| panic!("{}", failure.reason))
    }

    #[test]
    fn the_clean_sheet_wing_inventory_is_complete_and_fully_enumerated() {
        let (config, dv, plane) = clean_sheet_candidate();
        let wing = main_wing(&plane).expect("built aircraft has a main wing");
        let (primary, _) = sized_primary_wing(&config, &dv, &plane, &config.requirements)
            .unwrap_or_else(|failure| panic!("{}", failure.reason));
        let inventory = clean_sheet_secondary(&config, wing, primary)
            .unwrap_or_else(|failure| panic!("{}", failure.reason));

        inventory
            .require_complete()
            .expect("the enumerated clean-sheet inventory is complete");
        assert!(inventory.items.iter().all(|item| item.mass_kg > 0.0
            && item.centroid_m.iter().all(|value| value.is_finite())
            && !item.source.is_empty()
            && !item.applicability.is_empty()));
        // Both empirical wing groups are evaluated on the same candidate and
        // both plausibility ratios are reported rather than clamped.
        let diagnostics = inventory.diagnostics;
        assert!(diagnostics.torenbeek_group_total_kg > 0.0);
        assert!(diagnostics.flops_group_total_kg > 0.0);
        assert!(diagnostics.total_to_torenbeek_group_ratio.is_finite());
        assert!(diagnostics.total_to_flops_group_ratio.is_finite());
    }

    #[test]
    fn the_inventory_reaches_the_reconciled_wing_mass_first_moment_and_fuel_closure() {
        let (config, dv, plane) = clean_sheet_candidate();
        let wing = main_wing(&plane).expect("built aircraft has a main wing");
        let (primary, _) = sized_primary_wing(&config, &dv, &plane, &config.requirements)
            .unwrap_or_else(|failure| panic!("{}", failure.reason));
        let inventory = clean_sheet_secondary(&config, wing, primary)
            .unwrap_or_else(|failure| panic!("{}", failure.reason));

        let (masses, coords, _, feedback, reference, structural) =
            mass_analysis_with_structural_feedback(&config, &dv, &plane, None, None)
                .unwrap_or_else(|failure| panic!("{}", failure.reason));
        assert!(
            reference.is_none(),
            "clean sheet carries no frozen reference"
        );
        assert!(structural.is_complete());

        // The wing group the mass breakdown carries is exactly the sized box
        // plus the enumerated inventory, and its centroid is their first
        // moment divided by that mass.
        assert!((masses.wing - inventory.complete_wing_mass_kg).abs() < 1.0e-6);
        assert!((feedback.secondary_mass_kg - inventory.total_kg()).abs() < 1.0e-9);
        for axis in 0..3 {
            let moment = feedback.first_moment_kg_m[axis];
            let scale = moment.abs().max(1.0);
            // The reported wing centroid reproduces the reconciled moment.
            assert!((coords.wing[axis] * masses.wing - moment).abs() / scale < 1.0e-9);
            // The secondary part of that moment is the enumerated inventory's
            // own first moment, so no item is lost or double counted between
            // the inventory and the reconciliation.
            assert!(
                (feedback.secondary_first_moment_kg_m[axis] - inventory.first_moment_kg_m()[axis])
                    .abs()
                    / scale
                    < 1.0e-9
            );
            // Primary plus secondary closes the reconciled moment exactly.
            let primary_moment = moment - feedback.secondary_first_moment_kg_m[axis];
            assert!(
                (primary_moment + inventory.first_moment_kg_m()[axis] - moment).abs() / scale
                    < 1.0e-9
            );
        }

        // The fuel remainder is reclosed against the heavier wing, so the
        // inventory participates in the mass closure instead of sitting beside
        // it: OEW plus payload plus fuel is the takeoff-mass ceiling.
        let oew: f64 = OEW_KEYS
            .iter()
            .map(|&key| masses.get(key).unwrap_or(0.0))
            .sum();
        assert!((oew + masses.payload + masses.fuel - config.requirements.mtow_kg).abs() < 1.0e-6);
    }

    #[test]
    fn a_disabled_downstream_solve_still_sizes_the_same_wing_mass_and_centroid() {
        // `structures.enabled = false` only skips the optional downstream
        // NASTRAN/report stage (`alas_config::StructuresConfig::enabled`'s
        // help text; `alas-pipeline` is the caller that actually skips that
        // stage). The mass/CG input this module computes must be identical
        // whether or not that flag is set.
        let (mut config, dv, plane) = clean_sheet_candidate();
        let enabled = mass_analysis_with_structural_feedback(&config, &dv, &plane, None, None)
            .unwrap_or_else(|failure| panic!("{}", failure.reason));
        config.structures.enabled = false;
        let disabled = mass_analysis_with_structural_feedback(&config, &dv, &plane, None, None)
            .unwrap_or_else(|failure| panic!("{}", failure.reason));

        let (enabled_masses, enabled_coords, _, enabled_feedback, ..) = enabled;
        let (disabled_masses, disabled_coords, _, disabled_feedback, ..) = disabled;
        assert!((enabled_masses.wing - disabled_masses.wing).abs() < 1.0e-9);
        for axis in 0..3 {
            assert!((enabled_coords.wing[axis] - disabled_coords.wing[axis]).abs() < 1.0e-9);
        }
        assert!(
            (enabled_feedback.total_wing_mass_kg - disabled_feedback.total_wing_mass_kg).abs()
                < 1.0e-9
        );
    }

    #[test]
    fn a_genuinely_invalid_structure_still_rejects_regardless_of_the_downstream_solve_flag() {
        // Fewer than two spanwise stations cannot be strength-sized; this
        // must still reject the candidate whether or not the downstream
        // solve is enabled -- the `enabled` flag controls the optional
        // report stage, not the acceptance gate.
        let (mut config, dv, plane) = clean_sheet_candidate();
        config.structures.spanwise_stations = 1;
        let enabled_failure = sized_primary_wing(&config, &dv, &plane, &config.requirements)
            .expect_err("one spanwise station cannot be strength-sized");
        assert_eq!(enabled_failure.reason, "structural_sizing");

        config.structures.enabled = false;
        let disabled_failure = sized_primary_wing(&config, &dv, &plane, &config.requirements)
            .expect_err("disabling the downstream solve does not waive the strength gate");
        assert_eq!(disabled_failure.reason, "structural_sizing");
    }

    #[test]
    fn a_heavier_sized_box_makes_a_heavier_clean_sheet_wing() {
        let (config, dv, plane) = clean_sheet_candidate();
        let wing = main_wing(&plane).expect("built aircraft has a main wing");
        let (primary, _) = sized_primary_wing(&config, &dv, &plane, &config.requirements)
            .unwrap_or_else(|failure| panic!("{}", failure.reason));
        let nominal = clean_sheet_secondary(&config, wing, primary)
            .unwrap_or_else(|failure| panic!("{}", failure.reason));

        let heavier_box = SizedWingboxMass {
            mass_kg: primary.mass_kg * 1.10,
            ..primary
        };
        let heavier = clean_sheet_secondary(&config, wing, heavier_box)
            .unwrap_or_else(|failure| panic!("{}", failure.reason));

        // The inventory is independent of the box, so the whole box increment
        // reaches the wing group; nothing cancels it.
        assert!((heavier.total_kg() - nominal.total_kg()).abs() < 1.0e-9);
        let expected = nominal.complete_wing_mass_kg + 0.10 * 2.0 * primary.mass_kg;
        assert!((heavier.complete_wing_mass_kg - expected).abs() < 1.0e-6);
    }
}
