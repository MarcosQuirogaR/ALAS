// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Preliminary, stand-alone turboprop operating-point model.
//!
//! This module deliberately contains no aircraft or mission wiring.  It separates
//! gas-turbine shaft power, accessory and gearbox losses, propeller force, residual
//! jet thrust, and fuel flow.  The PW127M ratings are certified installation data;
//! the fuel and propeller models are explicitly labelled family-level surrogates.

use std::{error::Error, f64::consts::PI, fmt};

use crate::system::{
    ActiveLimit, DiagnosticItem, DiagnosticsQuery, FailureState, FlightCondition, ModelIdentity,
    ModelProvenance, OperatingMode, PropulsionCapability, PropulsionDemand, PropulsionDiagnostics,
    PropulsionError, PropulsionInstallation, PropulsionMassItem, PropulsionRating,
    PropulsionRequest, PropulsionResult, PropulsionSystemModel, Residual, ResourceFlow,
    ResourceKind, StateDerivative, ValidityStatus,
};

const WATTS_PER_SHP: f64 = 745.699_872;
const JOULES_PER_KWH: f64 = 3.6e6;

/// PW127M rating selected by the installation control schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pw127mRating {
    /// Normal all-engines-operating take-off rating, 2,475 shp.
    NormalTakeoff,
    /// Maximum take-off/automatic reserve rating, 2,750 shp.
    MaximumTakeoffReserve,
    /// Indefinitely sustainable maximum-continuous rating, 2,500 shp.
    MaximumContinuous,
    /// Published maximum-climb rating, 2,192 shp.
    MaximumClimb,
    /// Published maximum-cruise rating, 2,132 shp.
    MaximumCruise,
    /// Unvalidated flight-idle surrogate at eight percent of MCT power.
    FlightIdleSurrogate,
}

impl Pw127mRating {
    /// Rated free-turbine output power in watts.
    ///
    /// Take-off/continuous values follow the PW127M certification basis; the
    /// 2,192 shp climb and 2,132 shp cruise installation limits are from the
    /// ATR 72-600 manufacturer factsheet. Conversion uses
    /// 1 mechanical shp = 745.699872 W.
    #[must_use]
    pub fn shaft_power_w(self) -> f64 {
        let shaft_horsepower = match self {
            Self::NormalTakeoff => 2_475.0,
            Self::MaximumTakeoffReserve => 2_750.0,
            Self::MaximumContinuous => 2_500.0,
            Self::MaximumClimb => 2_192.0,
            Self::MaximumCruise => 2_132.0,
            Self::FlightIdleSurrogate => 0.08 * 2_500.0,
        };
        shaft_horsepower * WATTS_PER_SHP
    }
}

/// Discrete state interpreted by the isolated engine/propeller kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurbopropMode {
    /// Propeller governor varies blade angle to absorb commanded power at fixed rpm.
    Governed,
    /// Running airborne idle; unavailable until a measured idle schedule is supplied.
    FlightIdle,
    /// Blades at feather angle; unavailable until a measured windmilling map is supplied.
    Feathered,
    /// Fuel and shaft power off, with no windmilling drag model.
    Shutdown,
    /// Ground beta/reverse operation; unavailable until a reverse map is supplied.
    Reverse,
}

/// Ambient state required by the isolated propeller calculation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurbopropCondition {
    /// Freestream density, kg/m^3.
    pub density_kg_m3: f64,
    /// Freestream true airspeed along the propeller axis, m/s.
    pub true_airspeed_m_s: f64,
}

/// Control selection supplied to the isolated engine/propeller calculation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurbopropCommand {
    /// Certified engine rating defining available free-turbine power.
    pub rating: Pw127mRating,
    /// Requested fraction of the selected shaft-power rating, in [0, 1].
    pub power_fraction: f64,
    /// Discrete propeller and engine operating state.
    pub mode: TurbopropMode,
    /// Governed propeller speed. The ATR installation nominal value is 1,200 rpm.
    pub propeller_speed_rpm: f64,
}

/// Installation and surrogate-model parameters, all in SI units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pw127m568fModel {
    /// Normal all-engines-operating takeoff shaft rating, W.
    pub normal_takeoff_power_w: f64,
    /// Automatic-reserve / maximum-takeoff shaft rating, W.
    pub maximum_takeoff_reserve_power_w: f64,
    /// Maximum-continuous shaft rating, W.
    pub maximum_continuous_power_w: f64,
    /// Maximum-climb shaft rating, W.
    pub maximum_climb_power_w: f64,
    /// Maximum-cruise shaft rating, W.
    pub maximum_cruise_power_w: f64,
    /// Governed propeller rotational speed, rev/min.
    pub governed_propeller_speed_rpm: f64,
    /// Propeller diameter, m.
    pub propeller_diameter_m: f64,
    /// Gearbox mechanical output/input power ratio.
    pub gearbox_efficiency: f64,
    /// Per-engine baseline mechanical accessory extraction, W.
    pub accessory_power_w: f64,
    /// Per-engine residual core exhaust thrust kept outside propeller thrust, N.
    pub residual_jet_thrust_n: f64,
    /// Reference power-specific fuel consumption, kg/(kW h).
    pub reference_psfc_kg_kwh: f64,
    /// Density of the published maximum-cruise fuel-flow anchor, kg/m^3.
    pub fuel_reference_density_kg_m3: f64,
    /// Static figure of merit used only by the actuator-disk static fallback.
    pub static_figure_of_merit: f64,
    /// Sea-level reference density for the shaft-power lapse law, kg/m^3.
    pub power_lapse_reference_density_kg_m3: f64,
    /// Exponent in the density-ratio shaft-power lapse law.
    pub power_lapse_density_exponent: f64,
    /// Lower bound on the available rated-power fraction at very low density.
    pub minimum_power_lapse_fraction: f64,
    /// Generic propeller coefficient surface and pitch bounds.
    pub surrogate: PropellerSurrogate,
}

impl Default for Pw127m568fModel {
    fn default() -> Self {
        Self {
            normal_takeoff_power_w: Pw127mRating::NormalTakeoff.shaft_power_w(),
            maximum_takeoff_reserve_power_w: Pw127mRating::MaximumTakeoffReserve.shaft_power_w(),
            maximum_continuous_power_w: Pw127mRating::MaximumContinuous.shaft_power_w(),
            maximum_climb_power_w: Pw127mRating::MaximumClimb.shaft_power_w(),
            maximum_cruise_power_w: Pw127mRating::MaximumCruise.shaft_power_w(),
            governed_propeller_speed_rpm: 1_200.0,
            propeller_diameter_m: 3.93,
            gearbox_efficiency: 0.98,
            accessory_power_w: 25_000.0,
            residual_jet_thrust_n: 0.0,
            // Calibrated to ATR's published 762 kg/h two-engine fuel flow at
            // maximum cruise power (2,132 shp per engine). This is one
            // aircraft-level anchor, not a complete PW127M fuel deck.
            reference_psfc_kg_kwh: 0.239_647_943_644_226,
            fuel_reference_density_kg_m3: 0.70,
            static_figure_of_merit: 0.72,
            // Minimum-hypothesis lapse calibrated at aircraft level to the
            // published ATR 72-600 time to FL170, without segment-specific
            // schedules: sigma_FL170^0.75 is approximately 0.66.
            power_lapse_reference_density_kg_m3: 1.225,
            power_lapse_density_exponent: 0.75,
            minimum_power_lapse_fraction: 0.15,
            surrogate: PropellerSurrogate::generic_six_blade(),
        }
    }
}

/// Generic variable-pitch six-blade coefficient model. This is not a 568F map.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropellerSurrogate {
    /// Minimum blade angle admitted by the generic governor, deg.
    pub minimum_blade_angle_deg: f64,
    /// Maximum blade angle admitted by the generic governor, deg.
    pub maximum_blade_angle_deg: f64,
}

impl PropellerSurrogate {
    /// Construct the unvalidated generic six-blade coefficient surrogate.
    #[must_use]
    pub const fn generic_six_blade() -> Self {
        Self {
            // Wide generic governor envelope, not asserted as 568F mechanical
            // pitch stops. It permits low positive loading and reserve-power
            // absorption without silently clipping either operating point.
            // Extended into a generic low/negative-pitch region so the
            // explicitly unvalidated flight-idle schedule can be solved at
            // descent advance ratios. This is not a 568F mechanical stop.
            minimum_blade_angle_deg: -20.0,
            maximum_blade_angle_deg: 60.0,
        }
    }

    fn coefficients(self, advance_ratio: f64, blade_angle_deg: f64) -> (f64, f64) {
        // Smooth generic surrogate chosen for bounded preliminary calculations.
        // It must be replaced by digitised or measured 568F CT/CP surfaces for
        // aircraft validation. beta controls aerodynamic loading; J unloads it.
        let beta = blade_angle_deg.to_radians();
        let ct = 0.055 + 0.23 * beta - 0.035 * advance_ratio;
        let cp = 0.035 + 0.19 * beta + 0.018 * advance_ratio * advance_ratio;
        (ct, cp)
    }
}

