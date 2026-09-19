// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// The typed contract a consumer outside `alas-prop` needs in order to fly a
// turboprop.
// 
// Everything here is assembled from [`Pw127m568fModel::evaluate`] and adds no
// new physics: it exists because the quantities a field-performance, mission
// or results consumer needs — a static thrust, a representative ground-roll
// thrust, the fuel flow that goes with them, and the band and envelope they
// are only valid inside — were reachable only by knowing which operating
// points to ask for and which caveats to carry. A consumer that reads a bare
// number out of [`TurbopropOutput`] cannot tell a certificated rating from a
// single-point calibration; this contract makes that impossible to lose.
// 
// **No jet thrust is manufactured anywhere in this file.**
// `residual_jet_thrust_n` stays whatever the installation declares, which is
// `0.0` for the ATR 72-600, and a shaft power is never converted into a
// rated-thrust surrogate for FLOPS equations 121-122 or for a
// thrust-to-weight gate. What is published instead is the *propeller* thrust
// the momentum-bounded model actually produces at a stated airspeed and
// density, with its uncertainty attached.

/// Asymmetric relative band on a modelled propeller thrust.
///
/// Both bounds are fractions of the modelled value: `-0.20` means the true
/// thrust may be twenty percent below what the model returns. The band is
/// **not** applied to the number; it travels beside it so a consumer can gate,
/// display or widen a result rather than read a surrogate as a measurement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurbopropThrustUncertainty {
    /// Lower bound as a signed fraction of the modelled thrust, `<= 0`.
    pub relative_low: f64,
    /// Upper bound as a signed fraction of the modelled thrust, `>= 0`.
    pub relative_high: f64,
    /// What the two bounds rest on.
    pub basis: &'static str,
}

impl TurbopropThrustUncertainty {
    /// The band on a **static** thrust, which is set by the figure of merit.
    ///
    /// `T = FM^(2/3) T_ideal`, so the declared 0.72 against the defensible
    /// 0.50-0.70 band of the evidence file gives
    /// `(0.50/0.72)^(2/3) - 1 = -21.6 %` at the pessimistic end and nothing
    /// above, because 0.72 is already the optimistic end of the band. A
    /// take-off energy balance against the ATR 72-600's published 1,333 m
    /// field length independently wants a lower mean roll thrust than this
    /// model produces, which is consistent with the sign of the band.
    #[must_use]
    pub fn static_thrust(figure_of_merit: f64) -> Self {
        /// Pessimistic end of the defensible static figure-of-merit band.
        const LOWER_FIGURE_OF_MERIT: f64 = 0.50;
        let relative_low = if figure_of_merit.is_finite() && figure_of_merit > LOWER_FIGURE_OF_MERIT
        {
            (LOWER_FIGURE_OF_MERIT / figure_of_merit).powf(2.0 / 3.0) - 1.0
        } else {
            0.0
        };
        Self {
            relative_low,
            relative_high: 0.0,
            basis: "static figure of merit declared at 0.72, the optimistic end of a defensible 0.50-0.70 band; T scales as FM^(2/3). Closing it needs a measured 568F-1 map, which is not public.",
        }
    }

    /// The band on a **forward-flight** thrust, set by the blade efficiency.
    ///
    /// The declared 0.86 is the conservative end of a 0.86-0.91 band that
    /// three independent routes agree on (a Hamilton Standard four-blade
    /// `100 AF, 0.55 CL_i` map interpolated at cruise `J` and `C_P`, NASA
    /// TM-83458 p. 8; the Scholz chart as read for the ATR 72 by Nita 2008
    /// Table 3.4 p. 41; and aircraft-level closure on the published 762 kg/h
    /// and 1,355 ft/min). Thrust is proportional to the blade share, so the
    /// band is `-0 / +5.8 %`.
    #[must_use]
    pub fn forward_flight_thrust(blade_efficiency_cruise: f64) -> Self {
        /// Optimistic end of the band the three independent routes agree on.
        const UPPER_BLADE_EFFICIENCY: f64 = 0.91;
        let relative_high = if blade_efficiency_cruise.is_finite()
            && blade_efficiency_cruise > 0.0
            && blade_efficiency_cruise < UPPER_BLADE_EFFICIENCY
        {
            UPPER_BLADE_EFFICIENCY / blade_efficiency_cruise - 1.0
        } else {
            0.0
        };
        Self {
            relative_low: 0.0,
            relative_high,
            basis: "forward-flight blade efficiency declared at the conservative end of a 0.86-0.91 band bracketed by a Hamilton Standard map (NASA TM-83458 p. 8), Nita 2008 Tab. 3.4 p. 41 and aircraft-level closure; thrust is linear in the blade share.",
        }
    }
}

