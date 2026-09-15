// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Vertical-profile geometry: segments with consistent kinematics and the
//! cruise-altitude adaptation that keeps a short route from being overflown.
//!
//! Every climbing or descending segment flies at a true airspeed `V` along
//! the flight path with a planned vertical speed `Vz`; its ground speed in
//! still air is `sqrt(V^2 - Vz^2)`, so `|Vz| < V` is a hard requirement of
//! the configuration, not a rounding detail. The climb and descent ladders
//! of a trip occupy a horizontal footprint; when the requested distance is
//! shorter than that footprint at the configured cruise altitude, the cruise
//! altitude is lowered continuously until the footprint fits, and a route
//! shorter than the footprint at the lowest usable altitude is rejected with
//! [`FuelModelError::RouteTooShort`].

use alas_atmo::{us1976_try_compute_values, Us1976Values};
use alas_config::mission::{MissionProfileConfig, SpeedReference};
use alas_mass::fuel_plan::FuelModelError;
use alas_units::FOOT;

/// Altitude-bisection resolution, m.
const ALTITUDE_RESOLUTION_M: f64 = 0.5;
/// Bisection budget on the cruise altitude.
const ALTITUDE_BISECTION_ITERATIONS: usize = 60;
/// Vertical segments shorter than this are dropped, m.
const MINIMUM_SEGMENT_ALTITUDE_M: f64 = 1.0e-6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SegmentKind {
    Takeoff,
    Climb,
    Cruise,
    Descent,
    Landing,
}

/// One planned segment. Speeds are true airspeeds along the path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Segment {
    pub kind: SegmentKind,
    pub start_altitude_m: f64,
    pub end_altitude_m: f64,
    /// True airspeed along the flight path, m/s.
    pub tas_m_s: f64,
    /// Signed planned vertical speed, m/s; zero for level segments.
    pub vertical_rate_m_s: f64,
    /// Still-air horizontal distance, m.
    pub horizontal_distance_m: f64,
}

impl Segment {
    /// Horizontal speed `sqrt(V^2 - Vz^2)`, m/s.
    pub fn horizontal_speed_m_s(&self) -> f64 {
        (self.tas_m_s * self.tas_m_s - self.vertical_rate_m_s * self.vertical_rate_m_s).sqrt()
    }

    /// Planned duration, s.
    pub fn duration_s(&self) -> f64 {
        if self.vertical_rate_m_s != 0.0 {
            (self.end_altitude_m - self.start_altitude_m).abs() / self.vertical_rate_m_s.abs()
        } else {
            self.horizontal_distance_m / self.tas_m_s
        }
    }

    /// A climbing or descending segment from `start_m` to `end_m` at
    /// `tas_m_s` and vertical-speed magnitude `rate_m_s`; `None` when the
    /// altitude change is negligible.
    pub fn vertical(
        kind: SegmentKind,
        start_m: f64,
        end_m: f64,
        tas_m_s: f64,
        rate_m_s: f64,
    ) -> Result<Option<Self>, FuelModelError> {
        let delta_m = end_m - start_m;
        if delta_m.abs() <= MINIMUM_SEGMENT_ALTITUDE_M {
            return Ok(None);
        }
        if rate_m_s.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)
            || tas_m_s.partial_cmp(&rate_m_s) != Some(std::cmp::Ordering::Greater)
        {
            return Err(FuelModelError::InvalidModel(format!(
                "{kind:?} vertical rate {rate_m_s} m/s must be positive and below the {tas_m_s} m/s airspeed"
            )));
        }
        let vertical_rate_m_s = rate_m_s.copysign(delta_m);
        let segment = Self {
            kind,
            start_altitude_m: start_m,
            end_altitude_m: end_m,
            tas_m_s,
            vertical_rate_m_s,
            horizontal_distance_m: 0.0,
        };
        Ok(Some(Self {
            horizontal_distance_m: segment.horizontal_speed_m_s() * segment.duration_s(),
            ..segment
        }))
    }

    /// A level segment at `altitude_m` over `distance_m`.
    pub fn level(kind: SegmentKind, altitude_m: f64, tas_m_s: f64, distance_m: f64) -> Self {
        Self {
            kind,
            start_altitude_m: altitude_m,
            end_altitude_m: altitude_m,
            tas_m_s,
            vertical_rate_m_s: 0.0,
            horizontal_distance_m: distance_m,
        }
    }
}