/// Detailed output for one free-turbine engine and one propeller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurbopropOutput {
    /// Free-power-turbine shaft output before accessories, W.
    pub engine_shaft_power_w: f64,
    /// Mechanical accessory extraction, W.
    pub accessory_power_w: f64,
    /// Mechanical power dissipated in the reduction gearbox, W.
    pub gearbox_loss_w: f64,
    /// Mechanical power absorbed by the propeller, W.
    pub propeller_power_w: f64,
    /// Propeller shaft torque, N*m.
    pub propeller_torque_n_m: f64,
    /// Propeller aerodynamic thrust, N.
    pub propeller_thrust_n: f64,
    /// Residual core exhaust thrust, N.
    pub residual_jet_thrust_n: f64,
    /// Sum of propeller and residual-jet thrust, N.
    pub total_thrust_n: f64,
    /// Jet-A consumption predicted from shaft power and the PSFC prior, kg/s.
    pub fuel_flow_kg_s: f64,
    /// Blade angle selected by the generic governor, deg.
    pub blade_angle_deg: f64,
    /// Nondimensional advance ratio, `J = V/(n D)`.
    pub advance_ratio: f64,
    /// Useful propulsive power divided by propeller shaft power.
    pub propulsive_efficiency: f64,
    /// Shaft-accessory-gearbox-propeller power closure, W.
    pub power_balance_residual_w: f64,
    /// No quantitative 568F uncertainty can be defended without public maps.
    pub propeller_model_uncertainty: ModelUncertainty,
    /// Human-readable source and evidence qualification.
    pub provenance: &'static str,
}

/// Evidence-supported uncertainty classification of the propeller model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelUncertainty {
    /// The model is structurally useful, but no evidence supports a numeric band.
    UnquantifiedSurrogate,
}

/// Typed failures from the isolated turboprop kernel.
#[derive(Debug, Clone, PartialEq)]
pub enum TurbopropError {
    /// Named input was NaN or infinite.
    NonFinite(&'static str),
    /// Named input is outside the declared numerical or physical domain.
    OutsideDomain {
        /// Stable name of the rejected input.
        field: &'static str,
        /// Rejected value in the field's documented SI unit.
        value: f64,
    },
    /// Requested propeller state lacks a defensible public model.
    UnsupportedMode(TurbopropMode),
    /// No blade angle within the surrogate pitch bounds absorbs the requested power.
    GovernorNoSolution {
        /// Propeller power that could not be matched within pitch limits, W.
        required_power_w: f64,
    },
    /// A calculation completed but violated a physical invariant.
    NonPhysicalResult(&'static str),
}

impl fmt::Display for TurbopropError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite(field) => write!(formatter, "non-finite turboprop input: {field}"),
            Self::OutsideDomain { field, value } => {
                write!(formatter, "turboprop input outside domain: {field}={value}")
            }
            Self::UnsupportedMode(mode) => {
                write!(formatter, "unsupported turboprop mode: {mode:?}")
            }
            Self::GovernorNoSolution { required_power_w } => write!(
                formatter,
                "propeller governor cannot absorb requested power {required_power_w} W"
            ),
            Self::NonPhysicalResult(message) => {
                write!(formatter, "nonphysical turboprop result: {message}")
            }
        }
    }
}

impl Error for TurbopropError {}

impl Pw127m568fModel {
    fn rated_shaft_power_w(self, rating: Pw127mRating) -> f64 {
        match rating {
            Pw127mRating::NormalTakeoff => self.normal_takeoff_power_w,
            Pw127mRating::MaximumTakeoffReserve => self.maximum_takeoff_reserve_power_w,
            Pw127mRating::MaximumContinuous => self.maximum_continuous_power_w,
            Pw127mRating::MaximumClimb => self.maximum_climb_power_w,
            Pw127mRating::MaximumCruise => self.maximum_cruise_power_w,
            Pw127mRating::FlightIdleSurrogate => 0.08 * self.maximum_continuous_power_w,
        }
    }

    fn power_lapse_fraction(self, density_kg_m3: f64) -> f64 {
        let density_ratio = (density_kg_m3 / self.power_lapse_reference_density_kg_m3).min(1.0);
        density_ratio
            .powf(self.power_lapse_density_exponent)
            .max(self.minimum_power_lapse_fraction)
    }

    /// Evaluate one engine and propeller. No limit is silently clipped.
    pub fn evaluate(
        self,
        condition: TurbopropCondition,
        command: TurbopropCommand,
    ) -> Result<TurbopropOutput, TurbopropError> {
        self.validate(condition, command)?;
        if matches!(
            command.mode,
            TurbopropMode::Reverse | TurbopropMode::Feathered
        ) {
            return Err(TurbopropError::UnsupportedMode(command.mode));
        }
        if command.mode == TurbopropMode::Shutdown {
            return Ok(TurbopropOutput {
                engine_shaft_power_w: 0.0,
                accessory_power_w: 0.0,
                gearbox_loss_w: 0.0,
                propeller_power_w: 0.0,
                propeller_torque_n_m: 0.0,
                propeller_thrust_n: 0.0,
                residual_jet_thrust_n: 0.0,
                total_thrust_n: 0.0,
                fuel_flow_kg_s: 0.0,
                blade_angle_deg: self.surrogate.minimum_blade_angle_deg,
                advance_ratio: 0.0,
                propulsive_efficiency: 0.0,
                power_balance_residual_w: 0.0,
                propeller_model_uncertainty: ModelUncertainty::UnquantifiedSurrogate,
                provenance: PROVENANCE,
            });
        }

        let engine_power_w = self.rated_shaft_power_w(command.rating)
            * command.power_fraction
            * self.power_lapse_fraction(condition.density_kg_m3);
        if engine_power_w <= self.accessory_power_w {
            return Err(TurbopropError::OutsideDomain {
                field: "engine_shaft_power_minus_accessories_w",
                value: engine_power_w - self.accessory_power_w,
            });
        }
        let gearbox_input_w = engine_power_w - self.accessory_power_w;
        let propeller_power_w = gearbox_input_w * self.gearbox_efficiency;
        let gearbox_loss_w = gearbox_input_w - propeller_power_w;
        let revolutions_s = command.propeller_speed_rpm / 60.0;
        let omega_rad_s = 2.0 * PI * revolutions_s;
        let advance_ratio =
            condition.true_airspeed_m_s / (revolutions_s * self.propeller_diameter_m);

        let disk_area_m2 = PI * self.propeller_diameter_m.powi(2) / 4.0;
        let ideal_static_thrust_n =
            (2.0 * condition.density_kg_m3 * disk_area_m2 * propeller_power_w.powi(2)).cbrt();
        let static_thrust_n = self.static_figure_of_merit.powf(2.0 / 3.0) * ideal_static_thrust_n;
        // Public 568F data do not define the transition from static actuator-disk
        // behavior to the generic J-based surrogate. Blend them over 0..5 m/s
        // with smoothstep (zero slope at both ends), avoiding a regime switch or
        // force jump while keeping the exact finite actuator-disk limit at V=0.
        let transition_end_m_s = 5.0;
        let transition_fraction = condition.true_airspeed_m_s / transition_end_m_s;
        let transition_weight = if transition_fraction >= 1.0 {
            1.0
        } else {
            transition_fraction.powi(2) * (3.0 - 2.0 * transition_fraction)
        };
        let governor_solution = self.solve_governor(
            condition.density_kg_m3,
            revolutions_s,
            advance_ratio,
            propeller_power_w,
            command.mode == TurbopropMode::FlightIdle,
        );
        let (governed_angle_deg, governed_thrust_n) = match governor_solution {
            Ok(solution) => solution,
            Err(TurbopropError::GovernorNoSolution { .. })
            | Err(TurbopropError::NonPhysicalResult("negative forward thrust")) => {
                // Outside the generic CP surface, retain the nearest pitch-bound
                // indication but do not discard available engine power. Applying
                // the static figure of merit to shaft power in ideal momentum
                // theory is the minimum-hypothesis extension: at V=0 it exactly
                // recovers T = FM^(2/3) T_ideal, while remaining power-bounded at
                // every forward speed.
                let power_scale = condition.density_kg_m3
                    * revolutions_s.powi(3)
                    * self.propeller_diameter_m.powi(5);
                let lower_angle_deg = self.surrogate.minimum_blade_angle_deg;
                let upper_angle_deg = self.surrogate.maximum_blade_angle_deg;
                let lower_power_w = self
                    .surrogate
                    .coefficients(advance_ratio, lower_angle_deg)
                    .1
                    * power_scale;
                let upper_power_w = self
                    .surrogate
                    .coefficients(advance_ratio, upper_angle_deg)
                    .1
                    * power_scale;
                let nearest_angle_deg = if (lower_power_w - propeller_power_w).abs()
                    <= (upper_power_w - propeller_power_w).abs()
                {
                    lower_angle_deg
                } else {
                    upper_angle_deg
                };
                let extrapolated_thrust_n = if command.mode == TurbopropMode::FlightIdle {
                    // No public 568F windmilling/idle map supports converting
                    // residual idle shaft power into positive cruise-like thrust.
                    // Zero is the conservative minimum-hypothesis fallback.
                    0.0
                } else {
                    actuator_disk_thrust_bound_n(
                        self.static_figure_of_merit * propeller_power_w,
                        condition.density_kg_m3,
                        disk_area_m2,
                        condition.true_airspeed_m_s,
                    )
                };
                (nearest_angle_deg, extrapolated_thrust_n)
            }
            Err(error) => return Err(error),
        };
        let blade_angle_deg = self.surrogate.minimum_blade_angle_deg
            + transition_weight * (governed_angle_deg - self.surrogate.minimum_blade_angle_deg);
        let blended_thrust_n =
            static_thrust_n + transition_weight * (governed_thrust_n - static_thrust_n);
        // The generic CT/CP surface is not an energy-consistent propeller map.
        // Bound positive thrust by ideal one-dimensional actuator-disk momentum
        // theory at the actual shaft power. This preserves the surrogate below
        // the bound and prevents it from creating propulsive power. The static
        // fallback remains lower than this ideal bound through its measured-class
        // figure-of-merit correction above.
        let ideal_thrust_bound_n = actuator_disk_thrust_bound_n(
            propeller_power_w,
            condition.density_kg_m3,
            disk_area_m2,
            condition.true_airspeed_m_s,
        );
        let energy_bounded_thrust_n = blended_thrust_n.min(ideal_thrust_bound_n);
        let propeller_thrust_n = if command.mode == TurbopropMode::FlightIdle {
            // The generic powered CT polynomial is not a windmilling map and
            // predicts several kilonewtons of drag at some descent points. With
            // neither OEM idle pitch nor drag data, zero propeller force is the
            // bounded neutral hypothesis. Nacelle/airframe drag remains outside
            // this isolated propulsion kernel.
            0.0
        } else {
            energy_bounded_thrust_n
        };

        let useful_power_w = propeller_thrust_n * condition.true_airspeed_m_s;
        let efficiency = if command.mode == TurbopropMode::FlightIdle {
            0.0
        } else {
            useful_power_w / propeller_power_w
        };
        if !(0.0..=1.0).contains(&efficiency) {
            return Err(TurbopropError::NonPhysicalResult(
                "propulsive efficiency is outside [0, 1]",
            ));
        }
        // The 762 kg/h anchor is an optimum-altitude cruise datum, whereas the
        // typed rating is sea-level power. Correct the rating-basis coefficient
        // by the lapse at the declared fuel-reference density so the anchor is
        // reproduced after applying actual shaft-power lapse.
        let fuel_reference_lapse = self.power_lapse_fraction(self.fuel_reference_density_kg_m3);
        let calibrated_psfc_kg_kwh = self.reference_psfc_kg_kwh / fuel_reference_lapse;
        let fuel_flow_kg_s = engine_power_w * calibrated_psfc_kg_kwh / JOULES_PER_KWH;
        let output = TurbopropOutput {
            engine_shaft_power_w: engine_power_w,
            accessory_power_w: self.accessory_power_w,
            gearbox_loss_w,
            propeller_power_w,
            propeller_torque_n_m: propeller_power_w / omega_rad_s,
            propeller_thrust_n,
            residual_jet_thrust_n: self.residual_jet_thrust_n,
            total_thrust_n: propeller_thrust_n + self.residual_jet_thrust_n,
            fuel_flow_kg_s,
            blade_angle_deg,
            advance_ratio,
            propulsive_efficiency: efficiency,
            power_balance_residual_w: engine_power_w
                - self.accessory_power_w
                - gearbox_loss_w
                - propeller_power_w,
            propeller_model_uncertainty: ModelUncertainty::UnquantifiedSurrogate,
            provenance: PROVENANCE,
        };
        if !output.total_thrust_n.is_finite() || !output.fuel_flow_kg_s.is_finite() {
            return Err(TurbopropError::NonPhysicalResult("non-finite output"));
        }
        Ok(output)
    }

