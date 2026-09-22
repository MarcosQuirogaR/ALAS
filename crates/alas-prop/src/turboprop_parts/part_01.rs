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
    /// Fuel flow per unit of **sea-level-rated** maximum-cruise shaft power,
    /// kg/(kW h), **not** the power-specific fuel consumption the engine
    /// actually runs at.
    ///
    /// The name is historical and the quantity is a rating-basis bookkeeping
    /// coefficient. ATR publishes 762 kg/h for both engines at maximum cruise
    /// power, and the typed `MaximumCruise` rating is a *sea-level* 2,132 shp;
    /// dividing the one by the other gives this number directly:
    /// `762 / (2 x 1,589.83 kW) = 0.239648 kg/kWh`. The PSFC the model then
    /// applies at every operating point is this value divided by the shaft
    /// power lapse at [`Self::fuel_reference_density_kg_m3`], so that the
    /// anchor is reproduced after the lapse is applied: **0.364630 kg/kWh**
    /// at the declared inputs.
    ///
    /// **The trap this doc comment used to set.** Writing a measured PW120A
    /// PSFC of 0.295 kg/kWh into this field does *not* give the model a
    /// 0.295 kg/kWh engine: it gives it `0.295 x 0.657 = 0.194 kg/kWh`, 34 %
    /// below the intention. Use [`Pw127m568fModel::implied_psfc_kg_kwh`] to
    /// read what the engine is actually burning, and
    /// [`Pw127m568fModel::fuel_calibration`] for the whole typed statement.
    pub reference_psfc_kg_kwh: f64,
    /// Assumed ambient density of the published maximum-cruise fuel-flow
    /// anchor, kg/m^3. **An engineering estimate, not a source datum.**
    ///
    /// The anchor's stated condition is *"95 % MTOW, ISA, optimum FL, 275
    /// KTAS"* (ATR 72-600 factsheet; ATR Family brochure p. 19). **ATR does
    /// not publish which flight level "optimum" is**, and no retrieved
    /// document states it, so this value is an assumption standing in for a
    /// missing one. The declared 0.70 kg/m^3 is ISA at about FL180.
    ///
    /// It matters more than its size suggests. The 762 kg/h anchor is
    /// reproduced at *any* value of this field, because the calibration
    /// divides by the lapse at this same density, but the physical PSFC it
    /// implies, and therefore **every fuel flow away from the anchor**, moves
    /// with it: 0.3476 kg/kWh if the anchor is at FL160, 0.3564 at FL170,
    /// 0.3654 at FL180, 0.3843 at FL200, 0.4376 at FL250. That 26 % spread is
    /// driven entirely by an undeclared input.
    /// [`Pw127m568fModel::fuel_calibration`] reports it rather than hiding it.
    pub fuel_reference_density_kg_m3: f64,
    /// Static figure of merit: the share of shaft power that reaches the
    /// ideal actuator-disk induced power at zero airspeed.
    ///
    /// At `V = 0` this is what separates the thrust from its ideal bound,
    /// `T = FM^(2/3) T_ideal`, and the same number is the `J = 0` end of
    /// [`Self::blade_efficiency_cruise`]'s blend.
    pub static_figure_of_merit: f64,
    /// Share of shaft power that reaches ideal induced power in forward
    /// flight, i.e. `eta_p / eta_ideal`, once the blade is unstalled.
    ///
    /// Momentum theory bounds a propeller's thrust at `P = T (V + v_i)`, but
    /// that bound is a propeller with **no profile loss at all**: at the ATR's
    /// FL170 / 275 kt cruise it gives `eta_ideal = V/(V + v_i) = 0.976`, which
    /// is what this model used to report and is not a physical propeller.
    /// Real blades lose profile drag, tip and non-uniform-inflow power on top
    /// of it.
    ///
    /// Three independent routes put that loss at 0.86-0.91 for this
    /// installation, and the declared value is the conservative end:
    ///
    /// * the Hamilton Standard four-blade `100 AF, 0.55 CL_i` map of NASA
    ///   TM-83458 p. 8, interpolated at the cruise `J` and `C_P`, gives an
    ///   isolated 0.897 and 0.85-0.86 installed;
    /// * dividing the propeller efficiencies Nita (2008) Table 3.4 p. 41 reads
    ///   off the Scholz chart *for the ATR 72* by the ideal efficiency at the
    ///   same disc loading gives 0.878 (second climb segment), 0.880 (cruise)
    ///   and 0.913 (take-off);
    /// * closing the aircraft-level published 762 kg/h cruise fuel flow and
    ///   1,355 ft/min climb rate gives 0.76-0.86 and 0.835.
    ///
    /// **This is a bounded surrogate, not a 568F-1 map.** The uncertainty is
    /// the 0.86-0.91 spread above, about -0/+6 % on every forward-flight
    /// thrust, and it does not cover a blade operating outside the unstalled
    /// range the sources describe.
    pub blade_efficiency_cruise: f64,
    /// Advance ratio at which the blade efficiency has fully reached
    /// [`Self::blade_efficiency_cruise`].
    ///
    /// The static figure of merit and the forward-flight blade efficiency are
    /// two different flow states: at `V = 0` a governed blade works at high
    /// incidence with separated flow, and by the take-off climb-out it is
    /// unstalled and near its design incidence. The blend between them is a
    /// smoothstep in `J`, and the knee is the ATR's own take-off advance
    /// ratio, 115 kt at 1,200 rev/min on a 3.93 m propeller, which is the
    /// lowest-speed point any retrieved source characterises.
    pub blade_efficiency_knee_advance_ratio: f64,
    /// Hard ceiling on propulsive efficiency, whatever the surrogate returns.
    ///
    /// A single-rotation propeller of this class does not exceed this in any
    /// retrieved source; it is a guard against an unphysical operating point
    /// reaching a mission or a report, not a working part of the model.
    pub maximum_propulsive_efficiency: f64,
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
            // The conservative end of the 0.86-0.91 band the three routes in
            // the field documentation agree on.
            blade_efficiency_cruise: 0.86,
            // 115 KCAS take-off at 1,200 rev/min on 3.93 m: J = 59.2/(20 x 3.93).
            blade_efficiency_knee_advance_ratio: 0.753,
            maximum_propulsive_efficiency: 0.88,
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
    /// The power-specific fuel consumption this flow implies, kg/(kW h).
    ///
    /// Reported because it is the quantity a reader can compare against a
    /// measured engine, and because it is **constant**: the model carries no
    /// variation of PSFC with power setting, altitude or temperature, so this
    /// field returns the same number at maximum take-off power and at flight
    /// idle. See [`Pw127m568fModel::fuel_calibration`] for what that constant
    /// rests on and how far it sits from measurement.
    pub psfc_kg_kwh: f64,
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