/// A complete plan at one cruise altitude.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProfilePlan {
    /// Cruise altitude the plan flies at, m.
    pub cruise_altitude_m: f64,
    /// Whether the cruise altitude was lowered below the configured one.
    pub adapted: bool,
    /// Climb ladder, in order, ending at the cruise altitude.
    pub climb: Vec<Segment>,
    /// Cruise rungs `(true airspeed, distance fraction)`, in order.
    pub cruise_rungs: Vec<(f64, f64)>,
    /// Descent ladder, in order, ending at the arrival elevation.
    pub descent: Vec<Segment>,
    /// Requested still-air distance, m.
    pub range_m: f64,
}

impl ProfilePlan {
    /// Planned climb footprint, m.
    pub fn climb_footprint_m(&self) -> f64 {
        self.climb.iter().map(|s| s.horizontal_distance_m).sum()
    }

    /// Planned descent footprint, m.
    pub fn descent_footprint_m(&self) -> f64 {
        self.descent.iter().map(|s| s.horizontal_distance_m).sum()
    }

    /// Planned cruise distance, m.
    pub fn cruise_distance_m(&self) -> f64 {
        self.range_m - self.climb_footprint_m() - self.descent_footprint_m()
    }
}

/// Which leg a plan describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegKind {
    /// Departure to destination with the full configured ladders.
    Trip,
    /// Missed approach at the destination followed by the alternate leg.
    ///
    /// The reduced mission geometry currently has no separate alternate-field
    /// elevation.  It therefore uses the trip arrival elevation as both the
    /// go-around departure reference and the diversion landing reference,
    /// while still flying the configured climb and descent tiers.
    Diversion,
}

/// The configured profile, elevations and cruise altitude.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProfileGeometry<'a> {
    pub profile: &'a MissionProfileConfig,
    pub departure_elevation_m: f64,
    pub arrival_elevation_m: f64,
    pub cruise_altitude_m: f64,
    /// Equal-altitude sub-rungs each calibrated-airspeed climb or descent
    /// rung is split into. Production callers pass
    /// [`alas_config::mission::CAS_SPEED_SUBDIVISIONS`]; convergence studies pass more.
    pub cas_subdivisions: usize,
    /// Temperature deviation from the standard atmosphere, degrees C, applied
    /// to every ambient state the ladders are resolved against (the pressure
    /// and temperature a calibrated airspeed is converted at). The same
    /// single value the native schedule applies to every segment (the
    /// departure airport's deviation).
    pub isa_deviation_c: f64,
}