    fn solve_governor(
        self,
        density_kg_m3: f64,
        revolutions_s: f64,
        advance_ratio: f64,
        required_power_w: f64,
        allow_negative_thrust: bool,
    ) -> Result<(f64, f64), TurbopropError> {
        let power_scale = density_kg_m3 * revolutions_s.powi(3) * self.propeller_diameter_m.powi(5);
        let thrust_scale =
            density_kg_m3 * revolutions_s.powi(2) * self.propeller_diameter_m.powi(4);
        let residual = |angle: f64| {
            self.surrogate.coefficients(advance_ratio, angle).1 * power_scale - required_power_w
        };
        let mut lower = self.surrogate.minimum_blade_angle_deg;
        let mut upper = self.surrogate.maximum_blade_angle_deg;
        if residual(lower) * residual(upper) > 0.0 {
            return Err(TurbopropError::GovernorNoSolution { required_power_w });
        }
        for _ in 0..60 {
            let middle = 0.5 * (lower + upper);
            if residual(middle) > 0.0 {
                upper = middle;
            } else {
                lower = middle;
            }
        }
        let angle = 0.5 * (lower + upper);
        let (thrust_coefficient, _) = self.surrogate.coefficients(advance_ratio, angle);
        let thrust_n = thrust_coefficient * thrust_scale;
        if thrust_n < 0.0 && !allow_negative_thrust {
            return Err(TurbopropError::NonPhysicalResult("negative forward thrust"));
        }
        Ok((angle, thrust_n))
    }

    fn validate(
        self,
        condition: TurbopropCondition,
        command: TurbopropCommand,
    ) -> Result<(), TurbopropError> {
        for (name, value) in [
            ("density_kg_m3", condition.density_kg_m3),
            ("true_airspeed_m_s", condition.true_airspeed_m_s),
            ("power_fraction", command.power_fraction),
            ("propeller_speed_rpm", command.propeller_speed_rpm),
            ("propeller_diameter_m", self.propeller_diameter_m),
            ("normal_takeoff_power_w", self.normal_takeoff_power_w),
            (
                "maximum_takeoff_reserve_power_w",
                self.maximum_takeoff_reserve_power_w,
            ),
            (
                "maximum_continuous_power_w",
                self.maximum_continuous_power_w,
            ),
            ("maximum_climb_power_w", self.maximum_climb_power_w),
            ("maximum_cruise_power_w", self.maximum_cruise_power_w),
            (
                "governed_propeller_speed_rpm",
                self.governed_propeller_speed_rpm,
            ),
            ("gearbox_efficiency", self.gearbox_efficiency),
            ("accessory_power_w", self.accessory_power_w),
            ("reference_psfc_kg_kwh", self.reference_psfc_kg_kwh),
            (
                "fuel_reference_density_kg_m3",
                self.fuel_reference_density_kg_m3,
            ),
            ("static_figure_of_merit", self.static_figure_of_merit),
            (
                "power_lapse_reference_density_kg_m3",
                self.power_lapse_reference_density_kg_m3,
            ),
            (
                "power_lapse_density_exponent",
                self.power_lapse_density_exponent,
            ),
            (
                "minimum_power_lapse_fraction",
                self.minimum_power_lapse_fraction,
            ),
        ] {
            if !value.is_finite() {
                return Err(TurbopropError::NonFinite(name));
            }
        }
        for (field, value, minimum, maximum) in [
            (
                "density_kg_m3",
                condition.density_kg_m3,
                f64::MIN_POSITIVE,
                2.0,
            ),
            ("true_airspeed_m_s", condition.true_airspeed_m_s, 0.0, 250.0),
            ("power_fraction", command.power_fraction, 0.0, 1.0),
            (
                "propeller_speed_rpm",
                command.propeller_speed_rpm,
                100.0,
                2_000.0,
            ),
            (
                "gearbox_efficiency",
                self.gearbox_efficiency,
                f64::MIN_POSITIVE,
                1.0,
            ),
            (
                "static_figure_of_merit",
                self.static_figure_of_merit,
                f64::MIN_POSITIVE,
                1.0,
            ),
            (
                "power_lapse_reference_density_kg_m3",
                self.power_lapse_reference_density_kg_m3,
                f64::MIN_POSITIVE,
                2.0,
            ),
            (
                "fuel_reference_density_kg_m3",
                self.fuel_reference_density_kg_m3,
                f64::MIN_POSITIVE,
                2.0,
            ),
            (
                "power_lapse_density_exponent",
                self.power_lapse_density_exponent,
                f64::MIN_POSITIVE,
                2.0,
            ),
            (
                "minimum_power_lapse_fraction",
                self.minimum_power_lapse_fraction,
                f64::MIN_POSITIVE,
                1.0,
            ),
        ] {
            if value < minimum || value > maximum {
                return Err(TurbopropError::OutsideDomain { field, value });
            }
        }
        if self.propeller_diameter_m <= 0.0
            || self.reference_psfc_kg_kwh <= 0.0
            || self.normal_takeoff_power_w <= 0.0
            || self.maximum_takeoff_reserve_power_w <= 0.0
            || self.maximum_continuous_power_w <= 0.0
            || self.maximum_climb_power_w <= 0.0
            || self.maximum_cruise_power_w <= 0.0
            || !(100.0..=2_000.0).contains(&self.governed_propeller_speed_rpm)
        {
            return Err(TurbopropError::OutsideDomain {
                field: "positive_model_parameter",
                value: self.propeller_diameter_m.min(self.reference_psfc_kg_kwh),
            });
        }
        Ok(())
    }
}

