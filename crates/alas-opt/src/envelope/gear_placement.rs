// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The main-gear placement rule of a redesigned candidate, posed on the very
//! loading states and ground mechanisms this envelope gates.
//!
//! A registered aircraft keeps its published gear stations. A candidate that
//! moves the wing, the fuselage or the payload has no published gear, and
//! conceptual design places it (Raymer, *Aircraft Design: A Conceptual
//! Approach*, 6th ed., sec. 11.2; Currey, *Aircraft Landing Gear Design:
//! Principles and Practices*, ch. 3): the main gear far enough aft that the
//! tip-back angle at the most-aft, highest centre of gravity clears both the
//! configured minimum and the tail-scrape angle, and that the static nose
//! reaction stays between its steering minimum and handling maximum at every
//! loading state, while the nose wheel can still be lifted at rotation. This
//! module keeps the published nose-gear station and the published leg
//! spacing and solves only the rigid translation of the main group, choosing
//! the feasible station nearest the published one.
//!
//! Frame: body axes, x aft from the nose tip, z up, metres; masses kg.
//!
//! The ledger is rebuilt at a probe translation and each state's moment
//! derivative is taken as the secant between the two ledgers, so every
//! coupling of the ledger to the gear station (the gear's own mass and any
//! payload placement that targets the empty-aircraft centre of gravity)
//! enters the solve with a value the ledger itself produces. The rotation
//! boundary is measured the same way, from the envelope's own physical limits
//! at the two stations, and taken as the secant between them. It is not
//! exactly affine: the pitch inertia about the main-gear contact grows with
//! the square of the CG offset (`super::rotation::rotation_cg_offset_m`).
//! Nor is the ledger: baggage fills discrete hold slots and a hold clamp can
//! bind, so its centre of gravity is piecewise in the translation and the
//! secant can miss the boundary by a slot step. The caller therefore
//! re-checks the re-sized candidate at the solved station and re-solves from
//! it when a mechanism is missed; the unchanged envelope remains the verdict.

use alas_config::{AlasConfig, DerivedMainGearStation};
use alas_geom::aircraft::airplane::Airplane;
use alas_perf::landing_gear::geometry::FuselageLowerPoint;
use alas_perf::landing_gear::placement::{
    solve_main_gear_station, ForwardCgBoundary, GearPlacementState, MainGearPlacementInput,
};

use super::ledger_basis::{
    assess_model_cg_envelope_from_states, minimum_nose_gear_fraction, registered_aft_cg_nose_load,
};
use super::loading::operational_loading_states_with_z;
use super::support::ground_z_m;
use super::{
    unmeasured_main_gear_station, LedgerLoadingBasis, ModelCgConstraint, ModelCgLoadingState,
    PhaseLimits,
};

/// The main-gear translation `config` already carries, m; zero for the
/// configured stations.
#[must_use]
pub fn configured_translation_m(config: &AlasConfig) -> f64 {
    config
        .landing_gear
        .derived_main_gear
        .map_or(0.0, |derived| derived.translation_m)
}

/// Outcome of the main-gear placement rule for one candidate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MainGearPlacement {
    /// The configured stations already satisfy every ground mechanism.
    Unchanged,
    /// Translate the whole main group by this many metres beyond the
    /// configured stations (which may already carry a translation), aft
    /// positive.
    Translated(f64),
    /// No station inside the installation interval satisfies every state.
    Infeasible,
}

/// Whether the rule requires nose-wheel liftoff at `state`: exactly where
/// the envelope verdict gates it ([`PhaseLimits::rotation`]).
fn rotation_gated(state: ModelCgLoadingState) -> bool {
    PhaseLimits::for_state(state).rotation
}

/// The two ledgers of one candidate, as configured and with the main group
/// translated by `probe_translation_m`, and the aerodynamic reference the
/// envelope frame is built on.
#[derive(Debug, Clone, Copy)]
pub struct PlacementLedgers {
    /// Ledger with the configured main-gear stations.
    pub configured: LedgerLoadingBasis,
    /// Ledger with every main leg translated by `probe_translation_m`.
    pub probe: LedgerLoadingBasis,
    /// The probe translation, m; finite and nonzero.
    pub probe_translation_m: f64,
    /// The candidate's neutral point, m. No ground boundary the rule reads
    /// depends on it; the envelope assessment requires one.
    pub neutral_point_x_m: f64,
    /// The mean aerodynamic chord of the envelope frame, m.
    pub mac_m: f64,
}

