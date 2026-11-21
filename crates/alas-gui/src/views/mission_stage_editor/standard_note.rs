// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The text the standard mission's page adds to its flight: the note next to the cruise-leg
//! radios.

use alas_config::AlasConfig;

use super::standard::Standard;
use crate::views::tr_fields;

/// The note next to the cruise-leg radios: why the flight cannot be planned, the legs the run
/// flies when they are not the legs set, the legs it flies against what the route length
/// suggests, or what the route forces the plan to change. The count flown is the count of the
/// drawn flight (the pipeline's own phase list), so the note never contradicts the diagram.
pub(crate) fn leg_note(
    config: &AlasConfig,
    route_distance_m: f64,
    planned: &Result<Standard, String>,
) -> Option<String> {
    let set = crate::views::mission_profile_inputs::set_cruise_legs(config);
    match planned {
        Err(error) => Some(tr_fields(
            "No flight planned: {error}",
            &[("error", error.clone())],
        )),
        Ok(flight) => {
            let flown = flight.cruise_legs;
            if flown < set {
                return Some(tr_fields(
                    "{set} legs set; {flown} flown on this route",
                    &[("set", set.to_string()), ("flown", flown.to_string())],
                ));
            }
            let (origin, destination) =
                crate::views::mission_profile_inputs::resolved_route_airports(config)?;
            let suggested = alas_mission::propose_profile_for_route(
                config,
                &origin,
                &destination,
                route_distance_m,
            )
            .ok()?
            .active_cruise_legs;
            if suggested < flown {
                return Some(tr_fields(
                    "{flown} legs flown; the route length suggests {suggested}",
                    &[
                        ("flown", flown.to_string()),
                        ("suggested", suggested.to_string()),
                    ],
                ));
            }
            (!flight.warnings.is_empty()).then(|| flight.warnings.join("; "))
        }
    }
}
