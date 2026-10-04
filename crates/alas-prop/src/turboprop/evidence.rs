// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

// The typed contract a consumer outside `alas-prop` needs in order to fly a
// turboprop.
//
// Everything here is assembled from [`Pw127m568fModel::evaluate`] and adds no
// new physics: it exists because the quantities a field-performance, mission
// or results consumer needs (a static thrust, a representative ground-roll
// thrust, the fuel flow that goes with them, and the band and envelope they
// are only valid inside) were reachable only by knowing which operating
// points to ask for and which caveats to carry. A consumer that reads a bare
// number out of [`TurbopropOutput`] cannot tell a certificated rating from a
// class-level surrogate; this contract makes that impossible to lose.
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
    /// Momentum theory gives `T = FM^(2/3) (2 rho A)^(1/3) P^(2/3)` (Lutze,
    /// *Performance 10: Thrust Models*, Virginia Tech AOE 3104, eq. 5-14).
    /// The class-level static figure of merit is 0.70 for preliminary design
    /// with 0.65-0.80 the band real propellers of this size sit in (secondary
    /// transcription of Gudmundsson-type text; no 568F-1 static datum is
    /// public), so the declared 0.70 gives `(0.65/0.70)^(2/3) - 1 = -4.8 %`
    /// and `(0.80/0.70)^(2/3) - 1 = +9.3 %`. An end of the band the declared
    /// value already sits on contributes nothing.
    #[must_use]
    pub fn static_thrust(figure_of_merit: f64) -> Self {
        /// Pessimistic end of the static figure-of-merit band.
        const LOWER_FIGURE_OF_MERIT: f64 = 0.65;
        /// Optimistic end of the static figure-of-merit band.
        const UPPER_FIGURE_OF_MERIT: f64 = 0.80;
        let usable = figure_of_merit.is_finite() && figure_of_merit > 0.0;
        let relative_low = if usable && figure_of_merit > LOWER_FIGURE_OF_MERIT {
            (LOWER_FIGURE_OF_MERIT / figure_of_merit).powf(2.0 / 3.0) - 1.0
        } else {
            0.0
        };
        let relative_high = if usable && figure_of_merit < UPPER_FIGURE_OF_MERIT {
            (UPPER_FIGURE_OF_MERIT / figure_of_merit).powf(2.0 / 3.0) - 1.0
        } else {
            0.0
        };
        Self {
            relative_low,
            relative_high,
            basis: "static figure of merit declared at the class preliminary-design value 0.70 inside a 0.65-0.80 band; momentum-theory static thrust scales as FM^(2/3). Closing it needs a measured 568F-1 map, which is not public.",
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
    /// What the fuel flow rests on, and where it is extrapolated.
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
            // 275 kt true airspeed. Without the blade-efficiency loss the
            // model carries a 14 % thrust surplus here and can exceed this
            // speed; with it, it cannot, which is the one aircraft-level check
            // available on that loss.
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
            fuel_validity: "class PSFC on shaft power, 0.2945 kg/kWh at 249.65 K (measured PW120A, Majeed 2009 Tab. 3.3), scaled as sqrt(T/T_ref) and flat in power over the 46-100 % band it is sourced on; below 46 % of take-off power it is held flat as an unsourced assumption, so descent, hold and idle fuel are a lower bound. ATR's published 762 kg/h two-engine maximum-cruise flow is a validation point, not an input: the model is compared against it, it is not fitted to it.",
            source: "ATR 72-600 factsheet (ratings, 3.93 m 568F-1, 1,200 rev/min, 762 kg/h, 275 kt, 25,000 ft); EASA TCDS EASA.A.084; EASA TCDS IM.E.041 section 5 for the PW127M ratings and flat-rating temperatures; Majeed 2009 SRS-TSD-002 for the PSFC; Nita 2008 eq. 3.5.11 for the lapse exponent.",
        }
    }
}