impl ProfileGeometry<'_> {
    /// Ambient state at `altitude_m` on this geometry's day.
    fn ambient(&self, altitude_m: f64, what: &str) -> Result<Us1976Values, FuelModelError> {
        us1976_try_compute_values(altitude_m, self.isa_deviation_c).map_err(|error| {
            FuelModelError::InvalidModel(format!("{what} atmosphere at {altitude_m} m: {error}"))
        })
    }

    /// Cruise altitude the trip configuration asks for, never below either
    /// trip aerodrome.
    #[cfg(test)]
    pub fn configured_cruise_m(&self) -> f64 {
        self.configured_cruise_m_for(LegKind::Trip)
    }

    /// Cruise altitude the selected leg configuration asks for, m.
    ///
    /// A diversion starts at the trip arrival field.  Keeping its reference
    /// elevations explicit avoids using the original departure field when a
    /// route is rebuilt after a missed approach.
    fn configured_cruise_m_for(&self, leg: LegKind) -> f64 {
        let (departure, arrival) = match leg {
            LegKind::Trip => (self.departure_elevation_m, self.arrival_elevation_m),
            LegKind::Diversion => (self.arrival_elevation_m, self.arrival_elevation_m),
        };
        self.cruise_altitude_m.max(departure).max(arrival)
    }

    /// A climbing or descending leg from `start_m` to `end_m` flown at
    /// constant calibrated airspeed `cas_m_s`: `subdivisions` segments, each
    /// with true airspeed resolved from `cas_m_s` at its own midpoint
    /// altitude's real ambient pressure and temperature: a *discretized*
    /// approximation of constant CAS (each sub-rung still flies one constant
    /// true airspeed), not the continuous exact quantity; see
    /// [`ProfileGeometry::plan`]'s callers for the refinement test showing
    /// how the approximation error shrinks as `subdivisions` grows. Every
    /// production call site uses [`alas_config::mission::CAS_SPEED_SUBDIVISIONS`],
    /// the same count the native pseudospectral schedule
    /// (`alas-pipeline::mission_stage::schedule`) uses, so both paths apply
    /// an identical discretization rule rather than two independently chosen
    /// ones. Unlike the true-airspeed rungs, this needs no
    /// equivalent-airspeed adaptation trick to stay physically correct when
    /// the cruise altitude is lowered: the real atmosphere at whatever band
    /// is actually flown is what is sampled.
    fn cas_vertical_segments(
        &self,
        kind: SegmentKind,
        start_m: f64,
        end_m: f64,
        cas_m_s: f64,
        rate_m_s: f64,
        subdivisions: usize,
    ) -> Result<Vec<Segment>, FuelModelError> {
        if (end_m - start_m).abs() <= MINIMUM_SEGMENT_ALTITUDE_M {
            return Ok(Vec::new());
        }
        let subdivisions = subdivisions.max(1);
        let mut segments = Vec::with_capacity(subdivisions);
        let step_m = (end_m - start_m) / subdivisions as f64;
        let mut altitude_m = start_m;
        for _ in 0..subdivisions {
            let next_m = altitude_m + step_m;
            let mid_m = 0.5 * (altitude_m + next_m);
            let atmosphere = self.ambient(mid_m, "calibrated-airspeed")?;
            let tas_m_s = alas_atmo::airspeed::true_from_calibrated(
                cas_m_s,
                atmosphere.pressure_pa,
                atmosphere.temperature_k,
            )
            .map_err(|error| {
                FuelModelError::InvalidModel(format!(
                    "{kind:?} calibrated airspeed {cas_m_s} m/s at {mid_m} m: {error}"
                ))
            })?;
            segments.extend(Segment::vertical(
                kind, altitude_m, next_m, tas_m_s, rate_m_s,
            )?);
            altitude_m = next_m;
        }
        Ok(segments)
    }

    /// Lowest cruise altitude the ladders can be built at, m.
    pub fn floor_cruise_m(&self, leg: LegKind) -> f64 {
        // Both legs open with the configured takeoff band (the diversion
        // is a go-around from the arrival field, see `diversion_ladders`)
        // so neither may cruise below the top of that band. A floor at the
        // field elevation itself let a short alternate distance plan the
        // diversion cruise at 8 m above sea level at the literal cruise true
        // airspeed, which no propulsion deck sustains.
        let start = match leg {
            LegKind::Trip => self.departure_elevation_m,
            LegKind::Diversion => self.arrival_elevation_m,
        } + self.profile.takeoff_altitude_gain_m;
        start.max(self.arrival_elevation_m) + MINIMUM_SEGMENT_ALTITUDE_M
    }

    /// Climb-rung altitude bands `(takeoff top, initial top, step-one top)`
    /// for a cruise at `cruise_m`.
    fn climb_bands_from(&self, departure_m: f64, cruise_m: f64) -> (f64, f64, f64) {
        let p = self.profile;
        let takeoff_top = (departure_m + p.takeoff_altitude_gain_m).min(cruise_m);
        let initial_top = (cruise_m * p.initial_climb_altitude_fraction)
            .max(takeoff_top)
            .min(cruise_m);
        let step_one_top = (cruise_m * p.step_climb_1_altitude_fraction)
            .max(initial_top)
            .min(cruise_m);
        (takeoff_top, initial_top, step_one_top)
    }

    fn climb_ladder_from(
        &self,
        departure_m: f64,
        cruise_m: f64,
    ) -> Result<Vec<Segment>, FuelModelError> {
        let p = self.profile;
        let mut segments = Vec::with_capacity(4 * self.cas_subdivisions);
        let departure = departure_m;
        let (takeoff_top, initial_top, step_one_top) = self.climb_bands_from(departure_m, cruise_m);
        for (kind, start, end, configured_speed, rate) in [
            (
                SegmentKind::Takeoff,
                departure,
                takeoff_top,
                p.takeoff_air_speed_m_s,
                p.takeoff_climb_rate_m_s,
            ),
            (
                SegmentKind::Climb,
                takeoff_top,
                initial_top,
                p.initial_climb_air_speed_m_s,
                p.initial_climb_rate_m_s,
            ),
            (
                SegmentKind::Climb,
                initial_top,
                step_one_top,
                p.step_climb_1_air_speed_m_s,
                p.step_climb_1_rate_m_s,
            ),
            (
                SegmentKind::Climb,
                step_one_top,
                cruise_m,
                p.step_climb_2_air_speed_m_s,
                p.step_climb_2_rate_m_s,
            ),
        ] {
            match p.climb_descent_speed_reference {
                SpeedReference::TrueAirspeed => {
                    // A true-airspeed profile is literal, including when a
                    // short-route fit lowers the cruise altitude. The native
                    // schedule has the same contract; no implicit EAS
                    // rescaling is allowed here.
                    segments.extend(Segment::vertical(kind, start, end, configured_speed, rate)?);
                }
                SpeedReference::CalibratedAirspeed => {
                    segments.extend(self.cas_vertical_segments(
                        kind,
                        start,
                        end,
                        configured_speed,
                        rate,
                        self.cas_subdivisions,
                    )?);
                }
            }
        }
        Ok(segments)
    }

    fn climb_ladder(&self, cruise_m: f64) -> Result<Vec<Segment>, FuelModelError> {
        self.climb_ladder_from(self.departure_elevation_m, cruise_m)
    }

    fn descent_ladder_to(
        &self,
        cruise_m: f64,
        arrival_m: f64,
    ) -> Result<Vec<Segment>, FuelModelError> {
        let p = self.profile;
        let mut segments = Vec::with_capacity(5 * self.cas_subdivisions);
        // Every configured true airspeed remains literal when the cruise
        // altitude is adapted. Calibrated legs resolve their configured CAS
        // against the atmosphere at each live sub-rung instead.
        let mut altitude = cruise_m;
        for (target_ft, speed, rate) in [
            (
                p.descent_1_altitude_ft,
                p.descent_1_air_speed_m_s,
                p.descent_1_rate_m_s,
            ),
            (
                p.descent_2_altitude_ft,
                p.descent_2_air_speed_m_s,
                p.descent_2_rate_m_s,
            ),
            (
                p.descent_3_altitude_ft,
                p.descent_3_air_speed_m_s,
                p.descent_3_rate_m_s,
            ),
            (
                p.descent_4_altitude_ft,
                p.descent_4_air_speed_m_s,
                p.descent_4_rate_m_s,
            ),
        ] {
            let target = (target_ft * FOOT).max(arrival_m).min(altitude);
            match p.climb_descent_speed_reference {
                SpeedReference::TrueAirspeed => segments.extend(Segment::vertical(
                    SegmentKind::Descent,
                    altitude,
                    target,
                    speed,
                    rate,
                )?),
                SpeedReference::CalibratedAirspeed => {
                    segments.extend(self.cas_vertical_segments(
                        SegmentKind::Descent,
                        altitude,
                        target,
                        speed,
                        rate,
                        self.cas_subdivisions,
                    )?);
                }
            }
            altitude = target;
        }
        match p.climb_descent_speed_reference {
            SpeedReference::TrueAirspeed => {
                segments.extend(Segment::vertical(
                    SegmentKind::Landing,
                    altitude,
                    arrival_m,
                    p.landing_air_speed_m_s,
                    p.landing_descent_rate_m_s,
                )?);
            }
            SpeedReference::CalibratedAirspeed => {
                segments.extend(self.cas_vertical_segments(
                    SegmentKind::Landing,
                    altitude,
                    arrival_m,
                    p.landing_air_speed_m_s,
                    p.landing_descent_rate_m_s,
                    self.cas_subdivisions,
                )?);
            }
        }
        Ok(segments)
    }

    fn descent_ladder(&self, cruise_m: f64) -> Result<Vec<Segment>, FuelModelError> {
        self.descent_ladder_to(cruise_m, self.arrival_elevation_m)
    }

    fn diversion_ladders(
        &self,
        cruise_m: f64,
    ) -> Result<(Vec<Segment>, Vec<Segment>), FuelModelError> {
        // A missed approach is a go-around from the trip arrival field.  Use
        // the same configured takeoff/initial/step climb tiers as a trip,
        // then use the complete descent ladder and its low-speed landing
        // segment to the current model's alternate-field reference.  The
        // latter is the trip arrival elevation until the model gains a
        // separate alternate-elevation input.
        let climb = self.climb_ladder_from(self.arrival_elevation_m, cruise_m)?;
        let descent = self.descent_ladder_to(cruise_m, self.arrival_elevation_m)?;
        Ok((climb, descent))
    }

    /// The plan at an explicit cruise altitude, without adaptation.
    ///
    /// # Errors
    ///
    /// [`FuelModelError::RouteTooShort`] when the ladders do not fit in
    /// `range_m`; [`FuelModelError::InvalidModel`] for unusable kinematics.
    pub fn plan_at(
        &self,
        leg: LegKind,
        range_m: f64,
        cruise_m: f64,
    ) -> Result<ProfilePlan, FuelModelError> {
        // Cruise fields are literal true airspeeds. A shortened route may
        // lower the altitude, but that geometry adaptation must not silently
        // change the configured speed; this is the legacy/native contract.
        let (climb, descent, cruise_rungs) = match leg {
            LegKind::Trip => {
                let p = self.profile;
                (
                    self.climb_ladder(cruise_m)?,
                    self.descent_ladder(cruise_m)?,
                    vec![
                        (p.cruise_1_air_speed_m_s, p.cruise_1_distance_fraction),
                        (p.cruise_2_air_speed_m_s, p.cruise_2_distance_fraction),
                        (p.cruise_3_air_speed_m_s, p.cruise_3_distance_fraction),
                    ],
                )
            }
            LegKind::Diversion => {
                let (climb, descent) = self.diversion_ladders(cruise_m)?;
                (
                    climb,
                    descent,
                    vec![(self.profile.cruise_1_air_speed_m_s, 1.0)],
                )
            }
        };
        let plan = ProfilePlan {
            cruise_altitude_m: cruise_m,
            adapted: cruise_m < self.configured_cruise_m_for(leg),
            climb,
            cruise_rungs,
            descent,
            range_m,
        };
        if plan.cruise_distance_m() < 0.0 {
            return Err(FuelModelError::RouteTooShort {
                range_m,
                minimum_range_m: plan.climb_footprint_m() + plan.descent_footprint_m(),
            });
        }
        Ok(plan)
    }

    /// The plan for `range_m`, lowering the cruise altitude only as far as
    /// needed for the ladders to fit.
    ///
    /// # Errors
    ///
    /// As [`Self::plan_at`]; a route shorter than the footprint at the floor
    /// altitude is [`FuelModelError::RouteTooShort`] with that footprint.
    pub fn plan(&self, leg: LegKind, range_m: f64) -> Result<ProfilePlan, FuelModelError> {
        if !range_m.is_finite() || range_m < 0.0 {
            return Err(FuelModelError::InvalidDistance {
                distance_m: range_m,
            });
        }
        let configured = self.configured_cruise_m_for(leg);
        match self.plan_at(leg, range_m, configured) {
            Err(FuelModelError::RouteTooShort { .. }) => {}
            other => return other,
        }
        let floor = self.floor_cruise_m(leg).min(configured);
        let mut feasible = self.plan_at(leg, range_m, floor)?;
        let mut low = floor;
        let mut high = configured;
        for _ in 0..ALTITUDE_BISECTION_ITERATIONS {
            if high - low <= ALTITUDE_RESOLUTION_M {
                break;
            }
            let middle = 0.5 * (low + high);
            match self.plan_at(leg, range_m, middle) {
                Ok(plan) => {
                    feasible = plan;
                    low = middle;
                }
                Err(FuelModelError::RouteTooShort { .. }) => high = middle,
                Err(error) => return Err(error),
            }
        }
        Ok(feasible)
    }

    /// Horizontal footprint of the non-cruise phases at the configured
    /// cruise altitude, m.
    pub fn configured_footprint_m(&self, leg: LegKind) -> Result<f64, FuelModelError> {
        let cruise = self.configured_cruise_m_for(leg);
        let (climb, descent) = match leg {
            LegKind::Trip => (self.climb_ladder(cruise)?, self.descent_ladder(cruise)?),
            LegKind::Diversion => self.diversion_ladders(cruise)?,
        };
        Ok(climb
            .iter()
            .chain(descent.iter())
            .map(|s| s.horizontal_distance_m)
            .sum())
    }
}