/// Leading edge and chord of `wing` at spanwise station `y_m`, m, linear
/// between its sections; the root section inboard of it and the tip outboard.
fn chord_at(wing: &alas_geom::aircraft::wing::Wing, y_m: f64) -> Option<(f64, f64)> {
    let sections = &wing.xsecs;
    let first = sections.first()?;
    if y_m <= first.xyz_le[1] {
        return Some((first.xyz_le[0], first.chord));
    }
    for pair in sections.windows(2) {
        let (inboard, outboard) = (&pair[0], &pair[1]);
        let span_m = outboard.xyz_le[1] - inboard.xyz_le[1];
        if y_m <= outboard.xyz_le[1] && span_m > 0.0 {
            let t = (y_m - inboard.xyz_le[1]) / span_m;
            return Some((
                inboard.xyz_le[0] + t * (outboard.xyz_le[0] - inboard.xyz_le[0]),
                inboard.chord + t * (outboard.chord - inboard.chord),
            ));
        }
    }
    sections.last().map(|tip| (tip.xyz_le[0], tip.chord))
}

/// The main-gear installation interval for the effective station, m.
///
/// Each wing leg (the outboard pair, `L`/`R`) must stay under the main
/// wing's chord at that leg's own spanwise station, `[x_LE(y), x_LE(y) +
/// c(y)]`, where a wing-mounted leg reacts into the wing box and a
/// fuselage-mounted (sponson) leg into the frames carrying the wing. A body
/// leg (`Body-*`: the centre leg of a three-leg group, the inboard pair of a
/// four-leg one) is a fuselage gear reacting into the keel and fuselage
/// frames behind the wing box (Currey, *Aircraft Landing Gear Design:
/// Principles and Practices*, ch. 3; the DC-10-30 centre gear stands 0.76 m
/// aft of the wing-gear bogies, DAC-67803A Rev A Figure 7.2.2, behind the
/// trailing edge of the wing's centreline chord), so its own interval is the
/// fuselage, `[x_nose, x_tail]`. The lateral stations and labels are the
/// gear sizing's own (`alas_perf::landing_gear::mlg_strut_positions` at half
/// the configured track). A configured leg outside its interval bounds its
/// own side at its configured station, so the rule never requires moving a
/// real aircraft's gear. The legs translate together, so each leg's interval
/// maps onto the effective station through its fixed offset.
fn installation_bounds_m(
    plane: &Airplane,
    config: &AlasConfig,
    effective_station_m: f64,
    main_legs_m: &[f64],
) -> Option<(f64, f64)> {
    let wing = plane
        .wings
        .iter()
        .find(|wing| wing.name == "Main Wing")
        .or_else(|| plane.wings.first())?;
    let half_track_m =
        config.geometry.fuselage.diameter_m * config.landing_gear.track_diameter_factor / 2.0;
    let count = i64::try_from(main_legs_m.len()).ok()?;
    let lateral = alas_perf::landing_gear::mlg_strut_positions(count, half_track_m);
    let fuselage = plane.fuselages.first()?;
    let body_m = (
        fuselage.xsecs.first()?.xyz_c[0],
        fuselage.xsecs.last()?.xyz_c[0],
    );
    let mut bounds = (f64::NEG_INFINITY, f64::INFINITY);
    for (index, &leg_m) in main_legs_m.iter().enumerate() {
        let (label, y_m) = lateral
            .get(index)
            .map_or(("", 0.0), |(label, y)| (label.as_str(), y.abs()));
        let (leading_edge_m, trailing_edge_m) = if label.starts_with("Body") {
            body_m
        } else {
            let (leading_edge_m, chord_m) = chord_at(wing, y_m)?;
            if chord_m <= 0.0 {
                return None;
            }
            (leading_edge_m, leading_edge_m + chord_m)
        };
        if !leading_edge_m.is_finite() || !trailing_edge_m.is_finite() {
            return None;
        }
        let offset_m = leg_m - effective_station_m;
        bounds.0 = bounds.0.max(leading_edge_m.min(leg_m) - offset_m);
        bounds.1 = bounds.1.min(trailing_edge_m.max(leg_m) - offset_m);
    }
    (bounds.0.is_finite() && bounds.1.is_finite() && bounds.0 <= bounds.1).then_some(bounds)
}

type LoadingStates = Vec<(ModelCgLoadingState, f64, f64, f64)>;

fn states_of(ledger: LedgerLoadingBasis) -> LoadingStates {
    let (payload_kg, payload_x, payload_z, fuel_kg, fuel_x, fuel_z) = ledger.payload_and_fuel();
    operational_loading_states_with_z(
        ledger.oew_mass_kg,
        ledger.oew_cg_x_m,
        ledger.oew_cg_z_m,
        payload_kg,
        payload_x,
        payload_z,
        fuel_kg,
        fuel_x,
        fuel_z,
        ledger.takeoff_cg_x_m,
    )
}

