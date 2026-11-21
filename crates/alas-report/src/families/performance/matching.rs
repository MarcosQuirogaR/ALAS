// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/sidecar/figures_extra.py (`figure_matching_chart`).

//! The configuration- and report-driven aircraft sizing matching chart.

use alas_config::airports::Airport;
use alas_config::AlasConfig;
use alas_opt::mdo::{second_segment_evidence, EngineOutModel, YawAirframe};
use alas_perf::performance::{
    assess_oei_climb, build_matching_chart, compute_v_speeds, density_ratio, far25_oei_gradient,
    landing_runway_share, oei_cl_at_v2, ws_landing_limit_for, MatchingChartData,
};
use alas_pipeline::feasibility::landing_mass_limit_kg;
use alas_pipeline::field_reference::candidate_field_polar;
use alas_pipeline::full_analysis::AnalysisReport;
use alas_pipeline::report_config::config_for_report;

use crate::chart_kit::{draw_legend, draw_title, LegendMarker};
use crate::families::performance::support::{
    resolve_arrival, resolve_departure, static_thrust_to_weight, status_message_scene, G,
};
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

/// Fill colour shared by the plotted design-point marker and its legend swatch.
const DESIGN_POINT_COLOUR: &str = "#f1c40f";

const AIRPORT_COLOURS: [&str; 6] = [
    "#e74c3c", "#e67e22", "#f1c40f", "#2ecc71", "#1abc9c", "#9b59b6",
];

/// The status scene of a report whose airplane has no usable main wing.
fn no_main_wing_scene(report: &AnalysisReport, theme: Option<&str>) -> Option<Scene> {
    report
        .airplane
        .wings
        .first()
        .filter(|wing| wing.xsecs.len() >= 2)
        .is_none()
        .then(|| {
            status_message_scene(
                "Matching Chart",
                "The analyzed report has no main wing; matching constraints cannot be computed.",
                theme,
            )
        })
}

/// Generate the matching chart from the analyzed polar, live geometry and
/// configured requirements/field constraints, for the configured aerodromes
/// with the stated design field lengths applied (standalone reports).
///
/// A dispatched route draws through [`figure_matching_chart_for_airports`]
/// with the endpoints it flew, as the landing-and-take-off figures do.
pub fn figure_matching_chart(
    report: &AnalysisReport,
    config: &AlasConfig,
    theme: Option<&str>,
) -> Scene {
    // The geometry guard comes first, as it always has: a report with no main
    // wing says so whatever the airports resolve to.
    if let Some(scene) = no_main_wing_scene(report, theme) {
        return scene;
    }
    let (Ok(departure), Ok(arrival)) = (resolve_departure(config), resolve_arrival(config)) else {
        return status_message_scene(
            "Matching Chart",
            "Departure/arrival airport not found in the database.",
            theme,
        );
    };
    figure_matching_chart_for_airports(report, config, departure, arrival, theme)
}

