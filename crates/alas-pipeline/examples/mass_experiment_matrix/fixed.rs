// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Case A: the product report path at the declared design gross mass, and
//! the bounded A320 sensitivities evaluated through the low-level FLOPS API.

use std::time::Instant;

use alas_config::design_variables::DesignVector;
use alas_config::oew_reference::{self, OewReference, OewSourceTier};
use alas_config::{presets, AlasConfig, DesignMode};
use alas_mass::flops_transport::structure::wing_mass;
use alas_payload::layout::LayoutSummary;
use alas_pipeline::FullAnalysis;
use serde_json::{json, Value};

use super::support::{apply_run_options, buildup_json, oew_kg, oew_reference_json};

/// Case A: the product report path at the declared design gross mass, in the
/// design mode the registered aircraft belongs to (`BaselineSandbox`, whose
/// landing-mass basis is the declared limit) and, for contrast, in the
/// clean-sheet mode a preset loads in by default.
pub(crate) fn fixed_design_weight_case(
    name: &str,
    design: &DesignVector,
    mode: DesignMode,
) -> Value {
    let mut config = match AlasConfig::from_value(&json!({ "preset": name })) {
        Ok(config) => config,
        Err(error) => return json!({ "status": "config_error", "reason": error.to_string() }),
    };
    config.optimizer.design_space.mode = mode;
    apply_run_options(&mut config);
    let started = Instant::now();
    let report = match FullAnalysis::new(config.clone()).run(design, true) {
        Ok(report) => report,
        Err(error) => return json!({ "status": "failed", "reason": error }),
    };
    let Some(buildup) = report.flops_mass_buildup.as_deref() else {
        return json!({ "status": "no_pure_flops_buildup" });
    };
    let seated = report
        .payload_layout
        .as_ref()
        .and_then(|layout| match &layout.summary {
            LayoutSummary::Passenger(summary) => Some(json!({
                "seated_pax": summary.seated_pax,
                "total_pax": summary.total_pax,
                "unseated_pax": summary.unseated_pax,
                "classes": summary.classes,
                "payload_total_mass_kg": layout.total_mass,
                "seat_mass_t": summary.seat_mass_t,
                "bag_mass_t": summary.bag_mass_t,
                "belly_cargo_t": summary.belly_cargo_t,
                "payload_t": summary.payload_t,
            })),
            LayoutSummary::Cargo(_) => None,
        });
    let masses = &buildup.masses;
    let oew = oew_kg(masses);
    let zfw = oew + masses.payload;
    let preset = presets::get(name).ok();
    let mzfw = preset.and_then(|p| p.reference.mzfw_kg);
    let mlw = preset.and_then(|p| p.reference.mlw_kg);
    let record = oew_reference::get(name);
    let oew_ref = record.and_then(OewReference::preset_reference_oew_kg);
    let mut clean_sheet = config.clone();
    clean_sheet.optimizer.design_space.mode = DesignMode::CleanSheet;
    let mlw_clean_sheet_kg = clean_sheet.landing_mass_limit_kg(clean_sheet.requirements.mtow_kg);
    let mut sandbox = config.clone();
    sandbox.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    let mlw_sandbox_kg = sandbox.landing_mass_limit_kg(sandbox.requirements.mtow_kg);
    json!({
        "status": "ok",
        "elapsed_s": started.elapsed().as_secs_f64(),
        "design_mode": config.optimizer.design_space.mode.as_str(),
        "declared_mtow_kg": config.requirements.mtow_kg,
        "requirements_num_passengers": config.requirements.num_passengers,
        "requirements_cargo_payload_kg": config.requirements.cargo_payload_kg,
        "requirements_max_structural_payload_kg": config.requirements.max_structural_payload_kg,
        "passenger_mass_kg": config.requirements.passenger_mass_kg,
        "checked_bag_mass_kg": config.cabin.passenger.checked_bag_mass_kg,
        "belly_cargo_kg": config.cabin.passenger.belly_cargo_kg,
        "declared_baseline_engine_mass_kg": config.mass_model.flops_structure.baseline_engine_mass_kg,
        "landing_mass_limit_clean_sheet_kg": mlw_clean_sheet_kg,
        "landing_mass_limit_baseline_sandbox_kg": mlw_sandbox_kg,
        "buildup": buildup_json(buildup),
        "layout": seated,
        "oew_kg": oew,
        "actual_zfw_kg": zfw,
        "reference_mzfw_kg": mzfw,
        "mzfw_margin_kg": mzfw.map(|limit| limit - zfw),
        "reference_mlw_kg": mlw,
        "reference_oew_kg": oew_ref,
        "oew_residual_kg": oew_ref.map(|r| oew - r),
        "oew_reference": oew_reference_json(name, oew),
        "signed_fuel_closure_kg": masses.fuel,
        "usable_fuel_capacity_kg": preset.and_then(|p| p.reference.usable_fuel_mass_kg),
        "component_masses_report": report.component_masses,
        "analysis_mass_basis_kg": report.geometry_summary.get("analysis_mass_basis_kg").copied(),
    })
}