/// Each state's nose-wheel liftoff boundary as an affine (secant) function of
/// the effective main-gear station, `None` for a state whose phase has no
/// rotation. The coefficients come from the envelope's own boundary at the
/// configured and the probe stations.
fn rotation_boundaries(
    plane: &Airplane,
    ledgers: &PlacementLedgers,
    configured: (&LoadingStates, &AlasConfig),
    probe: (&LoadingStates, &AlasConfig),
    configured_station_m: f64,
) -> Result<Vec<Option<ForwardCgBoundary>>, String> {
    let x_lemac_m = plane
        .mac_frame()
        .ok_or_else(|| "gear placement requires a main wing".to_owned())?
        .x_lemac_m;
    let boundary_x = |states: &LoadingStates, ledger: &LedgerLoadingBasis, config: &AlasConfig| {
        assess_model_cg_envelope_from_states(
            plane,
            states.clone(),
            ledger.takeoff_pitch_inertia_kg_m2,
            ledger.takeoff_cg_x_m,
            ledgers.neutral_point_x_m,
            ledgers.neutral_point_x_m,
            ledgers.mac_m,
            config,
        )
        .map_err(|error| format!("gear placement envelope: {error}"))
        .map(|assessment| {
            assessment
                .loading_states
                .iter()
                .map(|state| {
                    rotation_gated(state.state).then(|| {
                        x_lemac_m
                            + state.physical_limits.rotation_fwd_pct_mac / 100.0 * ledgers.mac_m
                    })
                })
                .collect::<Vec<_>>()
        })
    };
    let at_configured = boundary_x(configured.0, &ledgers.configured, configured.1)?;
    let at_probe = boundary_x(probe.0, &ledgers.probe, probe.1)?;
    if at_configured.len() != configured.0.len() || at_probe.len() != at_configured.len() {
        return Err("gear placement envelope returned a different state list".to_owned());
    }
    Ok(at_configured
        .into_iter()
        .zip(at_probe)
        .map(|(base, moved)| {
            let (base, moved) = (base?, moved?);
            let slope = (moved - base) / ledgers.probe_translation_m;
            let boundary = ForwardCgBoundary {
                intercept_m: base - slope * configured_station_m,
                slope,
            };
            (boundary.intercept_m.is_finite() && boundary.slope.is_finite()).then_some(boundary)
        })
        .collect())
}

/// Whether the configured stations already meet every mechanism the rule
/// places the gear against, at every state of `configured`: tip-back, the
/// static nose-load window and the rotation boundary. `neutral_point_x_m`
/// and `mac_m` are as in [`PlacementLedgers`].
///
/// # Errors
///
/// The envelope failure, as a description.
pub fn configured_stations_suffice(
    plane: &Airplane,
    configured: LedgerLoadingBasis,
    neutral_point_x_m: f64,
    mac_m: f64,
    config: &AlasConfig,
) -> Result<bool, String> {
    let assessment = assess_model_cg_envelope_from_states(
        plane,
        states_of(configured),
        configured.takeoff_pitch_inertia_kg_m2,
        configured.takeoff_cg_x_m,
        neutral_point_x_m,
        neutral_point_x_m,
        mac_m,
        config,
    )
    .map_err(|error| format!("gear placement envelope: {error}"))?;
    Ok(assessment.loading_states.iter().all(|state| {
        let rotation_met = !rotation_gated(state.state)
            || state.cg_pct_mac >= state.physical_limits.rotation_fwd_pct_mac;
        rotation_met
            && state.constraints.iter().all(|constraint| {
                !constraint.violated
                    || !matches!(
                        constraint.constraint,
                        ModelCgConstraint::TipBack
                            | ModelCgConstraint::MinimumNoseGearLoad
                            | ModelCgConstraint::MaximumNoseGearLoadFraction
                    )
            })
    }))
}

