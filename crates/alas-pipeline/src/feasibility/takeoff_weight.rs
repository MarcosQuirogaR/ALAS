// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The report flag for a take-off weight the departure aerodrome limits.

use alas_config::{AlasConfig, DesignVector};

use super::{warning, FindingCode, PhysicalFinding};

/// A warning stating the climb- and obstacle-limited take-off weight when the
/// configured departure is above sea level or off the standard day and the
/// mission mass exceeds it. An operating limitation (AFM practice, CS 25.121,
/// CS 25.115, CAT.POL.A.210), not a design violation: the sea-level ISA
/// certification rows are separate hard requirements. `None` at a standard
/// sea-level departure, when nothing limits, or when the assessment fails.
///
/// Only a fixed certified baseline reports the limit
/// (`alas_opt::mdo::residuals_performance::wat_limit`): every mode that
/// designs or sizes the aircraft keeps the departure checks hard, so its
/// assessment carries no `takeoff_weight_limit` row, and the full candidate
/// assessment below (a sizing closure, with an engine sizing on a clean
/// sheet) would be spent to find none.
pub(super) fn finding(config: &AlasConfig, design: &DesignVector) -> Option<PhysicalFinding> {
    if config.optimizer.design_space.mode != alas_config::DesignMode::BaselineSandbox {
        return None;
    }
    let departure = config.departure_airport_record().ok()?;
    if departure.elevation_m.abs() < 1.0 && departure.isa_deviation_c.abs() < 1.0e-6 {
        return None;
    }
    let assessment = alas_opt::assess_product_candidate(config, design).ok()?;
    let row = assessment
        .residuals
        .iter()
        .find(|row| row.id == "takeoff_weight_limit")?;
    Some(warning(
        FindingCode::TakeoffWeightLimited,
        row.detail.clone().unwrap_or_default(),
        Some(row.actual),
        Some(row.limit),
        "kg",
    ))
}
