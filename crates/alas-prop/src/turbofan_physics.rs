// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Candidate physical primitives for the unified product turbofan.
//!
//! These routines deliberately do not depend on either legacy turbofan kernel.
//! They implement dimensional control-volume balances, so the static limit is
//! regular. Thermodynamic properties use NASA seven-coefficient ideal-gas data
//! documented by McBride et al., NASA/TP-2002-211556; nozzle and thrust
//! balances follow the one-dimensional equations used by Mattingly.
//!
//! The implemented temperature domain is deliberately restricted to
//! 200--2000 K, although the source species fits extend to 6000 K. That upper
//! limit covers the intended transport-engine cycle range without implying a
//! dissociation or equilibrium-combustion model.

use std::fmt;

const UNIVERSAL_GAS_CONSTANT_J_MOL_K: f64 = 8.314_462_618_153_24;
const MIN_TEMPERATURE_K: f64 = 200.0;
const MAX_TEMPERATURE_K: f64 = 2_000.0;

/// A recoverable physical-domain or conservation failure.
#[derive(Clone, Debug, PartialEq)]
pub enum PhysicsError {
    /// An input or intermediate value was NaN or infinite.
    NonFinite {
        /// Name of the invalid quantity.
        field: &'static str,
        /// Invalid numerical value.
        value: f64,
    },
    /// A finite value lies outside its supported closed interval.
    OutOfRange {
        /// Name of the invalid quantity.
        field: &'static str,
        /// Invalid numerical value.
        value: f64,
        /// Smallest supported value.
        minimum: f64,
        /// Largest supported value.
        maximum: f64,
    },
    /// Gas mass fractions are negative, non-finite, or do not sum to one.
    InvalidComposition {
        /// Sum of the supplied species mass fractions.
        mass_fraction_sum: f64,
    },
    /// A nozzle cannot discharge against a pressure at or above total pressure.
    InsufficientTotalPressure {
        /// Nozzle-inlet total pressure, Pa.
        total_pressure_pa: f64,
        /// Imposed back pressure, Pa.
        back_pressure_pa: f64,
    },
    /// A bracketed scalar equation did not converge.
    NoConvergence {
        /// Stable identifier for the failed equation.
        equation: &'static str,
    },
    /// The evaluated operating state produces zero or negative forward thrust.
    NonPositiveNetThrust {
        /// Computed net axial thrust, N.
        thrust_n: f64,
    },
}

impl fmt::Display for PhysicsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for PhysicsError {}

/// Calorically imperfect ideal-gas properties at one temperature.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GasProperties {
    /// Sensible specific enthalpy relative to 298.15 K, J/kg.
    pub specific_enthalpy_j_kg: f64,
    /// Specific heat at constant pressure, J/(kg K).
    pub specific_heat_cp_j_kg_k: f64,
    /// Specific heat at constant volume, J/(kg K).
    pub specific_heat_cv_j_kg_k: f64,
    /// Ratio of specific heats, `cp/cv`.
    pub gamma: f64,
    /// Mixture-specific ideal-gas constant, J/(kg K).
    pub gas_constant_j_kg_k: f64,
}

/// Fixed-composition ideal-gas mixture expressed as species mass fractions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GasComposition {
    /// Nitrogen mass fraction.
    pub nitrogen: f64,
    /// Oxygen mass fraction.
    pub oxygen: f64,
    /// Carbon-dioxide mass fraction.
    pub carbon_dioxide: f64,
    /// Water-vapor mass fraction.
    pub water_vapor: f64,
    /// Argon mass fraction.
    pub argon: f64,
}

impl GasComposition {
    /// Dry standard-air composition expressed as mass fractions.
    pub const DRY_AIR: Self = Self {
        nitrogen: 0.755_18,
        oxygen: 0.231_41,
        carbon_dioxide: 0.000_63,
        water_vapor: 0.0,
        argon: 0.012_78,
    };

