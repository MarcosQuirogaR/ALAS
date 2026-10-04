// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The closure's dispatch outcome as Quick Analysis text and flags.

use alas_mass::dispatch::DispatchStatus;

use super::QuickFlag;

/// One line describing a dispatch status.
pub(super) fn dispatch_status_text(status: &DispatchStatus) -> String {
    match status {
        DispatchStatus::Cancelled => "cancelled; no feasibility conclusion".to_owned(),
        DispatchStatus::Converged => "converged".to_owned(),
        DispatchStatus::MtowLimited { shortfall_kg } => {
            format!("MTOW-limited by {shortfall_kg:.0} kg")
        }
        DispatchStatus::TankLimited { shortfall_kg } => {
            format!("tank-limited by {shortfall_kg:.0} kg")
        }
        DispatchStatus::NotConverged { last_change_kg } => {
            format!("not converged (last change {last_change_kg:.0} kg)")
        }
        DispatchStatus::ModelFailed(message) => format!("model failed: {message}"),
    }
}

/// The feasibility flags the closure's dispatch raises.
pub(super) fn dispatch_flags_of(sized: &alas_opt::SizedCandidate) -> Vec<QuickFlag> {
    let mut flags = Vec::new();
    match &sized.dispatch.status {
        DispatchStatus::Converged => {}
        other => flags.push(QuickFlag {
            code: "Dispatch".to_owned(),
            blocking: true,
            message: dispatch_status_text(other),
        }),
    }
    if sized.dispatch.landing_mass_exceeds_mlw {
        flags.push(QuickFlag {
            code: "LandingMassLimit".to_owned(),
            blocking: true,
            message: "destination landing mass exceeds the landing mass limit".to_owned(),
        });
    }
    if sized.dispatch.zero_fuel_mass_exceeds_mzfw {
        flags.push(QuickFlag {
            code: "ZeroFuelMassLimit".to_owned(),
            blocking: true,
            message: "zero-fuel mass exceeds the maximum zero-fuel mass".to_owned(),
        });
    }
    if !sized.sizing_closed {
        flags.push(QuickFlag {
            code: "SizingNotClosed".to_owned(),
            blocking: false,
            message: format!(
                "takeoff-mass closure stopped after {} iterations",
                sized.sizing_iterations
            ),
        });
    }
    flags
}
