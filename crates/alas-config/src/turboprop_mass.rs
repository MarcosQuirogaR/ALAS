// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Declared inputs for the turboprop propulsion-group mass method.
//!
//! NASA/TM-2017-219627 Vol. I - the FLOPS source the production mass
//! architecture evaluates - contains **no** propeller, gearbox or shaft-power
//! engine-mass equation: every propulsion term in sections 5.2.9 and 5.3 is
//! parameterised on sea-level-static *thrust* (equations 69, 75-80, 86, 87,
//! 89 and 92). Searching the published text for "propeller", "turboprop" or
//! "shaft horsepower" returns nothing. NASA Aviary, the reference
//! implementation of the same source, has no propeller or gearbox component
//! in `mass/flops_based/` either.
//!
//! A turboprop therefore cannot be given a FLOPS propulsion group, and
//! substituting a thrust for its shaft power would invent a quantity the
//! aircraft does not have. This node instead declares the inputs of a
//! **shaft-power** propulsion group whose airframe, systems and
//! passenger-driven operating items stay on the FLOPS equations, with one
//! fixed ownership crosswalk per component:
//!
//! | Group | Owner | Source |
//! |---|---|---|
//! | Engine (turbomachine + reduction gearbox) | declared certificated dry mass, else GASP specific weight | NASA CR-152303 Vol. V eq. V.1.3-V.1.4 |
//! | Propeller | source-declared propeller mass, else Hamilton Standard regression | NASA CR-152303 Vol. V eq. V.1.28-V.1.29; NASA TM-83458 p. 5; aircraft source when declared |
//! | Reduction gearbox | inside the declared engine mass | EASA TCDS IM.E.041 sec. III.2 |
//! | Nacelle | area density times nacelle wetted area | NASA CR-152303 Vol. V eq. V.1.6 |
//! | Pylon | `F_PYL (W_ENG + W_NAC)^0.736` | NASA CR-152303 Vol. V eq. V.1.7 |
//! | Thrust reversers | none: reverse is by blade pitch | n/a |
//! | Engine controls, starters, mounts | declared installation mass | declared |
//! | Fuel system | FLOPS equation 92 (no thrust term) | NASA/TM-2017-219627 eq. 92 |
//! | Unusable fuel | FLOPS **alternate** equation 161 | NASA/TM-2017-219627 eq. 161 |
//! | Engine oil | declared; see below | - |
//!
//! Equations 121 and 123 are the only operating items that read a thrust, so
//! they are the only two that need a substitute. Section 7.1.4 of the same
//! NASA document gives thrust-free alternates for both, which keeps the
//! substitution inside the published source rather than importing a foreign
//! correlation - but only equation 161 survives inspection. The alternate
//! engine oil, equation 162, is printed as `WOIL = 240 (NPASS + 39) / 40`
//! (p. 56) and returns 1,248 lb of oil for the 169-passenger
//! `LargeSingleAisle1` case against 130.23 lb from the default equation 123
//! for the same aircraft: a factor of ten, and far above any real transport's
//! oil charge. It is therefore not used, and the oil is declared instead.

use serde::{Deserialize, Serialize};

use crate::{ConfigNode, Kind, Leaf};

/// Hamilton Standard propeller construction, which selects the regression
/// constants of NASA CR-152303 Vol. V table on p. V-1.12 and NASA TM-83458
/// p. 5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropellerConstruction {
    /// Solid aluminium blades, double-acting pitch change. `K_w = 355`
    /// (NASA TM-83458 p. 5).
    #[default]
    AluminiumDoubleActing,
    /// Solid aluminium blades, single-acting with counterweights.
    /// `K_w = 220` with the counterweight term of equation V.1.29.
    AluminiumSingleActing,
    /// Fibreglass or composite blades. NASA TM-83458 p. 5 states verbatim
    /// that "values of `K_w` ranging between 160 to 180 may be assumed for
    /// advanced technology fiberglass or composite propellers"; the
    /// declared [`FlopsTurbopropConfig::propeller_weight_coefficient`]
    /// selects the point inside that band and must carry its own evidence.
    Composite,
}

impl Leaf for PropellerConstruction {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Str
    }
}