    /// Representative lean kerosene-combustion products for cycle screening.
    /// A calibrated engine model should instead derive composition from FAR.
    pub const LEAN_COMBUSTION_PRODUCTS: Self = Self {
        nitrogen: 0.748,
        oxygen: 0.145,
        carbon_dioxide: 0.075,
        water_vapor: 0.025,
        argon: 0.007,
    };

    fn checked(self) -> Result<Self, PhysicsError> {
        let fractions = [
            self.nitrogen,
            self.oxygen,
            self.carbon_dioxide,
            self.water_vapor,
            self.argon,
        ];
        if fractions
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err(PhysicsError::InvalidComposition {
                mass_fraction_sum: fractions.iter().sum(),
            });
        }
        let sum: f64 = fractions.iter().sum();
        if (sum - 1.0).abs() > 1.0e-9 {
            return Err(PhysicsError::InvalidComposition {
                mass_fraction_sum: sum,
            });
        }
        Ok(self)
    }
}

#[derive(Clone, Copy)]
struct NasaSpecies {
    molar_mass_kg_mol: f64,
    low: [f64; 7],
    high: [f64; 7],
}

impl NasaSpecies {
    fn coefficients(self, temperature_k: f64) -> [f64; 7] {
        if temperature_k < 1_000.0 {
            self.low
        } else {
            self.high
        }
    }

    fn raw_cp_and_enthalpy(self, temperature_k: f64) -> (f64, f64, f64) {
        let [a1, a2, a3, a4, a5, a6, _a7] = self.coefficients(temperature_k);
        let t = temperature_k;
        let cp_over_r = a1 + a2 * t + a3 * t.powi(2) + a4 * t.powi(3) + a5 * t.powi(4);
        let h_over_rt = a1
            + a2 * t / 2.0
            + a3 * t.powi(2) / 3.0
            + a4 * t.powi(3) / 4.0
            + a5 * t.powi(4) / 5.0
            + a6 / t;
        let specific_r = UNIVERSAL_GAS_CONSTANT_J_MOL_K / self.molar_mass_kg_mol;
        (
            cp_over_r * specific_r,
            h_over_rt * specific_r * t,
            specific_r,
        )
    }

    fn properties(self, temperature_k: f64) -> (f64, f64, f64) {
        let (cp, absolute_enthalpy, specific_r) = self.raw_cp_and_enthalpy(temperature_k);
        let (_, reference_enthalpy, _) = self.raw_cp_and_enthalpy(298.15);
        (cp, absolute_enthalpy - reference_enthalpy, specific_r)
    }
}