/// The bounded A320 reconstruction and sensitivity rows, evaluated through
/// the low-level FLOPS wing equation with itemized input changes only.
pub(crate) fn a320_sensitivities(fixed: &Value) -> Value {
    if fixed.pointer("/buildup/structure").is_none() {
        return json!({ "status": "unavailable" });
    }
    // Re-evaluate the registered wing with one input changed at a time.
    let mut config = match AlasConfig::from_value(&json!({ "preset": "A320-200" })) {
        Ok(config) => config,
        Err(error) => return json!({ "status": "config_error", "reason": error.to_string() }),
    };
    apply_run_options(&mut config);
    let preset = match presets::get("A320-200") {
        Ok(preset) => preset,
        Err(error) => return json!({ "status": "preset_error", "reason": error.to_string() }),
    };
    let plane = match alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
    {
        Ok(plane) => plane,
        Err(error) => return json!({ "status": "build_error", "reason": error.to_string() }),
    };
    let analysis_mass_model = config.analysis_mass_model(config.requirements.mtow_kg);
    let built = match alas_mass::breakdown::calculate_flops_mass_buildup(
        &plane,
        &config.requirements,
        &config.geometry,
        &config.control_surfaces,
        Some(&analysis_mass_model),
        &config.landing_gear,
        &config.cabin,
    ) {
        Ok(alas_mass::breakdown::ProductMassBuildup::PureFlops(buildup)) => buildup,
        Ok(_) => return json!({ "status": "not_pure_flops" }),
        Err(error) => return json!({ "status": "buildup_error", "reason": error.to_string() }),
    };
    let wing_inputs = built.airframe.structure_inputs.wing;
    let nominal = wing_mass(&wing_inputs);
    let mut rows = Vec::new();
    let mut push = |label: &str,
                    inputs: alas_mass::flops_transport::structure::FlopsWingInputs,
                    note: &str| {
        let result = wing_mass(&inputs);
        rows.push(json!({
            "label": label,
            "wing_kg": result.total_kg,
            "delta_wing_kg": result.total_kg - nominal.total_kg,
            "inputs": {
                "design_gross_mass_kg": inputs.design_gross_mass_kg,
                "wing_span_m": inputs.wing_span_m,
                "wing_area_m2": inputs.wing_area_m2,
                "ultimate_load_factor": inputs.ultimate_load_factor,
                "thickness_to_chord": inputs.thickness_to_chord,
            },
            "note": note,
        }));
    };
    push(
        "nominal",
        wing_inputs,
        "registered A320-214 WV017 sharklet deck",
    );
    let mut no_sharklet = wing_inputs;
    no_sharklet.wing_span_m = 34.10;
    push(
        "span_34.10_m_pre_sharklet",
        no_sharklet,
        "Airbus A320 ACAP Rev 46 section 2-1-1: 34.10 m span without sharklets; area held",
    );
    let mut dg77 = wing_inputs;
    dg77.design_gross_mass_kg = 77_000.0;
    push(
        "dg_77000_kg",
        dg77,
        "operator F-HDRF sheet: 77,000 kg MTOW weight variant; structural DG assumed equal",
    );
    let mut both = no_sharklet;
    both.design_gross_mass_kg = 77_000.0;
    push(
        "span_34.10_m_and_dg_77000_kg",
        both,
        "both changes together",
    );
    let mut ulf = wing_inputs;
    ulf.ultimate_load_factor = 3.75 * 1.1;
    push(
        "ulf_plus_10pct",
        ulf,
        "bounded sensitivity only: no A320 design load case is retained",
    );
    json!({
        "status": "ok",
        "nominal_wing_kg": nominal.total_kg,
        "rows": rows,
    })
}