/// The operating envelope a turboprop result is only honest inside.
///
/// Every limit is a declared installation datum with its source named in
/// [`Self::source`], or an explicit statement that no model exists. Nothing
/// here is derived from the surrogate, and nothing here is a soft guideline
/// the evaluator silently clips to: [`Pw127m568fModel::evaluate`] returns
/// [`TurbopropError::UnsupportedMode`] for every mode listed as unsupported,
/// rather than producing a plausible number.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurbopropOperatingEnvelope {
    /// Maximum certificated operating altitude, m.
    pub maximum_operating_altitude_m: f64,
    /// Maximum operating Mach number, `MMO`.
    pub maximum_operating_mach: f64,
    /// Published maximum cruise true airspeed, m/s.
    pub maximum_cruise_true_airspeed_m_s: f64,
    /// Density at which the shaft-power lapse reaches its declared floor and
    /// below which the lapse model states nothing, kg/m^3.
    pub power_lapse_floor_density_kg_m3: f64,
    /// Discrete states the model evaluates on declared or certificated data.
    pub supported_modes: &'static [TurbopropMode],
    /// Discrete states the model will evaluate but only on an explicitly
    /// unvalidated surrogate, and what is missing.
    ///
    /// These are the dangerous ones for a consumer: a number comes back and
    /// it looks like every other number. A gate that must not run on a
    /// surrogate has to check this list.
    pub surrogate_modes: &'static [(TurbopropMode, &'static str)],
    /// Discrete states the model refuses with
    /// [`TurbopropError::UnsupportedMode`], and why. Enforced, not advisory:
    /// a test pins that every entry here is actually refused.
    pub unsupported_modes: &'static [(TurbopropMode, &'static str)],
    /// What the fuel flow is calibrated on, and where it is extrapolated.
    pub fuel_validity: &'static str,
    /// Documents behind the limits above.
    pub source: &'static str,
}

impl TurbopropOperatingEnvelope {
    /// The declared ATR 72-600 / PW127M / 568F-1 installation envelope.
    ///
    /// The altitude and speed limits are the airframe's, not the engine's;
    /// they belong here because [`Pw127m568fModel`] is that one installation
    /// and a consumer asking this model for a take-off or cruise point needs
    /// to know where the answer stops meaning anything.
    #[must_use]
    pub const fn atr72_600(power_lapse_floor_density_kg_m3: f64) -> Self {
        Self {
            // 25,000 ft.
            maximum_operating_altitude_m: 7_620.0,
            maximum_operating_mach: 0.55,
            // 275 kt true airspeed. Before the blade-efficiency correction the
            // model carried a 14 % thrust surplus at this point and could
            // exceed it; it no longer can, which is the one aircraft-level
            // check available on the correction.
            maximum_cruise_true_airspeed_m_s: 141.472_2,
            power_lapse_floor_density_kg_m3,
            supported_modes: &[TurbopropMode::Governed, TurbopropMode::Shutdown],
            surrogate_modes: &[(
                TurbopropMode::FlightIdle,
                "evaluable, but on an unvalidated 8 %-of-maximum-continuous rating and a neutral flight-idle hypothesis; no measured PW127M idle schedule is public",
            )],
            unsupported_modes: &[
                (
                    TurbopropMode::Feathered,
                    "no public windmilling or feather drag map for the 568F-1",
                ),
                (
                    TurbopropMode::Reverse,
                    "no public beta/reverse map; reverse is by blade pitch and cannot be inferred from the forward surrogate",
                ),
            ],
            fuel_validity: "single-point calibration: the PSFC constant reproduces ATR's published 762 kg/h two-engine maximum-cruise flow and does not vary with power setting or altitude. Against a measured PW120A (0.287-0.302 kg/kWh) and NASA GASP (0.284) the implied 0.3646 kg/kWh is 21-28 % high, and it is cancelling an optimistic drag polar; neither was changed alone because the 762 kg/h anchor is the only aircraft-level datum that constrains the pair. Climb, descent, hold, taxi and sea-level take-off fuel are extrapolated from that one point.",
            source: "ATR 72-600 factsheet (ratings, 3.93 m 568F-1, 1,200 rev/min, 762 kg/h, 275 kt, 25,000 ft); EASA TCDS EASA.A.084; EASA TCDS IM.E.041 for the PW127M certification basis.",
        }
    }
}

