// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The sea-level reference thrust the field-performance checks are scaled
//! from.
//!
//! Every matching-chart relation this pipeline uses - the take-off field
//! parameter, the cruise thrust margin and the engine-out second-segment
//! requirement - is written against an installed *sea-level* thrust and
//! applies its own density and lapse corrections on top. For a turbofan that
//! quantity is the certificated static rating and is read straight from the
//! catalogue: a turbofan's thrust is nearly flat through the ground roll, so
//! the static value is also its roll mean to within the correlation's own
//! fidelity.
//!
//! For a turboprop neither half of that is true. `EngineConfig::thrust_kn` is
//! held at exactly zero for a shaft-power engine, by design, so that no jet
//! thrust can ever be read for one, and the field-performance path used to
//! take that zero at face value: the ATR 72-600 reached the guard with a
//! static thrust-to-weight ratio of zero and got no take-off field
//! performance at all. And a propeller's thrust falls steeply with speed, so
//! its static value is *not* its roll mean, and using the static value would
//! flatter the field length.
//!
//! The propulsion model already answers exactly this question.
//! `Pw127m568fModel::field_performance` returns the ground-roll mean thrust at
//! `V_LOF / sqrt(2)` - the speed at which a quantity linear in `V^2` equals
//! its roll mean, which is the standard ground-roll convention - together with
//! its asymmetric uncertainty band, the residual core exhaust thrust the
//! installation declares (`0.0` here, and never a converted shaft power), and
//! the model's own evidence class. This module asks for that number at
//! sea-level ISA and at the lift-off *equivalent* airspeed, so the
//! correlation's own `sigma` still carries the airport, and reports what it
//! rests on.

use alas_config::{ActiveEngineModel, AlasConfig};
use alas_opt::mdo::propulsion::turboprop_unit_model;
use alas_prop::turboprop::Pw127mRating;

/// ISA sea-level density, kg/m^3: the reference the matching chart's
/// sea-level quantities and every equivalent airspeed are defined against.
const SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;

/// Where a sea-level reference thrust came from.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StaticThrustSource {
    /// The catalogue's certificated per-engine jet rating.
    CertifiedJetRating,
    /// The active propeller model's ground-roll mean thrust.
    PropellerRollMean {
        /// Propeller and model identity, for provenance.
        identity: String,
        /// Lift-off equivalent airspeed the roll mean was taken against, m/s.
        lift_off_eas_m_s: f64,
        /// `V_LOF / sqrt(2)`, m/s.
        mean_roll_speed_m_s: f64,
        /// All-engine static thrust, N, reported beside the roll mean so the
        /// difference between the two is visible rather than implied.
        static_thrust_n: f64,
        /// Lower bound of the all-engine roll-mean thrust, N.
        low_thrust_n: f64,
        /// Upper bound of the all-engine roll-mean thrust, N.
        high_thrust_n: f64,
        /// What the band rests on.
        uncertainty_basis: String,
        /// Residual core exhaust thrust per engine, N. Zero on the ATR.
        residual_jet_thrust_per_engine_n: f64,
        /// The propeller model's own evidence class.
        model_uncertainty: String,
    },
    /// No physical input exists for this aircraft.
    Unavailable {
        /// Why, in the terms of the model that could not supply it.
        reason: String,
    },
}

/// The installed all-engine sea-level reference thrust and its provenance.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SeaLevelStaticThrust {
    /// All-engine installed sea-level thrust, N: the certificated static
    /// rating for a turbofan, the ground-roll mean for a propeller. Zero
    /// exactly when the source is [`StaticThrustSource::Unavailable`].
    pub thrust_n: f64,
    /// Where it came from.
    pub source: StaticThrustSource,
}

impl SeaLevelStaticThrust {
    fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            thrust_n: 0.0,
            source: StaticThrustSource::Unavailable {
                reason: reason.into(),
            },
        }
    }

    /// The provenance sentence a reader needs beside a field length, or
    /// `None` when the quantity needs no qualification.
    pub fn provenance_note(&self) -> Option<String> {
        match &self.source {
            StaticThrustSource::CertifiedJetRating | StaticThrustSource::Unavailable { .. } => None,
            StaticThrustSource::PropellerRollMean {
                identity,
                lift_off_eas_m_s,
                mean_roll_speed_m_s,
                static_thrust_n,
                low_thrust_n,
                high_thrust_n,
                uncertainty_basis,
                residual_jet_thrust_per_engine_n,
                model_uncertainty,
            } => Some(format!(
                "take-off field performance for this propeller aircraft is scaled from the \
                 {identity} ground-roll MEAN thrust, {:.1} kN all engines at \
                 {mean_roll_speed_m_s:.1} m/s (V_LOF/sqrt(2), for a lift-off equivalent airspeed \
                 of {lift_off_eas_m_s:.1} m/s), not from a jet rating, which stays exactly zero \
                 for a shaft-power engine, and not from the static thrust, which is {:.1} kN and \
                 would flatter the field length. Roll-mean band {:.1} to {:.1} kN \
                 ({uncertainty_basis}). Residual core exhaust thrust \
                 {residual_jet_thrust_per_engine_n:.1} N per engine. Propeller model evidence: \
                 {model_uncertainty}. The resulting field length is preliminary-design evidence, \
                 not a certificated distance.",
                self.thrust_n / 1_000.0,
                static_thrust_n / 1_000.0,
                low_thrust_n / 1_000.0,
                high_thrust_n / 1_000.0,
            )),
        }
    }
}

