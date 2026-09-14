// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Declared inputs for the NASA FLOPS airframe and propulsion mass equations.
//!
//! The authoritative [`crate::MassArchitecture`] selects the complete
//! production buildup. This node carries FLOPS technology factors and
//! overrides; the derived legacy method selectors remain only for schema
//! migration and explicit comparison compatibility, so no production run can
//! pair FLOPS airframe groups with legacy systems fractions.
//!
//! Every technology factor below is a FLOPS input variable
//! (NASA/TM-2017-219627 Vol. I, section 5.2-5.3 and Appendix D) with the
//! FLOPS default. The wing-bending factor, the landing-gear lengths and the
//! design landing weight are the quantities FLOPS itself estimates when the
//! user leaves them blank; the same rule applies here, with the estimate
//! named in the evaluation record.

use serde::{Deserialize, Serialize};

use crate::{ConfigNode, Kind, Leaf};

/// Versioned method used for the structural group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructuralMassMethod {
    /// Frozen `alas/physics/mass.py` Torenbeek wing, tail and fuselage
    /// methods with the landing gear as a fraction of takeoff mass.
    #[default]
    ReferenceCompatible,
    /// NASA FLOPS transport structural equations 10-71 of TM-2017-219627.
    FlopsTransportV1,
}

impl StructuralMassMethod {
    /// Whether this selection preserves the frozen Python structural mass.
    pub fn is_reference_compatible(&self) -> bool {
        matches!(self, Self::ReferenceCompatible)
    }
}

impl Leaf for StructuralMassMethod {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Str
    }
}

/// Versioned method used for the installed propulsion group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropulsionMassMethod {
    /// Frozen thrust-to-weight and installation-factor correlation.
    #[default]
    ReferenceCompatible,
    /// NASA FLOPS transport propulsion equations 73-92 of TM-2017-219627.
    FlopsTransportV1,
}

impl PropulsionMassMethod {
    /// Whether this selection preserves the frozen propulsion correlation.
    pub fn is_reference_compatible(&self) -> bool {
        matches!(self, Self::ReferenceCompatible)
    }
}

impl Leaf for PropulsionMassMethod {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Str
    }
}

/// Which FLOPS wing equivalent-bending-material factor is evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlopsWingBendingMethod {
    /// Equation 10: a trapezoidal wing with an average thickness ratio.
    #[default]
    Simplified,
    /// Equations 18-32: numerical integration of the bending material along
    /// the load path of the built planform, with engine inertia relief.
    Detailed,
}

impl FlopsWingBendingMethod {
    /// Stable serialized name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Simplified => "simplified",
            Self::Detailed => "detailed",
        }
    }
}

impl Leaf for FlopsWingBendingMethod {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Str
    }
}

