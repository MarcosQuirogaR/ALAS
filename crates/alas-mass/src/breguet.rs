// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! An analytic Breguet-range [`FuelBurnModel`] for the conceptual design
//! loop.
//!
//! The mission network's own segment-by-segment model flies a real
//! trajectory and is too slow to call thousands of times inside a sizing
//! iteration. [`BreguetFuelModel`] prices the same quantities from a
//! parabolic drag polar, a constant cruise TSFC, and a handful of published
//! mission-segment fractions for the climb, descent and landing fuel the
//! Breguet range equation itself does not cover: closed-form, cheap, and
//! deliberately not a substitute for flying the finalist through the real
//! mission afterwards.
//!
//! `alas-mass` does not depend on `alas-atmo`; every density this model
//! needs is a constructor input the caller reads from an atmosphere model
//! at the chosen altitude, rather than computed here from an altitude
//! argument.

use crate::fuel_plan::{FuelBurnModel, FuelModelError, LegEstimate};

/// Published mission-segment fuel fractions (mass remaining as a fraction of
/// the mass at the start of the phase), from Raymer, *Aircraft Design: A
/// Conceptual Approach*, Table 3.2, and the equivalent table in Roskam,
/// *Airplane Design, Part I*: 0.995 for takeoff, 0.980 for climb to initial
/// cruise altitude, 0.990 for descent, and 0.995 for approach and landing,
/// for a jet transport.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SegmentFractions {
    /// Mass fraction remaining after takeoff.
    pub takeoff: f64,
    /// Mass fraction remaining after climb to the initial cruise level.
    pub climb: f64,
    /// Mass fraction remaining after descent to the approach.
    pub descent: f64,
    /// Mass fraction remaining after approach and landing.
    pub landing: f64,
}

impl Default for SegmentFractions {
    fn default() -> Self {
        Self {
            takeoff: 0.995,
            climb: 0.980,
            descent: 0.990,
            landing: 0.995,
        }
    }
}

/// An analytic Breguet-range fuel model: one cruise condition, one TSFC, and
/// the published segment fractions for everything the range equation does
/// not price.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BreguetFuelModel {
    /// True airspeed flown in cruise and in the diversion cruise leg, m/s.
    pub cruise_tas_m_s: f64,
    /// Air density at the cruise altitude, kg/m^3.
    pub cruise_density_kg_m3: f64,
    /// Air density at the holding altitude, kg/m^3.
    ///
    /// The closed-form maximum lift-to-drag ratio a parabolic drag polar
    /// gives at its best-endurance `CL = sqrt(cd0/k)` does not depend on
    /// density: that `CL` is reached at whatever dynamic pressure the mass
    /// and speed combination produces, at any altitude. This field is
    /// therefore validated, but [`FuelBurnModel::holding_fuel_flow_kg_s`]
    /// below does not read it; it is retained so a holding condition here
    /// is a complete atmospheric state, and for callers that need the
    /// density independently of this model's own holding formula.
    pub holding_density_kg_m3: f64,
    /// Reference wing area, m^2.
    pub wing_area_m2: f64,
    /// Zero-lift drag coefficient.
    pub cd0: f64,
    /// Induced drag factor `k` in `CD = cd0 + k CL^2`.
    pub induced_factor_k: f64,
    /// Cruise thrust-specific fuel consumption, kg of fuel per newton of
    /// thrust per second.
    pub tsfc_cruise_kg_per_n_s: f64,
    /// Ratio of holding TSFC to cruise TSFC.
    ///
    /// Three effects nearly cancel between cruise and a 1,500 ft hold: TSFC
    /// falls with the lower Mach number, rises with the higher static
    /// temperature, and rises again at the low part-power setting a hold
    /// needs. For a high-bypass narrowbody the holding fuel flow lands within
    /// 0.9-1.1 of the cruise fuel flow at the same mass (the sanity anchor in
    /// the 2026-09-05 fuel-regulation research note, section 7.1), which is
    /// where [`Self::DEFAULT_HOLDING_TSFC_FACTOR`] comes from. This is an
    /// engineering assumption, not a manufacturer deck, and a study of a
    /// specific engine should override it.
    pub holding_tsfc_factor: f64,
    /// All-engines takeoff fuel flow at sea-level static, kg/s.
    pub takeoff_fuel_flow_kg_s: f64,
    /// Ground-idle fuel flow as a fraction of the takeoff flow.
    pub idle_fuel_flow_fraction: f64,
    /// Gravitational acceleration, m/s^2.
    pub gravity_m_s2: f64,
    /// Published mission-segment fuel fractions for takeoff, climb, descent
    /// and landing.
    pub segment_fractions: SegmentFractions,
    /// Horizontal distance covered by climb and descent, m, subtracted from
    /// the mission range before the cruise Breguet integration and never
    /// applied below zero. A representative narrowbody climb-and-descent
    /// credit is of order 250 km; a design mission that flies climb and
    /// descent as explicit segments should size this from them instead of
    /// using a fixed assumption.
    pub climb_descent_range_credit_m: f64,
}

