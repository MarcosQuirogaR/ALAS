// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Boundary-condition choices for the OpenFOAM airfoil template.

use serde::{Deserialize, Serialize};

use super::config::FarFieldCondition;

/// Routine boundary-condition choices exposed by the CFD window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BoundarySettings {
    /// Inlet/freestream patch name from the versioned template.
    pub inlet_patch: String,
    /// Outlet patch name from the versioned template.
    pub outlet_patch: String,
    /// Airfoil wall patch name from the generated STL.
    pub airfoil_patch: String,
    /// Far-field treatment.
    pub far_field: FarFieldCondition,
    /// Treatment of `k` and `omega` on the outer `farField` patch when
    /// [`Self::far_field`] is [`FarFieldCondition::FixedValue`].
    ///
    /// `fixedValue` clamps the turbulence quantities on **every** outer face,
    /// including faces the flow is leaving.  Imposing a value on an outgoing
    /// characteristic is an over-specification, and it is measurable: at
    /// `y = +/-10 c` the free-stream turbulence has decayed several orders below
    /// the imposed value, the clamp fights the decayed interior, and those
    /// cells limit-cycle for the whole run (an internal CFD convergence
    /// study, 2026-09-16, `q17-field-change.py` on case
    /// `T1-gradfree-turbupwind`).  `inletOutlet` is `fixedValue` wherever the
    /// flow enters and `zeroGradient` wherever it leaves, so it imposes the
    /// same free-stream state on inflow and simply convects the interior value
    /// out on outflow.  It removes the over-specification; it does not change
    /// the model, the free-stream state, or any tolerance.
    #[serde(default)]
    pub far_field_turbulence: FarFieldTurbulenceCondition,
    /// Treatment of `U` on the outer `farField` patch when [`Self::far_field`]
    /// is [`FarFieldCondition::FixedValue`].
    ///
    /// The same over-specification argument as
    /// [`Self::far_field_turbulence`] applies to momentum, and at incidence it
    /// applies to the whole upper face: at `alpha = 4.04 deg` the free stream
    /// carries `v = +3.596 m/s`, so every face of the `y = +10 c` boundary is an
    /// outflow with a Dirichlet velocity imposed on it.  Clamping it to a
    /// uniform stream also suppresses the circulation-induced far field, which
    /// is a candidate for the lift excess against experiment.  `freestream` is
    /// OpenFOAM's `inletOutlet` for velocity: the free-stream vector on inflow,
    /// zero gradient on outflow.
    ///
    /// This is kept separate from [`FarFieldCondition::Freestream`], which also
    /// switches the pressure to `freestreamPressure` on every outer patch and
    /// was measured to break the mass-balance criterion outright (internal
    /// CFD study, 2026-09-16, case `E15`: continuity `3.87e-4`
    /// against a `1e-5` gate).  Here the pressure treatment is untouched, so
    /// the pressure level stays anchored by the outlet.
    #[serde(default)]
    pub far_field_velocity: FarFieldVelocityCondition,
    /// Static pressure reference in Pa. The incompressible `p` field stores
    /// kinematic pressure, so the dictionary value is `p_ref / rho`.
    pub pressure_reference_pa: f64,
}

impl Default for BoundarySettings {
    fn default() -> Self {
        Self {
            inlet_patch: "inlet".to_owned(),
            outlet_patch: "outlet".to_owned(),
            airfoil_patch: "airfoil".to_owned(),
            far_field: FarFieldCondition::FixedValue,
            far_field_turbulence: FarFieldTurbulenceCondition::default(),
            far_field_velocity: FarFieldVelocityCondition::default(),
            pressure_reference_pa: 0.0,
        }
    }
}

/// How `k` and `omega` are imposed on the outer `farField` patch.
///
/// Both impose the same free-stream state where the flow enters the domain;
/// they differ only where it leaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FarFieldTurbulenceCondition {
    /// Clamp the free-stream value on every outer face.  This was the template
    /// default until the clamp was measured to be what held the turbulence
    /// residuals off the gate; it is retained so that behaviour stays
    /// reproducible.
    FixedValue,
    /// `inletOutlet`: the free-stream value on inflow faces, zero gradient on
    /// outflow faces.  The default, because clamping the decayed interior on
    /// outflow faces was measured to hold the turbulence residuals up for an
    /// entire run while the aerodynamic solution was already stationary.
    #[default]
    InletOutlet,
}

impl FarFieldTurbulenceCondition {
    /// Stable label used in lifecycle messages and reports.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FixedValue => "fixedValue",
            Self::InletOutlet => "inletOutlet",
        }
    }

    /// Far-field entry for one turbulence scalar at its free-stream value.
    pub(crate) fn far_field_entry(self, free_stream: f64) -> String {
        match self {
            Self::FixedValue => {
                format!("farField {{ type fixedValue; value uniform {free_stream:.16e}; }}")
            }
            Self::InletOutlet => format!(
                "farField {{ type inletOutlet; inletValue uniform {free_stream:.16e}; value uniform {free_stream:.16e}; }}"
            ),
        }
    }
}

/// How the free-stream velocity is imposed on the outer `farField` patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FarFieldVelocityCondition {
    /// Clamp the free-stream vector on every outer face, the qualified template
    /// default.
    #[default]
    FixedValue,
    /// `freestreamVelocity`, OpenFOAM's `inletOutlet` for a vector: the
    /// free-stream vector on inflow faces, zero gradient on outflow faces.
    Freestream,
}

impl FarFieldVelocityCondition {
    /// Stable label used in lifecycle messages and reports.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FixedValue => "fixedValue",
            Self::Freestream => "freestreamVelocity",
        }
    }

    /// Far-field entry for the velocity field at its free-stream vector.
    pub(crate) fn far_field_entry(self, ux: f64, uy: f64) -> String {
        match self {
            Self::FixedValue => {
                format!("farField {{ type fixedValue; value uniform ({ux:.16e} {uy:.16e} 0); }}")
            }
            Self::Freestream => format!(
                "farField {{ type freestreamVelocity; freestreamValue uniform ({ux:.16e} {uy:.16e} 0); value uniform ({ux:.16e} {uy:.16e} 0); }}"
            ),
        }
    }
}
