// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Stable labels for UAV feasibility findings shown in result diagnostics.

use alas_uav::FindingKind;

pub(super) fn finding_label(kind: FindingKind) -> &'static str {
    match kind {
        FindingKind::InvalidInput => "Invalid input",
        FindingKind::MissingData => "Missing evidence",
        FindingKind::CellCountMismatch => "Cell-count mismatch",
        FindingKind::BatteryCurrentOverload => "Battery current overload",
        FindingKind::EscCurrentOverload => "Speed-controller current overload",
        FindingKind::MotorCurrentOverload => "Motor current overload",
        FindingKind::MotorPowerOverload => "Motor power overload",
        FindingKind::BecCurrentOverload => "BEC current overload",
        FindingKind::ServoVoltageMismatch => "Servo voltage mismatch",
        FindingKind::ControlVoltageMismatch => "Control voltage mismatch",
        FindingKind::ServoTorqueOverload => "Servo torque overload",
        FindingKind::PropellerCompatibilityUnverified => "Propeller compatibility unverified",
        FindingKind::PackagingViolation => "Packaging violation",
        FindingKind::CenterOfGravityViolation => "Center-of-gravity violation",
        FindingKind::InsufficientLift => "Insufficient lift",
        FindingKind::InsufficientThrust => "Insufficient thrust",
        FindingKind::EnergyShortfall => "Energy shortfall",
        FindingKind::EfficiencyShortfall => "Efficiency shortfall",
        FindingKind::StructuralOverload => "Structural overload",
        FindingKind::LandingGearOverload => "Landing-gear overload",
    }
}