/// Technology factors and declared overrides for the FLOPS airframe and
/// propulsion equations.
///
/// The defaults are the FLOPS defaults for a conventional metallic
/// transport, so an unmodified group evaluates the published baseline
/// correlation. Overrides that FLOPS accepts as user inputs are optional:
/// absent, the FLOPS estimate is used and the evaluation record says so.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(default, deny_unknown_fields)]
pub struct FlopsStructureConfig {
    /// Which wing bending-material factor is evaluated.
    #[config(
        advanced,
        options = FlopsWingBendingMethod,
        label = "Wing bending-factor method",
        help = "FLOPS wing equivalent bending material factor: the simplified trapezoidal fit of equation 10, or the detailed load-path integration of equations 18-32 over the built planform with engine inertia relief."
    )]
    pub wing_bending_method: FlopsWingBendingMethod,

    /// Composite utilization in the wing structure, FLOPS `FCOMP`.
    #[config(
        advanced,
        label = "Wing composite utilization",
        unit = "0-1",
        help = "FLOPS FCOMP: 0 for an all-metal wing structure, 1 for maximum use of composites. Reduces bending material by 40 percent, shear and control-surface material by 17 percent and miscellaneous items by 30 percent at full utilization."
    )]
    pub composite_utilization: f64,

    /// Aeroelastic tailoring factor, FLOPS `FAERT`.
    #[config(
        advanced,
        label = "Aeroelastic tailoring factor",
        unit = "0-1",
        help = "FLOPS FAERT: 0 for no aeroelastic tailoring, 1 for the maximum tailoring benefit in the wing bending material."
    )]
    pub aeroelastic_tailoring: f64,

    /// Strut bracing factor, FLOPS `FSTRT`.
    #[config(
        advanced,
        label = "Wing strut-bracing factor",
        unit = "0-1",
        help = "FLOPS FSTRT: 0 for a cantilever wing, 1 for full benefit from strut bracing."
    )]
    pub strut_bracing: f64,

    /// Fraction of the load carried by the wing, FLOPS `PCTL`.
    #[config(
        advanced,
        label = "Wing load fraction",
        unit = "0-1",
        help = "FLOPS PCTL: fraction of the aircraft load carried by the defined wing. 1 for a conventional single wing."
    )]
    pub wing_load_fraction: f64,

    /// Military cargo floor factor, FLOPS `CARGF`.
    #[config(
        advanced,
        label = "Military cargo floor factor",
        unit = "0-1",
        help = "FLOPS CARGF: 0 for a passenger transport fuselage, 1 for a military cargo transport floor (38 percent fuselage structure penalty)."
    )]
    pub military_cargo_floor: f64,

    /// Declared structural design gross mass, FLOPS `DG`.
    #[config(
        advanced,
        label = "Structural design gross mass",
        unit = "kg",
        help = "FLOPS DG for the wing, tail, fuselage, surface-control and pod-relief equations. Blank sizes at the takeoff mass of the case being evaluated: the declared MTOW of a fixed aircraft, or the closed takeoff mass while a clean-sheet design is being sized. Declare it when the structure was designed for a heavier weight variant than the MTOW in use."
    )]
    pub design_gross_mass_kg: Option<f64>,

    /// Declared design landing mass, FLOPS `WLDG`.
    #[config(
        advanced,
        label = "Design landing mass",
        unit = "kg",
        help = "FLOPS WLDG for the landing-gear equations. Blank uses the maximum landing mass fraction of the mass model applied to the design gross mass."
    )]
    pub design_landing_mass_kg: Option<f64>,

    /// Extended main-gear oleo length, FLOPS `XMLG`.
    #[config(
        advanced,
        label = "Main-gear oleo length",
        unit = "m",
        help = "FLOPS XMLG: length of the extended main landing-gear oleo. Blank uses the FLOPS estimate from nacelle diameter, wing dihedral, outboard engine position and fuselage width (equation 66)."
    )]
    pub main_gear_oleo_length_m: Option<f64>,

    /// Extended nose-gear oleo length, FLOPS `XNLG`.
    #[config(
        advanced,
        label = "Nose-gear oleo length",
        unit = "m",
        help = "FLOPS XNLG: length of the extended nose landing-gear oleo. Blank uses 70 percent of the main-gear length (equation 67)."
    )]
    pub nose_gear_oleo_length_m: Option<f64>,

    /// Paint area density, FLOPS `WPAINT`.
    #[config(
        advanced,
        label = "Paint area density",
        unit = "kg/m^2",
        help = "FLOPS WPAINT applied to the wetted area of wings, tails, fuselage and nacelles. The FLOPS default is zero."
    )]
    pub paint_area_density_kg_m2: f64,

    /// Baseline engine mass, FLOPS `WENGB`.
    #[config(
        advanced,
        label = "Baseline engine mass",
        unit = "kg",
        help = "FLOPS WENGB: dry mass of the baseline engine including inlet and nozzle. Blank uses the FLOPS transport estimate of baseline thrust over 5.5 (equation 76)."
    )]
    pub baseline_engine_mass_kg: Option<f64>,

    /// Baseline engine rated thrust, FLOPS `THRSO`.
    #[config(
        advanced,
        label = "Baseline engine thrust",
        unit = "kN",
        help = "FLOPS THRSO: sea-level static rated thrust of the baseline engine the mass is scaled from. Blank uses the installed engine's rated thrust, so the scaling ratio is one."
    )]
    pub baseline_engine_thrust_kn: Option<f64>,

    /// Engine mass scaling exponent, FLOPS `EEXP`.
    #[config(
        advanced,
        label = "Engine mass scaling exponent",
        help = "FLOPS EEXP: exponent on the thrust ratio when the baseline engine is scaled (equation 75). Values below 0.3 are treated as a linear mass-per-thrust slope. The FLOPS default is 1.15."
    )]
    pub engine_mass_scaling_exponent: f64,

    /// Baseline inlet mass, FLOPS `WINLB`.
    #[config(
        advanced,
        label = "Baseline inlet mass",
        unit = "kg",
        help = "FLOPS WINLB: inlet mass of the baseline engine, declared separately from the baseline engine mass (equation 77). Blank means the inlet is already inside the baseline engine mass, which is what an engine catalogue dry mass normally quotes. Declaring it requires an explicit baseline engine mass, so the inlet is not counted twice."
    )]
    pub baseline_inlet_mass_kg: Option<f64>,

    /// Inlet mass scaling exponent, FLOPS `EINL`.
    #[config(
        advanced,
        label = "Inlet mass scaling exponent",
        help = "FLOPS EINL: exponent on the thrust ratio when the baseline inlet is scaled (equation 77). The FLOPS default is 1."
    )]
    pub inlet_mass_scaling_exponent: f64,

    /// Baseline nozzle mass, FLOPS `WNOZB`.
    #[config(
        advanced,
        label = "Baseline nozzle mass",
        unit = "kg",
        help = "FLOPS WNOZB: nozzle mass of the baseline engine, declared separately from the baseline engine mass (equation 78). Blank means the nozzle is already inside the baseline engine mass. Declaring it requires an explicit baseline engine mass, so the nozzle is not counted twice."
    )]
    pub baseline_nozzle_mass_kg: Option<f64>,

    /// Nozzle mass scaling exponent, FLOPS `ENOZ`.
    #[config(
        advanced,
        label = "Nozzle mass scaling exponent",
        help = "FLOPS ENOZ: exponent on the thrust ratio when the baseline nozzle is scaled (equation 78). The FLOPS default is 1."
    )]
    pub nozzle_mass_scaling_exponent: f64,

    /// Whether thrust reversers are installed.
    #[config(
        advanced,
        label = "Thrust reversers installed",
        help = "Include the FLOPS thrust-reverser mass of 3.4 percent of rated thrust per nacelle (equation 86). Disable for an installation without reversers."
    )]
    pub thrust_reversers_installed: bool,

    /// Additional miscellaneous propulsion mass, FLOPS `WPMISC`.
    #[config(
        advanced,
        label = "Miscellaneous propulsion mass",
        unit = "kg",
        help = "FLOPS WPMISC: declared propulsion-system mass added to the engine controls and starters (equation 91)."
    )]
    pub misc_propulsion_mass_kg: f64,

    /// Empty-mass margin as a fraction of the computed empty mass.
    #[config(
        advanced,
        label = "Empty-mass margin fraction",
        unit = "0-1",
        help = "FLOPS WMARG expressed as a fraction of the computed structural, propulsion and systems mass, added to the empty mass (equation 139). Zero adds no margin."
    )]
    pub empty_mass_margin_fraction: f64,
}

