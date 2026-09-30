// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Model quantities the aircraft-parity contract requests that the base dump
//! did not export: ISA sea-level field performance, certification mass
//! limits, engine dry mass, Korn critical Mach, the cruise drag breakdown and
//! tail volume coefficients.
//!
//! Certification limits that the model does not compute (MMO, maximum
//! operating altitude) are deliberately not exported.

use super::*;
use alas_config::presets::AircraftPreset;
use alas_geom::aircraft::airplane::Airplane;
use alas_mass::flops_transport::propulsion::scaled_engine_kg;
use alas_mission::MissionResult;
use alas_pipeline::field_reference::isa_sea_level_field_reference;
use alas_pipeline::full_analysis::AnalysisReport;

/// A finite value as JSON, else null (JSON has no NaN).
fn finite(value: f64) -> Value {
    if value.is_finite() {
        json!(value)
    } else {
        Value::Null
    }
}

/// Maximum landing and zero-fuel mass with provenance.
///
/// MLW is the limit the gate applies (`AlasConfig::landing_mass_limit_kg`):
/// the registered reference when the preset declares one and the design mode
/// adopts it, else `mlw_fraction_mtow` of MTOW. MZFW exists only as a
/// registered reference: the model has no MZFW fraction, so a notional design
/// reports none rather than a made-up limit.
fn mass_limits(config: &AlasConfig, preset: &AircraftPreset) -> serde_json::Map<String, Value> {
    let mtow = config.requirements.mtow_kg;
    let mlw = config.landing_mass_limit_kg(mtow);
    let mlw_basis = if preset.reference.mlw_kg == Some(mlw) {
        "registered preset reference MLW".to_owned()
    } else {
        format!(
            "fraction of MTOW: mass_model.mlw_fraction_mtow = {}",
            config.mass_model.mlw_fraction_mtow
        )
    };
    let (mzfw, mzfw_basis) = match preset.reference.mzfw_kg {
        Some(value) => (finite(value), "registered preset reference MZFW"),
        None => (
            Value::Null,
            "unregistered: no MZFW is declared and the model defines no MZFW fraction of MTOW",
        ),
    };
    let mut out = serde_json::Map::new();
    out.insert("mlw_kg".to_owned(), finite(mlw));
    out.insert("mlw_basis".to_owned(), json!(mlw_basis));
    out.insert("mzfw_kg".to_owned(), mzfw);
    out.insert("mzfw_basis".to_owned(), json!(mzfw_basis));
    out
}

/// Dry mass of one engine, kg, with provenance.
///
/// A declared certificated dry mass (a preset input, not a prediction) is
/// used when the configuration carries one. Otherwise a turbofan gets the
/// FLOPS transport equation 76 default the mass model itself applies,
/// `THRSO/5.5` pounds scaled by the thrust ratio (NASA/TM-2017-219627
/// Appendix D); a turboprop without a declared dry mass has none, and the
/// GASP specific-weight fallback is not reported as an engine mass here.
fn engine_dry_mass(config: &AlasConfig) -> serde_json::Map<String, Value> {
    let structure = &config.mass_model.flops_structure;
    let (mass, basis) = match config.geometry.engine.active_model() {
        Ok(ActiveEngineModel::Turbofan(_)) => {
            let rated_n = config.geometry.engine.thrust_kn() * 1_000.0;
            let baseline_n = structure
                .baseline_engine_thrust_kn
                .map_or(rated_n, |kn| kn * 1_000.0);
            let kg = scaled_engine_kg(
                rated_n,
                baseline_n,
                structure.baseline_engine_mass_kg,
                structure.engine_mass_scaling_exponent,
            );
            let basis = if structure.baseline_engine_mass_kg.is_some() {
                "declared certificated dry mass (preset input, not a model prediction)"
            } else {
                "FLOPS equation 76 estimate, thrust/5.5 in pounds (model estimate)"
            };
            (finite(kg), basis)
        }
        Ok(ActiveEngineModel::Turboprop(_)) => {
            match config.mass_model.flops_turboprop.engine_dry_mass_kg {
                Some(kg) => (
                    finite(kg),
                    "declared certificated dry mass (preset input, not a model prediction)",
                ),
                None => (Value::Null, "no declared turboprop dry mass"),
            }
        }
        Err(_) => (Value::Null, "engine binding is not coherent"),
    };
    let mut out = serde_json::Map::new();
    out.insert("engine_dry_mass_kg".to_owned(), mass);
    out.insert("engine_dry_mass_basis".to_owned(), json!(basis));
    out
}

