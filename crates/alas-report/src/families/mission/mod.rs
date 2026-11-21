// SPDX-License-Identifier: AGPL-3.0-or-later
// Colet y_range = autoscale(series.iter().flat_map(|(pts, _, _)| pts.iter().map(|p| p.1)));yright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py

//! Flown mission trajectory, altitude/mass/speed timelines, aerodynamic
//! coefficient and force timelines, and drag-component breakdowns.
//!
//! Split across topical submodules to stay under the repository's 500-line
//! file limit while covering all six of `visualization.py`'s mission
//! figures: [`profile::figure_mission_profile`] (altitude/mass/TAS/SFC),
//! [`velocities::figure_mission_velocities`] (TAS+EAS/Mach),
//! [`velocities::figure_mission_flight_path`] (range/pitch),
//! [`aero::figure_mission_aero_coefficients`] (AoA/CL/CD/L-D),
//! [`aero::figure_mission_aero_forces`] (throttle/lift/thrust/drag) and
//! [`drag::figure_mission_drag_components`] (the five CD components).
//!
//! Every panel reads a scalar off [`alas_mission::Conditions`] at each
//! control point of each segment, matching the way `export_data.py`'s
//! `export_simulation_results` CSV writer reads `mission.columns` from
//! `segment.conditions...`; see each submodule's doc
//! for the field it mirrors. `EAS_m_s` and `SFC_kg_kgf_hr` are not stored
//! columns either side: both are computed inline by `export_data.py`
//! (`tas * sqrt(density / 1.225)` and `(mdot * 3600) / (thrust / g0)`), so
//! this module reproduces those two formulas rather than reading a field
//! that does not exist upstream either.

mod aero;
mod drag;
mod earth;
mod profile;
mod route;
mod route_3d;
mod velocities;

pub use aero::{figure_mission_aero_coefficients, figure_mission_aero_forces};
pub use drag::figure_mission_drag_components;
pub use profile::{figure_mission_profile, figure_mission_profile_vs_configured};
pub use route::figure_mission_route_2d;
pub use route_3d::{figure_mission_route_3d, route_focused_camera, route_globe_disk};
pub use velocities::{figure_mission_flight_path, figure_mission_velocities};

use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::Palette;
use alas_mission::solve::MissionResult;
use alas_mission::Conditions;

/// ISA sea-level density, kg/m^3: the EAS reference `export_data.py` uses.
pub(super) const RHO_SL: f64 = 1.225;

/// Standard gravity, m/s^2: the N -> kgf conversion `export_data.py`'s SFC
/// column uses.
pub(super) const G0: f64 = alas_atmo::STANDARD_GRAVITY;

/// Mission time in minutes at one control point, matching `_mission_time_min`
/// (`np.asarray(mission.time_s) / 60.0`, where `mission.time_s` is the CSV's
/// `Time_s` column). `Conditions::time_s` is already absolute along the whole
/// mission. `initialize_time` shifts each segment onto the end of the preceding
/// one, so no per-segment offset is needed here.
fn time_min_at(cond: &Conditions, i: usize) -> f64 {
    cond.time_s[i] / 60.0
}

/// Walk every control point of every segment in flight order, pairing mission
/// time (minutes) with one extracted scalar.
pub(super) fn collect_series(
    mission: &MissionResult,
    mut extract: impl FnMut(&Conditions, usize) -> f64,
) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for segment in &mission.segments {
        let cond = &segment.conditions;
        for i in 0..cond.len() {
            out.push((time_min_at(cond, i), extract(cond, i)));
        }
    }
    out
}

/// The shared time axis every panel in a mission figure plots against:
/// `[0, last control point]` minutes, floored at ten minutes so a mission of
/// one or two points still draws a legible axis.
pub(super) fn time_domain(mission: &MissionResult) -> (f64, f64) {
    let max_t = mission
        .segments
        .iter()
        .flat_map(|segment| segment.conditions.time_s.iter().copied())
        .fold(0.0_f64, f64::max)
        / 60.0;
    (0.0, max_t.max(10.0))
}

/// Fewer finite samples than this keep the plain min/max autoscale.
const PERCENTILE_MIN_SAMPLES: usize = 100;

/// The full finite extent is kept unless it is wider than this multiple of the 1st-99th
/// percentile span: such a series is dominated by a few outliers.
const OUTLIER_SPAN_FACTOR: f64 = 3.0;

