// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

impl Pw127m568fModel {
    pub(super) fn rated_shaft_power_w(self, rating: Pw127mRating) -> f64 {
        match rating {
            Pw127mRating::NormalTakeoff => self.normal_takeoff_power_w,
            Pw127mRating::MaximumTakeoffReserve => self.maximum_takeoff_reserve_power_w,
            Pw127mRating::MaximumContinuous => self.maximum_continuous_power_w,
            Pw127mRating::MaximumClimb => self.maximum_climb_power_w,
            Pw127mRating::MaximumCruise => self.maximum_cruise_power_w,
            Pw127mRating::FlightIdleSurrogate => 0.08 * self.maximum_continuous_power_w,
        }
    }

    /// The share of shaft power that reaches ideal induced power at this
    /// advance ratio.
    ///
    /// A smoothstep from [`Pw127m568fModel::static_figure_of_merit`] at
    /// `J = 0` to [`Pw127m568fModel::blade_efficiency_cruise`] at and above
    /// [`Pw127m568fModel::blade_efficiency_knee_advance_ratio`], with zero
    /// slope at both ends so no operating point sits on a kink. The shape
    /// between the two is a surrogate: the sources characterise the ends, not
    /// the transition.
    pub(crate) fn blade_efficiency(self, advance_ratio: f64) -> f64 {
        let static_value = self.static_figure_of_merit;
        let forward_value = self.blade_efficiency_cruise;
        // A NaN advance ratio is not positive, and must not be blended into a
        // NaN efficiency; the static value is the defined end of the blend.
        if !advance_ratio.is_finite()
            || advance_ratio <= 0.0
            || self.blade_efficiency_knee_advance_ratio <= 0.0
        {
            return static_value;
        }
        let fraction = (advance_ratio / self.blade_efficiency_knee_advance_ratio).min(1.0);
        let weight = fraction * fraction * (3.0 - 2.0 * fraction);
        static_value + weight * (forward_value - static_value)
    }