/// Resolve the installed all-engine sea-level reference thrust for `config`.
///
/// `lift_off_true_airspeed_m_s` and `density_ratio` are the departure field's
/// own lift-off speed and density ratio; the propeller branch converts them to
/// the sea-level equivalent airspeed, because the correlation it feeds applies
/// `sigma` itself and would otherwise count the airport twice.
///
/// The turbofan branch is the catalogue rating and is unchanged: the same
/// `n_engines * thrust_kn * 1000` this check has always used. A propeller
/// model that cannot be built or evaluated yields
/// [`StaticThrustSource::Unavailable`] with the model's own reason, so a
/// missing physical input is still a typed absence rather than a fabricated
/// number.
pub(crate) fn resolve(
    config: &AlasConfig,
    lift_off_true_airspeed_m_s: f64,
    density_ratio: f64,
) -> SeaLevelStaticThrust {
    let engine = &config.geometry.engine;
    let n_engines = engine.spanwise_positions_m.len();
    if n_engines == 0 {
        return SeaLevelStaticThrust::unavailable("no engine is installed on this aircraft");
    }
    match engine.active_model() {
        Ok(ActiveEngineModel::Turbofan(_)) => SeaLevelStaticThrust {
            thrust_n: n_engines as f64 * engine.thrust_kn() * 1_000.0,
            source: StaticThrustSource::CertifiedJetRating,
        },
        Ok(ActiveEngineModel::Turboprop(spec)) => {
            propeller_roll_mean_thrust(spec, n_engines, lift_off_true_airspeed_m_s, density_ratio)
        }
        Err(error) => SeaLevelStaticThrust::unavailable(format!(
            "the engine binding is not coherent: {error}"
        )),
    }
}

fn propeller_roll_mean_thrust(
    spec: &alas_config::TurbopropEngineSpec,
    n_engines: usize,
    lift_off_true_airspeed_m_s: f64,
    density_ratio: f64,
) -> SeaLevelStaticThrust {
    if !lift_off_true_airspeed_m_s.is_finite() || lift_off_true_airspeed_m_s <= 0.0 {
        return SeaLevelStaticThrust::unavailable(format!(
            "the lift-off airspeed {lift_off_true_airspeed_m_s} m/s is not usable, so no \
             ground-roll mean thrust can be taken"
        ));
    }
    if !density_ratio.is_finite() || density_ratio <= 0.0 {
        return SeaLevelStaticThrust::unavailable(format!(
            "the departure density ratio {density_ratio} is not usable"
        ));
    }
    // The correlation this feeds applies `sigma` itself, so the thrust has to
    // be a sea-level one. The matching lift-off speed at sea level is the
    // field's lift-off *equivalent* airspeed, which is what makes the pair
    // consistent instead of counting the airport's density twice.
    let lift_off_eas_m_s = lift_off_true_airspeed_m_s * density_ratio.sqrt();
    let model = turboprop_unit_model(spec);
    let field = match model.field_performance(
        SEA_LEVEL_DENSITY_KG_M3,
        Pw127mRating::NormalTakeoff,
        lift_off_eas_m_s,
    ) {
        Ok(field) => field,
        Err(error) => {
            return SeaLevelStaticThrust::unavailable(format!(
                "the propeller model could not be evaluated over the ground roll: {error}"
            ))
        }
    };
    let engines = n_engines as f64;
    let thrust_n = engines * field.mean_ground_roll_thrust_per_engine_n;
    if !thrust_n.is_finite() || thrust_n <= 0.0 {
        return SeaLevelStaticThrust::unavailable(format!(
            "the propeller model returned {thrust_n:.3} N of ground-roll mean thrust, which is \
             not usable"
        ));
    }
    let band = field.forward_flight_thrust_uncertainty;
    SeaLevelStaticThrust {
        thrust_n,
        source: StaticThrustSource::PropellerRollMean {
            identity: spec.propeller_model.clone(),
            lift_off_eas_m_s,
            mean_roll_speed_m_s: field.mean_ground_roll_true_airspeed_m_s,
            static_thrust_n: engines * field.static_thrust_per_engine_n,
            low_thrust_n: thrust_n * (1.0 + band.relative_low),
            high_thrust_n: thrust_n * (1.0 + band.relative_high),
            uncertainty_basis: band.basis.to_owned(),
            residual_jet_thrust_per_engine_n: field.residual_jet_thrust_per_engine_n,
            model_uncertainty: format!("{:?}", field.model_uncertainty),
        },
    }
}

