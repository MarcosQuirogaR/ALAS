// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


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
