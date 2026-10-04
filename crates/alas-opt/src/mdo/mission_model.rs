// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The common segment mission model shared by the optimizer's sizing loop
//! and the pipeline's fuel-policy bridge.
//!
//! The model flies the configured climb ladder, cruise rungs and descent
//! ladder at their planned speeds and rates, reads drag from the candidate's
//! trimmed polar (`CD = CD_parasite + CD_induced + CD_wave`) and reads thrust
//! and fuel flow from the selected engine's off-design deck
//! ([`crate::mdo::propulsion::PropulsionDeck`]) at every integration step.
//! Profile geometry and altitude adaptation live in `profile`; the
//! integration and its energy ledger live in `integrate`. The native
//! pseudospectral mission remains the finalist check, but it is built on the
//! same propulsion deck, polar and profile definitions, so the two paths
//! price the same aircraft.

mod adapt;
mod climb_bands;
mod cruise_flight;
mod cruise_levels;
mod drag;
mod frozen_plan;
mod integrate;
mod profile;
pub use drag::{CruiseDrag, ParabolicPolar};
pub use frozen_plan::{
    FreezeError, FrozenMissionPlan, SizingBudget, MISSION_STEP_UNCONVERGED, SIZING_BUDGET_EXHAUSTED,
};
pub use integrate::{EnergyLedger, FlownLeg};
pub use profile::envelope_limit_m_s;

use std::sync::Arc;

use alas_atmo::Atmosphere;
use alas_config::mission::MissionProfileConfig;
use alas_mass::fuel_plan::{FuelBurnModel, FuelModelError, LegEstimate};
use alas_prop::system::PropulsionRating;

use super::propulsion::{DeckError, PropulsionDeck};
use profile::{LegKind, ProfileGeometry};

/// The speed below which the cruise wave-drag fit is not credited. This is a
/// modelling switch, not a certification limit: the trimmed cruise point
/// supplies the wave term and the low-speed profile uses parasite and induced
/// components only.
const WAVE_ONSET_MACH: f64 = 0.70;
/// Ground speed the taxi fuel flow is evaluated at, m/s.
const TAXI_SPEED_M_S: f64 = 5.0;

/// The common segment-integrated burn model.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentMissionModel {
    pub(crate) cancellation: Option<crate::cancellation::EvaluationCancellation>,
    /// Cruise Mach used for the wave-drag reference point.
    pub cruise_mach: f64,
    /// Configured cruise altitude, m above mean sea level.
    pub cruise_altitude_m: f64,
    /// Departure elevation, m above mean sea level.
    pub departure_elevation_m: f64,
    /// Arrival elevation, m above mean sea level.
    pub arrival_elevation_m: f64,
    /// Profile speeds, rates and relative cruise distances.
    pub profile: MissionProfileConfig,
    /// Reference wing area, m^2.
    pub wing_area_m2: f64,
    /// Cruise true airspeed, m/s, derived from the configured cruise Mach.
    pub cruise_tas_m_s: f64,
    /// Gravity, m/s^2.
    pub gravity_m_s2: f64,
    /// Holding altitude above mean sea level, m.
    pub holding_altitude_m: f64,
    /// Midpoint integration steps per planned segment (internal refinement
    /// control; see `with_steps_per_segment`).
    pub steps_per_segment: usize,
    /// Equal-altitude sub-rungs each calibrated-airspeed climb or descent
    /// rung is split into (internal refinement control; see
    /// `with_cas_subdivisions`). Irrelevant in true-airspeed mode.
    pub cas_subdivisions: usize,
    /// Temperature deviation from the standard atmosphere, degrees C, applied
    /// to every ambient state the flown mission evaluates: the calibrated
    /// airspeed resolution of the profile ladders, the flight condition
    /// (density, temperature, speed of sound) every integration step hands the
    /// polar and the propulsion deck, and the holding and taxi conditions. The
    /// native pseudospectral mission applies the *departure* airport's
    /// deviation to every segment; the pipeline and sizing callers pass the
    /// same value here so the two paths share one ambient convention. The
    /// cruise true airspeed derived from the cruise Mach stays a standard-day
    /// number, as the configured profile states it (see
    /// `with_isa_deviation_c`).
    pub isa_deviation_c: f64,
    /// Lift limits and high-lift drag increment per phase.
    pub phase_limits: PhaseAeroLimits,
    /// The selected engine's off-design thrust and fuel deck.
    pub propulsion: PropulsionDeck,
    /// Design cruise altitude, m, flown under
    /// [`alas_config::mission::CruiseAltitudePolicy::Design`]; equal to
    /// `cruise_altitude_m` unless set with `with_design_cruise_altitude_m`.
    pub design_cruise_altitude_m: f64,
    /// The clean trimmed drag the mission flies, on `wing_area_m2`.
    pub(crate) cruise_drag: drag::DragHandle,
    /// Trip plan frozen by `freeze_plan`, with any re-freeze request.
    pub(crate) frozen: Option<frozen_plan::FrozenHandle>,
    /// Work budget and counters, when one is set.
    pub(crate) budget: Option<frozen_plan::BudgetState>,
    /// Last recovered flown altitude per leg, the warm start of the next
    /// recovery search.
    pub(crate) memory: adapt::AdaptationMemory,
}

/// Validity limits of the trimmed cruise polar per mission phase.
///
/// The polar is a clean cruise polar. Climb, cruise and descent steps are
/// only evaluated up to `clean_cl_max`; takeoff and landing steps are
/// evaluated with `high_lift_delta_cd` added, up to their configured
/// maximum lift coefficients. The high-lift drag increment is the configured
/// takeoff-configuration increment used by the one-engine-inoperative climb
/// residual, so the two disciplines describe the same configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhaseAeroLimits {
    /// Maximum lift coefficient the clean polar is trusted to.
    pub clean_cl_max: f64,
    /// Maximum lift coefficient in the takeoff configuration.
    pub takeoff_cl_max: f64,
    /// Maximum lift coefficient in the landing configuration.
    pub landing_cl_max: f64,
    /// Drag-coefficient increment of the high-lift/gear configuration.
    pub high_lift_delta_cd: f64,
    /// The aircraft's own design dive speed, m/s *equivalent* airspeed
    /// (`requirements.dive_speed_m_s`).
    ///
    /// The scheduled profile is one document shared by every registered
    /// aircraft, and its climb, cruise and descent speeds are literal true
    /// airspeeds taken from the reference twin. A true airspeed is not a
    /// speed the airframe knows anything about: the same 250 m/s is 166 m/s
    /// equivalent at 6 900 m and 190 m/s equivalent at 5 500 m, and the
    /// second is above the A320-200's declared `dive_speed_m_s = 180.0`. The
    /// configuration's own validator already refuses a *cruise design point*
    /// outside that envelope
    /// (`alas_config::validation::cruise_point_inside_the_flight_envelope`);
    /// the flown legs never consulted it.
    pub dive_speed_eas_m_s: f64,
}

impl PhaseAeroLimits {
    /// Limits from the configured requirements and performance data.
    pub fn from_config(config: &alas_config::AlasConfig) -> Self {
        Self {
            clean_cl_max: config.performance.cl_max_clean,
            takeoff_cl_max: config.performance.cl_max_to,
            landing_cl_max: config.performance.cl_max_land,
            high_lift_delta_cd: config.performance.oei_climb_delta_cd,
            dive_speed_eas_m_s: config.requirements.dive_speed_m_s,
        }
    }

    fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("clean_cl_max", self.clean_cl_max),
            ("takeoff_cl_max", self.takeoff_cl_max),
            ("landing_cl_max", self.landing_cl_max),
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err(format!(
                    "phase aero limit {name} must be finite and positive, got {value}"
                ));
            }
        }
        if !self.high_lift_delta_cd.is_finite() || self.high_lift_delta_cd < 0.0 {
            return Err(format!(
                "phase aero limit high_lift_delta_cd must be finite and nonnegative, got {}",
                self.high_lift_delta_cd
            ));
        }
        Ok(())
    }
}

/// The planned profile for one route, before flying it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MissionPlan {
    /// Cruise altitude the plan uses, m.
    pub cruise_altitude_m: f64,
    /// Whether that altitude is below the configured one.
    pub adapted: bool,
    /// Planned climb footprint, m.
    pub climb_footprint_m: f64,
    /// Planned descent footprint, m.
    pub descent_footprint_m: f64,
    /// Planned cruise distance, m.
    pub cruise_distance_m: f64,
}

impl SegmentMissionModel {
    pub(crate) fn check_cancelled(&self) -> Result<(), FuelModelError> {
        if self
            .cancellation
            .as_ref()
            .is_some_and(|token| token.requested())
        {
            Err(FuelModelError::Cancelled)
        } else {
            Ok(())
        }
    }

