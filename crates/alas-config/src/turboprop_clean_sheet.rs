// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The propeller and nacelle inputs of a turboprop that no preset declared.
//!
//! The shaft-power mass method ([`crate::turboprop_mass`]) refuses to run
//! without a propeller blade count, an activity factor, a weight coefficient
//! and a nacelle area density, and only the registered ATR 72-600 declares
//! them. A clean-sheet brief that names a catalogued turboprop engine would
//! therefore reject every candidate. This module fills exactly the inputs the
//! brief left undeclared, from the installed propeller of the catalogue
//! record where it states one and from the published statistics below
//! otherwise, and nothing the brief or a preset declared is overwritten.
//!
//! The propeller diameter, governed speed and reduction ratio were never at
//! issue: they come from the catalogue record
//! ([`crate::engines::TurbopropEngineSpec`]).
//!
//! # Sources and status
//!
//! * **Blade count.** The catalogue record's certificated blade count when it
//!   has one (Hamilton Standard 568F-1: six, TCDS P.012; Ratier-Figeac
//!   FH385/FH386: eight, TCDS P.012 Issue 04). For a custom record the count
//!   is the log-log fit `N = 0.6667 P^0.2771` (`P` in kW, rounded, bounded to
//!   2..=8) through the four installations whose power and blade count are
//!   both stated in the retained text:
//!   4 blades at 1,029 shp (Piper PA-46-500TP, PT6A-42A, Hartzell HC-E4N-3Q),
//!   6 blades at 4,591 shp (C-130J), both M. H. Sadraey, *Aircraft Design: A
//!   Systems Engineering Approach*, Wiley 2013, p. 459-460 (PDF pp. 483-484,
//!   Table 8.10 and the paragraph after it), 6 blades at 2,475 shp (PW127M,
//!   568F-1) and 8 blades at 7,971 kW (TP400-D6, FH385) from the engine
//!   catalogue. The fit reproduces them to 5 %, 11 %, 6 % and 0.5 %. The same
//!   page states the direction ("as the engine power increases, the number of
//!   blades must increase"); the exponent is a four-point regression and is
//!   not validated beyond 0.7 to 8 MW.
//! * **Activity factor.** 140: D. P. Raymer, *Aircraft Design: A Conceptual
//!   Approach*, 4th ed., p. 483 (PDF p. 517), "Activity factors range from
//!   about 90-200, with a typical light aircraft activity factor being 100 and
//!   a typical large turboprop having an activity factor of 140."
//! * **Construction and `K_w`.** Composite, `K_w = 170`, the midpoint of the
//!   160 to 180 band NASA TM-83458 p. 5 gives for "advanced technology
//!   fiberglass or composite propellers". Both catalogued propellers are
//!   composite (Sadraey Table 8.10 rows 3 and 6 for the 568F-1 class). Applied
//!   only when the brief declared no propeller construction, coefficient,
//!   blade count, activity factor or assembly mass at all.
//! * **Nacelle area coefficient.** 19.70 kg/m^2, the NASA ATR 42-600 "Engine
//!   Section" group mass (NTRS 20230006542 Table 2, 415.5 kg for two
//!   nacelles) over the two built nacelle wetted areas of the catalogue
//!   profile (2 x 10.543 m^2), the same anchor the ATR 72-600 preset uses.
//!   **Calibrated on one aircraft, not validated**: GASP equation V.1.6 gives
//!   no default for `UW_NAC` and no other source was retained.
//!
//! # Not filled
//!
//! Engine controls/starters/mounts, engine oil and the pylon coefficient stay
//! at their declared zero, which the evaluation ledger shows as absent: the
//! only source for them is the same single-aircraft statement and an
//! extrapolation from it would be per-aircraft tuning. For a 70-passenger
//! turboprop the omitted installation and oil are about 350 kg at the ATR
//! level and make the clean-sheet mass slightly optimistic.

use crate::engines::TurbopropEngineSpec;
use crate::turboprop_mass::{FlopsTurbopropConfig, PropellerConstruction};

