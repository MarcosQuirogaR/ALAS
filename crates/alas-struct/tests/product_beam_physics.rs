// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Physical invariants of the compatible-strain product wing-box model.

// All fallible calls construct declared fixtures; a failure is a failed test.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{materials, presets, AlasConfig, DesignVector, WingConfig};
use alas_geom::{
    aircraft::airfoil::Airfoil, builder::AircraftBuilder, wing_structure::WingStructureGeometry,
};
use alas_struct::{
    analytical::analyze_structure_with_wing_carried_mass,
    feasibility::{assess, LinearModelLimits},
    loads::engine_point_loads_n,
    scope::{WingFuelDesignCase, WingMountedRelief},
    sizing::{
        box_chord_band, box_running_mass_kg_m, size_for_linear_model, size_wingbox_with_scope,
        size_wingbox_with_wing_carried_mass, sizing_stations, WingFuelRelief, WingboxSizing,
    },
    tanks,
};

#[path = "product_beam_physics/invariants.rs"]
mod invariants;

fn geometry(config: &AlasConfig, design: &DesignVector) -> WingStructureGeometry {
    geometry_and_airfoils(config, design).0
}

fn geometry_and_airfoils(
    config: &AlasConfig,
    design: &DesignVector,
) -> (WingStructureGeometry, Airfoil, Airfoil) {
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(design), false)
        .unwrap();
    let wing = plane
        .wings
        .iter()
        .find(|wing| wing.name == "Main Wing")
        .unwrap();
    let (fractions, full_span) = config.structures.resolved_spars();
    let root_airfoil = &wing.xsecs.first().unwrap().airfoil;
    let tip_airfoil = &wing.xsecs.last().unwrap().airfoil;
    let geometry = WingStructureGeometry::new(
        design,
        &config.geometry.wing,
        root_airfoil,
        tip_airfoil,
        &fractions,
        Some(&full_span),
    )
    .unwrap();
    (geometry, root_airfoil.clone(), tip_airfoil.clone())
}

fn dry_strength_size(config: &AlasConfig, design: &DesignVector) -> WingboxSizing {
    let geometry = geometry(config, design);
    let cfg = &config.structures;
    let dry = vec![0.0; sizing_stations(&geometry, cfg).len()];
    size_wingbox_with_wing_carried_mass(
        &geometry,
        cfg,
        &config.requirements,
        materials::get(&cfg.skin_material).unwrap(),
        materials::get(&cfg.spar_web_material).unwrap(),
        materials::get(&cfg.spar_cap_material).unwrap(),
        materials::get(&cfg.rib_material).unwrap(),
        Some(&dry),
        &[],
    )
}

#[test]
fn transport_presets_conserve_the_declared_box_material_volumes() {
    for name in ["AVE", "A320-200", "B787-9", "A220-300"] {
        let config = AlasConfig::from_value(&serde_json::json!({"preset":name})).unwrap();
        let design = presets::get(name).unwrap().design_vector;
        let (geometry, root_airfoil, tip_airfoil) = geometry_and_airfoils(&config, &design);
        let cfg = &config.structures;
        let req = &config.requirements;
        let skin = materials::get(&cfg.skin_material).unwrap();
        let web = materials::get(&cfg.spar_web_material).unwrap();
        let cap = materials::get(&cfg.spar_cap_material).unwrap();
        let rib = materials::get(&cfg.rib_material).unwrap();
        let stations = sizing_stations(&geometry, cfg);
        let (front, rear) = box_chord_band(&geometry);
        let fuel = tanks::integral_fuel_running_mass_kg_m(&geometry, &stations, front, rear);
        let mounted = engine_point_loads_n(&config.geometry.engine, &config.mass_model, req);
        let initial = size_wingbox_with_wing_carried_mass(
            &geometry,
            cfg,
            req,
            skin,
            web,
            cap,
            rib,
            Some(&fuel),
            &mounted,
        );
        let product = size_for_linear_model(
            &geometry,
            initial,
            cfg,
            req,
            &config.geometry.engine,
            &config.mass_model,
            skin,
            web,
            cap,
            &fuel,
            &mounted,
            LinearModelLimits::default(),
        );
        assert!(product.converged, "{name}: {:?}", product.assessment);
        invariants::assert_material_volumes(&geometry, &product.sizing, skin, web, cap);
        invariants::assert_rib_material_volume(
            &geometry,
            &product.sizing,
            cfg.t_rib_m,
            rib.rho_kg_m3,
            [&root_airfoil, &tip_airfoil],
        );
        assert_eq!(
            product.assessment.primary_mass_kg,
            2.0 * product.sizing.total_mass_kg
        );
        // Positivity is an admissibility condition, not an aircraft-specific
        // displacement limit. Quadrature is verified against a closed-form
        // cantilever below; aircraft validation needs matched load/geometry.
        assert!(product.assessment.max_tip_deflection_ratio.is_finite());
        assert!(product.assessment.max_tip_deflection_ratio > 0.0);
    }
}

