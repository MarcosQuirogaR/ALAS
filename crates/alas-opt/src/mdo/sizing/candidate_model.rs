// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one mission model a candidate's fuel is priced on.
//!
//! The sizing closure, the finalist report, the payload-range corners and the
//! pipeline's dispatch all fly the candidate on this model: the route of the
//! configuration, the candidate's trimmed drag ([`CandidateDrag`]) on its
//! reference area, and the propulsion deck its [`DeckKey`] names. The closure
//! builds its route model through [`route_mission_model`], and a consumer
//! rebuilds the same model from the carried [`CandidateFuelArtifacts`] with
//! [`candidate_mission_model`], so a trip priced downstream is the trip the
//! closure priced, to the bit, rather than a second estimate from another
//! polar.

use std::sync::Arc;

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, TailSizing};

use super::route::{self, Route};
use crate::mdo::build::{build_geometry_with_fuselage_policy, first_mass_pass};
use crate::mdo::mission_model::{CruiseDrag, PhaseAeroLimits, SegmentMissionModel};
use crate::mdo::mtow_modes::closure_mission;
use crate::mdo::propulsion::{max_climb_rate_ft_min, PropulsionDeck};
use crate::mdo::trim::trim_and_polar;
use crate::mdo::types::{CandidateFuelArtifacts, DeckKey};

/// The propulsion deck of `config`'s engine at the sizing cruise point, and
/// the key that rebuilds it.
///
/// # Errors
///
/// The engine binding failure, as a description.
pub(super) fn sizing_deck(config: &AlasConfig) -> Result<(PropulsionDeck, DeckKey), String> {
    let req = &config.requirements;
    let max_climb_ft_min = max_climb_rate_ft_min(config.mission.profile.initial_climb_rate_m_s);
    let deck = PropulsionDeck::from_engine(
        &config.geometry.engine,
        req.cruise_mach,
        req.cruise_altitude_m,
        max_climb_ft_min,
    )
    .map_err(|error| format!("engine binding failed: {error}"))?;
    let key = DeckKey {
        identity: deck.identity().to_owned(),
        reference_mach: req.cruise_mach,
        reference_altitude_m: req.cruise_altitude_m,
        max_climb_rate_ft_min: max_climb_ft_min,
    };
    Ok((deck, key))
}

/// The deck `key` names, bound to `config`'s engine.
fn deck_from_key(config: &AlasConfig, key: &DeckKey) -> Result<PropulsionDeck, String> {
    let deck = PropulsionDeck::from_engine(
        &config.geometry.engine,
        key.reference_mach,
        key.reference_altitude_m,
        key.max_climb_rate_ft_min,
    )
    .map_err(|error| format!("engine binding failed: {error}"))?;
    if deck.identity() != key.identity {
        return Err(format!(
            "the configured engine binds deck `{}`, not the candidate's `{}`",
            deck.identity(),
            key.identity
        ));
    }
    Ok(deck)
}

/// The model of `route` flown on `cruise_drag` (coefficients on
/// `wing_area_m2`, m^2) and `propulsion`, at the route's flown level and the
/// departure's ISA deviation, the ambient convention of the native mission.
/// The design cruise altitude, which
/// [`alas_config::mission::CruiseAltitudePolicy::Design`] flies, is the
/// requirements' `cruise_altitude_m`, never the route's operational level.
///
/// # Errors
///
/// The first invalid model term, as a description.
pub(super) fn route_mission_model(
    config: &AlasConfig,
    route: &Route,
    wing_area_m2: f64,
    cruise_drag: Arc<dyn CruiseDrag>,
    propulsion: PropulsionDeck,
) -> Result<SegmentMissionModel, String> {
    let req = &config.requirements;
    SegmentMissionModel::new(
        config.mission.profile.clone(),
        req.cruise_mach,
        route.flown_cruise_altitude_m,
        route.departure_elevation_m,
        route.arrival_elevation_m,
        wing_area_m2,
        cruise_drag,
        req.gravity_m_s2,
        route.holding_altitude_m,
        PhaseAeroLimits::from_config(config),
        propulsion,
    )
    .map(|model| {
        model
            .with_design_cruise_altitude_m(req.cruise_altitude_m)
            .with_isa_deviation_c(
                route
                    .departure
                    .map(|airport| airport.isa_deviation_c)
                    .unwrap_or(0.0),
            )
    })
}