// Tests assert on `Result`s they just constructed, so a failed unwrap or
// expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::mission::CAS_SPEED_SUBDIVISIONS;
    use alas_units::NAUTICAL_MILE;

    #[test]
    fn diversion_uses_configured_climb_and_descent_tiers_from_arrival_field() {
        let profile = MissionProfileConfig::default();
        let cruise_m = 10_000.0;
        let arrival_m = 250.0;
        let geometry = ProfileGeometry {
            profile: &profile,
            // Deliberately keep the original departure field above the
            // diversion cruise altitude.  A missed approach starts at the
            // arrival field, so it must not inherit this trip-only reference.
            departure_elevation_m: 12_000.0,
            arrival_elevation_m: arrival_m,
            cruise_altitude_m: cruise_m,
            cas_subdivisions: CAS_SPEED_SUBDIVISIONS,
            isa_deviation_c: 0.0,
        };
        let plan = geometry
            .plan_at(LegKind::Diversion, 2_000_000.0, cruise_m)
            .expect("the high-altitude diversion profile fits the long test route");

        assert!(
            !plan.adapted,
            "diversion configuration uses its own arrival reference"
        );
        assert_eq!(geometry.configured_cruise_m(), 12_000.0);

        let expected_climb = [
            (SegmentKind::Takeoff, profile.takeoff_air_speed_m_s),
            (SegmentKind::Climb, profile.initial_climb_air_speed_m_s),
            (SegmentKind::Climb, profile.step_climb_1_air_speed_m_s),
            (SegmentKind::Climb, profile.step_climb_2_air_speed_m_s),
        ];
        assert_eq!(plan.climb.len(), expected_climb.len());
        assert!((plan.climb.first().unwrap().start_altitude_m - arrival_m).abs() < 1.0e-9);
        assert!((plan.climb.last().unwrap().end_altitude_m - cruise_m).abs() < 1.0e-9);
        for (segment, (kind, speed_m_s)) in plan.climb.iter().zip(expected_climb) {
            assert_eq!(segment.kind, kind);
            assert_eq!(segment.tas_m_s, speed_m_s);
        }

        let expected_descent = [
            (SegmentKind::Descent, profile.descent_1_air_speed_m_s),
            (SegmentKind::Descent, profile.descent_2_air_speed_m_s),
            (SegmentKind::Descent, profile.descent_3_air_speed_m_s),
            (SegmentKind::Descent, profile.descent_4_air_speed_m_s),
            (SegmentKind::Landing, profile.landing_air_speed_m_s),
        ];
        assert_eq!(plan.descent.len(), expected_descent.len());
        assert!((plan.descent.first().unwrap().start_altitude_m - cruise_m).abs() < 1.0e-9);
        assert!((plan.descent.last().unwrap().end_altitude_m - arrival_m).abs() < 1.0e-9);
        for (segment, (kind, speed_m_s)) in plan.descent.iter().zip(expected_descent) {
            assert_eq!(segment.kind, kind);
            assert_eq!(segment.tas_m_s, speed_m_s);
        }
        let approach_top_m = profile.descent_4_altitude_ft * FOOT;
        assert!(
            plan.descent.last().unwrap().start_altitude_m <= approach_top_m + 1.0e-9,
            "the low-speed landing segment must start at the approach rung, not cruise altitude"
        );
        assert!(
            plan.descent
                .iter()
                .any(|segment| segment.kind == SegmentKind::Descent),
            "a diversion must retain high-altitude descent phases"
        );
    }

    #[test]
    fn calibrated_airspeed_diversion_keeps_the_same_phase_boundaries() {
        // The default profile is true-airspeed based; its 250 m/s step speed
        // is above the subsonic CAS conversion domain at this altitude.  Use
        // a deliberately subsonic calibrated schedule here so the test
        // checks phase construction rather than a profile-domain rejection.
        let mut profile = MissionProfileConfig {
            climb_descent_speed_reference: SpeedReference::CalibratedAirspeed,
            takeoff_air_speed_m_s: 110.0,
            initial_climb_air_speed_m_s: 140.0,
            ..MissionProfileConfig::default()
        };
        profile.step_climb_1_air_speed_m_s = 160.0;
        profile.step_climb_2_air_speed_m_s = 165.0;
        profile.descent_1_air_speed_m_s = 160.0;
        profile.descent_2_air_speed_m_s = 150.0;
        profile.descent_3_air_speed_m_s = 140.0;
        profile.descent_4_air_speed_m_s = 130.0;
        profile.landing_air_speed_m_s = 110.0;
        let cruise_m = 10_000.0;
        let arrival_m = 250.0;
        let subdivisions = 4;
        let geometry = ProfileGeometry {
            profile: &profile,
            departure_elevation_m: 12_000.0,
            arrival_elevation_m: arrival_m,
            cruise_altitude_m: cruise_m,
            cas_subdivisions: subdivisions,
            isa_deviation_c: 0.0,
        };
        let plan = geometry
            .plan_at(LegKind::Diversion, 2_000_000.0, cruise_m)
            .expect("the calibrated-airspeed diversion profile resolves");

        assert_eq!(plan.climb.len(), 4 * subdivisions);
        assert_eq!(plan.descent.len(), 5 * subdivisions);
        assert!((plan.climb.first().unwrap().start_altitude_m - arrival_m).abs() < 1.0e-9);
        assert!((plan.climb.last().unwrap().end_altitude_m - cruise_m).abs() < 1.0e-9);
        assert!(plan
            .climb
            .iter()
            .take(subdivisions)
            .all(|segment| { segment.kind == SegmentKind::Takeoff }));
        assert!(plan
            .climb
            .iter()
            .skip(subdivisions)
            .all(|segment| { segment.kind == SegmentKind::Climb }));
        assert!(plan
            .descent
            .iter()
            .take(4 * subdivisions)
            .all(|segment| { segment.kind == SegmentKind::Descent }));
        assert!(plan
            .descent
            .iter()
            .skip(4 * subdivisions)
            .all(|segment| segment.kind == SegmentKind::Landing));
        assert!((plan.descent.last().unwrap().end_altitude_m - arrival_m).abs() < 1.0e-9);
        assert!(
            plan.climb
                .windows(2)
                .take(subdivisions - 1)
                .all(|pair| pair[1].tas_m_s > pair[0].tas_m_s),
            "constant CAS must resolve rising TAS through the high-altitude takeoff rung"
        );
    }

    #[test]
    fn default_diversion_200_nmi_closes_with_tiered_profile() {
        let profile = MissionProfileConfig::default();
        let geometry = ProfileGeometry {
            profile: &profile,
            departure_elevation_m: 0.0,
            arrival_elevation_m: 0.0,
            cruise_altitude_m: 10_668.0, // 35,000 ft, m
            cas_subdivisions: CAS_SPEED_SUBDIVISIONS,
            isa_deviation_c: 0.0,
        };
        let range_m = 200.0 * NAUTICAL_MILE;
        let plan = geometry
            .plan(LegKind::Diversion, range_m)
            .expect("the default 200 nmi diversion must dispatch with altitude adaptation");

        assert!(
            plan.adapted,
            "200 nmi is shorter than the full configured profile footprint"
        );
        assert!(plan.cruise_altitude_m > geometry.floor_cruise_m(LegKind::Diversion));
        assert!(plan.cruise_distance_m() >= -1.0e-9);
        let closed_range_m =
            plan.climb_footprint_m() + plan.cruise_distance_m() + plan.descent_footprint_m();
        assert!(
            (closed_range_m - range_m).abs() <= 1.0e-6,
            "profile dispatch must close the requested still-air distance: {closed_range_m} vs {range_m} m"
        );
        assert_eq!(plan.climb.len(), 4);
        assert!(plan.descent.len() >= 2);
        assert!(plan
            .descent
            .iter()
            .any(|segment| segment.kind == SegmentKind::Descent));
        assert_eq!(plan.descent.last().unwrap().kind, SegmentKind::Landing);
    }

    /// A calibrated-airspeed climb's true airspeed rises with altitude
    /// (falling density), and the sub-rung discretization converges as it is
    /// refined: this is a *discretized* approximation of holding constant
    /// CAS, not the continuous exact quantity, and refining from 4 to 8 to 16
    /// sub-rungs should shrink the discretization error by roughly a factor
    /// of four each time (the error in a piecewise-constant approximation of
    /// a smooth function scales with the sub-rung width squared), not merely
    /// change by some arbitrary amount.
    #[test]
    fn cas_climb_refines_under_subdivision_and_tas_rises_with_altitude() {
        let profile = MissionProfileConfig::default();
        let geometry = ProfileGeometry {
            profile: &profile,
            departure_elevation_m: 0.0,
            arrival_elevation_m: 0.0,
            cruise_altitude_m: 6000.0,
            cas_subdivisions: CAS_SPEED_SUBDIVISIONS,
            isa_deviation_c: 0.0,
        };
        let cas_m_s = 170.0 * 0.514_444_444; // 170 KCAS, the ATR optimum-climb figure.
        let rate_m_s = 6.0;

        let mut footprints_m = Vec::new();
        for &subdivisions in &[4_usize, 8, 16, 64] {
            let segments = geometry
                .cas_vertical_segments(
                    SegmentKind::Climb,
                    600.0,
                    5_600.0,
                    cas_m_s,
                    rate_m_s,
                    subdivisions,
                )
                .expect("a valid subsonic calibrated-airspeed climb resolves");
            assert_eq!(segments.len(), subdivisions);
            // True airspeed strictly rises with altitude at constant CAS.
            for pair in segments.windows(2) {
                assert!(
                    pair[1].tas_m_s > pair[0].tas_m_s,
                    "tas did not rise with altitude: {:?} -> {:?}",
                    pair[0],
                    pair[1]
                );
            }
            footprints_m.push(
                segments
                    .iter()
                    .map(|s| s.horizontal_distance_m)
                    .sum::<f64>(),
            );
        }
        // Richardson-style refinement check: each doubling should shrink the
        // gap to the finest (64-subdivision) reference footprint by roughly
        // a factor of four, not just "get a bit closer."
        let reference_m = *footprints_m.last().expect("four subdivision counts");
        let errors_m: Vec<f64> = footprints_m[..3]
            .iter()
            .map(|&footprint_m| (footprint_m - reference_m).abs())
            .collect();
        assert!(
            errors_m[0] > errors_m[1] && errors_m[1] > errors_m[2],
            "refinement did not monotonically shrink the discretization error: {errors_m:?}"
        );
        let ratio_4_to_8 = errors_m[0] / errors_m[1];
        let ratio_8_to_16 = errors_m[1] / errors_m[2];
        assert!(
            (2.0..8.0).contains(&ratio_4_to_8) && (2.0..8.0).contains(&ratio_8_to_16),
            "refinement ratio should be near the expected ~4x for a piecewise-constant \
             approximation, got 4->8: {ratio_4_to_8}, 8->16: {ratio_8_to_16}"
        );
    }

    /// Quantifies the discretized constant-CAS approximation against a
    /// continuous reference for the ATR-like climb the production
    /// `CAS_SPEED_SUBDIVISIONS` is used on: 170 KCAS from 610 m to the FL170
    /// cruise at 5 m/s. The reference is the same rung split 4096 ways
    /// (piecewise-constant error ~1e-7 relative, far below what is asserted).
    ///
    /// Three quantities are bounded at 4, 8 and 16 sub-rungs:
    /// - speed: the largest gap, anywhere in a sub-rung, between the constant
    ///   true airspeed flown and the true airspeed exact constant-CAS flight
    ///   would have at that altitude;
    /// - time: identically zero, because every sub-rung's duration is
    ///   `dh / Vz` for the constant planned vertical rate regardless of the
    ///   speed it is flown at (fuel and rate-limited time enter only in the
    ///   flown integration, see `mission_model::tests`);
    /// - distance: the relative footprint error.
    ///
    /// The asserted numbers are chosen numerical acceptance thresholds; the
    /// measured errors are reported separately by the
    /// `.agent/probes/atr-physics` probe (worst speed gap 3.24 m/s at 4
    /// sub-rungs on this 4.6 km climb when this test was written). The worst
    /// speed gap is first order in the sub-rung width and the midpoint-rule
    /// footprint error second order (the previous test checks the
    /// ~4x-per-doubling rate).
    #[test]
    fn eight_sub_rungs_bound_the_cas_speed_and_distance_error_for_an_atr_climb() {
        let profile = MissionProfileConfig::default();
        let geometry = ProfileGeometry {
            profile: &profile,
            departure_elevation_m: 610.0,
            arrival_elevation_m: 8.0,
            cruise_altitude_m: 5_182.0,
            cas_subdivisions: CAS_SPEED_SUBDIVISIONS,
            isa_deviation_c: 0.0,
        };
        let cas_m_s = 170.0 * 0.514_444_444;
        let rate_m_s = 5.0;
        let (start_m, end_m) = (610.0, 5_182.0);
        let exact_tas = |altitude_m: f64| {
            let atmosphere =
                alas_atmo::us1976_try_compute_values(altitude_m, 0.0).expect("valid altitude");
            alas_atmo::airspeed::true_from_calibrated(
                cas_m_s,
                atmosphere.pressure_pa,
                atmosphere.temperature_k,
            )
            .expect("subsonic")
        };
        let reference = geometry
            .cas_vertical_segments(SegmentKind::Climb, start_m, end_m, cas_m_s, rate_m_s, 4096)
            .expect("reference resolves");
        let reference_distance_m: f64 = reference.iter().map(|s| s.horizontal_distance_m).sum();
        let reference_time_s: f64 = reference.iter().map(Segment::duration_s).sum();

        // (subdivisions, max speed gap bound m/s, relative distance bound)
        let bounds = [(4_usize, 4.0, 1.0e-3), (8, 2.0, 2.5e-4), (16, 1.0, 6.5e-5)];
        for (subdivisions, speed_bound_m_s, distance_bound) in bounds {
            let segments = geometry
                .cas_vertical_segments(
                    SegmentKind::Climb,
                    start_m,
                    end_m,
                    cas_m_s,
                    rate_m_s,
                    subdivisions,
                )
                .expect("resolves");
            let max_speed_gap_m_s = segments
                .iter()
                .map(|s| {
                    // The gap is largest at a sub-rung's ends, where the
                    // exact TAS is furthest from the midpoint value.
                    (s.tas_m_s - exact_tas(s.start_altitude_m))
                        .abs()
                        .max((s.tas_m_s - exact_tas(s.end_altitude_m)).abs())
                })
                .fold(0.0, f64::max);
            let distance_m: f64 = segments.iter().map(|s| s.horizontal_distance_m).sum();
            let time_s: f64 = segments.iter().map(Segment::duration_s).sum();
            let distance_error = (distance_m - reference_distance_m).abs() / reference_distance_m;
            assert!(
                max_speed_gap_m_s < speed_bound_m_s,
                "{subdivisions} sub-rungs: worst TAS gap {max_speed_gap_m_s} m/s exceeds {speed_bound_m_s}"
            );
            assert!(
                distance_error < distance_bound,
                "{subdivisions} sub-rungs: footprint error {distance_error} exceeds {distance_bound}"
            );
            assert!(
                (time_s - reference_time_s).abs() <= 1.0e-9 * reference_time_s,
                "{subdivisions} sub-rungs: planned time {time_s} s differs from {reference_time_s} s"
            );
        }
    }
}
