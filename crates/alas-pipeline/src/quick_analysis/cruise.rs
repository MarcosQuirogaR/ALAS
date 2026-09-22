// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Thrust-limited cruise speed and service ceiling for the reduced analysis.
//!
//! Drag is a wave-free parabolic base polar (`cd0 + k CL^2`, fitted here
//! from the reduced sweep's parasite-plus-induced terms at the requested
//! cruise Mach and altitude) plus the Raymer/Korn wave-drag term evaluated
//! once, at the flight Mach and the instantaneous lift coefficient, on the
//! drawn aircraft's reference area. The full analysis' own `PolarFit` is
//! fitted to the total drag, which already contains the wave term at the
//! requested Mach, so it is not reused here: doing so counted wave drag
//! twice wherever the fit-window points lie past the drag-divergence Mach.
//! The base polar freezes the Mach and Reynolds dependence of the parasite
//! term at the requested cruise point; the wave term is the only one that
//! follows the flight Mach, and it is zero below the configured onset Mach.
//! The scan covers Mach 0.20 to 0.895, the subsonic and transonic drag-rise
//! regime the Korn correlation is meant for. Thrust is the catalogue
//! propulsion deck at the maximum-climb rating for the installed engines.
//! Both are evaluated at one representative mass, the takeoff-mass estimate,
//! which is conservative for the end of cruise.

use alas_aero::analysis::{AeroAnalysis, PolarSweep};
use alas_atmo::Atmosphere;
use alas_config::{AlasConfig, AnalysisConfig, DesignVector};
use alas_geom::aircraft::airplane::Airplane;
use alas_opt::mdo::propulsion::{max_climb_rate_ft_min, PropulsionDeck};
use alas_prop::system::PropulsionRating;

use crate::full_analysis::{AnalysisReport, FullAnalysis, PolarFit, PolarFitStatus};

/// Excess-power climb rate that defines the service ceiling, 100 ft/min.
pub const SERVICE_CEILING_CLIMB_RATE_M_S: f64 = 0.508;

const MACH_FLOOR: f64 = 0.20;
const MACH_CAP: f64 = 0.895;
const ALTITUDE_CAP_M: f64 = 13_716.0;

/// The dimensionless drag terms the cruise solve sums at one flight point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CruiseDrag {
    /// Lift coefficient for level flight at the solve mass.
    pub cl: f64,
    /// The wave-free base polar, `cd0 + k CL^2`.
    pub cd_base: f64,
    /// The Korn wave-drag term at the flight Mach and `cl`, applied once.
    pub cd_wave: f64,
}

impl CruiseDrag {
    /// Total drag coefficient on the reference area.
    pub fn cd_total(&self) -> f64 {
        self.cd_base + self.cd_wave
    }
}

/// The wave-free base polar `cd0 + k CL^2` of a sweep: the same fit window
/// and solver as the full analysis, applied to the parasite-plus-induced
/// drag instead of the total.
///
/// # Errors
///
/// When the sweep's component vectors do not line up with its lift vector.
pub(crate) fn base_polar_fit(
    polar: &PolarSweep,
    aspect_ratio: f64,
    cfg: &AnalysisConfig,
) -> Result<PolarFit, String> {
    let n = polar.cl.len();
    if polar.cd_parasite.len() != n || polar.cd_induced.len() != n {
        return Err(format!(
            "polar sweep components are inconsistent ({n} lift, {} parasite, {} induced points)",
            polar.cd_parasite.len(),
            polar.cd_induced.len()
        ));
    }
    let mut base = polar.clone();
    base.cd = polar
        .cd_parasite
        .iter()
        .zip(&polar.cd_induced)
        .map(|(parasite, induced)| parasite + induced)
        .collect();
    Ok(FullAnalysis::fit_polar_values(&base, aspect_ratio, cfg))
}

/// Whether a polar fit's coefficients were computed from the sweep, as
/// opposed to the historical constants the fit retains when the sweep
/// cannot support a fit. A fit from the documented fallback window is a
/// computed fit; the retained constants are not and must not publish as
/// cruise speed or ceiling.
pub(crate) fn polar_fit_is_computed(status: PolarFitStatus) -> bool {
    match status {
        PolarFitStatus::Fitted | PolarFitStatus::FittedFallbackWindow => true,
        PolarFitStatus::FallbackInsufficientPoints
        | PolarFitStatus::FallbackLeastSquaresFailure => false,
    }
}