/// What the turboprop fuel flow is, in one typed statement.
///
/// The fuel model is a **single-point calibration**, and every field here
/// exists so a consumer cannot read it as anything else. Nothing in this
/// struct is a new model: every number is either a declared input, the
/// published anchor, or arithmetic on the two.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurbopropFuelCalibration {
    /// The one aircraft-level datum the model is calibrated on, kg/s, summed
    /// over both engines. ATR's published maximum-cruise flow, 762 kg/h.
    pub anchor_total_fuel_flow_kg_s: f64,
    /// Installed engine count the anchor is quoted for.
    pub anchor_engine_count: u32,
    /// The anchor's condition, verbatim from the source.
    pub anchor_condition: &'static str,
    /// Assumed ambient density at the anchor, kg/m^3, and the reason it is an
    /// assumption.
    pub assumed_anchor_density_kg_m3: f64,
    /// Whether the anchor's altitude is published at all. It is not.
    pub anchor_altitude_is_published: bool,
    /// The constant power-specific fuel consumption the calibration implies,
    /// kg/(kW h). Identical at every altitude, airspeed and power setting.
    pub implied_psfc_kg_kwh: f64,
    /// Whether the implied PSFC varies with anything. It does not.
    pub psfc_varies_with_condition: bool,
    /// Lower and upper ends of the measured comparison band, kg/(kW h).
    pub measured_psfc_band_kg_kwh: (f64, f64),
    /// Relative excess of [`Self::implied_psfc_kg_kwh`] over the two ends of
    /// that band, as signed fractions.
    pub relative_excess_over_measured: (f64, f64),
    /// How the implied PSFC would move if the undeclared anchor altitude were
    /// resolved, as `(density kg/m^3, implied PSFC kg/kWh)` pairs across the
    /// flight levels this aircraft actually cruises at. The spread is the
    /// uncertainty the missing datum injects into every off-anchor fuel flow.
    pub anchor_altitude_sensitivity: &'static [(f64, f64)],
    /// What may and may not be concluded from the numbers above.
    pub validity: &'static str,
    /// Documents behind the anchor.
    pub source: &'static str,
}

impl Pw127m568fModel {
    /// The constant power-specific fuel consumption this model burns at,
    /// kg/(kW h).
    ///
    /// This is the physical quantity; [`Self::reference_psfc_kg_kwh`] is the
    /// rating-basis coefficient it is derived from and is **not** a PSFC.
    #[must_use]
    pub fn implied_psfc_kg_kwh(self) -> f64 {
        self.reference_psfc_kg_kwh / self.power_lapse_fraction(self.fuel_reference_density_kg_m3)
    }

    /// The typed fuel-calibration statement for this installation.
    #[must_use]
    pub fn fuel_calibration(self) -> TurbopropFuelCalibration {
        /// Measured PW120A from DFDR flight data, SRS-TSD-002 Tab. 3.3 p. 5,
        /// and NASA GASP's ATR 42-600 BSFC, NTRS 20230006542 Tab. 4 p. 9. The
        /// band is taken from the lowest published value to the highest.
        const MEASURED_BAND_KG_KWH: (f64, f64) = (0.284, 0.302);
        let implied = self.implied_psfc_kg_kwh();
        TurbopropFuelCalibration {
            // 762 kg/h, both engines, no APU.
            anchor_total_fuel_flow_kg_s: 762.0 / 3_600.0,
            anchor_engine_count: 2,
            anchor_condition: "95 % MTOW, ISA, optimum FL, 275 KTAS, both engines, no APU (the ATR 72-600 has none)",
            assumed_anchor_density_kg_m3: self.fuel_reference_density_kg_m3,
            // The factsheet says "optimum FL" and stops there.
            anchor_altitude_is_published: false,
            implied_psfc_kg_kwh: implied,
            psfc_varies_with_condition: false,
            measured_psfc_band_kg_kwh: MEASURED_BAND_KG_KWH,
            relative_excess_over_measured: (
                implied / MEASURED_BAND_KG_KWH.0 - 1.0,
                implied / MEASURED_BAND_KG_KWH.1 - 1.0,
            ),
            // ISA density at FL160, FL170, FL180, FL200, FL250, with the PSFC
            // the calibration would imply if the anchor sat there.
            anchor_altitude_sensitivity: &[
                (0.7460, 0.34764),
                (0.7218, 0.35636),
                (0.6981, 0.36536),
                (0.6527, 0.38428),
                (0.5489, 0.43755),
            ],
            validity: "Single-point calibration. The implied PSFC is constant at every altitude, airspeed and power setting, so climb, descent, hold, taxi, flight idle and sea-level take-off fuel are all extrapolated from one cruise point; a real turboprop's PSFC rises steeply as power falls, so low-power fuel flow is a lower bound rather than an estimate. The constant is 21-28 % above a measured PW120A and it is cancelling an optimistic drag polar: closing the published 762 kg/h with a measured PSFC implies an aircraft-level cruise L/D of about 13.0 against this model's 16.5. Neither was changed alone, because the anchor is the only aircraft-level datum that constrains the pair and moving one would break it while leaving the other wrong. The anchor's own flight level is not published, and that missing datum alone spans a 26 % range of implied PSFC.",
            source: "ATR 72-600 factsheet and ATR Family brochure p. 19 (762 kg/h, 275 KTAS, 95 % MTOW, ISA, optimum FL); Pratt & Whitney Canada PW127M ratings via EASA TCDS IM.E.041; measured comparison from SRS-TSD-002 Tab. 3.3 p. 5 and NASA NTRS 20230006542 Tab. 4 p. 9.",
        }
    }
}