impl BreguetFuelModel {
    /// Recommended holding-to-cruise TSFC ratio; see
    /// [`BreguetFuelModel::holding_tsfc_factor`].
    pub const DEFAULT_HOLDING_TSFC_FACTOR: f64 = 1.0;

    /// Recommended idle-to-takeoff fuel-flow ratio, matching the mission
    /// deck's flight-idle fraction; see
    /// [`BreguetFuelModel::idle_fuel_flow_fraction`].
    pub const DEFAULT_IDLE_FUEL_FLOW_FRACTION: f64 = 0.07;

    /// Lift-to-drag ratio at `mass_kg` and `density_kg_m3`, flown at
    /// [`BreguetFuelModel::cruise_tas_m_s`], from a parabolic drag polar:
    /// `CL = m g / (q S)`, `CD = cd0 + k CL^2`.
    pub fn lift_to_drag_at(&self, mass_kg: f64, density_kg_m3: f64) -> f64 {
        let dynamic_pressure_pa = 0.5 * density_kg_m3 * self.cruise_tas_m_s * self.cruise_tas_m_s;
        let lift_coefficient =
            mass_kg * self.gravity_m_s2 / (dynamic_pressure_pa * self.wing_area_m2);
        let drag_coefficient =
            self.cd0 + self.induced_factor_k * lift_coefficient * lift_coefficient;
        lift_coefficient / drag_coefficient
    }

    /// Maximum lift-to-drag ratio of the parabolic polar, `1 / (2 sqrt(cd0 k))`:
    /// the minimum-drag condition, independent of mass, speed and
    /// density.
    fn max_lift_to_drag(&self) -> f64 {
        1.0 / (2.0 * (self.cd0 * self.induced_factor_k).sqrt())
    }

    /// One mean-mass-corrected Breguet cruise: the lift-to-drag ratio is
    /// evaluated at `start_mass_kg`, then re-evaluated once at the mean of
    /// the start mass and the first-pass end mass, which tracks the
    /// mass-weighted average L/D over the leg closely enough for a
    /// single-digit-percent fuel fraction without an iterative solve. Both
    /// the trip and the diversion leg fly this same cruise model.
    fn cruise_breguet(&self, start_mass_kg: f64, distance_m: f64) -> f64 {
        let lift_to_drag_at_start = self.lift_to_drag_at(start_mass_kg, self.cruise_density_kg_m3);
        let end_mass_first_pass_kg =
            start_mass_kg * self.breguet_mass_ratio(distance_m, lift_to_drag_at_start);

        let mean_mass_kg = 0.5 * (start_mass_kg + end_mass_first_pass_kg);
        let lift_to_drag_at_mean = self.lift_to_drag_at(mean_mass_kg, self.cruise_density_kg_m3);
        start_mass_kg * self.breguet_mass_ratio(distance_m, lift_to_drag_at_mean)
    }

    /// The Breguet mass ratio `exp(-R tsfc g / (V (L/D)))` for a cruise of
    /// `distance_m` at `lift_to_drag`.
    fn breguet_mass_ratio(&self, distance_m: f64, lift_to_drag: f64) -> f64 {
        (-distance_m * self.tsfc_cruise_kg_per_n_s * self.gravity_m_s2
            / (self.cruise_tas_m_s * lift_to_drag))
            .exp()
    }

