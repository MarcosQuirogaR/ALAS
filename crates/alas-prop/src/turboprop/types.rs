// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

pub(super) const WATTS_PER_SHP: f64 = 745.699_872;
pub(super) const JOULES_PER_KWH: f64 = 3.6e6;

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
    /// The published maximum-cruise fuel flow per unit of sea-level
    /// maximum-cruise rating, kg/(kW h): a **validation datum**, not a model
    /// input and not a PSFC.
    ///
    /// ATR publishes 762 kg/h for both engines at maximum cruise power, and
    /// the typed `MaximumCruise` rating is a sea-level 2,132 shp, so this is
    /// `762 / (2 x 1,589.83 kW) = 0.239648 kg/kWh`; a caller mapping a
    /// catalogue entry writes the published flow here the same way. The
    /// model no longer burns fuel from it: the fuel flow is
    /// [`Self::psfc_reference_kg_kwh`] scaled by ambient temperature.
    /// [`Pw127m568fModel::fuel_validation`] multiplies it back by
    /// `2 x maximum_cruise_power_w` to recover the published flow and reports
    /// the model against it.
    pub reference_psfc_kg_kwh: f64,
    /// Assumed ambient density of the published 762 kg/h point, kg/m^3, used
    /// only to evaluate [`Pw127m568fModel::fuel_validation`]. **An
    /// engineering estimate, not a source datum.**
    ///
    /// The point's stated condition is *"95 % MTOW, ISA, optimum FL, 275
    /// KTAS"* (ATR 72-600 factsheet; ATR Family brochure p. 19). ATR does not
    /// publish which flight level "optimum" is, so 0.70 kg/m^3 (ISA at about
    /// FL180) stands in for it; the validation also reports the comparison
    /// across FL160-FL250.
    pub fuel_reference_density_kg_m3: f64,
    /// Power-specific fuel consumption on free-turbine shaft power at
    /// [`Self::psfc_reference_temperature_k`], kg/(kW h).
    ///
    /// 0.2945 kg/kWh is the mean of the two engines' PSFC in the Dash 8-100
    /// DFDR flight record of Majeed, O., *Parametric Specific Fuel
    /// Consumption Analysis of the PW120A Turboprop Engine*, Specific Range
    /// Solutions SRS-TSD-002 Rev. 1, 2009, Tab. 3.3 (0.287 and 0.302 kg/kWh
    /// at 15,616 ft, SAT -23.5 C, about 62 % of take-off power). It is chosen
    /// because it is *measured* PW100-family PSFC on the same shaft-power
    /// basis this model burns on (power from torque and propeller speed, no
    /// exhaust-thrust credit, and this installation declares zero residual
    /// jet thrust), at a power inside the flat band, and it sits in the
    /// middle of that document's 0.28-0.31 kg/kWh range. It is **not** chosen
    /// to reproduce the ATR's 762 kg/h, which is only a validation point.
    /// The PW127M is a later variant and is probably a few percent better;
    /// no primary PW127M figure was retrieved, so that is a known
    /// conservative bias rather than a correction.
    pub psfc_reference_kg_kwh: f64,
    /// Ambient static temperature of [`Self::psfc_reference_kg_kwh`], K:
    /// the -23.5 C SAT of Majeed (2009) Tab. 3.3.
    pub psfc_reference_temperature_k: f64,
    /// Ambient temperature to which the take-off ratings (normal and maximum
    /// reserve) are flat-rated at sea level, K: 39 C, EASA TCDS IM.E.041
    /// (PW100 series), section 5 "Ratings", PW127M row.
    pub takeoff_flat_rating_temperature_k: f64,
    /// Ambient temperature to which the maximum-continuous rating is
    /// flat-rated at sea level, K: 48 C, EASA TCDS IM.E.041, section 5, PW127M
    /// row. The climb, cruise and flight-idle ratings share this corner; see
    /// [`Pw127m568fModel::flat_rating_corner_density_kg_m3`].
    pub maximum_continuous_flat_rating_temperature_k: f64,
    /// Static figure of merit: the share of shaft power that reaches the
    /// ideal actuator-disk induced power at zero airspeed.
    ///
    /// At `V = 0` this is what separates the thrust from its ideal bound,
    /// `T = FM^(2/3) (2 rho A)^(1/3) P^(2/3)` (momentum theory, Lutze,
    /// *Performance 10: Thrust Models*, Virginia Tech AOE 3104, eq. 5-14),
    /// and the same number is the `J = 0` end of
    /// [`Self::blade_efficiency_cruise`]'s blend. The declared 0.70 is the
    /// class-level preliminary-design value (0.68-0.73, "0.7 for preliminary
    /// estimates"; 0.65-0.80 is the defensible band for real propellers).
    /// Confidence is medium-low: the figure comes from a secondary
    /// transcription of Gudmundsson-type text and no 568F-1 static datum is
    /// public.
    pub static_figure_of_merit: f64,
    /// Share of shaft power that reaches ideal induced power in forward
    /// flight, i.e. `eta_p / eta_ideal`, once the blade is unstalled.
    ///
    /// Momentum theory bounds a propeller's thrust at `P = T (V + v_i)`, but
    /// that bound is a propeller with **no profile loss at all**: at the ATR's
    /// FL170 / 275 kt cruise it gives `eta_ideal = V/(V + v_i) = 0.976`, which
    /// is not a physical propeller.
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
    /// ISA sea-level density, kg/m^3: the reference the flat-rating corners
    /// are placed against (the corner density is this times
    /// `288.15 K / T_flat`, i.e. sea-level pressure at the flat-rating
    /// temperature).
    pub power_lapse_reference_density_kg_m3: f64,
    /// Exponent `n` of the density lapse `P / P_corner = (rho / rho_corner)^n`
    /// above the flat-rating corner.
    ///
    /// 0.728 is Nita (2008, HAW Hamburg, ATR 72 design study) eq. 3.5.11, a
    /// regression on McCormick's (1995) PW120 maximum-cruise power chart,
    /// `P_CR / P_TO = 0.9 sigma^0.728`; Ruijgrok (1996, via Nita Tab. 3.2)
    /// gives 0.75 for the class. The PW100-family fit is used because it is
    /// the same engine family.
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
            // ATR's published 762 kg/h two-engine flow at maximum cruise power
            // over 2 x 2,132 shp: the validation point, not a model input.
            reference_psfc_kg_kwh: 0.239_647_943_644_226,
            fuel_reference_density_kg_m3: 0.70,
            // Majeed (2009) Tab. 3.3: mean of 0.287 and 0.302 kg/kWh at SAT
            // -23.5 C.
            psfc_reference_kg_kwh: 0.2945,
            psfc_reference_temperature_k: 249.65,
            // EASA TCDS IM.E.041: take-off to 39 C, MCT to 48 C.
            takeoff_flat_rating_temperature_k: 312.15,
            maximum_continuous_flat_rating_temperature_k: 321.15,
            static_figure_of_merit: 0.70,
            // The conservative end of the 0.86-0.91 band the three routes in
            // the field documentation agree on.
            blade_efficiency_cruise: 0.86,
            // 115 KCAS take-off at 1,200 rev/min on 3.93 m: J = 59.2/(20 x 3.93).
            blade_efficiency_knee_advance_ratio: 0.753,
            maximum_propulsive_efficiency: 0.88,
            // Flat to the TCDS corner, then Nita (2008) eq. 3.5.11's PW120
            // exponent.
            power_lapse_reference_density_kg_m3: 1.225,
            power_lapse_density_exponent: 0.728,
            minimum_power_lapse_fraction: 0.15,
            surrogate: PropellerSurrogate::generic_six_blade(),
        }
    }
}