impl PropellerConstruction {
    /// The published `K_w` for this construction, or `None` for the composite
    /// band, whose value must be declared explicitly.
    pub fn published_weight_coefficient(self) -> Option<f64> {
        match self {
            Self::AluminiumDoubleActing => Some(355.0),
            Self::AluminiumSingleActing => Some(220.0),
            Self::Composite => None,
        }
    }

    /// Whether the counterweight term `C_w` of equation V.1.29 applies.
    pub fn has_counterweights(self) -> bool {
        matches!(self, Self::AluminiumSingleActing)
    }
}

/// Declared inputs of the shaft-power propulsion-group mass method.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields, default)]
pub struct FlopsTurbopropConfig {
    /// Certificated dry mass of one installed engine.
    #[config(
        advanced,
        label = "Engine dry mass",
        unit = "kg",
        help = "Certificated dry mass of one engine, which for the PW100 family includes the reduction gearbox (EASA TCDS IM.E.041 section III.2). Blank uses the NASA GASP turboshaft/turboprop specific weight of 0.5 lb per shaft horsepower (CR-152303 Vol. V equation V.1.3) instead, which is a general-aviation regression rather than certification evidence."
    )]
    pub engine_dry_mass_kg: Option<f64>,

    /// Shaft power the declared engine dry mass belongs to.
    #[config(
        advanced,
        label = "Baseline engine shaft power",
        unit = "kW",
        help = "Take-off shaft power of the engine the declared dry mass was measured at. Blank uses the installed engine's own take-off rating, so the scaling ratio is one and the certificated mass is used unchanged."
    )]
    pub baseline_shaft_power_kw: Option<f64>,

    /// Exponent on the shaft-power ratio when the engine mass is scaled.
    #[config(
        advanced,
        label = "Engine mass scaling exponent",
        help = "Exponent on the shaft-power ratio when a declared engine dry mass is scaled to a different rating. An exponent of 1 reproduces the linear NASA GASP specific-weight relation (CR-152303 Vol. V equation V.1.4); the mass is unchanged at the baseline rating whatever the exponent."
    )]
    pub engine_mass_scaling_exponent: f64,

    /// Whether the declared engine dry mass already contains the gearbox.
    #[config(
        advanced,
        label = "Gearbox inside engine mass",
        help = "The certificated PW100 dry weight covers the turbomachine and the reduction gearbox together (EASA TCDS IM.E.041 section III.2), so a separately estimated gearbox would be counted twice. Disable only for an engine whose quoted dry mass excludes its gearbox, in which case the NASA TM-83458 torque relation is evaluated and reported on its own line."
    )]
    pub gearbox_inside_engine_mass: bool,

    /// Number of blades on one propeller, Hamilton Standard `B`.
    #[config(
        advanced,
        label = "Propeller blade count",
        help = "Number of blades on one propeller, the Hamilton Standard B of NASA CR-152303 Vol. V equation V.1.28."
    )]
    pub propeller_blade_count: u32,

    /// Blade activity factor per blade, Hamilton Standard `A.F.`.
    #[config(
        advanced,
        label = "Blade activity factor",
        help = "Hamilton Standard blade activity factor A.F. per blade (NASA CR-152303 Vol. V equation V.1.28). Zero blocks the propeller mass instead of assuming a value."
    )]
    pub propeller_activity_factor: f64,

    /// Blade construction, which selects the regression constants.
    #[config(
        advanced,
        options = PropellerConstruction,
        label = "Propeller construction",
        help = "Blade material and pitch-change hardware. This selects the Hamilton Standard regression constants of NASA TM-83458 p. 5 and whether the counterweight term of equation V.1.29 applies."
    )]
    pub propeller_construction: PropellerConstruction,

    /// Declared `K_w`, required for the composite band.
    #[config(
        advanced,
        label = "Propeller weight coefficient",
        help = "Hamilton Standard K_w. Blank uses the published constant for the selected construction (355 double-acting aluminium, 220 single-acting aluminium). A composite propeller has no single published value: NASA TM-83458 p. 5 sanctions the band 160 to 180, and the point inside it must be declared with its own evidence."
    )]
    pub propeller_weight_coefficient: Option<f64>,

    /// Mass of the spinner, blade de-icing and governor per propeller.
    #[config(
        advanced,
        label = "Propeller accessory mass",
        unit = "kg",
        help = "Spinner, blade de-icing and governor mass for one propeller. NASA CR-152303 Vol. V p. V-1.11 states verbatim that the regression's propeller weight excludes these, so an aircraft with electrically de-iced blades carries them here rather than inside the regression."
    )]
    pub propeller_accessory_mass_kg: f64,

    /// Declared mass of one source-specified propeller item, when an
    /// aircraft-specific source is available. The source inclusion scope is
    /// controlled separately because a certificate table may not define it.
    #[config(
        advanced,
        label = "Propeller assembly mass",
        unit = "kg",
        help = "Optional source-backed mass of one propeller item. When present it replaces the generic Hamilton Standard regression for that installation; the regression inputs remain the documented fallback for configurations without a source statement. Declare the accessory-scope control separately when the source defines it; an unknown scope forbids a separate accessory allowance. Update or clear this fixed declaration when changing the propeller design."
    )]
    pub propeller_assembly_mass_kg: Option<f64>,

    /// Whether a source-backed assembly mass explicitly includes the
    /// separately declared accessory scope.
    #[config(
        advanced,
        label = "Assembly includes accessories",
        help = "Scope control for a declared propeller assembly mass. Blank means the source scope is unknown and forbids a separate accessory allowance; false means the source explicitly excludes accessories; true means it includes them and the separate accessory mass must remain zero."
    )]
    pub propeller_assembly_accessories_included: Option<bool>,

    /// Nacelle mass per unit nacelle wetted area, GASP `UW_NAC`.
    #[config(
        advanced,
        label = "Nacelle area density",
        unit = "kg/m^2",
        help = "NASA GASP UW_NAC of CR-152303 Vol. V equation V.1.6: an empirical nacelle component area coefficient, not a material density. There is no published shaft-power-based turboprop nacelle relation, so this remains a declared model input. A reference component mass and area, when supplied, override this scalar. Zero blocks the nacelle mass instead of assuming a value."
    )]
    pub nacelle_area_density_kg_m2: f64,

    /// Component mass used to anchor a calibrated nacelle area density.
    #[config(
        advanced,
        label = "Nacelle reference component mass",
        unit = "kg",
        help = "Optional source-backed mass of one nacelle component used with the reference area below. When both reference fields are present, the evaluator resolves the area density as reference mass divided by reference area and applies it to the built nacelle area."
    )]
    pub nacelle_reference_mass_kg: Option<f64>,

    /// Wetted area corresponding to the reference nacelle component mass.
    #[config(
        advanced,
        label = "Nacelle reference area",
        unit = "m^2",
        help = "Optional reference wetted area of one nacelle corresponding to the reference component mass. This keeps a calibrated component anchor explicit when the runtime uses the built nacelle wetted area; it is not an independently measured material density."
    )]
    pub nacelle_reference_area_m2: Option<f64>,

    /// Pylon coefficient, GASP `F_PYL`.
    #[config(
        advanced,
        label = "Pylon coefficient",
        help = "NASA GASP F_PYL of CR-152303 Vol. V equation V.1.7, W_PYLON = F_PYL (W_ENG + W_NAC)^0.736, applied for a multi-engine installation. Zero charges no pylon mass, which is the FLOPS transport boundary; a wing-mounted turboprop nacelle that is faired into the wing rather than pylon-mounted is the physical case for zero."
    )]
    pub pylon_coefficient: f64,

    /// Engine controls, starters, mounts and fire protection, for all engines.
    #[config(
        advanced,
        label = "Engine installation mass",
        unit = "kg",
        help = "Engine controls, starters, mounts, fire protection and installation accessories for every installed engine together. FLOPS equations 87 and 89 estimate the same scope from rated thrust and have no shaft-power form, and no published turboprop equation for it was found, so this is a declared input. Zero declares that the installation mass is genuinely absent, which is visible in the ledger rather than hidden."
    )]
    pub engine_installation_mass_kg: f64,

    /// Engine oil charged to the operating items, for every engine together.
    #[config(
        advanced,
        label = "Engine oil mass",
        unit = "kg",
        help = "Engine oil for every installed engine together. FLOPS equation 123 estimates it from rated thrust and has no shaft-power form, and the same document's thrust-free alternate equation 162 as printed returns ten times equation 123 for the same aircraft, so it is not usable. Zero leaves the oil visibly absent from the ledger rather than filling it with a wrong value."
    )]
    pub engine_oil_mass_kg: f64,
}