/// The matching chart for explicit aerodromes, which already carry any stated
/// field length (see `AlasConfig::with_stated_for_route`).
pub fn figure_matching_chart_for_airports(
    report: &AnalysisReport,
    config: &AlasConfig,
    departure: &Airport,
    arrival: &Airport,
    theme: Option<&str>,
) -> Scene {
    if let Some(scene) = no_main_wing_scene(report, theme) {
        return scene;
    }

    // The sized takeoff mass when the report carries one; the declared MTOW
    // is only the unsized fallback.
    let takeoff_mass_kg = report.analysis_takeoff_mass_kg(config.requirements.mtow_kg);
    let wing_area = report
        .geometry_summary
        .get("wing_area_m2")
        .copied()
        .unwrap_or(report.airplane.s_ref);
    // The V speeds use the computed minimum control speed (CS 25.107), like
    // the feasibility gate; the declared ratio is only the fallback.
    let resolved = config_for_report(
        config,
        report,
        takeoff_mass_kg,
        landing_mass_limit_kg(config, report),
        wing_area,
    );
    let config = &*resolved;
    let performance = &config.performance;
    let requirements = &config.requirements;
    let n_engines = config.geometry.engine.spanwise_positions_m.len() as i64;
    let oei_gradient = far25_oei_gradient(n_engines).unwrap_or(performance.oei_gradient);
    let tw_design = static_thrust_to_weight(config, takeoff_mass_kg, 0.30);
    let v_speeds = compute_v_speeds(
        takeoff_mass_kg,
        wing_area,
        departure,
        performance.cl_max_to,
        performance.cl_max_land,
        performance,
    );
    let Some(cl_v2) = oei_cl_at_v2(
        performance.cl_max_to,
        v_speeds.v2_ms / v_speeds.v_stall_to_ms,
    ) else {
        return status_message_scene("Matching Chart", "Departure V2 lift is unavailable.", theme);
    };
    let departure_atmosphere =
        alas_atmo::us1976_compute_values(departure.elevation_m, departure.isa_deviation_c);
    let field_polar = candidate_field_polar(
        config,
        report,
        cl_v2,
        v_speeds.v2_ms / departure_atmosphere.speed_of_sound_m_s,
        departure.elevation_m,
        departure.isa_deviation_c,
    );
    let (cd0, k) = match field_polar {
        Ok(polar) => polar,
        Err(reason) => return status_message_scene("Matching Chart", &reason, theme),
    };
    // The V2 point is the climb-out locus: the take-off flaps are in it, and
    // so is the failed engine where its source states the reduction.
    let v2_polar = alas_pipeline::field_reference::v2_climb_out_polar(config, report);
    let artifacts = match report.fuel.artifacts(config, &report.design) {
        Ok(artifacts) => artifacts,
        Err(reason) => return status_message_scene("Matching Chart", &reason, theme),
    };
    let cruise_atmosphere = alas_atmo::Atmosphere::new(requirements.cruise_altitude_m);
    let cruise_speed = requirements.cruise_mach * cruise_atmosphere.speed_of_sound();
    let cruise_q = 0.5 * cruise_atmosphere.density() * cruise_speed * cruise_speed;
    if !cruise_q.is_finite()
        || cruise_q <= 0.0
        || !performance.thrust_lapse.is_finite()
        || performance.thrust_lapse <= 0.0
    {
        return status_message_scene(
            "Matching Chart",
            "Cruise atmosphere or lapse is invalid.",
            theme,
        );
    }
    let mut data = build_matching_chart(
        cd0,
        k,
        requirements.cruise_mach,
        requirements.cruise_altitude_m,
        takeoff_mass_kg,
        wing_area,
        n_engines,
        &[departure.clone(), arrival.clone()],
        Some(performance.takeoff_field_cl_max(performance.cl_max_to)),
        Some(performance.cl_max_land),
        Some(performance.thrust_lapse),
        Some(oei_gradient),
        Some(performance.k_land),
        Some(cl_v2),
        Some(0.0),
        Some(tw_design),
        performance.matching_chart_resolution,
        Some(performance.ws_min_pa),
        Some(performance.ws_max_pa),
    );
    // Each wing loading has its own cruise CL. The field point projection
    // above is valid only at V2, so replace the compatibility cruise curve
    // with direct evaluations of the same candidate drag used for fuel. The
    // line is the level-cruise check's own requirement (engine deck, plan
    // levels and masses) whenever that check is the active one; the flat
    // thrust lapse at the take-off-mass lift is drawn only when it is.
    let design_ws_pa = takeoff_mass_kg * requirements.gravity_m_s2 / wing_area;
    let deck_line =
        alas_pipeline::feasibility::planned_levels(config, report, takeoff_mass_kg, wing_area)
            .and_then(|levels| {
                deck_cruise_line(
                    &data.ws_pa,
                    design_ws_pa,
                    &levels,
                    requirements.gravity_m_s2 / artifacts.reference_area_m2,
                    &|cl, mach, altitude_m| artifacts.drag.cd(cl, mach, altitude_m),
                )
            });
    let cruise_label = if deck_line.is_some() {
        CRUISE_LABEL_DECK
    } else {
        CRUISE_LABEL_FLAT_LAPSE
    };
    data.tw_cruise = deck_line.unwrap_or_else(|| {
        data.ws_pa
            .iter()
            .map(|&ws| {
                let cl = ws * wing_area / artifacts.reference_area_m2 / cruise_q;
                artifacts
                    .drag
                    .cd(cl, requirements.cruise_mach, requirements.cruise_altitude_m)
                    / cl
                    / performance.thrust_lapse
            })
            .collect()
    });

    let landing_mass_kg = landing_mass_limit_kg(config, report);
    if !performance.legacy_field_correlations {
        // The chart builder knows only the k_land correlation; the production
        // limit is the landing segment model shared with the gate and residual,
        // the propeller model and runway share for a turboprop.
        let propeller = matches!(
            config.geometry.engine.active_model(),
            Ok(alas_config::ActiveEngineModel::Turboprop(_))
        );
        let dry_share = alas_pipeline::field_performance::dry_landing_distance_share(config);
        for ((_, limit), airport) in data.ws_land_limits.iter_mut().zip([&departure, &arrival]) {
            let sigma = density_ratio(airport.elevation_m, airport.isa_deviation_c);
            *limit = ws_landing_limit_for(performance, propeller, airport.lda_m, dry_share, sigma);
        }
    }
    for (_, limit) in &mut data.ws_land_limits {
        *limit = landing_limit_on_takeoff_axis(
            *limit,
            landing_mass_kg,
            takeoff_mass_kg,
            performance.wet_landing_runway,
        );
    }

    // The chart's y-axis is installed sea-level-static T/W.  A generic cruise
    // lapse is not a valid conversion for the OEI V2 condition: the thrust
    // ratio, failed-engine drag and asymmetric trim drag come from the same
    // evidence builder as the optimizer residual and the feasibility finding.
    let static_per_engine_n =
        tw_design * takeoff_mass_kg * requirements.gravity_m_s2 / n_engines.max(1) as f64;
    let engine_out = EngineOutModel::new(
        config,
        Some(YawAirframe {
            plane: &report.airplane,
            cg_x_m: report.physical_cg[0],
        }),
        wing_area,
        Some(static_per_engine_n),
    )
    .ok();
    let evidence = second_segment_evidence(
        engine_out.as_ref(),
        performance,
        departure,
        &v_speeds,
        &|mach, altitude_m, isa_deviation_c| {
            artifacts
                .drag
                .cd_at_atmosphere(0.0, mach, altitude_m, isa_deviation_c)
        },
    );
    data.oei_climb_assessment = assess_oei_climb(
        cd0,
        k,
        n_engines,
        oei_gradient,
        cl_v2,
        0.0,
        performance.cl_max_to,
        Some(v2_polar.engine_out_condition(evidence.condition)),
    );

    draw_matching_chart(&data, oei_gradient, cruise_label, theme)
}