// Coefficients transcribed from the NASA gas database distributed by Cantera
// (`data/nasa_gas.yaml`), whose source is NASA/TP-2002-211556. Each species is
// valid from 200 K through at least 6000 K and switches branch at 1000 K.
const NITROGEN: NasaSpecies = NasaSpecies {
    molar_mass_kg_mol: 0.028_013_4,
    low: [
        3.531_005_28,
        -1.236_609_87e-4,
        -5.029_994_33e-7,
        2.435_306_12e-9,
        -1.408_812_35e-12,
        -1.046_976_28e3,
        2.967_474_68,
    ],
    high: [
        2.952_576_26,
        1.396_900_57e-3,
        -4.926_316_91e-7,
        7.860_103_67e-11,
        -4.607_553_21e-15,
        -9.239_486_45e2,
        5.871_892_52,
    ],
};
const OXYGEN: NasaSpecies = NasaSpecies {
    molar_mass_kg_mol: 0.031_998_8,
    low: [
        3.782_456_36,
        -2.996_734_16e-3,
        9.847_302_01e-6,
        -9.681_295_09e-9,
        3.243_728_37e-12,
        -1.063_943_56e3,
        3.657_675_73,
    ],
    high: [
        3.660_960_83,
        6.563_655_23e-4,
        -1.411_494_85e-7,
        2.057_976_58e-11,
        -1.299_132_48e-15,
        -1.215_977_25e3,
        3.415_361_84,
    ],
};
const CARBON_DIOXIDE: NasaSpecies = NasaSpecies {
    molar_mass_kg_mol: 0.044_009_5,
    low: [
        2.356_773_52,
        8.984_596_77e-3,
        -7.123_562_69e-6,
        2.459_190_22e-9,
        -1.436_995_48e-13,
        -4.837_196_97e4,
        9.901_052_22,
    ],
    high: [
        4.636_594_93,
        2.741_319_91e-3,
        -9.958_285_31e-7,
        1.603_730_11e-10,
        -9.161_034_68e-15,
        -4.902_493_41e4,
        -1.935_348_55,
    ],
};
const WATER_VAPOR: NasaSpecies = NasaSpecies {
    molar_mass_kg_mol: 0.018_015_28,
    low: [
        4.198_640_56,
        -2.036_434_10e-3,
        6.520_402_11e-6,
        -5.487_970_62e-9,
        1.771_978_17e-12,
        -3.029_372_67e4,
        -0.849_032_208,
    ],
    high: [
        2.677_037_87,
        2.973_183_29e-3,
        -7.737_696_90e-7,
        9.443_366_89e-11,
        -4.269_009_59e-15,
        -2.988_589_38e4,
        6.882_555_71,
    ],
};
const ARGON: NasaSpecies = NasaSpecies {
    molar_mass_kg_mol: 0.039_948,
    low: [2.5, 0.0, 0.0, 0.0, 0.0, -745.375, 4.379_674_91],
    high: [2.5, 0.0, 0.0, 0.0, 0.0, -745.375, 4.379_674_91],
};

/// Returns temperature-dependent ideal-gas mixture properties over 200--2000 K.
///
/// Species enthalpies are shifted independently to zero at 298.15 K so that
/// the returned mixture enthalpy is sensible enthalpy, suitable for pairing
/// with a fuel lower heating value in the combustor balance.
pub fn gas_properties(
    temperature_k: f64,
    composition: GasComposition,
) -> Result<GasProperties, PhysicsError> {
    check_range(
        "temperature_k",
        temperature_k,
        MIN_TEMPERATURE_K,
        MAX_TEMPERATURE_K,
    )?;
    let composition = composition.checked()?;
    let species = [
        (composition.nitrogen, NITROGEN),
        (composition.oxygen, OXYGEN),
        (composition.carbon_dioxide, CARBON_DIOXIDE),
        (composition.water_vapor, WATER_VAPOR),
        (composition.argon, ARGON),
    ];
    let (mut cp, mut enthalpy, mut gas_constant) = (0.0, 0.0, 0.0);
    for (mass_fraction, model) in species {
        let (species_cp, species_enthalpy, species_r) = model.properties(temperature_k);
        cp += mass_fraction * species_cp;
        enthalpy += mass_fraction * species_enthalpy;
        gas_constant += mass_fraction * species_r;
    }
    let cv = cp - gas_constant;
    Ok(GasProperties {
        specific_enthalpy_j_kg: enthalpy,
        specific_heat_cp_j_kg_k: cp,
        specific_heat_cv_j_kg_k: cv,
        gamma: cp / cv,
        gas_constant_j_kg_k: gas_constant,
    })
}

/// Stagnation temperature and pressure at an engine station.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TotalState {
    /// Stagnation temperature, K.
    pub temperature_k: f64,
    /// Stagnation pressure, Pa.
    pub pressure_pa: f64,
}

/// Apply an adiabatic inlet total-pressure recovery. Total temperature is
/// unchanged because the inlet has neither shaft work nor heat transfer.
pub fn adiabatic_inlet(
    upstream: TotalState,
    total_pressure_recovery: f64,
) -> Result<TotalState, PhysicsError> {
    check_range(
        "upstream.temperature_k",
        upstream.temperature_k,
        MIN_TEMPERATURE_K,
        MAX_TEMPERATURE_K,
    )?;
    check_positive("upstream.pressure_pa", upstream.pressure_pa)?;
    check_range(
        "total_pressure_recovery",
        total_pressure_recovery,
        f64::EPSILON,
        1.0,
    )?;
    Ok(TotalState {
        temperature_k: upstream.temperature_k,
        pressure_pa: upstream.pressure_pa * total_pressure_recovery,
    })
}