/// Which registry value a reconstructed case is compared with.
#[derive(Clone, Copy)]
pub(crate) enum CaseReference {
    /// No published value for this case.
    None,
    /// The registry's comparable value of the preset itself.
    PresetValue,
    /// The registry's anchor for a different configuration, which this case
    /// reconstructs.
    CaseAnchor,
}

/// One explicit reconstructed reference case: the registered preset with an
/// itemized list of declared input changes, evaluated through the same
/// product report path as case A. Nothing in the registry is edited.
pub(crate) struct ReconstructedCase {
    pub(crate) label: &'static str,
    pub(crate) preset: &'static str,
    /// Where the published comparison value comes from.
    pub(crate) reference: CaseReference,
    pub(crate) reference_note: &'static str,
    pub(crate) apply: fn(&mut AlasConfig),
    pub(crate) changes: &'static [&'static str],
}

impl ReconstructedCase {
    /// The comparison value, kg, its tier and applicability, read from the
    /// registry.
    fn resolved_reference(&self) -> (Option<f64>, Option<&'static str>, &'static str) {
        let Some(record) = oew_reference::get(self.preset) else {
            return (None, None, "no_registry_record");
        };
        match self.reference {
            CaseReference::None => (None, None, "none"),
            CaseReference::PresetValue => (
                record.reference_oew_kg,
                record.source.map(|s| OewSourceTier::as_str(s.tier)),
                record.applicability.as_str(),
            ),
            CaseReference::CaseAnchor => match record.case_anchor {
                Some(anchor) if anchor.case_label == self.label => (
                    Some(anchor.value_kg),
                    Some(OewSourceTier::as_str(anchor.source.tier)),
                    "case_anchor",
                ),
                _ => (None, None, "case_anchor_missing"),
            },
        }
    }
}

/// Declare a cabin by per-class count and seat geometry (pitch and width in
/// metres), the way a planning layout states it.
fn declare_count_cabin(
    config: &mut AlasConfig,
    first: (i64, f64, f64),
    business: (i64, f64, f64),
    economy: (i64, f64, f64),
) {
    let cabin = &mut config.cabin.passenger;
    cabin.class_mix_mode = "count".to_owned();
    cabin.first.count = first.0;
    cabin.first.pitch_m = first.1;
    cabin.first.width_m = first.2;
    cabin.business.count = business.0;
    cabin.business.pitch_m = business.1;
    cabin.business.width_m = business.2;
    cabin.premium.count = 0;
    cabin.economy.count = economy.0;
    cabin.economy.pitch_m = economy.1;
    cabin.economy.width_m = economy.2;
    config.requirements.num_passengers = first.0 + business.0 + economy.0;
}