impl Pw127m568fModel {
    /// Reject a non-positive rating, diameter or PSFC, or a governed speed
    /// outside 100-2,000 rev/min, naming the parameter that failed with its
    /// own value.
    pub(super) fn validate_positive_parameters(&self) -> Result<(), TurbopropError> {
        for (field, value) in [
            ("propeller_diameter_m", self.propeller_diameter_m),
            ("reference_psfc_kg_kwh", self.reference_psfc_kg_kwh),
            ("psfc_reference_kg_kwh", self.psfc_reference_kg_kwh),
            (
                "psfc_reference_temperature_k",
                self.psfc_reference_temperature_k,
            ),
            (
                "takeoff_flat_rating_temperature_k",
                self.takeoff_flat_rating_temperature_k,
            ),
            (
                "maximum_continuous_flat_rating_temperature_k",
                self.maximum_continuous_flat_rating_temperature_k,
            ),
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
        ] {
            if value <= 0.0 {
                return Err(TurbopropError::OutsideDomain { field, value });
            }
        }
        if !(100.0..=2_000.0).contains(&self.governed_propeller_speed_rpm) {
            return Err(TurbopropError::OutsideDomain {
                field: "governed_propeller_speed_rpm",
                value: self.governed_propeller_speed_rpm,
            });
        }
        Ok(())
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

    pub(super) fn coefficients(self, advance_ratio: f64, blade_angle_deg: f64) -> (f64, f64) {
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
    /// Jet-A consumption, engine shaft power times [`Self::psfc_kg_kwh`], kg/s.
    pub fuel_flow_kg_s: f64,
    /// The power-specific fuel consumption this flow implies, kg/(kW h).
    ///
    /// It follows ambient temperature as `sqrt(T / T_ref)` and is flat in
    /// power setting (see [`Pw127m568fModel::psfc_kg_kwh`]), so it is the same
    /// at maximum take-off power and at flight idle at one temperature. See
    /// [`Pw127m568fModel::fuel_validation`] for how the resulting flow
    /// compares with the published cruise point.
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
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TurbopropError {
    /// Named input was NaN or infinite.
    #[error("non-finite turboprop input: {0}")]
    NonFinite(&'static str),
    /// Named input is outside the declared numerical or physical domain.
    #[error("turboprop input outside domain: {field}={value}")]
    OutsideDomain {
        /// Stable name of the rejected input.
        field: &'static str,
        /// Rejected value in the field's documented SI unit.
        value: f64,
    },
    /// Requested propeller state lacks a defensible public model.
    #[error("unsupported turboprop mode: {0:?}")]
    UnsupportedMode(TurbopropMode),
    /// No blade angle within the surrogate pitch bounds absorbs the requested power.
    #[error("propeller governor cannot absorb requested power {required_power_w} W")]
    GovernorNoSolution {
        /// Propeller power that could not be matched within pitch limits, W.
        required_power_w: f64,
    },
    /// A calculation completed but violated a physical invariant.
    #[error("nonphysical turboprop result: {0}")]
    NonPhysicalResult(&'static str),
}
