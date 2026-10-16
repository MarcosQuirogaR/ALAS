// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Small persistence and validation helpers for the pipeline coordinator.
//!
//! Keeping these helpers beside, rather than inside, the coordinator leaves
//! the stage ordering readable and prevents the orchestration module from
//! becoming a second export implementation.

use std::collections::BTreeMap;
use std::path::Path;

use alas_aero::mses::{MsesPolarResult, MsesPressureResult};
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use serde_json::json;

pub(super) fn persist_mses_raw_exports(
    result: &MsesPressureResult,
    output_dir: &Path,
) -> std::io::Result<()> {
    if result.raw_bl_dump.is_empty()
        && result.raw_flowfield_dump.is_empty()
        && result.solver_attempts.is_empty()
        && result.status == alas_aero::mses::MsesStatus::NotRun
    {
        return Ok(());
    }
    let directory = output_dir.join("mses");
    std::fs::create_dir_all(&directory)?;
    if !result.raw_bl_dump.is_empty() {
        std::fs::write(directory.join("bl_dump.txt"), &result.raw_bl_dump)?;
    }
    if !result.raw_flowfield_dump.is_empty() {
        std::fs::write(directory.join("flowfield.txt"), &result.raw_flowfield_dump)?;
    }
    let attempts = result
        .solver_attempts
        .iter()
        .map(|attempt| {
            json!({
                "alpha_deg": attempt.alpha_deg,
                "purpose": attempt.purpose,
                "status": attempt.status.as_str(),
                "solver_output": attempt.solver_output,
            })
        })
        .collect::<Vec<_>>();
    let document = json!({
        "status": result.status.as_str(),
        "error": result.error,
        "alpha_deg": result.alpha_deg,
        "convergence_verified": result.has_convergence_evidence(),
        "transition_model_valid": result.transition_model_is_valid(),
        "osmap_required": result.osmap_required,
        "osmap_status": result.osmap_status.as_str(),
        "osmap_path": result.osmap_path,
        "osmap_diagnostic": result.osmap_diagnostic,
        "surface_sample_count": result.x_upper.len() + result.x_lower.len(),
        "flowfield_sample_count": result.field_x.len(),
        "solver_attempts": attempts,
    });
    let text = serde_json::to_string_pretty(&document).map_err(std::io::Error::other)?;
    std::fs::write(directory.join("pressure_diagnostics.json"), text)
}

/// Persist the exact requested alpha schedule and MSES transcript per point.
///
/// Coefficient arrays deliberately omit non-converged points. This sibling
/// artifact records their solver evidence without altering those arrays or the
/// existing raw `mplot` figure exports.
pub(super) fn persist_mses_polar_diagnostics(
    result: &MsesPolarResult,
    output_dir: &Path,
) -> std::io::Result<()> {
    let directory = output_dir.join("mses");
    if !result.point_diagnostics.is_empty() {
        std::fs::create_dir_all(&directory)?;
        write_polar_document(result, &directory.join("polar_diagnostics.json"))?;
    }
    for section in &result.span_sections {
        if section.polar.point_diagnostics.is_empty() {
            continue;
        }
        std::fs::create_dir_all(&directory)?;
        write_polar_document(
            &section.polar,
            &directory.join(format!("polar_diagnostics_{}.json", section.label)),
        )?;
    }
    Ok(())
}

fn write_polar_document(result: &MsesPolarResult, path: &Path) -> std::io::Result<()> {
    let points = result
        .point_diagnostics
        .iter()
        .enumerate()
        .map(|(index, point)| {
            json!({
                "requested_alpha_deg": point.requested_alpha_deg,
                "status": point.status.as_str(),
                "class": result.point_classes.get(index).map(|class| class.as_str()),
                "solver_output": point.solver_output,
            })
        })
        .collect::<Vec<_>>();
    let attempts = result
        .solver_attempts
        .iter()
        .map(|attempt| {
            json!({
                "alpha_deg": attempt.alpha_deg,
                "purpose": attempt.purpose,
                "status": attempt.status.as_str(),
                "solver_output": attempt.solver_output,
            })
        })
        .collect::<Vec<_>>();
    use alas_aero::mses::MsesPointClass as Class;
    let document = json!({
        "status": result.status.as_str(),
        "error": result.error,
        "airfoil_name": &result.airfoil_name,
        "mach": result.mach,
        "reynolds": result.reynolds,
        "requested_alpha_count": result.requested_alpha_count,
        "converged_alpha_count": result.converged_alpha_count,
        "design_alpha_deg": result.design_alpha_deg,
        "design_point_class": result.design_point_class().map(|class| class.as_str()),
        "outside_validity_domain_count": result.count_class(Class::OutsideValidityDomain),
        "solver_failure_count": result.count_class(Class::SolverFailure),
        "transition_model_valid": result.transition_model_is_valid(),
        "osmap_required": result.osmap_required,
        "osmap_status": result.osmap_status.as_str(),
        "osmap_path": result.osmap_path,
        "osmap_diagnostic": result.osmap_diagnostic,
        "converged_alpha_deg": &result.alpha_deg,
        "cl": &result.cl,
        "cd": &result.cd,
        "cdw": &result.cdw,
        "points": points,
        "solver_attempts": attempts,
    });
    let text = serde_json::to_string_pretty(&document).map_err(std::io::Error::other)?;
    std::fs::write(path, text)
}

