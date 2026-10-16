// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Which nacelle geometry the builder lofts for an engine installation.

use serde::{Deserialize, Serialize};

pub(super) fn is_legacy_nacelle_model(value: &NacelleModel) -> bool {
    *value == NacelleModel::Legacy
}

/// Which nacelle geometry the builder lofts for an installation.
///
/// [`Self::V2`] asks for the G2 revolve of the catalogue `nacelle_shape_v2`
/// (and the drawn centre-engine pod). It applies only where that data exist
/// and the installation still carries the catalogue silhouette and radius; if
/// the revolve is rejected the builder falls back to the legacy polyline and
/// records a `nacelle_v2_fallback` warning in its report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NacelleModel {
    /// The circular-section polyline of `nacelle_profile`. The historical
    /// behaviour.
    #[default]
    Legacy,
    /// The G2 revolve of the catalogue nacelle shape.
    V2,
}