/// Blade-count statistic `N = A P^B`, `P` in kW.
const BLADE_FIT_COEFFICIENT: f64 = 0.666_749;
/// Exponent of the blade-count statistic.
const BLADE_FIT_EXPONENT: f64 = 0.277_145;
/// Fewest and most blades the statistic returns.
const BLADE_COUNT_RANGE: (u32, u32) = (2, 8);
/// Raymer p. 483 typical large-turboprop activity factor.
pub const CLEAN_SHEET_ACTIVITY_FACTOR: f64 = 140.0;
/// NASA TM-83458 p. 5 composite `K_w` band midpoint.
pub const CLEAN_SHEET_COMPOSITE_WEIGHT_COEFFICIENT: f64 = 170.0;
/// NASA ATR 42-600 engine-section anchor: 415.5 kg over 2 x 10.543 172 m^2.
pub const CLEAN_SHEET_NACELLE_AREA_DENSITY_KG_M2: f64 = 415.5 / (2.0 * 10.543_172_340_348_41);

/// Propeller blade count statistic for a shaft power, kW.
///
/// A nonfinite or nonpositive power returns the lower bound.
pub fn statistical_blade_count(shaft_power_kw: f64) -> u32 {
    if !(shaft_power_kw.is_finite() && shaft_power_kw > 0.0) {
        return BLADE_COUNT_RANGE.0;
    }
    let blades = BLADE_FIT_COEFFICIENT * shaft_power_kw.powf(BLADE_FIT_EXPONENT);
    (blades.round() as u32).clamp(BLADE_COUNT_RANGE.0, BLADE_COUNT_RANGE.1)
}

/// A turboprop mass configuration completed for an installed engine.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedTurboprop {
    /// The configuration to evaluate.
    pub config: FlopsTurbopropConfig,
    /// Whether any propeller input (blade count, activity factor, weight
    /// coefficient) came from the clean-sheet statistics instead of the brief.
    pub propeller_derived: bool,
    /// Whether the nacelle area density came from the clean-sheet statistic.
    pub nacelle_derived: bool,
}

