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
//! FLOPS default. `FCOMP` is an empirical composite-utilization coefficient,
//! not a measured percentage of composite structural mass; a zero value is
//! the published metallic-equation endpoint and does not prove that a named
//! aircraft is all-metal. The wing-bending factor, the landing-gear lengths
//! and the design landing weight are the quantities FLOPS itself estimates
//! when the user leaves them blank; the same rule applies here, with the
//! estimate named in the evaluation record.

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

/// Which method prices the engine pylons of a podded installation.
///
/// **FLOPS has no pylon term at all.** Every propulsion mass in
/// NASA/TM-2017-219627 Vol. I sections 5.2.9 and 5.3 is an engine, a nacelle,
/// a reverser, a control, a starter or a fuel system; searching the published
/// equation set for a strut or pylon returns nothing, and equation 137 sums
/// only those groups. The structure that carries a podded engine to the wing
/// is therefore outside the published empty-weight boundary, not estimated at
/// zero by it.
///
/// That gap is not small. Inverting the published computed masses and
/// deviations of Fernandes da Moura (2001) against three independent methods
/// gives an actual pylon mass of **469 kg per pylon on the A320-200** and
/// **724 kg per pylon on the A340-300**, i.e. **2.27 % and 2.23 % of operating
/// empty weight**: the whole of ALAS's A320 deficit and about a sixth of the
/// A340's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PylonMassMethod {
    /// Charge no pylon, which is the published FLOPS boundary exactly.
    ///
    /// This is the auditable baseline: it reproduces the transport equation
    /// set as printed, and it is also the physically right answer for an
    /// installation with no pylon at all, such as a wing-faired turboprop
    /// nacelle.
    #[default]
    None,
    /// The LTH box-beam pylon relation, `m = n x 0.2648 x SLST^0.6517` with
    /// the sea-level static thrust of one engine in newtons and the mass in
    /// kilograms.
    ///
    /// Source: Luftfahrttechnisches Handbuch, Masseanalyse MA 401 12-01 B
    /// (Dorbath, 2013), whose stated validity is *"grosse zivile
    /// Verkehrsflugzeuge (MTOM > 40 t)"* and *"bezieht sich ausschliesslich
    /// auf zivile Verkehrsflugzeuge"*. Against the two pylon masses derived
    /// above it returns 515 kg (+9.8 %) and 625 kg (-13.7 %) per pylon.
    ///
    /// It prices the wing pylons of a podded installation and nothing else: a
    /// tail-mounted centre engine is carried by fuselage and fin structure
    /// that this relation was not fitted on, so it is not charged one.
    LthBoxBeamV1,
}

impl PylonMassMethod {
    /// Stable serialized name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::LthBoxBeamV1 => "lth_box_beam_v1",
        }
    }
}

impl Leaf for PylonMassMethod {
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

/// How the source engine mass treats the starting system that FLOPS equation
/// 89 prices separately.
///
/// A certified dry mass can include a starter, while the published FLOPS
/// equation still adds one unless the user declares the scope.  Keeping that
/// decision as an enum makes an unresolved data-sheet scope visible and lets
/// the conservative branch retain the published starter term without
/// pretending the overlap has been measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlopsStarterScope {
    /// Use the published FLOPS equation 89 as a separate term.
    #[default]
    SeparateEquation89,
    /// The declared baseline engine mass already contains the starter.
    IncludedInBaseline,
    /// The source places the starter hardware in the engine type design, but
    /// does not establish that every component of FLOPS' broader starter
    /// *system* is inside the quoted mass. Keep equation 89 conservatively.
    HardwareIncludedSystemUnresolved,
    /// The source does not resolve the scope; keep equation 89 so the model
    /// does not silently understate the installation, and retain this status
    /// for provenance.
    UnknownConservativeSeparate,
}

impl FlopsStarterScope {
    /// Stable serialized name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SeparateEquation89 => "separate_equation_89",
            Self::IncludedInBaseline => "included_in_baseline",
            Self::HardwareIncludedSystemUnresolved => "hardware_included_system_unresolved",
            Self::UnknownConservativeSeparate => "unknown_conservative_separate",
        }
    }

    /// Whether equation 89 contributes a starter mass under this scope.
    pub const fn includes_equation_89(self) -> bool {
        !matches!(self, Self::IncludedInBaseline)
    }
}

impl Leaf for FlopsStarterScope {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Str
    }
}

/// How the source engine mass treats the exhaust nozzle relative to FLOPS
/// equations 77-80.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlopsNozzleScope {
    /// The baseline `WENGB` includes the nozzle; equation 80 is used.
    #[default]
    IncludedInBaseline,
    /// A separately declared `WNOZB` is scaled with equation 78.
    SeparateEquation78,
    /// The source excludes the nozzle from the baseline, but no retained
    /// FLOPS-compatible term prices it.  The resulting omission is explicit.
    OutsideUnmodelled,
    /// The source does not establish whether the nozzle is inside the
    /// baseline.  No mass is invented and no separate term is evaluated.
    Unknown,
}

impl FlopsNozzleScope {
    /// Stable serialized name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::IncludedInBaseline => "included_in_baseline",
            Self::SeparateEquation78 => "separate_equation_78",
            Self::OutsideUnmodelled => "outside_unmodelled",
            Self::Unknown => "unknown",
        }
    }
}

impl Leaf for FlopsNozzleScope {
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

