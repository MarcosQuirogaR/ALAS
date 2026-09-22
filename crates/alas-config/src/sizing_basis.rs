// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Which mass the FLOPS structural and systems equations size against.
//!
//! The mass model answers three physically different questions, and the
//! difference between them is the design gross mass `DG` the wing, tails,
//! fuselage, surface controls and pod-relief terms are evaluated at, and the
//! design landing mass `WLDG` the gear is evaluated at:
//!
//! * **Fixed-aircraft mass estimation.** A registered airframe at its
//!   declared design weights. `DG` is the declared MTOW and `WLDG` the
//!   declared landing limit; the result is the aircraft's operating empty
//!   mass, and it does not depend on what mission is flown.
//! * **Fixed-aircraft mission evaluation.** The same airframe flies a
//!   payload/range case. Only fuel and dispatch mass move; the component
//!   ledger is the one from the first question.
//! * **Coupled new-aircraft sizing.** A clean-sheet design whose design
//!   gross mass is the takeoff mass the mission closure converges to. Every
//!   pass re-evaluates the components at the current iterate, which is what
//!   a sizing loop is for.
//!
//! [`DesignMode`] already says which of these a run is: `BaselineSandbox`
//! and `ReferenceAdaptation` replay or adapt a registered aircraft whose
//! certified weights are inputs, `CleanSheet` sizes a new one. Before this
//! seam existed the mission-sized closure re-evaluated every component at the
//! dispatch iterate under all three modes, so a registered aircraft flown on
//! a short route quietly became a lighter aircraft with the same name.

use crate::optimizer::DesignMode;
use crate::AlasConfig;

/// The design-weight basis of one mass evaluation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MassSizingBasis {
    /// Component masses are evaluated at fixed design weights; a mission
    /// changes only the fuel and dispatch mass.
    FixedAircraft {
        /// FLOPS `DG`, kg.
        design_gross_mass_kg: f64,
        /// FLOPS `WLDG`, kg.
        design_landing_mass_kg: f64,
    },
    /// The design gross mass follows the closed takeoff mass and the landing
    /// mass follows it through the configured fraction.
    Coupled,
}

impl MassSizingBasis {
    /// Stable report label.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FixedAircraft { .. } => "fixed_aircraft",
            Self::Coupled => "coupled",
        }
    }

    /// Whether the component ledger is invariant under mission-only changes.
    pub const fn is_fixed_aircraft(self) -> bool {
        matches!(self, Self::FixedAircraft { .. })
    }
}

impl AlasConfig {
    /// The design-weight basis this configuration's design mode implies.
    ///
    /// A declared `flops_structure.design_gross_mass_kg` pins the basis in
    /// every mode: it is the user saying the structure was designed for that
    /// weight, whatever the closure does. When that explicit DG has no WLDG,
    /// the configured landing-mass fraction is applied to the same DG. With
    /// no DG override, the fixed-aircraft modes size at
    /// `requirements.mtow_kg` and the mode-aware landing limit, and the
    /// clean-sheet mode couples.
    pub fn mass_sizing_basis(&self) -> MassSizingBasis {
        let structure = &self.mass_model.flops_structure;
        if let Some(declared) = structure.design_gross_mass_kg {
            // An explicit DG override defines a fixed aircraft variant. If
            // WLDG is omitted, apply the configured landing-mass fraction to
            // that same DG; a registered preset's absolute certified MLW is
            // tied to its registry MTOW and would otherwise exceed a smaller
            // user-declared design weight (for example DG=60 t, MLW=66 t).
            return MassSizingBasis::FixedAircraft {
                design_gross_mass_kg: declared,
                design_landing_mass_kg: structure
                    .design_landing_mass_kg
                    .unwrap_or(declared * self.mass_model.mlw_fraction_mtow),
            };
        }
        match self.optimizer.design_space.mode {
            DesignMode::CleanSheet => MassSizingBasis::Coupled,
            DesignMode::BaselineSandbox | DesignMode::ReferenceAdaptation => {
                MassSizingBasis::FixedAircraft {
                    design_gross_mass_kg: self.requirements.mtow_kg,
                    design_landing_mass_kg: structure
                        .design_landing_mass_kg
                        .unwrap_or_else(|| self.landing_mass_limit_kg(self.requirements.mtow_kg)),
                }
            }
        }
    }

    /// The landing mass a case evaluated at `takeoff_mass_kg` is designed
    /// for: the fixed aircraft's declared basis in every mode, or the
    /// coupled fraction of the closure mass.
    pub fn design_landing_mass_for(&self, takeoff_mass_kg: f64) -> f64 {
        match self.mass_sizing_basis() {
            MassSizingBasis::FixedAircraft {
                design_landing_mass_kg,
                ..
            } => design_landing_mass_kg,
            MassSizingBasis::Coupled => self.landing_mass_limit_kg(takeoff_mass_kg),
        }
    }