    /// Whether every input is finite and inside a physically admissible
    /// range.
    ///
    /// # Errors
    ///
    /// [`FuelModelError::InvalidModel`], naming the first field found
    /// outside its admissible range.
    pub fn validate(&self) -> Result<(), FuelModelError> {
        let positive = [
            ("cruise_tas_m_s", self.cruise_tas_m_s),
            ("cruise_density_kg_m3", self.cruise_density_kg_m3),
            ("holding_density_kg_m3", self.holding_density_kg_m3),
            ("wing_area_m2", self.wing_area_m2),
            ("cd0", self.cd0),
            ("induced_factor_k", self.induced_factor_k),
            ("tsfc_cruise_kg_per_n_s", self.tsfc_cruise_kg_per_n_s),
            ("holding_tsfc_factor", self.holding_tsfc_factor),
            ("takeoff_fuel_flow_kg_s", self.takeoff_fuel_flow_kg_s),
            ("gravity_m_s2", self.gravity_m_s2),
        ];
        for (name, value) in positive {
            if !value.is_finite() || value <= 0.0 {
                return Err(FuelModelError::InvalidModel(format!(
                    "breguet fuel model {name} must be finite and positive, got {value}"
                )));
            }
        }
        if !self.idle_fuel_flow_fraction.is_finite()
            || !(0.0..=1.0).contains(&self.idle_fuel_flow_fraction)
        {
            return Err(FuelModelError::InvalidModel(format!(
                "breguet fuel model idle_fuel_flow_fraction must lie in [0, 1], got {}",
                self.idle_fuel_flow_fraction
            )));
        }
        if !self.climb_descent_range_credit_m.is_finite() || self.climb_descent_range_credit_m < 0.0
        {
            return Err(FuelModelError::InvalidModel(format!(
                "breguet fuel model climb_descent_range_credit_m must be finite and nonnegative, got {}",
                self.climb_descent_range_credit_m
            )));
        }
        for (name, value) in [
            ("takeoff", self.segment_fractions.takeoff),
            ("climb", self.segment_fractions.climb),
            ("descent", self.segment_fractions.descent),
            ("landing", self.segment_fractions.landing),
        ] {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(FuelModelError::InvalidModel(format!(
                    "breguet fuel model segment_fractions.{name} must lie in [0, 1], got {value}"
                )));
            }
        }
        Ok(())
    }
}

impl FuelBurnModel for BreguetFuelModel {
    fn trip(&self, takeoff_mass_kg: f64, range_m: f64) -> Result<LegEstimate, FuelModelError> {
        self.validate()?;
        let takeoff_mass_kg = checked_positive_mass(takeoff_mass_kg)?;
        let range_m = checked_nonnegative_distance(range_m)?;

        let post_climb_mass_kg =
            takeoff_mass_kg * self.segment_fractions.takeoff * self.segment_fractions.climb;
        let cruise_range_m = (range_m - self.climb_descent_range_credit_m).max(0.0);
        let cruise_end_mass_kg = self.cruise_breguet(post_climb_mass_kg, cruise_range_m);
        let landing_mass_kg =
            cruise_end_mass_kg * self.segment_fractions.descent * self.segment_fractions.landing;

        Ok(LegEstimate {
            fuel_kg: takeoff_mass_kg - landing_mass_kg,
            time_s: range_m / self.cruise_tas_m_s,
        })
    }

    fn diversion(
        &self,
        start_mass_kg: f64,
        distance_m: f64,
    ) -> Result<LegEstimate, FuelModelError> {
        self.validate()?;
        let start_mass_kg = checked_positive_mass(start_mass_kg)?;
        let distance_m = checked_nonnegative_distance(distance_m)?;

        // One climb fraction covers the go-around and the climb back to
        // cruise level after the missed approach; there is no separate
        // takeoff fraction, since the aircraft never stops and re-starts.
        let post_climb_mass_kg = start_mass_kg * self.segment_fractions.climb;
        let cruise_end_mass_kg = self.cruise_breguet(post_climb_mass_kg, distance_m);
        let landing_mass_kg =
            cruise_end_mass_kg * self.segment_fractions.descent * self.segment_fractions.landing;

        Ok(LegEstimate {
            fuel_kg: start_mass_kg - landing_mass_kg,
            time_s: distance_m / self.cruise_tas_m_s,
        })
    }

