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

    /// The design landing mass `WLDG`, kg, of the structure a closure at
    /// `closure_mass_kg` is designed for, under the takeoff-mass sizing plan.
    ///
    /// When the plan designs the structure at the closure
    /// ([`crate::optimizer::StructuralBasis::ClosureMass`]), an explicit
    /// `design_landing_mass_kg` still wins; a registered aircraft in a
    /// fixed-aircraft design mode keeps its declared landing-to-takeoff
    /// ratio, `WLDG = (MLW / MTOW) x closure` from the preset reference
    /// weights; everything else uses the configured `mlw_fraction_mtow`.
    /// Otherwise this is [`Self::design_landing_mass_for`].
    ///
    /// Consumers that compare a landing mass with its limit should read this
    /// rather than [`Self::landing_mass_limit_kg`], which is the declared
    /// limit of the unsized aircraft.
    pub fn design_landing_mass_at_closure(&self, closure_mass_kg: f64) -> f64 {
        if self.mtow_plan().structural_basis != crate::optimizer::StructuralBasis::ClosureMass
            || self.mass_sizing_basis() == MassSizingBasis::Coupled
        {
            return self.design_landing_mass_for(closure_mass_kg);
        }
        if let Some(declared) = self.mass_model.flops_structure.design_landing_mass_kg {
            return declared;
        }
        let declared = self
            .is_fixed_aircraft_mode()
            .then(|| crate::presets::get(&self.preset).ok())
            .flatten()
            .and_then(|preset| Some((preset.reference.mlw_kg?, preset.reference.mtow_kg?)))
            .filter(|(mlw, mtow)| mlw.is_finite() && mtow.is_finite() && *mlw > 0.0 && *mtow > 0.0);
        match declared {
            // Scaled as MLW x (closure / MTOW) so a closure at the declared
            // MTOW returns the declared MLW exactly.
            Some((mlw_kg, mtow_kg)) => mlw_kg * (closure_mass_kg / mtow_kg),
            None => closure_mass_kg * self.mass_model.mlw_fraction_mtow,
        }
    }

    /// FLOPS `DG` and `WLDG`, kg, of the structure a closure at
    /// `closure_mass_kg` is designed for, under the takeoff-mass sizing plan.
    pub fn sized_design_weights_kg(&self, closure_mass_kg: f64) -> (f64, f64) {
        match self
            .at_sized_closure_mass(closure_mass_kg)
            .mass_sizing_basis()
        {
            MassSizingBasis::FixedAircraft {
                design_gross_mass_kg,
                design_landing_mass_kg,
            } => (design_gross_mass_kg, design_landing_mass_kg),
            MassSizingBasis::Coupled => (
                closure_mass_kg,
                self.design_landing_mass_at_closure(closure_mass_kg),
            ),
        }
    }

    /// [`Self::at_closure_mass`] under the takeoff-mass sizing plan.
    ///
    /// Identical to it except when the plan designs the structure at the
    /// closure and the design mode would otherwise keep a registered
    /// aircraft's declared weights (the MTOW band and payload-adjusted
    /// modes in `BaselineSandbox` or `ReferenceAdaptation`): there `DG` is
    /// written as the closure and `WLDG` as
    /// [`Self::design_landing_mass_at_closure`]. Call it on the unsized
    /// configuration; the rewritten one carries the overrides and reads as a
    /// declared structure.
    pub fn at_sized_closure_mass(&self, closure_mass_kg: f64) -> Self {
        let designs_at_closure =
            self.mtow_plan().structural_basis == crate::optimizer::StructuralBasis::ClosureMass;
        if !designs_at_closure || self.mass_sizing_basis() == MassSizingBasis::Coupled {
            return self.at_closure_mass(closure_mass_kg);
        }
        let mut config = self.clone();
        config.mass_model.flops_structure.design_gross_mass_kg = Some(closure_mass_kg);
        config.mass_model.flops_structure.design_landing_mass_kg =
            Some(self.design_landing_mass_at_closure(closure_mass_kg));
        config.requirements.mtow_kg = closure_mass_kg;
        config
    }

    /// Whether the reserve floor of
    /// [`Self::design_landing_mass_with_reserve_floor`] applies: only the two
    /// design modes, only when they design the structure at the closure, and
    /// never over an explicit `design_landing_mass_kg`.
    fn landing_reserve_floor_applies(&self) -> bool {
        let plan = self.mtow_plan();
        plan.requires_mission_sized_evaluation()
            && plan.structural_basis == crate::optimizer::StructuralBasis::ClosureMass
            && self
                .mass_model
                .flops_structure
                .design_landing_mass_kg
                .is_none()
    }

    /// The design landing mass `WLDG`, kg, of a structure designed at the
    /// closure `closure_mass_kg`, raised to cover `landing_floor_kg`, the
    /// design zero-fuel mass plus all fuel the sizing mission carries past
    /// the destination (contingency, alternate, final reserve, additional
    /// and extra) at that closure.
    ///
    /// `WLDG = max(design_landing_mass_at_closure, ZFW + reserves)` in the
    /// MTOW band and payload-adjusted modes: an aircraft has to be able to
    /// land with its full payload and the reserves it is dispatched with,
    /// which is the usual transport design practice `MLW >= MZFW + reserves`
    /// (E. Torenbeek, *Synthesis of Subsonic Airplane Design*, Delft
    /// University Press / Kluwer, 1982, Sec. 8.4 on design weights; section
    /// and page not re-checked here: engineering estimate [E]). A ratio
    /// scaled from a long-range preset alone falls below it when the closure
    /// mission is short. Every other mode, and an explicit landing-mass
    /// override, returns [`Self::design_landing_mass_at_closure`] unchanged.
    pub fn design_landing_mass_with_reserve_floor(
        &self,
        closure_mass_kg: f64,
        landing_floor_kg: Option<f64>,
    ) -> f64 {
        let scaled_kg = self.design_landing_mass_at_closure(closure_mass_kg);
        match landing_floor_kg {
            Some(floor_kg)
                if floor_kg.is_finite()
                    && floor_kg > 0.0
                    && self.landing_reserve_floor_applies() =>
            {
                scaled_kg.max(floor_kg)
            }
            _ => scaled_kg,
        }
    }

    /// [`Self::at_sized_closure_mass`] with the landing gear designed at
    /// [`Self::design_landing_mass_with_reserve_floor`]. Identical to it
    /// unless the floor applies and exceeds the scaled landing mass.
    pub fn at_sized_closure_mass_with_landing_floor(
        &self,
        closure_mass_kg: f64,
        landing_floor_kg: Option<f64>,
    ) -> Self {
        let mut config = self.at_sized_closure_mass(closure_mass_kg);
        let scaled_kg = self.design_landing_mass_at_closure(closure_mass_kg);
        let floored_kg =
            self.design_landing_mass_with_reserve_floor(closure_mass_kg, landing_floor_kg);
        if floored_kg > scaled_kg {
            config.mass_model.flops_structure.design_landing_mass_kg = Some(floored_kg);
        }
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
    fn the_design_modes_size_a_registered_structure_at_the_closure_with_its_mlw_ratio() {
        use crate::MtowSizing;
        for design_mode in [DesignMode::BaselineSandbox, DesignMode::ReferenceAdaptation] {
            for sizing in MtowSizing::ALL {
                let mut config = a320(design_mode);
                config.optimizer.objective.mtow_sizing = sizing;
                let sized = config.at_sized_closure_mass(70_000.0);
                assert_eq!(sized.requirements.mtow_kg, 70_000.0);
                let structure = &sized.mass_model.flops_structure;
                if sizing.requires_mission_sized_evaluation() {
                    let wldg = 66_000.0 / 78_000.0 * 70_000.0;
                    assert_eq!(structure.design_gross_mass_kg, Some(70_000.0));
                    let landing = structure.design_landing_mass_kg.unwrap();
                    assert!((landing - wldg).abs() < 1e-6, "{sizing:?}");
                    assert!((config.design_landing_mass_at_closure(70_000.0) - wldg).abs() < 1e-6);
                    assert_eq!(config.sized_design_weights_kg(70_000.0).0, 70_000.0);
                } else {
                    assert_eq!(sized, config.at_closure_mass(70_000.0), "{sizing:?}");
                    assert_eq!(config.design_landing_mass_at_closure(70_000.0), 66_000.0);
                }
            }
        }
        // At the declared MTOW the ratio reproduces the declared MLW exactly.
        let mut band = a320(DesignMode::ReferenceAdaptation);
        band.optimizer.objective.mtow_sizing = MtowSizing::MtowBand;
        assert_eq!(band.design_landing_mass_at_closure(78_000.0), 66_000.0);
    }

    #[test]
    fn the_design_modes_raise_the_landing_mass_to_cover_zero_fuel_mass_and_reserves() {
        use crate::MtowSizing;
        let ratio_kg = 66_000.0 / 78_000.0 * 70_000.0;
        for design_mode in [DesignMode::ReferenceAdaptation, DesignMode::CleanSheet] {
            for sizing in MtowSizing::ALL {
                let mut config = a320(design_mode);
                config.optimizer.objective.mtow_sizing = sizing;
                let scaled = config.design_landing_mass_at_closure(70_000.0);
                let floored =
                    config.design_landing_mass_with_reserve_floor(70_000.0, Some(65_000.0));
                let sized =
                    config.at_sized_closure_mass_with_landing_floor(70_000.0, Some(65_000.0));
                if sizing.requires_mission_sized_evaluation() {
                    assert_eq!(floored, 65_000.0, "{design_mode:?} {sizing:?}");
                    assert_eq!(
                        sized.mass_model.flops_structure.design_landing_mass_kg,
                        Some(65_000.0)
                    );
                    // A floor below the scaled mass changes nothing.
                    assert_eq!(
                        config.design_landing_mass_with_reserve_floor(70_000.0, Some(1_000.0)),
                        scaled
                    );
                } else {
                    assert_eq!(floored, scaled, "{design_mode:?} {sizing:?}");
                    assert_eq!(sized, config.at_sized_closure_mass(70_000.0));
                }
            }
        }
        let mut band = a320(DesignMode::ReferenceAdaptation);
        band.optimizer.objective.mtow_sizing = MtowSizing::MtowBand;
        assert!((band.design_landing_mass_at_closure(70_000.0) - ratio_kg).abs() < 1e-6);
        // An explicit landing mass is never raised.
        band.mass_model.flops_structure.design_landing_mass_kg = Some(50_000.0);
        assert_eq!(
            band.design_landing_mass_with_reserve_floor(70_000.0, Some(65_000.0)),
            50_000.0
        );
    }

    #[test]
    fn a_clean_sheet_design_mode_keeps_the_landing_fraction_of_the_closure() {
        let mut config = a320(DesignMode::CleanSheet);
        config.optimizer.objective.mtow_sizing = crate::MtowSizing::PayloadAdjusted;
        assert_eq!(
            config.at_sized_closure_mass(70_000.0),
            config.at_closure_mass(70_000.0)
        );
        let fraction = config.mass_model.mlw_fraction_mtow;
        assert!(
            (config.design_landing_mass_at_closure(70_000.0) - 70_000.0 * fraction).abs() < 1e-9
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