/// Flow regime at a convergent-nozzle exit plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NozzleRegime {
    /// Exit static pressure closes to ambient and exit Mach is below one.
    Unchoked,
    /// Throat Mach is one and exit static pressure can exceed ambient.
    Choked,
}

/// Solved convergent-nozzle state and local conservation diagnostics.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NozzleResult {
    /// Choked or unchoked exit regime.
    pub regime: NozzleRegime,
    /// Exit static temperature, K.
    pub exit_temperature_k: f64,
    /// Exit static pressure, Pa.
    pub exit_pressure_pa: f64,
    /// Axial exit velocity, m/s.
    pub exit_velocity_m_s: f64,
    /// Exit Mach number.
    pub exit_mach: f64,
    /// Exit density, kg/m³.
    pub exit_density_kg_m3: f64,
    /// Area required to pass the supplied mass flow, m².
    pub required_area_m2: f64,
    /// Gross momentum flux `mass_flow * exit_velocity`, N.
    pub momentum_thrust_n: f64,
    /// Exit pressure thrust `(exit_pressure - ambient_pressure) * area`, N.
    pub pressure_thrust_n: f64,
    /// Steady adiabatic energy-balance residual, W.
    pub energy_residual_w: f64,
    /// Exit continuity residual, kg/s.
    pub mass_residual_kg_s: f64,
}

/// Solve an ideal, adiabatic convergent nozzle with a supplied mass flow.
///
/// The unchoked exit closes to ambient pressure. The choked exit satisfies
/// `V=a` at the throat. Pressure thrust remains explicit; it is never hidden
/// in an equivalent velocity. Area follows from continuity after the state is
/// solved, which makes the routine useful while engine flow size is unknown.
pub fn convergent_nozzle(
    total_state: TotalState,
    ambient_pressure_pa: f64,
    mass_flow_kg_s: f64,
    composition: GasComposition,
) -> Result<NozzleResult, PhysicsError> {
    check_range(
        "total_state.temperature_k",
        total_state.temperature_k,
        MIN_TEMPERATURE_K,
        MAX_TEMPERATURE_K,
    )?;
    check_positive("total_state.pressure_pa", total_state.pressure_pa)?;
    check_positive("ambient_pressure_pa", ambient_pressure_pa)?;
    check_positive("mass_flow_kg_s", mass_flow_kg_s)?;
    if total_state.pressure_pa <= ambient_pressure_pa {
        return Err(PhysicsError::InsufficientTotalPressure {
            total_pressure_pa: total_state.pressure_pa,
            back_pressure_pa: ambient_pressure_pa,
        });
    }

    let total_properties = gas_properties(total_state.temperature_k, composition)?;
    let critical_temperature_k = bisect_temperature(
        MIN_TEMPERATURE_K.max(0.25 * total_state.temperature_k),
        total_state.temperature_k,
        |temperature_k| {
            let properties = gas_properties(temperature_k, composition)?;
            Ok(
                2.0 * (total_properties.specific_enthalpy_j_kg - properties.specific_enthalpy_j_kg)
                    - properties.gamma * properties.gas_constant_j_kg_k * temperature_k,
            )
        },
        "critical nozzle energy",
    )?;
    let critical_pressure_pa =
        isentropic_pressure(total_state, critical_temperature_k, composition)?;

    let (regime, exit_pressure_pa, exit_temperature_k) =
        if ambient_pressure_pa <= critical_pressure_pa {
            (
                NozzleRegime::Choked,
                critical_pressure_pa,
                critical_temperature_k,
            )
        } else {
            let temperature_k =
                temperature_at_isentropic_pressure(total_state, ambient_pressure_pa, composition)?;
            (NozzleRegime::Unchoked, ambient_pressure_pa, temperature_k)
        };
    let exit_properties = gas_properties(exit_temperature_k, composition)?;
    let exit_velocity_m_s = (2.0
        * (total_properties.specific_enthalpy_j_kg - exit_properties.specific_enthalpy_j_kg))
        .sqrt();
    let speed_of_sound_m_s =
        (exit_properties.gamma * exit_properties.gas_constant_j_kg_k * exit_temperature_k).sqrt();
    let exit_density_kg_m3 =
        exit_pressure_pa / (exit_properties.gas_constant_j_kg_k * exit_temperature_k);
    let required_area_m2 = mass_flow_kg_s / (exit_density_kg_m3 * exit_velocity_m_s);
    let reconstructed_mass_flow = exit_density_kg_m3 * exit_velocity_m_s * required_area_m2;
    let energy_residual_w = mass_flow_kg_s
        * (total_properties.specific_enthalpy_j_kg
            - exit_properties.specific_enthalpy_j_kg
            - 0.5 * exit_velocity_m_s.powi(2));

    Ok(NozzleResult {
        regime,
        exit_temperature_k,
        exit_pressure_pa,
        exit_velocity_m_s,
        exit_mach: exit_velocity_m_s / speed_of_sound_m_s,
        exit_density_kg_m3,
        required_area_m2,
        momentum_thrust_n: mass_flow_kg_s * exit_velocity_m_s,
        pressure_thrust_n: (exit_pressure_pa - ambient_pressure_pa) * required_area_m2,
        energy_residual_w,
        mass_residual_kg_s: reconstructed_mass_flow - mass_flow_kg_s,
    })
}

