// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one engine-out evidence builder shared by the optimizer residuals, the
//! feasibility findings, the field reference and the matching chart.
//!
//! The engine-out climb checks of CS 25.119 and 25.121 compare a required
//! sea-level-static thrust-to-weight with the installed one. Converting the
//! in-flight requirement to that axis needs the thrust the remaining engines
//! deliver at the check's speed, altitude and temperature relative to their
//! rated sea-level-static thrust, and the drag of the failed engine and of
//! the asymmetric-thrust trim. This module derives all three from the engine
//! deck and the candidate airframe, in one place:
//!
//! - **Thrust ratio** `r = F(V, h, dT, rating) / F_SLS`. Turbofan: the
//!   Bartel-Young OpenAP deck (`PropulsionDeck`) at the take-off/go-around
//!   rating, ratio of the same deck's two evaluations. The deck's take-off
//!   thrust depends on pressure ratio and Mach only, so a hot day acts only
//!   through the speed of sound (no temperature lapse): recorded limitation.
//!   Maximum continuous is the deck's maximum-climb envelope, which the deck
//!   itself reports as extrapolated. Turboprop: the PW127M unit model at the
//!   take-off (all engines) or maximum take-off reserve (one engine out)
//!   rating, relative to the installed static thrust the caller supplies, so
//!   the ratio is consistent with whichever static figure the caller uses.
//! - **Failed-engine drag**, Roskam VI Eq. 4.67 / Torenbeek G-82 (jets) and
//!   Torenbeek Sec. 5.4.3(d) (feathered propeller), unless the configuration
//!   supplies a value.
//! - **Asymmetric trim drag**, Torenbeek G-84 to G-89 from the fin, rudder and
//!   engine arm of the candidate, unless the configuration supplies a value.
//!   With the aircraft weight bound ([`EngineOutModel::with_weight_kg`]) the
//!   fin side force comes from the solved yaw-balance trim state (bank within
//!   5 deg, sideslip only when the bank limit forces it), otherwise from the
//!   zero-sideslip balance. Sideslip toward the live engine relieves the
//!   rudder deflection, not the fin side force, so the zero-sideslip balance
//!   is also the least-drag state whenever the bank limit allows it.

use alas_atmo::us1976_compute_values;
use alas_config::airports::Airport;
use alas_config::{ActiveEngineModel, AlasConfig, PerformanceConfig};
use alas_geom::aircraft::airplane::Airplane;
use alas_perf::performance::failed_engine_drag::{
    asymmetric_trim_drag_area_m2, feathered_propeller_cd, fin_force_drag_areas_m2,
    high_bypass_streams, windmilling_jet_drag_area_m2,
};
use alas_perf::performance::yaw_balance::{YawControlGeometry, YawTrimMode};
use alas_perf::performance::{OeiV2Condition, VSpeeds};
use alas_prop::system::PropulsionRating;
use alas_prop::turboprop::{
    Pw127m568fModel, Pw127mRating, TurbopropCommand, TurbopropCondition, TurbopropMode,
};

use crate::mdo::propulsion::{max_climb_rate_ft_min, turboprop_unit_model, PropulsionDeck};

/// Name of the fin surface in the built airplane.
const FIN_NAME: &str = "Vertical Stabilizer";

/// Flight condition of one engine-out check.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EngineOutCondition {
    /// Geometric altitude above sea level, m.
    pub altitude_m: f64,
    /// ISA temperature deviation, K.
    pub isa_deviation_c: f64,
    /// True airspeed, m/s.
    pub tas_m_s: f64,
}

