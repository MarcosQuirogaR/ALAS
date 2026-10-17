// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Installed static thrust read off the configured engine binding, for the
//! airworthiness thrust-to-weight residuals.
//!
//! Mission fuel and off-design thrust do not come from scalar terms: the
//! sizing loop flies the selected engine's deck through
//! [`super::propulsion::PropulsionDeck`]. What remains here is the
//! sea-level-static rating the takeoff and one-engine-inoperative climb
//! residuals compare against, written once for both technology branches.
//! A turboprop's static thrust is the unit model's own evaluation, the same
//! call the mission deck and the field-performance contract make, so the
//! optimizer and the deck cannot disagree on it.

use alas_config::{ActiveEngineModel, EngineConfig, TurbopropEngineSpec};
use alas_prop::turboprop::{Pw127mRating, TurbopropCommand, TurbopropCondition, TurbopropMode};

use super::propulsion::turboprop_unit_model;

/// ISA sea-level density, kg/m^3.
const SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;
/// ISA sea-level temperature, K.
const SEA_LEVEL_TEMPERATURE_K: f64 = 288.15;

/// Installed static thrust per engine, kN, for the performance residuals'
/// available thrust-to-weight ratio.
///
/// # Errors
///
/// A description of the binding failure, when `engine` does not resolve to a
/// coherent turbofan or turboprop physics payload, or of the propeller model's
/// rejection.
pub(crate) fn static_thrust_kn_per_engine(engine: &EngineConfig) -> Result<f64, String> {
    match engine
        .active_model()
        .map_err(|error| format!("engine binding failed: {error}"))?
    {
        ActiveEngineModel::Turbofan(spec) => Ok(spec.rated_thrust_kn),
        ActiveEngineModel::Turboprop(spec) => {
            turboprop_static_thrust_n(spec).map(|thrust_n| thrust_n / 1_000.0)
        }
    }
}