impl ResolvedTurboprop {
    /// The evidence label of the propeller mass: `evaluated` (the label the
    /// evaluation gave) unless a clean-sheet statistic entered the regression.
    pub fn propeller_basis(&self, evaluated: &'static str) -> &'static str {
        if self.propeller_derived && evaluated == "hamilton_standard_regression" {
            "clean_sheet_statistical_hamilton_standard"
        } else {
            evaluated
        }
    }

    /// The evidence label of the nacelle area density: `evaluated` unless the
    /// clean-sheet anchor was used.
    pub fn nacelle_basis(&self, evaluated: &'static str) -> &'static str {
        if self.nacelle_derived {
            "clean_sheet_atr42_engine_section_anchor"
        } else {
            evaluated
        }
    }
}

impl FlopsTurbopropConfig {
    /// Whether the node declares nothing about the propeller.
    fn propeller_undeclared(&self) -> bool {
        let default = Self::default();
        self.propeller_blade_count == 0
            && self.propeller_activity_factor == default.propeller_activity_factor
            && self.propeller_construction == default.propeller_construction
            && self.propeller_weight_coefficient.is_none()
            && self.propeller_assembly_mass_kg.is_none()
    }

    /// This configuration with the propeller and nacelle inputs a brief left
    /// undeclared filled from the installed engine and the statistics of the
    /// module documentation. Declared values are never overwritten.
    #[must_use]
    pub fn resolved_for_engine(&self, spec: &TurbopropEngineSpec) -> ResolvedTurboprop {
        let mut resolved = self.clone();
        if self.propeller_undeclared() {
            resolved.propeller_construction = PropellerConstruction::Composite;
            resolved.propeller_weight_coefficient = Some(CLEAN_SHEET_COMPOSITE_WEIGHT_COEFFICIENT);
        }
        if resolved.propeller_assembly_mass_kg.is_none() {
            if resolved.propeller_blade_count == 0 {
                resolved.propeller_blade_count = spec
                    .propeller_blade_count
                    .filter(|count| *count > 0)
                    .unwrap_or_else(|| statistical_blade_count(spec.takeoff_shaft_power_kw));
            }
            if !(resolved.propeller_activity_factor.is_finite()
                && resolved.propeller_activity_factor > 0.0)
            {
                resolved.propeller_activity_factor = CLEAN_SHEET_ACTIVITY_FACTOR;
            }
        }
        if resolved.nacelle_area_density_kg_m2 == 0.0
            && resolved.nacelle_reference_mass_kg.is_none()
            && resolved.nacelle_reference_area_m2.is_none()
        {
            resolved.nacelle_area_density_kg_m2 = CLEAN_SHEET_NACELLE_AREA_DENSITY_KG_M2;
        }
        ResolvedTurboprop {
            propeller_derived: resolved.propeller_blade_count != self.propeller_blade_count
                || resolved.propeller_activity_factor != self.propeller_activity_factor
                || resolved.propeller_weight_coefficient != self.propeller_weight_coefficient,
            nacelle_derived: resolved.nacelle_area_density_kg_m2 != self.nacelle_area_density_kg_m2,
            config: resolved,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pw127m() -> TurbopropEngineSpec {
        crate::engines::database()
            .iter()
            .find_map(|engine| engine.turboprop.clone().filter(|_| engine.name == "PW127M"))
            .unwrap_or_else(|| panic!("the catalogue carries the PW127M"))
    }

    #[test]
    fn the_blade_statistic_reproduces_its_four_source_points() {
        for (kw, blades) in [
            (1029.0 * 0.745_699_872, 4),
            (1845.6, 6),
            (4591.0 * 0.745_699_872, 6),
            (7971.0, 8),
        ] {
            let fit = BLADE_FIT_COEFFICIENT * f64::powf(kw, BLADE_FIT_EXPONENT);
            assert!(
                (fit / f64::from(blades) - 1.0).abs() < 0.12,
                "{kw} kW: {fit} vs {blades}"
            );
        }
        assert_eq!(statistical_blade_count(f64::NAN), 2);
        assert_eq!(statistical_blade_count(-1.0), 2);
        assert_eq!(statistical_blade_count(1.0e9), 8);
    }

    #[test]
    fn an_undeclared_propeller_is_completed_from_the_catalogue_and_validates() {
        let resolved = FlopsTurbopropConfig::default()
            .resolved_for_engine(&pw127m())
            .config;
        assert_eq!(resolved.propeller_blade_count, 6);
        assert_eq!(resolved.propeller_activity_factor, 140.0);
        assert_eq!(resolved.resolved_weight_coefficient(), Some(170.0));
        assert!(resolved
            .resolved_nacelle_area_density_kg_m2()
            .is_some_and(|density| density > 0.0));
        assert_eq!(resolved.validate(), Ok(()));
    }

    #[test]
    fn a_custom_engine_without_a_blade_count_uses_the_statistic() {
        let mut spec = pw127m();
        spec.propeller_blade_count = None;
        let resolved = FlopsTurbopropConfig::default()
            .resolved_for_engine(&spec)
            .config;
        assert_eq!(resolved.propeller_blade_count, 5);
    }

    #[test]
    fn declared_values_are_never_overwritten() {
        let declared = crate::AlasConfig::from_value(&serde_json::json!({ "preset": "ATR72-600" }))
            .unwrap_or_else(|error| panic!("ATR72-600 is a registered preset: {error}"))
            .mass_model
            .flops_turboprop;
        let untouched = declared.resolved_for_engine(&pw127m());
        assert_eq!(untouched.config, declared);
        assert!(!untouched.propeller_derived && !untouched.nacelle_derived);
        let partial = FlopsTurbopropConfig {
            propeller_blade_count: 4,
            nacelle_area_density_kg_m2: 30.0,
            ..FlopsTurbopropConfig::default()
        };
        let resolved = partial.resolved_for_engine(&pw127m()).config;
        assert_eq!(resolved.propeller_blade_count, 4);
        assert_eq!(resolved.nacelle_area_density_kg_m2, 30.0);
        assert_eq!(resolved.propeller_activity_factor, 140.0);
    }
}
