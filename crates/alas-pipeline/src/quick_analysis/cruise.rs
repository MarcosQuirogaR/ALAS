// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Thrust-limited cruise speed and service ceiling for the reduced analysis.
//!
//! Drag is the parabolic polar fitted by the reduced full analysis
//! (`cd0 + k CL^2`) plus the Raymer/Korn wave-drag term at the flight Mach,
//! evaluated on the drawn aircraft's reference area. Thrust is the catalogue
//! propulsion deck at the maximum-climb rating for the installed engines.
//! Both are evaluated at one representative mass, the takeoff-mass estimate,
//! which is conservative for the end of cruise.

use alas_aero::analysis::AeroAnalysis;
use alas_atmo::Atmosphere;
use alas_config::{AlasConfig, DesignVector};
use alas_geom::aircraft::airplane::Airplane;
use alas_opt::mdo::propulsion::{max_climb_rate_ft_min, PropulsionDeck};
use alas_prop::system::PropulsionRating;

use crate::full_analysis::AnalysisReport;

/// Excess-power climb rate that defines the service ceiling, 100 ft/min.
pub const SERVICE_CEILING_CLIMB_RATE_M_S: f64 = 0.508;

const MACH_FLOOR: f64 = 0.20;
const MACH_CAP: f64 = 0.895;
const ALTITUDE_CAP_M: f64 = 13_716.0;

/// The thrust-drag balance of one aircraft at one mass.
pub struct CruiseSolve<'a> {
    config: &'a AlasConfig,
    aero: AeroAnalysis<'a>,
    deck: PropulsionDeck,
    s_ref_m2: f64,
    cd0: f64,
    k: f64,
    mass_kg: f64,
}

impl<'a> CruiseSolve<'a> {
    /// Bind the aircraft, its fitted polar and its propulsion deck.
    ///
    /// # Errors
    ///
    /// When the polar fit is unusable or the propulsion deck cannot be bound.
    pub fn new(
        config: &'a AlasConfig,
        plane: &'a Airplane,
        design: &DesignVector,
        report: &AnalysisReport,
        mass_kg: f64,
    ) -> Result<Self, String> {
        let cd0 = report.polar_fit.cd0;
        let k = report.polar_fit.k;
        if !(cd0.is_finite() && cd0 > 0.0 && k.is_finite() && k > 0.0) {
            return Err(format!("polar fit is not usable (cd0 {cd0}, k {k})"));
        }
        if !(mass_kg.is_finite() && mass_kg > 0.0) {
            return Err(format!("mass {mass_kg} kg is not usable"));
        }
        let requirements = &config.requirements;
        let deck = PropulsionDeck::from_engine(
            &config.geometry.engine,
            requirements.cruise_mach,
            requirements.cruise_altitude_m,
            max_climb_rate_ft_min(config.mission.profile.initial_climb_rate_m_s),
        )?;
        let aero = AeroAnalysis::new(
            plane,
            design.sweep_deg,
            Some(config.geometry.clone()),
            Some(config.drag_model.clone()),
            Some(config.analysis.clone()),
        );
        Ok(Self {
            config,
            aero,
            deck,
            s_ref_m2: plane.s_ref,
            cd0,
            k,
            mass_kg,
        })
    }

    /// The requested cruise true airspeed.
    pub fn requested_tas_m_s(&self) -> f64 {
        self.tas_at_requested_altitude(self.config.requirements.cruise_mach)
    }

    /// True airspeed at `mach` and the requested cruise altitude.
    pub fn tas_at_requested_altitude(&self, mach: f64) -> f64 {
        mach * Atmosphere::new(self.config.requirements.cruise_altitude_m).speed_of_sound()
    }