/// Solve the placement rule for one candidate.
///
/// `config` must be the configuration the `configured` ledger was built
/// with (no derived translation). Every envelope state of the ledger takes
/// part: operating empty, zero fuel, mid-mission, reserve and takeoff.
///
/// # Errors
///
/// A description when the stations, ledgers or ground geometry are not
/// physical: unequal state masses between the two ledgers, a translated
/// mass outside `[0, M)`, or a missing surface. Such a candidate keeps its
/// configured stations and the envelope reports it.
pub fn place_main_gear(
    plane: &Airplane,
    ledgers: PlacementLedgers,
    config: &AlasConfig,
) -> Result<MainGearPlacement, String> {
    let fuselage = plane
        .fuselages
        .first()
        .ok_or_else(|| "gear placement requires a fuselage".to_owned())?;
    let frame = plane
        .mac_frame()
        .ok_or_else(|| "gear placement requires a main wing".to_owned())?;
    let start_x_m = fuselage
        .xsecs
        .first()
        .map(|section| section.xyz_c[0])
        .ok_or_else(|| "gear placement requires fuselage sections".to_owned())?;
    let end_x_m = fuselage
        .xsecs
        .last()
        .map(|section| section.xyz_c[0])
        .ok_or_else(|| "gear placement requires fuselage sections".to_owned())?;
    let length_m = end_x_m - start_x_m;
    // The same fallbacks the envelope resolves its stations with.
    let stations = config.landing_gear.resolved_station_positions(
        start_x_m + length_m * config.mass_model.nlg_x_fraction,
        frame.x_lemac_m + config.mass_model.mlg_x_fraction_mac * frame.chord_m,
        start_x_m,
        length_m,
    );
    if let Some(error) = unmeasured_main_gear_station(plane, config, &stations) {
        return Err(error.to_string());
    }
    let probe_m = ledgers.probe_translation_m;
    if !probe_m.is_finite() || probe_m == 0.0 {
        return Err("gear placement probe translation must be finite and nonzero".to_owned());
    }
    let Some(bounds) =
        installation_bounds_m(plane, config, stations.x_mlg_m, &stations.main_gear_x_m)
    else {
        return Err("gear placement has no wing-root installation interval".to_owned());
    };

    let configured = states_of(ledgers.configured);
    let probe = states_of(ledgers.probe);
    let mut probe_config = config.clone();
    probe_config.landing_gear.derived_main_gear = Some(DerivedMainGearStation {
        translation_m: configured_translation_m(config) + probe_m,
    });
    let rotation = rotation_boundaries(
        plane,
        &ledgers,
        (&configured, config),
        (&probe, &probe_config),
        stations.x_mlg_m,
    )?;
    let ground_m = ground_z_m(fuselage, config);
    let published_split = registered_aft_cg_nose_load(config);
    let mut placement_states = Vec::with_capacity(configured.len());
    for ((&(state, x_m, z_m, mass_kg), &(probe_state, probe_x_m, _, probe_mass_kg)), boundary) in
        configured.iter().zip(&probe).zip(rotation)
    {
        // Moving gear legs moves no mass between states: the two ledgers
        // must agree on every state mass to their own summation round-off.
        let roundoff_kg = 64.0 * f64::EPSILON * mass_kg.abs();
        if state != probe_state || (mass_kg - probe_mass_kg).abs() > roundoff_kg {
            return Err(format!(
                "gear placement ledgers disagree on the {state:?} mass: {mass_kg} kg against {probe_mass_kg} kg"
            ));
        }
        // The mass that effectively travels with the main group in this
        // state: the moment derivative of the rebuilt ledger.
        let translated_kg = mass_kg * (probe_x_m - x_m) / probe_m;
        if !translated_kg.is_finite() || translated_kg < 0.0 || translated_kg >= mass_kg {
            return Err(format!(
                "gear placement moment derivative {translated_kg} kg is outside [0, {mass_kg}) kg at {state:?}"
            ));
        }
        placement_states.push(GearPlacementState {
            mass_kg,
            main_gear_mass_kg: translated_kg,
            moment_without_main_gear_kg_m: mass_kg * x_m - translated_kg * stations.x_mlg_m,
            cg_height_m: z_m - ground_m,
            minimum_nose_fraction: minimum_nose_gear_fraction(config, published_split, mass_kg),
            maximum_nose_fraction: config.mass_model.pct_load_nlg_max_handling,
            forward_cg_boundary: boundary,
        });
    }
    let contour: Vec<FuselageLowerPoint> = fuselage
        .xsecs
        .iter()
        .map(|section| FuselageLowerPoint {
            x_m: section.xyz_c[0],
            z_bottom_m: section.xyz_c[2] - section.height / 2.0,
        })
        .collect();
    // The envelope measures tip-back from the most-aft main axle, which is
    // never ahead of the effective station; solving at the effective station
    // is therefore exact for a uniform group and conservative otherwise.
    let solved = solve_main_gear_station(&MainGearPlacementInput {
        nose_gear_x_m: stations.x_nlg_m,
        requested_main_gear_x_m: stations.x_mlg_m,
        installation_bounds_m: bounds,
        minimum_tip_back_deg: config.landing_gear.min_tip_back_deg,
        ground_z_m: ground_m,
        lower_contour: &contour,
        states: &placement_states,
    })
    .map_err(|error| error.to_string())?;
    Ok(match solved {
        None => MainGearPlacement::Infeasible,
        Some(station_m) if station_m == stations.x_mlg_m => MainGearPlacement::Unchanged,
        Some(station_m) => MainGearPlacement::Translated(station_m - stations.x_mlg_m),
    })
}
