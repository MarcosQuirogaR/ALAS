// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Shaft-power propulsion-group mass for a turboprop installation.
//!
//! # Why this exists
//!
//! NASA/TM-2017-219627 Vol. I, the FLOPS source the production architecture
//! evaluates, has **no** turboprop branch. Every propulsion mass in sections
//! 5.2.9 and 5.3 is parameterised on sea-level-static thrust: the nacelle
//! (equation 69), the engine (75-80, whose transport default is `THRSO/5.5`),
//! the thrust reversers (86), the engine controls (87), the starters (89) and,
//! among the operating items, the unusable fuel (121) and the engine oil
//! (122). Searching the published text for "propeller", "turboprop" or "shaft
//! horsepower" returns nothing at all, and NASA Aviary — the reference
//! implementation of the same source — has no propeller or gearbox component
//! in `mass/flops_based/` either.
//!
//! A turboprop has no rated thrust to feed those equations. Manufacturing one
//! from shaft power would put a quantity into the model that the aircraft does
//! not have, and every downstream consumer would read it as a jet thrust. This
//! module instead evaluates the propulsion group from shaft power, and the
//! FLOPS airframe, systems and passenger-driven operating items are left
//! untouched.
//!
//! # Ownership crosswalk
//!
//! Exactly one method owns each component, and nothing is estimated twice:
//!
//! | Component | Method | Source |
//! |---|---|---|
//! | Engine (turbomachine and reduction gearbox) | declared certificated dry mass scaled on shaft power, else the GASP turboshaft specific weight `0.5 lb/hp` | NASA CR-152303 Vol. V eq. V.1.3-V.1.4, p. V-1.4 |
//! | Reduction gearbox | inside the declared engine mass for a PW100-class engine; otherwise the torque relation | EASA TCDS IM.E.041 §III.2; NASA TM-83458 p. 14 eq. 3B |
//! | Propeller | Hamilton Standard regression | NASA CR-152303 Vol. V eq. V.1.28-V.1.29, pp. V-1.10 to V-1.12; NASA TM-83458 p. 5 |
//! | Spinner, blade de-icing, governor | declared, because the regression excludes them verbatim | NASA CR-152303 Vol. V p. V-1.11 |
//! | Nacelle | area density times nacelle wetted area | NASA CR-152303 Vol. V eq. V.1.6, p. V-1.5 |
//! | Pylon | `F_PYL (W_ENG + W_NAC)^0.736` | NASA CR-152303 Vol. V eq. V.1.7, p. V-1.5 |
//! | Thrust reversers | none; a turboprop reverses by blade pitch, and that hardware is already inside the propeller regression's double-acting/reversing population | — |
//! | Engine controls, starters, mounts, fire protection | declared installation mass | declared input |
//! | Fuel system, tanks and plumbing | FLOPS equation 92, which reads capacity, engine count and Mach and has no thrust term | NASA/TM-2017-219627 eq. 92 |
//! | Unusable fuel | FLOPS **alternate** equation 161, `0.0084 x FMXTOT`, which has no thrust term | NASA/TM-2017-219627 eq. 161 |
//! | Engine oil | declared; see below | — |
//!
//! ## Why the engine oil is declared rather than taken from equation 162
//!
//! FLOPS equation 122 reads a thrust, so it cannot be used here. The same
//! document's alternate equation 162 is thrust-free, but as printed
//! (`WOIL = 240 (NPASS + 39) / 40`, p. 56) it returns 1,248 lb of engine oil
//! for the 169-passenger `LargeSingleAisle1` case against 130.23 lb from the
//! default equation 122 for the same aircraft — a factor of ten, and far
//! above any real transport's oil charge. The printed alternate constant is
//! therefore not usable, and the oil is a declared input instead of a wrong
//! one. Left undeclared it is zero and visible as an accounting gap; on a
//! turboprop of this class the quantity is a few tens of kilograms.

use alas_config::{FlopsTurbopropConfig, PropellerConstruction};
use alas_units::{FOOT, POUND_MASS};

use super::propulsion::fuel_system_kg;

const WATTS_PER_SHP: f64 = 745.699_872;

fn kg(pounds: f64) -> f64 {
    pounds * POUND_MASS
}

