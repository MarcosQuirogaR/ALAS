// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

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
    /// Exit density, kg/m--.
    pub exit_density_kg_m3: f64,
    /// Area required to pass the supplied mass flow, m--.
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