/// Ideal positive-thrust limit from one-dimensional actuator-disk theory.
///
/// The disk velocity `u` obeys `P = 2 rho A u^2 (u - V)` and thrust is
/// `T = P / u`. The physical root is unique for `u >= V`; bisection avoids a
/// poorly conditioned closed-form cubic near the static limit.
fn actuator_disk_thrust_bound_n(
    shaft_power_w: f64,
    density_kg_m3: f64,
    disk_area_m2: f64,
    true_airspeed_m_s: f64,
) -> f64 {
    let ideal_static_thrust_n = (2.0 * density_kg_m3 * disk_area_m2 * shaft_power_w.powi(2)).cbrt();
    if true_airspeed_m_s == 0.0 {
        return ideal_static_thrust_n;
    }

    let ideal_power_for_thrust = |thrust_n: f64| {
        let induced_velocity_m_s = 0.5
            * ((true_airspeed_m_s.powi(2) + 2.0 * thrust_n / (density_kg_m3 * disk_area_m2))
                .sqrt()
                - true_airspeed_m_s);
        thrust_n * (true_airspeed_m_s + induced_velocity_m_s)
    };
    let mut lower_thrust_n = 0.0;
    let mut upper_thrust_n = ideal_static_thrust_n;
    for _ in 0..60 {
        let middle_thrust_n = 0.5 * (lower_thrust_n + upper_thrust_n);
        if ideal_power_for_thrust(middle_thrust_n) > shaft_power_w {
            upper_thrust_n = middle_thrust_n;
        } else {
            lower_thrust_n = middle_thrust_n;
        }
    }
    0.5 * (lower_thrust_n + upper_thrust_n)
}

const PROVENANCE: &str = "PW127M takeoff/continuous ratings: certification evidence; ATR 72-600 climb/cruise ratings, 568F-1 diameter, 762 kg/h maximum-cruise fuel flow and 17.5 min climb to FL170: ATR manufacturer factsheet; rated shaft power lapses as max(0.15, min(1, rho/1.225)^0.75), an aircraft-level minimum-hypothesis calibration rather than an OEM engine deck; the 762 kg/h fuel anchor is calibrated after power lapse at rho=0.70 kg/m3 (FL170-like optimum-altitude cruise), so sea-level use remains extrapolated; propeller coefficients: generic six-blade surrogate (not OEM 568F data), bounded by ideal one-dimensional actuator-disk momentum theory; outside the generic governor surface, actuator-disk thrust uses static figure of merit as an effective-power loss factor; flight-idle propeller force is the neutral zero-force hypothesis because no OEM idle/windmilling map is available; fuel model has one aircraft-level calibration anchor, not a PW127M deck";

/// Technology-neutral adapter for the two-engine ATR 72 PW127M/568F installation.
///
/// It aggregates exactly two independent engine/propeller evaluations. It does
/// not claim an altitude-lapse deck, a measured 568F coefficient map, a PW127M
/// fuel map, validated flight-idle, feather/windmill, or reverse-beta physics.
pub struct Atr72TurbopropSystem {
    unit_model: Pw127m568fModel,
    provenance: ModelProvenance,
    mass_inventory: Vec<PropulsionMassItem>,
    installation: PropulsionInstallation,
}

impl Atr72TurbopropSystem {
    /// Construct a two-unit ATR propulsion adapter from explicit installation data.
    ///
    /// Exactly two positions and two thrust axes are required. Installation mass
    /// remains caller-owned evidence because a defensible common definition for
    /// dry engine, propeller, nacelle, fluids, and mounting mass is not yet fixed.
    pub fn new(
        unit_model: Pw127m568fModel,
        mass_inventory: Vec<PropulsionMassItem>,
        installation: PropulsionInstallation,
    ) -> Result<Self, PropulsionError> {
        if installation.unit_positions_m.len() != 2 || installation.thrust_axes_body.len() != 2 {
            return Err(PropulsionError::OutsideModelDomain(
                "ATR 72 adapter requires exactly two positions and two thrust axes".to_owned(),
            ));
        }
        for axis in &installation.thrust_axes_body {
            let magnitude = (axis[0].powi(2) + axis[1].powi(2) + axis[2].powi(2)).sqrt();
            if !magnitude.is_finite() || (magnitude - 1.0).abs() > 1.0e-9 {
                return Err(PropulsionError::OutsideModelDomain(
                    "ATR 72 thrust axes must be finite unit vectors".to_owned(),
                ));
            }
        }
        Ok(Self {
            unit_model,
            provenance: ModelProvenance {
                model: ModelIdentity {
                    family: "pw127m-568f-turboprop-surrogate".to_owned(),
                    version: "1".to_owned(),
                },
                dataset: Some("ATR 72-212A / PW127M / 568F-1 preliminary".to_owned()),
                sources: vec![
                    "EASA TCDS E.041: PW127M certified shaft-power ratings".to_owned(),
                    "EASA TCDS A.084 and ATR public data: ATR 72 installation and 3.93 m 568F-1 propeller".to_owned(),
                    "ATR 72-600 factsheet: 762 kg/h two-engine fuel flow at maximum cruise; single-point PSFC calibration only".to_owned(),
                    "Generic six-blade CT/CP/J surrogate: no public OEM 568F map; unvalidated".to_owned(),
                ],
            },
            mass_inventory,
            installation,
        })
    }

    fn active_units(&self, failure: &FailureState) -> Result<Vec<usize>, PropulsionError> {
        match failure {
            FailureState::None => Ok(vec![0, 1]),
            FailureState::UnitsUnavailable(indices) => {
                if indices.iter().any(|index| *index > 1)
                    || (indices.len() == 2 && indices[0] == indices[1])
                    || indices.len() > 2
                {
                    return Err(PropulsionError::UnsupportedFailureState);
                }
                Ok((0..2).filter(|index| !indices.contains(index)).collect())
            }
        }
    }

    fn rating_and_fraction(
        &self,
        demand: PropulsionDemand,
        active_count: usize,
    ) -> Result<(Pw127mRating, f64), PropulsionError> {
        let takeoff = if active_count == 1 {
            Pw127mRating::MaximumTakeoffReserve
        } else {
            Pw127mRating::NormalTakeoff
        };
        match demand {
            PropulsionDemand::NormalizedForce(value)
                if value.is_finite() && (0.0..=1.0).contains(&value) =>
            {
                Ok((takeoff, value))
            }
            PropulsionDemand::NormalizedForce(value) => Err(PropulsionError::InvalidInput {
                field: "normalized propulsion demand",
                value,
            }),
            PropulsionDemand::Rating(PropulsionRating::TakeoffGoAround) => Ok((takeoff, 1.0)),
            PropulsionDemand::Rating(PropulsionRating::MaximumContinuous) => {
                Ok((Pw127mRating::MaximumContinuous, 1.0))
            }
            PropulsionDemand::Rating(PropulsionRating::MaximumClimb) => {
                Ok((Pw127mRating::MaximumClimb, 1.0))
            }
            PropulsionDemand::Rating(PropulsionRating::Cruise) => {
                Ok((Pw127mRating::MaximumCruise, 1.0))
            }
            PropulsionDemand::Rating(PropulsionRating::FlightIdle) => {
                Ok((Pw127mRating::FlightIdleSurrogate, 1.0))
            }
            PropulsionDemand::RatedFraction { rating, fraction }
                if fraction.is_finite() && (0.0..=1.0).contains(&fraction) =>
            {
                let (rating, _) =
                    self.rating_and_fraction(PropulsionDemand::Rating(rating), active_count)?;
                Ok((rating, fraction))
            }
            PropulsionDemand::RatedFraction { fraction, .. } => {
                Err(PropulsionError::InvalidInput {
                    field: "rated force fraction",
                    value: fraction,
                })
            }
            PropulsionDemand::RequiredBodyForceN(_) => Err(PropulsionError::UnsupportedDemand(
                "ATR turboprop surrogate has no inverse force solver",
            )),
        }
    }

    fn solve_normalized_force_fraction(
        unit_model: Pw127m568fModel,
        condition: TurbopropCondition,
        rating: Pw127mRating,
        mode: TurbopropMode,
        requested_force_fraction: f64,
    ) -> Result<f64, PropulsionError> {
        let output_at = |power_fraction| {
            unit_model.evaluate(
                condition,
                TurbopropCommand {
                    rating,
                    power_fraction,
                    mode,
                    propeller_speed_rpm: unit_model.governed_propeller_speed_rpm,
                },
            )
        };
        let maximum = output_at(1.0).map_err(Self::map_error)?.total_thrust_n;
        let target = requested_force_fraction * maximum;
        let mut lower = None;
        for step in 1..=1_000 {
            let fraction = f64::from(step) / 1_000.0;
            if let Ok(output) = output_at(fraction) {
                lower = Some((fraction, output.total_thrust_n));
                break;
            }
        }
        let (mut low, minimum) = lower.ok_or_else(|| {
            PropulsionError::OutsideModelDomain(
                "no governed operating point exists below full rating".to_owned(),
            )
        })?;
        if target < minimum {
            return Err(PropulsionError::OutsideModelDomain(
                "requested force is below the lowest governed point; no PW127M flight-idle schedule is available"
                    .to_owned(),
            ));
        }
        let mut high = 1.0;
        for _ in 0..60 {
            let middle = 0.5 * (low + high);
            let thrust = output_at(middle).map_err(Self::map_error)?.total_thrust_n;
            if thrust < target {
                low = middle;
            } else {
                high = middle;
            }
        }
        Ok(0.5 * (low + high))
    }