#[test]
fn span_increases_mass_and_depth_reduces_mass_in_the_strength_critical_domain() {
    let config = AlasConfig::from_value(&serde_json::json!({"preset":"A320-200"})).unwrap();
    let design = presets::get("A320-200").unwrap().design_vector;
    let baseline = dry_strength_size(&config, &design);
    let mut longer = design;
    longer.span_m *= 1.10;
    assert!(dry_strength_size(&config, &longer).total_mass_kg > baseline.total_mass_kg);
    // Keep chord, material, gross mass and fuel fixed. In a strength-critical
    // box, added depth reduces required bending material. This is not a
    // universal total-mass law after manufacturing floors become active:
    // deeper minimum-gauge webs/ribs themselves contain more material.
    assert!(
        baseline.mass_breakdown_kg.spar_caps
            > baseline.mass_breakdown_kg.spar_webs + baseline.mass_breakdown_kg.ribs
    );
    let mut deeper = design;
    deeper.airfoil_thickness_scale *= 1.10;
    assert!(dry_strength_size(&config, &deeper).total_mass_kg < baseline.total_mass_kg);
}

#[test]
fn running_mass_and_strength_response_use_the_exact_sized_inventory() {
    let config = AlasConfig::from_value(&serde_json::json!({"preset":"A320-200"})).unwrap();
    let design = presets::get("A320-200").unwrap().design_vector;
    let geometry = geometry(&config, &design);
    let sizing = dry_strength_size(&config, &design);
    let cfg = &config.structures;
    let skin = materials::get(&cfg.skin_material).unwrap();
    let web = materials::get(&cfg.spar_web_material).unwrap();
    let cap = materials::get(&cfg.spar_cap_material).unwrap();
    let (front, rear) = box_chord_band(&geometry);
    let mass = box_running_mass_kg_m(&sizing, skin, web, cap, front, rear);
    let integral: f64 = mass
        .windows(2)
        .zip(sizing.y_stations.windows(2))
        .map(|(mass, y)| 0.5 * (mass[0] + mass[1]) * (y[1] - y[0]))
        .sum();
    assert!((integral - sizing.total_mass_kg).abs() < 1.0e-8);
    let dry = vec![0.0; sizing.y_stations.len()];
    let report = analyze_structure_with_wing_carried_mass(
        &geometry,
        &sizing,
        cfg,
        &config.requirements,
        &config.geometry.engine,
        &config.mass_model,
        skin,
        web,
        cap,
        &dry,
        &[],
    );
    let result = assess(&sizing, &report, LinearModelLimits::default());
    assert!(result.max_strength_utilization <= 1.0 + alas_struct::sizing::MARGIN_NUMERICAL_ZERO);
    let area_per_height = sizing.spars[0].a_cap[0] / sizing.spars[0].h[0];
    for spar in &sizing.spars {
        // Common added flange gauge allocates area in proportion to bending
        // lever arm. This fixture has no root cap-width or thickness clipping.
        assert!((spar.a_cap[0] / spar.h[0] / area_per_height - 1.0).abs() < 1.0e-12);
    }
    let point = (0.37 * geometry.semi_span, 1_000.0);
    let with_point = analyze_structure_with_wing_carried_mass(
        &geometry,
        &sizing,
        cfg,
        &config.requirements,
        &config.geometry.engine,
        &config.mass_model,
        skin,
        web,
        cap,
        &dry,
        &[point],
    );
    for (base, relieved) in report.load_cases.iter().zip(&with_point.load_cases) {
        let force = base.load_factor * config.requirements.gravity_m_s2 * point.1;
        for (station, &span) in report.y.iter().enumerate() {
            let force_relief = if span <= point.0 { force } else { 0.0 };
            let moment_relief = force * (point.0 - span).max(0.0);
            assert!(
                (base.shear_n[station] - relieved.shear_n[station] - force_relief).abs() < 1.0e-8
            );
            assert!(
                (base.moment_nm[station] - relieved.moment_nm[station] - moment_relief).abs()
                    < 1.0e-8
            );
        }
    }
}