/// Engine rating a check evaluates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineOutRating {
    /// Take-off / go-around thrust (turboprop: normal take-off all engines
    /// operating, maximum take-off reserve with one engine out).
    TakeoffGoAround,
    /// Maximum continuous thrust.
    MaximumContinuous,
    /// Maximum-cruise thrust, the rating level flight at the cruise point is
    /// judged at (jet: 0.93 of maximum climb, Torenbeek 2013 Sec. 9.2 and
    /// Jenkinson Figs. 9.19-9.21; turboprop: the published maximum-cruise
    /// rating).
    MaximumCruise,
    /// Maximum-climb thrust, the rating the cruise-altitude capability is
    /// judged at (initial cruise altitude capability, top-of-climb residual
    /// rate of climb).
    MaximumClimb,
}

/// Where the asymmetric trim drag of a point came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrimDragBasis {
    /// `PerformanceConfig::oei_asymmetric_trim_cd` supplied by the user.
    ConfiguredOverride,
    /// Torenbeek G-84 to G-89 at zero sideslip from the candidate's fin and
    /// engine arm, no weight known: the fin carries the whole moment, which is
    /// the least-side-force balance when the bank limit allows it.
    ZeroSideslipBalance,
    /// The yaw-balance trim state (`alas_perf::performance::yaw_balance`) at
    /// the aircraft weight: bank within 5 deg, sideslip only when the bank
    /// limit forces it, fin side force from the solved state.
    YawTrim,
    /// No fin geometry and no override: the value is unknown.
    Unavailable,
}

/// The engine-out evidence of one condition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EngineOutPoint {
    /// Thrust of each operating engine at the condition, N.
    pub thrust_per_engine_n: f64,
    /// Available per-engine thrust over the rated static per-engine thrust.
    pub thrust_ratio: f64,
    /// Failed-engine (windmilling or feathered) drag coefficient on the
    /// reference wing area.
    pub windmilling_cd: f64,
    /// Asymmetric trim drag coefficient, when it can be evaluated.
    pub asymmetric_trim_cd: Option<f64>,
    /// Origin of [`Self::asymmetric_trim_cd`].
    pub trim_basis: TrimDragBasis,
}

enum Source {
    Jet {
        deck: PropulsionDeck,
        fan_diameter_m: f64,
    },
    Propeller {
        model: Pw127m568fModel,
        static_per_engine_n: f64,
    },
}

/// Engine-out evidence for one candidate or one configured aircraft.
pub struct EngineOutModel {
    source: Source,
    n_engines: usize,
    gravity_m_s2: f64,
    s_ref_m2: f64,
    windmilling_override: Option<f64>,
    trim_override: Option<f64>,
    yaw: Option<yaw_model::YawModel>,
    weight_n: Option<f64>,
    spool_fraction: f64,
}

/// Airframe handle used only for the fin and centre-of-gravity geometry.
#[derive(Clone, Copy)]
pub struct YawAirframe<'a> {
    /// The built airplane.
    pub plane: &'a Airplane,
    /// Centre-of-gravity x station in the airplane frame, m.
    pub cg_x_m: f64,
}

