// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Independent structural constraints on every candidate; the FLOPS mass
//! ledger remains authoritative. All structural loads use the design gross
//! mass, not a convenient light mission dispatch state.

use alas_config::{AlasConfig, ConstraintPolicy, DesignVector};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::wing_structure::WingStructureGeometry;
use alas_struct::feasibility::{LinearModelLimits, StructuralFeasibility};

use super::types::{ConstraintFamily, ConstraintResidual};

/// Governing structural gross mass, kg, shared by search and final reporting.
pub fn structural_design_mass_kg(config: &AlasConfig) -> f64 {
    let declared = alas_mass::wing_reconciliation::design_gross_mass_kg(config);
    if !declared.is_finite() || !config.requirements.mtow_kg.is_finite() {
        return f64::NAN;
    }
    if config.optimizer.objective.mtow_sizing == alas_config::MtowSizing::Unconstrained {
        declared
    } else {
        declared.max(config.requirements.mtow_kg)
    }
}

/// Size and check one candidate under a single, consistent structural state.
/// The returned metrics are graded even outside the linear model's domain.
pub fn assess_candidate(
    config: &AlasConfig,
    dv: &DesignVector,
    plane: &Airplane,
) -> Result<StructuralFeasibility, &'static str> {
    let cfg = &config.structures;
    let mut requirements = config.requirements.clone();
    requirements.mtow_kg = structural_design_mass_kg(config);
    if cfg.spanwise_stations < 2
        || !requirements.mtow_kg.is_finite()
        || requirements.mtow_kg <= 0.0
        || !requirements.gravity_m_s2.is_finite()
        || requirements.gravity_m_s2 <= 0.0
        || !requirements.ultimate_load_factor.is_finite()
        || requirements.ultimate_load_factor <= 1.0
        || !requirements.limit_load_factor_neg.is_finite()
        || requirements.limit_load_factor_neg >= 0.0
        || !cfg.additional_safety_factor.is_finite()
        || cfg.additional_safety_factor < 1.0
    {
        return Err("structural_input_invalid");
    }
    let wing = alas_mass::wing_reconciliation::main_wing(plane).ok_or("structural_geometry")?;
    let root = wing.xsecs.first().ok_or("structural_geometry")?;
    let tip = wing.xsecs.last().ok_or("structural_geometry")?;
    let (fractions, full_span) = cfg.resolved_spars();
    let geometry = WingStructureGeometry::new(
        dv,
        &config.geometry.wing,
        &root.airfoil,
        &tip.airfoil,
        &fractions,
        Some(&full_span),
    )
    .map_err(|_| "structural_geometry")?;
    let skin =
        alas_config::materials::get(&cfg.skin_material).map_err(|_| "structural_material")?;
    let web =
        alas_config::materials::get(&cfg.spar_web_material).map_err(|_| "structural_material")?;
    let cap =
        alas_config::materials::get(&cfg.spar_cap_material).map_err(|_| "structural_material")?;
    let rib = alas_config::materials::get(&cfg.rib_material).map_err(|_| "structural_material")?;
    let stations = alas_struct::sizing::sizing_stations(&geometry, cfg);
    let (front, rear) = alas_struct::sizing::box_chord_band(&geometry);
    let declared = alas_mass::wing_reconciliation::declared_wing_fuel_case(
        config,
        dv,
        &requirements,
        &geometry,
        &stations,
        front,
        rear,
    );
    let fuel = declared
        .as_ref()
        .map(|case| case.running_mass_kg_m.clone())
        .unwrap_or_else(|| {
            alas_struct::tanks::integral_fuel_running_mass_kg_m(&geometry, &stations, front, rear)
        });
    let wing_mounted = alas_struct::scope::wing_mounted_relief(
        &config.geometry.engine,
        &config.mass_model,
        &requirements,
    );
    let point_masses = &wing_mounted.point_masses_kg;
    if !geometry.semi_span.is_finite()
        || geometry.semi_span <= 0.0
        || fuel.len() != stations.len()
        || fuel.iter().any(|mass| !mass.is_finite() || *mass < 0.0)
        || point_masses.iter().any(|(y, mass)| {
            !y.is_finite()
                || *y < 0.0
                || *y > geometry.semi_span
                || !mass.is_finite()
                || *mass < 0.0
        })
    {
        return Err("structural_input_invalid");
    }
    let fuel_scope = declared.as_ref().map_or(
        alas_struct::sizing::WingFuelRelief::EnclosedBoxVolume,
        |case| alas_struct::sizing::WingFuelRelief::Declared {
            running_mass_kg_m: &fuel,
            design_case: alas_struct::scope::WingFuelDesignCase::declared(
                case.capacity_kg,
                case.design_gross_mass_kg,
                case.max_zero_fuel_mass_kg,
            ),
        },
    );
    let scoped = alas_struct::sizing::size_wingbox_with_scope(
        &geometry,
        cfg,
        &requirements,
        skin,
        web,
        cap,
        rib,
        &fuel_scope,
        &wing_mounted,
    );
    if !scoped.scope.relief_convergence.is_settled() {
        return Err("structural_relief_not_converged");
    }
    let sizing = scoped.sizing;
    let result = alas_struct::sizing::size_for_linear_model(
        &geometry,
        sizing,
        cfg,
        &requirements,
        &config.geometry.engine,
        &config.mass_model,
        skin,
        web,
        cap,
        &fuel,
        point_masses,
        LinearModelLimits {
            max_curvature_relative_error: cfg.max_linear_curvature_relative_error,
        },
    );
    if !result.converged && result.assessment.passes() {
        return Err("structural_stiffness_not_converged");
    }
    let mut assessment = result.assessment;
    if !assessment.input_valid {
        return Ok(assessment);
    }
    if cfg.mesh_chordwise_points < 2 {
        return Err("structural_mesh_invalid");
    }
    // This is the identical product builder and design-load state used by
    // final structural reporting. Native primary mass cannot substitute for
    // the shell/bar material actually written to the solver deck. No solver
    // is launched during candidate evaluation, and CONM2 is excluded.
    let (deck, _, _) = alas_struct::mesh::build_wing_mesh_bdf_product(
        &geometry,
        &result.sizing,
        cfg,
        &config.geometry.engine,
        &config.mass_model,
        &requirements,
        skin,
        web,
        cap,
        rib,
    )
    .map_err(|_| "structural_mesh_invalid")?;
    assessment.mesh_primary_mass_kg = Some(
        deck.primary_structural_mass_kg()
            .map(|mass| 2.0 * mass)
            .filter(|mass| mass.is_finite() && *mass > 0.0)
            .ok_or("structural_mesh_invalid")?,
    );
    Ok(assessment)
}