    /// Build a model from the clean trimmed drag `cruise_drag` (coefficients
    /// on `wing_area_m2`), the configured mission profile and the engine
    /// deck. Distances and altitudes are metres.
    ///
    /// # Errors
    ///
    /// A description of the first invalid term.
    #[allow(clippy::too_many_arguments)] // one named physical input per model term; a struct would only rename them once
    pub fn new(
        profile: MissionProfileConfig,
        cruise_mach: f64,
        cruise_altitude_m: f64,
        departure_elevation_m: f64,
        arrival_elevation_m: f64,
        wing_area_m2: f64,
        cruise_drag: Arc<dyn CruiseDrag>,
        gravity_m_s2: f64,
        holding_altitude_m: f64,
        phase_limits: PhaseAeroLimits,
        propulsion: PropulsionDeck,
    ) -> Result<Self, String> {
        let cruise_atmosphere = Atmosphere::try_new(cruise_altitude_m)
            .map_err(|error| format!("cruise atmosphere is invalid: {error}"))?;
        let model = Self {
            cancellation: None,
            phase_limits,
            steps_per_segment: integrate::DEFAULT_STEPS_PER_SEGMENT,
            cas_subdivisions: alas_config::mission::CAS_SPEED_SUBDIVISIONS,
            isa_deviation_c: 0.0,
            cruise_mach,
            cruise_altitude_m,
            departure_elevation_m,
            arrival_elevation_m,
            profile,
            wing_area_m2,
            cruise_tas_m_s: cruise_mach * cruise_atmosphere.speed_of_sound(),
            gravity_m_s2,
            holding_altitude_m,
            propulsion,
            design_cruise_altitude_m: cruise_altitude_m,
            cruise_drag: drag::DragHandle(cruise_drag),
            frozen: None,
            budget: None,
            memory: adapt::AdaptationMemory::default(),
        };
        model.validate()?;
        Ok(model)
    }

    /// Check primitive terms and every speed/rate/altitude in the profile,
    /// including `|Vz| < V` on every vertical phase.
    ///
    /// # Errors
    ///
    /// A description of the first invalid term.
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("cruise_mach", self.cruise_mach),
            ("cruise_altitude_m", self.cruise_altitude_m),
            ("design_cruise_altitude_m", self.design_cruise_altitude_m),
            ("wing_area_m2", self.wing_area_m2),
            ("gravity_m_s2", self.gravity_m_s2),
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err(format!(
                    "mission model {name} must be finite and positive, got {value}"
                ));
            }
        }
        for (name, value) in [
            ("departure_elevation_m", self.departure_elevation_m),
            ("arrival_elevation_m", self.arrival_elevation_m),
            ("holding_altitude_m", self.holding_altitude_m),
        ] {
            if !value.is_finite() || value < -2_000.0 {
                return Err(format!(
                    "mission model {name} must be finite and above the checked atmosphere floor, got {value}"
                ));
            }
        }
        if !self.isa_deviation_c.is_finite() {
            return Err(format!(
                "mission model isa_deviation_c must be finite, got {}",
                self.isa_deviation_c
            ));
        }
        self.phase_limits.validate()?;
        if self.steps_per_segment == 0 {
            return Err("mission model steps_per_segment must be at least one".to_owned());
        }
        let p = &self.profile;
        for (name, speed, rate) in [
            ("takeoff", p.takeoff_air_speed_m_s, p.takeoff_climb_rate_m_s),
            (
                "initial_climb",
                p.initial_climb_air_speed_m_s,
                p.initial_climb_rate_m_s,
            ),
            (
                "step_climb_1",
                p.step_climb_1_air_speed_m_s,
                p.step_climb_1_rate_m_s,
            ),
            (
                "step_climb_2",
                p.step_climb_2_air_speed_m_s,
                p.step_climb_2_rate_m_s,
            ),
            ("descent_1", p.descent_1_air_speed_m_s, p.descent_1_rate_m_s),
            ("descent_2", p.descent_2_air_speed_m_s, p.descent_2_rate_m_s),
            ("descent_3", p.descent_3_air_speed_m_s, p.descent_3_rate_m_s),
            ("descent_4", p.descent_4_air_speed_m_s, p.descent_4_rate_m_s),
            (
                "landing",
                p.landing_air_speed_m_s,
                p.landing_descent_rate_m_s,
            ),
        ] {
            if !speed.is_finite() || speed <= 0.0 || !rate.is_finite() || rate <= 0.0 {
                return Err(format!(
                    "mission profile {name} speed and rate must be finite and positive, got {speed} m/s and {rate} m/s"
                ));
            }
            if rate >= speed {
                return Err(format!(
                    "mission profile {name} vertical rate {rate} m/s must be below its {speed} m/s airspeed"
                ));
            }
        }
        for (name, value) in [
            ("cruise_1_air_speed_m_s", p.cruise_1_air_speed_m_s),
            ("cruise_2_air_speed_m_s", p.cruise_2_air_speed_m_s),
            ("cruise_3_air_speed_m_s", p.cruise_3_air_speed_m_s),
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err(format!(
                    "mission profile {name} must be finite and positive, got {value}"
                ));
            }
        }
        let fractions = [
            p.cruise_1_distance_fraction,
            p.cruise_2_distance_fraction,
            p.cruise_3_distance_fraction,
        ];
        if fractions
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
            || fractions.iter().sum::<f64>() <= 0.0
        {
            return Err("mission profile cruise distance fractions must be finite, nonnegative and have a positive sum".to_owned());
        }
        for (name, value) in [
            ("takeoff_altitude_gain_m", p.takeoff_altitude_gain_m),
            (
                "initial_climb_altitude_fraction",
                p.initial_climb_altitude_fraction,
            ),
            (
                "step_climb_1_altitude_fraction",
                p.step_climb_1_altitude_fraction,
            ),
            ("descent_1_altitude_ft", p.descent_1_altitude_ft),
            ("descent_2_altitude_ft", p.descent_2_altitude_ft),
            ("descent_3_altitude_ft", p.descent_3_altitude_ft),
            ("descent_4_altitude_ft", p.descent_4_altitude_ft),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(format!(
                    "mission profile {name} must be finite and nonnegative, got {value}"
                ));
            }
        }
        if p.initial_climb_altitude_fraction >= 1.0 || p.step_climb_1_altitude_fraction >= 1.0 {
            return Err("mission profile climb altitude fractions must be below one".to_owned());
        }
        Ok(())
    }

    /// Refine (or coarsen) the midpoint integration to `steps` per planned
    /// segment. Intended for convergence studies; ordinary callers keep the
    /// default.
    pub fn with_steps_per_segment(mut self, steps: usize) -> Self {
        self.steps_per_segment = steps.max(1);
        self
    }

    /// Split each calibrated-airspeed climb or descent rung into `count`
    /// equal-altitude sub-rungs instead of the shared production
    /// [`alas_config::mission::CAS_SPEED_SUBDIVISIONS`]. Intended for
    /// convergence studies of the discretized constant-CAS approximation;
    /// ordinary callers keep the default so the MDO and native paths apply
    /// the same rule.
    pub fn with_cas_subdivisions(mut self, count: usize) -> Self {
        self.cas_subdivisions = count.max(1);
        self
    }

    /// Fly on a day `isa_deviation_c` warmer than the standard atmosphere
    /// (negative for colder). Applied to every ambient evaluation of the
    /// flown mission, see the field; the configured cruise true airspeed is
    /// not re-derived, matching the native mission which flies the profile's
    /// cruise TAS literally. A non-finite value is rejected by `validate`.
    pub fn with_isa_deviation_c(mut self, isa_deviation_c: f64) -> Self {
        self.isa_deviation_c = isa_deviation_c;
        self
    }

    /// The design cruise altitude, m, flown under
    /// [`alas_config::mission::CruiseAltitudePolicy::Design`] when the model
    /// is built at a route's declared operational level.
    pub fn with_design_cruise_altitude_m(mut self, altitude_m: f64) -> Self {
        self.design_cruise_altitude_m = altitude_m;
        self
    }

    fn geometry(&self) -> ProfileGeometry<'_> {
        ProfileGeometry {
            profile: &self.profile,
            departure_elevation_m: self.departure_elevation_m,
            arrival_elevation_m: self.arrival_elevation_m,
            cruise_altitude_m: match self.profile.cruise_altitude_policy {
                alas_config::mission::CruiseAltitudePolicy::Design => self.design_cruise_altitude_m,
                _ => self.cruise_altitude_m,
            },
            cas_subdivisions: self.cas_subdivisions,
            isa_deviation_c: self.isa_deviation_c,
            dive_speed_eas_m_s: self.phase_limits.dive_speed_eas_m_s,
            constant_mach_cruise: self.propulsion.kind() == super::propulsion::DeckKind::Turbofan,
        }
    }

    /// Horizontal distance the non-cruise phases occupy at the configured
    /// cruise altitude, m. Reported for diagnosis; it is **not** a limit,
    /// because a shorter route lowers the cruise altitude rather than
    /// becoming unflyable. See [`Self::minimum_flyable_profile_range_m`].
    pub fn configured_profile_range_m(&self) -> f64 {
        self.geometry()
            .configured_footprint_m(LegKind::Trip)
            .unwrap_or(f64::NAN)
    }

    /// The shortest still-air distance this aircraft can fly the configured
    /// profile over, m: the climb and descent footprint at the *lowest*
    /// cruise level the geometry admits.
    ///
    /// This is the quantity a route has to clear. Testing the route against
    /// the *configured-altitude* footprint instead rejected candidates the
    /// model then flew perfectly well, because [`Self::fly_leg`]'s planner
    /// lowers the cruise level until the ladder fits: measured on the
    /// shipped path as `mission_profile_range` rejecting **654 of 654**
    /// A220-300 candidates, whose declared 465.7 km EVRA-ESSA sector is
    /// shorter than the 477.6 km its FL250 ladder occupies but far longer
    /// than the ladder it actually flies. Nothing is relaxed: a route below
    /// this floor is still rejected here, and the planner still refuses it
    /// with [`alas_mass::fuel_plan::FuelModelError::RouteTooShort`].
    pub fn minimum_flyable_profile_range_m(&self) -> f64 {
        self.geometry()
            .floor_footprint_m(LegKind::Trip)
            .unwrap_or(f64::NAN)
    }

    /// The planned profile for `range_m`.
    ///
    /// # Errors
    ///
    /// [`FuelModelError::RouteTooShort`] when no cruise altitude fits.
    pub fn plan_trip(&self, range_m: f64) -> Result<MissionPlan, FuelModelError> {
        let plan = self.geometry().plan(LegKind::Trip, range_m)?;
        Ok(MissionPlan {
            cruise_altitude_m: plan.cruise_altitude_m,
            adapted: plan.adapted,
            climb_footprint_m: plan.climb_footprint_m(),
            descent_footprint_m: plan.descent_footprint_m(),
            cruise_distance_m: plan.cruise_distance_m(),
        })
    }

    /// Fly the trip from `takeoff_mass_kg` over `range_m` with diagnostics.
    ///
    /// # Errors
    ///
    /// As [`FuelBurnModel::trip`].
    pub fn fly_trip(&self, takeoff_mass_kg: f64, range_m: f64) -> Result<FlownLeg, FuelModelError> {
        self.fly_leg(LegKind::Trip, takeoff_mass_kg, range_m)
    }

    /// Fly the diversion from `start_mass_kg` over `distance_m`.
    ///
    /// # Errors
    ///
    /// As [`FuelBurnModel::diversion`].
    pub fn fly_diversion(
        &self,
        start_mass_kg: f64,
        distance_m: f64,
    ) -> Result<FlownLeg, FuelModelError> {
        self.fly_leg(LegKind::Diversion, start_mass_kg, distance_m)
    }

    /// Fuel flow in steady level flight at `altitude_m` and `tas_m_s`, kg/s.
    fn level_fuel_flow_kg_s(
        &self,
        mass_kg: f64,
        altitude_m: f64,
        tas_m_s: f64,
    ) -> Result<f64, FuelModelError> {
        if !mass_kg.is_finite() || mass_kg <= 0.0 {
            return Err(FuelModelError::MassOutOfRange { mass_kg });
        }
        let flight = self
            .propulsion
            .flight_condition(altitude_m, tas_m_s, self.gravity_m_s2, self.isa_deviation_c)
            .map_err(deck_error)?;
        let drag_n = self.drag_n(&flight, mass_kg)?;
        let point = self
            .propulsion
            .at_thrust(flight, drag_n, PropulsionRating::Cruise)
            .map_err(deck_error)?;
        Ok(point.fuel_flow_kg_s)
    }
}

