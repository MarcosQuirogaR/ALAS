// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

/// Size the candidate's primary wing structure and reconcile it with the
/// secondary structure of its design mode: the clean-sheet inventory, or the
/// frozen reference aircraft's wing mass.
///
/// # Errors
///
/// [`WingReconciliationError`], with the mass and coordinates of the sized
/// box never partially applied.
pub fn reconcile_structural_wing(
    config: &AlasConfig,
    dv: &DesignVector,
    plane: &Airplane,
    reference: Option<ReferenceWingMass>,
) -> Result<
    (
        WingboxFeedback,
        Option<ReferenceWingMass>,
        StructuralInventory,
        Option<alas_struct::sizing::CompositeProxyDeclaration>,
    ),
    WingReconciliationError,
> {
    let (candidate_primary, sizing) =
        sized_primary_wing(config, dv, plane, &design_requirements(config))?;
    let primary_declaration = sizing.composite_declaration;
    match config.optimizer.design_space.mode {
        DesignMode::CleanSheet => {
            let wing = main_wing(plane).ok_or(WingReconciliationError::StructuralSizing)?;
            let inventory = clean_sheet_secondary(config, wing, candidate_primary)?;
            let feedback =
                reconcile_clean_sheet_wing(candidate_primary, inventory.secondary_wing_mass())
                    .map_err(|_| WingReconciliationError::StructuralSizing)?;
            // The inventory is always additive to the sized box, so the
            // candidate is still evaluable when a plausibility gate fails; the
            // gate only removes the right to present the wing as complete.
            Ok((
                feedback,
                None,
                StructuralInventory::CleanSheet(Box::new(inventory)),
                primary_declaration,
            ))
        }
        DesignMode::ReferenceAdaptation | DesignMode::BaselineSandbox => {
            let reference = match reference {
                Some(value) => value,
                None => reference_wing(config)?,
            };
            reconcile_against_reference(reference, candidate_primary, primary_declaration)
        }
    }
}

/// Strength-size the main-wing box using the actual built wing and resolve a
/// structural centroid from the same geometry/material/load inputs. The
/// sizing crate reports one symmetric semispan; the feedback boundary makes
/// that extent explicit before any total-wing mass is formed.
pub fn sized_primary_wing(
    config: &AlasConfig,
    dv: &DesignVector,
    plane: &Airplane,
    requirements: &alas_config::DesignRequirements,
) -> Result<(SizedWingboxMass, WingboxSizing), WingReconciliationError> {
    // `structures.enabled` gates the optional downstream NASTRAN/report
    // solve only (see `alas_config::StructuresConfig::enabled`'s own help
    // text: "the configured spars, materials, and gauges still define the
    // main-wing mass centroid used by weight and balance, without replacing
    // the Torenbeek total wing mass"). Sizing the box here is that mass/CG
    // input, not the downstream solve, so it must run regardless of the
    // flag; `crates/alas-pipeline/src/pipeline.rs` is what actually skips
    // the NASTRAN/report stage when it is false.
    let wing = main_wing(plane).ok_or(WingReconciliationError::StructuralSizing)?;
    if !wing.symmetric || wing.xsecs.len() < 2 || config.structures.spanwise_stations < 2 {
        return Err(WingReconciliationError::StructuralSizing);
    }
    let (spar_fractions, spar_full_span) = config.structures.resolved_spars();
    let root = wing.xsecs.first().ok_or(WingReconciliationError::StructuralSizing)?;
    let tip = wing.xsecs.last().ok_or(WingReconciliationError::StructuralSizing)?;
    let geometry = alas_geom::wing_structure::WingStructureGeometry::new(
        dv,
        &config.geometry.wing,
        &root.airfoil,
        &tip.airfoil,
        &spar_fractions,
        Some(&spar_full_span),
    )
    .map_err(|_| WingReconciliationError::StructuralSizing)?;
    let skin = alas_config::materials::get(&config.structures.skin_material)
        .map_err(|_| WingReconciliationError::StructuralSizing)?;
    let web = alas_config::materials::get(&config.structures.spar_web_material)
        .map_err(|_| WingReconciliationError::StructuralSizing)?;
    let cap = alas_config::materials::get(&config.structures.spar_cap_material)
        .map_err(|_| WingReconciliationError::StructuralSizing)?;
    let rib = alas_config::materials::get(&config.structures.rib_material)
        .map_err(|_| WingReconciliationError::StructuralSizing)?;
    // The wing-carried masses `size_wingbox`'s own signature cannot see, and
    // this caller can: the aircraft's declared integral wing-tank capacity in
    // place of the geometric estimate, and its wing-mounted powerplant. Both
    // relieve wing-root bending at their own stations. Neither is a correction
    // in a favourable direction: supplying them makes five of the eight
    // registered boxes heavier and two lighter.
    let stations = alas_struct::sizing::sizing_stations(&geometry, &config.structures);
    let (front, rear) = alas_struct::sizing::box_chord_band(&geometry);
    // The declared capacity is bounded to the fuel the loading envelope
    // guarantees is in the wings at the design gross mass the manoeuvre is
    // applied at: a published tank volume says what the wing can hold, and an
    // aircraft that can reach its design mass at the maximum structural payload
    // holds only `DG - MZFW` of it. `requirements` is already the design-gross
    // mass form, so the bound and the load case read one mass.
    let declared_fuel = fuel_relief::declared_integral_wing_fuel_kg_m(
        config,
        dv,
        requirements,
        &geometry,
        &stations,
        front,
        rear,
    );
    let wing_mounted = alas_struct::loads::engine_point_loads_n(
        &config.geometry.engine,
        &config.mass_model,
        requirements,
    );
    let sizing = alas_struct::sizing::size_wingbox_with_wing_carried_mass(
        &geometry,
        &config.structures,
        requirements,
        skin,
        web,
        cap,
        rib,
        declared_fuel.as_deref(),
        &wing_mounted,
    );
    // A station sized to a margin of exactly zero can report one or two units
    // in the last place below it, because the reported margin recomputes the
    // equality the sizing solved and that round trip is not exact in binary
    // floating point. That is arithmetic, not a strength deficit, and
    // `alas_struct::sizing::MARGIN_NUMERICAL_ZERO` states the band and derives
    // it from the four inexact operations involved.
    //
    // This seam used to carry its own `-1.0e-10`, six orders of magnitude
    // wider than the arithmetic needs and with no stated basis. Reading the
    // shared constant **tightens** the gate rather than widening it, and puts
    // this caller and `alas-pipeline`'s own structural gate on one predicate,
    // so a wingbox cannot be feasible for mass and infeasible for the
    // structural solve on the same numbers.
    let strength_margins_ok = sizing
        .spars
        .iter()
        .flat_map(|spar| spar.margin_of_safety.iter())
        .all(|margin| alas_struct::sizing::margin_is_structurally_non_negative(*margin));
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
        return Err(WingReconciliationError::StructuralSizing);
    }
    let centroid =
        crate::wing_centroid::wing_structural_centroid(wing, requirements, &config.structures)
            .map_err(|_| WingReconciliationError::StructuralSizing)?;
    let primary = SizedWingboxMass::symmetric_semiwing(sizing.total_mass_kg, centroid.xyz_m);
    Ok((primary, sizing))
}

