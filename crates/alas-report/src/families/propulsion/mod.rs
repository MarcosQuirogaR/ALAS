// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py

//! Propulsion thermodynamic cycle summary, the engine designer's nacelle
//! preview, and parametric trade-space sweeps (carpet plot, efficiency
//! decomposition, bypass-ratio sensitivity, altitude/Mach envelope).
//!
//! Every plotted quantity comes from [`alas_prop::cycle::compute_turbofan_cycle`]
//! or one of its [`alas_prop::cycle::sweeps`] wrappers, evaluated at the
//! current [`AlasConfig`]'s engine and flight-condition fields, matching
//! upstream's five `figure_propulsion_*` functions plus
//! `figure_engine_designer_preview` in `alas/reporting/visualization.py`.
//! None of those six Python functions reads its `report` parameter (checked
//! by inspection of each body), so this port drops it from every signature
//! rather than carrying an argument nothing consumes.

mod altitude;
mod cycle;
mod support;
mod sweeps;
mod technology;
mod ts_labels;
mod ts_preview;
mod turboprop_cycle;

pub use altitude::{cruise_thrust_lapse, figure_propulsion_altitude_sweep};
pub use cycle::{
    figure_engine_designer_preview, figure_engine_designer_preview_sized,
    figure_propulsion_cycle_summary, propulsion_cycle_summary,
};
pub use sweeps::{
    figure_propulsion_bpr_sensitivity, figure_propulsion_carpet_plot,
    figure_propulsion_efficiency_decomposition,
};

use alas_config::{ActiveEngineModel, AlasConfig, TurbofanEngineSpec};
use alas_opt::mdo::propulsion::{max_climb_rate_ft_min, PropulsionDeck};
use alas_prop::cycle::operating_line::solve_cruise_operating_point;
use alas_prop::cycle::TurbofanCycleInputs;
use alas_prop::system::PropulsionRating;

use crate::families::common::linspace;
use crate::scene::Scene;

/// Standard gravity, m/s^2.
const STANDARD_GRAVITY_M_S2: f64 = alas_atmo::STANDARD_GRAVITY;

/// The on-design cycle at the engine's catalogue (take-off) turbine inlet
/// temperature and the design requirements' cruise Mach and altitude.
///
/// Ported from `_propulsion_design_point` (`visualization.py`). This is the
/// full-rating design cycle the engine designer previews; a cruising engine is
/// throttled below it, see [`cruise_cycle`].
fn catalogue_design_point(config: &AlasConfig, eng: &TurbofanEngineSpec) -> TurbofanCycleInputs {
    let req = &config.requirements;
    TurbofanCycleInputs {
        mach: req.cruise_mach,
        altitude_m: req.cruise_altitude_m,
        bypass_ratio: eng.bypass_ratio,
        overall_pressure_ratio: eng.overall_pressure_ratio,
        fan_pressure_ratio: eng.fan_pressure_ratio,
        turbine_inlet_temperature_k: eng.turbine_inlet_temp_k,
    }
}

/// What the throttled cruise point was matched to: the mission deck at the
/// same condition and thrust.
#[derive(Debug, Clone, Copy, PartialEq)]
struct DeckMatch {
    /// Cruise thrust of one engine, N.
    thrust_n: f64,
    /// The deck's fuel flow over that thrust, mg/(N s).
    tsfc_mg_ns: f64,
    /// `true` when the catalogue cruise thrust exceeds the deck's maximum
    /// climb thrust, so the match is to the deck's maximum instead.
    capped_at_maximum_climb: bool,
    /// The catalogue cruise thrust carried to the design altitude, per engine, N.
    catalogue_thrust_n: f64,
}

/// The cruise cycle the propulsion figures show.
///
/// An engine at cruise is not at its take-off rating: the turbine inlet
/// temperature falls until the thrust equals the cruise thrust of the
/// engine's catalogue datum (the part-power cruise thrust the TSFC is quoted
/// at, carried to the design altitude at constant `T / delta`, Torenbeek
/// 2013 p. 67), capped by what the mission deck delivers at maximum climb.
/// The pressure ratios and the air flow follow the temperature through the
/// turbine-compatibility relations of [`alas_prop::cycle::operating_line`].
/// When the deck or the match is unavailable the catalogue design point is
/// returned and [`CruiseCycle::matched`] carries the reason.
#[derive(Debug, Clone, PartialEq)]
struct CruiseCycle {
    /// The cycle inputs at the matched (throttled) operating point.
    inputs: TurbofanCycleInputs,
    /// The catalogue take-off turbine inlet temperature, K.
    takeoff_tit_k: f64,
    /// Thrust of one engine at those inputs, N (`NaN` without a match).
    thrust_n: f64,
    /// The deck point the cycle was matched to, or why there is none.
    matched: Result<DeckMatch, String>,
    /// Why the operating line could not be solved, when the deck matched but
    /// the cycle stayed at the catalogue design point.
    solver_failure: Option<String>,
}

