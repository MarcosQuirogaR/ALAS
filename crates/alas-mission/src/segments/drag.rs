// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Shared candidate drag boundary and the explicit frozen parity alternative.

use std::sync::Arc;

use alas_aero::drag_buildup::{ComponentParasiteDrag, DragBreakdown};

/// Whole-aircraft drag coefficients on the mission reference area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MissionDragCoefficients {
    /// Parasite drag, including the candidate buildup's declared margins.
    pub parasite: f64,
    /// Induced drag of the trimmed aircraft, including tail trim drag.
    pub induced: f64,
    /// Wave drag of the same aircraft and flight condition.
    pub wave: f64,
}

/// The candidate drag model supplied by the owning sizing pipeline.
///
/// Reynolds number is per metre; each component uses its own reference
/// length. Keeping this boundary independent of the optimizer avoids a
/// dependency cycle while allowing the mission to share its exact table.
pub trait MissionDragModel: Send + Sync {
    /// Evaluate clean trimmed drag at the aircraft lift coefficient.
    fn coefficients(
        &self,
        lift_coefficient: f64,
        mach: f64,
        reynolds_number_per_m: f64,
    ) -> MissionDragCoefficients;
}

/// An explicit drag source; there is no default parity fallback.
pub enum MissionDragSource {
    /// The same candidate table or externally measured polar used for sizing.
    SharedCandidate(Arc<dyn MissionDragModel>),
    /// SUAVE Fidelity_Zero retained only to reproduce frozen parity fixtures.
    FrozenSuaveParity,
}

impl MissionDragCoefficients {
    /// Project the shared whole-aircraft terms onto the historical telemetry
    /// schema. Per-component parity diagnostics are absent: the candidate
    /// table already includes trim and its own parasite allowances.
    pub(crate) fn breakdown(self) -> DragBreakdown {
        let total = self.parasite + self.induced + self.wave;
        DragBreakdown {
            parasite_wings: Vec::new(),
            parasite_fuselages: Vec::new(),
            parasite_nacelles: Vec::new(),
            parasite_pylon: ComponentParasiteDrag {
                parasite_drag_coefficient: 0.0,
                skin_friction_coefficient: 0.0,
                form_factor: 0.0,
                compressibility_factor: 0.0,
                reynolds_factor: 0.0,
            },
            parasite_total: self.parasite,
            induced_total: self.induced,
            induced_viscous: 0.0,
            induced_viscous_wings: Vec::new(),
            compressible_wings: Vec::new(),
            compressible_total: self.wave,
            miscellaneous_total_wetted_area_m2: 0.0,
            miscellaneous_total: 0.0,
            untrimmed: total,
            trim_corrected: total,
            spoiler: 0.0,
            total,
        }
    }
}