/// Sea-level-static, ISA, zero-airspeed thrust of one turboprop engine at the
/// normal take-off rating, N, from the unit model (accessory extraction,
/// gearbox efficiency and the static figure of merit included).
///
/// # Errors
///
/// The unit model's rejection, as a description.
pub fn turboprop_static_thrust_n(spec: &TurbopropEngineSpec) -> Result<f64, String> {
    let model = turboprop_unit_model(spec);
    model
        .evaluate_at_temperature(
            TurbopropCondition {
                density_kg_m3: SEA_LEVEL_DENSITY_KG_M3,
                true_airspeed_m_s: 0.0,
            },
            SEA_LEVEL_TEMPERATURE_K,
            TurbopropCommand {
                rating: Pw127mRating::NormalTakeoff,
                power_fraction: 1.0,
                mode: TurbopropMode::Governed,
                propeller_speed_rpm: model.governed_propeller_speed_rpm,
            },
        )
        .map(|output| output.total_thrust_n)
        .map_err(|error| format!("turboprop static thrust: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::EngineConfig;

    #[test]
    fn the_default_turbofan_binding_yields_a_finite_positive_static_thrust() {
        let engine = EngineConfig::default();
        let thrust_kn = static_thrust_kn_per_engine(&engine)
            .unwrap_or_else(|error| panic!("static thrust: {error}"));
        assert!(thrust_kn > 0.0 && thrust_kn.is_finite());
    }

    fn atr_config() -> alas_config::AlasConfig {
        alas_config::AlasConfig::from_value(&serde_json::json!({"preset": "ATR72-600"}))
            .unwrap_or_else(|error| panic!("preset: {error}"))
    }

    /// PROP-03: the optimizer's turboprop static thrust is the mission deck's
    /// own sea-level-static take-off thrust, per engine, to rounding.
    #[test]
    fn the_optimizer_static_thrust_is_the_decks_take_off_thrust_for_every_turboprop() {
        use crate::mdo::propulsion::PropulsionDeck;
        use alas_prop::system::PropulsionRating;
        for preset in ["ATR72-600", "A400M"] {
            let config =
                alas_config::AlasConfig::from_value(&serde_json::json!({"preset": preset}))
                    .unwrap_or_else(|error| panic!("{preset}: {error}"));
            let engine = &config.geometry.engine;
            let optimizer_n = static_thrust_kn_per_engine(engine).unwrap() * 1_000.0;
            let deck = PropulsionDeck::from_engine(
                engine,
                config.requirements.cruise_mach,
                config.requirements.cruise_altitude_m,
                None,
            )
            .unwrap();
            let flight = deck.flight_condition(0.0, 0.0, 9.806_65, 0.0).unwrap();
            let deck_n = deck
                .rated_point(flight, PropulsionRating::TakeoffGoAround)
                .unwrap()
                .thrust_n
                / deck.n_engines() as f64;
            assert!(
                (optimizer_n - deck_n).abs() <= 1.0e-9 * deck_n,
                "{preset}: optimizer {optimizer_n} N, deck {deck_n} N"
            );
        }
    }

    /// PROP-16: the production taxi/idle flow of every turbofan preset is the
    /// ICAO LTO 7 % thrust point of its catalogue schedule
    /// (`part_power_fuel_flow_ratios[0]`, 0.0893 for the CFM56-5B4/3) times
    /// the take-off fuel flow, not the generic 0.07 of the unused Breguet
    /// fixture.
    #[test]
    fn production_taxi_fuel_is_the_icao_seven_percent_point_for_every_turbofan_preset() {
        use crate::mdo::propulsion::PropulsionDeck;
        let mut checked = 0;
        for name in alas_config::presets::available() {
            let config = alas_config::AlasConfig::from_value(&serde_json::json!({"preset": name}))
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            let Ok(alas_config::ActiveEngineModel::Turbofan(spec)) =
                config.geometry.engine.active_model()
            else {
                continue;
            };
            let deck = PropulsionDeck::from_engine(
                &config.geometry.engine,
                config.requirements.cruise_mach,
                config.requirements.cruise_altitude_m,
                None,
            )
            .unwrap();
            let flight = deck.flight_condition(0.0, 0.0, 9.806_65, 0.0).unwrap();
            let taxi_kg_s = deck.idle_point(flight).unwrap().fuel_flow_kg_s;
            let expected = spec.part_power_fuel_flow_ratios[0]
                * spec.takeoff_fuel_flow_kg_s
                * deck.n_engines() as f64;
            assert!(
                (taxi_kg_s - expected).abs() <= 1.0e-9 * expected,
                "{name}: taxi {taxi_kg_s} kg/s, ICAO 7 % point {expected} kg/s"
            );
            checked += 1;
        }
        assert!(checked >= 8, "{checked} turbofan presets checked");
    }

    /// Documentation of the retired closed form: full shaft power through an
    /// actuator disc at figure of merit 0.75 over the unit model's
    /// (P - accessory) x gearbox at 0.70 is (0.75/0.70)^(2/3) x
    /// (P / ((P - 25 kW) 0.98))^(2/3), about 1.071 for the PW127M.
    #[test]
    fn the_retired_closed_form_overstated_the_static_thrust_by_about_seven_percent() {
        let config = atr_config();
        let Ok(alas_config::ActiveEngineModel::Turboprop(spec)) =
            config.geometry.engine.active_model()
        else {
            panic!("ATR72-600 is a turboprop");
        };
        let disk_area_m2 = std::f64::consts::PI * (spec.propeller_diameter_m / 2.0).powi(2);
        let shaft_power_w = spec.takeoff_shaft_power_kw * 1_000.0;
        let retired_n = 0.75_f64.powf(2.0 / 3.0)
            * (2.0 * SEA_LEVEL_DENSITY_KG_M3 * disk_area_m2 * shaft_power_w * shaft_power_w).cbrt();
        let unit_n = turboprop_static_thrust_n(spec).unwrap();
        let ratio = retired_n / unit_n;
        let expected = (0.75_f64 / 0.70).powf(2.0 / 3.0)
            * (shaft_power_w / ((shaft_power_w - 25_000.0) * 0.98)).powf(2.0 / 3.0);
        assert!((ratio - expected).abs() < 1.0e-9, "{ratio} vs {expected}");
        assert!((ratio - 1.071).abs() < 0.005, "{ratio}");
    }
}