fn lb(kilograms: f64) -> f64 {
    kilograms / POUND_MASS
}

/// A finite, strictly positive declared quantity. A `NaN` is not positive, so
/// it is refused by name rather than propagating through a power law.
fn is_positive(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

/// The GASP turboshaft and turboprop specific weight, lb per shaft
/// horsepower, of NASA CR-152303 Vol. V equation V.1.3 (`NTYEX = 5, 6`).
///
/// GASP is a general-aviation synthesis program and this is a
/// general-aviation regression; it is the fallback when no certificated dry
/// mass is declared, and the evaluation record says which was used.
pub const GASP_TURBOSHAFT_SPECIFIC_WEIGHT_LB_PER_HP: f64 = 0.5;

/// Equations V.1.3 and V.1.4: engine mass for one installed engine, kg.
///
/// With a declared certificated dry mass the mass is scaled on the
/// shaft-power ratio, which is unity when the engine runs at the rating its
/// mass was measured at. Without one the GASP specific weight is applied to
/// the maximum sea-level-static shaft power.
pub fn engine_mass_each_kg(
    takeoff_shaft_power_w: f64,
    baseline_shaft_power_w: f64,
    declared_dry_mass_kg: Option<f64>,
    scaling_exponent: f64,
) -> f64 {
    match declared_dry_mass_kg {
        Some(mass_kg) if baseline_shaft_power_w > 0.0 => {
            mass_kg * (takeoff_shaft_power_w / baseline_shaft_power_w).powf(scaling_exponent)
        }
        Some(mass_kg) => mass_kg,
        None => {
            kg(GASP_TURBOSHAFT_SPECIFIC_WEIGHT_LB_PER_HP * takeoff_shaft_power_w / WATTS_PER_SHP)
        }
    }
}

/// Equation V.1.11: propeller shaft torque, N*m, from shaft power and speed.
pub fn shaft_torque_n_m(shaft_power_w: f64, propeller_speed_rpm: f64) -> f64 {
    if propeller_speed_rpm <= 0.0 {
        return 0.0;
    }
    shaft_power_w / (2.0 * std::f64::consts::PI * propeller_speed_rpm / 60.0)
}

/// NASA TM-83458 p. 14 equation 3B with the p. 16 defaults: reduction-gearbox
/// mass, kg, from output torque and gear ratio.
///
/// The stated validity domain (p. 6) is *in-line* gearboxes in the 1,000 to
/// 2,500 horsepower range, and the document itself calls the result a rough
/// estimate. It is evaluated only when the declared engine mass does not
/// already contain the gearbox; a PW100-class certificated dry weight does
/// (EASA TCDS IM.E.041 §III.2), and evaluating both would count it twice.
pub fn gearbox_mass_kg(torque_n_m: f64, gear_ratio: f64) -> f64 {
    if gear_ratio <= 0.0 {
        return 0.0;
    }
    // 1 N*m = 0.737 562 149 277 ft*lbf; the relation is printed in imperial.
    let torque_ft_lbf = torque_n_m * 0.737_562_149_277_265;
    kg((0.0174 * torque_ft_lbf + 45.0) * (0.118 / gear_ratio).sqrt())
}

/// The Hamilton Standard regression constants selected by a construction.
///
/// `K_w` comes from the configuration because a composite propeller has no
/// single published value; `u`, `v` and the counterweight coefficient `y` are
/// the published exponents for the pitch-change hardware.
fn propeller_exponents(construction: PropellerConstruction) -> (f64, f64, f64) {
    match construction {
        // NASA TM-83458 p. 5, double-acting form.
        PropellerConstruction::AluminiumDoubleActing => (0.75, 0.5, 0.0),
        // NASA TM-83458 p. 5, single-acting counterweighted form with C_w.
        PropellerConstruction::AluminiumSingleActing => (0.7, 0.4, 5.0),
        // A composite blade is a material substitution into the same
        // regression: TM-83458 p. 5 sanctions K_w in 160-180 for "advanced
        // technology fiberglass or composite propellers" without changing the
        // exponents. The 568F-1 is a double-acting, full-feathering,
        // reversing propeller, so it takes the double-acting exponents.
        PropellerConstruction::Composite => (0.75, 0.5, 0.0),
    }
}

/// Equations V.1.28 and V.1.29: the wet mass of one propeller, kg.
///
/// The regression excludes the spinner, the blade de-icing and the governor
/// (NASA CR-152303 Vol. V p. V-1.11, verbatim); those are added by the caller
/// from the declared accessory mass.
///
/// `design_mach` is the maximum-power cruise Mach number the regression's
/// `(M + 1)^0.5` term reads.
#[allow(clippy::too_many_arguments)]
pub fn propeller_mass_each_kg(
    weight_coefficient: f64,
    diameter_m: f64,
    blade_count: u32,
    activity_factor: f64,
    propeller_speed_rpm: f64,
    shaft_power_w: f64,
    design_mach: f64,
    construction: PropellerConstruction,
) -> f64 {
    let (activity_exponent, speed_exponent, counterweight_coefficient) =
        propeller_exponents(construction);
    let diameter_ft = diameter_m / FOOT;
    let blades = f64::from(blade_count);
    let shaft_horsepower = shaft_power_w / WATTS_PER_SHP;
    // `N D` with N in rev/min and D in ft, referenced to 20,000 as printed.
    let speed_diameter = propeller_speed_rpm * diameter_ft;
    let disk_power_loading = shaft_horsepower / (10.0 * diameter_ft * diameter_ft);
    let base_lb = weight_coefficient
        * (diameter_ft / 10.0).powi(2)
        * (blades / 4.0).powf(0.7)
        * (activity_factor / 100.0).powf(activity_exponent)
        * (speed_diameter / 20_000.0).powf(speed_exponent)
        * (design_mach + 1.0).sqrt()
        * disk_power_loading.powf(0.12);
    let counterweight_lb = if counterweight_coefficient > 0.0 {
        counterweight_coefficient
            * (diameter_ft / 10.0).powi(2)
            * blades
            * (activity_factor / 100.0).powi(2)
            * (20_000.0 / speed_diameter).powf(0.3)
    } else {
        0.0
    };
    kg(base_lb + counterweight_lb)
}

/// Equation V.1.6: nacelle mass for one nacelle, kg.
pub fn nacelle_mass_each_kg(area_density_kg_m2: f64, nacelle_wetted_area_m2: f64) -> f64 {
    area_density_kg_m2 * nacelle_wetted_area_m2
}

/// Equation V.1.7: pylon mass for one engine pod, kg, from the engine and
/// nacelle mass it carries. The relation is printed in pounds.
pub fn pylon_mass_each_kg(
    pylon_coefficient: f64,
    engine_mass_kg: f64,
    nacelle_mass_kg: f64,
) -> f64 {
    if pylon_coefficient <= 0.0 {
        return 0.0;
    }
    kg(pylon_coefficient * lb(engine_mass_kg + nacelle_mass_kg).powf(0.736))
}

/// FLOPS alternate equation 161: unusable fuel, kg, from fuel capacity alone.
///
/// The default equation 121 reads a rated thrust and therefore has no
/// turboprop form; equation 161 estimates the same quantity from the fuel
/// capacity with no thrust term, so the substitution stays inside the same
/// published document.
pub fn unusable_fuel_kg(maximum_fuel_capacity_kg: f64) -> f64 {
    0.0084 * maximum_fuel_capacity_kg
}

/// SI inputs to the shaft-power propulsion group.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurbopropPropulsionInputs {
    /// Total installed engines.
    pub engine_count: usize,
    /// Take-off shaft power of one installed engine, W.
    pub takeoff_shaft_power_per_engine_w: f64,
    /// Governed propeller speed, rev/min.
    pub propeller_speed_rpm: f64,
    /// Propeller diameter, m.
    pub propeller_diameter_m: f64,
    /// Engine-to-propeller reduction ratio, engine speed over propeller
    /// speed, used only when the gearbox is charged separately.
    pub reduction_ratio: f64,
    /// Maximum-power cruise Mach number the regression's `(M + 1)^0.5` reads.
    ///
    /// NASA CR-152303 Vol. V p. V-1.11 defines this as the Mach number at
    /// the maximum-power cruise design condition. The product adapter passes
    /// the declared maximum Mach `VMAX`, which is an upper bound on it: for
    /// the ATR 72-600 that is 0.55 against a 275 KTAS cruise near Mach 0.45,
    /// and the term moves the propeller mass by under four percent across
    /// that range.
    pub design_mach: f64,
    /// Wetted area of one nacelle, m^2.
    pub nacelle_wetted_area_m2: f64,
    /// Maximum usable fuel capacity `FMXTOT`, kg.
    pub maximum_fuel_capacity_kg: f64,
}