pub(crate) fn reconstructed_cases() -> Vec<ReconstructedCase> {
    vec![
        ReconstructedCase {
            label: "A320_registered_78t_product_cabin",
            preset: "A320-200",
            reference: CaseReference::None,
            reference_note: "no configuration-matched OEW is published for A320-214 WV017 Sharklet; the registry records the 45,000 kg maintenance and 41,000 kg rescue figures as not OEW and the former 41,244 kg as unsourced",
            apply: |_| {},
            changes: &["none: registered preset in BaselineSandbox; cabin as the product seats it (180Y at 28 in)"],
        },
        ReconstructedCase {
            label: "A320_F-HDRF_77t_180Y",
            preset: "A320-200",
            reference: CaseReference::CaseAnchor,
            reference_note: "operator F-HDRF sheet (secondary): A320-214, CFM56-5B4/3, 180Y, MTOW 77,000 kg, MLW 64,500 kg, MZFW 61,000 kg, max fuel 19,476 kg, empty weight 41,052 kg with an unspecified inclusion list; the 34.10 m wingtip-fence span is not applied to the geometry (see the wing sensitivity row)",
            apply: |config| {
                config.requirements.mtow_kg = 77_000.0;
                config.mass_model.flops_structure.design_landing_mass_kg = Some(64_500.0);
                config.mass_model.flops_transport.maximum_fuel_capacity_kg = Some(19_476.0);
                config.mass_model.flops_transport.flight_attendant_count = Some(4);
            },
            changes: &[
                "requirements.mtow_kg 78000 -> 77000 (design gross mass, F-HDRF sheet)",
                "flops_structure.design_landing_mass_kg None -> 64500 (F-HDRF sheet MLW)",
                "flops_transport.maximum_fuel_capacity_kg 19334 -> 19476 (F-HDRF sheet max fuel)",
                "flops_transport.flight_attendant_count 4 (unchanged; 180 seats need four)",
                "cabin: the product layout already seats 180Y, matching the sheet's 180-seat configuration",
            ],
        },
        ReconstructedCase {
            label: "A320_150_two_class_declared",
            preset: "A320-200",
            reference: CaseReference::None,
            reference_note: "Airbus ACAP Rev 46 typical 12F+138Y layout declared by count with planning seat geometry (first 36 in pitch 2-2, economy 31 in pitch 3-3); a planning diagram, not a weighed interior; no matched OEW",
            apply: |config| {
                declare_count_cabin(
                    config,
                    (12, 0.9144, 0.55),
                    (0, 0.9144, 0.55),
                    (138, 0.7874, 0.46),
                );
            },
            changes: &[
                "cabin.passenger.class_mix_mode percent -> count: 12F/0B/138Y (ACAP Rev 46 section 2-4-1)",
                "cabin.passenger.first pitch/width -> 0.9144 m / 0.55 m (declared planning geometry)",
                "cabin.passenger.economy pitch/width -> 0.7874 m / 0.46 m (declared planning geometry; 32 in leaves one row unseated in this cabin model)",
            ],
        },
        ReconstructedCase {
            label: "A220_registered_product_cabin",
            preset: "A220-300",
            reference: CaseReference::PresetValue,
            reference_note: "Airbus A220 recovery publication planning OEW 37,149 kg (primary; inclusion list stated); holdout frozen before any correction",
            apply: |_| {},
            changes: &["none: registered preset in BaselineSandbox; cabin as the product seats it (145Y, exit-limited)"],
        },
        ReconstructedCase {
            label: "A220_140Y_declared",
            preset: "A220-300",
            reference: CaseReference::PresetValue,
            reference_note: "same primary planning OEW; the 140-seat standard planning cabin (ACP Issue 013, OWE 37,081 kg for the same cabin) declared by count",
            apply: |config| {
                declare_count_cabin(
                    config,
                    (0, 0.9144, 0.55),
                    (0, 0.9144, 0.55),
                    (140, 0.8128, 0.47),
                );
            },
            changes: &[
                "cabin.passenger.class_mix_mode percent -> count: 140Y (Airbus 140-seat standard planning cabin)",
                "cabin.passenger.economy pitch/width -> 0.8128 m / 0.47 m (declared planning geometry)",
            ],
        },
        ReconstructedCase {
            label: "A340_typical_335_two_class_declared",
            preset: "A340-300",
            reference: CaseReference::PresetValue,
            reference_note: "Airbus ACAP Rev 33 jacking-figure OEW 131,215 kg (primary; weight variant and cabin not stated); the document's typical 30F/305Y layout declared by count",
            apply: |config| {
                declare_count_cabin(
                    config,
                    (30, 2.0, 0.95),
                    (0, 1.55, 0.70),
                    (305, 0.81, 0.46),
                );
                config.mass_model.flops_transport.flight_attendant_count = Some(9);
            },
            changes: &[
                "cabin.passenger.class_mix_mode percent -> count: 30F/0B/305Y (ACAP Rev 33 section 2-4-1 typical layout)",
                "cabin.passenger.first pitch/width -> 2.0 m / 0.95 m; economy 0.81 m / 0.46 m (declared planning geometry)",
                "flops_transport.flight_attendant_count -> 9 (the typical layout's attendant positions)",
            ],
        },
        ReconstructedCase {
            label: "A380_typical_555_three_class_declared",
            preset: "A380-800",
            reference: CaseReference::CaseAnchor,
            reference_note: "aggregator typical three-class 277,000 kg (no primary document; spread 270-285 t); the Airbus typical 22F/96C/437Y layout declared by count",
            apply: |config| {
                declare_count_cabin(
                    config,
                    (22, 2.0, 0.95),
                    (96, 1.55, 0.70),
                    (437, 0.81, 0.46),
                );
            },
            changes: &[
                "cabin.passenger.class_mix_mode percent -> count: 22F/96B/437Y (ACAP Rev 20 typical three-class layout)",
                "cabin.passenger first/business/economy pitch and width -> 2.0/0.95, 1.55/0.70, 0.81/0.46 m (declared planning geometry)",
            ],
        },
        ReconstructedCase {
            label: "B787_typical_two_class_290_declared",
            preset: "B787-9",
            reference: CaseReference::CaseAnchor,
            reference_note: "128,850 kg attributed to Boeing ACAP Rev L (superseded, not retrieved; inclusion list unknown); the Boeing typical 28C/262Y layout declared by count",
            apply: |config| {
                declare_count_cabin(
                    config,
                    (0, 2.0, 0.95),
                    (28, 1.55, 0.70),
                    (262, 0.81, 0.46),
                );
            },
            changes: &[
                "cabin.passenger.class_mix_mode percent -> count: 0F/28B/262Y (ACAP Rev Q section 2.1.2 typical two-class layout)",
                "cabin.passenger business/economy pitch and width -> 1.55/0.70, 0.81/0.46 m (declared planning geometry)",
            ],
        },
        ReconstructedCase {
            label: "DC10_registered_572k_product_cabin",
            preset: "DC-10",
            reference: CaseReference::PresetValue,
            reference_note: "ACAP Series 30 passenger OWE with the 572,000 lb footnote applied: 120,914 kg (primary; standard 255-seat cabin, class split not published)",
            apply: |_| {},
            changes: &["none: registered preset in BaselineSandbox; cabin as the product seats it"],
        },
        ReconstructedCase {
            label: "DC10_standard_555k_row_declared",
            preset: "DC-10",
            reference: CaseReference::CaseAnchor,
            reference_note: "ACAP Series 30 passenger standard row (555,000 lb): OWE 120,742 kg, MLW 182,798 kg; same cabin and document, different weight option",
            apply: |config| {
                config.requirements.mtow_kg = 251_744.0;
                config.mass_model.flops_structure.design_landing_mass_kg = Some(182_798.0);
            },
            changes: &[
                "requirements.mtow_kg 259454 -> 251744 (ACAP standard 555,000 lb row)",
                "flops_structure.design_landing_mass_kg None -> 182798 (ACAP standard row MLW 403,000 lb)",
            ],
        },
    ]
}

