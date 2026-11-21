// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Thrust-limited cruise speed and service ceiling for the Quick Analysis.
//!
//! Drag and thrust are the report's fuel model
//! ([`crate::fuel_model::report_mission_model`]): the trimmed drag table
//! `CD(CL, M, h)` the segment mission model and the payload-range corners
//! fly ([`alas_opt::mdo::CandidateDrag::cd`]: parasite at the flight Reynolds
//! number, trimmed induced drag, Lock/Korn wave drag), on the table's
//! reference area, and the same model's propulsion deck at the
//! maximum-climb rating. Both are evaluated at one representative mass, the
//! takeoff-mass estimate, which is conservative for the end of cruise. The
//! Mach scan runs from 0.20 to the smaller of 0.895 and the table's upper
//! Mach node, `M_cruise + `[`MACH_ABOVE_CRUISE`]: above that node the table
//! clamps its tabulated wave and form factors, so a speed found there would
//! sit on a frozen drag rise. Atmosphere is the standard day.

use std::sync::Arc;

use alas_atmo::Atmosphere;
use alas_config::AlasConfig;
use alas_opt::mdo::drag_table::MACH_ABOVE_CRUISE;
use alas_opt::mdo::propulsion::PropulsionDeck;
use alas_opt::mdo::CandidateFuelArtifacts;
use alas_prop::system::PropulsionRating;

use crate::fuel_model::report_mission_model;
use crate::full_analysis::AnalysisReport;

/// Excess-power climb rate that defines the service ceiling, 100 ft/min.
pub const SERVICE_CEILING_CLIMB_RATE_M_S: f64 = 0.508;

const MACH_FLOOR: f64 = 0.20;
const MACH_CAP: f64 = 0.895;
/// Upper bound of the altitude scan of the service ceiling, m (45 000 ft).
/// A bound of the search, not a property of the aircraft or of the deck: a
/// climb rate still above the criterion here is reported as "at least this
/// altitude" ([`ServiceCeiling::at_scan_bound`]), never as the ceiling.
pub const CEILING_SCAN_BOUND_M: f64 = 13_716.0;
const ALTITUDE_CAP_M: f64 = CEILING_SCAN_BOUND_M;

/// The outcome of the service-ceiling search.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ServiceCeiling {
    /// The service ceiling, m; or [`CEILING_SCAN_BOUND_M`] when
    /// `at_scan_bound`.
    pub altitude_m: f64,
    /// The best climb rate still reaches the criterion at the upper bound of
    /// the scan: the ceiling is at or above `altitude_m` and was not found.
    pub at_scan_bound: bool,
}

/// The level-flight lift and drag coefficients at one flight point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CruiseDrag {
    /// Lift coefficient for level flight at the solve mass.
    pub cl: f64,
    /// Trimmed drag coefficient of the report's drag table at `cl`.
    pub cd: f64,
    /// Whether `(cl, mach)` lies outside the range the drag table was solved
    /// on: `cd` is then an extrapolation of the table, not a solved point.
    pub outside_solved_range: bool,
}

/// What a reported speed or ceiling says when its drag is read outside the
/// range the drag table was solved on.
pub const OUTSIDE_SOLVED_DRAG_RANGE: &str = "outside the solved drag range";

/// The thrust-drag balance of one aircraft at one mass.
pub struct CruiseSolve<'a> {
    config: &'a AlasConfig,
    artifacts: Arc<CandidateFuelArtifacts>,
    deck: PropulsionDeck,
    mass_kg: f64,
}

impl<'a> CruiseSolve<'a> {
    /// Bind the report's drag table and propulsion deck at `mass_kg`.
    ///
    /// # Errors
    ///
    /// When the report's fuel model cannot be built, its reference area is
    /// not usable, or the mass is not finite and positive.
    pub fn new(
        config: &'a AlasConfig,
        report: &AnalysisReport,
        mass_kg: f64,
    ) -> Result<Self, String> {
        if !(mass_kg.is_finite() && mass_kg > 0.0) {
            return Err(format!("mass {mass_kg} kg is not usable"));
        }
        let artifacts = report.fuel.artifacts(config, &report.design)?;
        let area_m2 = artifacts.reference_area_m2;
        if !(area_m2.is_finite() && area_m2 > 0.0) {
            return Err(format!("drag reference area {area_m2} m2 is not usable"));
        }
        let deck = report_mission_model(config, report)?.propulsion;
        Ok(Self {
            config,
            artifacts,
            deck,
            mass_kg,
        })
    }