    fn holding_fuel_flow_kg_s(&self, mass_kg: f64, altitude_m: f64) -> Result<f64, FuelModelError> {
        self.validate()?;
        let mass_kg = checked_positive_mass(mass_kg)?;
        // The altitude only selects which density the caller should have
        // used to build this model; the closed-form (L/D)max below does not
        // consume it, so it is validated for finiteness and otherwise
        // ignored. Reuses `InvalidDistance` because an altitude is a length
        // in metres and `fuel_plan` defines no separate variant for one.
        if !altitude_m.is_finite() {
            return Err(FuelModelError::InvalidDistance {
                distance_m: altitude_m,
            });
        }
        let holding_tsfc_kg_per_n_s = self.holding_tsfc_factor * self.tsfc_cruise_kg_per_n_s;
        let thrust_required_n = mass_kg * self.gravity_m_s2 / self.max_lift_to_drag();
        Ok(holding_tsfc_kg_per_n_s * thrust_required_n)
    }

    fn cruise_fuel_flow_kg_s(&self, mass_kg: f64) -> Result<f64, FuelModelError> {
        self.validate()?;
        let mass_kg = checked_positive_mass(mass_kg)?;
        let lift_to_drag = self.lift_to_drag_at(mass_kg, self.cruise_density_kg_m3);
        Ok(self.tsfc_cruise_kg_per_n_s * mass_kg * self.gravity_m_s2 / lift_to_drag)
    }

    fn taxi_fuel_flow_kg_s(&self) -> Result<f64, FuelModelError> {
        self.validate()?;
        Ok(self.idle_fuel_flow_fraction * self.takeoff_fuel_flow_kg_s)
    }
}

fn checked_positive_mass(mass_kg: f64) -> Result<f64, FuelModelError> {
    if mass_kg.is_finite() && mass_kg > 0.0 {
        Ok(mass_kg)
    } else {
        Err(FuelModelError::MassOutOfRange { mass_kg })
    }
}

fn checked_nonnegative_distance(distance_m: f64) -> Result<f64, FuelModelError> {
    if distance_m.is_finite() && distance_m >= 0.0 {
        Ok(distance_m)
    } else {
        Err(FuelModelError::InvalidDistance { distance_m })
    }
}