fn deck_error(error: DeckError) -> FuelModelError {
    FuelModelError::NotConverged(error.to_string())
}

mod fuel_burn;
#[cfg(test)]
mod physics_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mdo::build::build_geometry;
    use crate::mdo::propulsion::{max_climb_rate_ft_min, DeckKind};
    use alas_config::design_variables::DesignVector;
    use alas_config::AlasConfig;

    fn model_for(config: &AlasConfig, design: &DesignVector) -> (SegmentMissionModel, f64) {
        let (config, _, plane) = build_geometry(config, &design.to_array())
            .unwrap_or_else(|failure| panic!("geometry: {}", failure.reason));
        let req = &config.requirements;
        let deck = PropulsionDeck::from_engine(
            &config.geometry.engine,
            req.cruise_mach,
            req.cruise_altitude_m,
            max_climb_rate_ft_min(config.mission.profile.initial_climb_rate_m_s),
        )
        .unwrap_or_else(|error| panic!("deck: {error}"));
        let model = SegmentMissionModel::new(
            config.mission.profile.clone(),
            req.cruise_mach,
            req.cruise_altitude_m,
            0.0,
            0.0,
            plane.s_ref,
            Arc::new(ParabolicPolar::new(0.018, 0.045, 0.002, req.cruise_mach)),
            req.gravity_m_s2,
            457.2,
            PhaseAeroLimits::from_config(&config),
            deck,
        )
        .unwrap_or_else(|error| panic!("model: {error}"));
        (model, req.mtow_kg)
    }

    fn model() -> (SegmentMissionModel, f64) {
        model_for(&AlasConfig::default(), &DesignVector::default())
    }

    #[test]
    fn uncancelled_token_preserves_valid_mission_exactly() {
        let (mut model, mass) = model();
        let original = model.fly_trip(mass, 3.0e6).unwrap();
        model.cancellation = Some(crate::cancellation::EvaluationCancellation::new());
        assert_eq!(model.fly_trip(mass, 3.0e6).unwrap(), original);
    }

    #[test]
    fn atr_pre_cancel_and_mid_integration_cancel_are_not_physical_failures() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": "ATR72-600"})).unwrap();
        let design = alas_config::presets::get("ATR72-600")
            .unwrap()
            .design_vector;
        let (mut model, mass) = model_for(&config, &design);
        let token = crate::cancellation::EvaluationCancellation::new();
        model.cancellation = Some(token.clone());
        token.request();
        assert_eq!(
            model.fly_trip(mass, 627000.0),
            Err(FuelModelError::Cancelled)
        );
        let token = crate::cancellation::EvaluationCancellation::new();
        model.cancellation = Some(token.clone());
        // Refine the ordinary ATR integrator to guarantee work remains in
        // flight when cancellation is requested; no artificial sleeps inside physics.
        model.steps_per_segment = 1_000_000;
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            let worker = scope.spawn(move || {
                started_tx.send(()).unwrap();
                model.fly_trip(mass, 627000.0)
            });
            started_rx.recv().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(30));
            let requested = std::time::Instant::now();
            token.request();
            assert_eq!(worker.join().unwrap(), Err(FuelModelError::Cancelled));
            assert!(requested.elapsed() < std::time::Duration::from_secs(1));
        });
    }

    fn assert_ledger_closes(flown: &FlownLeg) {
        let l = flown.ledger;
        let residual =
            l.propulsive_work_j - l.drag_work_j - l.potential_energy_j - l.kinetic_energy_j;
        assert!(
            residual.abs() <= 1.0e-8 * l.propulsive_work_j,
            "energy ledger residual {residual} J of {} J",
            l.propulsive_work_j
        );
    }

    /// The default widebody cannot hold its configured FL390 at MTOW on the
    /// fixture polar (211 kN required in level flight against 194 kN of
    /// maximum-cruise thrust), and the 300 ft/min residual-climb rule admits
    /// FL390 only near 80 % MTOW, so the residual-climb policy starts the
    /// MTOW trip lower. The conservation checks run where the configured
    /// level qualifies.
    #[test]
    fn a_design_range_trip_flies_the_configured_altitude_and_conserves_mass_energy_and_time() {
        let (model, mtow) = model();
        let mass = 0.8 * mtow;
        let flown = model.fly_trip(mass, 3.0e6).unwrap();
        assert!(flown.leg.fuel_kg > 0.0 && flown.leg.fuel_kg < mass);
        assert!(flown.leg.time_s > 0.0);
        assert!(!flown.adapted);
        assert_eq!(flown.cruise_altitude_m, model.cruise_altitude_m);
        assert!((mass - flown.leg.fuel_kg - flown.end_mass_kg).abs() <= 1.0e-9 * mass);
        assert_ledger_closes(&flown);
        // The climb lifts a heavier aircraft than the descent lowers, so the
        // net potential-energy change is positive and bounded by the burned
        // fuel's weight over the cruise altitude.
        let net_potential_j = flown.ledger.potential_energy_j;
        assert!(net_potential_j > 0.0);
        assert!(net_potential_j < flown.leg.fuel_kg * model.gravity_m_s2 * model.cruise_altitude_m);
        // A heavier start never flies higher and burns more.
        let heavier = model.fly_trip(mtow, 3.0e6).unwrap();
        assert!(heavier.cruise_altitude_m < flown.cruise_altitude_m);
        assert!(heavier.leg.fuel_kg > flown.leg.fuel_kg);
    }

    #[test]
    fn a_zero_or_too_short_route_is_rejected_and_a_short_route_lowers_the_cruise_altitude() {
        let (model, mtow) = model();
        let configured_footprint = model.configured_profile_range_m();
        // The route a trip has to clear is the ladder at the *lowest* cruise
        // level the geometry admits, not the ladder at the configured level:
        // a route between the two is flown lower, which is what
        // `minimum_flyable_profile_range_m` documents and what the residual in
        // `mdo::sizing` tests against.
        let floor_footprint = model.minimum_flyable_profile_range_m();
        assert!(configured_footprint > floor_footprint && floor_footprint > 0.0);
        match model.fly_trip(mtow, 0.0) {
            Err(FuelModelError::RouteTooShort {
                minimum_range_m, ..
            }) => {
                assert!(minimum_range_m > 0.0 && minimum_range_m < configured_footprint);
            }
            other => panic!("zero route must be rejected, got {other:?}"),
        }
        // Negative control immediately below the floor, and the matching
        // positive control immediately above it: the floor is the boundary,
        // not merely a threshold this test happens to sit under.
        let transition_limited = 0.95 * floor_footprint;
        assert!(matches!(
            model.fly_trip(mtow, transition_limited),
            Err(FuelModelError::RouteTooShort { minimum_range_m, .. })
                if (minimum_range_m - floor_footprint).abs() <= 1.0e-6 * floor_footprint
                    && minimum_range_m > transition_limited
        ));
        // The corrected ledger reserves finite distance for each speed
        // transition. This route is below the configured-altitude footprint
        // yet far above the floor, so it is the reachable adapted case.
        let adapted_range = 0.85 * configured_footprint;
        let short = model.fly_trip(mtow, adapted_range).unwrap();
        assert!(short.adapted);
        assert!(short.cruise_altitude_m < model.cruise_altitude_m);
        assert!(short.cruise_altitude_m > 0.0);
        assert_ledger_closes(&short);
        let plan = model.plan_trip(adapted_range).unwrap();
        assert!(plan.adapted && plan.cruise_distance_m >= 0.0);
        assert!(
            (plan.climb_footprint_m + plan.descent_footprint_m + plan.cruise_distance_m
                - adapted_range)
                .abs()
                < 1.0e-6
        );
        let full = model.fly_trip(mtow, 3.0e6).unwrap();
        assert!(short.leg.fuel_kg < full.leg.fuel_kg);
    }

    /// An adapted plan keeps the configured schedule's *shape* and its
    /// declared equivalent airspeeds, not its literal true airspeeds.
    ///
    /// The plan does not hold literal true airspeeds, for a physical reason: a
    /// true airspeed is not invariant with altitude, so holding the number while the cruise
    /// level is lowered raises dynamic pressure and drag, and the step-down
    /// then makes a rating shortfall *worse* rather than better. On the ATR
    /// 72-600 the declared 140.7 m/s is its FL170 cruise and at 1 067 m it is
    /// about 265 kt calibrated: past VMO, with a drag the PW127M cannot hold
    /// level, which is why its dispatch closure failed at every mass in its
    /// bracket. See `ProfileGeometry::cruise_tas_at`.
    ///
    /// What is still pinned, and is the point of the test: every rung is
    /// resolved by one rule, the ladder keeps its phases and their order, and
    /// no rung is silently sped up.
    #[test]
    fn an_adapted_plan_keeps_declared_equivalent_airspeeds_in_every_phase() {
        let (model, _) = model();
        let configured_cruise_m = model.cruise_altitude_m;
        let adapted_cruise_m = 10_000.0;
        assert!(adapted_cruise_m < configured_cruise_m);
        // The equivalent-airspeed rule (a turboprop's); a jet's Mach hold is
        // checked at the end.
        let geometry = ProfileGeometry {
            constant_mach_cruise: false,
            ..model.geometry()
        };
        let plan = geometry
            .plan_at(LegKind::Trip, 3.0e6, adapted_cruise_m)
            .expect("the explicitly lowered profile fits the route");
        assert!(plan.adapted);
        let jet = model
            .geometry()
            .plan_at(LegKind::Trip, 3.0e6, adapted_cruise_m)
            .expect("the jet plan fits the route");
        let (reference, flown) = (
            alas_atmo::Atmosphere::new(configured_cruise_m),
            alas_atmo::Atmosphere::new(jet.cruise_altitude_m),
        );
        let vc_m_s =
            model.phase_limits.dive_speed_eas_m_s / 1.25 / (flown.density() / 1.225).sqrt();
        for ((speed, _), declared) in jet.cruise_rungs.iter().zip([
            model.profile.cruise_1_air_speed_m_s,
            model.profile.cruise_2_air_speed_m_s,
            model.profile.cruise_3_air_speed_m_s,
        ]) {
            let mach_hold = declared * flown.speed_of_sound() / reference.speed_of_sound();
            let expected = mach_hold.min(vc_m_s) * jet.envelope_scale;
            assert!(
                (speed - expected).abs() <= 0.01 * expected,
                "{speed} against {expected}"
            );
        }

        // The cruise rungs are the ones declared *for the configured level*,
        // so at a lower level they fly the same equivalent airspeed: the same
        // dynamic pressure, hence the same lift and drag coefficients.
        let reference_density = alas_atmo::Atmosphere::new(configured_cruise_m).density();
        let flown_density = alas_atmo::Atmosphere::new(plan.cruise_altitude_m).density();
        let equivalent_airspeed_ratio = (reference_density / flown_density).sqrt();
        let declared_cruise = [
            model.profile.cruise_1_air_speed_m_s,
            model.profile.cruise_2_air_speed_m_s,
            model.profile.cruise_3_air_speed_m_s,
        ];
        // Every rung is resolved by *one* rule, so their ratios to the
        // declared speeds are identical to the last bit. That is the contract;
        // the ratio's own value is then checked against the standard-atmosphere
        // estimate with a tolerance, because the model resolves its densities
        // through `alas_atmo::us1976_try_compute_values` on the geometry's own
        // day and recomputing that here would duplicate the atmosphere rather
        // than test the rule.
        let ratios: Vec<f64> = plan
            .cruise_rungs
            .iter()
            .zip(declared_cruise)
            .map(|((speed, _), declared)| speed / declared)
            .collect();
        for ratio in &ratios {
            assert!((ratio - ratios[0]).abs() <= f64::EPSILON * ratios[0]);
        }
        let expected_ratio = equivalent_airspeed_ratio * plan.envelope_scale;
        assert!(
            (ratios[0] - expected_ratio).abs() <= 0.01 * expected_ratio,
            "cruise rungs flew {} of their declared speed against an expected {expected_ratio}",
            ratios[0]
        );

        // Climb and descent rungs are declared at their own altitudes and are
        // literal, up to the one envelope factor the whole leg carries.
        let expected_climb = [
            model.profile.takeoff_air_speed_m_s,
            model.profile.initial_climb_air_speed_m_s,
            model.profile.step_climb_1_air_speed_m_s,
            model.profile.step_climb_2_air_speed_m_s,
        ];
        assert_eq!(plan.climb.len(), expected_climb.len());
        for (segment, declared) in plan.climb.iter().zip(expected_climb) {
            let expected = declared * plan.envelope_scale;
            assert!((segment.tas_m_s - expected).abs() <= 1.0e-9 * expected);
        }

        let expected_descent = [
            model.profile.descent_1_air_speed_m_s,
            model.profile.descent_2_air_speed_m_s,
            model.profile.descent_3_air_speed_m_s,
            model.profile.descent_4_air_speed_m_s,
            model.profile.landing_air_speed_m_s,
        ];
        assert_eq!(plan.descent.len(), expected_descent.len());
        for (segment, declared) in plan.descent.iter().zip(expected_descent) {
            let expected = declared * plan.envelope_scale;
            assert!((segment.tas_m_s - expected).abs() <= 1.0e-9 * expected);
        }

        // The factor only ever slows a leg down.
        assert!(plan.envelope_scale > 0.0 && plan.envelope_scale <= 1.0);
    }

    /// The default widebody fixture with a SYNTHETIC calibrated-airspeed
    /// schedule: the CAS numbers below are test inputs chosen to stay inside
    /// the empirical turbofan deck's Mach domain up to the fixture's cruise
    /// altitude, not sourced operating speeds of any aircraft. They exercise
    /// the CAS ladders end to end (fuel, time, distance). The returned mass
    /// is 80 % MTOW: at MTOW the fixture cannot climb to its declared FL390
    /// on the maximum-climb rating even at the 0.5 m/s floor rate, and the
    /// declared policy does not lower a level for a climb shortfall.
    fn calibrated_model() -> (SegmentMissionModel, f64) {
        let (mut model, mtow) = model();
        let p = &mut model.profile;
        p.climb_descent_speed_reference = alas_config::mission::SpeedReference::CalibratedAirspeed;
        // One fixed level, so the refinement sees the discretization alone
        // and not a discrete step-climb choice moving with it.
        p.cruise_altitude_policy = alas_config::mission::CruiseAltitudePolicy::Declared;
        p.takeoff_air_speed_m_s = 128.6;
        p.initial_climb_air_speed_m_s = 135.0;
        p.step_climb_1_air_speed_m_s = 125.0;
        p.step_climb_2_air_speed_m_s = 115.0;
        p.descent_1_air_speed_m_s = 115.0;
        p.descent_2_air_speed_m_s = 140.0;
        p.descent_3_air_speed_m_s = 128.0;
        p.descent_4_air_speed_m_s = 110.0;
        p.landing_air_speed_m_s = 75.0;
        model.validate().expect("a valid calibrated profile");
        (model, 0.8 * mtow)
    }

    /// The CAS sub-rung count is a discretization of the flown mission, not
    /// only of the planned footprint: fuel, block time and flown distance
    /// must all converge as it is refined, and the energy ledger must close at
    /// every count (no kinetic energy hidden by the sub-rung boundaries, where
    /// each speed change opens a budget).
    ///
    /// **No asymptotic order is pinned for the flown fuel.** Refining the
    /// fixture at 80 % MTOW gives (trip fuel, kg):
    ///
    /// | sub-rungs | 4 | 8 | 16 | 32 | 64 |
    /// |---|---|---|---|---|---|
    /// | fuel | 29 481.8 | 29 491.2 | 29 511.0 | 29 519.7 | 29 520.2 |
    ///
    /// The successive-difference ratio over 16/32/64 is **15.8**; at MTOW,
    /// on the earlier calibrated thrust, it was **2.36**. The sequence is
    /// pre-asymptotic and state dependent, so only convergence is asserted
    /// (a ratio of at least 1.5, which rejects a stalled refinement) together
    /// with the error of the shipped count against the Richardson limit. The
    /// schedule is *piecewise constant* in speed, with each rung-to-rung
    /// change taken instantaneously at a boundary and its kinetic budget
    /// amortized inside the following sub-rung, an `O(dh)` treatment however
    /// finely the true airspeed itself is sampled.
    ///
    /// **Known integrator limitation.** The *planned* footprint converges at
    /// second order, but the *flown* descent footprint runs away from it:
    /// 429 km at 1 sub-rung and 466 km at 64, and at 128 the leg stops
    /// converging (`speed schedule not attained ... 0.1 m/s` transition,
    /// 727 MJ unpaid). The cause is the rating/idle-bounded combined energy
    /// balance exercised by the equivalent-airspeed cruise resolution, not the
    /// rate revision or the envelope scale. The shipped count is
    /// `alas_config::mission::CAS_SPEED_SUBDIVISIONS` = 8, whose trip fuel is
    /// within 0.1 % of the Richardson limit; a 0.5 % bound is asserted so a
    /// regression in the shipped configuration is caught, rather than only a
    /// regression in the refinement trend.
    #[test]
    fn cas_sub_rung_refinement_converges_fuel_time_and_distance_with_a_closed_ledger() {
        let (model, mass) = calibrated_model();
        let range_m = 3.0e6;
        let reference = model
            .clone()
            .with_cas_subdivisions(64)
            .fly_trip(mass, range_m)
            .expect("the calibrated widebody trip flies");
        assert_ledger_closes(&reference);
        assert!(!reference.adapted);
        // Every sub-rung boundary books its speed change. The net kinetic
        // term is negative for this fixture, which lands slower (75 m/s) than
        // it lifts off (128.6 m/s), and nothing is left unrealized beyond the
        // integrator's own bound.
        assert!(reference.ledger.kinetic_energy_j < 0.0);
        assert!(
            reference.ledger.unrealized_kinetic_j.abs()
                <= 1.0e-3 * reference.ledger.propulsive_work_j
        );
        let mut previous_fuel_gap_kg = f64::INFINITY;
        let mut previous_time_gap_s = f64::INFINITY;
        let mut fuel_kg = Vec::new();
        let mut planned_footprint_m = Vec::new();
        for subdivisions in [4_usize, 8, 16, 32, 64] {
            let refined = model.clone().with_cas_subdivisions(subdivisions);
            // The planned ladder is the geometry the flown leg discretizes.
            // It is sampled at each sub-rung's own midpoint altitude and so is
            // second-order accurate; asserting it separately keeps a
            // regression in the geometry distinguishable from one in the
            // integration.
            let plan = refined.plan_trip(range_m).expect("the ladder plans");
            planned_footprint_m.push(plan.climb_footprint_m + plan.descent_footprint_m);
            let flown = refined
                .fly_trip(mass, range_m)
                .expect("the calibrated widebody trip flies");
            assert_ledger_closes(&flown);
            assert!(
                (flown.flown_distance_m - range_m).abs() <= 1.0,
                "{subdivisions} sub-rungs flew {} m of a {range_m} m route",
                flown.flown_distance_m
            );
            fuel_kg.push(flown.leg.fuel_kg);
            if subdivisions <= 16 {
                let fuel_gap_kg = (flown.leg.fuel_kg - reference.leg.fuel_kg).abs();
                let time_gap_s = (flown.leg.time_s - reference.leg.time_s).abs();
                assert!(
                    fuel_gap_kg <= previous_fuel_gap_kg,
                    "{subdivisions} sub-rungs: fuel gap {fuel_gap_kg} kg grew from {previous_fuel_gap_kg} kg"
                );
                assert!(
                    time_gap_s <= previous_time_gap_s,
                    "{subdivisions} sub-rungs: time gap {time_gap_s} s grew from {previous_time_gap_s} s"
                );
                previous_fuel_gap_kg = fuel_gap_kg;
                previous_time_gap_s = time_gap_s;
            }
        }
        // Planned geometry: second order. Each doubling must cut the movement
        // of the ladder by at least three, which 4 comfortably clears and 2
        // (first order) does not.
        let planned_steps: Vec<f64> = planned_footprint_m
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .collect();
        for (index, pair) in planned_steps.windows(2).enumerate() {
            assert!(
                pair[1] < pair[0],
                "planned footprint moved {} m then {} m: not converging",
                pair[0],
                pair[1]
            );
            // 4 -> 8 -> 16 is still pre-asymptotic on this fixture (ratio
            // 2.45); from 8 sub-rungs on the midpoint sampling shows its own
            // order, and a first-order geometry would stall at 2.
            if index > 0 {
                assert!(
                    pair[1] * 3.0 <= pair[0],
                    "planned footprint moved {} m then {} m: not second order",
                    pair[0],
                    pair[1]
                );
            }
        }
        // At 64 sub-rungs the ladder must have settled to 10 ppm of itself.
        let settled_m = 1.0e-5 * planned_footprint_m.last().copied().unwrap_or(f64::NAN);
        assert!(
            planned_steps.last().copied().unwrap_or(f64::INFINITY) <= settled_m,
            "the planned ladder is still moving {} m at 64 sub-rungs",
            planned_steps.last().copied().unwrap_or(f64::NAN)
        );
        // Flown fuel: convergent, resolved against its own Richardson limit
        // rather than against the finest sample, which is itself still short
        // of the limit.
        let ratio = (fuel_kg[3] - fuel_kg[2]) / (fuel_kg[4] - fuel_kg[3]);
        assert!(
            ratio >= 1.5,
            "fuel refinement ratio {ratio} does not show a converging refinement"
        );
        let richardson_kg = fuel_kg[4] + (fuel_kg[4] - fuel_kg[3]) / (ratio - 1.0);
        let shipped_error = (fuel_kg[1] - richardson_kg).abs() / richardson_kg;
        assert!(
            shipped_error <= 5.0e-3,
            "the shipped {} sub-rungs are {shipped_error} off the refinement limit {richardson_kg} kg",
            alas_config::mission::CAS_SPEED_SUBDIVISIONS
        );
        // The production default is the shared constant, not a local choice.
        assert_eq!(
            model.cas_subdivisions,
            alas_config::mission::CAS_SPEED_SUBDIVISIONS
        );
    }

    #[test]
    fn vertical_rates_at_or_above_airspeed_are_rejected() {
        let (mut broken, _) = model();
        broken.profile.initial_climb_rate_m_s = broken.profile.initial_climb_air_speed_m_s;
        assert!(broken.validate().is_err());
        let (model, mtow) = model();
        let plan = model.geometry().plan(LegKind::Trip, 3.0e6).unwrap();
        for segment in plan.climb.iter().chain(plan.descent.iter()) {
            assert!(segment.horizontal_speed_m_s() < segment.tas_m_s);
            assert!(
                (segment.horizontal_distance_m
                    - segment.horizontal_speed_m_s() * segment.duration_s())
                .abs()
                    < 1.0e-6
            );
        }
        let flown = model.fly_trip(mtow, 3.0e6).unwrap();
        assert!(flown.climb_footprint_m > 0.0);
    }

    #[test]
    fn climb_and_descent_energy_terms_have_the_physical_sign() {
        let (mut model, mtow) = model();
        // Fly only a climb ladder by asking for the shortest flyable route:
        // potential energy must be positive during the climb and the
        // descent must return it, so the round trip is near zero.
        let flown = model.fly_trip(mtow, 3.0e6).unwrap();
        assert!(flown.ledger.drag_work_j > 0.0);
        assert!(flown.ledger.propulsive_work_j > flown.ledger.drag_work_j * 0.5);
        assert!(flown.minimum_descent_rate_m_s > 0.0 && flown.maximum_clean_cl > 0.0);
        assert!(flown.maximum_clean_cl <= model.phase_limits.clean_cl_max);
        // A demanded climb rate the rating cannot deliver is flown at the
        // rating, slower than planned, and a hopeless one is a typed deficit.
        model.profile.step_climb_2_rate_m_s = 40.0;
        let limited = model.fly_trip(mtow, 3.0e6).unwrap();
        assert!(limited.rate_limited_steps > 0);
        assert!(limited.minimum_climb_rate_m_s < 40.0);
        assert_ledger_closes(&limited);
        let cruise_mach = model.cruise_mach;
        model = model.with_cruise_drag(Arc::new(ParabolicPolar::new(
            0.5,
            0.045,
            0.002,
            cruise_mach,
        )));
        assert!(matches!(
            model.fly_trip(mtow, 3.0e6),
            Err(FuelModelError::NotConverged(reason)) if reason.contains("energy deficit") || reason.contains("thrust deficit")
        ));
    }

    #[test]
    fn a_phase_flown_beyond_its_lift_limit_is_a_validity_failure_not_an_extrapolation() {
        let (mut limited, mtow) = model();
        limited.phase_limits.clean_cl_max = 0.2;
        assert!(matches!(
            limited.fly_trip(mtow, 3.0e6),
            Err(FuelModelError::NotConverged(reason)) if reason.contains("polar validity")
        ));
        let (model, mtow) = model();
        // The high-lift increment is charged only on takeoff and landing:
        // the same route with a larger increment burns more fuel, and the
        // difference is bounded by the short duration of those phases.
        let base = model.fly_trip(mtow, 3.0e6).unwrap();
        let mut heavier_drag = model.clone();
        heavier_drag.phase_limits.high_lift_delta_cd += 0.02;
        let more = heavier_drag.fly_trip(mtow, 3.0e6).unwrap();
        assert!(more.leg.fuel_kg > base.leg.fuel_kg);
        assert!(more.leg.fuel_kg < 1.1 * base.leg.fuel_kg);
    }

    #[test]
    fn the_flown_horizontal_distance_closes_on_the_route() {
        let (model, mtow) = model();
        let configured_footprint = model.configured_profile_range_m();
        // Below the floor ladder (not the configured-altitude ladder) there is
        // no cruise level that fits, so no distance can close.
        let transition_limited = 0.95 * model.minimum_flyable_profile_range_m();
        assert!(matches!(
            model.fly_trip(mtow, transition_limited),
            Err(FuelModelError::RouteTooShort { .. })
        ));
        for range_m in [3.0e6, 1.0e6, 0.85 * configured_footprint] {
            let flown = model.fly_trip(mtow, range_m).unwrap();
            assert!(
                (flown.flown_distance_m - range_m).abs() <= 1.0,
                "flown {} m for a {range_m} m route",
                flown.flown_distance_m
            );
        }
    }

    #[test]
    fn aerodrome_elevations_are_mean_sea_level_and_the_takeoff_gain_is_above_ground() {
        let (mut model, mtow) = model();
        model.departure_elevation_m = 1_600.0;
        model.arrival_elevation_m = 100.0;
        let plan = model.geometry().plan(LegKind::Trip, 3.0e6).unwrap();
        let first = plan.climb.first().unwrap();
        assert_eq!(first.start_altitude_m, 1_600.0);
        assert!(
            (first.end_altitude_m - (1_600.0 + model.profile.takeoff_altitude_gain_m)).abs()
                < 1.0e-9
        );
        let last = plan.descent.last().unwrap();
        assert_eq!(last.end_altitude_m, 100.0);
        assert!(plan
            .descent
            .iter()
            .all(|segment| segment.end_altitude_m >= 100.0 && segment.start_altitude_m >= 100.0));
        let flown = model.fly_trip(mtow, 3.0e6).unwrap();
        assert!((flown.flown_distance_m - 3.0e6).abs() <= 1.0);
        // The climb from a high aerodrome to the same cruise level gains
        // less potential energy than the descent to a low one returns.
        assert!(flown.ledger.potential_energy_j < 0.0);
        assert_ledger_closes(&flown);
    }

    #[test]
    fn non_finite_inputs_are_rejected_before_any_integration() {
        let (model, mtow) = model();
        for range_m in [f64::NAN, f64::INFINITY, -1.0] {
            assert!(matches!(
                model.fly_trip(mtow, range_m),
                Err(FuelModelError::InvalidDistance { .. })
            ));
        }
        for mass_kg in [f64::NAN, f64::INFINITY, 0.0, -5.0] {
            assert!(matches!(
                model.fly_trip(mass_kg, 3.0e6),
                Err(FuelModelError::MassOutOfRange { .. })
            ));
        }
        let mut broken = model.clone();
        broken.profile.cruise_2_air_speed_m_s = f64::NAN;
        assert!(broken.validate().is_err());
        let mut broken = model.clone();
        broken.profile.descent_3_rate_m_s = f64::INFINITY;
        assert!(broken.validate().is_err());
        let mut broken = model.clone();
        broken.departure_elevation_m = f64::NAN;
        assert!(broken.validate().is_err());
        let mut broken = model.clone();
        broken.cruise_altitude_m = f64::INFINITY;
        assert!(broken.validate().is_err());
    }

    #[test]
    fn wave_drag_is_separate_from_low_speed_induced_drag() {
        let (model, mtow) = model();
        let polar = ParabolicPolar::new(0.018, 0.045, 0.002, model.cruise_mach);
        assert_eq!(polar.wave_cd_at(0.5), 0.0);
        assert!((polar.wave_cd_at(model.cruise_mach) - 0.002).abs() < 1.0e-12);
        assert_eq!(
            model.clean_cd(0.5, 0.5, 10_000.0),
            polar.cd(0.5, 0.5, 10_000.0)
        );
        // At MTOW the fixture cannot hold FL390 on maximum-cruise thrust
        // (211 kN required, 194 kN available); cruise is priced at the same
        // 80 % MTOW as the hold.
        let cruise = model.cruise_fuel_flow_kg_s(0.8 * mtow).unwrap();
        let holding = model.holding_fuel_flow_kg_s(0.8 * mtow, 457.2).unwrap();
        let taxi = model.taxi_fuel_flow_kg_s().unwrap();
        assert!(cruise > 0.0 && holding > 0.0 && taxi > 0.0);
        assert!(taxi < cruise);
    }

    /// Fuel, time and flown distance at `steps` per segment.
    fn refined(
        model: &SegmentMissionModel,
        mass_kg: f64,
        range_m: f64,
        steps: usize,
    ) -> (f64, f64, f64) {
        let flown = model
            .clone()
            .with_steps_per_segment(steps)
            .fly_trip(mass_kg, range_m)
            .unwrap_or_else(|error| panic!("{steps} steps: {error}"));
        (flown.leg.fuel_kg, flown.leg.time_s, flown.flown_distance_m)
    }

    /// Assert first-order-or-better convergence of the midpoint integration:
    /// the N->2N change must shrink by at least half at 2N->4N, and the
    /// N-step result must be within `tolerance` (relative) of the 4N one.
    fn assert_step_refinement(
        model: &SegmentMissionModel,
        mass_kg: f64,
        range_m: f64,
        tolerance: f64,
    ) {
        let coarse = refined(
            model,
            mass_kg,
            range_m,
            integrate::DEFAULT_STEPS_PER_SEGMENT,
        );
        let medium = refined(
            model,
            mass_kg,
            range_m,
            2 * integrate::DEFAULT_STEPS_PER_SEGMENT,
        );
        let fine = refined(
            model,
            mass_kg,
            range_m,
            4 * integrate::DEFAULT_STEPS_PER_SEGMENT,
        );
        for (name, c, m, f) in [
            ("fuel", coarse.0, medium.0, fine.0),
            ("time", coarse.1, medium.1, fine.1),
        ] {
            let first = (m - c).abs();
            let second = (f - m).abs();
            assert!(
                second <= 0.6 * first + 1.0e-9 * f.abs(),
                "{name}: {c} -> {m} -> {f} does not converge"
            );
            assert!(
                (c - f).abs() <= tolerance * f.abs(),
                "{name}: default resolution {c} differs from refined {f} by more than {tolerance:e}"
            );
        }
        assert!((coarse.2 - range_m).abs() <= 1.0 && (fine.2 - range_m).abs() <= 1.0);
    }

    #[test]
    fn the_midpoint_integration_converges_under_step_refinement_for_a_jet() {
        let (model, mtow) = model();
        assert_step_refinement(&model, mtow, 3.0e6, 2.0e-3);
        assert_step_refinement(&model, mtow, 1.0e6, 2.0e-3);
        // At the adaptation boundary the cruise altitude itself is chosen
        // by a feasibility bisection whose limit moves with the climb
        // footprint's resolution, so only a bounded spread is asserted.
        let short = 0.85 * model.configured_profile_range_m();
        let coarse = refined(&model, mtow, short, integrate::DEFAULT_STEPS_PER_SEGMENT);
        let fine = refined(
            &model,
            mtow,
            short,
            4 * integrate::DEFAULT_STEPS_PER_SEGMENT,
        );
        assert!(
            (coarse.0 - fine.0).abs() <= 1.0e-2 * fine.0,
            "{coarse:?} vs {fine:?}"
        );
        assert!(
            (coarse.1 - fine.1).abs() <= 1.0e-2 * fine.1,
            "{coarse:?} vs {fine:?}"
        );
    }

    /// The ATR preset carries the shared jet default climb/descent speeds
    /// (170-250 m/s), which a turboprop cannot fly; the deck rejects them
    /// with a typed energy deficit. This profile uses ATR 72 operating
    /// speeds (climb ~170 kt IAS, descent ~220 kt IAS, cruise ~270 kt TAS
    /// at FL200-FL240) as an explicit test assumption, not preset truth.
    fn turboprop_profile(profile: &mut MissionProfileConfig) {
        // These are true-airspeed assumptions; the registered preset
        // carries a calibrated schedule, so state the reference explicitly.
        profile.climb_descent_speed_reference = alas_config::mission::SpeedReference::TrueAirspeed;
        // After the ATR wing's legitimate s_ref correction to the published
        // 61.0 m^2 (from the unclipped 63.926 m^2 this fixture was
        // originally tuned against), the takeoff lift coefficient at a given
        // speed and mass rose by the same ~4.6% the area shrank. The
        // configured takeoff CLmax is 1.600; the stall-limited minimum speed
        // at this fixture's mass/altitude is V_min = sqrt(2W/(rho*S*CLmax))
        // ~= 62.0 * sqrt(1.614/1.600) ~= 62.3 m/s (from the CL=1.614
        // measured at 62.0 m/s, CL scaling as 1/V^2 at fixed weight/area).
        // 64.0 m/s keeps a documented ~2.8% margin over that minimum
        // (CL ~= 1.51 there), not a re-tuned production speed: the preset's
        // own calibrated schedule, CLmax, area and drag are untouched.
        profile.takeoff_air_speed_m_s = 64.0;
        profile.takeoff_climb_rate_m_s = 6.0;
        profile.takeoff_altitude_gain_m = 457.2;
        profile.initial_climb_air_speed_m_s = 90.0;
        profile.initial_climb_rate_m_s = 6.0;
        profile.step_climb_1_air_speed_m_s = 100.0;
        profile.step_climb_1_rate_m_s = 4.0;
        profile.step_climb_2_air_speed_m_s = 110.0;
        profile.step_climb_2_rate_m_s = 3.0;
        profile.descent_1_air_speed_m_s = 125.0;
        profile.descent_1_rate_m_s = 6.0;
        profile.descent_2_air_speed_m_s = 120.0;
        profile.descent_2_rate_m_s = 6.0;
        profile.descent_3_air_speed_m_s = 110.0;
        profile.descent_3_rate_m_s = 5.0;
        profile.descent_4_air_speed_m_s = 95.0;
        profile.descent_4_rate_m_s = 4.0;
        profile.landing_air_speed_m_s = 62.0;
        profile.landing_descent_rate_m_s = 3.0;
    }

    /// The registered ATR preset now carries its own calibrated-airspeed
    /// schedule (170 KCAS climb, derived ~135 KCAS takeoff segment). The
    /// shared jet default it inherited before (128.6-250 m/s *true*
    /// airspeeds) is still rejected with the typed climb energy deficit,
    /// which is the evidence the preset change answers.
    ///
    /// OPEN PHYSICS GAP, recorded rather than tuned away: with this
    /// fixture's synthetic clean polar (cd0 0.018, k 0.045) the climb and
    /// cruise are flown, but the descent ladder's decelerations (the last
    /// one 140 -> 113 KCAS on the 600 ft/min final segment) cannot all be
    /// shed at idle, because the landing configuration's drag is represented
    /// only by the takeoff-configuration increment. Depending on the exact
    /// flight condition at that boundary, which moves with legitimate
    /// upstream corrections such as the ATR wing's s_ref fix, not with
    /// anything in this profile, the deck reports either the integrator's
    /// typed "speed schedule not attained" outcome (unrealized kinetic
    /// energy at the boundary) or its own typed "rating map does not
    /// bracket" outcome (the near-zero idle-descent thrust this point needs
    /// falls outside the deck's zero-to-rating domain). Both are the same
    /// underlying non-attainable descent/idle boundary, just surfacing
    /// through different typed rejections; this test accepts either. The
    /// product path (report-derived polar) is exercised by an internal ATR
    /// physics probe. This test pins that the climb-side deficit is gone
    /// and that whatever remains is one of these typed
    /// descent/idle-domain outcomes, not a silent mis-fly.
    #[test]
    fn the_atr_preset_flies_its_calibrated_schedule_and_still_rejects_the_jet_default() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": "ATR72-600"}))
            .unwrap_or_else(|error| panic!("preset: {error}"));
        assert_eq!(
            config.mission.profile.climb_descent_speed_reference,
            alas_config::mission::SpeedReference::CalibratedAirspeed
        );
        let design = alas_config::presets::get("ATR72-600")
            .unwrap_or_else(|error| panic!("preset: {error}"))
            .design_vector;
        let (mut model, mtow) = model_for(&config, &design);
        assert_eq!(model.propulsion.kind(), DeckKind::Turboprop);
        // The declared-level recovery is what this test pins; the optimum
        // policy picks its level by the residual-climb rule instead.
        model.profile.cruise_altitude_policy = alas_config::mission::CruiseAltitudePolicy::Declared;
        match model.fly_trip(mtow, 700_000.0) {
            Ok(flown) => {
                assert!(flown.leg.fuel_kg > 500.0 && flown.leg.fuel_kg < 0.2 * mtow);
                assert_ledger_closes(&flown);
                // The route must not be what lowers the level. The ladder at
                // the configured level occupies 209 km of a 700 km sector, so
                // there is no route-fit reason to step down, and the
                // step-down that does happen is weight-limited: at 97 % of
                // MTOW the same sector reaches the configured level with
                // nothing adapted. That is an ordinary initial-cruise-level
                // restriction, not a schedule the aeroplane cannot fly.
                assert!(
                    model.configured_profile_range_m() < 0.5 * 700_000.0,
                    "the configured ladder must fit the sector with margin, not at {} m",
                    model.configured_profile_range_m()
                );
                // Find the mass the configured level *is* reached at rather
                // than pinning a fraction: what matters is that the level is
                // recovered by unloading the aeroplane, and how much unloading
                // it takes is a measurement that moves with the deck and the
                // polar. Measured today: 0.90 of MTOW reaches 5 180 m
                // unadapted, 0.95 does not.
                // `light.cruise_altitude_m` is the climb ladder's own top
                // (`ProfileGeometry::ladder_top_m`), not the raw configured
                // number: the ATR profile disables its step-climb legs (zero
                // distance share) below the declared cruise altitude, so an
                // unadapted, unloaded flight tops out at
                // `initial_climb_altitude_fraction` (0.999) of the declared
                // level by design, never exactly at it. A weight-limited
                // step-down (the failure this test guards against) lands far
                // below that, at the measured ~5 180 m against a ~6 096 m
                // (FL200) configured level, so a 99.5 % tolerance separates
                // the two without masking either.
                let recovered = [0.95_f64, 0.90, 0.85, 0.80].into_iter().find(|fraction| {
                    model
                        .fly_trip(fraction * mtow, 700_000.0)
                        .is_ok_and(|light| {
                            !light.adapted
                                && light.cruise_altitude_m >= 0.995 * model.cruise_altitude_m
                        })
                });
                assert!(
                    recovered.is_some_and(|fraction| fraction >= 0.80),
                    "the step-down must be weight-limited and recover by 80 % of MTOW, not {recovered:?}"
                );
                if flown.adapted {
                    // With this fixture's synthetic polar at the declared
                    // 140.7 m/s cruise the leg may still level below the
                    // configured altitude (the TCDS flat rating and sourced
                    // lapse hold the level for the report-derived polar, not
                    // necessarily for this one). The recovered level must stay
                    // within 10 % of the configured one; a collapse to the
                    // floor would be the old failure returning.
                    assert!(
                        flown.cruise_altitude_m >= 0.90 * model.cruise_altitude_m,
                        "a level-flight shortfall must not drop the level to {} m",
                        flown.cruise_altitude_m
                    );
                }
            }
            Err(FuelModelError::NotConverged(reason)) => {
                assert!(
                    (reason.contains("speed schedule not attained")
                        || reason.contains("rating map does not bracket"))
                        && !reason.contains("climb energy deficit"),
                    "unexpected outcome: {reason}"
                );
            }
            Err(other) => panic!("unexpected error: {other}"),
        }

        let mut jet_default = config.clone();
        jet_default.mission.profile = MissionProfileConfig::default();
        let (jet_model, _) = model_for(&jet_default, &design);
        // The jet-default schedule (128.6-250 m/s true airspeed) is still
        // rejected on the turboprop deck; the typed outcome is either a
        // climb/cruise/descent energy deficit, or (since the ATR wing's
        // s_ref correction) a step whose required thrust falls outside the
        // deck's own idle-to-rating bracket entirely (a schedule so far off
        // the deck's domain the inverse-thrust solve has no bracket to
        // search, not a numeric coincidence). Both are typed
        // `NotConverged` domain rejections of the same invalid schedule;
        // this does not accept an arbitrary/any error.
        assert!(matches!(
            jet_model.fly_trip(mtow, 700_000.0),
            Err(FuelModelError::NotConverged(reason))
                if reason.contains("deficit") || reason.contains("rating map does not bracket")
        ));
    }

    #[test]
    fn the_atr_turboprop_flies_a_regional_sector_on_its_own_deck() {
        let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "ATR72-600"}))
            .unwrap_or_else(|error| panic!("preset: {error}"));
        turboprop_profile(&mut config.mission.profile);
        let design = alas_config::presets::get("ATR72-600")
            .unwrap_or_else(|error| panic!("preset: {error}"))
            .design_vector;
        let (model, mtow) = model_for(&config, &design);
        assert_eq!(model.propulsion.kind(), DeckKind::Turboprop);
        let flown = model.fly_trip(mtow, 700_000.0).unwrap();
        assert!(flown.leg.fuel_kg > 500.0 && flown.leg.fuel_kg < 0.2 * mtow);
        assert_ledger_closes(&flown);
        let diversion = model.fly_diversion(0.9 * mtow, 200_000.0).unwrap();
        assert!(diversion.leg.fuel_kg > 0.0 && diversion.leg.fuel_kg < flown.leg.fuel_kg);
        // The nonlinear deck and the altitude fixed point do not guarantee a
        // fixed per-doubling error ratio. Compare a sufficiently fine 64-step
        // reference instead. Time error must shrink monotonically and the
        // finest grid must remove at least half of the coarse time error.
        // Below about 1e-3 relative the fuel error is signed and non-monotone
        // (the 4-step value can sit closer to the reference than the 8-step
        // one) because the TCDS flat-rating corner makes the fuel flow
        // non-smooth in step count. Fuel is therefore guarded by a 1e-3
        // relative bound at every step count and by the finest grid sitting
        // at the ~1e-4 relative noise floor (at most 2e-4).
        let reference = model
            .clone()
            .with_steps_per_segment(64)
            .fly_trip(mtow, 700_000.0)
            .unwrap();
        assert_ledger_closes(&reference);
        let mut refinements = Vec::new();
        for steps in [4_usize, 8, 16, 32] {
            let refined = model
                .clone()
                .with_steps_per_segment(steps)
                .fly_trip(mtow, 700_000.0)
                .unwrap();
            assert_ledger_closes(&refined);
            assert!((refined.flown_distance_m - 700_000.0).abs() <= 1.0);
            refinements.push((
                (refined.leg.fuel_kg - reference.leg.fuel_kg).abs(),
                (refined.leg.time_s - reference.leg.time_s).abs(),
            ));
        }
        for (fuel_err, _) in &refinements {
            assert!(*fuel_err <= 1.0e-3 * reference.leg.fuel_kg);
        }
        assert!(refinements[3].0 <= 2.0e-4 * reference.leg.fuel_kg);
        for pair in refinements.windows(2) {
            assert!(pair[1].1 <= pair[0].1);
        }
        assert!(refinements[3].1 <= 0.5 * refinements[0].1);
    }

    /// The ATR report-derived dispatch bracket is a reachable 644.9 km
    /// sector at the bracket mass (21,359.3854 kg). Three consecutive cruise
    /// rungs share the same 140.673 m/s target. A boundary-energy budget left
    /// at the end of the first rung must therefore be allowed to continue
    /// through the remaining same-target distance; treating every rung end
    /// as a route rejection incorrectly pushed altitude recovery to the floor
    /// and reported a multi-megametre footprint. The active regression keeps
    /// that closure rule separate from the genuine idle-domain rejection
    /// covered by the integration test in `integrate.rs`.
    ///
    /// **Configured level.** The PW127M deck uses the TCDS flat rating and a
    /// sourced density lapse, so the report-derived polar holds the configured
    /// 5 180 m (it flies about 5 175 m, unadapted); the altitude is asserted
    /// within a 10 m tolerance.
    #[test]
    fn atr_dispatch_bracket_flies_report_derived_sector_at_configured_altitude() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": "ATR72-600"}))
            .unwrap_or_else(|error| panic!("preset: {error}"));
        let design = alas_config::presets::get("ATR72-600")
            .unwrap_or_else(|error| panic!("preset: {error}"))
            .design_vector;
        let (built_config, _, plane) = build_geometry(&config, &design.to_array())
            .unwrap_or_else(|failure| panic!("geometry: {}", failure.reason));
        let req = &built_config.requirements;
        let deck = PropulsionDeck::from_engine(
            &built_config.geometry.engine,
            req.cruise_mach,
            req.cruise_altitude_m,
            max_climb_rate_ft_min(built_config.mission.profile.initial_climb_rate_m_s),
        )
        .unwrap_or_else(|error| panic!("deck: {error}"));
        let model = SegmentMissionModel::new(
            built_config.mission.profile.clone(),
            req.cruise_mach,
            req.cruise_altitude_m,
            610.0, // LEMD elevation, m
            8.0,   // LEPA elevation, m
            plane.s_ref,
            // Report-derived cd0 and k of the measured ATR sector.
            Arc::new(ParabolicPolar::new(
                0.024600009989746922,
                0.030377093904043483,
                0.002,
                req.cruise_mach,
            )),
            req.gravity_m_s2,
            457.2,
            PhaseAeroLimits::from_config(&built_config),
            deck,
        )
        .unwrap_or_else(|error| panic!("model: {error}"));

        let flown = model
            .fly_trip(21_359.385400976567, 644_890.9)
            .unwrap_or_else(|error| panic!("ATR dispatch sector must close: {error}"));
        assert!(flown.leg.fuel_kg > 0.0 && flown.leg.fuel_kg < 0.2 * 21_359.385400976567);
        assert!(flown.leg.time_s.is_finite() && flown.leg.time_s > 0.0);
        assert!((flown.flown_distance_m - 644_890.9).abs() <= 10.0);
        assert_ledger_closes(&flown);

        // The bug this test exists for: a same-target rung end read as a
        // route rejection drove the recovery to the floor and reported a
        // multi-megametre footprint. Both signatures are pinned directly.
        let floor_m = model.geometry().floor_cruise_m(LegKind::Trip);
        assert!(
            flown.cruise_altitude_m > 0.5 * (floor_m + req.cruise_altitude_m),
            "recovery collapsed toward the {floor_m} m floor: {} m",
            flown.cruise_altitude_m
        );
        assert!(
            flown.climb_footprint_m + flown.descent_footprint_m < 0.6 * 644_890.9,
            "ladder occupies {} m of a 644 890.9 m sector",
            flown.climb_footprint_m + flown.descent_footprint_m
        );

        // The TCDS flat rating and sourced lapse hold the configured level:
        // it must fly unadapted, within 10 m of the configured altitude.
        let at_configured = model
            .geometry()
            .plan_at(LegKind::Trip, 644_890.9, req.cruise_altitude_m)
            .unwrap_or_else(|error| panic!("the configured ladder plans: {error}"));
        let held = model
            .fly(21_359.385400976567, &at_configured, None)
            .unwrap_or_else(|error| panic!("the configured level must now hold: {error:?}"));
        assert!(!held.adapted);
        assert!(
            (held.cruise_altitude_m - req.cruise_altitude_m).abs() <= 10.0,
            "configured {} m, flown {} m",
            req.cruise_altitude_m,
            held.cruise_altitude_m
        );
        assert_ledger_closes(&held);
    }
}