    /// The highest Mach the scans fly: 0.895, or the drag table's upper Mach
    /// node when that is lower.
    pub fn mach_cap(&self) -> f64 {
        MACH_CAP.min(self.config.requirements.cruise_mach + MACH_ABOVE_CRUISE)
    }

    /// The requested cruise true airspeed.
    pub fn requested_tas_m_s(&self) -> f64 {
        self.tas_at_requested_altitude(self.config.requirements.cruise_mach)
    }

    /// True airspeed at `mach` and the requested cruise altitude.
    pub fn tas_at_requested_altitude(&self, mach: f64) -> f64 {
        mach * Atmosphere::new(self.config.requirements.cruise_altitude_m).speed_of_sound()
    }

    /// The level-flight lift coefficient at `mach` and `altitude_m` and the
    /// drag table's trimmed drag coefficient there.
    ///
    /// # Errors
    ///
    /// When the dynamic pressure at `altitude_m` is not usable.
    pub fn drag_coefficients(&self, mach: f64, altitude_m: f64) -> Result<CruiseDrag, String> {
        let atmosphere = Atmosphere::new(altitude_m);
        let tas = mach * atmosphere.speed_of_sound();
        let q = 0.5 * atmosphere.density() * tas * tas;
        if !(q.is_finite() && q > 0.0) {
            return Err(format!("dynamic pressure is not usable at {altitude_m} m"));
        }
        let cl = self.mass_kg * self.config.requirements.gravity_m_s2
            / (q * self.artifacts.reference_area_m2);
        Ok(CruiseDrag {
            cl,
            cd: self.artifacts.drag.cd(cl, mach, altitude_m),
            outside_solved_range: self
                .artifacts
                .drag
                .cruise_drag()
                .out_of_range(cl, mach)
                .is_some(),
        })
    }

    /// Maximum-climb thrust minus drag, N, and the true airspeed, m/s, at
    /// `mach` and `altitude_m`.
    ///
    /// # Errors
    ///
    /// When the dynamic pressure is not usable or the deck does not cover
    /// the flight condition.
    pub fn excess_thrust_n(&self, mach: f64, altitude_m: f64) -> Result<(f64, f64), String> {
        let atmosphere = Atmosphere::new(altitude_m);
        let tas = mach * atmosphere.speed_of_sound();
        let q = 0.5 * atmosphere.density() * tas * tas;
        let drag_n =
            q * self.artifacts.reference_area_m2 * self.drag_coefficients(mach, altitude_m)?.cd;
        let flight = self
            .deck
            .flight_condition(altitude_m, tas, self.config.requirements.gravity_m_s2, 0.0)
            .map_err(|error| error.to_string())?;
        let thrust_n = self
            .deck
            .rated_point(flight, PropulsionRating::MaximumClimb)
            .map_err(|error| error.to_string())?
            .thrust_n;
        Ok((thrust_n - drag_n, tas))
    }

    /// The highest Mach at which thrust covers drag at the requested
    /// altitude, capped at the drag domain's upper Mach when thrust still
    /// exceeds drag there.
    ///
    /// Excess thrust is not monotonic in Mach: it is negative at low speed,
    /// where the lift coefficient and induced drag are large, positive in the
    /// middle and negative again past the drag rise. The scan therefore
    /// walks down from the cap to the first covered sample and bisects the
    /// crossing above it.
    pub fn achievable_mach_at_requested_altitude(&self) -> Result<f64, String> {
        let altitude_m = self.config.requirements.cruise_altitude_m;
        const STEPS: usize = 30;
        let cap = self.mach_cap();
        let sample = |k: usize| MACH_FLOOR + (cap - MACH_FLOOR) * (k as f64) / (STEPS as f64);
        let mut covered: Option<usize> = None;
        for k in (0..=STEPS).rev() {
            let (excess, _) = self.excess_thrust_n(sample(k), altitude_m)?;
            if excess >= 0.0 {
                covered = Some(k);
                break;
            }
        }
        let Some(k) = covered else {
            // The sampled Mach closest to covering the drag, to say by how much it fails.
            let mut closest: Option<(f64, f64)> = None;
            for k in 0..=STEPS {
                let (excess, _) = self.excess_thrust_n(sample(k), altitude_m)?;
                if closest.is_none_or(|(_, best)| excess > best) {
                    closest = Some((sample(k), excess));
                }
            }
            let (mach, excess_n) = closest.unwrap_or((cap, f64::NAN));
            return Err(format!(
                "thrust does not cover drag at any Mach between {MACH_FLOOR} and {cap:.3} at {altitude_m:.0} m and {:.0} kg: the closest is Mach {mach:.3}, {:.0} N short; lower the altitude or the mass, or raise the installed thrust",
                self.mass_kg, -excess_n
            ));
        };
        if k == STEPS {
            return Ok(cap);
        }
        let mut low = sample(k);
        let mut high = sample(k + 1);
        for _ in 0..40 {
            let mid = 0.5 * (low + high);
            let (excess, _) = self.excess_thrust_n(mid, altitude_m)?;
            if excess >= 0.0 {
                low = mid;
            } else {
                high = mid;
            }
        }
        Ok(0.5 * (low + high))
    }

