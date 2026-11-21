// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The flight the Mission page draws for the standard mission.
//!
//! Building it is not frame-rate work (hashing the configuration for the planner's key, the
//! airport lookups, the pipeline's phase list, the stage texts), so [`standard_flight`] keeps it
//! per case: it is rebuilt only when the configuration, the design point, the route of the last
//! run or the language changes, and a frame in between only asks the planner whether the
//! background flight has landed (until it has) or returns the kept result.

use std::sync::{Arc, Mutex};

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, MissionPlan};

use super::{planned_flight, standard, standard_note};
use crate::state::AppState;
use crate::views::tr_label;

/// The flight drawn and the note for the cruise-leg radios.
pub(super) type Planned = Arc<(Option<standard::Standard>, Option<String>)>;

/// What a kept flight depends on.
#[derive(Clone, PartialEq)]
struct Inputs {
    config: u64,
    design: u64,
    route_m: Option<u64>,
    language: String,
}

/// The kept flight of the current case.
struct Kept {
    inputs: Inputs,
    config: Arc<AlasConfig>,
    design: Option<DesignVector>,
    distance_m: f64,
    key: u64,
    /// The nominal schematic, shown dimmed while the flight is computed.
    nominal: Result<standard::Standard, String>,
    /// The page while the flight is computing (built once), and when the
    /// page started waiting for it.
    computing: Option<(Planned, std::time::Instant)>,
    /// The page once the flight has landed or failed.
    settled: Option<Planned>,
}

#[derive(Clone, Default)]
struct Memo(Arc<Mutex<Option<Kept>>>);

/// The planned flight of the standard mission on the case's route, and the note
/// for the cruise-leg radios. No flight when the case has stages, no route, or
/// the pipeline refuses the profile (the note says why).
pub(super) fn standard_flight(
    state: &AppState,
    ctx: &egui::Context,
    plan: &MissionPlan,
) -> Planned {
    let none: Planned = Arc::new((None, None));
    if plan.has_stages() {
        return none;
    }
    let Some(config) = state.typed_config_arc() else {
        return none;
    };
    let inputs = Inputs {
        config: state.config_fingerprint_now(),
        design: crate::state::state_memo::design_values_fingerprint(&state.design_values),
        route_m: run_route(state, &config).map(|route| route.total_distance_m().to_bits()),
        language: alas_i18n::get_language(),
    };
    let memo: Memo = ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Memo>(egui::Id::new("mission_flight_memo"))
            .clone()
    });
    let Ok(mut guard) = memo.0.lock() else {
        return none;
    };
    if guard.as_ref().map(|kept| &kept.inputs) != Some(&inputs) {
        *guard = build(state, config, inputs);
    }
    let Some(kept) = guard.as_mut() else {
        return none;
    };
    if let Some(done) = &kept.settled {
        return Arc::clone(done);
    }
    // The flight the run will fly, computed in the background; until it lands the nominal
    // schematic is shown dimmed (never the flight of an earlier case).
    let status = match &kept.design {
        Some(design) => planned_flight::poll_keyed(
            ctx,
            kept.key,
            (&kept.config, design, run_route(state, &kept.config)),
            planned_flight::fly,
        ),
        // No design point to fly: say so instead of computing for ever (the
        // memo rebuilds once the design values change).
        None => planned_flight::Status::Failed(
            alas_i18n::t(Some("the design point is incomplete"), None).into_owned(),
        ),
    };
    let (config, distance_m) = (Arc::clone(&kept.config), kept.distance_m);
    match status {
        planned_flight::Status::Ready(flight) => {
            let planned = Ok(standard::from_flight(&config, &flight));
            let done = page(&config, distance_m, planned);
            kept.settled = Some(Arc::clone(&done));
            done
        }
        planned_flight::Status::Failed(error) => {
            let planned = kept.nominal.clone().map(|mut standard| {
                standard.computing = true;
                standard.warnings = vec![crate::views::tr_fields(
                    "Flight not computed: {error}",
                    &[("error", error)],
                )];
                standard
            });
            let done = page(&config, distance_m, planned);
            kept.settled = Some(Arc::clone(&done));
            done
        }
        planned_flight::Status::Computing => {
            let (waiting, since) = kept.computing.get_or_insert_with(|| {
                let planned = kept.nominal.clone().map(|mut standard| {
                    standard.computing = true;
                    standard
                });
                (
                    page(&config, distance_m, planned),
                    std::time::Instant::now(),
                )
            });
            // The note counts the seconds the flight has been computing (see
            // `theme::in_progress`); the flight's own note replaces it once
            // the flight lands.
            match &waiting.1 {
                Some(note) if since.elapsed().as_secs() >= 1 => {
                    let note = crate::theme::in_progress(ctx, note, Some(*since));
                    Arc::new((waiting.0.clone(), Some(note)))
                }
                _ => Arc::clone(waiting),
            }
        }
    }
}

/// The kept state of a new case, or none when the case has no route.
fn build(state: &AppState, config: Arc<AlasConfig>, inputs: Inputs) -> Option<Kept> {
    let distance_m = crate::views::mission_profile_inputs::route_distance_m(&state.config_values)?;
    let design = state.current_design();
    let key = design.as_ref().map_or(0, |design| {
        planned_flight::case_key(&config, design, run_route(state, &config))
    });
    let nominal = standard::planned(&config, distance_m);
    Some(Kept {
        inputs,
        config,
        design,
        distance_m,
        key,
        nominal,
        computing: None,
        settled: None,
    })
}

/// The flight and the leg note of a planned flight (or of the reason there is none).
fn page(
    config: &AlasConfig,
    distance_m: f64,
    planned: Result<standard::Standard, String>,
) -> Planned {
    let note = match &planned {
        Ok(standard) if standard.computing => Some(
            standard
                .warnings
                .first()
                .cloned()
                .unwrap_or_else(|| tr_label("Estimate - computing the flown mission")),
        ),
        _ => standard_note::leg_note(config, distance_m, &planned),
    };
    Arc::new((planned.ok(), note))
}

/// The route the last run of this case flew (a dispatched one included), so that the page flies the
/// route the run flew; none when there is no run of the same airports.
fn run_route<'a>(
    state: &'a AppState,
    config: &alas_config::AlasConfig,
) -> Option<&'a alas_route::route::Route> {
    let result = state.pipeline_result.as_ref()?;
    (result.config.departure_airport == config.departure_airport
        && result.config.arrival_airport == config.arrival_airport)
        .then_some(result.route.as_ref())
        .flatten()
}

/// The solver phase whose parameters a stage of the standard mission stands
/// for, so a click on the plot opens that phase's editor.
pub(super) fn phase_of_standard_stage(id: &str) -> Option<&'static str> {
    match id {
        "takeoff" => Some("takeoff"),
        "climb" => Some("initial_climb"),
        "cruise" => Some("cruise_1"),
        "descent" => Some("descent_1"),
        "landing" => Some("final_approach"),
        _ => None,
    }
}