/// Evaluate one reconstructed case through the product report path.
pub(crate) fn reconstructed_case(case: &ReconstructedCase) -> Value {
    let Ok(preset) = presets::get(case.preset) else {
        return json!({ "label": case.label, "status": "preset_error" });
    };
    let mut config = match AlasConfig::from_value(&json!({ "preset": case.preset })) {
        Ok(config) => config,
        Err(error) => {
            return json!({ "label": case.label, "status": "config_error", "reason": error.to_string() })
        }
    };
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    apply_run_options(&mut config);
    (case.apply)(&mut config);
    let (reference_oew_kg, reference_tier, reference_status) = case.resolved_reference();
    let report = match FullAnalysis::new(config.clone()).run(&preset.design_vector, true) {
        Ok(report) => report,
        Err(error) => return json!({ "label": case.label, "status": "failed", "reason": error }),
    };
    let Some(buildup) = report.flops_mass_buildup.as_deref() else {
        return json!({ "label": case.label, "status": "no_pure_flops_buildup" });
    };
    let seated = report
        .payload_layout
        .as_ref()
        .and_then(|layout| match &layout.summary {
            LayoutSummary::Passenger(summary) => Some(json!({
                "seated_pax": summary.seated_pax,
                "unseated_pax": summary.unseated_pax,
                "classes": summary.classes,
                "payload_total_mass_kg": layout.total_mass,
            })),
            LayoutSummary::Cargo(_) => None,
        });
    let oew = oew_kg(&buildup.masses);
    json!({
        "label": case.label,
        "preset": case.preset,
        "status": "ok",
        "changes": case.changes,
        "reference_oew_kg": reference_oew_kg,
        "reference_tier": reference_tier,
        "reference_status": reference_status,
        "reference_note": case.reference_note,
        "declared_mtow_kg": config.requirements.mtow_kg,
        "declared_baseline_engine_mass_kg": config.mass_model.flops_structure.baseline_engine_mass_kg,
        "buildup": buildup_json(buildup),
        "layout": seated,
        "oew_kg": oew,
        "oew_residual_kg": reference_oew_kg.map(|r| oew - r),
        "oew_residual_pct": reference_oew_kg.map(|r| 100.0 * (oew - r) / r),
        "actual_zfw_kg": oew + buildup.masses.payload,
        "signed_fuel_closure_kg": buildup.masses.fuel,
    })
}
