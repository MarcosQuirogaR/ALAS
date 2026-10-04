// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Exact identity of a completed native reporting analysis.

use super::*;

/// Native analysis, mission and feasibility reusable only for the same inputs.
#[derive(Debug, Clone, PartialEq)]
pub struct VerifiedAnalysis {
    config: AlasConfig,
    route: Option<AcceptanceRoute>,
    verification: std::sync::Arc<FinalistVerification>,
}

impl VerifiedAnalysis {
    pub(crate) fn new(
        config: AlasConfig,
        route: Option<AcceptanceRoute>,
        verification: FinalistVerification,
    ) -> Self {
        Self {
            config,
            route,
            verification: std::sync::Arc::new(verification),
        }
    }

    /// Configuration, reported aircraft and planned route must all match.
    /// The raw search vector can differ from the resolved reported aircraft.
    pub fn matches(
        &self,
        config: &AlasConfig,
        design: &DesignVector,
        route: Option<&AcceptanceRoute>,
    ) -> bool {
        self.config == *config
            && self.verification.report.design == *design
            && self.route.as_ref() == route
    }

    /// Completed native evidence; external delivery checks remain fresh.
    pub fn verification(&self) -> &FinalistVerification {
        &self.verification
    }
}