/// Autoscale a Y range from data, matplotlib-style: a 5% margin on each side
/// of the finite extent, falling back to `[0, 1]` when there is no finite
/// data at all (an empty mission).
///
/// The range always contains every finite sample (so no part of a curve is cut
/// off) unless the series has [`PERCENTILE_MIN_SAMPLES`] or more finite samples
/// and its full extent exceeds [`OUTLIER_SPAN_FACTOR`] times its 1st-99th
/// percentile span. Only then is the extent the 1st to 99th percentile, so a few
/// spikes (near-idle SFC, a numerical outlier) cannot flatten the real trend;
/// the clipped samples are then marked by [`draw_clipped_markers`].
pub(super) fn autoscale(values: impl Iterator<Item = f64>) -> (f64, f64) {
    let mut finite: Vec<f64> = values.filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return (0.0, 1.0);
    }
    finite.sort_by(f64::total_cmp);
    let full = (finite[0], finite[finite.len() - 1]);
    let (lo, hi) = if finite.len() >= PERCENTILE_MIN_SAMPLES {
        let last = finite.len() - 1;
        let at = |q: f64| finite[((last as f64) * q).round() as usize];
        let core = (at(0.01), at(0.99));
        if full.1 - full.0 > OUTLIER_SPAN_FACTOR * (core.1 - core.0) {
            core
        } else {
            full
        }
    } else {
        full
    };
    let span = (hi - lo).max(1e-9);
    let pad = span * 0.05;
    (lo - pad, hi + pad)
}

/// Mark every run of samples that lies beyond the axes' Y range with a small triangle on the
/// frame edge it left through, so a curve cut by an outlier-robust range is visibly cut rather
/// than silently truncated.
fn draw_clipped_markers(axes: &Axes2D, scene: &mut Scene, pts: &[(f64, f64)], color: Color) {
    let (y_lo, y_hi) = (axes.y_min.min(axes.y_max), axes.y_min.max(axes.y_max));
    let mut was_out = 0_i8;
    for &(x, y) in pts {
        let side = if !y.is_finite() || x < axes.x_min || x > axes.x_max {
            0
        } else if y > y_hi {
            1
        } else if y < y_lo {
            -1
        } else {
            0
        };
        if side != 0 && side != was_out {
            let [px, _] = axes.map_point(x, y_lo);
            let (edge, dir) = if side > 0 {
                (axes.top, 1.0)
            } else {
                (axes.top + axes.height, -1.0)
            };
            scene.add(SceneElement::Polygon {
                points: vec![
                    [px - 3.5, edge + dir * 6.0],
                    [px + 3.5, edge + dir * 6.0],
                    [px, edge],
                ],
                fill: Some(Fill::new(color)),
                stroke: None,
            });
        }
        was_out = side;
    }
}

/// Draw a label over imagery with a contrasting halo: four offset copies in the
/// halo colour under the label itself, so it reads on dark ocean and light land.
pub(super) fn add_halo_text(
    scene: &mut Scene,
    text: &str,
    pos: [f64; 2],
    font_size: f64,
    align: TextAlign,
    baseline: TextBaseline,
) {
    let halo = Color::from_hex("#05080f");
    for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
        scene.add(SceneElement::Text {
            text: text.to_owned(),
            pos: [pos[0] + dx, pos[1] + dy],
            font_size,
            color: halo,
            align,
            baseline,
            angle_deg: 0.0,
            bold: false,
        });
    }
    scene.add(SceneElement::Text {
        text: text.to_owned(),
        pos,
        font_size,
        color: Color::from_hex("#f4f6fa"),
        align,
        baseline,
        angle_deg: 0.0,
        bold: false,
    });
}

/// Draw a line series, breaking it into separate polylines at any non-finite
/// point rather than drawing a straight segment across it. `L_over_D` and
/// `SFC_kg_kgf_hr` are both undefined where their denominator is zero
/// (`export_data.py` reports `nan` there too), and a scene's `Polyline` has
/// no gap primitive of its own to lean on.
fn add_series_with_gaps(
    axes: &Axes2D,
    scene: &mut Scene,
    pts: &[(f64, f64)],
    stroke: Stroke,
    name: Option<&str>,
) {
    let draw = |scene: &mut Scene, run: &[(f64, f64)]| {
        if let Some(name) = name {
            scene.data_trace.name_next_line(name.to_owned());
        }
        axes.add_line_series(scene, run, stroke.clone());
    };
    let mut run: Vec<(f64, f64)> = Vec::new();
    for &(x, y) in pts {
        if y.is_finite() {
            run.push((x, y));
        } else if !run.is_empty() {
            draw(scene, &run);
            run.clear();
        }
    }
    if !run.is_empty() {
        draw(scene, &run);
    }
}