/// What a field-performance consumer needs from a propeller installation.
///
/// All thrusts, powers and flows are **per engine**. The consumer multiplies
/// by the installed engine count and applies its own failure case; this
/// contract does not know how many engines the airframe has.
///
/// The three airspeeds are not arbitrary. A ground roll integrates
/// `V dV = a ds`, so distance weights every speed uniformly in `V^2`: the
/// roll-mean of a quantity that is linear in `V^2` is its value at
/// `V^2 = V_LOF^2 / 2`, i.e. at `V = V_LOF / sqrt(2)`. That is the standard
/// ground-roll mean-thrust convention and it is exact for a thrust linear in
/// `V^2`; for this model's thrust it is an approximation, and it is declared
/// as one rather than integrated, because integrating a surrogate to higher
/// order would not make it more true.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurbopropFieldPerformance {
    /// Certificated rating the contract was evaluated at.
    pub rating: Pw127mRating,
    /// Ambient density the contract was evaluated at, kg/m^3.
    pub density_kg_m3: f64,
    /// Shaft power available at this density after the declared lapse, W.
    pub available_shaft_power_per_engine_w: f64,
    /// Propeller thrust at zero airspeed, N.
    pub static_thrust_per_engine_n: f64,
    /// Lift-off true airspeed the contract was asked about, m/s.
    pub lift_off_true_airspeed_m_s: f64,
    /// `V_LOF / sqrt(2)`, the speed the roll-mean is taken at, m/s.
    pub mean_ground_roll_true_airspeed_m_s: f64,
    /// Propeller thrust at [`Self::mean_ground_roll_true_airspeed_m_s`], N.
    pub mean_ground_roll_thrust_per_engine_n: f64,
    /// Propeller thrust at lift-off, N.
    pub lift_off_thrust_per_engine_n: f64,
    /// Fuel flow over the roll, kg/s.
    pub mean_ground_roll_fuel_flow_per_engine_kg_s: f64,
    /// Propulsive efficiency at lift-off, dimensionless.
    pub lift_off_propulsive_efficiency: f64,
    /// Band on [`Self::static_thrust_per_engine_n`].
    pub static_thrust_uncertainty: TurbopropThrustUncertainty,
    /// Band on the two forward-flight thrusts above.
    pub forward_flight_thrust_uncertainty: TurbopropThrustUncertainty,
    /// Residual core exhaust thrust the installation declares, N. `0.0` on the
    /// ATR 72-600, and never a converted shaft power.
    pub residual_jet_thrust_per_engine_n: f64,
    /// What the fuel flow above rests on. Carried with the number so a
    /// consumer cannot display or gate on a single-point calibration without
    /// being able to see that that is what it is.
    pub fuel_calibration: TurbopropFuelCalibration,
    /// Evidence class of the propeller model behind every thrust above.
    pub model_uncertainty: ModelUncertainty,
    /// Envelope the numbers above are only meaningful inside.
    pub envelope: TurbopropOperatingEnvelope,
    /// The evaluated model's own provenance string, unmodified.
    pub provenance: &'static str,
}

