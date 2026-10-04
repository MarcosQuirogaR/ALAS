// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The full multi-condition neutral-point set, carried alongside a report's
//! existing fixed-condition `x_neutral_point` rather than replacing it.

use alas_config::analysis::AnalysisConfig;
use alas_config::requirements::DesignRequirements;
use alas_geom::aircraft::airplane::Airplane;
use alas_stab::neutral_point::{
    neutral_point_conditions, NeutralPointConditions, NpConditionsInput,
};

use super::FullAnalysis;

impl FullAnalysis {
    /// Clean low-speed/cruise, estimated high-lift and elastic-band
    /// neutral-point conditions, at the requirements' own cruise
    /// Mach/altitude. `None` on the frozen `reference_compatibility` path
    /// or a failed VLM probe.
    pub(crate) fn np_conditions_for(
        &self,
        plane: &Airplane,
        fine_analysis: &AnalysisConfig,
        req: &DesignRequirements,
    ) -> Option<NeutralPointConditions> {
        if self.reference_compatibility {
            return None;
        }
        let np_input = NpConditionsInput {
            low_speed_altitude_m: 0.0,
            cruise_mach: req.cruise_mach,
            cruise_altitude_m: req.cruise_altitude_m,
            ..NpConditionsInput::default()
        };
        neutral_point_conditions(plane, fine_analysis, &np_input).ok()
    }
}