/// How the class fuel model compares with ATR's published cruise fuel flow.
///
/// The fuel flow is **not** calibrated on this point any more: it is shaft
/// power times [`Pw127m568fModel::psfc_kg_kwh`], a measured PW100-family
/// PSFC scaled by ambient temperature, with the available power from the
/// TCDS flat rating and a sourced density lapse. The published 762 kg/h is
/// what the result is checked against, and every field here is either the
/// published datum, a declared assumption, or the model's own evaluation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurbopropFuelValidation {
    /// ATR's published maximum-cruise flow, kg/s, summed over both engines:
    /// 762 kg/h.
    pub published_total_fuel_flow_kg_s: f64,
    /// Installed engine count the published flow is quoted for.
    pub published_engine_count: u32,
    /// The published point's condition, verbatim from the source.
    pub published_condition: &'static str,
    /// Assumed ambient density of the published point, kg/m^3 (ISA day).
    pub assumed_density_kg_m3: f64,
    /// Whether the point's altitude is published at all. It is not.
    pub altitude_is_published: bool,
    /// The model's two-engine flow at maximum-cruise power, 275 KTAS and the
    /// assumed density on the ISA day, kg/s.
    pub modelled_total_fuel_flow_kg_s: f64,
    /// `modelled / published - 1`.
    pub relative_error: f64,
    /// The PSFC the model burns at that point, kg/(kW h).
    pub modelled_psfc_kg_kwh: f64,
    /// Shaft power per engine the model makes available there, W.
    pub modelled_shaft_power_per_engine_w: f64,
    /// The PW100-family PSFC range of the source, kg/(kW h): Majeed (2009)
    /// flight data and cycle model, 0.28-0.31.
    pub source_psfc_band_kg_kwh: (f64, f64),
    /// The comparison repeated with the point placed at ISA FL160, FL170,
    /// FL180, FL200 and FL250, as `(density kg/m^3, relative error)`; the
    /// spread is what the missing altitude alone is worth.
    pub altitude_sensitivity: [(f64, f64); 5],
    /// What may and may not be concluded from the numbers above.
    pub validity: &'static str,
    /// Documents behind the published point and the model.
    pub source: &'static str,
}