/// One plotted series: its `(x, y)` points, its stroke and an optional recorded name.
type NamedSeries<'a> = (&'a [(f64, f64)], Stroke, Option<&'a str>);

/// Draw one panel's frame, one or more overlaid series, and a bold
/// left-aligned title above the axes. This follows the panel-identity convention
/// `figure_mission_profile` establishes upstream in place of a rotated
/// y-axis label (see that function's module doc for why). Y range is
/// autoscaled from every series' finite values combined; X range is shared
/// across a whole figure's panels via [`time_domain`].
pub(super) fn draw_time_panel_multi(
    scene: &mut Scene,
    pal: &Palette,
    rect: (f64, f64, f64, f64),
    x_range: (f64, f64),
    series: &[(&[(f64, f64)], Stroke)],
    title: &str,
    title_font: f64,
) -> Axes2D {
    let named: Vec<NamedSeries<'_>> = series
        .iter()
        .map(|(pts, stroke)| (*pts, stroke.clone(), None))
        .collect();
    draw_time_panel_named(scene, pal, rect, x_range, &named, title, title_font)
}

/// [`draw_time_panel_multi`] where a series may carry a name its plot data
/// records for each of its runs.
pub(super) fn draw_time_panel_named(
    scene: &mut Scene,
    pal: &Palette,
    rect: (f64, f64, f64, f64),
    x_range: (f64, f64),
    series: &[NamedSeries<'_>],
    title: &str,
    title_font: f64,
) -> Axes2D {
    let y_range = autoscale(
        series
            .iter()
            .flat_map(|(pts, _, _)| pts.iter().map(|p| p.1)),
    );
    let axes = Axes2D::new(rect, x_range, y_range);
    axes.draw_frame(scene, pal);
    // The stack shares one time axis, drawn once under the bottom panel, and
    // each panel's title carries its quantity and unit. Record both against
    // this panel, or plot-data export takes the next panel's title for its
    // x label (and its unit for the time unit).
    scene
        .data_trace
        .note_labels(&axes, Some("Time (min)"), Some(title));
    for (pts, stroke, name) in series {
        add_series_with_gaps(&axes, scene, pts, stroke.clone(), *name);
        draw_clipped_markers(&axes, scene, pts, stroke.color);
    }
    scene.add(SceneElement::Text {
        text: title.to_owned(),
        pos: [rect.0, rect.1 - 6.0],
        font_size: title_font,
        color: Color::from_hex(pal.title),
        align: TextAlign::Left,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });
    axes
}

/// [`draw_time_panel_multi`] for the common case of a single series.
// Eight parameters: every mission figure below is a stack of these panels,
// each varying in rect/series/title, so a struct-of-arguments would just
// move the same eight names one level out without shrinking anything.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_time_panel(
    scene: &mut Scene,
    pal: &Palette,
    rect: (f64, f64, f64, f64),
    x_range: (f64, f64),
    pts: &[(f64, f64)],
    stroke: Stroke,
    title: &str,
    title_font: f64,
) -> Axes2D {
    draw_time_panel_multi(
        scene,
        pal,
        rect,
        x_range,
        &[(pts, stroke)],
        title,
        title_font,
    )
}

