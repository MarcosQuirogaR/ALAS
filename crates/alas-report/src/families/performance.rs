// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py and alas/sidecar/figures_extra.py
// Reference: alas @ rust-port-baseline.

//! Flight envelope V-n diagrams, payload-range curves, matching charts, and field performance.

mod envelope;
mod lto;
mod matching;
mod payload_range;
mod support;

pub use envelope::figure_vn_diagram;
pub use lto::{figure_lto_arrival, figure_lto_departure, figure_lto_for_airport};
pub use matching::figure_matching_chart;
pub use payload_range::figure_payload_range;