impl EngineOutModel {
    /// Bind the engine deck and airframe geometry of `config`.
    ///
    /// `s_ref_m2` is the reference wing area the drag coefficients are
    /// expressed on. `propeller_static_per_engine_n` is the installed static
    /// thrust per engine the caller compares against (turboprops only; it
    /// must be the same figure that defines the caller's available thrust).
    ///
    /// # Errors
    ///
    /// The deck or engine-binding failure, as text.
    pub fn new(
        config: &AlasConfig,
        airframe: Option<YawAirframe<'_>>,
        s_ref_m2: f64,
        propeller_static_per_engine_n: Option<f64>,
    ) -> Result<Self, String> {
        let engine = &config.geometry.engine;
        let n_engines = engine.spanwise_positions_m.len();
        if n_engines == 0 {
            return Err("engine binding has no installed units".to_owned());
        }
        if !(s_ref_m2.is_finite() && s_ref_m2 > 0.0) {
            return Err("reference wing area must be positive".to_owned());
        }
        let source = match engine.active_model().map_err(|error| error.to_string())? {
            ActiveEngineModel::Turbofan(_) => {
                let requirements = &config.requirements;
                let deck = PropulsionDeck::from_engine(
                    engine,
                    requirements.cruise_mach,
                    requirements.cruise_altitude_m,
                    max_climb_rate_ft_min(config.mission.profile.initial_climb_rate_m_s),
                )?;
                Source::Jet {
                    deck,
                    fan_diameter_m: engine.fan_diameter_m,
                }
            }
            ActiveEngineModel::Turboprop(spec) => {
                let static_per_engine_n = propeller_static_per_engine_n
                    .filter(|value| value.is_finite() && *value > 0.0)
                    .ok_or_else(|| {
                        "turboprop engine-out thrust ratio needs the installed static thrust"
                            .to_owned()
                    })?;
                Source::Propeller {
                    model: turboprop_unit_model(spec),
                    static_per_engine_n,
                }
            }
        };
        let perf = &config.performance;
        let yaw = airframe.and_then(|frame| yaw_model::build(config, frame));
        Ok(Self {
            source,
            n_engines,
            gravity_m_s2: config.requirements.gravity_m_s2,
            s_ref_m2,
            windmilling_override: perf.oei_windmilling_cd,
            trim_override: perf.oei_asymmetric_trim_cd,
            yaw,
            weight_n: None,
            spool_fraction: perf.go_around_spool_thrust_fraction,
        })
    }

    /// Number of installed engines.
    pub fn n_engines(&self) -> usize {
        self.n_engines
    }

    /// Bind the aircraft weight the bank limit of the yaw trim is evaluated
    /// against. Without it the trim drag is the zero-sideslip balance.
    #[must_use]
    pub fn with_weight_kg(mut self, weight_kg: f64) -> Self {
        self.weight_n =
            (weight_kg.is_finite() && weight_kg > 0.0).then_some(weight_kg * self.gravity_m_s2);
        self
    }

    /// The fin, rudder and airframe data of the yaw balance, when the
    /// airframe supplies them.
    pub fn yaw_control(&self) -> Option<YawControlGeometry> {
        self.yaw.and_then(|model| model.control)
    }

    /// Fraction of go-around thrust available 8 s after the levers move.
    pub fn spool_fraction(&self) -> f64 {
        self.spool_fraction
    }

    /// Thrust per operating engine and its ratio to the rated static thrust,
    /// with `engines_out` engines inoperative (0 or 1).
    ///
    /// # Errors
    ///
    /// The deck or unit-model rejection, as text.
    pub fn thrust(
        &self,
        condition: EngineOutCondition,
        rating: EngineOutRating,
        engine_out: bool,
    ) -> Result<(f64, f64), String> {
        match &self.source {
            Source::Jet { deck, .. } => {
                let flight = deck
                    .flight_condition(
                        condition.altitude_m,
                        condition.tas_m_s,
                        self.gravity_m_s2,
                        condition.isa_deviation_c,
                    )
                    .map_err(|error| error.to_string())?;
                let static_flight = deck
                    .flight_condition(0.0, 0.0, self.gravity_m_s2, 0.0)
                    .map_err(|error| error.to_string())?;
                let deck_rating = match rating {
                    EngineOutRating::TakeoffGoAround => PropulsionRating::TakeoffGoAround,
                    EngineOutRating::MaximumContinuous => PropulsionRating::MaximumContinuous,
                    EngineOutRating::MaximumCruise => PropulsionRating::Cruise,
                    EngineOutRating::MaximumClimb => PropulsionRating::MaximumClimb,
                };
                let at_condition = deck
                    .rated_point(flight, deck_rating)
                    .map_err(|error| error.to_string())?
                    .thrust_n;
                let at_static = deck
                    .rated_point(static_flight, PropulsionRating::TakeoffGoAround)
                    .map_err(|error| error.to_string())?
                    .thrust_n;
                if !(at_static > 0.0 && at_condition.is_finite() && at_condition > 0.0) {
                    return Err("engine deck returned no usable thrust".to_owned());
                }
                Ok((
                    at_condition / self.n_engines as f64,
                    at_condition / at_static,
                ))
            }
            Source::Propeller {
                model,
                static_per_engine_n,
            } => {
                let atmosphere =
                    us1976_compute_values(condition.altitude_m, condition.isa_deviation_c);
                let rating = match (rating, engine_out) {
                    (EngineOutRating::TakeoffGoAround, false) => Pw127mRating::NormalTakeoff,
                    (EngineOutRating::TakeoffGoAround, true) => Pw127mRating::MaximumTakeoffReserve,
                    (EngineOutRating::MaximumContinuous, _) => Pw127mRating::MaximumContinuous,
                    (EngineOutRating::MaximumCruise, _) => Pw127mRating::MaximumCruise,
                    (EngineOutRating::MaximumClimb, _) => Pw127mRating::MaximumClimb,
                };
                let output = model
                    .evaluate_at_temperature(
                        TurbopropCondition {
                            density_kg_m3: atmosphere.density_kg_m3,
                            true_airspeed_m_s: condition.tas_m_s,
                        },
                        atmosphere.temperature_k,
                        TurbopropCommand {
                            rating,
                            power_fraction: 1.0,
                            mode: TurbopropMode::Governed,
                            propeller_speed_rpm: model.governed_propeller_speed_rpm,
                        },
                    )
                    .map_err(|error| format!("turboprop engine-out thrust: {error}"))?;
                let thrust = output.total_thrust_n;
                if !(thrust.is_finite() && thrust > 0.0) {
                    return Err("turboprop model returned no usable thrust".to_owned());
                }
                Ok((thrust, thrust / static_per_engine_n))
            }
        }
    }