/// The evaluated shaft-power propulsion group, kg.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurbopropPropulsionBreakdown {
    /// One engine, turbomachine and — for a PW100-class declaration — its
    /// reduction gearbox, kg.
    pub engine_each_kg: f64,
    /// Every installed engine, kg.
    pub engines_kg: f64,
    /// Reduction gearboxes charged separately, kg. Zero when the declared
    /// engine mass already contains them.
    pub gearboxes_kg: f64,
    /// One propeller from the Hamilton Standard regression, kg, excluding the
    /// spinner, de-icing and governor.
    pub propeller_each_kg: f64,
    /// Every propeller including the declared accessory mass, kg.
    pub propellers_kg: f64,
    /// Every nacelle, kg. Reported separately so the caller can charge it to
    /// the same slot the FLOPS nacelle uses, exactly once.
    pub nacelles_kg: f64,
    /// Every pylon, kg.
    pub pylons_kg: f64,
    /// Declared engine controls, starters, mounts and fire protection, kg.
    pub engine_installation_kg: f64,
    /// Fuel system, tanks and plumbing from FLOPS equation 92, kg.
    pub fuel_system_kg: f64,
    /// Unusable fuel from FLOPS alternate equation 161, kg. An operating
    /// item, reported here because this group owns the substitution.
    pub unusable_fuel_kg: f64,
    /// Propulsion group excluding the nacelles: engines, gearboxes,
    /// propellers, pylons, installation and fuel system, kg.
    pub total_without_nacelles_kg: f64,
    /// Which engine-mass source was used.
    pub engine_mass_source: &'static str,
}