/// Legend entry of the cruise line drawn from the level-cruise check's own
/// requirement: the engine deck at the maximum-cruise rating over the levels
/// and masses of the mission plan.
const CRUISE_LABEL_DECK: &str = "Cruise, engine deck at plan levels (T/W0 floor)";
/// Legend entry of the cruise line when the flat thrust lapse is the active
/// check: cruise `D/W` at the take-off mass over the configured lapse.
const CRUISE_LABEL_FLAT_LAPSE: &str = "Cruise, flat thrust lapse (T/W0 floor)";

/// The level-cruise requirement along the wing-loading axis `ws_pa` (take-off
/// wing loading, Pa): required installed sea-level-static `T/W` over the
/// take-off weight.
///
/// Each plan level `i` carries the requirement `tw_i` the check found at the
/// design wing loading `design_ws_pa`, with its mass, Mach and altitude. The
/// deck thrust ratio of a level does not depend on the wing area, so at another
/// wing loading the requirement moves with the drag-to-lift ratio alone:
///
/// `tw_i(ws) = tw_i (CD(CL)/CL) / (CD(CL_i)/CL_i)`, `CL = CL_i ws / design_ws`,
///
/// `CL_i = m_i g / (q_i S_ref)` on the drag model's reference area
/// (`gravity_over_reference_area` = `g / S_ref`, m/s^2 per m^2) and `q_i` the
/// dynamic pressure of the level on its day. The line is the largest of the
/// levels, as the check takes the governing one, and passes through the
/// check's value at the design wing loading. `None` when no level gives a
/// finite positive requirement.
fn deck_cruise_line(
    ws_pa: &[f64],
    design_ws_pa: f64,
    levels: &[alas_opt::mdo::PlannedCruiseLevel],
    gravity_over_reference_area: f64,
    cd: &dyn Fn(f64, f64, f64) -> f64,
) -> Option<Vec<f64>> {
    // (required T/W, CL, drag-to-lift ratio, Mach, altitude) at the design point.
    let anchors: Vec<(f64, f64, f64, f64, f64)> = levels
        .iter()
        .filter_map(|level| {
            let air = alas_atmo::us1976_compute_values(level.altitude_m, level.isa_deviation_c);
            let speed = level.mach * air.speed_of_sound_m_s;
            let q = 0.5 * air.density_kg_m3 * speed * speed;
            let cl = level.mass_kg * gravity_over_reference_area / q;
            let drag_to_lift = cd(cl, level.mach, level.altitude_m) / cl;
            let usable = [level.required_tw, cl, drag_to_lift]
                .iter()
                .all(|value| value.is_finite() && *value > 0.0);
            usable.then_some((
                level.required_tw,
                cl,
                drag_to_lift,
                level.mach,
                level.altitude_m,
            ))
        })
        .collect();
    if anchors.is_empty() || !(design_ws_pa.is_finite() && design_ws_pa > 0.0) {
        return None;
    }
    Some(
        ws_pa
            .iter()
            .map(|&ws| {
                anchors
                    .iter()
                    .map(
                        |&(required_tw, cl_design, drag_to_lift, mach, altitude_m)| {
                            let cl = cl_design * ws / design_ws_pa;
                            required_tw * (cd(cl, mach, altitude_m) / cl) / drag_to_lift
                        },
                    )
                    .fold(f64::NAN, f64::max)
            })
            .collect(),
    )
}

/// A landing wing-loading limit on the chart's take-off wing-loading axis.
///
/// The limit is a wing loading at the maximum landing mass, while the x-axis
/// is the take-off wing loading: the same landing condition sits at
/// `W_TO/S = limit * MTOW/MLW` on that axis (FLD-06). A wet runway
/// (CAT.POL.A.235) shrinks the usable share of the LDA by 1/1.15 first. An
/// unusable landing mass leaves the limit on its own axis, unconverted.
fn landing_limit_on_takeoff_axis(
    limit_pa: f64,
    landing_mass_kg: f64,
    takeoff_mass_kg: f64,
    wet_runway: bool,
) -> f64 {
    let limit_pa = limit_pa * landing_runway_share(1.0, wet_runway);
    if landing_mass_kg.is_finite() && landing_mass_kg > 0.0 && takeoff_mass_kg > 0.0 {
        limit_pa * takeoff_mass_kg / landing_mass_kg
    } else {
        limit_pa
    }
}

