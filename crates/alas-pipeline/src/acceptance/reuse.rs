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
    /// The vector the analysis was requested for, when it differs from the
    /// resolved aircraft it reports (a clean sheet's start re-derives its
    /// fuselage from the cabin).
    requested: Option<DesignVector>,
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
            requested: None,
        }
    }

    /// [`Self::new`] for an analysis requested for `requested`, which then
    /// matches that vector as well as the resolved aircraft it reports.
    pub(crate) fn of_request(
        config: AlasConfig,
        route: Option<AcceptanceRoute>,
        verification: FinalistVerification,
        requested: DesignVector,
    ) -> Self {
        Self {
            requested: Some(requested),
            ..Self::new(config, route, verification)
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
            && (self.verification.report.design == *design
                || self.requested.as_ref() == Some(design))
            && self.route.as_ref() == route
    }

    /// Completed native evidence; external delivery checks remain fresh.
    pub fn verification(&self) -> &FinalistVerification {
        &self.verification
    }
}