impl Default for FlopsTurbopropConfig {
    fn default() -> Self {
        Self {
            engine_dry_mass_kg: None,
            baseline_shaft_power_kw: None,
            engine_mass_scaling_exponent: 1.0,
            gearbox_inside_engine_mass: true,
            propeller_blade_count: 0,
            propeller_activity_factor: 0.0,
            propeller_construction: PropellerConstruction::default(),
            propeller_weight_coefficient: None,
            propeller_accessory_mass_kg: 0.0,
            propeller_assembly_mass_kg: None,
            propeller_assembly_accessories_included: None,
            nacelle_area_density_kg_m2: 0.0,
            nacelle_reference_mass_kg: None,
            nacelle_reference_area_m2: None,
            pylon_coefficient: 0.0,
            engine_installation_mass_kg: 0.0,
            engine_oil_mass_kg: 0.0,
        }
    }
}

impl FlopsTurbopropConfig {
    /// Whether the node is entirely undeclared, so it can be omitted from a
    /// saved file.
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// The Hamilton Standard `K_w` this configuration selects, if it is
    /// resolvable.
    ///
    /// A composite propeller has no single published constant, so it returns
    /// the declared value or `None`; the other constructions fall back to
    /// their published value when nothing is declared.
    pub fn resolved_weight_coefficient(&self) -> Option<f64> {
        match self.propeller_weight_coefficient {
            Some(value) if value.is_finite() && value > 0.0 => Some(value),
            Some(_) => None,
            None => self.propeller_construction.published_weight_coefficient(),
        }
    }