/// The mission deck's point at the cruise thrust of the engine's catalogue
/// datum, per engine.
fn deck_cruise_point(config: &AlasConfig, eng: &TurbofanEngineSpec) -> Result<DeckMatch, String> {
    let engine = &config.geometry.engine;
    let req = &config.requirements;
    let count = engine.spanwise_positions_m.len().max(1) as f64;
    let deck = PropulsionDeck::from_engine(
        engine,
        req.cruise_mach,
        req.cruise_altitude_m,
        max_climb_rate_ft_min(config.mission.profile.initial_climb_rate_m_s),
    )?;
    let air = alas_atmo::us1976_compute_values(req.cruise_altitude_m, 0.0);
    let flight = deck
        .flight_condition(
            req.cruise_altitude_m,
            req.cruise_mach * air.speed_of_sound_m_s,
            STANDARD_GRAVITY_M_S2,
            0.0,
        )
        .map_err(|error| error.to_string())?;
    // The catalogue cruise thrust at its own altitude, carried to the design
    // altitude at constant T / delta, and capped at the deck's maximum climb.
    let reference =
        alas_atmo::us1976_compute_values(eng.off_design.cruise_reference_altitude_m, 0.0);
    let catalogue_n =
        eng.off_design.cruise_reference_thrust_n * count * air.pressure_pa / reference.pressure_pa;
    let climb = deck
        .rated_point(flight, PropulsionRating::MaximumClimb)
        .map_err(|error| error.to_string())?;
    let capped_at_maximum_climb = catalogue_n > climb.thrust_n;
    let point = deck
        .at_thrust(
            flight,
            catalogue_n.min(climb.thrust_n),
            PropulsionRating::MaximumClimb,
        )
        .map_err(|error| error.to_string())?;
    if !(point.thrust_n > 0.0 && point.fuel_flow_kg_s > 0.0) {
        return Err("the deck delivers no cruise thrust at the design point".to_owned());
    }
    Ok(DeckMatch {
        thrust_n: point.thrust_n / count,
        tsfc_mg_ns: point.fuel_flow_kg_s / point.thrust_n * 1.0e6,
        capped_at_maximum_climb,
        catalogue_thrust_n: catalogue_n / count,
    })
}

/// The throttled cruise cycle of `eng` at the design point.
fn cruise_cycle(config: &AlasConfig, eng: &TurbofanEngineSpec) -> CruiseCycle {
    let catalogue = catalogue_design_point(config, eng);
    let unmatched = |matched| CruiseCycle {
        inputs: catalogue,
        takeoff_tit_k: eng.turbine_inlet_temp_k,
        thrust_n: f64::NAN,
        matched,
        solver_failure: None,
    };
    let deck = match deck_cruise_point(config, eng) {
        Ok(deck) => deck,
        Err(reason) => return unmatched(Err(reason)),
    };
    // A cycle that cannot deliver the deck's thrust even at the take-off
    // temperature stays at the catalogue design point, with the deck's own
    // figures beside it.
    match solve_cruise_operating_point(
        &catalogue,
        eng.rated_thrust_kn * 1_000.0,
        deck.thrust_n,
        &config.propulsion_cycle,
    ) {
        Ok(point) => CruiseCycle {
            inputs: point.inputs,
            takeoff_tit_k: eng.turbine_inlet_temp_k,
            thrust_n: point.thrust_n,
            matched: Ok(deck),
            solver_failure: None,
        },
        Err(reason) => CruiseCycle {
            solver_failure: Some(reason),
            ..unmatched(Ok(deck))
        },
    }
}

/// The cruise design point the cycle summary, carpet, efficiency and bypass
/// figures evaluate the cycle at: the throttled temperature of
/// [`cruise_cycle`], so the four show one operating point.
fn design_point(config: &AlasConfig, eng: &TurbofanEngineSpec) -> TurbofanCycleInputs {
    cruise_cycle(config, eng).inputs
}

/// Resolve the single authoritative turbofan payload used by every jet figure.
/// Invalid technology/payload combinations must never fall back to the
/// deprecated flat compatibility copy; they surface as the error text so the
/// figure can show a status scene instead of aborting the process.
fn turbofan_spec(config: &AlasConfig) -> Result<&TurbofanEngineSpec, String> {
    match config.geometry.engine.active_model() {
        Ok(ActiveEngineModel::Turbofan(spec)) => Ok(spec),
        Ok(ActiveEngineModel::Turboprop(_)) => {
            Err("a turboprop binding reached a turbofan-only figure".to_owned())
        }
        Err(error) => Err(format!("invalid propulsion binding: {error}")),
    }
}

/// [`turbofan_spec`], or the status scene a figure of `size` returns instead.
fn turbofan_or_status<'a>(
    config: &'a AlasConfig,
    theme: Option<&str>,
    size: (f64, f64),
) -> Result<&'a TurbofanEngineSpec, Scene> {
    turbofan_spec(config).map_err(|message| {
        crate::status_figure::sized_status_scene(
            "Propulsion model unavailable",
            &message,
            false,
            crate::theme::get_palette(theme),
            size.0,
        )
    })
}

fn is_turboprop(config: &AlasConfig) -> bool {
    matches!(
        config.geometry.engine.propulsion_technology,
        alas_config::PropulsionTechnology::Turboprop
    )
}

#[cfg(test)]
// A failed expect in this module is the assertion failing, not a library
// invariant being broken.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn design_point_reads_the_engine_and_the_cruise_requirement() {
        let config = AlasConfig::default();
        let eng = turbofan_spec(&config).expect("default config binds a turbofan");
        let dp = design_point(&config, eng);
        assert_eq!(dp.mach, config.requirements.cruise_mach);
        assert_eq!(dp.altitude_m, config.requirements.cruise_altitude_m);
        assert_eq!(dp.bypass_ratio, config.geometry.engine.bypass_ratio);
        // The catalogue datum is the take-off cycle; the cruise point is
        // throttled to the cruise thrust, and its OPR is the operating line's
        // own: it scales with Tt4/Tt2, which is higher at cruise than at
        // take-off because the inlet air is colder (Tt2 = 249 K against 288 K).
        let datum = catalogue_design_point(&config, eng);
        assert_eq!(
            datum.overall_pressure_ratio,
            config.geometry.engine.overall_pressure_ratio
        );
        assert!(dp.overall_pressure_ratio.is_finite() && dp.overall_pressure_ratio > 1.0);
        assert!(dp.fan_pressure_ratio > 1.0);
    }
}
