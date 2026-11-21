// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Where the built engines stand, for the FLOPS `FNEW` and `FNEF` counts.
//!
//! The installation fields say it: an engine on the centreline cannot hang
//! from a wing, and `aft_fuselage` puts every off-centreline engine on the
//! rear fuselage. A declared pair is kept when it agrees with them. A pair
//! that does not (the two-engine study default under a four-engine brief, or
//! a sandbox edit that adds an engine) describes engines that were not built,
//! so the installation decides. A pair that was never declared stays missing:
//! that is the FLOPS contract's own blocker, not a disagreement.

use super::EngineConfig;

impl EngineConfig {
    /// The engines the installation itself puts on the fuselage or empennage.
    pub fn installed_fuselage_engine_count(&self) -> usize {
        let positions = &self.spanwise_positions_m;
        if self.aft_fuselage.is_some() {
            return positions.len();
        }
        positions.iter().filter(|y| **y == 0.0).count()
    }

    /// `(wing, fuselage)` engine counts of the built installation, from the
    /// declared pair; `None` when a count is undeclared or no engine is built.
    pub fn resolved_engine_mounting(
        &self,
        declared_wing: Option<usize>,
        declared_fuselage: Option<usize>,
    ) -> Option<(usize, usize)> {
        let built = self.spanwise_positions_m.len();
        let (wing, fuselage) = (declared_wing?, declared_fuselage?);
        if built == 0 {
            return None;
        }
        let installed_fuselage = self.installed_fuselage_engine_count();
        if wing.checked_add(fuselage) == Some(built) && fuselage >= installed_fuselage {
            return Some((wing, fuselage));
        }
        Some((built - installed_fuselage, installed_fuselage))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::{AftFuselageEngineInstallation, EngineConfig};

    fn engine(positions: &[f64]) -> EngineConfig {
        EngineConfig {
            spanwise_positions_m: positions.to_vec(),
            ..EngineConfig::default()
        }
    }

    #[test]
    fn a_declared_pair_that_agrees_with_the_installation_is_kept() {
        let trijet = engine(&[8.18, -8.18, 0.0]);
        assert_eq!(
            trijet.resolved_engine_mounting(Some(2), Some(1)),
            Some((2, 1))
        );
    }

    #[test]
    fn the_installation_decides_when_the_pair_describes_other_engines() {
        let quad = engine(&[20.0, 11.0, -11.0, -20.0]);
        assert_eq!(
            quad.resolved_engine_mounting(Some(2), Some(0)),
            Some((4, 0))
        );
        let trijet = engine(&[9.0, -9.0, 0.0]);
        assert_eq!(
            trijet.resolved_engine_mounting(Some(2), Some(0)),
            Some((2, 1))
        );
    }

    #[test]
    fn rear_fuselage_engines_are_never_wing_mounted() {
        let mut rear = engine(&[1.32, -1.32]);
        rear.aft_fuselage = Some(AftFuselageEngineInstallation {
            inlet_ahead_of_tail_m: 7.0,
            axis_z_m: 0.7,
        });
        assert_eq!(
            rear.resolved_engine_mounting(Some(2), Some(0)),
            Some((0, 2))
        );
    }

    #[test]
    fn an_undeclared_pair_stays_missing() {
        let twin = engine(&[9.8, -9.8]);
        assert_eq!(twin.resolved_engine_mounting(None, Some(0)), None);
        assert_eq!(twin.resolved_engine_mounting(Some(2), None), None);
        assert_eq!(engine(&[]).resolved_engine_mounting(Some(0), Some(0)), None);
    }
}