    /// The engine-out evidence at `condition`: remaining-engine thrust,
    /// failed-engine drag and asymmetric trim drag.
    ///
    /// `clean_cd0` is the clean zero-lift drag coefficient at the condition
    /// (used only by the feathered-propeller estimate).
    ///
    /// # Errors
    ///
    /// The thrust evaluation failure, as text.
    pub fn point(
        &self,
        condition: EngineOutCondition,
        rating: EngineOutRating,
        clean_cd0: f64,
    ) -> Result<EngineOutPoint, String> {
        let (thrust_per_engine_n, thrust_ratio) = self.thrust(condition, rating, true)?;
        let atmosphere = us1976_compute_values(condition.altitude_m, condition.isa_deviation_c);
        let q_pa = 0.5 * atmosphere.density_kg_m3 * condition.tas_m_s * condition.tas_m_s;
        let mach = condition.tas_m_s / atmosphere.speed_of_sound_m_s;
        let windmilling_cd = match self.windmilling_override {
            Some(value) => value,
            None => match &self.source {
                Source::Jet { fan_diameter_m, .. } => {
                    windmilling_jet_drag_area_m2(
                        *fan_diameter_m,
                        &high_bypass_streams(*fan_diameter_m),
                        mach,
                    )
                    .ok_or_else(|| "windmilling drag inputs are invalid".to_owned())?
                        / self.s_ref_m2
                }
                Source::Propeller { .. } => feathered_propeller_cd(clean_cd0)
                    .ok_or_else(|| "feathered propeller drag input is invalid".to_owned())?,
            },
        };
        let (asymmetric_trim_cd, trim_basis) = match (self.trim_override, self.yaw) {
            (Some(value), _) => (Some(value), TrimDragBasis::ConfiguredOverride),
            (None, Some(model)) => {
                let force_n = thrust_per_engine_n + windmilling_cd * q_pa * self.s_ref_m2;
                self.trim_drag(model, force_n, q_pa, mach)
            }
            (None, None) => (None, TrimDragBasis::Unavailable),
        };
        Ok(EngineOutPoint {
            thrust_per_engine_n,
            thrust_ratio,
            windmilling_cd,
            asymmetric_trim_cd,
            trim_basis,
        })
    }

