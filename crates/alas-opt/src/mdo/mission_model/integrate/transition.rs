// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The kinetic-energy budget a speed change at a segment boundary opens.

use alas_mass::fuel_plan::FuelModelError;

use super::{
    FlyError, Integrator, BOUNDARY_KE_ABSOLUTE_TOLERANCE_J, BOUNDARY_KE_RELATIVE_TOLERANCE,
};

impl Integrator<'_> {
    /// Open the kinetic budget for a transition to `tas_m_s`.
    ///
    /// A boundary is an instantaneous profile change in this reduced model.
    /// A material unpaid budget may continue while the same target is split
    /// across segments, but it must close before a distinct target is opened.
    /// Otherwise the schedule could report an intermediate speed as flown
    /// even though the aircraft never attained it.
    pub(in crate::mdo::mission_model) fn transition_tolerance_j(
        &self,
        previous: f64,
        target: f64,
    ) -> f64 {
        let speed_scale = previous.abs().max(target.abs());
        let kinetic_scale = 0.5 * self.mass_kg * speed_scale * speed_scale;
        BOUNDARY_KE_ABSOLUTE_TOLERANCE_J.max(BOUNDARY_KE_RELATIVE_TOLERANCE * kinetic_scale)
    }

    pub(in crate::mdo::mission_model) fn target_is_distinct(
        &self,
        previous: f64,
        target: f64,
    ) -> bool {
        let tolerance_j = self.transition_tolerance_j(previous, target);
        (target - previous).abs() > (2.0 * tolerance_j / self.mass_kg.max(f64::MIN_POSITIVE)).sqrt()
    }

    pub(in crate::mdo::mission_model) fn open_transition(
        &mut self,
        tas_m_s: f64,
    ) -> Result<(), FlyError> {
        if let Some(previous) = self.previous_tas_m_s {
            let tolerance_j = self.transition_tolerance_j(previous, tas_m_s);
            let previous_pending_j = self.pending_kinetic_j;
            let target_is_distinct = self.target_is_distinct(previous, tas_m_s);
            if previous_pending_j.abs() > tolerance_j && target_is_distinct {
                return Err(FuelModelError::NotConverged(format!(
                    "speed schedule not attained before next distinct transition from {previous:.3} to {tas_m_s:.3} m/s: {pending:.3} J of kinetic energy remained unpaid (boundary tolerance {tolerance_j:.3} J)",
                    pending = previous_pending_j,
                ))
                .into());
            }
            let previous_kinetic_j = 0.5 * self.mass_kg * previous * previous;
            // `pending_kinetic_j` is target KE minus the KE the deck has
            // actually delivered. Reconstruct that delivered state before
            // changing targets; adding the new target delta to the old
            // budget would let two target changes cancel without ever
            // reaching the intermediate speed.
            let actual_kinetic_j = if previous_pending_j.abs() <= tolerance_j {
                previous_kinetic_j
            } else {
                previous_kinetic_j - previous_pending_j
            };
            if !actual_kinetic_j.is_finite() || actual_kinetic_j < -tolerance_j {
                return Err(FuelModelError::NotConverged(format!(
                    "nonphysical kinetic state before transition from {previous:.3} m/s: {actual_kinetic_j:.3} J"
                ))
                .into());
            }
            let actual_kinetic_j = actual_kinetic_j.max(0.0);
            let target_kinetic_j = 0.5 * self.mass_kg * tas_m_s * tas_m_s;
            let target_pending_j = target_kinetic_j - actual_kinetic_j;
            // Drop only a round-off residue. A material same-direction
            // budget must remain open so later steps can pay it; clearing it
            // here would recreate the boundary-energy cancellation bug.
            self.pending_kinetic_j = if target_pending_j.abs() <= tolerance_j {
                0.0
            } else {
                target_pending_j
            };
        }
        self.previous_tas_m_s = Some(tas_m_s);
        Ok(())
    }
}