    /// Thrust available minus drag, in newtons, at `mach` and `altitude_m`.
    fn excess_thrust_n(&self, mach: f64, altitude_m: f64) -> Result<(f64, f64), String> {
        let atmosphere = Atmosphere::new(altitude_m);
        let tas = mach * atmosphere.speed_of_sound();
        let q = 0.5 * atmosphere.density() * tas * tas;
        if !(q.is_finite() && q > 0.0) {
            return Err(format!("dynamic pressure is not usable at {altitude_m} m"));
        }
        let cl = self.mass_kg * self.config.requirements.gravity_m_s2 / (q * self.s_ref_m2);
        let cd_induced = self.k * cl * cl;
        let cd = self.cd0
            + cd_induced
            + self
                .aero
                .drag_components(mach, altitude_m, cl, 0.0, Some(&atmosphere))
                .cd_wave;
        let drag_n = q * self.s_ref_m2 * cd;
        let flight = self
            .deck
            .flight_condition(altitude_m, tas, self.config.requirements.gravity_m_s2, 0.0)
            .map_err(|error| format!("{error:?}"))?;
        let thrust_n = self
            .deck
            .rated_point(flight, PropulsionRating::MaximumClimb)
            .map_err(|error| format!("{error:?}"))?
            .thrust_n;
        Ok((thrust_n - drag_n, tas))
    }

    /// The highest Mach at which thrust covers drag at the requested
    /// altitude, capped at `MACH_CAP` when thrust still exceeds drag there.
    ///
    /// Excess thrust is not monotonic in Mach: it is negative at low speed,
    /// where the lift coefficient and induced drag are large, positive in the
    /// middle and negative again past the drag rise. The scan therefore
    /// walks down from the cap to the first covered sample and bisects the
    /// crossing above it.
    pub fn achievable_mach_at_requested_altitude(&self) -> Result<f64, String> {
        let altitude_m = self.config.requirements.cruise_altitude_m;
        const STEPS: usize = 30;
        let sample = |k: usize| MACH_FLOOR + (MACH_CAP - MACH_FLOOR) * (k as f64) / (STEPS as f64);
        let mut covered: Option<usize> = None;
        for k in (0..=STEPS).rev() {
            let (excess, _) = self.excess_thrust_n(sample(k), altitude_m)?;
            if excess >= 0.0 {
                covered = Some(k);
                break;
            }
        }
        let Some(k) = covered else {
            return Err(format!(
                "thrust does not cover drag at any Mach between {MACH_FLOOR} and {MACH_CAP} at {altitude_m:.0} m and {:.0} kg",
                self.mass_kg
            ));
        };
        if k == STEPS {
            return Ok(MACH_CAP);
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

    /// The best excess-power climb rate over the deck's Mach domain at one
    /// altitude, in metres per second. Samples that fall outside the deck
    /// domain do not count.
    fn best_climb_rate_m_s(&self, altitude_m: f64) -> Result<f64, String> {
        const STEPS: usize = 14;
        let weight_n = self.mass_kg * self.config.requirements.gravity_m_s2;
        let mut best = f64::NEG_INFINITY;
        let mut any = false;
        for k in 0..=STEPS {
            let mach = MACH_FLOOR + (MACH_CAP - MACH_FLOOR) * (k as f64) / (STEPS as f64);
            if let Ok((excess_n, tas)) = self.excess_thrust_n(mach, altitude_m) {
                any = true;
                best = best.max(excess_n * tas / weight_n);
            }
        }
        if any {
            Ok(best)
        } else {
            Err(format!(
                "the propulsion deck covers no flight condition at {altitude_m:.0} m"
            ))
        }
    }

    /// The service ceiling: the highest altitude at which the best-climb
    /// rate over the deck Mach domain still reaches the threshold, bounded
    /// by the deck's altitude domain.
    pub fn service_ceiling_m(&self) -> Result<f64, String> {
        let climb_rate = |altitude_m: f64| self.best_climb_rate_m_s(altitude_m);
        if climb_rate(0.0)? < SERVICE_CEILING_CLIMB_RATE_M_S {
            return Err(format!(
                "best excess-power climb rate at sea level is below {SERVICE_CEILING_CLIMB_RATE_M_S} m/s at {:.0} kg",
                self.mass_kg
            ));
        }
        if climb_rate(ALTITUDE_CAP_M)? >= SERVICE_CEILING_CLIMB_RATE_M_S {
            return Ok(ALTITUDE_CAP_M);
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
        Ok(0.5 * (low + high))
    }
}
