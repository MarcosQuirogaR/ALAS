// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

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