/// The thrust-drag balance of one aircraft at one mass.
pub struct CruiseSolve<'a> {
    config: &'a AlasConfig,
    aero: AeroAnalysis<'a>,
    deck: PropulsionDeck,
    s_ref_m2: f64,
    base_polar: PolarFit,
    mass_kg: f64,
}

impl<'a> CruiseSolve<'a> {
    /// Bind the aircraft, its wave-free base polar and its propulsion deck.
    ///
    /// # Errors
    ///
    /// When the base polar was not computed from the sweep (a retained
    /// fallback constant), is unusable, or the propulsion deck cannot be
    /// bound.
    pub fn new(
        config: &'a AlasConfig,
        plane: &'a Airplane,
        design: &DesignVector,
        report: &AnalysisReport,
        mass_kg: f64,
    ) -> Result<Self, String> {
        let base_polar = base_polar_fit(
            &report.polar,
            report.polar_fit.aspect_ratio,
            &config.analysis,
        )?;
        if !polar_fit_is_computed(base_polar.status) {
            return Err(format!(
                "base polar was not fitted from the reduced sweep (status {}); cruise speed and ceiling are unsupported for this design",
                base_polar.status.as_str()
            ));
        }
        let (cd0, k) = (base_polar.cd0, base_polar.k);
        if !(cd0.is_finite() && cd0 > 0.0 && k.is_finite() && k > 0.0) {
            return Err(format!("base polar fit is not usable (cd0 {cd0}, k {k})"));
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
            base_polar,
            mass_kg,
        })
    }

    /// The wave-free base polar the solve extrapolates over Mach.
    pub fn base_polar(&self) -> &PolarFit {
        &self.base_polar
    }

    /// The requested cruise true airspeed.
    pub fn requested_tas_m_s(&self) -> f64 {
        self.tas_at_requested_altitude(self.config.requirements.cruise_mach)
    }

    /// True airspeed at `mach` and the requested cruise altitude.
    pub fn tas_at_requested_altitude(&self, mach: f64) -> f64 {
        mach * Atmosphere::new(self.config.requirements.cruise_altitude_m).speed_of_sound()
    }

    /// The drag terms for level flight at `mach` and `altitude_m`: the base
    /// polar at the level-flight lift coefficient plus one Korn wave term.
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
        let cl = self.mass_kg * self.config.requirements.gravity_m_s2 / (q * self.s_ref_m2);
        Ok(CruiseDrag {
            cl,
            cd_base: self.base_polar.cd0 + self.base_polar.k * cl * cl,
            cd_wave: self.aero.wave_drag(mach, cl, None),
        })
    }

    /// Thrust available minus drag, in newtons, at `mach` and `altitude_m`.
    fn excess_thrust_n(&self, mach: f64, altitude_m: f64) -> Result<(f64, f64), String> {
        let atmosphere = Atmosphere::new(altitude_m);
        let tas = mach * atmosphere.speed_of_sound();
        let q = 0.5 * atmosphere.density() * tas * tas;
        let drag_n = q * self.s_ref_m2 * self.drag_coefficients(mach, altitude_m)?.cd_total();
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

// Tests assert on the synthetic sweeps they construct here, so a failed
// expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    const CD_PARASITE: f64 = 0.0180;
    const K_TRUE: f64 = 0.0450;

    /// Five points inside the default fit window (CL 0.3 to 0.6) with a
    /// constant parasite term, a parabolic induced term and `wave(cl)` as
    /// the Korn term, summed into `cd` exactly as `run_sweep` does.
    fn synthetic_sweep(wave: impl Fn(f64) -> f64) -> PolarSweep {
        sweep_at(
            &(0..5)
                .map(|i| 0.35 + 0.05 * f64::from(i))
                .collect::<Vec<_>>(),
            wave,
        )
    }

    /// The same construction at arbitrary lift coefficients.
    fn sweep_at(cl: &[f64], wave: impl Fn(f64) -> f64) -> PolarSweep {
        let cl = cl.to_vec();
        let cd_parasite = vec![CD_PARASITE; cl.len()];
        let cd_induced: Vec<f64> = cl.iter().map(|cl| K_TRUE * cl * cl).collect();
        let cd_wave: Vec<f64> = cl.iter().map(|&cl| wave(cl)).collect();
        let cd: Vec<f64> = (0..cl.len())
            .map(|i| cd_parasite[i] + cd_induced[i] + cd_wave[i])
            .collect();
        PolarSweep {
            alpha_deg: cl.iter().map(|cl| cl * 10.0).collect(),
            geometric_alpha_deg: cl.iter().map(|cl| cl * 10.0).collect(),
            l_over_d: cl.iter().zip(&cd).map(|(cl, cd)| cl / cd).collect(),
            cm: vec![0.0; cl.len()],
            cl,
            cd,
            cd_induced,
            cd_wave,
            cd_parasite,
        }
    }

    #[test]
    fn the_base_polar_excludes_the_wave_term_the_total_fit_absorbs() {
        let sweep = synthetic_sweep(|cl| 0.0030 + 0.02 * cl * cl);
        let cfg = AnalysisConfig::default();
        let base = base_polar_fit(&sweep, 9.0, &cfg).expect("consistent sweep");
        assert!((base.cd0 - CD_PARASITE).abs() < 1e-12, "cd0 {}", base.cd0);
        assert!((base.k - K_TRUE).abs() < 1e-12, "k {}", base.k);

        // The full-analysis fit of the same sweep carries the wave term: at
        // CL 0.45 it sits above the base polar by the wave drag there.
        let total = FullAnalysis::fit_polar_values(&sweep, 9.0, &cfg);
        let cl = 0.45;
        let duplicate = (total.cd0 + total.k * cl * cl) - (base.cd0 + base.k * cl * cl);
        let wave_at_cl = 0.0030 + 0.02 * cl * cl;
        assert!(
            (duplicate - wave_at_cl).abs() < 1e-10,
            "total-minus-base {duplicate} should equal the wave term {wave_at_cl}"
        );
    }

    #[test]
    fn a_zero_wave_sweep_gives_the_same_base_and_total_polar() {
        let sweep = synthetic_sweep(|_| 0.0);
        let cfg = AnalysisConfig::default();
        let base = base_polar_fit(&sweep, 9.0, &cfg).expect("consistent sweep");
        let total = FullAnalysis::fit_polar_values(&sweep, 9.0, &cfg);
        assert_eq!(base.cd0.to_bits(), total.cd0.to_bits());
        assert_eq!(base.k.to_bits(), total.k.to_bits());
        assert_eq!(base.status, total.status);
        assert!((base.cd0 - CD_PARASITE).abs() < 1e-12);
        assert!((base.k - K_TRUE).abs() < 1e-12);
    }

    #[test]
    fn a_sweep_with_mismatched_components_is_rejected() {
        let mut sweep = synthetic_sweep(|_| 0.001);
        sweep.cd_induced.pop();
        let error = base_polar_fit(&sweep, 9.0, &AnalysisConfig::default())
            .expect_err("mismatched components");
        assert!(error.contains("inconsistent"), "{error}");
    }
    #[test]
    fn only_fitted_statuses_count_as_computed() {
        assert!(polar_fit_is_computed(PolarFitStatus::Fitted));
        assert!(polar_fit_is_computed(PolarFitStatus::FittedFallbackWindow));
        assert!(!polar_fit_is_computed(
            PolarFitStatus::FallbackInsufficientPoints
        ));
        assert!(!polar_fit_is_computed(
            PolarFitStatus::FallbackLeastSquaresFailure
        ));
    }

    #[test]
    fn the_base_polar_reports_the_full_analysis_status_contract() {
        let cfg = AnalysisConfig::default();
        let fitted = base_polar_fit(&synthetic_sweep(|_| 0.0), 9.0, &cfg).expect("consistent");
        assert_eq!(fitted.status, PolarFitStatus::Fitted);

        // Two points inside the configured window and one outside it: the
        // documented fallback window fits, and that is a computed fit.
        let window =
            base_polar_fit(&sweep_at(&[0.35, 0.45, 0.70], |_| 0.0), 9.0, &cfg).expect("consistent");
        assert_eq!(window.status, PolarFitStatus::FittedFallbackWindow);
        assert!((window.cd0 - CD_PARASITE).abs() < 1e-12 && (window.k - K_TRUE).abs() < 1e-12);

        // One point: the historical constants come back and are flagged.
        let short = base_polar_fit(&sweep_at(&[0.45], |_| 0.0), 9.0, &cfg).expect("consistent");
        assert_eq!(short.status, PolarFitStatus::FallbackInsufficientPoints);
        assert!(
            short.cd0 > 0.0 && short.k > 0.0,
            "the constants pass the coefficient gate"
        );

        // A non-finite parasite term: the solve is refused and flagged.
        let mut broken = synthetic_sweep(|_| 0.0);
        broken.cd_parasite[2] = f64::NAN;
        let failure = base_polar_fit(&broken, 9.0, &cfg).expect("consistent");
        assert_eq!(failure.status, PolarFitStatus::FallbackLeastSquaresFailure);
        assert!(failure.cd0 > 0.0 && failure.k > 0.0);
    }
}
