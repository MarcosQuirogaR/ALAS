// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The exposed inboard aerodynamic station of a [`WingConfig`].

use super::config::WingConfig;
use super::planform::{InboardAerodynamicStation, TransportPlanformError};
use crate::DesignVector;

impl WingConfig {
    /// Return the exposed inboard section for a 2-D aerodynamic calculation.
    ///
    /// # Errors
    ///
    /// Returns [`TransportPlanformError`] when the parent planform is not
    /// physically constructible.
    pub fn inboard_aerodynamic_station(
        &self,
        design: &DesignVector,
    ) -> Result<InboardAerodynamicStation, TransportPlanformError> {
        let planform = self.transport_planform(design)?;
        if let Some(side_of_body) = planform.side_of_body {
            let root_to_kink_fraction = side_of_body.y_m / planform.kink.y_m;
            return Ok(InboardAerodynamicStation {
                y_m: side_of_body.y_m,
                leading_edge_x_m: side_of_body.leading_edge_x_m,
                chord_m: side_of_body.chord_m,
                twist_deg: self.root_twist_deg
                    + root_to_kink_fraction * (self.break_twist_deg - self.root_twist_deg),
            });
        }
        Ok(InboardAerodynamicStation {
            y_m: planform.root.y_m,
            leading_edge_x_m: planform.root.leading_edge_x_m,
            chord_m: planform.root.chord_m,
            twist_deg: self.root_twist_deg,
        })
    }
}