/// The `"Time (min)"` x-axis label Python places under the bottom-most panel
/// of each mission figure only.
pub(super) fn draw_time_axis_label(scene: &mut Scene, pal: &Palette, rect: (f64, f64, f64, f64)) {
    scene.add(SceneElement::Text {
        text: "Time (min)".to_owned(),
        pos: [rect.0 + rect.2 * 0.5, rect.1 + rect.3 + 22.0],
        font_size: 9.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
}

/// Shared across every submodule's unit tests as `crate::families::mission::
/// test_support::sample_mission` (each figure submodule is a descendant of
/// this one, so its `pub(in crate::families::mission)` visibility already
/// reaches them, so no re-export is needed.
#[cfg(test)]
mod test_support {
    use alas_aero::drag_buildup::{ComponentParasiteDrag, DragBreakdown};
    use alas_math::hybrd::Status;
    use alas_mission::solve::{MissionResult, SegmentSolution};
    use alas_mission::{Conditions, Numerics, Segment, SegmentKind, SegmentSpec};

    fn dummy_drag_breakdown(total: f64) -> DragBreakdown {
        let zero_component = ComponentParasiteDrag {
            parasite_drag_coefficient: 0.0,
            skin_friction_coefficient: 0.0,
            form_factor: 0.0,
            compressibility_factor: 0.0,
            reynolds_factor: 0.0,
        };
        DragBreakdown {
            parasite_wings: Vec::new(),
            parasite_fuselages: Vec::new(),
            parasite_nacelles: Vec::new(),
            parasite_pylon: zero_component,
            parasite_total: total * 0.6,
            induced_total: total * 0.3,
            induced_viscous: 0.0,
            induced_viscous_wings: Vec::new(),
            compressible_wings: Vec::new(),
            compressible_total: total * 0.05,
            miscellaneous_total_wetted_area_m2: 0.0,
            miscellaneous_total: total * 0.05,
            untrimmed: total,
            trim_corrected: total,
            spoiler: 0.0,
            total,
        }
    }

    /// One three-point cruise segment with a distinct, hand-picked value on
    /// every field a mission figure reads, so a unit test can check an exact
    /// number rather than only "a line was drawn somewhere". `time_offset_s`/
    /// `mass_offset_kg`/`range_offset_m` let [`sample_mission`] chain two of
    /// these into one mission whose `time_s` stays absolute and monotonic
    /// across the join, the way [`Conditions::time_s`]'s own doc requires.
    fn cruise_segment(time_offset_s: f64, mass_offset_kg: f64, range_offset_m: f64) -> Segment {
        let n = 3;
        let mut cond = Conditions::expanded(n);
        cond.time_s = [0.0, 600.0, 1200.0].map(|t| t + time_offset_s).to_vec();
        cond.altitude_m = vec![10_000.0, 10_500.0, 11_000.0];
        cond.velocity_m_s = vec![230.0, 232.0, 234.0];
        cond.density_kg_m3 = vec![0.4127, 0.4, 0.39];
        cond.mach = vec![0.78, 0.785, 0.79];
        cond.aircraft_range_m = [0.0, 138_000.0, 277_000.0]
            .map(|r| r + range_offset_m)
            .to_vec();
        cond.body_inertial_rotations_rad[0][1] = 0.02;
        cond.body_inertial_rotations_rad[1][1] = 0.021;
        cond.body_inertial_rotations_rad[2][1] = 0.022;
        cond.angle_of_attack_rad = vec![0.03, 0.031, 0.032];
        cond.lift_coefficient = vec![0.5, 0.51, 0.52];
        cond.drag_coefficient = vec![0.025, 0.0255, 0.026];
        cond.throttle = vec![0.7, 0.71, 0.72];
        cond.wind_lift_force_vector_n = vec![
            [0.0, 0.0, -700_000.0],
            [0.0, 0.0, -695_000.0],
            [0.0, 0.0, -690_000.0],
        ];
        cond.wind_drag_force_vector_n = vec![
            [-35_000.0, 0.0, 0.0],
            [-35_500.0, 0.0, 0.0],
            [-36_000.0, 0.0, 0.0],
        ];
        cond.thrust_force_vector_n = vec![
            [36_000.0, 0.0, 0.0],
            [36_200.0, 0.0, 0.0],
            [36_400.0, 0.0, 0.0],
        ];
        cond.total_mass_kg = [68_000.0, 67_800.0, 67_600.0]
            .map(|m| m - mass_offset_kg)
            .to_vec();
        cond.vehicle_mass_rate_kg_s = vec![0.35, 0.351, 0.352];
        cond.drag_breakdown = cond
            .drag_coefficient
            .iter()
            .map(|&cd| dummy_drag_breakdown(cd))
            .collect();

        Segment {
            spec: SegmentSpec {
                tag: "test_cruise".to_owned(),
                kind: SegmentKind::Cruise {
                    altitude_m: Some(10_000.0),
                    distance_m: 277_000.0,
                },
                air_speed_m_s: 232.0,
                air_speed_reference: alas_config::mission::SpeedReference::TrueAirspeed,
                speed_caps: alas_config::mission::SpeedCaps::default(),
                true_course_rad: 0.0,
                temperature_deviation_k: 0.0,
                number_control_points: n,
                takeoff_rated: false,
                rating_limited: false,
                configuration_delta_cd: 0.0,
                load_factor: 1.0,
                entry_speed_m_s: 0.0,
                exit_speed_m_s: 0.0,
                speed_profile: Vec::new(),
            },
            numerics: Numerics::default(),
            conditions: cond,
            initials: None,
            throttle: vec![0.0; n],
            body_angle_rad: vec![0.0; n],
            residuals: vec![[0.0; 2]; n],
            // Empty on any leg that is not a rating-limited climb.
            rate_profile_m_s: Vec::new(),
        }
    }

    /// A mission of two three-point cruise segments back to back, joined so
    /// `time_s`/`total_mass_kg`/`aircraft_range_m` stay monotonic across the
    /// seam. This follows the invariant documented for [`Conditions::time_s`], so a
    /// test can check that a figure's series concatenates across segment
    /// boundaries rather than only rendering the first one.
    pub(in crate::families::mission) fn sample_mission() -> MissionResult {
        let first = cruise_segment(0.0, 0.0, 0.0);
        let last_time = *first.conditions.time_s.last().unwrap_or(&0.0);
        let mass_burned = first.conditions.total_mass_kg[0]
            - *first.conditions.total_mass_kg.last().unwrap_or(&0.0);
        let last_range = *first.conditions.aircraft_range_m.last().unwrap_or(&0.0);
        let second = cruise_segment(last_time, mass_burned, last_range);
        MissionResult {
            segments: vec![first, second],
            solutions: vec![
                SegmentSolution {
                    converged: true,
                    status: Status::Converged,
                    evaluations: 0,
                    throttle_limited: false,
                    peak_throttle: 0.5,
                    minimum_throttle: 0.4,
                    available_throttle_floor: f64::NAN,
                    idle_floor_limited: false,
                    sub_idle_points: 0,
                },
                SegmentSolution {
                    converged: true,
                    status: Status::Converged,
                    evaluations: 0,
                    throttle_limited: false,
                    peak_throttle: 0.5,
                    minimum_throttle: 0.4,
                    available_throttle_floor: f64::NAN,
                    idle_floor_limited: false,
                    sub_idle_points: 0,
                },
            ],
            scheduled_segment_count: 2,
            fuel_exhaustion: None,
        }
    }

    #[test]
    fn the_sample_mission_has_six_points_across_two_segments() {
        let mission = sample_mission();
        let total: usize = mission.segments.iter().map(|s| s.conditions.len()).sum();
        assert_eq!(total, 6);
        assert!(mission.figure_data_ready());
    }

    #[test]
    fn figure_readiness_rejects_a_missing_mach_series() {
        let mut mission = sample_mission();
        mission.segments[0].conditions.mach.pop();
        assert!(!mission.figure_data_ready());
    }
}

#[cfg(test)]
// Fixture parsing failures are test-authoring failures, not runtime paths.
#[allow(clippy::expect_used)]
mod tests {
    use super::test_support::sample_mission;
    use super::*;

    #[test]
    fn time_domain_reads_the_last_control_points_time_and_floors_at_ten_minutes() {
        let mission = sample_mission();
        let (lo, hi) = time_domain(&mission);
        assert_eq!(lo, 0.0);
        // Two back-to-back three-point segments, each spanning 1200 s and
        // joined so the second's clock continues the first's: 2400 s total.
        assert!((hi - 40.0).abs() < 1e-9);
    }

    #[test]
    fn autoscale_pads_a_nondegenerate_range_by_five_percent_and_falls_back_on_no_data() {
        let (lo, hi) = autoscale([1.0, 2.0, 3.0].into_iter());
        assert!((lo - 0.9).abs() < 1e-9);
        assert!((hi - 3.1).abs() < 1e-9);
        assert_eq!(autoscale(std::iter::empty()), (0.0, 1.0));
        assert_eq!(autoscale([f64::NAN, f64::NAN].into_iter()), (0.0, 1.0));
    }

    #[test]
    fn autoscale_ignores_one_percent_outliers_on_long_series() {
        let mut values: Vec<f64> = (0..300)
            .map(|i| 0.55 + 0.0001 * f64::from(i % 10))
            .collect();
        values[10] = 400.0;
        values[200] = -50.0;
        let (lo, hi) = autoscale(values.into_iter());
        assert!(lo > 0.4 && hi < 0.7, "{lo} {hi}");
    }

    #[test]
    fn autoscale_keeps_a_moderate_spike_inside_the_axes() {
        // A start-up spike of a few percent of the samples is part of the curve, not an
        // outlier: the full extent is kept so no part of the line leaves the box.
        let mut values: Vec<f64> = (0..300)
            .map(|i| 0.55 + 0.0001 * f64::from(i % 10))
            .collect();
        for value in values.iter_mut().take(6) {
            *value = 0.9;
        }
        let (lo, hi) = autoscale(values.into_iter());
        assert!(lo < 0.55 && hi > 0.9, "{lo} {hi}");
    }

    #[test]
    fn clipped_outliers_are_marked_on_the_frame() {
        let mut values: Vec<f64> = (0..300)
            .map(|i| 0.55 + 0.0001 * f64::from(i % 10))
            .collect();
        values[10] = 400.0;
        let pts: Vec<(f64, f64)> = values
            .iter()
            .enumerate()
            .map(|(i, &v)| (i as f64 / 30.0, v))
            .collect();
        let mut scene = Scene::new(400.0, 300.0, None);
        let pal = crate::theme::get_palette(Some("dark"));
        draw_time_panel(
            &mut scene,
            pal,
            (50.0, 40.0, 300.0, 200.0),
            (0.0, 10.0),
            &pts,
            Stroke::new(Color::from_hex("tab:blue"), 1.0),
            "x",
            9.0,
        );
        let triangles = scene
            .elements
            .iter()
            .filter(|element| matches!(element, SceneElement::Polygon { .. }))
            .count();
        assert_eq!(triangles, 1);
    }

    #[test]
    fn collect_series_concatenates_every_segment_in_flight_order() {
        let mission = sample_mission();
        let pts = collect_series(&mission, |c, i| c.altitude_m[i]);
        assert_eq!(pts.len(), 6);
        assert_eq!(pts[0], (0.0, 10_000.0));
        assert_eq!(pts[2], (20.0, 11_000.0));
    }

    #[test]
    fn reference_contracts_cover_every_mission_scene_in_both_themes() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../golden/report/reference_render_w33.json"
        ))
        .expect("reference fixture is valid JSON");
        let mission = sample_mission();
        for theme in ["light", "dark"] {
            let figures = [
                (
                    "mission_profile",
                    figure_mission_profile(&mission, Some(theme)),
                ),
                (
                    "mission_velocities",
                    figure_mission_velocities(&mission, Some(theme)),
                ),
                (
                    "mission_flight_path",
                    figure_mission_flight_path(&mission, Some(theme)),
                ),
                (
                    "mission_aero_coefficients",
                    figure_mission_aero_coefficients(&mission, Some(theme)),
                ),
                (
                    "mission_aero_forces",
                    figure_mission_aero_forces(&mission, Some(theme)),
                ),
                (
                    "mission_drag_components",
                    figure_mission_drag_components(&mission, Some(theme)),
                ),
            ];

            for (id, scene) in figures {
                let contract = &fixture["figures"][&format!("{id}:{theme}")];
                assert_eq!(contract["available"], true, "{id} reference unavailable");
                let expected_panels = match id {
                    "mission_profile" => 4,
                    "mission_velocities" | "mission_flight_path" => 2,
                    "mission_aero_coefficients" | "mission_aero_forces" => 4,
                    "mission_drag_components" => 1,
                    _ => unreachable!("all mission ids are listed above"),
                };
                assert_eq!(contract["panel_count"].as_u64(), Some(expected_panels));
                let svg = crate::svg::render_svg(&scene);
                let reference_title = if contract["suptitle"].as_str().unwrap_or("").is_empty() {
                    contract["axes"][0]["title"].as_str().unwrap_or("")
                } else {
                    contract["suptitle"].as_str().unwrap_or("")
                };
                assert_eq!(
                    scene.title.as_deref(),
                    Some(reference_title),
                    "{id} title differs from the pinned reference"
                );
                for axis in contract["axes"].as_array().expect("reference axes") {
                    for field in ["title", "title_left", "title_right", "xlabel", "ylabel"] {
                        let label = axis[field].as_str().unwrap_or("");
                        if !label.is_empty() {
                            assert!(svg.contains(label), "{id} is missing {field}: {label}");
                        }
                    }
                    for label in axis["legend"].as_array().into_iter().flatten() {
                        assert!(svg.contains(label.as_str().unwrap_or("")));
                    }
                }
            }
        }
    }
}