    /// The best excess-power climb rate over the scanned Mach range at one
    /// altitude, in metres per second. Samples that fall outside the deck
    /// domain do not count.
    fn best_climb_rate_m_s(&self, altitude_m: f64) -> Result<f64, String> {
        self.best_climb_point(altitude_m).map(|(rate, _)| rate)
    }

    /// [`Self::best_climb_rate_m_s`] with the Mach number of the sample that
    /// gives it.
    fn best_climb_point(&self, altitude_m: f64) -> Result<(f64, f64), String> {
        const STEPS: usize = 14;
        let weight_n = self.mass_kg * self.config.requirements.gravity_m_s2;
        let cap = self.mach_cap();
        let mut best = f64::NEG_INFINITY;
        let mut best_mach = f64::NAN;
        let mut any = false;
        for k in 0..=STEPS {
            let mach = MACH_FLOOR + (cap - MACH_FLOOR) * (k as f64) / (STEPS as f64);
            if let Ok((excess_n, tas)) = self.excess_thrust_n(mach, altitude_m) {
                any = true;
                let rate = excess_n * tas / weight_n;
                if rate > best {
                    best_mach = mach;
                }
                best = best.max(rate);
            }
        }
        if any {
            Ok((best, best_mach))
        } else {
            Err(format!(
                "the propulsion deck covers no flight condition at {altitude_m:.0} m"
            ))
        }
    }

    /// Whether the drag of the reported speed, `mach` at the requested
    /// altitude, is read outside the range the drag table was solved on.
    pub fn speed_outside_solved_drag_range(&self, mach: f64) -> bool {
        self.drag_coefficients(mach, self.config.requirements.cruise_altitude_m)
            .is_ok_and(|drag| drag.outside_solved_range)
    }

    /// Whether the best-climb sample that sets the service ceiling at
    /// `ceiling_m` reads its drag outside the range the drag table was solved
    /// on. `false` when no sample is covered there.
    pub fn ceiling_outside_solved_drag_range(&self, ceiling_m: f64) -> bool {
        self.best_climb_point(ceiling_m)
            .ok()
            .filter(|(_, mach)| mach.is_finite())
            .is_some_and(|(_, mach)| {
                self.drag_coefficients(mach, ceiling_m)
                    .is_ok_and(|drag| drag.outside_solved_range)
            })
    }

    /// [`Self::service_ceiling`] as an altitude, m. At the scan bound the
    /// value is a lower bound of the ceiling; callers that publish it read
    /// [`Self::service_ceiling`] to say so.
    pub fn service_ceiling_m(&self) -> Result<f64, String> {
        self.service_ceiling().map(|ceiling| ceiling.altitude_m)
    }

    /// The service ceiling: the highest altitude at which the best-climb
    /// rate over the deck Mach domain still reaches the threshold, searched
    /// up to [`CEILING_SCAN_BOUND_M`].
    pub fn service_ceiling(&self) -> Result<ServiceCeiling, String> {
        let climb_rate = |altitude_m: f64| self.best_climb_rate_m_s(altitude_m);
        if climb_rate(0.0)? < SERVICE_CEILING_CLIMB_RATE_M_S {
            return Err(format!(
                "best excess-power climb rate at sea level is below {SERVICE_CEILING_CLIMB_RATE_M_S} m/s at {:.0} kg",
                self.mass_kg
            ));
        }
        if climb_rate(ALTITUDE_CAP_M)? >= SERVICE_CEILING_CLIMB_RATE_M_S {
            return Ok(ServiceCeiling {
                altitude_m: ALTITUDE_CAP_M,
                at_scan_bound: true,
            });
        }
        let mut low = 0.0;
        let mut high = ALTITUDE_CAP_M;
        for _ in 0..40 {
            let mid = 0.5 * (low + high);
            if climb_rate(mid)? >= SERVICE_CEILING_CLIMB_RATE_M_S {
                low = mid;
            } else {
                high = mid;
            }
        }
        Ok(ServiceCeiling {
            altitude_m: 0.5 * (low + high),
            at_scan_bound: false,
        })
    }
}
