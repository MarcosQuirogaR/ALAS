// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The clean cruise drag model the mission reads.
//!
//! [`CruiseDrag`] is the seam between the mission and the polar. The
//! optimizer flies every native candidate on its [`TrimmedDragTable`]; a
//! polar supplied by an external solver is flown as the [`ParabolicPolar`]
//! it describes.

use std::fmt;
use std::sync::Arc;

use crate::mdo::drag_table::{DragTableOutOfRange, TrimmedDragTable, MACH_MIN};
use crate::mdo::types::ExternalPolar;

/// Clean, trimmed drag coefficient of the aircraft.
pub trait CruiseDrag: Send + Sync {
    /// Drag coefficient at lift coefficient `cl`, Mach `mach` and pressure
    /// altitude `altitude_m` (m), dimensionless, on the model's reference
    /// area.
    fn cd(&self, cl: f64, mach: f64, altitude_m: f64) -> f64;

    /// The same polar for a high-lift (take-off, landing) configuration
    /// before its configuration drag increment: without the lift-dependent
    /// viscous term, which is a clean-polar term (Torenbeek 2013, p. 96:
    /// Eq. 4.21 and the e = 0.75 to 0.85 range are stated for the
    /// en-route configuration). Equal to [`Self::cd`] for a polar that has
    /// no such term.
    /// `flap_lift_cl` is the lift the devices add at constant angle of attack
    /// ([`crate::mdo::drag_table::high_lift_lift_shift`]).
    fn cd_high_lift_base(&self, cl: f64, mach: f64, altitude_m: f64, flap_lift_cl: f64) -> f64 {
        let _ = flap_lift_cl;
        self.cd(cl, mach, altitude_m)
    }

    /// Lift coefficient of minimum drag-to-lift ratio at `mach` and
    /// `altitude_m`: the holding (endurance) point of a jet.
    fn min_drag_cl(&self, mach: f64, altitude_m: f64) -> f64;

    /// How `(cl, mach)` lies outside the range the model was solved on, when
    /// it does: [`Self::cd`] is then an extrapolation. `None` for a query
    /// inside the range and for a closed-form polar, which has no solved
    /// range.
    fn out_of_range(&self, cl: f64, mach: f64) -> Option<DragTableOutOfRange> {
        let _ = (cl, mach);
        None
    }
}

/// `CD = CD0 + k CL^2 + CD_wave(M)`, the wave term a quadratic ramp from
/// `onset_mach` to `wave_cd` at `cruise_mach`.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct ParabolicPolar {
    /// Zero-lift drag coefficient.
    pub cd0: f64,
    /// Induced-drag factor `k`.
    pub induced_factor_k: f64,
    /// Wave drag coefficient at `cruise_mach`.
    pub wave_cd: f64,
    /// Mach the wave term is anchored at.
    pub cruise_mach: f64,
    /// Mach below which no wave drag is credited.
    pub onset_mach: f64,
}

impl ParabolicPolar {
    /// The wave term at `mach`.
    pub fn wave_cd_at(&self, mach: f64) -> f64 {
        if self.wave_cd <= 0.0 || mach <= self.onset_mach || self.cruise_mach <= self.onset_mach {
            return 0.0;
        }
        let ratio = (mach - self.onset_mach) / (self.cruise_mach - self.onset_mach);
        self.wave_cd * ratio.max(0.0).powi(2)
    }
}

impl ParabolicPolar {
    /// `cd0 + k CL^2` with the wave term `wave_cd` at `cruise_mach`, ramped
    /// from the mission model's wave onset Mach.
    pub fn new(cd0: f64, induced_factor_k: f64, wave_cd: f64, cruise_mach: f64) -> Self {
        Self {
            cd0,
            induced_factor_k,
            wave_cd,
            cruise_mach,
            onset_mach: super::WAVE_ONSET_MACH,
        }
    }

    /// The polar an external solver measured at its cruise point, its wave
    /// term ramped from the mission model's onset Mach to `polar.mach`.
    pub(crate) fn from_external(polar: &ExternalPolar) -> Self {
        Self::new(
            polar.cd0,
            polar.induced_factor_k,
            polar.wave_drag_cd,
            polar.mach,
        )
    }
}