/// One exhaust stream in the engine control-volume thrust balance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StreamThrust {
    /// Freestream air captured by this stream, kg/s.
    pub inlet_air_mass_flow_kg_s: f64,
    /// Exhaust mass flow, including injected fuel where applicable, kg/s.
    pub exit_mass_flow_kg_s: f64,
    /// Axial exhaust velocity, m/s.
    pub exit_velocity_m_s: f64,
    /// Exhaust-plane static pressure, Pa.
    pub exit_pressure_pa: f64,
    /// Local ambient static pressure, Pa.
    pub ambient_pressure_pa: f64,
    /// Exhaust-plane flow area, m².
    pub exit_area_m2: f64,
}

/// Decomposed axial thrust and control-volume mass closure.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThrustBalance {
    /// Sum of gross exhaust momentum fluxes, N.
    pub momentum_thrust_n: f64,
    /// Sum of exhaust pressure-thrust terms, N.
    pub pressure_thrust_n: f64,
    /// Captured freestream momentum flux, N.
    pub ram_drag_n: f64,
    /// Momentum plus pressure thrust minus ram drag, N.
    pub net_thrust_n: f64,
    /// Exhaust minus captured-air and fuel mass flows, kg/s.
    pub mass_residual_kg_s: f64,
}

/// Computes a dimensional multi-stream thrust balance.
///
/// Unlike formulations normalized by Mach, this control-volume equation is
/// regular at exactly zero freestream velocity. A non-positive result is
/// rejected because this primitive currently represents powered forward-thrust
/// operation; windmilling and reverse thrust require separate operating modes.
pub fn net_thrust(
    freestream_velocity_m_s: f64,
    fuel_mass_flow_kg_s: f64,
    streams: &[StreamThrust],
) -> Result<ThrustBalance, PhysicsError> {
    check_nonnegative("freestream_velocity_m_s", freestream_velocity_m_s)?;
    check_nonnegative("fuel_mass_flow_kg_s", fuel_mass_flow_kg_s)?;
    let mut inlet_air_mass_flow = 0.0;
    let mut exit_mass_flow = 0.0;
    let mut momentum_thrust = 0.0;
    let mut pressure_thrust = 0.0;
    for stream in streams {
        check_nonnegative(
            "stream.inlet_air_mass_flow_kg_s",
            stream.inlet_air_mass_flow_kg_s,
        )?;
        check_positive("stream.exit_mass_flow_kg_s", stream.exit_mass_flow_kg_s)?;
        check_nonnegative("stream.exit_velocity_m_s", stream.exit_velocity_m_s)?;
        check_positive("stream.exit_pressure_pa", stream.exit_pressure_pa)?;
        check_positive("stream.ambient_pressure_pa", stream.ambient_pressure_pa)?;
        check_positive("stream.exit_area_m2", stream.exit_area_m2)?;
        inlet_air_mass_flow += stream.inlet_air_mass_flow_kg_s;
        exit_mass_flow += stream.exit_mass_flow_kg_s;
        momentum_thrust += stream.exit_mass_flow_kg_s * stream.exit_velocity_m_s;
        pressure_thrust +=
            (stream.exit_pressure_pa - stream.ambient_pressure_pa) * stream.exit_area_m2;
    }
    let ram_drag = inlet_air_mass_flow * freestream_velocity_m_s;
    let net = momentum_thrust + pressure_thrust - ram_drag;
    if net <= 0.0 {
        return Err(PhysicsError::NonPositiveNetThrust { thrust_n: net });
    }
    Ok(ThrustBalance {
        momentum_thrust_n: momentum_thrust,
        pressure_thrust_n: pressure_thrust,
        ram_drag_n: ram_drag,
        net_thrust_n: net,
        mass_residual_kg_s: exit_mass_flow - inlet_air_mass_flow - fuel_mass_flow_kg_s,
    })
}