fn draw_matching_chart(
    data: &MatchingChartData,
    oei_gradient: f64,
    cruise_label: &str,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    if data.ws_pa.is_empty() {
        return status_message_scene(
            "Matching Chart",
            "Matching chart resolution produced no wing-loading points.",
            theme,
        );
    }

    let ws_kg: Vec<f64> = data.ws_pa.iter().map(|ws| ws / G).collect();
    let oei_sls_floor = data
        .oei_climb_assessment
        .required_sls_tw
        .filter(|value| value.is_finite() && *value >= 0.0);
    let mut tw_floor = data.tw_cruise.clone();
    if let Some(oei_sls_floor) = oei_sls_floor {
        for value in &mut tw_floor {
            *value = value.max(oei_sls_floor);
        }
    }
    for (_, curve) in &data.tw_takeoff {
        for (floor, value) in tw_floor.iter_mut().zip(curve) {
            *floor = floor.max(*value);
        }
    }
    let floor_max = tw_floor
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .fold(oei_sls_floor.unwrap_or(0.0), f64::max);
    // Preserve f64::min/max NaN handling used by the translated calculation.
    #[allow(clippy::manual_clamp)]
    let y_max = (floor_max * 1.4).min(0.6).max(0.1);

    // Wide enough for the longest legend entry (airport names, off-scale
    // landing limits) at the figure's point size without clipping.
    let mut scene = Scene::new(1060.0, 600.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Matching Chart".to_owned());
    draw_title(&mut scene, "Matching Chart", pal);
    scene.suppress_derived_title();
    let axes = Axes2D::new(
        (80.0, 45.0, 565.0, 410.0),
        (ws_kg[0], *ws_kg.last().unwrap_or(&ws_kg[0])),
        (0.0, y_max),
    );

    axes.add_line_series(
        &mut scene,
        &ws_kg
            .iter()
            .copied()
            .zip(data.tw_cruise.iter().copied())
            .collect::<Vec<_>>(),
        Stroke::new(Color::from_hex("#3498db"), 2.0),
    );
    if let Some(oei_sls_floor) = oei_sls_floor {
        axes.add_line_series(
            &mut scene,
            &[
                (ws_kg[0], oei_sls_floor),
                (*ws_kg.last().unwrap_or(&ws_kg[0]), oei_sls_floor),
            ],
            Stroke::dashed(Color::from_hex("#9b59b6"), 1.8, 5.0, 4.0),
        );
    }
    for (index, (name, curve)) in data.tw_takeoff.iter().enumerate() {
        let colour = Color::from_hex(AIRPORT_COLOURS[index % AIRPORT_COLOURS.len()]);
        axes.add_line_series(
            &mut scene,
            &ws_kg
                .iter()
                .copied()
                .zip(curve.iter().copied())
                .collect::<Vec<_>>(),
            Stroke::dashed(colour, 1.6, 6.0, 3.0),
        );
        if let Some((_, limit)) = data
            .ws_land_limits
            .iter()
            .find(|(airport, _)| airport == name)
        {
            axes.add_line_series(
                &mut scene,
                &[(*limit / G, 0.0), (*limit / G, y_max)],
                Stroke::dashed(colour, 1.4, 2.0, 4.0),
            );
        }
    }

    let mut fill_points: Vec<[f64; 2]> = ws_kg
        .iter()
        .copied()
        .zip(tw_floor.iter().copied())
        .map(|(ws, tw)| axes.map_point(ws, tw))
        .collect();
    fill_points.extend(ws_kg.iter().rev().map(|ws| axes.map_point(*ws, 0.0)));
    scene.add(SceneElement::Polygon {
        points: fill_points,
        fill: Some(Fill::new(Color::rgba(231, 76, 60, 31))),
        stroke: None,
    });

    // Beyond the tightest landing limit the wing loading is infeasible too.
    let x_first = ws_kg[0];
    let x_last = *ws_kg.last().unwrap_or(&ws_kg[0]);
    let land_limit_kg = data
        .ws_land_limits
        .iter()
        .map(|(_, limit)| limit / G)
        .filter(|limit| limit.is_finite() && *limit > 0.0)
        .fold(f64::INFINITY, f64::min);
    let feasible_right = land_limit_kg.clamp(x_first, x_last);
    if land_limit_kg < x_last {
        scene.add(SceneElement::Polygon {
            points: vec![
                axes.map_point(feasible_right, 0.0),
                axes.map_point(x_last, 0.0),
                axes.map_point(x_last, y_max),
                axes.map_point(feasible_right, y_max),
            ],
            fill: Some(Fill::new(Color::rgba(231, 76, 60, 31))),
            stroke: None,
        });
    }

    if let (Some(ws), Some(tw)) = (data.design_ws_pa, data.design_tw) {
        let center = axes.map_point(ws / G, tw);
        scene.add(SceneElement::Circle {
            center,
            radius: 5.0,
            fill: Some(Fill::new(Color::from_hex(DESIGN_POINT_COLOUR))),
            stroke: Some(Stroke::new(Color::from_hex("#ffffff"), 0.8)),
        });
        let violated = violated_constraints(data, &ws_kg, oei_sls_floor, ws / G, tw, land_limit_kg);
        if !violated.is_empty() {
            // A stacked block in the plot's top-right corner, on a paper
            // plate, joined to the design point by a thin leader: it never
            // sits on the curves or runs past the plot edge.
            let lines = violated.len() as f64 + 1.0;
            let longest = violated
                .iter()
                .map(|l| l.chars().count())
                .max()
                .unwrap_or(0)
                .max("Design point violates:".len()) as f64;
            let block_w = (longest * 6.7 + 16.0).min(axes.width - 12.0);
            let block_h = 14.0 * lines + 10.0;
            let x0 = axes.left + axes.width - block_w - 6.0;
            let y0 = axes.top + 6.0;
            let red = Color::from_hex("#c0392b");
            scene.add(SceneElement::Line {
                p1: [x0 + 0.5 * block_w, y0 + block_h],
                p2: center,
                stroke: Stroke::new(Color::rgba(192, 57, 43, 150), 0.7),
            });
            scene.add(SceneElement::Rect {
                x: x0,
                y: y0,
                width: block_w,
                height: block_h,
                rx: 2.0,
                fill: Some(Fill::new(Color::rgba(255, 255, 255, 238))),
                stroke: Some(Stroke::new(red, 0.8)),
            });
            scene.add(SceneElement::Text {
                text: format!("Design point violates:\n{}", violated.join("\n")),
                pos: [x0 + 7.0, y0 + 4.0],
                font_size: 9.0,
                color: red,
                align: TextAlign::Left,
                baseline: TextBaseline::Top,
                angle_deg: 0.0,
                bold: true,
            });
        }
    }

    axes.draw_frame(&mut scene, pal);
    add_axis_labels(&mut scene, &axes, pal);
    // The red bands are the infeasible regions: below the upper envelope of
    // every T/W floor and beyond the tightest landing W/S limit. The label
    // sits in the remaining region, midway between the floor and the top.
    let label_x = 0.5 * (x_first + feasible_right);
    let label_index = ws_kg
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1 - label_x).abs().total_cmp(&(b.1 - label_x).abs()))
        .map_or(0, |(i, _)| i);
    let mid = label_index;
    scene.add(SceneElement::Text {
        text: "FEASIBLE\nDESIGN SPACE".to_owned(),
        pos: axes.map_point(ws_kg[mid], tw_floor[mid] + 0.5 * (y_max - tw_floor[mid])),
        font_size: 13.0,
        color: Color::from_hex("#2ecc71"),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: true,
    });

    let mut legend = vec![(
        cruise_label.to_owned(),
        LegendMarker::Line(Stroke::new(Color::from_hex("#3498db"), 2.0)),
    )];
    if oei_sls_floor.is_some() {
        legend.push((
            format!("OEI climb >= {:.1}% (SLS-equivalent)", oei_gradient * 100.0),
            LegendMarker::Line(Stroke::dashed(Color::from_hex("#9b59b6"), 1.8, 5.0, 4.0)),
        ));
    }
    for (index, (name, _)) in data.tw_takeoff.iter().enumerate() {
        let colour = Color::from_hex(AIRPORT_COLOURS[index % AIRPORT_COLOURS.len()]);
        legend.push((
            format!("T/O {name}"),
            LegendMarker::Line(Stroke::dashed(colour, 1.6, 6.0, 3.0)),
        ));
        // A landing limit beyond the plotted wing loading is not drawn: say so
        // in the legend, with its value, rather than list an invisible line.
        let off_scale = data
            .ws_land_limits
            .iter()
            .find(|(airport, _)| airport == name)
            .map(|(_, limit)| limit / G)
            .filter(|limit| limit.is_finite() && *limit > x_last);
        legend.push((
            match off_scale {
                Some(limit) => format!("Land {name} (off scale: {limit:.0} kg/m²)"),
                None => format!("Land {name}"),
            },
            LegendMarker::Line(Stroke::dashed(colour, 1.4, 2.0, 4.0)),
        ));
    }
    if data.design_ws_pa.is_some() && data.design_tw.is_some() {
        legend.push((
            "Design point (W/S, T0/W0)".to_owned(),
            LegendMarker::Circle(Color::from_hex(DESIGN_POINT_COLOUR)),
        ));
    }
    // The OEI SLS diagnostic is reported through the pipeline feasibility
    // findings; the figure itself carries no status text.
    draw_legend(&mut scene, [675.0, 70.0], &legend, pal, 8.0);
    scene
}