/// Korn drag-divergence and critical Mach at the design-point lift
/// coefficient. MMO and the maximum operating altitude are certification
/// limits the model does not compute, so they are absent by design.
fn critical_mach(
    config: &AlasConfig,
    report: &AnalysisReport,
    sweep_le_deg: f64,
) -> serde_json::Map<String, Value> {
    let cl = report.design_point.cl;
    // Same sweep the design point and the cruise polar use: the area-weighted
    // quarter-chord sweep, not the design vector's leading-edge sweep.
    let aero = AeroAnalysis::new(
        &report.airplane,
        AeroAnalysis::quarter_chord_sweep_deg(&report.airplane, sweep_le_deg),
        Some(config.geometry.clone()),
        Some(config.drag_model.clone()),
        Some(config.analysis.clone()),
    );
    let (mach_dd, mach_crit) = aero.korn_mach_numbers(cl, None);
    let mut out = serde_json::Map::new();
    out.insert("critical_mach".to_owned(), finite(mach_crit));
    out.insert("drag_divergence_mach".to_owned(), finite(mach_dd));
    out.insert("critical_mach_cl".to_owned(), finite(cl));
    out.insert(
        "critical_mach_basis".to_owned(),
        json!("Korn/Lock relations of AeroAnalysis::wave_drag at the design-point CL and the area-weighted c/4 sweep; M_crit = M_dd - (0.1/(4C))^(1/3)"),
    );
    out
}

/// Add every quantity of this module to the assembled per-preset document:
/// mass limits, engine dry mass, critical Mach and the mission drag breakdown
/// (moved from the route block to `aerodynamics`), plus the top-level
/// `performance` and `tail_volume_coefficients` blocks.
pub(super) fn attach(
    doc: &mut Value,
    config: &AlasConfig,
    preset: &AircraftPreset,
    report: &AnalysisReport,
) {
    let limits = mass_limits(config, preset);
    let mlw_kg = limits
        .get("mlw_kg")
        .and_then(Value::as_f64)
        .unwrap_or(f64::NAN);
    let breakdown = doc
        .pointer_mut("/route")
        .and_then(Value::as_object_mut)
        .and_then(|route| route.remove("cruise_drag_breakdown"));
    let mut extend = |section: &str, entries: serde_json::Map<String, Value>| {
        if let Some(target) = doc.get_mut(section).and_then(Value::as_object_mut) {
            target.extend(entries);
        }
    };
    extend("mass", limits);
    extend("propulsion", engine_dry_mass(config));
    let mut aerodynamics = critical_mach(config, report, preset.design_vector.sweep_deg);
    if let Some(value) = breakdown {
        aerodynamics.insert("cruise_drag_breakdown".to_owned(), value);
    }
    extend("aerodynamics", aerodynamics);
    doc["performance"] = field_performance(config, report, mlw_kg);
    doc["tail_volume_coefficients"] = tail_volumes(&report.airplane);
}

/// ISA sea-level field performance and reference speed at MTOW / MLW.
fn field_performance(config: &AlasConfig, report: &AnalysisReport, mlw_kg: f64) -> Value {
    let wing_area = report
        .geometry_summary
        .get("wing_area_m2")
        .copied()
        .unwrap_or(report.airplane.s_ref);
    match isa_sea_level_field_reference(
        config,
        wing_area,
        report.polar_fit.cd0,
        report.polar_fit.k,
        config.requirements.mtow_kg,
        mlw_kg,
    ) {
        Err(error) => json!({ "status": "unavailable", "error": error }),
        Ok(f) => json!({
            "status": "computed",
            "condition": "ISA sea level, zero wind, level dry runway, flaps per performance.cl_max_to / cl_max_land",
            "takeoff_field_length_isa_sl_mtow_m": f.takeoff_field_length_m,
            "takeoff_field_length_basis": f.takeoff_field_length_basis,
            "model_balanced_field_length_isa_sl_mtow_m": f.model_balanced_field_length_m,
            "model_balanced_field_length_basis": f.model_balanced_field_length_basis,
            "landing_field_length_isa_sl_mlw_m": f.landing_field_length_m,
            "landing_field_length_basis": f.landing_field_length_basis,
            "landing_distance_unfactored_isa_sl_mlw_m": f.landing_distance_m,
            "landing_distance_unfactored_basis": "alas_perf k_land (W/S)/(sigma CLmax_land) at MLW, ISA sea level: actual landing distance from 50 ft, before the 14 CFR 121.195(b) 1/0.6 factor",
            "vs1g_landing_m_s": f.vs1g_landing_m_s,
            "vref_mlw_m_s": f.vref_m_s,
            "vref_basis": "1.23 x V_S1g at MLW, 14 CFR 25.125(b)(2)(i) with V_SR taken as V_S1g (V_SR >= V_S1g, so slightly low); not the app approach-speed factor",
            "takeoff_mass_kg": f.takeoff_mass_kg,
            "landing_mass_kg": f.landing_mass_kg,
            "static_thrust_n": f.static_thrust_n,
            "static_thrust_basis": f.static_thrust_basis,
            "static_thrust_to_weight": f.static_tw,
            "oei_second_segment": {
                "required_gradient_far25_121b": f.oei.required_gradient,
                "required_inflight_tw": f.oei.required_inflight_tw,
                "required_sls_tw": f.oei.required_sls_tw,
                "status": format!("{:?}", f.oei.status),
                "available_gradient": null,
                "basis": f.oei.basis,
            },
        }),
    }
}