    /// Whether the bound engine is a turboprop.
    pub fn is_turboprop(&self) -> bool {
        matches!(self.source, Source::Propeller { .. })
    }

    /// Asymmetric trim drag coefficient and its basis for the asymmetric
    /// force `force_n` (live-engine thrust plus windmilling drag) at the fin
    /// dynamic pressure `q_pa`: the solved yaw trim when the weight is bound,
    /// else the zero-sideslip balance.
    fn trim_drag(
        &self,
        model: yaw_model::YawModel,
        force_n: f64,
        q_pa: f64,
        mach: f64,
    ) -> (Option<f64>, TrimDragBasis) {
        if let (Some(control), Some(weight_n)) = (model.control, self.weight_n) {
            let trim = control.trim(
                force_n,
                q_pa,
                mach,
                Some(weight_n),
                YawTrimMode::MinimumDrag,
            );
            if let Some(trim) = trim {
                let areas = fin_force_drag_areas_m2(model.balance, trim.fin_force_n.abs(), q_pa);
                return (
                    areas.map(|(induced, rudder)| (induced + rudder) / self.s_ref_m2),
                    TrimDragBasis::YawTrim,
                );
            }
        }
        let area = asymmetric_trim_drag_area_m2(model.balance, force_n, q_pa);
        (
            area.map(|value| value / self.s_ref_m2),
            TrimDragBasis::ZeroSideslipBalance,
        )
    }
}

/// The second-segment evidence of one departure, built once for every
/// consumer: the optimizer residual, the feasibility finding, the ISA
/// reference and the matching chart all call this instead of assembling the
/// thrust ratio and drag terms themselves.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SecondSegmentEvidence {
    /// The departure/V2 record for [`alas_perf::performance::assess_oei_climb`].
    pub condition: OeiV2Condition,
    /// The engine-out point behind it, when the deck could be evaluated.
    pub point: Option<EngineOutPoint>,
}

/// Build the CS 25.121(b) evidence at `V2` for `departure`.
///
/// A configured ratio or drag value replaces the derived one. When the deck
/// or the airframe cannot supply a term, the term stays `None` and the
/// assessor reports an evidence gap instead of scoring with a placeholder.
///
/// `clean_cd0` returns the clean zero-lift drag coefficient for
/// `(mach, altitude_m, isa_deviation_c)`.
pub fn second_segment_evidence(
    model: Option<&EngineOutModel>,
    perf: &PerformanceConfig,
    departure: &Airport,
    speeds: &VSpeeds,
    clean_cd0: &dyn Fn(f64, f64, f64) -> f64,
) -> SecondSegmentEvidence {
    let atmosphere = us1976_compute_values(departure.elevation_m, departure.isa_deviation_c);
    let mach = speeds.v2_ms / atmosphere.speed_of_sound_m_s;
    let point = model.and_then(|model| {
        model
            .point(
                EngineOutCondition {
                    altitude_m: departure.elevation_m,
                    isa_deviation_c: departure.isa_deviation_c,
                    tas_m_s: speeds.v2_ms,
                },
                EngineOutRating::TakeoffGoAround,
                clean_cd0(mach, departure.elevation_m, departure.isa_deviation_c),
            )
            .ok()
    });
    let condition = OeiV2Condition {
        departure_elevation_m: departure.elevation_m,
        departure_isa_deviation_c: departure.isa_deviation_c,
        v2_over_vstall: speeds.v2_ms / speeds.v_stall_to_ms,
        condition_to_sls_thrust_ratio: perf
            .oei_condition_to_sls_thrust_ratio
            .or(point.map(|p| p.thrust_ratio)),
        asymmetric_trim_cd: perf
            .oei_asymmetric_trim_cd
            .or(point.and_then(|p| p.asymmetric_trim_cd)),
        windmilling_cd: perf.oei_windmilling_cd.or(point.map(|p| p.windmilling_cd)),
    };
    SecondSegmentEvidence { condition, point }
}

#[cfg(test)]
mod tests;
mod yaw_model;