/// Computes fuel/air ratio from `(1+f) h4 = h3 + f eta_b LHV`.
///
/// Both enthalpies are sensible enthalpies relative to 298.15 K. Chemical
/// energy is represented once, through the supplied lower heating value.
pub fn combustor_fuel_air_ratio(
    inlet_temperature_k: f64,
    outlet_temperature_k: f64,
    combustion_efficiency: f64,
    fuel_lower_heating_value_j_kg: f64,
    inlet_composition: GasComposition,
    outlet_composition: GasComposition,
) -> Result<f64, PhysicsError> {
    check_range(
        "combustion_efficiency",
        combustion_efficiency,
        f64::EPSILON,
        1.0,
    )?;
    check_positive(
        "fuel_lower_heating_value_j_kg",
        fuel_lower_heating_value_j_kg,
    )?;
    let inlet_h = gas_properties(inlet_temperature_k, inlet_composition)?.specific_enthalpy_j_kg;
    let outlet_h = gas_properties(outlet_temperature_k, outlet_composition)?.specific_enthalpy_j_kg;
    let denominator = combustion_efficiency * fuel_lower_heating_value_j_kg - outlet_h;
    if denominator <= 0.0 || outlet_h <= inlet_h {
        return Err(PhysicsError::OutOfRange {
            field: "combustor enthalpy balance",
            value: denominator,
            minimum: f64::EPSILON,
            maximum: f64::INFINITY,
        });
    }
    Ok((outlet_h - inlet_h) / denominator)
}

fn isentropic_pressure(
    total_state: TotalState,
    static_temperature_k: f64,
    composition: GasComposition,
) -> Result<f64, PhysicsError> {
    let integral_cp_over_t = integrate_simpson(
        static_temperature_k,
        total_state.temperature_k,
        |temperature_k| {
            let properties = gas_properties(temperature_k, composition)?;
            Ok(properties.specific_heat_cp_j_kg_k / temperature_k)
        },
    )?;
    let gas_constant = gas_properties(total_state.temperature_k, composition)?.gas_constant_j_kg_k;
    Ok(total_state.pressure_pa * (-integral_cp_over_t / gas_constant).exp())
}