    fn map_error(error: TurbopropError) -> PropulsionError {
        match error {
            TurbopropError::NonFinite(field) => PropulsionError::InvalidInput {
                field,
                value: f64::NAN,
            },
            TurbopropError::OutsideDomain { field, value } => {
                PropulsionError::InvalidInput { field, value }
            }
            TurbopropError::UnsupportedMode(_) => PropulsionError::OutsideModelDomain(
                "requested PW127M/568F operating mode lacks a public calibrated map".to_owned(),
            ),
            TurbopropError::GovernorNoSolution { required_power_w } => {
                PropulsionError::OutsideModelDomain(format!(
                    "generic propeller governor cannot absorb {required_power_w} W"
                ))
            }
            TurbopropError::NonPhysicalResult(field) => PropulsionError::NonFiniteOutput(field),
        }
    }

    fn extrapolated() -> ValidityStatus {
        ValidityStatus::Extrapolated {
            reason: "generic unvalidated six-blade propeller surrogate, family-level PSFC prior, and no PW127M altitude-lapse deck".to_owned(),
        }
    }

    fn zero_result(&self, limit_name: &str) -> PropulsionResult {
        PropulsionResult {
            body_force_n: [0.0; 3],
            body_moment_nm: [0.0; 3],
            resource_flows: vec![ResourceFlow {
                resource: ResourceKind::JetA,
                mass_flow_kg_s: Some(0.0),
                power_w: None,
            }],
            state_derivatives: vec![StateDerivative {
                state: "jet_a_mass_kg".to_owned(),
                rate_per_s: 0.0,
                unit: "kg/s".to_owned(),
            }],
            shaft_power_w: Some(0.0),
            electrical_power_w: None,
            heat_rejection_w: None,
            torque_nm: Some(0.0),
            rotational_speed_rpm: Some(0.0),
            achieved_demand: PropulsionDemand::NormalizedForce(0.0),
            active_limits: vec![ActiveLimit {
                name: limit_name.to_owned(),
                utilization: 1.0,
            }],
            residuals: vec![Residual {
                name: "shaft power balance".to_owned(),
                value: 0.0,
                unit: "W".to_owned(),
            }],
            validity: Self::extrapolated(),
            provenance: self.provenance.clone(),
            trace: None,
        }
    }
}

impl PropulsionSystemModel for Atr72TurbopropSystem {
    fn evaluate(&self, request: &PropulsionRequest) -> Result<PropulsionResult, PropulsionError> {
        if request.loads.bleed_mass_flow_kg_s != 0.0 || request.loads.electrical_power_w != 0.0 {
            return Err(PropulsionError::UnsupportedDemand(
                "PW127M surrogate supports mechanical accessory extraction only",
            ));
        }
        if !request.state.values.is_empty() {
            return Err(PropulsionError::UnsupportedDemand(
                "PW127M surrogate has no dynamic state contract",
            ));
        }
        let active = self.active_units(&request.failure)?;
        let mut mode = match request.mode {
            OperatingMode::Normal => TurbopropMode::Governed,
            OperatingMode::Shutdown => {
                if request.demand != PropulsionDemand::NormalizedForce(0.0) {
                    return Err(PropulsionError::UnsupportedDemand(
                        "shutdown requires exactly zero normalized demand",
                    ));
                }
                return Ok(self.zero_result("propulsion shutdown commanded"));
            }
            OperatingMode::FlightIdle => TurbopropMode::FlightIdle,
            other => return Err(PropulsionError::UnsupportedMode(other)),
        };
        let (rating, requested_fraction) =
            self.rating_and_fraction(request.demand, active.len())?;
        if rating == Pw127mRating::FlightIdleSurrogate {
            mode = TurbopropMode::FlightIdle;
        }
        if active.is_empty() {
            return Ok(
                self.zero_result("zero capability: both installed propulsion units unavailable")
            );
        }
        let extra_accessory_per_unit = request.loads.accessory_power_w / active.len() as f64;
        if !extra_accessory_per_unit.is_finite() || extra_accessory_per_unit < 0.0 {
            return Err(PropulsionError::InvalidInput {
                field: "accessory power",
                value: request.loads.accessory_power_w,
            });
        }
        let mut unit_model = self.unit_model;
        unit_model.accessory_power_w += extra_accessory_per_unit;
        let force_fraction = match request.demand {
            PropulsionDemand::NormalizedForce(value) => Some(value),
            _ => None,
        };
        let solved_fraction = if let Some(force_fraction) = force_fraction {
            Self::solve_normalized_force_fraction(
                unit_model,
                TurbopropCondition {
                    density_kg_m3: request.flight.density_kg_m3,
                    true_airspeed_m_s: request.flight.velocity_m_s,
                },
                rating,
                mode,
                force_fraction,
            )?
        } else {
            requested_fraction
        };
        let command_power_fraction = match request.demand {
            PropulsionDemand::RatedFraction { .. }
                if rating != Pw127mRating::FlightIdleSurrogate =>
            {
                let rated_power_w = unit_model.rated_shaft_power_w(rating);
                let idle_power_w =
                    unit_model.rated_shaft_power_w(Pw127mRating::FlightIdleSurrogate);
                (idle_power_w + solved_fraction * (rated_power_w - idle_power_w)) / rated_power_w
            }
            _ => solved_fraction,
        };
        let reported_utilization = match request.demand {
            PropulsionDemand::RatedFraction { fraction, .. } => fraction,
            _ => solved_fraction,
        };
        let command = TurbopropCommand {
            rating,
            power_fraction: command_power_fraction,
            mode,
            propeller_speed_rpm: unit_model.governed_propeller_speed_rpm,
        };
        let condition = TurbopropCondition {
            density_kg_m3: request.flight.density_kg_m3,
            true_airspeed_m_s: request.flight.velocity_m_s,
        };
        let mut force = [0.0; 3];
        let mut moment = [0.0; 3];
        let mut fuel_flow = 0.0;
        let mut shaft_power = 0.0;
        let mut torque = 0.0;
        let mut power_residual = 0.0;
        for index in active {
            let output = unit_model
                .evaluate(condition, command)
                .map_err(Self::map_error)?;
            let axis = self.installation.thrust_axes_body[index];
            let unit_force = [
                axis[0] * output.total_thrust_n,
                axis[1] * output.total_thrust_n,
                axis[2] * output.total_thrust_n,
            ];
            for component in 0..3 {
                force[component] += unit_force[component];
            }
            let position = self.installation.unit_positions_m[index];
            moment[0] += position[1] * unit_force[2] - position[2] * unit_force[1];
            moment[1] += position[2] * unit_force[0] - position[0] * unit_force[2];
            moment[2] += position[0] * unit_force[1] - position[1] * unit_force[0];
            fuel_flow += output.fuel_flow_kg_s;
            shaft_power += output.propeller_power_w;
            torque += output.propeller_torque_n_m;
            power_residual += output.power_balance_residual_w;
        }
        Ok(PropulsionResult {
            body_force_n: force,
            body_moment_nm: moment,
            resource_flows: vec![ResourceFlow {
                resource: ResourceKind::JetA,
                mass_flow_kg_s: Some(fuel_flow),
                power_w: None,
            }],
            state_derivatives: vec![StateDerivative {
                state: "jet_a_mass_kg".to_owned(),
                rate_per_s: -fuel_flow,
                unit: "kg/s".to_owned(),
            }],
            shaft_power_w: Some(shaft_power),
            electrical_power_w: None,
            heat_rejection_w: None,
            torque_nm: Some(torque),
            rotational_speed_rpm: Some(unit_model.governed_propeller_speed_rpm),
            achieved_demand: request.demand,
            active_limits: vec![ActiveLimit {
                name: format!("PW127M {rating:?}"),
                utilization: reported_utilization,
            }],
            residuals: vec![Residual {
                name: "shaft power balance".to_owned(),
                value: power_residual,
                unit: "W".to_owned(),
            }],
            validity: Self::extrapolated(),
            provenance: self.provenance.clone(),
            trace: None,
        })
    }

    fn capability(
        &self,
        flight: FlightCondition,
        failure: FailureState,
    ) -> Result<PropulsionCapability, PropulsionError> {
        let result = self.evaluate(&PropulsionRequest {
            flight,
            demand: PropulsionDemand::Rating(PropulsionRating::TakeoffGoAround),
            mode: OperatingMode::Normal,
            failure,
            loads: Default::default(),
            state: Default::default(),
            time_step_s: None,
        })?;
        Ok(PropulsionCapability {
            maximum_body_force_n: result.body_force_n,
            minimum_body_force_n: [0.0; 3],
            maximum_shaft_power_w: result.shaft_power_w,
            active_limits: result.active_limits,
            validity: result.validity,
            provenance: self.provenance.clone(),
        })
    }