/// Linear interpolation of a curve sampled on `xs` (ascending), clamped.
fn interpolate(xs: &[f64], ys: &[f64], x: f64) -> Option<f64> {
    let n = xs.len().min(ys.len());
    if n == 0 {
        return None;
    }
    if x <= xs[0] {
        return Some(ys[0]);
    }
    for i in 1..n {
        if x <= xs[i] {
            let t = (x - xs[i - 1]) / (xs[i] - xs[i - 1]).max(f64::MIN_POSITIVE);
            return Some(ys[i - 1] + t * (ys[i] - ys[i - 1]));
        }
    }
    Some(ys[n - 1])
}

/// Names of the constraints the design point violates, as labelled lines.
/// Flags only: nothing is clipped or hidden.
fn violated_constraints(
    data: &MatchingChartData,
    ws_kg: &[f64],
    oei_sls_floor: Option<f64>,
    design_ws_kg: f64,
    design_tw: f64,
    land_limit_kg: f64,
) -> Vec<String> {
    let mut out = Vec::new();
    if !design_tw.is_finite() || !design_ws_kg.is_finite() {
        return out;
    }
    if let Some(floor) = interpolate(ws_kg, &data.tw_cruise, design_ws_kg) {
        if design_tw < floor {
            let (have, need) = tw_pair(design_tw, floor);
            out.push(format!("cruise T/W {have} < {need}"));
        }
    }
    if let Some(floor) = oei_sls_floor {
        if design_tw < floor {
            let (have, need) = tw_pair(design_tw, floor);
            out.push(format!("OEI climb T/W {have} < {need}"));
        }
    }
    for (name, curve) in &data.tw_takeoff {
        if let Some(floor) = interpolate(ws_kg, curve, design_ws_kg) {
            if design_tw < floor {
                let (have, need) = tw_pair(design_tw, floor);
                out.push(format!("take-off {name} T/W {have} < {need}"));
            }
        }
    }
    if design_ws_kg > land_limit_kg {
        out.push(format!(
            "landing W/S {design_ws_kg:.0} > {land_limit_kg:.0} kg/m²"
        ));
    }
    out
}