#[test]
fn axial_laminate_cap_reaches_the_declared_strain_limit_in_a_strength_critical_box() {
    let mut config = AlasConfig::from_value(&serde_json::json!({"preset":"A320-200"})).unwrap();
    config.requirements.mtow_kg = 15_000.0;
    config.structures.t_skin_min_m = 0.001;
    config.structures.t_web_min_m = 0.001;
    config.structures.spar_cap_material = "CFRP 60/30/10".into();
    config.structures.spar_web_material = "Al 7075-T6".into();
    config.structures.num_ribs_override = Some(41);
    let design = DesignVector {
        span_m: 20.0,
        root_chord_m: 4.0,
        break_chord_m: 4.0,
        tip_chord_m: 4.0,
        sweep_deg: 0.0,
        ..DesignVector::default()
    };
    // Equal-depth spars and metallic webs isolate cap axial compatibility,
    // as in a hybrid box. The independent preset test also exercises QI webs.
    // Flat covers still contribute their actual modulus-weighted EI.
    let airfoil = Airfoil::from_coordinates(
        "flat cover test section",
        vec![
            (1.0, 0.0),
            (0.9, 0.06),
            (0.1, 0.06),
            (0.0, 0.0),
            (0.1, -0.06),
            (0.9, -0.06),
            (1.0, 0.0),
        ],
    );
    let geometry = WingStructureGeometry::new(
        &design,
        &WingConfig::default(),
        &airfoil,
        &airfoil,
        &[0.25, 0.70],
        None,
    )
    .unwrap();
    let cfg = &config.structures;
    let cap = materials::get(&cfg.spar_cap_material).unwrap();
    let web = materials::get(&cfg.spar_web_material).unwrap();
    let skin = materials::get(&cfg.skin_material).unwrap();
    let dry = vec![0.0; sizing_stations(&geometry, cfg).len()];
    let scoped = size_wingbox_with_scope(
        &geometry,
        cfg,
        &config.requirements,
        skin,
        web,
        cap,
        materials::get(&cfg.rib_material).unwrap(),
        &WingFuelRelief::Declared {
            running_mass_kg_m: &dry,
            design_case: WingFuelDesignCase::declared(0.0, config.requirements.mtow_kg, None),
        },
        &WingMountedRelief {
            point_masses_kg: Vec::new(),
            omitted: Vec::new(),
        },
    );
    assert!(
        scoped.scope.relief_convergence.is_settled(),
        "{:?}",
        scoped.scope
    );
    let sizing = scoped.sizing;
    let response = analyze_structure_with_wing_carried_mass(
        &geometry,
        &sizing,
        cfg,
        &config.requirements,
        &config.geometry.engine,
        &config.mass_model,
        skin,
        web,
        cap,
        &dry,
        &[],
    );
    let result = assess(&sizing, &response, LinearModelLimits::default());
    assert!(result.max_strength_utilization <= 1.0 + alas_struct::sizing::MARGIN_NUMERICAL_ZERO);
    let maximum_cap_strain = response
        .load_cases
        .iter()
        .flat_map(|case| &case.spar_stress)
        .flat_map(|spar| &spar.stress_pa)
        .map(|stress| stress / cap.e_pa)
        .fold(0.0_f64, f64::max);
    // The 60/30/10 axial modulus follows CLT (NASA RP-1351, sec. V-B),
    // with AS4/3501-6 lamina data (NASA TM-104055, table 1). The product
    // damage-tolerant strain proxy is 0.004; clear-web combined strength and
    // buckling must size webs instead of imposing a lower artificial cap limit.
    let allowable_strain = alas_struct::allowables::bending_allowable_pa(cap) / cap.e_pa;
    assert!((allowable_strain - 0.004).abs() < 1.0e-15);
    // A 1e-8 relative bound permits the 1e-9 relieved-mass fixed-point closure
    // and section arithmetic, not uncertainty in the declared strain limit.
    assert!(
        (maximum_cap_strain / allowable_strain - 1.0).abs() < 1.0e-8,
        "cap strain={maximum_cap_strain}, root sections={:?}, assessment={result:?}",
        sizing
            .spars
            .iter()
            .map(|spar| (spar.h[0], spar.a_cap[0], spar.t_cap[0], spar.t_web))
            .collect::<Vec<_>>()
    );
}