    fn mass_inventory(&self) -> &[PropulsionMassItem] {
        &self.mass_inventory
    }

    fn installation(&self) -> &PropulsionInstallation {
        &self.installation
    }

    fn provenance(&self) -> &ModelProvenance {
        &self.provenance
    }

    fn diagnostics(&self, query: DiagnosticsQuery) -> PropulsionDiagnostics {
        let (code, message) = match query {
            DiagnosticsQuery::Provenance => (
                "atr72-turboprop-surrogate-provenance",
                PROVENANCE,
            ),
            DiagnosticsQuery::SupportedSemantics => (
                "atr72-turboprop-surrogate-semantics",
                "Two installed engines; normalized-force inverse control; takeoff, maximum-continuous, maximum-climb, maximum-cruise and unvalidated flight-idle ratings; normal, flight-idle and shutdown modes; OEI automatic-reserve rating; mechanical accessory load.",
            ),
            DiagnosticsQuery::ValidityDomain => (
                "atr72-turboprop-surrogate-validity",
                "All points are Extrapolated: public PW127M lapse/fuel decks and Hamilton 568F-1 CT/CP maps are unavailable. Flight idle is a declared surrogate; feather/windmill and reverse are rejected.",
            ),
        };
        PropulsionDiagnostics {
            query,
            items: vec![DiagnosticItem {
                code: code.to_owned(),
                message: message.to_owned(),
            }],
            provenance: self.provenance.clone(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn atr_system() -> Atr72TurbopropSystem {
        Atr72TurbopropSystem::new(
            Pw127m568fModel::default(),
            Vec::new(),
            PropulsionInstallation {
                unit_positions_m: vec![[2.0, -6.0, 0.0], [2.0, 6.0, 0.0]],
                thrust_axes_body: vec![[1.0, 0.0, 0.0]; 2],
                nacelle_wetted_area_m2: None,
                frontal_area_m2: None,
            },
        )
        .unwrap()
    }

    fn static_flight() -> FlightCondition {
        FlightCondition {
            altitude_m: 0.0,
            mach: 0.0,
            pressure_pa: 101_325.0,
            temperature_k: 288.15,
            density_kg_m3: 1.225,
            dynamic_viscosity_pa_s: 1.789e-5,
            gravity_m_s2: 9.806_65,
            gamma: 1.4,
            cp_j_kgk: 1_004.5,
            gas_constant_j_kgk: 287.05,
            speed_of_sound_m_s: 340.3,
            velocity_m_s: 0.0,
            stagnation_temperature_k: 288.15,
            stagnation_pressure_pa: 101_325.0,
        }
    }

    fn system_request(failure: FailureState) -> PropulsionRequest {
        PropulsionRequest {
            flight: static_flight(),
            demand: PropulsionDemand::Rating(PropulsionRating::TakeoffGoAround),
            mode: OperatingMode::Normal,
            failure,
            loads: Default::default(),
            state: Default::default(),
            time_step_s: None,
        }
    }

    fn nominal_command() -> TurbopropCommand {
        TurbopropCommand {
            rating: Pw127mRating::NormalTakeoff,
            power_fraction: 0.70,
            mode: TurbopropMode::Governed,
            propeller_speed_rpm: 1_200.0,
        }
    }

    #[test]
    fn certified_ratings_remain_distinct_and_in_si() {
        let normal = Pw127mRating::NormalTakeoff.shaft_power_w();
        let reserve = Pw127mRating::MaximumTakeoffReserve.shaft_power_w();
        let continuous = Pw127mRating::MaximumContinuous.shaft_power_w();
        let climb = Pw127mRating::MaximumClimb.shaft_power_w();
        let cruise = Pw127mRating::MaximumCruise.shaft_power_w();
        assert!((normal - 1_845_607.183_2).abs() < 1.0);
        assert!(reserve > continuous && continuous > normal);
        assert!(normal > climb && climb > cruise);
        assert!((climb / WATTS_PER_SHP - 2_192.0).abs() < 1.0e-12);
        assert!((cruise / WATTS_PER_SHP - 2_132.0).abs() < 1.0e-12);
    }

    #[test]
    fn sea_level_density_preserves_exact_typed_shaft_rating() {
        let model = Pw127m568fModel::default();
        let output = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: model.power_lapse_reference_density_kg_m3,
                    true_airspeed_m_s: 85.0,
                },
                TurbopropCommand {
                    rating: Pw127mRating::MaximumClimb,
                    power_fraction: 1.0,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: model.governed_propeller_speed_rpm,
                },
            )
            .unwrap();

        assert_eq!(output.engine_shaft_power_w, model.maximum_climb_power_w);
    }

    #[test]
    fn available_shaft_power_decreases_monotonically_with_density() {
        let model = Pw127m568fModel::default();
        let evaluate_power = |density_kg_m3| {
            model
                .evaluate(
                    TurbopropCondition {
                        density_kg_m3,
                        true_airspeed_m_s: 100.0,
                    },
                    TurbopropCommand {
                        rating: Pw127mRating::MaximumClimb,
                        power_fraction: 1.0,
                        mode: TurbopropMode::Governed,
                        propeller_speed_rpm: model.governed_propeller_speed_rpm,
                    },
                )
                .unwrap()
                .engine_shaft_power_w
        };
        let sea_level_power_w = evaluate_power(1.225);
        let intermediate_power_w = evaluate_power(0.90);
        let fl170_power_w = evaluate_power(0.70);

        assert!(sea_level_power_w > intermediate_power_w);
        assert!(intermediate_power_w > fl170_power_w);
        assert_eq!(
            fl170_power_w,
            model.maximum_climb_power_w * (0.70_f64 / 1.225).powf(0.75)
        );
    }

    #[test]
    fn live_typed_rating_values_drive_the_kernel_instead_of_enum_defaults() {
        let model = Pw127m568fModel {
            maximum_cruise_power_w: 1_400_000.0,
            ..Pw127m568fModel::default()
        };
        let output = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: 1.225,
                    true_airspeed_m_s: 0.0,
                },
                TurbopropCommand {
                    rating: Pw127mRating::MaximumCruise,
                    power_fraction: 1.0,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: model.governed_propeller_speed_rpm,
                },
            )
            .unwrap();
        assert_eq!(output.engine_shaft_power_w, 1_400_000.0);
    }

    #[test]
    fn cruise_fuel_anchor_reproduces_the_published_aircraft_flow() {
        let model = Pw127m568fModel::default();
        let command = TurbopropCommand {
            rating: Pw127mRating::MaximumCruise,
            power_fraction: 1.0,
            mode: TurbopropMode::Governed,
            propeller_speed_rpm: model.governed_propeller_speed_rpm,
        };
        let reference_output = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: model.fuel_reference_density_kg_m3,
                    true_airspeed_m_s: 140.0,
                },
                command,
            )
            .unwrap();
        let sea_level_output = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: model.power_lapse_reference_density_kg_m3,
                    true_airspeed_m_s: 140.0,
                },
                command,
            )
            .unwrap();

        assert!((2.0 * reference_output.fuel_flow_kg_s * 3_600.0 - 762.0).abs() < 1.0e-9);
        assert!(sea_level_output.fuel_flow_kg_s > reference_output.fuel_flow_kg_s);
    }

    #[test]
    fn static_thrust_is_finite_without_velocity_division() {
        let model = Pw127m568fModel::default();
        let output = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: 1.225,
                    true_airspeed_m_s: 0.0,
                },
                nominal_command(),
            )
            .unwrap();
        assert!(output.propeller_thrust_n.is_finite());
        assert!(output.propeller_thrust_n > 0.0);
        assert_eq!(output.propulsive_efficiency, 0.0);
    }

    #[test]
    fn low_speed_blend_is_continuous_at_static_and_map_boundary() {
        let evaluate_at = |true_airspeed_m_s| {
            Pw127m568fModel::default()
                .evaluate(
                    TurbopropCondition {
                        density_kg_m3: 1.225,
                        true_airspeed_m_s,
                    },
                    nominal_command(),
                )
                .unwrap()
                .propeller_thrust_n
        };
        let static_thrust = evaluate_at(0.0);
        let near_static_thrust = evaluate_at(1.0e-6);
        assert!((near_static_thrust - static_thrust).abs() / static_thrust < 1.0e-10);
        let below_transition = evaluate_at(5.0 - 1.0e-6);
        let above_transition = evaluate_at(5.0 + 1.0e-6);
        assert!((above_transition - below_transition).abs() / below_transition.abs() < 1.0e-6);
    }

    #[test]
    fn shaft_power_balance_and_torque_units_close() {
        let model = Pw127m568fModel::default();
        let output = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: 1.0,
                    true_airspeed_m_s: 90.0,
                },
                nominal_command(),
            )
            .unwrap();
        assert!(output.power_balance_residual_w.abs() < 1.0e-8);
        let reconstructed_power = output.propeller_torque_n_m * 2.0 * PI * 20.0;
        assert!((reconstructed_power - output.propeller_power_w).abs() < 1.0e-8);
        let expected_fuel = output.engine_shaft_power_w * model.reference_psfc_kg_kwh
            / model.power_lapse_fraction(model.fuel_reference_density_kg_m3)
            / JOULES_PER_KWH;
        assert!((output.fuel_flow_kg_s - expected_fuel).abs() < 1.0e-12);
    }

    #[test]
    fn governed_efficiency_is_physically_bounded() {
        let output = Pw127m568fModel::default()
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: 1.0,
                    true_airspeed_m_s: 90.0,
                },
                nominal_command(),
            )
            .unwrap();
        assert!((0.0..=1.0).contains(&output.propulsive_efficiency));
        let surrogate = PropellerSurrogate::generic_six_blade();
        assert!(output.blade_angle_deg >= surrogate.minimum_blade_angle_deg);
        assert!(output.blade_angle_deg <= surrogate.maximum_blade_angle_deg);
    }

    #[test]
    fn takeoff_speed_thrust_respects_actuator_disk_power() {
        let model = Pw127m568fModel::default();
        let output = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: 1.225,
                    true_airspeed_m_s: 85.0,
                },
                TurbopropCommand {
                    rating: Pw127mRating::NormalTakeoff,
                    power_fraction: 1.0,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: model.governed_propeller_speed_rpm,
                },
            )
            .unwrap();
        let disk_area_m2 = PI * model.propeller_diameter_m.powi(2) / 4.0;
        let induced_velocity_m_s = 0.5
            * ((85.0_f64.powi(2) + 2.0 * output.propeller_thrust_n / (1.225 * disk_area_m2))
                .sqrt()
                - 85.0);
        let ideal_power_w = output.propeller_thrust_n * (85.0 + induced_velocity_m_s);

        assert!(ideal_power_w <= output.propeller_power_w * (1.0 + 1.0e-12));
        assert!(output.propulsive_efficiency < 1.0);
        assert!(output.propulsive_efficiency > 0.0);
    }

    #[test]
    fn actuator_disk_bound_is_continuous_near_takeoff_speed() {
        let model = Pw127m568fModel::default();
        let evaluate_at = |true_airspeed_m_s| {
            model
                .evaluate(
                    TurbopropCondition {
                        density_kg_m3: 1.225,
                        true_airspeed_m_s,
                    },
                    TurbopropCommand {
                        rating: Pw127mRating::NormalTakeoff,
                        power_fraction: 1.0,
                        mode: TurbopropMode::Governed,
                        propeller_speed_rpm: model.governed_propeller_speed_rpm,
                    },
                )
                .unwrap()
                .propeller_thrust_n
        };
        let below_takeoff = evaluate_at(85.0 - 1.0e-4);
        let above_takeoff = evaluate_at(85.0 + 1.0e-4);

        assert!((above_takeoff - below_takeoff).abs() / below_takeoff < 1.0e-5);
    }

    #[test]
    fn fl170_climb_extrapolates_when_generic_governor_map_is_exhausted() {
        let model = Pw127m568fModel {
            power_lapse_reference_density_kg_m3: 0.70,
            ..Pw127m568fModel::default()
        };
        let density_kg_m3 = 0.70;
        let true_airspeed_m_s = 120.0;
        let revolutions_s = model.governed_propeller_speed_rpm / 60.0;
        let advance_ratio = true_airspeed_m_s / (revolutions_s * model.propeller_diameter_m);
        let engine_power_w = model.maximum_climb_power_w;
        let propeller_power_w =
            (engine_power_w - model.accessory_power_w) * model.gearbox_efficiency;
        assert!(matches!(
            model.solve_governor(
                density_kg_m3,
                revolutions_s,
                advance_ratio,
                propeller_power_w,
                false,
            ),
            Err(TurbopropError::GovernorNoSolution { .. })
        ));

        let output = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3,
                    true_airspeed_m_s,
                },
                TurbopropCommand {
                    rating: Pw127mRating::MaximumClimb,
                    power_fraction: 1.0,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: model.governed_propeller_speed_rpm,
                },
            )
            .unwrap();
        let disk_area_m2 = PI * model.propeller_diameter_m.powi(2) / 4.0;
        let expected_fallback_thrust_n = actuator_disk_thrust_bound_n(
            model.static_figure_of_merit * output.propeller_power_w,
            density_kg_m3,
            disk_area_m2,
            true_airspeed_m_s,
        );

        assert!((output.propeller_thrust_n - expected_fallback_thrust_n).abs() < 1.0e-8);
        assert_eq!(
            output.blade_angle_deg,
            model.surrogate.maximum_blade_angle_deg
        );
        assert!((0.0..1.0).contains(&output.propulsive_efficiency));
        assert_eq!(
            output.propeller_model_uncertainty,
            ModelUncertainty::UnquantifiedSurrogate
        );
    }

    #[test]
    fn governed_high_advance_ratio_rejects_negative_map_thrust_and_extrapolates() {
        let model = Pw127m568fModel::default();
        let density_kg_m3 = 0.70;
        let true_airspeed_m_s = 180.0;
        let power_fraction = 0.20;
        let revolutions_s = model.governed_propeller_speed_rpm / 60.0;
        let advance_ratio = true_airspeed_m_s / (revolutions_s * model.propeller_diameter_m);
        let engine_power_w = model.maximum_continuous_power_w * power_fraction;
        let propeller_power_w =
            (engine_power_w - model.accessory_power_w) * model.gearbox_efficiency;
        assert_eq!(
            model
                .solve_governor(
                    density_kg_m3,
                    revolutions_s,
                    advance_ratio,
                    propeller_power_w,
                    false,
                )
                .unwrap_err(),
            TurbopropError::NonPhysicalResult("negative forward thrust")
        );

        let output = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3,
                    true_airspeed_m_s,
                },
                TurbopropCommand {
                    rating: Pw127mRating::MaximumContinuous,
                    power_fraction,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: model.governed_propeller_speed_rpm,
                },
            )
            .unwrap();
        let disk_area_m2 = PI * model.propeller_diameter_m.powi(2) / 4.0;
        let expected_fallback_thrust_n = actuator_disk_thrust_bound_n(
            model.static_figure_of_merit * output.propeller_power_w,
            density_kg_m3,
            disk_area_m2,
            true_airspeed_m_s,
        );

        assert!((output.propeller_thrust_n - expected_fallback_thrust_n).abs() < 1.0e-8);
        assert!(output.propeller_thrust_n > 0.0);
        assert!((0.0..1.0).contains(&output.propulsive_efficiency));
    }

    #[test]
    fn invalid_domain_is_rejected_without_clipping() {
        let mut command = nominal_command();
        command.power_fraction = 1.01;
        assert!(matches!(
            Pw127m568fModel::default().evaluate(
                TurbopropCondition {
                    density_kg_m3: 1.225,
                    true_airspeed_m_s: 50.0,
                },
                command
            ),
            Err(TurbopropError::OutsideDomain {
                field: "power_fraction",
                ..
            })
        ));
    }

    #[test]
    fn unavailable_568f_modes_are_explicit_errors() {
        let mut command = nominal_command();
        command.mode = TurbopropMode::Reverse;
        assert_eq!(
            Pw127m568fModel::default()
                .evaluate(
                    TurbopropCondition {
                        density_kg_m3: 1.225,
                        true_airspeed_m_s: 0.0,
                    },
                    command
                )
                .unwrap_err(),
            TurbopropError::UnsupportedMode(TurbopropMode::Reverse)
        );
    }

    #[test]
    fn atr_adapter_aggregates_two_units_once_and_exposes_resources() {
        let system = atr_system();
        let result = system
            .evaluate(&system_request(FailureState::None))
            .unwrap();
        let unit = Pw127m568fModel::default()
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: 1.225,
                    true_airspeed_m_s: 0.0,
                },
                TurbopropCommand {
                    rating: Pw127mRating::NormalTakeoff,
                    power_fraction: 1.0,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: 1_200.0,
                },
            )
            .unwrap();
        assert!((result.body_force_n[0] - 2.0 * unit.total_thrust_n).abs() < 1.0e-9);
        assert_eq!(result.body_moment_nm, [0.0; 3]);
        assert_eq!(result.shaft_power_w, Some(2.0 * unit.propeller_power_w));
        assert_eq!(result.torque_nm, Some(2.0 * unit.propeller_torque_n_m));
        assert_eq!(result.rotational_speed_rpm, Some(1_200.0));
        assert_eq!(
            result.resource_flows[0].mass_flow_kg_s,
            Some(2.0 * unit.fuel_flow_kg_s)
        );
        assert!(matches!(
            result.validity,
            ValidityStatus::Extrapolated { .. }
        ));
    }

    #[test]
    fn atr_oei_uses_one_reserve_rated_engine_without_double_counting() {
        let system = atr_system();
        let result = system
            .evaluate(&system_request(FailureState::UnitsUnavailable(vec![0])))
            .unwrap();
        let unit = Pw127m568fModel::default()
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: 1.225,
                    true_airspeed_m_s: 0.0,
                },
                TurbopropCommand {
                    rating: Pw127mRating::MaximumTakeoffReserve,
                    power_fraction: 1.0,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: 1_200.0,
                },
            )
            .unwrap();
        assert!((result.body_force_n[0] - unit.total_thrust_n).abs() < 1.0e-9);
        assert_eq!(result.shaft_power_w, Some(unit.propeller_power_w));
        assert!(result.active_limits[0]
            .name
            .contains("MaximumTakeoffReserve"));
    }

    #[test]
    fn normalized_force_is_inverted_in_force_space_not_power_space() {
        let system = atr_system();
        let maximum = system
            .evaluate(&system_request(FailureState::None))
            .unwrap();
        let mut half_request = system_request(FailureState::None);
        half_request.demand = PropulsionDemand::NormalizedForce(0.5);
        let half = system.evaluate(&half_request).unwrap();
        assert!((half.body_force_n[0] / maximum.body_force_n[0] - 0.5).abs() < 1.0e-10);
        assert!(half.shaft_power_w.unwrap() / maximum.shaft_power_w.unwrap() < 0.5);
    }

    #[test]
    fn rated_fraction_preserves_selected_rating_and_power_fraction() {
        let system = atr_system();
        let mut half_request = system_request(FailureState::None);
        half_request.demand = PropulsionDemand::RatedFraction {
            rating: PropulsionRating::Cruise,
            fraction: 0.5,
        };
        let half = system.evaluate(&half_request).unwrap();
        let model = Pw127m568fModel::default();
        let idle_power_w = model.rated_shaft_power_w(Pw127mRating::FlightIdleSurrogate);
        let interpolated_engine_power_w =
            idle_power_w + 0.5 * (model.maximum_cruise_power_w - idle_power_w);
        let expected_unit_propeller_power_w =
            (interpolated_engine_power_w - model.accessory_power_w) * model.gearbox_efficiency;

        assert_eq!(
            half.shaft_power_w,
            Some(2.0 * expected_unit_propeller_power_w)
        );
        assert_eq!(
            half.achieved_demand,
            PropulsionDemand::RatedFraction {
                rating: PropulsionRating::Cruise,
                fraction: 0.5,
            }
        );
        assert_eq!(half.active_limits[0].utilization, 0.5);
    }

    #[test]
    fn non_idle_rated_fraction_endpoints_span_idle_to_named_rating_power() {
        let system = atr_system();
        let model = Pw127m568fModel::default();
        let evaluate_at = |fraction| {
            let mut request = system_request(FailureState::None);
            request.demand = PropulsionDemand::RatedFraction {
                rating: PropulsionRating::MaximumContinuous,
                fraction,
            };
            system.evaluate(&request).unwrap()
        };
        let at_idle_floor = evaluate_at(0.0);
        let at_named_rating = evaluate_at(1.0);
        let idle_engine_power_w = model.rated_shaft_power_w(Pw127mRating::FlightIdleSurrogate);
        let expected_idle_propeller_power_w =
            (idle_engine_power_w - model.accessory_power_w) * model.gearbox_efficiency;
        let expected_full_propeller_power_w =
            (model.maximum_continuous_power_w - model.accessory_power_w) * model.gearbox_efficiency;

        assert_eq!(
            at_idle_floor.shaft_power_w,
            Some(2.0 * expected_idle_propeller_power_w)
        );
        assert_eq!(
            at_named_rating.shaft_power_w,
            Some(2.0 * expected_full_propeller_power_w)
        );
        assert_eq!(at_idle_floor.active_limits[0].utilization, 0.0);
        assert_eq!(at_named_rating.active_limits[0].utilization, 1.0);
    }

    #[test]
    fn flight_idle_rated_fraction_evaluates_directly_in_idle_mode() {
        let system = atr_system();
        let mut request = system_request(FailureState::None);
        request.demand = PropulsionDemand::RatedFraction {
            rating: PropulsionRating::FlightIdle,
            fraction: 1.0,
        };
        let result = system.evaluate(&request).unwrap();
        let model = Pw127m568fModel::default();
        let unit = model
            .evaluate(
                TurbopropCondition {
                    density_kg_m3: request.flight.density_kg_m3,
                    true_airspeed_m_s: request.flight.velocity_m_s,
                },
                TurbopropCommand {
                    rating: Pw127mRating::FlightIdleSurrogate,
                    power_fraction: 1.0,
                    mode: TurbopropMode::FlightIdle,
                    propeller_speed_rpm: model.governed_propeller_speed_rpm,
                },
            )
            .unwrap();

        assert_eq!(result.body_force_n[0], 2.0 * unit.total_thrust_n);
        assert_eq!(result.shaft_power_w, Some(2.0 * unit.propeller_power_w));
        assert_eq!(result.active_limits[0].utilization, 1.0);
        assert!(result.active_limits[0].name.contains("FlightIdleSurrogate"));
    }

    #[test]
    fn flight_idle_uses_neutral_force_without_a_windmilling_map() {
        let model = Pw127m568fModel::default();
        let density_kg_m3 = 0.95;
        let revolutions_s = model.governed_propeller_speed_rpm / 60.0;
        let engine_power_w = model.rated_shaft_power_w(Pw127mRating::FlightIdleSurrogate)
            * model.power_lapse_fraction(density_kg_m3);
        let propeller_power_w =
            (engine_power_w - model.accessory_power_w) * model.gearbox_efficiency;
        let advance_ratio_at_100 = 100.0 / (revolutions_s * model.propeller_diameter_m);
        let polynomial_solution = model
            .solve_governor(
                density_kg_m3,
                revolutions_s,
                advance_ratio_at_100,
                propeller_power_w,
                true,
            )
            .unwrap();
        assert!(polynomial_solution.1 < 0.0);

        for true_airspeed_m_s in [100.0, 115.0, 130.0] {
            let output = model
                .evaluate(
                    TurbopropCondition {
                        density_kg_m3,
                        true_airspeed_m_s,
                    },
                    TurbopropCommand {
                        rating: Pw127mRating::FlightIdleSurrogate,
                        power_fraction: 1.0,
                        mode: TurbopropMode::FlightIdle,
                        propeller_speed_rpm: model.governed_propeller_speed_rpm,
                    },
                )
                .unwrap();

            assert_eq!(output.propeller_thrust_n, 0.0);
            assert_eq!(output.total_thrust_n, 0.0);
            assert_eq!(output.propulsive_efficiency, 0.0);
        }
    }

    #[test]
    fn atr_adapter_rejects_unavailable_maps_and_nonmechanical_loads() {
        let system = atr_system();
        let mut request = system_request(FailureState::None);
        request.mode = OperatingMode::Reverse;
        assert!(matches!(
            system.evaluate(&request),
            Err(PropulsionError::UnsupportedMode(OperatingMode::Reverse))
        ));
        request.mode = OperatingMode::Normal;
        request.demand = PropulsionDemand::Rating(PropulsionRating::Cruise);
        let cruise = system.evaluate(&request).unwrap();
        assert!(matches!(
            cruise.validity,
            ValidityStatus::Extrapolated { .. }
        ));
        request.demand = PropulsionDemand::NormalizedForce(0.5);
        request.loads.electrical_power_w = 1_000.0;
        assert!(matches!(
            system.evaluate(&request),
            Err(PropulsionError::UnsupportedDemand(_))
        ));
    }

    #[test]
    fn shutdown_requires_zero_demand_and_has_no_active_power_rating() {
        let system = atr_system();
        let mut request = system_request(FailureState::None);
        request.mode = OperatingMode::Shutdown;
        assert!(matches!(
            system.evaluate(&request),
            Err(PropulsionError::UnsupportedDemand(_))
        ));
        request.demand = PropulsionDemand::NormalizedForce(0.0);
        let result = system.evaluate(&request).unwrap();
        assert_eq!(result.body_force_n, [0.0; 3]);
        assert_eq!(result.rotational_speed_rpm, Some(0.0));
        assert_eq!(
            result.achieved_demand,
            PropulsionDemand::NormalizedForce(0.0)
        );
        assert_eq!(
            result.active_limits[0].name,
            "propulsion shutdown commanded"
        );
        assert!(!result.active_limits[0].name.contains("PW127M"));
    }

    #[test]
    fn unavailable_unit_semantics_are_normalized_and_trace_zero_capability() {
        let system = atr_system();
        let aeo = system
            .evaluate(&system_request(FailureState::None))
            .unwrap();
        let empty_selection = system
            .evaluate(&system_request(FailureState::UnitsUnavailable(Vec::new())))
            .unwrap();
        assert_eq!(empty_selection.body_force_n, aeo.body_force_n);
        assert_eq!(empty_selection.shaft_power_w, aeo.shaft_power_w);

        let unavailable = system
            .evaluate(&system_request(FailureState::UnitsUnavailable(vec![0, 1])))
            .unwrap();
        assert_eq!(unavailable.body_force_n, [0.0; 3]);
        assert_eq!(unavailable.shaft_power_w, Some(0.0));
        assert_eq!(
            unavailable.achieved_demand,
            PropulsionDemand::NormalizedForce(0.0)
        );
        assert!(unavailable.active_limits[0]
            .name
            .contains("zero capability"));
    }
}