impl Pw127m568fModel {
    /// The shaft power this rating actually delivers at an ambient density, W.
    ///
    /// The rating is a sea-level-static certificated number; this applies the
    /// declared minimum-hypothesis lapse to it. Exposed because a consumer
    /// sizing a field length or a climb gradient needs the available power,
    /// and back-computing it from a thrust and an efficiency would invert a
    /// surrogate.
    #[must_use]
    pub fn available_shaft_power_w(self, rating: Pw127mRating, density_kg_m3: f64) -> f64 {
        self.rated_shaft_power_w(rating) * self.power_lapse_fraction(density_kg_m3)
    }

    /// The declared installation envelope, with its lapse floor filled in.
    #[must_use]
    pub fn operating_envelope(self) -> TurbopropOperatingEnvelope {
        // The density at which `(rho/rho_ref)^n` reaches the declared floor.
        let floor_density_kg_m3 = if self.power_lapse_density_exponent > 0.0 {
            self.power_lapse_reference_density_kg_m3
                * self
                    .minimum_power_lapse_fraction
                    .powf(1.0 / self.power_lapse_density_exponent)
        } else {
            0.0
        };
        TurbopropOperatingEnvelope::atr72_600(floor_density_kg_m3)
    }

    /// Assemble the field-performance contract at one density and rating.
    ///
    /// `lift_off_true_airspeed_m_s` is the consumer's own `V_LOF`; this model
    /// has no lift coefficient and does not compute one. Errors propagate
    /// unchanged from [`Self::evaluate`]: a governor that cannot absorb the
    /// commanded power at a pitch inside the surrogate's bounds is reported,
    /// not clipped.
    pub fn field_performance(
        self,
        density_kg_m3: f64,
        rating: Pw127mRating,
        lift_off_true_airspeed_m_s: f64,
    ) -> Result<TurbopropFieldPerformance, TurbopropError> {
        if !lift_off_true_airspeed_m_s.is_finite() {
            return Err(TurbopropError::NonFinite("lift_off_true_airspeed_m_s"));
        }
        if lift_off_true_airspeed_m_s <= 0.0 {
            return Err(TurbopropError::OutsideDomain {
                field: "lift_off_true_airspeed_m_s",
                value: lift_off_true_airspeed_m_s,
            });
        }
        let mean_roll_speed_m_s = lift_off_true_airspeed_m_s / std::f64::consts::SQRT_2;
        let command = TurbopropCommand {
            rating,
            power_fraction: 1.0,
            mode: TurbopropMode::Governed,
            propeller_speed_rpm: self.governed_propeller_speed_rpm,
        };
        let at = |true_airspeed_m_s: f64| {
            self.evaluate(
                TurbopropCondition {
                    density_kg_m3,
                    true_airspeed_m_s,
                },
                command,
            )
        };
        let static_point = at(0.0)?;
        let mean_roll = at(mean_roll_speed_m_s)?;
        let lift_off = at(lift_off_true_airspeed_m_s)?;
        Ok(TurbopropFieldPerformance {
            rating,
            density_kg_m3,
            available_shaft_power_per_engine_w: self.available_shaft_power_w(rating, density_kg_m3),
            static_thrust_per_engine_n: static_point.total_thrust_n,
            lift_off_true_airspeed_m_s,
            mean_ground_roll_true_airspeed_m_s: mean_roll_speed_m_s,
            mean_ground_roll_thrust_per_engine_n: mean_roll.total_thrust_n,
            lift_off_thrust_per_engine_n: lift_off.total_thrust_n,
            mean_ground_roll_fuel_flow_per_engine_kg_s: mean_roll.fuel_flow_kg_s,
            lift_off_propulsive_efficiency: lift_off.propulsive_efficiency,
            static_thrust_uncertainty: TurbopropThrustUncertainty::static_thrust(
                self.static_figure_of_merit,
            ),
            forward_flight_thrust_uncertainty: TurbopropThrustUncertainty::forward_flight_thrust(
                self.blade_efficiency_cruise,
            ),
            residual_jet_thrust_per_engine_n: self.residual_jet_thrust_n,
            fuel_calibration: self.fuel_calibration(),
            model_uncertainty: lift_off.propeller_model_uncertainty,
            envelope: self.operating_envelope(),
            provenance: lift_off.provenance,
        })
    }
}
