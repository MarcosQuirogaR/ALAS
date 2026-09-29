// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Convert the mass breakdown and coordinate types to the
//! string-keyed maps [`super::AnalysisReport`] reports.

use std::collections::HashMap;

use alas_mass::breakdown::{MassBreakdown, MassCoordinates};

pub(super) fn breakdown_to_map(mb: &MassBreakdown) -> HashMap<String, f64> {
    mb.as_pairs()
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}

pub(super) fn coordinates_to_map(mc: &MassCoordinates) -> HashMap<String, [f64; 3]> {
    mc.as_pairs()
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}
