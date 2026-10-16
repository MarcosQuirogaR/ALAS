// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What a build did that the geometry alone does not show.

/// One thing the builder substituted, ignored or tolerated.
#[derive(Debug, Clone, PartialEq)]
pub struct BuildWarning {
    /// Stable machine-readable code.
    pub code: &'static str,
    /// Human-readable explanation.
    pub message: String,
}

/// Figures of a lofted fuselage build.
#[derive(Debug, Clone, PartialEq)]
pub struct LoftBuildInfo {
    /// Id of the embedded loft that was resolved.
    pub loft_id: String,
    /// Built length over the loft's reference length.
    pub length_ratio: f64,
    /// Stations of the legacy polyline view.
    pub stations: usize,
    /// Largest distance of that polyline from the exact crown, keel and
    /// plan half-width, m.
    pub polyline_deviation_m: f64,
    /// Widest section of the loft, m.
    pub max_width_m: f64,
    /// Tallest section of the loft (crown minus keel), m.
    pub max_height_m: f64,
}

/// The builder's account of one build: substitutions, ignored inputs and the
/// figures of the loft path. Warnings are also emitted through `tracing`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BuildReport {
    /// Everything the builder did not do silently.
    pub warnings: Vec<BuildWarning>,
    /// Set when the fuselage was built from a loft.
    pub loft: Option<LoftBuildInfo>,
}

impl BuildReport {
    /// Whether a warning with this code was recorded.
    #[must_use]
    pub fn has_warning(&self, code: &str) -> bool {
        self.warnings.iter().any(|w| w.code == code)
    }

    /// Record a warning (once per distinct message) and log it.
    pub(super) fn warn(&mut self, code: &'static str, message: String) {
        if self
            .warnings
            .iter()
            .any(|w| w.code == code && w.message == message)
        {
            return;
        }
        tracing::warn!(code, "{message}");
        self.warnings.push(BuildWarning { code, message });
    }
}