/// Equivalent thrust-specific fuel consumption for a turboprop, from its
/// power-specific fuel consumption and the speed and efficiency at which
/// shaft power becomes thrust: `TSFC = PSFC * V / eta_p` (the standard
/// turboprop equivalent-SFC relation; see e.g. Raymer, *Aircraft Design: A
/// Conceptual Approach*, ch. 3, or Mattingly, *Elements of Gas Turbine
/// Propulsion*, ch. 2). Useful thrust power is `eta_p` times shaft power, so
/// fuel flow per unit of thrust is fuel flow per unit of shaft power divided
/// by `eta_p / V`.
pub fn equivalent_tsfc_from_psfc(
    psfc_kg_per_w_s: f64,
    tas_m_s: f64,
    propeller_efficiency: f64,
) -> f64 {
    psfc_kg_per_w_s * tas_m_s / propeller_efficiency
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a320_class_model() -> BreguetFuelModel {
        BreguetFuelModel {
            cruise_tas_m_s: 230.0,
            cruise_density_kg_m3: 0.38,
            holding_density_kg_m3: 1.167, // ISA density at 1,500 ft
            wing_area_m2: 122.6,
            cd0: 0.020,
            induced_factor_k: 0.045,
            tsfc_cruise_kg_per_n_s: 1.70e-5, // 0.6 lb/lbf/hr
            holding_tsfc_factor: BreguetFuelModel::DEFAULT_HOLDING_TSFC_FACTOR,
            takeoff_fuel_flow_kg_s: 2.3,
            idle_fuel_flow_fraction: BreguetFuelModel::DEFAULT_IDLE_FUEL_FLOW_FRACTION,
            gravity_m_s2: alas_units::STANDARD_GRAVITY,
            segment_fractions: SegmentFractions::default(),
            climb_descent_range_credit_m: 250_000.0,
        }
    }

    #[test]
    fn an_a320_class_case_lands_trip_fuel_and_flows_in_plausible_brackets() {
        let model = a320_class_model();
        assert!(model.validate().is_ok());

        let range_m = 2_000.0 * alas_units::NAUTICAL_MILE;
        let trip = model.trip(73_500.0, range_m).unwrap();
        assert!(
            (9_000.0..14_000.0).contains(&trip.fuel_kg),
            "trip fuel {} kg outside the expected bracket",
            trip.fuel_kg
        );
        assert!((trip.time_s - range_m / model.cruise_tas_m_s).abs() < 1e-9);

        // A narrowbody's 1,500 ft holding flow is within 0.9-1.1 of its
        // cruise flow at the same mass (fuel-regulation research note,
        // section 7.1); a model predicting half the cruise flow is wrong.
        // The bracket is deliberately wider than the anchor so it checks the
        // physics, not a pinned value from one set of coefficients.
        let holding_flow = model.holding_fuel_flow_kg_s(60_000.0, 457.2).unwrap();
        let cruise_flow = model.cruise_fuel_flow_kg_s(60_000.0).unwrap();
        let ratio = holding_flow / cruise_flow;
        assert!(
            (0.8..1.2).contains(&ratio),
            "holding flow {holding_flow} kg/s is {ratio} of cruise flow {cruise_flow} kg/s"
        );

        let taxi_flow = model.taxi_fuel_flow_kg_s().unwrap();
        assert!(
            (taxi_flow - 0.161).abs() < 0.01,
            "taxi flow {taxi_flow} kg/s"
        );
    }

    #[test]
    fn cruise_flow_falls_as_mass_falls() {
        let model = a320_class_model();
        let heavy = model.cruise_fuel_flow_kg_s(73_500.0).unwrap();
        let light = model.cruise_fuel_flow_kg_s(60_000.0).unwrap();
        assert!(light < heavy);
    }

    #[test]
    fn a_zero_distance_diversion_still_prices_the_missed_approach_and_landing() {
        let model = a320_class_model();
        let leg = model.diversion(60_000.0, 0.0).unwrap();
        assert!(leg.fuel_kg > 0.0);
        assert_eq!(leg.time_s, 0.0);
    }

    #[test]
    fn equivalent_tsfc_scales_with_speed_and_inversely_with_efficiency() {
        let base = equivalent_tsfc_from_psfc(6.0e-8, 130.0, 0.85);
        let faster = equivalent_tsfc_from_psfc(6.0e-8, 260.0, 0.85);
        let less_efficient = equivalent_tsfc_from_psfc(6.0e-8, 130.0, 0.425);
        assert!((faster - 2.0 * base).abs() < 1e-15);
        assert!((less_efficient - 2.0 * base).abs() < 1e-15);
    }

    #[test]
    fn validate_rejects_non_finite_and_nonpositive_inputs() {
        let base = a320_class_model();
        assert!(base.validate().is_ok());

        let mut invalid = base;
        invalid.cd0 = 0.0;
        assert!(invalid.validate().is_err());

        let mut invalid = base;
        invalid.cruise_tas_m_s = f64::NAN;
        assert!(invalid.validate().is_err());

        let mut invalid = base;
        invalid.induced_factor_k = -0.01;
        assert!(invalid.validate().is_err());

        let mut invalid = base;
        invalid.idle_fuel_flow_fraction = 1.5;
        assert!(invalid.validate().is_err());

        let mut invalid = base;
        invalid.climb_descent_range_credit_m = -1.0;
        assert!(invalid.validate().is_err());

        let mut invalid = base;
        invalid.segment_fractions.climb = 1.5;
        assert!(invalid.validate().is_err());

        let mut invalid = base;
        invalid.holding_density_kg_m3 = f64::INFINITY;
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn an_invalid_model_is_reported_by_every_trait_method() {
        let mut model = a320_class_model();
        model.wing_area_m2 = 0.0;
        assert!(model.trip(73_500.0, 1_000_000.0).is_err());
        assert!(model.diversion(60_000.0, 300_000.0).is_err());
        assert!(model.holding_fuel_flow_kg_s(60_000.0, 457.2).is_err());
        assert!(model.cruise_fuel_flow_kg_s(60_000.0).is_err());
        assert!(model.taxi_fuel_flow_kg_s().is_err());
    }

    #[test]
    fn an_invalid_mass_or_distance_argument_is_reported() {
        let model = a320_class_model();
        assert!(matches!(
            model.trip(-1.0, 1_000_000.0),
            Err(FuelModelError::MassOutOfRange { .. })
        ));
        assert!(matches!(
            model.trip(73_500.0, -1.0),
            Err(FuelModelError::InvalidDistance { .. })
        ));
        assert!(matches!(
            model.holding_fuel_flow_kg_s(60_000.0, f64::NAN),
            Err(FuelModelError::InvalidDistance { .. })
        ));
    }
}