pub(super) fn validate_bounds(bounds: &[(f64, f64)]) -> Result<(), String> {
    let expected = DesignVector::bounds().len();
    // The MTOW band mode adds the takeoff mass as a 17th coordinate.
    if bounds.len() != expected && bounds.len() != expected + 1 {
        return Err(format!(
            "design space has {} bound pairs; expected {expected}",
            bounds.len()
        ));
    }
    for (index, &(lower, upper)) in bounds.iter().enumerate() {
        if !lower.is_finite() || !upper.is_finite() || lower > upper {
            return Err(format!(
                "design-space bound {index} is invalid: [{lower}, {upper}]"
            ));
        }
    }
    Ok(())
}

pub(super) fn add_manifest_artifact(
    artifacts: &mut BTreeMap<String, String>,
    output_dir: &Path,
    name: &str,
    path: &Path,
) {
    let value = path
        .strip_prefix(output_dir)
        .unwrap_or(path)
        .display()
        .to_string();
    artifacts.insert(name.to_owned(), value);
}

pub(super) fn add_manifest_artifact_if_exists(
    artifacts: &mut BTreeMap<String, String>,
    output_dir: &Path,
    name: &str,
    path: &Path,
) {
    if path.is_file() {
        add_manifest_artifact(artifacts, output_dir, name, path);
    }
}

/// The dispatch-time preset barrier of clarified App Features 1.2: a run on
/// a registered preset must name a registered aircraft, and in preset mode
/// its locked geometry, initial design point and search bounds must match
/// the registry's values and the design-space envelope anchored there. The guided
/// workspace restores these after every edit; this check is the second
/// barrier for buffers that reached the pipeline by another route.
pub(super) fn check_preset_policy(
    config: &AlasConfig,
    initial_design: Option<&DesignVector>,
    bounds: Option<&[(f64, f64)]>,
) -> Result<(), String> {
    if !config.preset.is_empty() {
        alas_config::presets::get(&config.preset).map_err(|error| {
            format!(
                "configuration preset identity is not registered: {error}; clear the preset field or select a registered aircraft preset"
            )
        })?;
    }
    let violations =
        alas_config::preset_policy::dispatch_violations(config, initial_design, bounds)
            .map_err(|error| error.to_string())?;
    if violations.is_empty() {
        return Ok(());
    }
    Err(format!(
        "preset '{}' is protected in preset mode and the run does not match its registered definition: {}; restore the preset values or use the sandbox for geometry experiments",
        config.preset,
        alas_config::preset_policy::describe_violations(&violations)
    ))
}

/// `lo..hi` over the finite entries of `values` at `decimals` places, or
/// `none` when there are none.
pub(super) fn finite_range_text(values: &[f64], decimals: usize) -> String {
    let mut finite = values.iter().copied().filter(|value| value.is_finite());
    let Some(first) = finite.next() else {
        return "none".to_owned();
    };
    let (lo, hi) = finite.fold((first, first), |(lo, hi), value| {
        (lo.min(value), hi.max(value))
    });
    format!("{lo:.decimals$}..{hi:.decimals$}")
}

/// Carry the reference wing of `source` onto the CPACS-canonicalised
/// `canonical` aircraft.
///
/// A CPACS document holds the planform and the reference length and area but
/// not where the reference MAC starts, so the aircraft read back from it
/// measures `%MAC` from the drawn wing's own MAC leading edge. The delivered
/// report was balanced, trimmed and gated against `source`, whose published
/// reference wing (`mac_le_x_m`) is the `%MAC` datum of the registered
/// aircraft; without this the same centre of gravity reads several percent MAC
/// away between the search and the delivered report (A220-300: 0.160 m of
/// leading edge, 4.2 percent MAC). The main wing is matched by position: the
/// canonical document resolves the largest projected wing to `wings[0]`, as the
/// builder does.
pub(super) fn carry_reference_wing(canonical: &mut Airplane, source: &Airplane) {
    let reference = source.main_wing().and_then(|wing| wing.reference);
    if let (Some(reference), Some(wing)) = (reference, canonical.wings.first_mut()) {
        wing.reference = Some(reference);
    }
}

// A failed expect in a test is the assertion failing on a registered preset.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod reference_wing_tests {
    use super::*;
    use alas_geom::builder::AircraftBuilder;

    #[test]
    fn a_canonicalised_aircraft_keeps_the_registered_percent_mac_datum() {
        let preset = alas_config::presets::get("A220-300").expect("registered A220");
        let source = AircraftBuilder::new(Some(preset.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .expect("A220 geometry");
        let datum = source.mac_frame().expect("main wing").x_lemac_m;
        // The CPACS read-back: same planform, no reference wing.
        let mut canonical = source.clone();
        canonical.wings[0].reference = None;
        let drawn = canonical.mac_frame().expect("main wing").x_lemac_m;
        assert!(
            (datum - drawn).abs() > 0.1,
            "the A220 drawn MAC leading edge {drawn} m is the published {datum} m"
        );
        carry_reference_wing(&mut canonical, &source);
        assert_eq!(canonical.mac_frame(), source.mac_frame());
    }
}