/// The candidate's mission model, rebuilt from its carried `artifacts`: the
/// model its sizing closure flew, at the design-mission level when the
/// configuration's takeoff-mass plan declares a design mission, with the
/// closure's frozen trip plan. A trip over the plan's range flies that plan;
/// any other trip adapts its level and climb as the closure's would.
///
/// `config` is the configuration the candidate was sized under.
///
/// # Errors
///
/// The engine binding or the first invalid model term, as a description.
pub fn candidate_mission_model(
    config: &AlasConfig,
    artifacts: &CandidateFuelArtifacts,
) -> Result<SegmentMissionModel, String> {
    let route = route::resolve(config);
    let model = route_mission_model(
        config,
        &route,
        artifacts.reference_area_m2,
        artifacts.drag.cruise_drag(),
        deck_from_key(config, &artifacts.deck)?,
    )?;
    let mut model = closure_mission(
        &config.mtow_plan(),
        config,
        &model,
        route.route_distance_m,
        (0.0, 0),
    )
    .map_err(|failure| format!("design-mission model is not valid: {}", failure.reason))?
    .map_or(model, |mission| mission.model);
    if let Some(plan) = &artifacts.frozen_plan {
        model = model.with_frozen_plan(plan.clone());
    }
    Ok(model)
}

/// The fuel artifacts of the aircraft `design` describes under `config`,
/// for an analysis that has no sized candidate: the geometry, the first mass
/// pass and the cruise trim of the sizing closure's first pass
/// ([`super::run_candidate_cancellable`]), with the declared fuselage length
/// kept, and no frozen plan.
///
/// # Errors
///
/// The geometry, mass, trim or engine failure, as a description.
pub fn baseline_fuel_artifacts(
    config: &AlasConfig,
    design: &DesignVector,
) -> Result<CandidateFuelArtifacts, String> {
    let failed = |stage: &str, reason: &str| format!("{stage} failed: {reason}");
    let (candidate_config, dv, mut plane) =
        build_geometry_with_fuselage_policy(config, &design.to_array(), true)
            .map_err(|failure| failed("geometry build", failure.reason))?;
    let plan = candidate_config.mtow_plan();
    let seeded = plan
        .requires_mission_sized_evaluation()
        .then(|| candidate_config.at_sized_closure_mass(plan.seed_kg));
    let (_, _, cg, ..) = first_mass_pass(seeded.as_ref().unwrap_or(&candidate_config), &dv, &plane)
        .map_err(|failure| failed("mass analysis", failure.reason))?;
    let polar = trim_and_polar(&candidate_config, &mut plane, cg[0], &dv, plan.seed_kg)
        .map_err(|failure| failed("cruise trim", failure.reason))?;
    let (_, deck) = sizing_deck(&candidate_config)?;
    Ok(CandidateFuelArtifacts {
        drag: polar.drag,
        reference_area_m2: plane.s_ref,
        deck,
        frozen_plan: None,
        tail_sizing: TailSizing::of(&candidate_config.geometry.empennage, &dv),
    })
}

// A test asserts on values it constructed, so a failed unwrap is the
// assertion failing.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::mission::CruiseAltitudePolicy;

    const NMI: f64 = 1_852.0;

    /// The design cruise level is the requirements' `cruise_altitude_m` on
    /// every model a candidate is priced on. The A320-200 preset route is
    /// filed at its declared operational level, below the design level, so a
    /// route model whose design field followed the route flew that level
    /// under `CruiseAltitudePolicy::Design`.
    #[test]
    fn the_design_policy_flies_the_design_level_on_the_route_model() {
        let mut config =
            AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).unwrap();
        config.mission.profile.cruise_altitude_policy = CruiseAltitudePolicy::Design;
        let design = alas_config::presets::get("A320-200").unwrap().design_vector;
        let artifacts = baseline_fuel_artifacts(&config, &design).unwrap();
        let route = route::resolve(&config);
        let design_m = config.requirements.cruise_altitude_m;
        assert!(
            route.flown_cruise_altitude_m < design_m - 300.0,
            "precondition: the preset route level {} m lies below the design level {design_m} m",
            route.flown_cruise_altitude_m
        );
        let model = route_mission_model(
            &config,
            &route,
            artifacts.reference_area_m2,
            artifacts.drag.cruise_drag(),
            deck_from_key(&config, &artifacts.deck).unwrap(),
        )
        .unwrap();
        assert_eq!(model.cruise_altitude_m, route.flown_cruise_altitude_m);
        assert_eq!(model.design_cruise_altitude_m, design_m);
        // A route long enough for the design ladder, at a light mass the
        // design level does not have to be lowered for.
        let flown = model.fly_trip(60_000.0, 1_500.0 * NMI).unwrap();
        assert_eq!(
            flown.cruise_altitude_m, design_m,
            "flown level against the design level; the route level is {} m",
            route.flown_cruise_altitude_m
        );
    }
}