/// Horizontal and vertical tail volume coefficients from the built geometry.
///
/// `V_H = S_h l_h / (S_w c_ref)` and `V_V = S_v l_v / (S_w b_ref)` (Raymer,
/// *Aircraft Design*, eqs. 6.2-6.3), with the moment arm the streamwise
/// distance between the quarter-MAC points of the tail and the main wing.
/// The fin is a one-sided surface built in the XZ plane, so its area is
/// integrated over height; the horizontal tail area is its planform.
fn tail_volumes(plane: &Airplane) -> Value {
    let (Some(wing), Some(htail), Some(vtail)) = (
        plane.wings.first(),
        plane
            .wings
            .iter()
            .find(|w| w.name == "Horizontal Stabilizer"),
        plane.wings.iter().find(|w| w.name == "Vertical Stabilizer"),
    ) else {
        return json!({ "status": "unavailable: main wing or tail surface missing" });
    };
    let fin_area: f64 = vtail
        .xsecs
        .windows(2)
        .map(|pair| {
            (pair[1].xyz_le[2] - pair[0].xyz_le[2]).abs() * (pair[0].chord + pair[1].chord) / 2.0
        })
        .sum();
    let x_wing = wing.aerodynamic_center(0.25)[0];
    let l_h = htail.aerodynamic_center(0.25)[0] - x_wing;
    let l_v = vtail.aerodynamic_center(0.25)[0] - x_wing;
    let s_h = htail.projected_area();
    let s_w = plane.s_ref;
    json!({
        "horizontal": finite(s_h * l_h / (s_w * plane.c_ref)),
        "vertical": finite(fin_area * l_v / (s_w * plane.b_ref)),
        "htail_area_m2": s_h,
        "vtail_area_m2": fin_area,
        "htail_arm_m": l_h,
        "vtail_arm_m": l_v,
        "basis": "Raymer eqs. 6.2-6.3; arm = quarter-MAC to quarter-MAC along x; S_w, c_ref, b_ref from the analysed airplane",
    })
}

/// The mission deck's drag buildup at mid-cruise of the last cruise segment:
/// parasite by component, induced, compressibility (wave), ESDU
/// miscellaneous, trim and total, all on the vehicle reference area.
pub(super) fn cruise_drag_breakdown(mission: &MissionResult, plane: &Airplane) -> Value {
    let Some(segment) = mission.segments.iter().rev().find(|segment| {
        format!("{:?}", segment.spec.kind)
            .to_lowercase()
            .contains("cruise")
    }) else {
        return json!({ "status": "unavailable: no cruise segment flown" });
    };
    let conditions = &segment.conditions;
    let mid = conditions.drag_breakdown.len() / 2;
    let Some(db) = conditions.drag_breakdown.get(mid) else {
        return json!({ "status": "unavailable: no drag breakdown recorded" });
    };
    let wing_label = |index: usize| {
        if conditions.drag_breakdown[mid].parasite_wings.len() == plane.wings.len() {
            plane.wings[index].name.clone()
        } else {
            format!("wing_{index}")
        }
    };
    let wings: Vec<Value> = db
        .parasite_wings
        .iter()
        .enumerate()
        .map(|(i, c)| json!({ "component": wing_label(i), "cd": c.parasite_drag_coefficient }))
        .collect();
    let fuselages: Vec<f64> = db
        .parasite_fuselages
        .iter()
        .map(|c| c.parasite_drag_coefficient)
        .collect();
    let nacelles: Vec<f64> = db
        .parasite_nacelles
        .iter()
        .map(|c| c.parasite_drag_coefficient)
        .collect();
    json!({
        "status": "computed",
        "basis": "mission drag buildup (alas_aero::drag_buildup) at mid-point of the last cruise segment; coefficients on the vehicle reference area",
        "altitude_m": conditions.altitude_m.get(mid),
        "mach": conditions.mach.get(mid),
        "cl": conditions.lift_coefficient.get(mid),
        "parasite_total": db.parasite_total,
        "parasite_wings": wings,
        "parasite_fuselages": fuselages,
        "parasite_nacelles": nacelles,
        "parasite_pylon": db.parasite_pylon.parasite_drag_coefficient,
        "induced": db.induced_total,
        "induced_viscous": db.induced_viscous,
        "wave_compressibility": db.compressible_total,
        "miscellaneous_esdu": db.miscellaneous_total,
        "trim_increment": db.trim_corrected - db.untrimmed,
        "spoiler": db.spoiler,
        "untrimmed": db.untrimmed,
        "total": db.total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cheap_exports_are_finite_for_the_a320_preset() {
        let config = AlasConfig::from_value(&json!({ "preset": "A320-200" })).expect("A320 preset");
        let preset = presets::get("A320-200").expect("A320 preset");
        let limits = mass_limits(&config, &preset);
        assert_eq!(limits["mlw_kg"], json!(66_000.0));
        assert_eq!(limits["mzfw_kg"], json!(62_500.0));
        assert!(limits["mlw_basis"]
            .as_str()
            .is_some_and(|s| s.contains("registered")));
        let engine = engine_dry_mass(&config);
        assert_eq!(engine["engine_dry_mass_kg"], json!(2_454.8));
    }
}