    /// Composite-utilization coefficient in the FLOPS wing fit, `FCOMP`.
    ///
    /// This is not a percentage of the aircraft's composite material. NASA's
    /// Appendix D defines an empirical technology coefficient between the metallic
    /// endpoint and the maximum composite benefit represented by the fits;
    /// no retained aircraft source maps a material percentage to it.
    #[config(
        advanced,
        label = "Wing composite utilization",
        unit = "0-1",
        help = "FLOPS FCOMP is an empirical technology coefficient, not a composite material percentage. 0 is the published metallic-equation endpoint and does not prove a named aircraft is all-metal; 1 is the maximum composite benefit represented by the fits. At fixed geometry it multiplies bending, shear/control and miscellaneous wing terms by (1 - 0.4 FCOMP), (1 - 0.17 FCOMP) and (1 - 0.3 FCOMP)."
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
        help = "FLOPS WPAINT applied to the wetted area of wings, tails, fuselage and nacelles. NASA's own validated decks declare 0.037 lbm/ft^2 (0.1807 kg/m^2) and 0.07 lbm/ft^2; the published equation set has no default, and zero means an unpainted aircraft."
    )]
    pub paint_area_density_kg_m2: f64,

    /// Which method prices the engine pylons, which FLOPS itself does not.
    #[config(
        advanced,
        options = PylonMassMethod,
        label = "Pylon mass method",
        help = "The published FLOPS transport equations have no pylon term, so a podded installation is missing the structure that carries its engines. Select the LTH box-beam relation to charge it from the engine's sea-level static thrust, or leave it off to reproduce the published FLOPS boundary exactly."
    )]
    pub pylon_mass_method: PylonMassMethod,

    /// Baseline engine mass, FLOPS `WENGB`.
    #[config(
        advanced,
        label = "Baseline engine mass",
        unit = "kg",
        help = "FLOPS WENGB: declared dry mass of the baseline engine. Whether inlet/nozzle and starter hardware are inside the source boundary is recorded by the component-scope controls below; blank uses the FLOPS transport estimate of baseline thrust over 5.5 (equation 76)."
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

    /// Scope of the engine starting system relative to FLOPS equation 89.
    #[config(
        advanced,
        options = FlopsStarterScope,
        label = "Engine starter scope",
        help = "State whether the declared engine dry mass already includes the starter system. Separate equation 89 is the FLOPS default; included_in_baseline suppresses the separate term only when the full system boundary is proven; hardware_included_system_unresolved and unknown_conservative_separate retain equation 89 while exposing the unresolved overlap."
    )]
    pub starter_scope: FlopsStarterScope,

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

    /// Scope of the engine exhaust nozzle relative to FLOPS equations 77-80.
    #[config(
        advanced,
        options = FlopsNozzleScope,
        label = "Engine nozzle scope",
        help = "State whether the baseline engine mass includes the nozzle, declares it as a separate equation 78 term, or leaves it outside/unknown. Outside and unknown scopes do not invent a nozzle mass; they retain the omission status explicitly."
    )]
    pub nozzle_scope: FlopsNozzleScope,

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
            pylon_mass_method: PylonMassMethod::None,
            baseline_engine_mass_kg: None,
            baseline_engine_thrust_kn: None,
            engine_mass_scaling_exponent: 1.15,
            starter_scope: FlopsStarterScope::SeparateEquation89,
            baseline_inlet_mass_kg: None,
            inlet_mass_scaling_exponent: 1.0,
            baseline_nozzle_mass_kg: None,
            nozzle_scope: FlopsNozzleScope::IncludedInBaseline,
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

    /// Interpret the FCOMP value for reports and exports.
    ///
    /// A zero value is an explicit metallic-equation baseline when no
    /// aircraft-specific mapping exists; it is not evidence that the named
    /// aircraft contains no composite structure. Nonzero values remain
    /// declared FLOPS coefficients and must not be relabelled as material
    /// percentages.
    pub const fn composite_utilization_interpretation(&self) -> &'static str {
        if self.composite_utilization == 0.0 {
            "declared FLOPS metallic-equation baseline; not an aircraft material percentage"
        } else {
            "declared FLOPS empirical technology coefficient; not an aircraft material percentage"
        }
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
        match (self.nozzle_scope, self.baseline_nozzle_mass_kg) {
            (FlopsNozzleScope::SeparateEquation78, None) => {
                return Err(
                    "FLOPS nozzle_scope=separate_equation_78 requires baseline_nozzle_mass_kg"
                        .to_owned(),
                );
            }
            (FlopsNozzleScope::SeparateEquation78, Some(_)) => {}
            (_, Some(_)) => {
                return Err(
                    "FLOPS baseline_nozzle_mass_kg requires nozzle_scope=separate_equation_78"
                        .to_owned(),
                );
            }
            (_, None) => {}
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
        assert_eq!(
            config.composite_utilization_interpretation(),
            "declared FLOPS metallic-equation baseline; not an aircraft material percentage"
        );
        assert!(config.thrust_reversers_installed);
        assert_eq!(config.starter_scope, FlopsStarterScope::SeparateEquation89);
        assert_eq!(config.nozzle_scope, FlopsNozzleScope::IncludedInBaseline);
    }

    #[test]
    fn a_technology_factor_outside_its_fitted_range_is_rejected() {
        let config = FlopsStructureConfig {
            composite_utilization: 1.5,
            ..Default::default()
        };
        assert!(config.validate().is_err());
        let declared_composite = FlopsStructureConfig {
            composite_utilization: 0.5,
            ..Default::default()
        };
        assert_eq!(
            declared_composite.composite_utilization_interpretation(),
            "declared FLOPS empirical technology coefficient; not an aircraft material percentage"
        );
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
            nozzle_scope: FlopsNozzleScope::SeparateEquation78,
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
                nozzle_scope: FlopsNozzleScope::SeparateEquation78,
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
            nozzle_scope: FlopsNozzleScope::SeparateEquation78,
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