impl Default for FlopsStructureConfig {
    fn default() -> Self {
        Self {
            wing_bending_method: FlopsWingBendingMethod::Simplified,
            composite_utilization: 0.0,
            aeroelastic_tailoring: 0.0,
            strut_bracing: 0.0,
            wing_load_fraction: 1.0,
            military_cargo_floor: 0.0,
            design_gross_mass_kg: None,
            design_landing_mass_kg: None,
            main_gear_oleo_length_m: None,
            nose_gear_oleo_length_m: None,
            paint_area_density_kg_m2: 0.0,
            baseline_engine_mass_kg: None,
            baseline_engine_thrust_kn: None,
            engine_mass_scaling_exponent: 1.15,
            baseline_inlet_mass_kg: None,
            inlet_mass_scaling_exponent: 1.0,
            baseline_nozzle_mass_kg: None,
            nozzle_mass_scaling_exponent: 1.0,
            thrust_reversers_installed: true,
            misc_propulsion_mass_kg: 0.0,
            empty_mass_margin_fraction: 0.0,
        }
    }
}

impl FlopsStructureConfig {
    /// Whether the group equals the FLOPS defaults.
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Reject values outside the ranges the FLOPS equations are fitted over.
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("composite_utilization", self.composite_utilization),
            ("aeroelastic_tailoring", self.aeroelastic_tailoring),
            ("strut_bracing", self.strut_bracing),
            ("wing_load_fraction", self.wing_load_fraction),
            ("military_cargo_floor", self.military_cargo_floor),
            (
                "empty_mass_margin_fraction",
                self.empty_mass_margin_fraction,
            ),
        ] {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(format!("FLOPS {name} must lie in [0, 1]"));
            }
        }
        for (name, value) in [
            ("paint_area_density_kg_m2", self.paint_area_density_kg_m2),
            ("misc_propulsion_mass_kg", self.misc_propulsion_mass_kg),
            (
                "engine_mass_scaling_exponent",
                self.engine_mass_scaling_exponent,
            ),
            (
                "inlet_mass_scaling_exponent",
                self.inlet_mass_scaling_exponent,
            ),
            (
                "nozzle_mass_scaling_exponent",
                self.nozzle_mass_scaling_exponent,
            ),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(format!("FLOPS {name} must be finite and nonnegative"));
            }
        }
        for (name, value) in [
            ("design_gross_mass_kg", self.design_gross_mass_kg),
            ("design_landing_mass_kg", self.design_landing_mass_kg),
            ("main_gear_oleo_length_m", self.main_gear_oleo_length_m),
            ("nose_gear_oleo_length_m", self.nose_gear_oleo_length_m),
            ("baseline_engine_mass_kg", self.baseline_engine_mass_kg),
            ("baseline_engine_thrust_kn", self.baseline_engine_thrust_kn),
        ] {
            if let Some(value) = value {
                if !value.is_finite() || value <= 0.0 {
                    return Err(format!("FLOPS {name} must be positive when declared"));
                }
            }
        }
        for (name, value) in [
            ("baseline_inlet_mass_kg", self.baseline_inlet_mass_kg),
            ("baseline_nozzle_mass_kg", self.baseline_nozzle_mass_kg),
        ] {
            if let Some(value) = value {
                if !value.is_finite() || value < 0.0 {
                    return Err(format!(
                        "FLOPS {name} must be finite and nonnegative when declared"
                    ));
                }
            }
        }
        // NASA/TM-2017-219627 Vol. I is explicit that `WENGB` "includes inlet
        // and nozzle weight if they are not specified separately", and the
        // equation 76 fallback `THRSO / 5.5` is such an all-in baseline.
        // Declaring a separate inlet or nozzle against that fallback would
        // add mass the baseline already contains, so an explicit baseline
        // engine mass is required before either may be declared.
        if (self.baseline_inlet_mass_kg.is_some() || self.baseline_nozzle_mass_kg.is_some())
            && self.baseline_engine_mass_kg.is_none()
        {
            return Err(
                "FLOPS baseline_engine_mass_kg must be declared before a separate \
                 baseline_inlet_mass_kg or baseline_nozzle_mass_kg, because the equation 76 \
                 estimate already includes the inlet and nozzle"
                    .to_owned(),
            );
        }
        Ok(())
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reference_methods_are_the_backward_compatible_defaults() {
        assert!(StructuralMassMethod::default().is_reference_compatible());
        assert!(PropulsionMassMethod::default().is_reference_compatible());
        assert_eq!(
            FlopsWingBendingMethod::default(),
            FlopsWingBendingMethod::Simplified
        );
    }

    #[test]
    fn the_default_group_is_the_flops_metallic_transport_baseline() {
        let config = FlopsStructureConfig::default();
        assert!(config.is_default());
        assert!(config.validate().is_ok());
        assert_eq!(config.engine_mass_scaling_exponent, 1.15);
        assert_eq!(config.wing_load_fraction, 1.0);
        assert!(config.thrust_reversers_installed);
    }

    #[test]
    fn a_technology_factor_outside_its_fitted_range_is_rejected() {
        let config = FlopsStructureConfig {
            composite_utilization: 1.5,
            ..Default::default()
        };
        assert!(config.validate().is_err());
        let declared = FlopsStructureConfig {
            baseline_engine_thrust_kn: Some(0.0),
            ..Default::default()
        };
        assert!(declared.validate().is_err());
    }

    #[test]
    fn the_separate_inlet_and_nozzle_are_absent_by_default_and_scale_linearly() {
        // Equations 77-78 default `EINL` and `ENOZ` to 1; equation 80 (no
        // separate items) stays the default path, so an unmodified group is
        // still the catalogue-dry-mass baseline.
        let config = FlopsStructureConfig::default();
        assert_eq!(config.baseline_inlet_mass_kg, None);
        assert_eq!(config.baseline_nozzle_mass_kg, None);
        assert_eq!(config.inlet_mass_scaling_exponent, 1.0);
        assert_eq!(config.nozzle_mass_scaling_exponent, 1.0);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn a_separate_inlet_or_nozzle_requires_a_declared_baseline_engine_mass() {
        // The equation 76 estimate `THRSO / 5.5` already includes the inlet
        // and nozzle, so adding them on top of it would double count.
        let orphan = FlopsStructureConfig {
            baseline_inlet_mass_kg: Some(150.0),
            ..Default::default()
        };
        assert!(orphan.validate().is_err());
        let orphan_nozzle = FlopsStructureConfig {
            baseline_nozzle_mass_kg: Some(90.0),
            ..Default::default()
        };
        assert!(orphan_nozzle.validate().is_err());
        let declared = FlopsStructureConfig {
            baseline_engine_mass_kg: Some(3_000.0),
            baseline_inlet_mass_kg: Some(150.0),
            baseline_nozzle_mass_kg: Some(90.0),
            ..Default::default()
        };
        assert!(declared.validate().is_ok());
        // Nonfinite or negative values are rejected on their own.
        for bad in [
            FlopsStructureConfig {
                baseline_engine_mass_kg: Some(3_000.0),
                baseline_inlet_mass_kg: Some(-1.0),
                ..Default::default()
            },
            FlopsStructureConfig {
                baseline_engine_mass_kg: Some(3_000.0),
                baseline_nozzle_mass_kg: Some(f64::NAN),
                ..Default::default()
            },
            FlopsStructureConfig {
                inlet_mass_scaling_exponent: -0.5,
                ..Default::default()
            },
            FlopsStructureConfig {
                nozzle_mass_scaling_exponent: f64::INFINITY,
                ..Default::default()
            },
        ] {
            assert!(bad.validate().is_err());
        }
    }

    #[test]
    fn a_saved_configuration_without_the_inlet_and_nozzle_fields_still_loads() {
        // Backward compatibility: every configuration written before the
        // separate inlet and nozzle existed must deserialize to the
        // equation 80 defaults, not fail on a missing field.
        let legacy = serde_json::json!({
            "wing_bending_method": "simplified",
            "composite_utilization": 0.2,
            "engine_mass_scaling_exponent": 1.15,
            "baseline_engine_mass_kg": 3_000.0
        });
        let loaded: FlopsStructureConfig =
            serde_json::from_value(legacy).expect("a pre-existing configuration still loads");
        assert_eq!(loaded.baseline_inlet_mass_kg, None);
        assert_eq!(loaded.baseline_nozzle_mass_kg, None);
        assert_eq!(loaded.inlet_mass_scaling_exponent, 1.0);
        assert_eq!(loaded.nozzle_mass_scaling_exponent, 1.0);
        assert_eq!(loaded.composite_utilization, 0.2);
        assert!(loaded.validate().is_ok());

        // And a round trip of the new fields is stable.
        let declared = FlopsStructureConfig {
            baseline_engine_mass_kg: Some(3_000.0),
            baseline_inlet_mass_kg: Some(150.0),
            baseline_nozzle_mass_kg: Some(90.0),
            inlet_mass_scaling_exponent: 0.8,
            nozzle_mass_scaling_exponent: 1.2,
            ..Default::default()
        };
        let round_trip: FlopsStructureConfig =
            serde_json::from_value(serde_json::to_value(&declared).expect("serializes"))
                .expect("deserializes");
        assert_eq!(round_trip, declared);
    }

    #[test]
    fn the_method_names_are_stable_in_saved_configuration() {
        assert_eq!(
            serde_json::to_value(StructuralMassMethod::FlopsTransportV1).ok(),
            Some(serde_json::json!("flops_transport_v1"))
        );
        assert_eq!(
            serde_json::to_value(PropulsionMassMethod::ReferenceCompatible).ok(),
            Some(serde_json::json!("reference_compatible"))
        );
        assert_eq!(
            serde_json::to_value(FlopsWingBendingMethod::Detailed).ok(),
            Some(serde_json::json!("detailed"))
        );
    }
}