    /// Resolve the nacelle area density from an explicit component anchor
    /// when one is declared, retaining the legacy scalar as the fallback for
    /// older or generic configurations.
    pub fn resolved_nacelle_area_density_kg_m2(&self) -> Option<f64> {
        match (
            self.nacelle_reference_mass_kg,
            self.nacelle_reference_area_m2,
        ) {
            (Some(mass), Some(area))
                if mass.is_finite() && mass > 0.0 && area.is_finite() && area > 0.0 =>
            {
                let density = mass / area;
                density.is_finite().then_some(density)
            }
            (None, None) if self.nacelle_area_density_kg_m2.is_finite() => {
                Some(self.nacelle_area_density_kg_m2)
            }
            _ => None,
        }
    }

    /// Reject values outside the ranges the published relations admit.
    ///
    /// # Errors
    ///
    /// The name of the first field whose declared value is nonfinite or
    /// outside its physical range.
    pub fn validate(&self) -> Result<(), &'static str> {
        for (value, field) in [
            (
                self.engine_mass_scaling_exponent,
                "engine_mass_scaling_exponent",
            ),
            (self.propeller_activity_factor, "propeller_activity_factor"),
            (
                self.propeller_accessory_mass_kg,
                "propeller_accessory_mass_kg",
            ),
            (
                self.nacelle_area_density_kg_m2,
                "nacelle_area_density_kg_m2",
            ),
            (self.pylon_coefficient, "pylon_coefficient"),
            (
                self.engine_installation_mass_kg,
                "engine_installation_mass_kg",
            ),
            (self.engine_oil_mass_kg, "engine_oil_mass_kg"),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(field);
            }
        }
        if let Some(mass_kg) = self.propeller_assembly_mass_kg {
            if !mass_kg.is_finite() || mass_kg <= 0.0 {
                return Err("propeller_assembly_mass_kg");
            }
        }
        if self.propeller_assembly_mass_kg.is_some() && self.propeller_accessory_mass_kg > 0.0 {
            match self.propeller_assembly_accessories_included {
                Some(false) => {}
                Some(true) => return Err("propeller_accessory_mass_kg"),
                None => return Err("propeller_assembly_accessories_included"),
            }
        }
        if self.propeller_assembly_mass_kg.is_none()
            && self.propeller_assembly_accessories_included.is_some()
        {
            return Err("propeller_assembly_mass_kg");
        }
        match (
            self.nacelle_reference_mass_kg,
            self.nacelle_reference_area_m2,
        ) {
            (Some(mass_kg), Some(area_m2)) => {
                if !mass_kg.is_finite() || mass_kg <= 0.0 {
                    return Err("nacelle_reference_mass_kg");
                }
                if !area_m2.is_finite() || area_m2 <= 0.0 {
                    return Err("nacelle_reference_area_m2");
                }
            }
            (None, None) => {}
            (Some(_), None) => return Err("nacelle_reference_area_m2"),
            (None, Some(_)) => return Err("nacelle_reference_mass_kg"),
        }
        if let Some(mass_kg) = self.engine_dry_mass_kg {
            if !mass_kg.is_finite() || mass_kg <= 0.0 {
                return Err("engine_dry_mass_kg");
            }
        }
        if let Some(power_kw) = self.baseline_shaft_power_kw {
            if !power_kw.is_finite() || power_kw <= 0.0 {
                return Err("baseline_shaft_power_kw");
            }
        }
        if let Some(coefficient) = self.propeller_weight_coefficient {
            if !coefficient.is_finite() || coefficient <= 0.0 {
                return Err("propeller_weight_coefficient");
            }
        }
        // The regression is fitted over four-blade-class propellers scaled by
        // (B/4)^0.7; a blade count beyond the published range is a declared
        // extrapolation rather than a silent one.
        if self.propeller_blade_count > 12 {
            return Err("propeller_blade_count");
        }
        Ok(())
    }
}