    /// Evaluate one engine and propeller at a stated ambient static
    /// temperature, K. No limit is silently clipped.
    ///
    /// The temperature sets the fuel flow through
    /// [`Self::psfc_kg_kwh`]; shaft power and thrust follow the density.
    pub fn evaluate_at_temperature(
        self,
        condition: TurbopropCondition,
        ambient_temperature_k: f64,
        command: TurbopropCommand,
    ) -> Result<TurbopropOutput, TurbopropError> {
        self.validate(condition, command)?;
        if !ambient_temperature_k.is_finite() {
            return Err(TurbopropError::NonFinite("ambient_temperature_k"));
        }
        // Colder than any tropospheric/stratospheric ISA day minus 60 K, or
        // hotter than any airfield, is outside the relation's basis.
        if !(150.0..=350.0).contains(&ambient_temperature_k) {
            return Err(TurbopropError::OutsideDomain {
                field: "ambient_temperature_k",
                value: ambient_temperature_k,
            });
        }
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
                // A shut-down engine burns nothing, and the specific
                // consumption of nothing is undefined rather than zero.
                psfc_kg_kwh: f64::NAN,
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
            * self.power_lapse_fraction(command.rating, condition.density_kg_m3);
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
        // Thrust is the one-dimensional actuator-disk bound evaluated on the
        // share of shaft power that survives blade profile/tip losses and
        // tip-Mach compressibility (see `propeller_loss`). The earlier
        // generic CT/CP polynomial is no longer a thrust source: it was not
        // energy-consistent (J CT/CP > 1 over part of its map) and was only
        // ever kept honest by this bound. At V = 0 the bound with the static
        // figure of merit is exactly `FM^(2/3) (2 rho A P^2)^(1/3)`.
        let helical_tip_mach = self.helical_tip_mach(
            condition.true_airspeed_m_s,
            command.propeller_speed_rpm,
            ambient_temperature_k,
        );
        let blade_efficiency = self.blade_efficiency(advance_ratio)
            * self.tip_mach_efficiency_factor(helical_tip_mach);
        let ideal_thrust_bound_n = actuator_disk_thrust_bound_n(
            blade_efficiency * propeller_power_w,
            condition.density_kg_m3,
            disk_area_m2,
            condition.true_airspeed_m_s,
        );
        // A guard, not a working part of the model: no retrieved source puts a
        // single-rotation propeller of this class above it.
        let efficiency_capped_thrust_n = if condition.true_airspeed_m_s > 0.0 {
            self.maximum_propulsive_efficiency * propeller_power_w / condition.true_airspeed_m_s
        } else {
            f64::INFINITY
        };
        let blade_angle_deg = self.indicative_blade_angle_deg(
            condition.density_kg_m3,
            revolutions_s,
            advance_ratio,
            propeller_power_w,
            command.mode == TurbopropMode::FlightIdle,
        );
        let propeller_thrust_n = if command.mode == TurbopropMode::FlightIdle {
            // No OEM idle pitch or windmilling drag data: zero propeller
            // force is the bounded neutral hypothesis. Nacelle/airframe drag
            // remains outside this isolated propulsion kernel.
            0.0
        } else {
            ideal_thrust_bound_n.min(efficiency_capped_thrust_n)
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
        // Class PSFC on free-turbine shaft power: sqrt(theta) in ambient
        // temperature and a part-power multiplier below the sourced flat band
        // (see `psfc_at_power_kg_kwh`).
        let psfc_kg_kwh = self.psfc_at_power_kg_kwh(
            ambient_temperature_k,
            engine_power_w
                / self
                    .available_shaft_power_w(Pw127mRating::NormalTakeoff, condition.density_kg_m3),
        );
        let fuel_flow_kg_s = engine_power_w * psfc_kg_kwh / JOULES_PER_KWH;
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
            psfc_kg_kwh,
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

    pub(super) fn solve_governor(
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
            ("static_figure_of_merit", self.static_figure_of_merit),
            ("blade_efficiency_cruise", self.blade_efficiency_cruise),
            (
                "blade_efficiency_knee_advance_ratio",
                self.blade_efficiency_knee_advance_ratio,
            ),
            (
                "maximum_propulsive_efficiency",
                self.maximum_propulsive_efficiency,
            ),
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
            ("part_power_knee_fraction", self.part_power_knee_fraction),
            ("part_power_psfc_exponent", self.part_power_psfc_exponent),
            ("tip_mach_critical", self.tip_mach_critical),
            ("tip_mach_loss_coefficient", self.tip_mach_loss_coefficient),
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
                "blade_efficiency_cruise",
                self.blade_efficiency_cruise,
                f64::MIN_POSITIVE,
                1.0,
            ),
            (
                "blade_efficiency_knee_advance_ratio",
                self.blade_efficiency_knee_advance_ratio,
                f64::MIN_POSITIVE,
                10.0,
            ),
            (
                "maximum_propulsive_efficiency",
                self.maximum_propulsive_efficiency,
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
            (
                "part_power_knee_fraction",
                self.part_power_knee_fraction,
                f64::MIN_POSITIVE,
                1.0,
            ),
            (
                "part_power_psfc_exponent",
                self.part_power_psfc_exponent,
                0.0,
                3.0,
            ),
            ("tip_mach_critical", self.tip_mach_critical, 0.3, 2.0),
            (
                "tip_mach_loss_coefficient",
                self.tip_mach_loss_coefficient,
                0.0,
                50.0,
            ),
        ] {
            if value < minimum || value > maximum {
                return Err(TurbopropError::OutsideDomain { field, value });
            }
        }
        self.validate_positive_parameters()
    }
}

/// Ideal positive-thrust limit from one-dimensional actuator-disk theory.
///
/// With disk velocity `u = V + v_i` the momentum and energy balances give
/// thrust `T = 2 rho A u (u - V)` and ideal power `P = T u` (McCormick,
/// "Aerodynamics of V/STOL Flight", 1967, sec. 3.1; Glauert, "Airplane
/// Propellers", in Durand (ed.), Aerodynamic Theory vol. IV, 1935). Hence
/// `u^2 (u - V) = q` with `q = P / (2 rho A)`, i.e. `u^3 - V u^2 - q = 0`.
/// Substituting `u = y + V/3` gives `y^3 + p y + r = 0` with `p = -V^2/3` and
/// `r = -2 V^3/27 - q`. For `V > 0` and `q > 0` the Cardano discriminant
/// `(r/2)^2 + (p/3)^3 = q V^3/27 + q^2/4` is positive, so there is exactly
/// one real root, the physical one (`u >= V`). The second cube root follows
/// from `cbrt(A) cbrt(B) = -p/3 = V^2/9` instead of a subtraction, so no
/// cancellation occurs. The static limit (`V = 0`) is
/// `T = (2 rho A P^2)^(1/3)`; `P <= 0` at forward speed gives no thrust.
/// Callers must supply finite nonnegative power/speed and positive density/area;
/// this arithmetic bound does not validate an engine or propeller operating point.
pub fn actuator_disk_thrust_bound_n(
    shaft_power_w: f64,
    density_kg_m3: f64,
    disk_area_m2: f64,
    true_airspeed_m_s: f64,
) -> f64 {
    if true_airspeed_m_s == 0.0 {
        return (2.0 * density_kg_m3 * disk_area_m2 * shaft_power_w.powi(2)).cbrt();
    }
    if shaft_power_w <= 0.0 {
        return 0.0;
    }
    let speed = true_airspeed_m_s;
    let q = shaft_power_w / (2.0 * density_kg_m3 * disk_area_m2);
    let cube = speed.powi(3) / 27.0;
    let cube_root_a = (cube + 0.5 * q + (q * cube + 0.25 * q * q).sqrt()).cbrt();
    let disk_velocity_m_s = cube_root_a + speed * speed / (9.0 * cube_root_a) + speed / 3.0;
    shaft_power_w / disk_velocity_m_s
}
