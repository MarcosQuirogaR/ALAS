// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (figure_structures_stress,
// _structures_unavailable_message, figure_status_message, plt_cm_tab10).
// Reference: alas @ rust-port-baseline.

//! Structural stress-margin figures that require completed analytical results.

mod layout;
mod modes;
mod patran;
mod status;
mod stress;
mod vibration;

pub use modes::{figure_structures_modes, nearest_frequency_index};
pub use patran::figure_structures_patran;
pub use stress::figure_structures_stress;
pub use vibration::figure_structures_vibration;