#[cfg(test)]
// Shipped presets are test preconditions: a missing one is the failure being
// reported, not a recoverable library condition.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    /// A representative sea-level lift-off speed, so the tests exercise the
    /// contract rather than an airport.
    const LIFT_OFF_M_S: f64 = 70.0;

    fn preset_config(name: &str) -> AlasConfig {
        let preset = alas_config::presets::get(name).expect("a shipped preset");
        let mut config = AlasConfig {
            preset: preset.name.to_owned(),
            geometry: preset.geometry.clone(),
            requirements: preset.requirements.clone(),
            ..AlasConfig::default()
        };
        config.geometry.engine.apply_engine_spec();
        config
    }

    #[test]
    fn a_turbofan_keeps_the_certificated_rating_unchanged() {
        let config = preset_config("A320-200");
        let resolved = resolve(&config, LIFT_OFF_M_S, 1.0);
        let expected_n = config.geometry.engine.spanwise_positions_m.len() as f64
            * config.geometry.engine.thrust_kn()
            * 1_000.0;
        assert_eq!(resolved.source, StaticThrustSource::CertifiedJetRating);
        assert_eq!(resolved.thrust_n, expected_n);
        assert!(resolved.provenance_note().is_none());
    }

    #[test]
    fn the_turboprop_uses_the_roll_mean_not_the_static_thrust() {
        let config = preset_config("ATR72-600");
        assert_eq!(
            config.geometry.engine.thrust_kn(),
            0.0,
            "the jet rating must stay exactly zero for a shaft-power engine"
        );
        let resolved = resolve(&config, LIFT_OFF_M_S, 1.0);
        let StaticThrustSource::PropellerRollMean {
            static_thrust_n,
            mean_roll_speed_m_s,
            residual_jet_thrust_per_engine_n,
            low_thrust_n,
            high_thrust_n,
            ..
        } = &resolved.source
        else {
            panic!("expected a propeller roll mean, got {:?}", resolved.source);
        };
        // A propeller loses thrust with speed, so the roll mean is strictly
        // below the static value. Taking the static one is the flattering
        // error this replaced.
        assert!(
            resolved.thrust_n < *static_thrust_n,
            "roll mean {} is not below static {static_thrust_n}",
            resolved.thrust_n
        );
        assert!((mean_roll_speed_m_s - LIFT_OFF_M_S / std::f64::consts::SQRT_2).abs() < 1.0e-9);
        assert_eq!(*residual_jet_thrust_per_engine_n, 0.0);
        // The forward-flight band is one-sided by construction: the blade
        // efficiency is declared at the conservative end of the 0.86-0.91
        // range the three independent routes agree on, so the modelled thrust
        // is its own lower bound and the band only opens upward.
        assert!(*low_thrust_n <= resolved.thrust_n && resolved.thrust_n < *high_thrust_n);

        let weight_n = config.requirements.mtow_kg * config.requirements.gravity_m_s2;
        let roll_mean_tw = resolved.thrust_n / weight_n;
        assert!(
            (0.15..=0.45).contains(&roll_mean_tw),
            "ground-roll mean T/W {roll_mean_tw} is outside the physical band for a twin turboprop"
        );
        let note = resolved
            .provenance_note()
            .expect("a propeller result must carry its provenance");
        assert!(note.contains("ground-roll MEAN"));
        assert!(note.contains("stays exactly zero"));
    }

    #[test]
    fn an_aircraft_with_no_installed_engine_is_a_typed_absence() {
        let mut config = preset_config("ATR72-600");
        config.geometry.engine.spanwise_positions_m.clear();
        let resolved = resolve(&config, LIFT_OFF_M_S, 1.0);
        assert!(matches!(
            resolved.source,
            StaticThrustSource::Unavailable { .. }
        ));
        assert_eq!(resolved.thrust_n, 0.0);
    }

    #[test]
    fn an_unusable_lift_off_speed_is_a_typed_absence_rather_than_a_guess() {
        let config = preset_config("ATR72-600");
        let resolved = resolve(&config, f64::NAN, 1.0);
        assert!(matches!(
            resolved.source,
            StaticThrustSource::Unavailable { .. }
        ));
    }
}