/// Why a shaft-power propulsion group could not be evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TurbopropMassUnverifiedReason {
    /// A declared turboprop mass input is nonfinite or out of range.
    InvalidConfiguration,
    /// No engine is installed, or the shaft-power rating is absent.
    ShaftPowerRating,
    /// Propeller diameter, blade count, activity factor, speed or weight
    /// coefficient is missing, so the Hamilton Standard regression has no
    /// inputs.
    PropellerGeometry,
    /// No nacelle area density is declared and no nacelle geometry resolves.
    NacelleArchitecture,
}

impl TurbopropMassUnverifiedReason {
    /// A stable identifier for reports and exports.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidConfiguration => "turboprop_mass_configuration",
            Self::ShaftPowerRating => "turboprop_shaft_power_rating",
            Self::PropellerGeometry => "turboprop_propeller_geometry",
            Self::NacelleArchitecture => "turboprop_nacelle_architecture",
        }
    }
}

/// Evaluate the shaft-power propulsion group.
///
/// # Errors
///
/// Every blocker, sorted and deduplicated. A missing input is never replaced
/// by a fraction of takeoff mass or by a thrust invented from shaft power.
pub fn estimate_turboprop_propulsion(
    inputs: &TurbopropPropulsionInputs,
    config: &FlopsTurbopropConfig,
    maximum_mach: f64,
) -> Result<TurbopropPropulsionBreakdown, Vec<TurbopropMassUnverifiedReason>> {
    let mut reasons = Vec::new();
    if config.validate().is_err() {
        reasons.push(TurbopropMassUnverifiedReason::InvalidConfiguration);
    }
    if inputs.engine_count == 0
        || !inputs.takeoff_shaft_power_per_engine_w.is_finite()
        || inputs.takeoff_shaft_power_per_engine_w <= 0.0
    {
        reasons.push(TurbopropMassUnverifiedReason::ShaftPowerRating);
    }
    let weight_coefficient = config.resolved_weight_coefficient();
    if weight_coefficient.is_none()
        || config.propeller_blade_count == 0
        || !is_positive(config.propeller_activity_factor)
        || !is_positive(inputs.propeller_diameter_m)
        || !is_positive(inputs.propeller_speed_rpm)
    {
        reasons.push(TurbopropMassUnverifiedReason::PropellerGeometry);
    }
    if !is_positive(config.nacelle_area_density_kg_m2)
        || !is_positive(inputs.nacelle_wetted_area_m2)
    {
        reasons.push(TurbopropMassUnverifiedReason::NacelleArchitecture);
    }
    if !reasons.is_empty() {
        reasons.sort_unstable();
        reasons.dedup();
        return Err(reasons);
    }
    let Some(weight_coefficient) = weight_coefficient else {
        return Err(vec![TurbopropMassUnverifiedReason::PropellerGeometry]);
    };

    let count = inputs.engine_count as f64;
    let baseline_shaft_power_w = config
        .baseline_shaft_power_kw
        .map_or(inputs.takeoff_shaft_power_per_engine_w, |kw| kw * 1_000.0);
    let engine_each_kg = engine_mass_each_kg(
        inputs.takeoff_shaft_power_per_engine_w,
        baseline_shaft_power_w,
        config.engine_dry_mass_kg,
        config.engine_mass_scaling_exponent,
    );
    let engine_mass_source = match config.engine_dry_mass_kg {
        Some(_) => "declared_certificated_dry_mass",
        None => "gasp_turboshaft_specific_weight",
    };
    let gearboxes_kg = if config.gearbox_inside_engine_mass {
        0.0
    } else {
        count
            * gearbox_mass_kg(
                shaft_torque_n_m(
                    inputs.takeoff_shaft_power_per_engine_w,
                    inputs.propeller_speed_rpm,
                ),
                inputs.reduction_ratio,
            )
    };
    let propeller_each_kg = propeller_mass_each_kg(
        weight_coefficient,
        inputs.propeller_diameter_m,
        config.propeller_blade_count,
        config.propeller_activity_factor,
        inputs.propeller_speed_rpm,
        inputs.takeoff_shaft_power_per_engine_w,
        inputs.design_mach,
        config.propeller_construction,
    );
    let propellers_kg = count * (propeller_each_kg + config.propeller_accessory_mass_kg);
    let nacelle_each_kg = nacelle_mass_each_kg(
        config.nacelle_area_density_kg_m2,
        inputs.nacelle_wetted_area_m2,
    );
    let nacelles_kg = count * nacelle_each_kg;
    let pylons_kg =
        count * pylon_mass_each_kg(config.pylon_coefficient, engine_each_kg, nacelle_each_kg);
    let fuel_system = fuel_system_kg(inputs.maximum_fuel_capacity_kg, count, maximum_mach);
    let engines_kg = count * engine_each_kg;
    Ok(TurbopropPropulsionBreakdown {
        engine_each_kg,
        engines_kg,
        gearboxes_kg,
        propeller_each_kg,
        propellers_kg,
        nacelles_kg,
        pylons_kg,
        engine_installation_kg: config.engine_installation_mass_kg,
        fuel_system_kg: fuel_system,
        unusable_fuel_kg: unusable_fuel_kg(inputs.maximum_fuel_capacity_kg),
        total_without_nacelles_kg: engines_kg
            + gearboxes_kg
            + propellers_kg
            + pylons_kg
            + config.engine_installation_mass_kg
            + fuel_system,
        engine_mass_source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The registered ATR 72-600 installation: two PW127M at their
    /// certificated 481.7 kg dry mass, 2,475 shp normal take-off, and a
    /// six-blade composite 568F-1 of 3.93 m at 1,200 rev/min.
    fn atr_inputs() -> TurbopropPropulsionInputs {
        TurbopropPropulsionInputs {
            engine_count: 2,
            takeoff_shaft_power_per_engine_w: 2_475.0 * WATTS_PER_SHP,
            propeller_speed_rpm: 1_200.0,
            propeller_diameter_m: 3.93,
            reduction_ratio: 16.7,
            design_mach: 0.44,
            nacelle_wetted_area_m2: 22.0,
            maximum_fuel_capacity_kg: 5_000.0,
        }
    }

    fn atr_config() -> FlopsTurbopropConfig {
        FlopsTurbopropConfig {
            engine_dry_mass_kg: Some(481.7),
            baseline_shaft_power_kw: Some(2_475.0 * WATTS_PER_SHP / 1_000.0),
            engine_mass_scaling_exponent: 1.0,
            gearbox_inside_engine_mass: true,
            propeller_blade_count: 6,
            propeller_activity_factor: 130.0,
            propeller_construction: PropellerConstruction::Composite,
            propeller_weight_coefficient: Some(170.0),
            propeller_accessory_mass_kg: 0.0,
            nacelle_area_density_kg_m2: 12.0,
            pylon_coefficient: 0.0,
            engine_installation_mass_kg: 0.0,
            engine_oil_mass_kg: 0.0,
        }
    }

    #[test]
    fn a_declared_certificated_engine_mass_is_used_unchanged_at_its_own_rating() {
        let breakdown = estimate_turboprop_propulsion(&atr_inputs(), &atr_config(), 0.55)
            .expect("the declared ATR installation evaluates");
        assert!((breakdown.engine_each_kg - 481.7).abs() < 1e-9);
        assert!((breakdown.engines_kg - 963.4).abs() < 1e-9);
        assert_eq!(
            breakdown.engine_mass_source,
            "declared_certificated_dry_mass"
        );
        // EASA TCDS IM.E.041 §III.2 puts the reduction gearbox inside that
        // certificated mass, so charging one separately would count it twice.
        assert_eq!(breakdown.gearboxes_kg, 0.0);
    }

    #[test]
    fn the_gasp_specific_weight_is_the_fallback_and_is_named_as_such() {
        let mut config = atr_config();
        config.engine_dry_mass_kg = None;
        config.baseline_shaft_power_kw = None;
        let breakdown = estimate_turboprop_propulsion(&atr_inputs(), &config, 0.55)
            .expect("the GASP fallback evaluates");
        // Equation V.1.3/V.1.4: 0.5 lb/hp x 2,475 hp = 1,237.5 lb.
        assert!((lb(breakdown.engine_each_kg) - 1_237.5).abs() < 1e-6);
        assert_eq!(
            breakdown.engine_mass_source,
            "gasp_turboshaft_specific_weight"
        );
    }

    #[test]
    fn the_propeller_regression_lands_in_the_corroborated_mass_band() {
        // NASA NTRS 20230006542 Table 2 gives a GASP "Propulsor" group of
        // 858 lbf for the two-propeller ATR 42-600, i.e. 429 lb = 194.6 kg
        // per propeller at 2,160 shp. The ATR 72 runs the same 568F-1 family
        // at 2,475 shp, so a defensible model must land near that, not at the
        // ~976 kg the AeroSandbox Torenbeek propeller form returns.
        let breakdown = estimate_turboprop_propulsion(&atr_inputs(), &atr_config(), 0.55)
            .expect("the declared ATR installation evaluates");
        assert!(
            (150.0..=260.0).contains(&breakdown.propeller_each_kg),
            "propeller {} kg is outside the corroborated 568F-class band",
            breakdown.propeller_each_kg
        );
        assert!((breakdown.propellers_kg - 2.0 * breakdown.propeller_each_kg).abs() < 1e-9);
    }

    #[test]
    fn the_declared_accessory_mass_is_added_outside_the_regression() {
        // NASA CR-152303 Vol. V p. V-1.11 states the regression excludes the
        // spinner, de-icing and governor, so they must not be folded into it.
        let mut config = atr_config();
        config.propeller_accessory_mass_kg = 30.0;
        let with_accessories = estimate_turboprop_propulsion(&atr_inputs(), &config, 0.55)
            .expect("declared accessories evaluate");
        let bare = estimate_turboprop_propulsion(&atr_inputs(), &atr_config(), 0.55)
            .expect("the bare installation evaluates");
        assert!((with_accessories.propeller_each_kg - bare.propeller_each_kg).abs() < 1e-12);
        assert!((with_accessories.propellers_kg - bare.propellers_kg - 60.0).abs() < 1e-9);
    }

    #[test]
    fn the_group_responds_to_power_diameter_blades_and_engine_count() {
        let base = estimate_turboprop_propulsion(&atr_inputs(), &atr_config(), 0.55)
            .expect("baseline evaluates");

        let mut powerful = atr_inputs();
        powerful.takeoff_shaft_power_per_engine_w *= 1.3;
        let mut scaled = atr_config();
        scaled.engine_mass_scaling_exponent = 1.0;
        let powerful = estimate_turboprop_propulsion(&powerful, &scaled, 0.55)
            .expect("a higher rating evaluates");
        assert!(powerful.engines_kg > base.engines_kg);
        assert!(powerful.propellers_kg > base.propellers_kg);

        let mut larger = atr_inputs();
        larger.propeller_diameter_m *= 1.2;
        let larger = estimate_turboprop_propulsion(&larger, &atr_config(), 0.55)
            .expect("a larger propeller evaluates");
        assert!(larger.propellers_kg > base.propellers_kg);

        let mut eight_blades = atr_config();
        eight_blades.propeller_blade_count = 8;
        let eight_blades = estimate_turboprop_propulsion(&atr_inputs(), &eight_blades, 0.55)
            .expect("eight blades evaluate");
        assert!(eight_blades.propellers_kg > base.propellers_kg);

        let mut four_engines = atr_inputs();
        four_engines.engine_count = 4;
        let four_engines = estimate_turboprop_propulsion(&four_engines, &atr_config(), 0.55)
            .expect("four engines evaluate");
        assert!((four_engines.engines_kg - 2.0 * base.engines_kg).abs() < 1e-9);
        assert!(four_engines.fuel_system_kg > base.fuel_system_kg);
    }

    #[test]
    fn an_incomplete_declaration_is_refused_with_every_named_blocker() {
        let mut config = atr_config();
        config.propeller_blade_count = 0;
        config.nacelle_area_density_kg_m2 = 0.0;
        let reasons = estimate_turboprop_propulsion(&atr_inputs(), &config, 0.55)
            .expect_err("an incomplete declaration must not produce a mass");
        assert!(reasons.contains(&TurbopropMassUnverifiedReason::PropellerGeometry));
        assert!(reasons.contains(&TurbopropMassUnverifiedReason::NacelleArchitecture));

        let mut no_power = atr_inputs();
        no_power.takeoff_shaft_power_per_engine_w = 0.0;
        let reasons = estimate_turboprop_propulsion(&no_power, &atr_config(), 0.55)
            .expect_err("no shaft power must not produce a mass");
        assert!(reasons.contains(&TurbopropMassUnverifiedReason::ShaftPowerRating));
    }

    #[test]
    fn a_separately_charged_gearbox_follows_the_torque_relation() {
        let mut config = atr_config();
        config.gearbox_inside_engine_mass = false;
        let breakdown = estimate_turboprop_propulsion(&atr_inputs(), &config, 0.55)
            .expect("a separately charged gearbox evaluates");
        let torque = shaft_torque_n_m(2_475.0 * WATTS_PER_SHP, 1_200.0);
        let expected = 2.0 * gearbox_mass_kg(torque, 16.7);
        assert!((breakdown.gearboxes_kg - expected).abs() < 1e-9);
        assert!(breakdown.gearboxes_kg > 0.0);
    }

    #[test]
    fn the_unusable_fuel_uses_the_thrust_free_alternate_equation() {
        let breakdown = estimate_turboprop_propulsion(&atr_inputs(), &atr_config(), 0.55)
            .expect("baseline evaluates");
        assert!((breakdown.unusable_fuel_kg - 0.0084 * 5_000.0).abs() < 1e-12);
    }

    #[test]
    fn the_pylon_relation_is_charged_only_when_a_coefficient_is_declared() {
        let base = estimate_turboprop_propulsion(&atr_inputs(), &atr_config(), 0.55)
            .expect("baseline evaluates");
        assert_eq!(base.pylons_kg, 0.0);
        let mut config = atr_config();
        config.pylon_coefficient = 0.7;
        let with_pylon = estimate_turboprop_propulsion(&atr_inputs(), &config, 0.55)
            .expect("a declared pylon evaluates");
        let nacelle_each = 12.0 * 22.0;
        let expected = 2.0 * kg(0.7 * lb(481.7 + nacelle_each).powf(0.736));
        assert!((with_pylon.pylons_kg - expected).abs() < 1e-9);
        assert!(
            (with_pylon.total_without_nacelles_kg
                - base.total_without_nacelles_kg
                - with_pylon.pylons_kg)
                .abs()
                < 1e-9
        );
    }
}
