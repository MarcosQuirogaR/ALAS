// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Binding an optimizer finalist's report to its mission-sized takeoff mass.

use std::sync::atomic::AtomicBool;
use std::time::Instant;

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;

use super::emit_diagnostic;
use crate::full_analysis::{AnalysisReport, FullAnalysis};
use crate::runs::RunEvent;

/// Replay the typed finalist assessment and rebuild the report at its closed
/// takeoff mass.
///
/// Returns the vector the assessment was evaluated on together with the report
/// built on it. Callers must publish that vector, not the one they supplied:
/// feasibility, payload layout and the result all read the aircraft this gate
/// passed.
pub(super) fn bind_sized_finalist(
    config: &AlasConfig,
    full: &FullAnalysis,
    supplied_design: DesignVector,
    events: Option<&(dyn Fn(RunEvent) + Sync)>,
    run_clock: Instant,
    cancel: Option<&AtomicBool>,
) -> Result<(DesignVector, AnalysisReport), String> {
    // The product optimizer's objective closes the mass/dispatch
    // fixed point below the configured MTOW limit.  A branch report
    // built directly at `requirements.mtow_kg` would consequently
    // calculate cruise lift, trim and component fuel for a heavier
    // aircraft than the one that actually won the search.  Replay
    // the typed finalist assessment and bind the report to its
    // closed takeoff mass before any export or downstream tool sees
    // it.  A disagreement is a real integration error, not a reason
    // to silently fall back to the ceiling-mass report.
    let assessment = alas_opt::assess_product_candidate_cancellable(
        config,
        &supplied_design,
        cancel,
    )
    .map_err(|error| {
        if error == "cancelled" {
            "Cancelled safely: finalist replay interrupted".to_owned()
        } else {
            format!("optimized finalist could not be re-evaluated at its exported design: {error}")
        }
    })?;
    if !assessment.hard_feasible {
        let violations = assessment.violated_hard_ids().join(", ");
        return Err(format!(
            "optimized finalist is not hard-feasible on replay: {}",
            if violations.is_empty() {
                "unidentified hard residual".to_owned()
            } else {
                violations
            }
        ));
    }
    // Bind the report to the vector the assessment was *evaluated*
    // on, not the one it was handed. A clean-sheet design space
    // derives the fuselage coordinate from the cabin load case, so
    // the two are the same vector for an optimizer finalist and can
    // differ for any other supplied design (see
    // `alas_opt::ResolvedProductState::design`). Reporting the
    // caller's vector there would publish a different aeroplane from
    // the one this gate just passed.
    let assessed_design = assessment.resolved.design;
    if assessed_design != supplied_design {
        emit_diagnostic(
            events,
            run_clock,
            "full_analysis",
            &format!(
                "Finalist geometry re-derived by the design space: fuselage length {:.6} m evaluated against {:.6} m supplied; the report is bound to the evaluated aircraft",
                assessed_design.fuselage_length_m, supplied_design.fuselage_length_m,
            ),
        );
    }
    let report =
        full.run_at_sized_takeoff_mass(&assessed_design, true, assessment.sized.takeoff_mass_kg)?;
    emit_diagnostic(
        events,
        run_clock,
        "full_analysis",
        &format!(
            "Finalist report bound to mission-sized takeoff mass {:.3} kg (MTOW limit {:.3} kg)",
            assessment.sized.takeoff_mass_kg, config.requirements.mtow_kg,
        ),
    );
    Ok((assessed_design, report))
}