fn temperature_at_isentropic_pressure(
    total_state: TotalState,
    static_pressure_pa: f64,
    composition: GasComposition,
) -> Result<f64, PhysicsError> {
    bisect_temperature(
        MIN_TEMPERATURE_K.max(0.25 * total_state.temperature_k),
        total_state.temperature_k,
        |temperature_k| {
            Ok(isentropic_pressure(total_state, temperature_k, composition)? - static_pressure_pa)
        },
        "isentropic pressure",
    )
}

fn integrate_simpson<F>(lower: f64, upper: f64, function: F) -> Result<f64, PhysicsError>
where
    F: Fn(f64) -> Result<f64, PhysicsError>,
{
    const INTERVALS: usize = 64;
    let step = (upper - lower) / INTERVALS as f64;
    let mut sum = function(lower)? + function(upper)?;
    for index in 1..INTERVALS {
        let weight = if index % 2 == 0 { 2.0 } else { 4.0 };
        sum += weight * function(lower + index as f64 * step)?;
    }
    Ok(sum * step / 3.0)
}

fn bisect_temperature<F>(
    mut lower: f64,
    mut upper: f64,
    function: F,
    equation: &'static str,
) -> Result<f64, PhysicsError>
where
    F: Fn(f64) -> Result<f64, PhysicsError>,
{
    let mut lower_value = function(lower)?;
    let upper_value = function(upper)?;
    if lower_value * upper_value > 0.0 {
        return Err(PhysicsError::NoConvergence { equation });
    }
    for _ in 0..80 {
        let midpoint = 0.5 * (lower + upper);
        let midpoint_value = function(midpoint)?;
        if midpoint_value.abs() < 1.0e-8 || (upper - lower) < 1.0e-9 {
            return Ok(midpoint);
        }
        if lower_value * midpoint_value <= 0.0 {
            upper = midpoint;
        } else {
            lower = midpoint;
            lower_value = midpoint_value;
        }
    }
    Err(PhysicsError::NoConvergence { equation })
}

fn check_positive(field: &'static str, value: f64) -> Result<(), PhysicsError> {
    check_range(field, value, f64::EPSILON, f64::INFINITY)
}

fn check_nonnegative(field: &'static str, value: f64) -> Result<(), PhysicsError> {
    check_range(field, value, 0.0, f64::INFINITY)
}

