// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The neutral point for the pre-run Load & trim sheet preview.
//!
//! `quick_preview_report` skips the aerodynamic analysis, so its neutral point
//! is NaN and the CG gate (aft limit = neutral point minus the static-margin
//! requirement) cannot be assessed. This module fills it from the same
//! multi-condition VLM neutral-point set the full analysis uses, on the
//! configuration's own analysis resolution. The set depends only on the
//! geometry, the control surfaces and the cruise condition, not on the mass
//! inputs, so the last result is memoised and a mass-form drag does not
//! re-run the VLM.

use std::sync::Mutex;

use alas_config::AlasConfig;
use alas_pipeline::full_analysis::AnalysisReport;
use alas_stab::neutral_point::{
    neutral_point_conditions, HighLiftDevices, NeutralPointConditions, NpConditionsInput,
    PropellerSpec,
};

static LAST: Mutex<Option<(String, NeutralPointConditions)>> = Mutex::new(None);

/// Give a pre-run draft report a real neutral point and neutral-point set so
/// the Load & trim sheet can be built from the current inputs. Errors (a
/// geometry the VLM cannot mesh, a cruise Mach outside the subsonic domain)
/// leave the report untouched.
///
/// # Errors
///
/// A message naming why the neutral point could not be computed.
pub fn attach_preview_neutral_point(
    report: &mut AnalysisReport,
    config: &AlasConfig,
) -> Result<(), String> {
    let req = &config.requirements;
    if !req.cruise_mach.is_finite()
        || !(0.0..1.0).contains(&req.cruise_mach)
        || !req.cruise_altitude_m.is_finite()
    {
        return Err("cruise condition is outside the finite subsonic domain".to_owned());
    }
    let jet_inlets = alas_opt::envelope::jet_inlet_spec(
        config,
        preview_inlet_polar(report, config),
        report.airplane.s_ref,
    );
    let key = format!(
        "{:?}|{:?}|{:?}|{:?}|{}|{}|{:?}|{:?}",
        config.geometry,
        report.design,
        config.analysis,
        config.control_surfaces,
        req.cruise_mach,
        req.cruise_altitude_m,
        config.performance.high_lift_chord_extension,
        jet_inlets,
    );
    let cached = LAST.lock().ok().and_then(|last| {
        last.as_ref()
            .filter(|(k, _)| *k == key)
            .map(|(_, c)| c.clone())
    });
    let conditions = match cached {
        Some(conditions) => conditions,
        None => {
            let mut input = NpConditionsInput {
                low_speed_altitude_m: 0.0,
                cruise_mach: req.cruise_mach,
                cruise_altitude_m: req.cruise_altitude_m,
                max_operating: config.max_operating_condition(),
                high_lift: HighLiftDevices::declared(
                    config.performance.high_lift_chord_extension,
                    &config.control_surfaces,
                ),
                jet_inlets,
                propeller: config.geometry.engine.turboprop.as_ref().and_then(|spec| {
                    Some(PropellerSpec {
                        blades: spec.propeller_blade_count?,
                        diameter_m: spec.propeller_diameter_m,
                    })
                }),
                ..NpConditionsInput::default()
            };
            // The preview airplane is not built by the analysis pipeline, so it
            // carries no tail ratio yet: give it the one the full analysis
            // gives (stated, else the geometry's), so the two neutral points
            // are one path.
            let mut plane = report.airplane.clone();
            alas_aero::analysis::apply_tail_efficiency(&mut plane, config);
            input.wing_stiffness =
                alas_mass::wing_reconciliation::wing_box_stiffness(config, &report.design, &plane)
                    .ok()
                    .map(|table| alas_stab::neutral_point::WingStiffness {
                        y_m: table.y_m,
                        ei_nm2: table.ei_nm2,
                        gj_nm2: table.gj_nm2,
                        box_axis_fraction: table.box_axis_fraction,
                        inertia_relief: Some(alas_stab::neutral_point::InertiaRelief {
                            carried_kg_m: table.carried_kg_m,
                            point_masses_kg: table.point_masses_kg,
                            aircraft_mass_kg: table.aircraft_mass_kg,
                        }),
                    });
            input.tail_flexibility =
                alas_mass::wing_reconciliation::tail_flexibility(config, &report.design, &plane)
                    .ok()
                    .map(|t| alas_stab::neutral_point::TailFlexibility {
                        fuselage_x_m: t.fuselage_x_m,
                        fuselage_ei_nm2: t.fuselage_ei_nm2,
                        stabilizer: alas_stab::neutral_point::WingStiffness {
                            y_m: t.stabilizer_y_m,
                            ei_nm2: t.stabilizer_ei_nm2,
                            gj_nm2: t.stabilizer_gj_nm2,
                            box_axis_fraction: t.stabilizer_axis_fraction,
                            inertia_relief: None,
                        },
                    });
            let conditions = neutral_point_conditions(&plane, &config.analysis, &input)
                .map_err(|error| format!("neutral-point conditions: {error}"))?;
            if let Ok(mut last) = LAST.lock() {
                *last = Some((key, conditions.clone()));
            }
            conditions
        }
    };
    if !conditions.cruise.is_finite() || !conditions.critical.is_finite() {
        return Err("neutral point is not finite".to_owned());
    }
    report.x_neutral_point = conditions.cruise;
    report.neutral_point_conditions = Some(conditions);
    Ok(())
}

/// The clean drag polar the preview's jet-inlet term sets the engine thrust
/// with: the report's fitted polar when it has one, else (the draft runs no
/// aerodynamic sweep) the zero-lift drag of the component build-up at the
/// cruise point and the lift-dependent factor of the transport flight-test
/// correlation `1/(pi A e) = 1.02/(pi A) + 0.0075 to 0.0090`, mid value (Obert,
/// *Aerodynamic Design of Transport Aircraft*, 2009, Fig. 40.36, p. 542).
fn preview_inlet_polar(
    report: &AnalysisReport,
    config: &AlasConfig,
) -> alas_opt::envelope::InletDragPolar {
    let fit = &report.polar_fit;
    if fit.cd0.is_finite() && fit.k.is_finite() {
        return alas_opt::envelope::InletDragPolar {
            cd0: fit.cd0,
            k: fit.k,
        };
    }
    let plane = &report.airplane;
    let aero = alas_aero::analysis::AeroAnalysis::new(
        plane,
        alas_aero::analysis::AeroAnalysis::quarter_chord_sweep_deg(plane, 25.0),
        Some(config.geometry.clone()),
        Some(config.resolved_drag_model()),
        Some(config.analysis.clone()),
    );
    let req = &config.requirements;
    let cd0 = aero.parasite_drag(req.cruise_mach, req.cruise_altitude_m, 0.0, None, None);
    let aspect_ratio = plane.b_ref * plane.b_ref / plane.s_ref;
    let k = 1.02 / (std::f64::consts::PI * aspect_ratio) + 0.008_25;
    alas_opt::envelope::InletDragPolar { cd0, k }
}