impl CruiseDrag for TrimmedDragTable {
    fn cd(&self, cl: f64, mach: f64, altitude_m: f64) -> f64 {
        TrimmedDragTable::cd(self, cl, mach, altitude_m)
    }

    fn cd_high_lift_base(&self, cl: f64, mach: f64, altitude_m: f64, flap_lift_cl: f64) -> f64 {
        TrimmedDragTable::cd_without_lift_viscous(self, cl, mach, altitude_m, flap_lift_cl)
    }

    /// A Mach below the table's lowest node, such as the zero the holding
    /// leg passes for "low speed", carries no Reynolds number for the skin
    /// friction (`CD0 -> 0` as `M -> 0`), so the search is made at the
    /// lowest node, [`MACH_MIN`]. The holding optimum is flat in the
    /// Reynolds number: its fuel flow is then evaluated at the actual
    /// holding speed.
    fn min_drag_cl(&self, mach: f64, altitude_m: f64) -> f64 {
        TrimmedDragTable::min_drag_cl(self, mach.max(MACH_MIN), altitude_m)
    }

    fn out_of_range(&self, cl: f64, mach: f64) -> Option<DragTableOutOfRange> {
        TrimmedDragTable::out_of_range(self, cl, mach)
    }
}

impl CruiseDrag for ParabolicPolar {
    fn cd(&self, cl: f64, mach: f64, _altitude_m: f64) -> f64 {
        self.cd0 + self.induced_factor_k * cl * cl + self.wave_cd_at(mach)
    }

    fn min_drag_cl(&self, _mach: f64, _altitude_m: f64) -> f64 {
        // Holding speeds sit below the wave onset, where the polar is
        // parabolic and D/L is least at CL = sqrt(CD0 / k).
        (self.cd0 / self.induced_factor_k).sqrt()
    }
}

/// Shared handle on a drag model; equal only to itself.
#[derive(Clone)]
pub(crate) struct DragHandle(pub(crate) Arc<dyn CruiseDrag>);

impl fmt::Debug for DragHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DragHandle")
    }
}

impl PartialEq for DragHandle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl super::SegmentMissionModel {
    /// Fly against `drag` instead of the model's current drag.
    pub fn with_cruise_drag(mut self, drag: Arc<dyn CruiseDrag>) -> Self {
        self.cruise_drag = DragHandle(drag);
        self
    }

    /// Clean drag coefficient from the model's drag.
    pub(super) fn clean_cd(&self, cl: f64, mach: f64, altitude_m: f64) -> f64 {
        self.cruise_drag.0.cd(cl, mach, altitude_m)
    }

    /// Drag coefficient of a high-lift configuration before its increment:
    /// the clean polar without the lift-dependent viscous term, its devices
    /// adding `flap_lift_cl` at constant angle of attack.
    pub(super) fn high_lift_base_cd(
        &self,
        cl: f64,
        mach: f64,
        altitude_m: f64,
        flap_lift_cl: f64,
    ) -> f64 {
        self.cruise_drag
            .0
            .cd_high_lift_base(cl, mach, altitude_m, flap_lift_cl)
    }

    /// Minimum drag-to-lift lift coefficient from the model's drag.
    pub(super) fn min_drag_cl(&self, mach: f64, altitude_m: f64) -> f64 {
        self.cruise_drag.0.min_drag_cl(mach, altitude_m)
    }
}

impl super::PhaseAeroLimits {
    /// Lift the take-off configuration adds at constant angle of attack.
    pub(super) fn takeoff_lift_shift(&self) -> f64 {
        crate::mdo::drag_table::high_lift_lift_shift(self.takeoff_cl_max, self.clean_cl_max)
    }

    /// Lift the landing configuration adds at constant angle of attack.
    pub(super) fn landing_lift_shift(&self) -> f64 {
        crate::mdo::drag_table::high_lift_lift_shift(self.landing_cl_max, self.clean_cl_max)
    }
}