#[cfg(test)]
// A test asserts on a density it resolved from a configuration it built here,
// so a failed unwrap is the assertion failing, not a library invariant being
// broken.
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_default_node_declares_nothing_and_blocks_no_field() {
        let config = FlopsTurbopropConfig::default();
        assert!(config.is_default());
        assert_eq!(config.validate(), Ok(()));
        assert!(config.engine_dry_mass_kg.is_none());
        // Aluminium double-acting is the published default construction, so
        // its coefficient resolves without a declaration.
        assert_eq!(config.resolved_weight_coefficient(), Some(355.0));
        assert_eq!(config.resolved_nacelle_area_density_kg_m2(), Some(0.0));
    }

    #[test]
    fn a_nacelle_reference_pair_resolves_an_explicit_component_density() {
        let mut config = FlopsTurbopropConfig {
            nacelle_reference_mass_kg: Some(207.75),
            nacelle_reference_area_m2: Some(12.25),
            ..Default::default()
        };
        assert_eq!(config.validate(), Ok(()));
        assert!(
            (config.resolved_nacelle_area_density_kg_m2().unwrap() - 16.959_183_673_469_4).abs()
                < 1e-12
        );
        config.nacelle_reference_area_m2 = None;
        assert_eq!(config.validate(), Err("nacelle_reference_area_m2"));
    }

    #[test]
    fn a_composite_propeller_has_no_published_coefficient_until_one_is_declared() {
        let mut config = FlopsTurbopropConfig {
            propeller_construction: PropellerConstruction::Composite,
            ..FlopsTurbopropConfig::default()
        };
        assert_eq!(
            PropellerConstruction::Composite.published_weight_coefficient(),
            None
        );
        assert_eq!(config.resolved_weight_coefficient(), None);
        config.propeller_weight_coefficient = Some(170.0);
        assert_eq!(config.resolved_weight_coefficient(), Some(170.0));
        assert!(!PropellerConstruction::Composite.has_counterweights());
        assert!(PropellerConstruction::AluminiumSingleActing.has_counterweights());
    }

    #[test]
    fn nonphysical_declarations_are_named_rather_than_clamped() {
        let mut config = FlopsTurbopropConfig {
            engine_dry_mass_kg: Some(0.0),
            ..FlopsTurbopropConfig::default()
        };
        assert_eq!(config.validate(), Err("engine_dry_mass_kg"));
        config.engine_dry_mass_kg = Some(481.7);
        config.nacelle_area_density_kg_m2 = -1.0;
        assert_eq!(config.validate(), Err("nacelle_area_density_kg_m2"));
        config.nacelle_area_density_kg_m2 = 12.0;
        config.propeller_blade_count = 20;
        assert_eq!(config.validate(), Err("propeller_blade_count"));
    }
}