/// A violated T/W and its floor, printed with enough digits that the two
/// differ (a shortfall below 0.0005 would otherwise read "0.266 < 0.266").
fn tw_pair(have: f64, need: f64) -> (String, String) {
    for decimals in 3..=6 {
        let (a, b) = (format!("{have:.decimals$}"), format!("{need:.decimals$}"));
        if a != b {
            return (a, b);
        }
    }
    (format!("{have:.6}"), format!("{need:.6}"))
}

fn add_axis_labels(scene: &mut Scene, axes: &Axes2D, pal: &crate::theme::Palette) {
    scene.add(SceneElement::Text {
        text: "Wing loading W/S [kg/m²]".to_owned(),
        pos: [axes.left + axes.width * 0.5, axes.top + axes.height + 30.0],
        font_size: 10.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
    scene.add(SceneElement::Text {
        text: "Thrust-to-weight  T0/W0  [-]".to_owned(),
        pos: [axes.left - 50.0, axes.top + axes.height * 0.5],
        font_size: 10.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: -90.0,
        bold: false,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_perf::performance::OeiClimbStatus;

    fn no_wing_report() -> AnalysisReport {
        let mut airplane = crate::families::stability::test_support::probe_airplane(false, false);
        airplane.wings.clear();
        crate::families::stability::test_support::probe_report(airplane)
    }

    /// A report with no main wing says so before anything about the airports,
    /// whether they resolve or not; the two entry points share one guard.
    #[test]
    fn the_no_main_wing_message_wins_over_an_unresolvable_airport() {
        let report = no_wing_report();
        let mut config = AlasConfig::default();
        let wing_message =
            "The analyzed report has no main wing; matching constraints cannot be computed.";
        let airport_message = "Departure/arrival airport not found in the database.";
        let message_of = |scene: &Scene| {
            scene
                .elements
                .iter()
                .filter_map(|element| match element {
                    SceneElement::Text { text, .. } | SceneElement::TextBlock { text, .. } => {
                        Some(text.clone())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let resolved = figure_matching_chart(&report, &config, None);
        assert!(message_of(&resolved).contains(wing_message));
        config.arrival_airport = "NOT-AN-AIRPORT".to_owned();
        let unresolved = figure_matching_chart(&report, &config, None);
        assert!(message_of(&unresolved).contains(wing_message));
        assert!(!message_of(&unresolved).contains(airport_message));
        let airports = (
            resolve_departure(&AlasConfig::default()).unwrap(),
            resolve_arrival(&AlasConfig::default()).unwrap(),
        );
        let explicit =
            figure_matching_chart_for_airports(&report, &config, airports.0, airports.1, None);
        assert!(message_of(&explicit).contains(wing_message));
    }

    /// An unresolvable airport on a report with a wing is its own message.
    #[test]
    fn an_unresolvable_airport_gets_its_own_status_message_on_a_report_with_a_wing() {
        let config = AlasConfig {
            arrival_airport: "NOT-AN-AIRPORT".to_owned(),
            ..AlasConfig::default()
        };
        let airplane = crate::families::stability::test_support::probe_airplane(false, false);
        let report = crate::families::stability::test_support::probe_report(airplane);
        let scene = figure_matching_chart(&report, &config, None);
        let text = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { text, .. } | SceneElement::TextBlock { text, .. } => {
                    Some(text.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("Departure/arrival airport not found in the database."));
    }

    #[test]
    fn a_matching_chart_with_no_points_degrades_to_a_status_scene() {
        let scene = draw_matching_chart(
            &MatchingChartData {
                ws_pa: Vec::new(),
                tw_cruise: Vec::new(),
                tw_oei_climb: 0.0,
                oei_climb_assessment: alas_perf::performance::OeiClimbAssessment {
                    status: OeiClimbStatus::NotApplicable,
                    required_inflight_tw: None,
                    required_sls_tw: None,
                    diagnostic: "not applicable",
                },
                tw_takeoff: Vec::new(),
                ws_land_limits: Vec::new(),
                design_ws_pa: None,
                design_tw: None,
            },
            0.024,
            CRUISE_LABEL_FLAT_LAPSE,
            None,
        );
        assert_eq!(scene.title.as_deref(), Some("Matching Chart"));
    }

    #[test]
    fn conceptual_oei_estimate_is_not_plotted_on_the_sls_axis() {
        let scene = draw_matching_chart(
            &MatchingChartData {
                ws_pa: vec![2_000.0, 10_000.0],
                tw_cruise: vec![0.12, 0.18],
                // Deliberately large: this compatibility value must not set
                // the SLS chart floor when its condition evidence is absent.
                tw_oei_climb: 0.90,
                oei_climb_assessment: alas_perf::performance::OeiClimbAssessment {
                    status: OeiClimbStatus::ConceptualInflight,
                    required_inflight_tw: Some(0.90),
                    required_sls_tw: None,
                    diagnostic: "conceptual in-flight only",
                },
                tw_takeoff: Vec::new(),
                ws_land_limits: Vec::new(),
                design_ws_pa: None,
                design_tw: None,
            },
            0.024,
            CRUISE_LABEL_FLAT_LAPSE,
            None,
        );
        assert!(!scene.elements.iter().any(|element| matches!(
            element,
            SceneElement::Text { text, .. }
                if text.contains("conceptual in-flight only")
        )));
        assert!(!scene.elements.iter().any(|element| matches!(
            element,
            SceneElement::Text { text, .. } if text.contains("OEI climb >=")
        )));
    }

    #[test]
    fn the_landing_limit_is_drawn_on_the_takeoff_wing_loading_axis() {
        // MLW 0.9 MTOW: a landing wing loading of 8000 Pa at MLW is
        // 8000 / 0.9 at MTOW.
        let on_axis = landing_limit_on_takeoff_axis(8_000.0, 63_000.0, 70_000.0, false);
        assert!((on_axis - 8_000.0 / 0.9).abs() < 1e-9);
        // Equal masses leave the dry limit untouched.
        assert_eq!(
            landing_limit_on_takeoff_axis(8_000.0, 70_000.0, 70_000.0, false),
            8_000.0
        );
        // A wet runway divides it by 1.15; an unusable landing mass leaves it
        // unconverted.
        let wet = landing_limit_on_takeoff_axis(8_000.0, 70_000.0, 70_000.0, true);
        assert!((wet - 8_000.0 / 1.15).abs() < 1e-9);
        assert_eq!(
            landing_limit_on_takeoff_axis(8_000.0, f64::NAN, 70_000.0, false),
            8_000.0
        );
    }

    fn chart_data(design: Option<(f64, f64)>) -> MatchingChartData {
        MatchingChartData {
            ws_pa: vec![2_000.0, 10_000.0],
            tw_cruise: vec![0.12, 0.18],
            tw_oei_climb: 0.0,
            oei_climb_assessment: alas_perf::performance::OeiClimbAssessment {
                status: OeiClimbStatus::NotApplicable,
                required_inflight_tw: None,
                required_sls_tw: None,
                diagnostic: "OEI SLS evidence gap: synthetic status that must not be drawn",
            },
            tw_takeoff: vec![("LEMD".to_owned(), vec![0.20, 0.30])],
            ws_land_limits: vec![("LEMD".to_owned(), 8_000.0)],
            design_ws_pa: design.map(|(ws, _)| ws),
            design_tw: design.map(|(_, tw)| tw),
        }
    }

    fn legend_texts(scene: &Scene) -> Vec<String> {
        scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_design_point_marker_is_named_in_the_legend() {
        let scene = draw_matching_chart(
            &chart_data(Some((6_000.0, 0.30))),
            0.024,
            CRUISE_LABEL_FLAT_LAPSE,
            None,
        );
        let texts = legend_texts(&scene);
        assert!(
            texts.iter().any(|text| text.starts_with("Design point")),
            "legend texts: {texts:?}"
        );
        let swatch = Color::from_hex(DESIGN_POINT_COLOUR);
        let circles = scene
            .elements
            .iter()
            .filter(|element| {
                matches!(
                    element,
                    SceneElement::Circle { fill: Some(fill), .. } if fill.color == swatch
                )
            })
            .count();
        assert_eq!(circles, 2, "plotted marker plus one legend swatch");
    }

    #[test]
    fn the_figure_carries_no_oei_status_text() {
        for design in [None, Some((6_000.0, 0.30))] {
            let scene =
                draw_matching_chart(&chart_data(design), 0.024, CRUISE_LABEL_FLAT_LAPSE, None);
            let texts = legend_texts(&scene);
            assert!(
                !texts.iter().any(|text| text.contains("evidence gap")),
                "status text drawn: {texts:?}"
            );
            assert_eq!(
                texts.iter().any(|text| text.starts_with("Design point")),
                design.is_some()
            );
        }
    }

    /// The deck cruise line passes through each level's requirement at the
    /// design wing loading, follows the drag-to-lift ratio away from it, and
    /// takes the governing level.
    #[test]
    fn the_deck_cruise_line_passes_through_the_check_value_at_the_design_wing_loading() {
        let level =
            |altitude_m: f64, mass_kg: f64, required_tw: f64| alas_opt::mdo::PlannedCruiseLevel {
                altitude_m,
                mach: 0.78,
                isa_deviation_c: 0.0,
                mass_kg,
                required_tw,
            };
        // CD = 0.02 + 0.045 CL^2 on a 120 m^2 reference area.
        let cd = |cl: f64, _: f64, _: f64| 0.02 + 0.045 * cl * cl;
        let g_over_s = 9.806_65 / 120.0;
        let design_ws = 6_000.0;
        let ws = [3_000.0, design_ws, 9_000.0];
        let low = level(10_000.0, 70_000.0, 0.20);
        let line = deck_cruise_line(&ws, design_ws, &[low], g_over_s, &cd).unwrap();
        assert!((line[1] - 0.20).abs() < 1e-12);
        // Hand value away from the design point: the ratio of CD/CL.
        let air = alas_atmo::us1976_compute_values(10_000.0, 0.0);
        let speed = 0.78 * air.speed_of_sound_m_s;
        let q = 0.5 * air.density_kg_m3 * speed * speed;
        let cl_design = 70_000.0 * g_over_s / q;
        let drag_to_lift = |cl: f64| (0.02 + 0.045 * cl * cl) / cl;
        let expected = 0.20 * drag_to_lift(1.5 * cl_design) / drag_to_lift(cl_design);
        assert!((line[2] - expected).abs() < 1e-12);
        // A second, more demanding level governs.
        let high = level(11_500.0, 66_000.0, 0.26);
        let both = deck_cruise_line(&ws, design_ws, &[low, high], g_over_s, &cd).unwrap();
        assert!((both[1] - 0.26).abs() < 1e-12);
        assert!(both.iter().zip(&line).all(|(b, l)| b >= l));
        // No usable level: no line.
        let unusable = level(10_000.0, 70_000.0, f64::NAN);
        assert!(deck_cruise_line(&ws, design_ws, &[unusable], g_over_s, &cd).is_none());
        assert!(deck_cruise_line(&ws, design_ws, &[], g_over_s, &cd).is_none());
    }

    /// A design point under a floor or past the landing limit is named, not
    /// moved; the feasible label stays in the unshaded region.
    #[test]
    fn a_design_point_below_a_floor_or_past_landing_is_flagged() {
        let ws_kg: Vec<f64> = vec![2_000.0 / 9.81, 10_000.0 / 9.81];
        let data = chart_data(Some((6_000.0, 0.10)));
        let flags = violated_constraints(&data, &ws_kg, None, 6_000.0 / 9.81, 0.10, 8_000.0 / 9.81);
        assert!(flags.iter().any(|f| f.starts_with("cruise")), "{flags:?}");
        assert!(
            flags.iter().any(|f| f.starts_with("take-off LEMD")),
            "{flags:?}"
        );
        assert!(!flags.iter().any(|f| f.starts_with("landing")), "{flags:?}");
        let past = violated_constraints(&data, &ws_kg, None, 9_000.0 / 9.81, 0.9, 8_000.0 / 9.81);
        assert_eq!(past.len(), 1, "{past:?}");
        assert!(past[0].starts_with("landing"));
        let scene = draw_matching_chart(
            &chart_data(Some((6_000.0, 0.10))),
            0.024,
            CRUISE_LABEL_FLAT_LAPSE,
            None,
        );
        assert!(legend_texts(&scene)
            .iter()
            .any(|t| t.starts_with("Design point violates")));
    }

    /// A landing wing-loading limit is a constraint on W/S alone, so it is a
    /// vertical line on the (W/S, T/W) chart; a sloped line would claim a
    /// thrust-to-weight dependence landing does not have.
    #[test]
    fn the_landing_limit_is_a_vertical_line() {
        let scene = draw_matching_chart(&chart_data(None), 0.024, CRUISE_LABEL_FLAT_LAPSE, None);
        let landing_lines: Vec<&Vec<[f64; 2]>> = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Polyline { points, stroke }
                    if points.len() == 2 && (stroke.width - 1.4).abs() < 1e-9 =>
                {
                    Some(points)
                }
                _ => None,
            })
            .collect();
        assert_eq!(landing_lines.len(), 1, "one landing limit for one airport");
        let points = landing_lines[0];
        assert!((points[0][0] - points[1][0]).abs() < 1e-9, "{points:?}");
        assert!(points[0][1] != points[1][1]);
    }
}