/// The wing named `Main Wing`, which is the one the reconciliation sizes.
pub fn main_wing(plane: &Airplane) -> Option<&Wing> {
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
fn reference_wing(config: &AlasConfig) -> Result<ReferenceWingMass, WingReconciliationError> {
    let reference_dv = if config.preset.is_empty() {
        DesignVector::default()
    } else {
        alas_config::presets::get(&config.preset)
            .map_err(|_| WingReconciliationError::StructuralSizing)?
            .design_vector
    };
    let reference_plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&reference_dv), false)
        .map_err(|_| WingReconciliationError::StructuralSizing)?;
    let (reference_primary, _) = sized_primary_wing(
        config,
        &reference_dv,
        &reference_plane,
        &design_requirements(config),
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
    .map_err(|_| WingReconciliationError::MassCoordinates)?;
    if !masses.wing.is_finite() || masses.wing <= 0.0 || !coords.wing.iter().all(|v| v.is_finite())
    {
        return Err(WingReconciliationError::StructuralSizing);
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
/// the same call arguments, so the group's basic structure (the empirical
/// counterpart of the analytically sized box) follows as their difference
/// without duplicating any private Appendix C coefficient. The FLOPS inputs
/// describe the same built wing; `alas-config` carries no FLOPS composite
/// utilisation, aeroelastic tailoring or strut-bracing datum, so those are
/// zero here, which is the metallic cantilever case and is the conservative
/// (heaviest) branch of Eqs. 33-38.
pub fn clean_sheet_secondary(
    config: &AlasConfig,
    wing: &Wing,
    sized_box: SizedWingboxMass,
) -> Result<WingNonBoxInventory, WingReconciliationError> {
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
    let design_gross_mass_kg = design_gross_mass_kg(config);
    let torenbeek_arguments = (
        design_gross_mass_kg,
        config.requirements.ultimate_load_factor,
        design_gross_mass_kg * mm.suspended_mass_fraction,
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
    build_wing_inventory(&inputs).map_err(|_| WingReconciliationError::StructuralSizing)
}