    /// This configuration with the mass ledger closed at `closure_mass_kg`
    /// instead of the declared requirement.
    ///
    /// `requirements.mtow_kg` becomes the closure mass, which is what the
    /// fuel remainder, the trim mass and the payload layout read. Under a
    /// fixed-aircraft basis the FLOPS design gross and landing masses are
    /// written into the structure overrides first, so the components keep
    /// their declared design weights while the closure mass moves; under a
    /// coupled basis they follow the closure mass as before.
    pub fn at_closure_mass(&self, closure_mass_kg: f64) -> Self {
        let mut config = self.clone();
        if let MassSizingBasis::FixedAircraft {
            design_gross_mass_kg,
            design_landing_mass_kg,
        } = self.mass_sizing_basis()
        {
            config.mass_model.flops_structure.design_gross_mass_kg = Some(design_gross_mass_kg);
            config.mass_model.flops_structure.design_landing_mass_kg = Some(design_landing_mass_kg);
        }
        config.requirements.mtow_kg = closure_mass_kg;
        config
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn a320(mode: DesignMode) -> AlasConfig {
        let mut config = AlasConfig::from_value(&json!({"preset": "A320-200"})).unwrap();
        config.optimizer.design_space.mode = mode;
        config
    }

    #[test]
    fn a_registered_aircraft_keeps_its_declared_design_weights_while_the_closure_mass_moves() {
        for mode in [DesignMode::BaselineSandbox, DesignMode::ReferenceAdaptation] {
            let config = a320(mode);
            assert_eq!(
                config.mass_sizing_basis(),
                MassSizingBasis::FixedAircraft {
                    design_gross_mass_kg: 78_000.0,
                    design_landing_mass_kg: 66_000.0,
                },
                "{mode:?}"
            );
            let at_dispatch = config.at_closure_mass(70_000.0);
            assert_eq!(at_dispatch.requirements.mtow_kg, 70_000.0);
            assert_eq!(
                at_dispatch.mass_model.flops_structure.design_gross_mass_kg,
                Some(78_000.0)
            );
            assert_eq!(
                at_dispatch
                    .mass_model
                    .flops_structure
                    .design_landing_mass_kg,
                Some(66_000.0)
            );
            // The basis survives the closure-mass rewrite, so a second pass
            // built from the rewritten configuration sizes the same aircraft.
            assert_eq!(at_dispatch.mass_sizing_basis(), config.mass_sizing_basis());
        }
    }

    #[test]
    fn a_clean_sheet_design_couples_unless_a_design_gross_mass_is_declared() {
        let config = a320(DesignMode::CleanSheet);
        assert_eq!(config.mass_sizing_basis(), MassSizingBasis::Coupled);
        let at_dispatch = config.at_closure_mass(70_000.0);
        assert_eq!(at_dispatch.requirements.mtow_kg, 70_000.0);
        assert_eq!(
            at_dispatch.mass_model.flops_structure.design_gross_mass_kg,
            None
        );
        assert_eq!(
            at_dispatch
                .mass_model
                .flops_structure
                .design_landing_mass_kg,
            None
        );

        let mut declared = config;
        declared.mass_model.flops_structure.design_gross_mass_kg = Some(79_000.0);
        assert_eq!(
            declared.mass_sizing_basis(),
            MassSizingBasis::FixedAircraft {
                design_gross_mass_kg: 79_000.0,
                design_landing_mass_kg: 79_000.0 * declared.mass_model.mlw_fraction_mtow,
            }
        );
    }

    #[test]
    fn an_explicit_lower_dg_derives_wldg_from_the_same_fixed_aircraft_basis() {
        let mut config = a320(DesignMode::BaselineSandbox);
        config.mass_model.flops_structure.design_gross_mass_kg = Some(60_000.0);

        assert_eq!(
            config.mass_sizing_basis(),
            MassSizingBasis::FixedAircraft {
                design_gross_mass_kg: 60_000.0,
                design_landing_mass_kg: 60_000.0 * config.mass_model.mlw_fraction_mtow,
            }
        );
    }

    #[test]
    fn the_basis_labels_are_stable() {
        assert_eq!(MassSizingBasis::Coupled.as_str(), "coupled");
        assert!(!MassSizingBasis::Coupled.is_fixed_aircraft());
        let fixed = MassSizingBasis::FixedAircraft {
            design_gross_mass_kg: 1.0,
            design_landing_mass_kg: 1.0,
        };
        assert_eq!(fixed.as_str(), "fixed_aircraft");
        assert!(fixed.is_fixed_aircraft());
    }
}