/// Hard constraints cannot be disabled through geometric objective weights.
/// Invalid reports get an explicit sentinel; valid but flexible wings retain
/// their graded curvature residual so feasibility restoration has a direction.
pub(crate) fn residuals(
    config: &AlasConfig,
    dv: &DesignVector,
    plane: &Airplane,
    wing_mass_kg: f64,
) -> Vec<ConstraintResidual> {
    if !wing_mass_kg.is_finite() || wing_mass_kg <= 0.0 {
        return vec![ConstraintResidual::direct(
            "structural_wing_mass_unavailable",
            ConstraintFamily::Structure,
            1.0,
            0.0,
            "bool",
            1.0,
            1.0,
            ConstraintPolicy::Hard,
        )];
    }
    let assessment = match assess_candidate(config, dv, plane) {
        Ok(assessment) if assessment.input_valid => assessment,
        result => {
            let id = result.err().unwrap_or("structural_response_invalid");
            return vec![ConstraintResidual::direct(
                id,
                ConstraintFamily::Structure,
                1.0,
                0.0,
                "bool",
                1.0,
                1.0,
                ConstraintPolicy::Hard,
            )];
        }
    };
    let mut rows: Vec<_> = [
        (
            "structural_strength",
            assessment.max_strength_utilization,
            1.0 + alas_struct::sizing::MARGIN_NUMERICAL_ZERO,
            "-",
        ),
        (
            "structural_rib_spacing",
            assessment.rib_spacing_ratio,
            1.0,
            "-",
        ),
        (
            "structural_linear_model_domain",
            assessment.max_linear_curvature_relative_error,
            assessment.limits.max_curvature_relative_error,
            "-",
        ),
        (
            "structural_cap_packaging",
            assessment.cap_packaging_ratio,
            1.0 + alas_struct::sizing::MARGIN_NUMERICAL_ZERO,
            "-",
        ),
    ]
    .into_iter()
    .map(|(id, actual, limit, unit)| {
        // Domain budgets are explicit: do not enlarge them by the generic
        // geometry slack. Infinity is always a violation, never a NaN max.
        let residual = actual - limit;
        let violation = if residual.is_finite() {
            (residual / limit).max(0.0)
        } else {
            1.0e6
        };
        ConstraintResidual::direct(
            id,
            ConstraintFamily::Structure,
            actual,
            limit,
            unit,
            residual,
            violation,
            ConstraintPolicy::Hard,
        )
    })
    .collect();
    // FLOPS and explicit primary inventories have different model scopes.
    // Keep the signed differences inspectable, with no rejection or penalty.
    for (id, actual) in [
        (
            "structural_primary_mass_discrepancy",
            assessment.primary_mass_kg,
        ),
        (
            "structural_mesh_mass_discrepancy",
            assessment.mesh_primary_mass_kg.unwrap_or(f64::NAN),
        ),
    ] {
        if !actual.is_finite() || actual <= 0.0 {
            rows.push(ConstraintResidual::direct(
                "structural_response_invalid",
                ConstraintFamily::Structure,
                1.0,
                0.0,
                "bool",
                1.0,
                1.0,
                ConstraintPolicy::Hard,
            ));
            continue;
        }
        rows.push(ConstraintResidual::direct(
            id,
            ConstraintFamily::Structure,
            actual,
            wing_mass_kg,
            "kg",
            actual - wing_mass_kg,
            0.0,
            ConstraintPolicy::Off,
        ));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_design_mass_cannot_follow_a_light_dispatch_or_invalid_override() {
        let mut config = AlasConfig::default();
        let declared = config.requirements.mtow_kg;
        config.mass_model.flops_structure.design_gross_mass_kg = Some(declared * 0.8);
        assert_eq!(structural_design_mass_kg(&config), declared);
        config.mass_model.flops_structure.design_gross_mass_kg = Some(f64::NAN);
        assert!(structural_design_mass_kg(&config).is_nan());
    }

    #[test]
    fn empirical_mass_disagreement_is_diagnostic_while_physical_checks_remain_hard() {
        let mut config = AlasConfig::from_value(&serde_json::json!({"preset":"B787-9"})).unwrap();
        config.structures.enabled = false;
        config.optimizer.objective.geometry_constraints = ConstraintPolicy::Off;
        let design = alas_config::presets::get("B787-9").unwrap().design_vector;
        let plane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), true)
            .unwrap();
        let rows = residuals(&config, &design, &plane, 1.0);
        let comparison = rows
            .iter()
            .find(|r| r.id == "structural_primary_mass_discrepancy")
            .expect("a finite structural candidate keeps its quantitative mass comparison");
        assert_eq!(comparison.policy, ConstraintPolicy::Off);
        assert!(comparison.actual > comparison.limit);
        assert_eq!(comparison.normalized_violation, 0.0);
        assert!(rows
            .iter()
            .filter(|r| !r.id.ends_with("_mass_discrepancy"))
            .all(|r| r.policy == ConstraintPolicy::Hard));
        let other_estimate = residuals(&config, &design, &plane, 1.0e9);
        assert_eq!(
            rows.iter()
                .filter(|r| r.policy == ConstraintPolicy::Hard)
                .collect::<Vec<_>>(),
            other_estimate
                .iter()
                .filter(|r| r.policy == ConstraintPolicy::Hard)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn native_mass_cannot_hide_the_product_mesh_material_mass() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset":"B787-9"})).unwrap();
        // Reproduced native-only winner, seed 7, eight workers. Inventories
        // remain visible without treating an empirical estimate as a limit.
        let design: DesignVector = serde_json::from_value(serde_json::json!({
            "airfoil_camber_scale":0.9951258142606898,
            "airfoil_thickness_scale":1.089107744367106,
            "break_chord_m":6.793936224446202,
            "bump_lower_mid":0.001016907580121824,
            "bump_lower_rear":0.0006525770100045887,
            "bump_upper_front":-0.0001422129687166599,
            "bump_upper_rear":0.00012157895992083976,
            "fuselage_length_m":62.81,"root_chord_m":12.432046053434357,
            "span_m":54.741361038055786,"sweep_deg":32.46682464341532,
            "tail_scale":1.0,"tail_x_shift_m":0.0,"tip_chord_m":1.6182107675829762,
            "tip_twist_deg":-0.28065561627890057,"wing_x_shift_m":-1.7614440516931102
        }))
        .unwrap();
        let plane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), true)
            .unwrap();
        // The FE deck carries material the native beam inventory does not
        // (the skin and ribs ahead of and behind the box), so a wing budget
        // the native mass meets can still be exceeded by the deck that is
        // actually solved. The budget is placed between the two rather than
        // pinned, so the check survives any change to the sizing law: the
        // mesh row must report the deck's own mass and land over the budget
        // the native row lands under, and both rows stay diagnostic.
        let assessment = assess_candidate(&config, &design, &plane).unwrap();
        let native_kg = assessment.primary_mass_kg;
        let mesh_kg = assessment.mesh_primary_mass_kg.unwrap();
        assert!(
            mesh_kg > native_kg,
            "mesh {mesh_kg} kg vs native {native_kg} kg"
        );
        let budget = 0.5 * (native_kg + mesh_kg);
        let rows = residuals(&config, &design, &plane, budget);
        let native = rows
            .iter()
            .find(|r| r.id == "structural_primary_mass_discrepancy")
            .unwrap();
        let mesh = rows
            .iter()
            .find(|r| r.id == "structural_mesh_mass_discrepancy")
            .unwrap();
        assert_eq!(native.actual, native_kg);
        assert_eq!(mesh.actual, mesh_kg);
        assert!(native.raw_residual < 0.0);
        assert!(mesh.raw_residual > 0.0);
        for row in [native, mesh] {
            assert_eq!(row.policy, ConstraintPolicy::Off);
            assert_eq!(row.normalized_violation, 0.0);
        }
    }

    #[test]
    fn invalid_product_mesh_resolution_fails_closed_before_search_acceptance() {
        let mut config = AlasConfig::from_value(&serde_json::json!({"preset":"B787-9"})).unwrap();
        // Chordwise mesh resolution participates only in the FE deck, so
        // this catches an invalid mesh request the native section cannot see.
        config.structures.mesh_chordwise_points = 0;
        let design = alas_config::presets::get("B787-9").unwrap().design_vector;
        let plane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), true)
            .unwrap();
        assert_eq!(
            assess_candidate(&config, &design, &plane),
            Err("structural_mesh_invalid")
        );
        let rows = residuals(&config, &design, &plane, 1.0e9);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "structural_mesh_invalid");
        assert_eq!(rows[0].policy, ConstraintPolicy::Hard);
        assert!(rows[0].normalized_violation > 0.0);
    }
}