fn check_range(
    field: &'static str,
    value: f64,
    minimum: f64,
    maximum: f64,
) -> Result<(), PhysicsError> {
    if !value.is_finite() {
        return Err(PhysicsError::NonFinite { field, value });
    }
    if value < minimum || value > maximum {
        return Err(PhysicsError::OutOfRange {
            field,
            value,
            minimum,
            maximum,
        });
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn dry_air_properties_change_physically_with_temperature() {
        let cold = gas_properties(288.15, GasComposition::DRY_AIR).unwrap();
        let hot = gas_properties(1_800.0, GasComposition::DRY_AIR).unwrap();
        assert!((1_000.0..1_010.0).contains(&cold.specific_heat_cp_j_kg_k));
        assert!(hot.specific_heat_cp_j_kg_k > cold.specific_heat_cp_j_kg_k);
        assert!(hot.gamma < cold.gamma);
        assert!((286.0..289.0).contains(&cold.gas_constant_j_kg_k));
    }

    #[test]
    fn nasa_property_branches_are_continuous_at_one_thousand_kelvin() {
        for composition in [
            GasComposition::DRY_AIR,
            GasComposition::LEAN_COMBUSTION_PRODUCTS,
        ] {
            let below = gas_properties(1_000.0 - 1.0e-6, composition).unwrap();
            let above = gas_properties(1_000.0, composition).unwrap();
            assert!((above.specific_heat_cp_j_kg_k - below.specific_heat_cp_j_kg_k).abs() < 1.0e-3);
            assert!((above.specific_enthalpy_j_kg - below.specific_enthalpy_j_kg).abs() < 1.0e-2);
        }
    }

    #[test]
    fn inlet_pressure_loss_does_not_create_total_temperature_change() {
        let inlet = adiabatic_inlet(
            TotalState {
                temperature_k: 310.0,
                pressure_pa: 160_000.0,
            },
            0.98,
        )
        .unwrap();
        assert_eq!(inlet.temperature_k, 310.0);
        assert_eq!(inlet.pressure_pa, 156_800.0);
    }

    #[test]
    fn nozzle_closes_pressure_when_unchoked() {
        let nozzle = convergent_nozzle(
            TotalState {
                temperature_k: 500.0,
                pressure_pa: 120_000.0,
            },
            101_325.0,
            10.0,
            GasComposition::DRY_AIR,
        )
        .unwrap();
        assert_eq!(nozzle.regime, NozzleRegime::Unchoked);
        assert!((nozzle.exit_pressure_pa - 101_325.0).abs() < 1.0e-8);
        assert!(nozzle.exit_mach < 1.0);
        assert!(nozzle.pressure_thrust_n.abs() < 1.0e-8);
        assert!(nozzle.energy_residual_w.abs() < 1.0e-7);
        assert!(nozzle.mass_residual_kg_s.abs() < 1.0e-12);
    }

    #[test]
    fn convergent_nozzle_chokes_and_retains_pressure_thrust() {
        let nozzle = convergent_nozzle(
            TotalState {
                temperature_k: 1_200.0,
                pressure_pa: 400_000.0,
            },
            101_325.0,
            25.0,
            GasComposition::LEAN_COMBUSTION_PRODUCTS,
        )
        .unwrap();
        assert_eq!(nozzle.regime, NozzleRegime::Choked);
        assert!((nozzle.exit_mach - 1.0).abs() < 1.0e-8);
        assert!(nozzle.exit_pressure_pa > 101_325.0);
        assert!(nozzle.pressure_thrust_n > 0.0);
        assert!(nozzle.energy_residual_w.abs() < 1.0e-6);
    }

    #[test]
    fn static_thrust_is_finite_and_includes_fuel_mass() {
        let balance = net_thrust(
            0.0,
            1.0,
            &[StreamThrust {
                inlet_air_mass_flow_kg_s: 100.0,
                exit_mass_flow_kg_s: 101.0,
                exit_velocity_m_s: 500.0,
                exit_pressure_pa: 0.0,
                ambient_pressure_pa: 101_325.0,
                exit_area_m2: 1.0,
            }],
        );
        assert!(matches!(balance, Err(PhysicsError::OutOfRange { .. })));

        let balance = net_thrust(
            0.0,
            1.0,
            &[StreamThrust {
                inlet_air_mass_flow_kg_s: 100.0,
                exit_mass_flow_kg_s: 101.0,
                exit_velocity_m_s: 500.0,
                exit_pressure_pa: 101_325.0,
                ambient_pressure_pa: 101_325.0,
                exit_area_m2: 1.0,
            }],
        )
        .unwrap();
        assert_eq!(balance.ram_drag_n, 0.0);
        assert_eq!(balance.mass_residual_kg_s, 0.0);
        assert!(balance.net_thrust_n.is_finite());
    }

    #[test]
    fn rejects_out_of_domain_hot_gas_temperature() {
        assert!(matches!(
            gas_properties(2_100.0, GasComposition::DRY_AIR),
            Err(PhysicsError::OutOfRange { .. })
        ));
    }

    #[test]
    fn combustor_balance_returns_a_transport_engine_order_fuel_ratio() {
        let ratio = combustor_fuel_air_ratio(
            800.0,
            1_600.0,
            0.99,
            43.0e6,
            GasComposition::DRY_AIR,
            GasComposition::LEAN_COMBUSTION_PRODUCTS,
        )
        .unwrap();
        assert!((0.015..0.050).contains(&ratio), "fuel/air ratio = {ratio}");
    }
}
