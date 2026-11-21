// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Where a main gear can stand: the interval the placement rule solves in.
//! Frame: body axes, x aft from the nose tip, z up, metres.

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec};

/// Keel rise over the deepest body section below which a fuselage-mounted leg
/// keeps its published length: 2 %, about a tenth of a metre on a transport
/// body, the order of the tyre deflection and of the outline reading. Not a
/// fit: no preset's interval is set by it, published stations stay inside.
const KEEL_LEVEL_DEPTH_FRACTION: f64 = 0.02;

/// Leading edge and chord of `wing` at spanwise station `y_m`, m, linear
/// between its sections; the root section inboard of it and the tip outboard.
fn chord_at(wing: &alas_geom::aircraft::wing::Wing, y_m: f64) -> Option<(f64, f64)> {
    // A leg reacts into the box, which inside the body is the carried one.
    let wing = wing.carried_box();
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

/// Whether the wing root stands above the body axis, out of reach of a main
/// gear leg: the gear of such an aircraft is carried by the fuselage, in
/// sponsons (A400M, C295, ATR 72), whatever its leg labels say.
fn wing_above_body_axis(wing: &alas_geom::aircraft::wing::Wing, body: &Fuselage) -> Option<bool> {
    let carried = wing.carried_box();
    let root_le = carried.xsecs.first()?.xyz_le;
    let centre = body
        .xsecs
        .iter()
        .min_by(|a, b| {
            let distance = |s: &&FuselageXSec| (s.xyz_c[0] - root_le[0]).abs();
            distance(a).total_cmp(&distance(b))
        })?
        .xyz_c[2];
    Some(root_le[2] > centre)
}

/// The stations around `leg_m` where a fuselage-mounted leg keeps its length:
/// the run of body stations whose keel stands no more than
/// [`KEEL_LEVEL_DEPTH_FRACTION`] of the deepest section above the keel at the
/// leg. Ahead of it the nose rises and aft of it the rear fuselage does (the
/// A400M ramp, a tail-scrape upsweep): a leg there is a longer leg, a different
/// gear. `leg_m` itself is always inside.
fn level_keel_interval_m(body: &Fuselage, leg_m: f64) -> Option<(f64, f64)> {
    let sections = &body.xsecs;
    let keel = |s: &FuselageXSec| s.xyz_c[2] - 0.5 * s.height;
    let depth_m = sections.iter().map(|s| s.height).fold(0.0, f64::max);
    let next = sections.iter().position(|s| s.xyz_c[0] >= leg_m)?;
    let previous = next.saturating_sub(1);
    let span_m = sections[next].xyz_c[0] - sections[previous].xyz_c[0];
    let t = if span_m > 0.0 {
        (leg_m - sections[previous].xyz_c[0]) / span_m
    } else {
        0.0
    };
    let limit_m = keel(&sections[previous]) * (1.0 - t)
        + keel(&sections[next]) * t
        + KEEL_LEVEL_DEPTH_FRACTION * depth_m;
    let level = |s: &&FuselageXSec| keel(s) <= limit_m;
    let aft = sections[next..].iter().take_while(level).last();
    let fore = sections[..=previous].iter().rev().take_while(level).last();
    Some((
        fore.map_or(leg_m, |s| s.xyz_c[0].min(leg_m)),
        aft.map_or(leg_m, |s| s.xyz_c[0].max(leg_m)),
    ))
}

/// The main-gear installation interval for the effective station, m.
///
/// A leg the wing can carry (`L`/`R` and the further rows of the same line,
/// under a wing at or below the body axis) must stay under the main wing's
/// chord at that leg's own spanwise station, `[x_LE(y), x_LE(y) + c(y)]`, where
/// it reacts into the wing box. Every other leg is fuselage-mounted: a body leg
/// (`Body-*`: the centre leg of a three-leg group, the inboard pair of a
/// four-leg one) reacts into the keel and fuselage frames behind the wing box
/// (Currey, *Aircraft Landing Gear Design: Principles and Practices*, ch. 3;
/// the DC-10-30 centre gear stands 0.76 m aft of the wing-gear bogies,
/// DAC-67803A Rev A Figure 7.2.2, behind the trailing edge of the wing's
/// centreline chord), and all the legs under a high wing into the frames of
/// their sponsons. A fuselage-mounted leg's interval is the level run of the
/// keel around it, [`level_keel_interval_m`]. The lateral stations and labels
/// are the gear sizing's own (`alas_perf::landing_gear::mlg_strut_positions` at
/// half the configured track). A configured leg outside its interval bounds its
/// own side at its configured station, so the rule never requires moving a
/// real aircraft's gear. The legs translate together, so each leg's interval
/// maps onto the effective station through its fixed offset.
pub(super) fn installation_bounds_m(
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
    let lateral = alas_perf::landing_gear::mlg_strut_positions(
        count,
        half_track_m,
        config.landing_gear.wheels_per_mlg_strut,
    );
    let fuselage = plane.fuselages.first()?;
    let high_wing = wing_above_body_axis(wing, fuselage)?;
    let mut bounds = (f64::NEG_INFINITY, f64::INFINITY);
    for (index, &leg_m) in main_legs_m.iter().enumerate() {
        let (label, y_m) = lateral
            .get(index)
            .map_or(("", 0.0), |(label, y)| (label.as_str(), y.abs()));
        let (leading_edge_m, trailing_edge_m) = if high_wing || label.starts_with("Body") {
            level_keel_interval_m(fuselage, leg_m)?
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

/// How far the main-gear group of a redesigned `candidate` moves with its
/// wing, m (aft positive), from its own published stations
/// `candidate_legs_m`: each leg the wing carries keeps the fraction of the
/// local chord, at its own spanwise station, at which it stands
/// (`registered_legs_m`) under the registered wing, and the group (body legs
/// included, at their published spacing) translates by the mean of those
/// moves.
///
/// A wing-mounted main gear hangs from the rear spar and the gear beam
/// behind it (Torenbeek, *Synthesis of Subsonic Airplane Design*, 1982,
/// ch. 10, undercarriage layout; Currey, *Aircraft Landing Gear Design:
/// Principles and Practices*, ch. 3): it goes where the wing box goes. Left
/// at its fuselage station while the wing moved and its chord shrank, the
/// gear of a redesigned B747-400 stood at 63 % of the mean aerodynamic chord
/// against 45 % registered, and the nose-load and tip-back limits it sets
/// moved 16 % of the chord aft with it, which the search used as
/// centre-of-gravity room.
///
/// `None` when no leg is carried by the wing (a high wing, whose gear is in
/// the fuselage sponsons), when a wing leg does not stand under the
/// registered chord, or when the geometry is not resolvable: the group then
/// keeps its fuselage stations.
pub(super) fn wing_follow_translation_m(
    registered: &Airplane,
    candidate: &Airplane,
    config: &AlasConfig,
    registered_legs_m: &[f64],
    candidate_legs_m: &[f64],
) -> Option<f64> {
    if registered_legs_m.len() != candidate_legs_m.len() {
        return None;
    }
    let main_legs_m = registered_legs_m;
    let main = |plane: &Airplane| {
        plane
            .wings
            .iter()
            .find(|wing| wing.name == "Main Wing")
            .or_else(|| plane.wings.first())
            .cloned()
    };
    let (registered_wing, candidate_wing) = (main(registered)?, main(candidate)?);
    if wing_above_body_axis(&registered_wing, registered.fuselages.first()?)? {
        return None;
    }
    let half_track_m =
        config.geometry.fuselage.diameter_m * config.landing_gear.track_diameter_factor / 2.0;
    let lateral = alas_perf::landing_gear::mlg_strut_positions(
        i64::try_from(main_legs_m.len()).ok()?,
        half_track_m,
        config.landing_gear.wheels_per_mlg_strut,
    );
    let mut moves = Vec::new();
    for (index, &leg_m) in main_legs_m.iter().enumerate() {
        let Some((label, y_m)) = lateral
            .get(index)
            .map(|(label, y)| (label.as_str(), y.abs()))
        else {
            continue;
        };
        if label.starts_with("Body") {
            continue;
        }
        let (registered_le_m, registered_chord_m) = chord_at(&registered_wing, y_m)?;
        let (candidate_le_m, candidate_chord_m) = chord_at(&candidate_wing, y_m)?;
        if registered_chord_m <= 0.0 || candidate_chord_m <= 0.0 {
            return None;
        }
        let fraction = (leg_m - registered_le_m) / registered_chord_m;
        if !(0.0..=1.0).contains(&fraction) {
            return None;
        }
        moves.push(candidate_le_m + fraction * candidate_chord_m - candidate_legs_m[index]);
    }
    if moves.is_empty() {
        return None;
    }
    let mean = moves.iter().sum::<f64>() / moves.len() as f64;
    mean.is_finite().then_some(mean)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::builder::AircraftBuilder;

    /// The published main legs of `name` and the aircraft built from
    /// `design` under its preset configuration.
    fn legs_and_plane(
        name: &str,
        design: &alas_config::DesignVector,
    ) -> (AlasConfig, Vec<f64>, Airplane) {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(design), false)
            .unwrap();
        let body = &plane.fuselages[0];
        let (start_m, end_m) = (
            body.xsecs[0].xyz_c[0],
            body.xsecs[body.xsecs.len() - 1].xyz_c[0],
        );
        let frame = plane.mac_frame().unwrap();
        let stations = config.landing_gear.resolved_station_positions(
            start_m + (end_m - start_m) * config.mass_model.nlg_x_fraction,
            frame.x_lemac_m + config.mass_model.mlg_x_fraction_mac * frame.chord_m,
            start_m,
            end_m - start_m,
        );
        (config, stations.main_gear_x_m, plane)
    }

    /// A wing-mounted gear goes with the wing: by the wing's own move when
    /// the chord is unchanged, by the move of its chord fraction when the
    /// chord shrinks, and not at all on the registered wing or under a high
    /// wing, whose gear is in the fuselage.
    #[test]
    fn a_wing_mounted_main_gear_keeps_its_chord_fraction_on_a_redesigned_wing() {
        for name in ["A320-200", "B747-400"] {
            let registered_design = alas_config::presets::get(name).unwrap().design_vector;
            let (config, legs, registered) = legs_and_plane(name, &registered_design);
            assert_eq!(
                wing_follow_translation_m(&registered, &registered, &config, &legs, &legs),
                Some(0.0),
                "{name}"
            );
            let mut moved = registered_design;
            moved.wing_x_shift_m -= 0.6;
            let (_, _, forward) = legs_and_plane(name, &moved);
            let translation =
                wing_follow_translation_m(&registered, &forward, &config, &legs, &legs).unwrap();
            assert!((translation + 0.6).abs() < 1e-9, "{name}: {translation}");
            // Shorter chords about the same leading edge: the legs stand
            // behind mid-chord, so they come forward, by less than the chord
            // loses.
            let mut shorter = registered_design;
            shorter.root_chord_m *= 0.9;
            shorter.break_chord_m *= 0.9;
            shorter.tip_chord_m *= 0.9;
            let (_, _, narrow) = legs_and_plane(name, &shorter);
            let translation =
                wing_follow_translation_m(&registered, &narrow, &config, &legs, &legs).unwrap();
            let lost_m = 0.1 * registered_design.root_chord_m;
            assert!(
                translation < -0.3 * lost_m && translation > -lost_m,
                "{name}: {translation} m for {lost_m} m of root chord"
            );
        }
        for name in ["ATR72-600", "C295", "A400M"] {
            let registered_design = alas_config::presets::get(name).unwrap().design_vector;
            let (config, legs, registered) = legs_and_plane(name, &registered_design);
            let mut moved = registered_design;
            moved.wing_x_shift_m -= 0.3;
            let (_, _, forward) = legs_and_plane(name, &moved);
            assert_eq!(
                wing_follow_translation_m(&registered, &forward, &config, &legs, &legs),
                None,
                "{name}"
            );
        }
    }

    /// Every published main leg stays inside the interval of the effective
    /// station, each leg's label and lateral station are the row they stand
    /// in, and a high wing puts the whole group on the level keel.
    // The test log carries the stations behind the assertions.
    #[allow(clippy::print_stdout)]
    #[test]
    fn the_installation_interval_fits_every_preset_with_more_than_one_row() {
        for name in [
            "A340-300",
            "A380-800",
            "B747-400",
            "DC-10",
            "A400M",
            "ATR72-600",
            "C295",
        ] {
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
            let plane = AircraftBuilder::new(Some(config.geometry.clone()))
                .build(
                    Some(&alas_config::presets::get(name).unwrap().design_vector),
                    false,
                )
                .unwrap();
            let body = &plane.fuselages[0];
            let (start_m, end_m) = (
                body.xsecs[0].xyz_c[0],
                body.xsecs[body.xsecs.len() - 1].xyz_c[0],
            );
            let frame = plane.mac_frame().unwrap();
            let stations = config.landing_gear.resolved_station_positions(
                start_m + (end_m - start_m) * config.mass_model.nlg_x_fraction,
                frame.x_lemac_m + config.mass_model.mlg_x_fraction_mac * frame.chord_m,
                start_m,
                end_m - start_m,
            );
            let legs = &stations.main_gear_x_m;
            let bounds = installation_bounds_m(&plane, &config, stations.x_mlg_m, legs).unwrap();
            let half_track_m = config.geometry.fuselage.diameter_m
                * config.landing_gear.track_diameter_factor
                / 2.0;
            let lateral = alas_perf::landing_gear::mlg_strut_positions(
                i64::try_from(legs.len()).unwrap(),
                half_track_m,
                config.landing_gear.wheels_per_mlg_strut,
            );
            println!(
                "{name}: effective {:.2} bounds {:.2} .. {:.2}",
                stations.x_mlg_m, bounds.0, bounds.1
            );
            for ((label, y_m), x_m) in lateral.iter().zip(legs) {
                println!("    {label:7} y {y_m:6.2} x {x_m:6.2}");
            }
            assert_eq!(lateral.len(), legs.len(), "{name}");
            assert!(
                lateral.iter().map(|leg| leg.1).sum::<f64>().abs() < 1e-9,
                "{name}"
            );
            assert!(
                bounds.0 <= stations.x_mlg_m && stations.x_mlg_m <= bounds.1,
                "{name}"
            );
        }
    }

    #[test]
    fn a_leg_stops_where_the_keel_rises() {
        let section = |x: f64, keel: f64| FuselageXSec {
            xyz_c: [x, 0.0, keel + 2.5],
            width: 5.0,
            height: 5.0,
            shape: 2.0,
        };
        let body = Fuselage::new(
            "Fuselage",
            vec![
                section(0.0, -1.0),
                section(5.0, -2.5),
                section(10.0, -2.5),
                section(20.0, -2.5),
                section(25.0, -2.0),
                section(30.0, -1.0),
            ],
        );
        assert_eq!(level_keel_interval_m(&body, 12.0), Some((5.0, 20.0)));
        // A leg already on the rising keel keeps its own station as the aft limit.
        let (_, aft) = level_keel_interval_m(&body, 27.0).unwrap();
        assert!((27.0..=30.0).contains(&aft));
    }
}