impl Pw127m568fModel {
    /// Compare the class fuel model with the published 762 kg/h.
    ///
    /// Engine level only: maximum-cruise power is what ATR quotes the flow
    /// at, so the check needs no airframe drag. Whether the aircraft needs
    /// that power at 275 KTAS is an aircraft-level question for the mission.
    ///
    /// # Errors
    ///
    /// Any [`TurbopropError`] from evaluating the maximum-cruise point.
    pub fn fuel_validation(self) -> Result<TurbopropFuelValidation, TurbopropError> {
        /// 275 KTAS, m/s.
        const PUBLISHED_TRUE_AIRSPEED_M_S: f64 = 275.0 * 1_852.0 / 3_600.0;
        /// ISA density at FL160, FL170, FL180, FL200 and FL250, kg/m^3.
        const ISA_DENSITIES_KG_M3: [f64; 5] = [0.7460, 0.7218, 0.6981, 0.6527, 0.5489];
        // The catalogue writes the published two-engine flow into
        // `reference_psfc_kg_kwh` per unit of maximum-cruise rating; for the
        // ATR 72-600 this recovers 762 kg/h.
        let published_total_fuel_flow_kg_s =
            self.reference_psfc_kg_kwh * 2.0 * self.maximum_cruise_power_w / JOULES_PER_KWH;
        let at = |density_kg_m3: f64| {
            let temperature_k = isa_temperature_from_density_k(
                density_kg_m3,
                self.power_lapse_reference_density_kg_m3,
            );
            self.evaluate_at_temperature(
                TurbopropCondition {
                    density_kg_m3,
                    true_airspeed_m_s: PUBLISHED_TRUE_AIRSPEED_M_S,
                },
                temperature_k,
                TurbopropCommand {
                    rating: Pw127mRating::MaximumCruise,
                    power_fraction: 1.0,
                    mode: TurbopropMode::Governed,
                    propeller_speed_rpm: self.governed_propeller_speed_rpm,
                },
            )
        };
        let point = at(self.fuel_reference_density_kg_m3)?;
        let modelled_total_fuel_flow_kg_s = 2.0 * point.fuel_flow_kg_s;
        let mut altitude_sensitivity = [(0.0, 0.0); 5];
        for (entry, density_kg_m3) in altitude_sensitivity.iter_mut().zip(ISA_DENSITIES_KG_M3) {
            let flow_kg_s = 2.0 * at(density_kg_m3)?.fuel_flow_kg_s;
            *entry = (
                density_kg_m3,
                flow_kg_s / published_total_fuel_flow_kg_s - 1.0,
            );
        }
        Ok(TurbopropFuelValidation {
            published_total_fuel_flow_kg_s,
            published_engine_count: 2,
            published_condition: "95 % MTOW, ISA, optimum FL, 275 KTAS, maximum cruise power, both engines, no APU (the ATR 72-600 has none)",
            assumed_density_kg_m3: self.fuel_reference_density_kg_m3,
            // The factsheet says "optimum FL" and stops there.
            altitude_is_published: false,
            modelled_total_fuel_flow_kg_s,
            relative_error: modelled_total_fuel_flow_kg_s / published_total_fuel_flow_kg_s - 1.0,
            modelled_psfc_kg_kwh: point.psfc_kg_kwh,
            modelled_shaft_power_per_engine_w: point.engine_shaft_power_w,
            source_psfc_band_kg_kwh: (0.28, 0.31),
            altitude_sensitivity,
            validity: "Validation, not calibration: no model constant was chosen to reproduce this point. The PSFC is measured PW120A data (a PW127M is probably a few percent better, so the flow is biased high on that account) and the available maximum-cruise power follows the TCDS flat rating and Nita's PW120 sigma^0.728 chart fit, whose own scatter is 5-10 % rms; Majeed's constant-TIT cycle model lapses much less (sigma^0.31 between 13,000 and 25,000 ft), so the modelled power, and with it this flow, is more likely low than high. The point's flight level is not published.",
            source: "ATR 72-600 factsheet and ATR Family brochure p. 19 (762 kg/h, 275 KTAS, 95 % MTOW, ISA, optimum FL); EASA TCDS IM.E.041 section 5 (PW127M ratings and flat-rating temperatures); Majeed, SRS-TSD-002 Rev. 1, 2009, Tab. 3.3, 4.1-4.3 (PSFC); Nita 2008 eq. 3.5.11 (lapse).",
        })
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
    /// How the fuel model behind the flow above compares with the published
    /// cruise point. Carried with the number so a consumer can see how far
    /// the class model is from the one aircraft-level fuel datum; `None` when
    /// this model cannot be evaluated at that point.
    pub fuel_validation: Option<TurbopropFuelValidation>,
    /// Evidence class of the propeller model behind every thrust above.
    pub model_uncertainty: ModelUncertainty,
    /// Envelope the numbers above are only meaningful inside.
    pub envelope: TurbopropOperatingEnvelope,
    /// The evaluated model's own provenance string, unmodified.
    pub provenance: &'static str,
}

impl Pw127m568fModel {
    /// The declared installation envelope, with its lapse floor filled in.
    #[must_use]
    pub fn operating_envelope(self) -> TurbopropOperatingEnvelope {
        // The density at which `(rho/rho_corner)^n` reaches the declared
        // floor, taken from the take-off corner, the highest of the corners,
        // so the floor is not understated for any rating.
        let floor_density_kg_m3 = if self.power_lapse_density_exponent > 0.0 {
            self.flat_rating_corner_density_kg_m3(Pw127mRating::NormalTakeoff)
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
            fuel_validation: self.fuel_validation().ok(),
            model_uncertainty: lift_off.propeller_model_uncertainty,
            envelope: self.operating_envelope(),
            provenance: lift_off.provenance,
        })
    }
}
