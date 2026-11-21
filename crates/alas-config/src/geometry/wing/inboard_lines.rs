// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The two inboard edge lines of the wing.
//!
//! A fillet is a blend body, not wing. Each edge of the wing is the line through the
//! longest straight run of its built polyline between the side of body and the kink,
//! and it stays on that line from `y = 0` to where the run starts. The stations a
//! fillet adds inboard of that point stay in the configuration as data for a drawn
//! blend; what the wing is made of does not read them. The reference planform
//! ([`TransportPlanform::reference_area_m2`]) is the one exception, because the
//! published reference quantities are stated on it.
//!
//! A user-defined section is a section of the wing, not a blend. Between the side of
//! body and the kink it is a vertex of both polylines, so a run it breaks is not treated
//! as straight, and it stays as drawn wherever the run starts: only the blend stations
//! go to the lines. One at or inboard of the side of body is refused at build.
//!
//! Frame: x aft, y to starboard, metres, in the planform's own x origin.

use super::planform::{MainWingStation, TransportPlanform};

/// One edge's line, `x = x_at_centreline_m + slope * y`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InboardEdgeLine {
    /// Edge x at `y = 0`, m.
    pub x_at_centreline_m: f64,
    /// `dx/dy` of the run, dimensionless.
    pub slope: f64,
    /// Where the run starts going outboard, m. Inboard of here the edge is the line;
    /// from here on it is its own polyline.
    pub run_start_y_m: f64,
}

impl InboardEdgeLine {
    /// The edge x of this line at spanwise `y_m`.
    #[must_use]
    pub fn x_at(&self, y_m: f64) -> f64 {
        self.x_at_centreline_m + self.slope * y_m
    }
}

/// Both inboard edge lines of a wing with a side-of-body station.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InboardEdgeLines {
    /// The leading edge.
    pub leading: InboardEdgeLine,
    /// The trailing edge, stated on `x_le + c` before any section twist.
    pub trailing: InboardEdgeLine,
    /// Below this distance two spanwise stations are the same one, m.
    tolerance_m: f64,
}

impl InboardEdgeLines {
    fn on_leading(&self, y_m: f64) -> bool {
        y_m < self.leading.run_start_y_m - self.tolerance_m
    }

    fn on_trailing(&self, y_m: f64) -> bool {
        y_m < self.trailing.run_start_y_m - self.tolerance_m
    }

    /// Whether a section at `y_m` has an edge that the lines replace.
    #[must_use]
    pub fn replaces_at(&self, y_m: f64) -> bool {
        self.on_leading(y_m) || self.on_trailing(y_m)
    }

    /// Leading-edge x and chord at `y_m`, for a section drawn with `leading_x_m` and
    /// `chord_m`: each edge on its line inboard of its run and as drawn from there on.
    /// A section with neither edge replaced comes back exactly as it was given.
    #[must_use]
    pub fn section_at(&self, y_m: f64, leading_x_m: f64, chord_m: f64) -> (f64, f64) {
        if !self.replaces_at(y_m) {
            return (leading_x_m, chord_m);
        }
        let leading = if self.on_leading(y_m) {
            self.leading.x_at(y_m)
        } else {
            leading_x_m
        };
        let trailing = if self.on_trailing(y_m) {
            self.trailing.x_at(y_m)
        } else {
            leading_x_m + chord_m
        };
        (leading, trailing - leading)
    }
}

impl TransportPlanform {
    /// The inboard edge lines, `None` with no side-of-body station. The one place
    /// that decides where the wing's edges run inside the body and across a fillet.
    #[must_use]
    pub fn inboard_edge_lines(&self) -> Option<InboardEdgeLines> {
        let side_of_body = self.side_of_body?;
        let kink = self.kink;
        let tolerance_m = 1.0e-9 * self.tip.y_m;
        let mut inboard: Vec<MainWingStation> = std::iter::once(side_of_body)
            .chain(
                self.edge_stations
                    .iter()
                    .chain(&self.custom_stations)
                    .copied()
                    .filter(|station| station.y_m > side_of_body.y_m && station.y_m < kink.y_m),
            )
            .chain(std::iter::once(kink))
            .collect();
        inboard.sort_by(|a, b| a.y_m.total_cmp(&b.y_m));
        inboard.dedup_by(|a, b| (a.y_m - b.y_m).abs() <= tolerance_m);
        let line = |edge_x_m: fn(&MainWingStation) -> f64| {
            let vertices: Vec<(f64, f64)> = inboard
                .iter()
                .map(|station| (station.y_m, edge_x_m(station)))
                .collect();
            longest_run(&vertices, tolerance_m)
        };
        Some(InboardEdgeLines {
            leading: line(|station| station.leading_edge_x_m)?,
            trailing: line(|station| station.leading_edge_x_m + station.chord_m)?,
            tolerance_m,
        })
    }

    /// Whether a section at `y_m` is a user-defined section, which stays as drawn.
    #[must_use]
    pub fn is_custom_station(&self, y_m: f64) -> bool {
        let tolerance_m = 1.0e-9 * self.tip.y_m;
        self.custom_stations
            .iter()
            .any(|station| (station.y_m - y_m).abs() <= tolerance_m)
    }

    /// This planform with the stations inboard of the edge runs on the two lines: the
    /// wing without its blends, and with the user-defined sections inboard of the kink
    /// among its stations as drawn, as the built wing has them. The root stays the
    /// design datum.
    #[must_use]
    pub fn without_blends(&self) -> Self {
        let Some(lines) = self.inboard_edge_lines() else {
            return self.clone();
        };
        let on_lines = |station: MainWingStation| {
            if self.is_custom_station(station.y_m) {
                return station;
            }
            let (leading_edge_x_m, chord_m) =
                lines.section_at(station.y_m, station.leading_edge_x_m, station.chord_m);
            MainWingStation {
                leading_edge_x_m,
                chord_m,
                ..station
            }
        };
        let mut clean = self.clone();
        clean.side_of_body = self.side_of_body.map(on_lines);
        for station in &mut clean.edge_stations {
            *station = on_lines(*station);
        }
        let inside = (
            self.side_of_body.map_or(0.0, |station| station.y_m),
            self.kink.y_m,
        );
        for &custom in &self.custom_stations {
            let listed = self
                .edge_stations
                .iter()
                .any(|station| (station.y_m - custom.y_m).abs() <= 1.0e-9 * self.tip.y_m);
            if custom.y_m > inside.0 && custom.y_m < inside.1 && !listed {
                clean.edge_stations.push(custom);
            }
        }
        clean.edge_stations.sort_by(|a, b| a.y_m.total_cmp(&b.y_m));
        clean
    }
}

/// The line through the longest straight run of an edge polyline of `(y, x)` vertices
/// in increasing `y`, metres. Vertices on one line (a station that only moves the
/// other edge) do not break a run; the slope comparison only absorbs rounding. Runs
/// within `tie_m` of each other are a tie, and the most inboard one wins: station
/// fractions that make two runs equal must not let the rounding of a span choose
/// between them. `None` with fewer than two vertices or no span.
fn longest_run(vertices: &[(f64, f64)], tie_m: f64) -> Option<InboardEdgeLine> {
    const SAME_LINE_SLOPE: f64 = 1.0e-9;
    let slope = |a: (f64, f64), b: (f64, f64)| (b.1 - a.1) / (b.0 - a.0);
    let mut best: Option<(f64, InboardEdgeLine)> = None;
    let mut start = *vertices.first()?;
    for (index, &end) in vertices.iter().enumerate().skip(1) {
        let run_slope = slope(start, end);
        let continues = vertices
            .get(index + 1)
            .is_some_and(|&next| (slope(end, next) - run_slope).abs() <= SAME_LINE_SLOPE);
        if continues {
            continue;
        }
        let span_m = end.0 - start.0;
        if span_m > 0.0 && best.is_none_or(|(longest_m, _)| span_m > longest_m + tie_m) {
            best = Some((
                span_m,
                InboardEdgeLine {
                    x_at_centreline_m: start.1 - run_slope * start.0,
                    slope: run_slope,
                    run_start_y_m: start.0,
                },
            ));
        }
        start = end;
    }
    best.map(|(_, line)| line)
}

// A failed expect in a test is the assertion failing on a value built here.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::super::planform::MainWingStationKind;
    use super::*;

    #[test]
    fn the_line_is_the_longest_run_not_the_fillet_or_a_blend() {
        // A fillet from a tip 1 m ahead of x = 10 + 0.5 y, then a run with one
        // station on the line, then a short blend into the kink.
        let vertices = [
            (2.0, 10.0),
            (2.5, 11.25),
            (5.0, 12.5),
            (8.0, 14.0),
            (9.0, 14.3),
        ];
        let line = longest_run(&vertices, 1.0e-9).expect("a run");
        assert!((line.x_at_centreline_m - 10.0).abs() < 1e-12, "{line:?}");
        assert!((line.slope - 0.5).abs() < 1e-12, "{line:?}");
        assert!((line.run_start_y_m - 2.5).abs() < 1e-12, "{line:?}");
        assert_eq!(longest_run(&[(2.0, 10.0)], 1.0e-9), None);
    }

    #[test]
    fn two_equal_runs_are_a_tie_the_inboard_one_wins_whatever_the_semispan() {
        // Side of body, one station and the kink at equal span fractions: the two
        // runs differ only by the rounding of `fraction * semispan`, which used to
        // move the apex by the difference of the two lines.
        let inboard_line_at_centreline_m = 9.25;
        for semispan_m in [15.0, 15.00125, 15.0025, 22.3, 31.07, 40.000_001] {
            let (y_a, y_b, y_c) = (0.10 * semispan_m, 0.25 * semispan_m, 0.40 * semispan_m);
            let inboard = |y: f64| inboard_line_at_centreline_m + 0.5 * y;
            let vertices = [
                (y_a, inboard(y_a)),
                (y_b, inboard(y_b)),
                (y_c, inboard(y_b) + 0.9 * (y_c - y_b)),
            ];
            let line = longest_run(&vertices, 1.0e-9 * semispan_m).expect("two runs");
            assert!(
                (line.x_at_centreline_m - inboard_line_at_centreline_m).abs() < 1e-9,
                "semispan {semispan_m}: {line:?}"
            );
        }
    }

    /// A straight run from `y = 2.5` to the kink, a leading-edge fillet from there to the
    /// side of body and a custom section at `custom_y`, its leading edge `off_m` off the
    /// line.
    fn with_custom(custom_y: f64, off_m: f64) -> TransportPlanform {
        let station = |y_m: f64, x_m: f64, chord_m: f64, kind| MainWingStation {
            kind,
            span_fraction: y_m / 20.0,
            y_m,
            leading_edge_x_m: x_m,
            chord_m,
        };
        let le = |y: f64| 1.0 + 0.5 * y;
        let te = |y: f64| 12.0 + 0.1 * y;
        let on = |y: f64, kind| station(y, le(y), te(y) - le(y), kind);
        let mut filleted = on(2.0, MainWingStationKind::SideOfBody);
        filleted.leading_edge_x_m -= 0.5;
        filleted.chord_m += 0.5;
        let mut custom = on(custom_y, MainWingStationKind::EdgeStation);
        custom.leading_edge_x_m += off_m;
        custom.chord_m -= off_m;
        TransportPlanform {
            root: on(0.0, MainWingStationKind::Root),
            side_of_body: Some(filleted),
            kink: on(8.0, MainWingStationKind::Kink),
            tip: station(20.0, 18.0, 2.0, MainWingStationKind::Tip),
            tip_rake: None,
            te_breaks: Vec::new(),
            edge_stations: vec![on(2.5, MainWingStationKind::EdgeStation)],
            custom_stations: vec![custom],
            inboard_le_sweep_deg: 26.6,
            outboard_le_sweep_deg: 26.6,
        }
    }

    #[test]
    fn a_custom_section_on_the_run_does_not_break_it_and_one_off_the_run_does() {
        let on_the_run = with_custom(5.0, 0.0).inboard_edge_lines().expect("lines");
        assert!((on_the_run.leading.run_start_y_m - 2.5).abs() < 1e-12);
        assert!((on_the_run.leading.slope - 0.5).abs() < 1e-12);

        // 0.4 m off the line at y = 6: the run 2.5-6 is longest, and it is not the
        // straight 2.5-8 run the leading edge had without the section.
        let off = with_custom(6.0, 0.4).inboard_edge_lines().expect("lines");
        assert!((off.leading.run_start_y_m - 2.5).abs() < 1e-12, "{off:?}");
        assert!((off.leading.slope - 0.5).abs() > 1e-3, "{off:?}");
    }

    #[test]
    fn two_equal_runs_around_a_custom_section_are_a_tie_the_inboard_one_wins() {
        // 2.5-5.25 and 5.25-8 have the same length.
        let lines = with_custom(5.25, 0.4).inboard_edge_lines().expect("lines");
        assert!(
            (lines.leading.run_start_y_m - 2.5).abs() < 1e-12,
            "{lines:?}"
        );
    }

    #[test]
    fn a_custom_section_never_moves_and_the_clean_planform_lists_it() {
        // Inside the fillet, inboard of the start of the run: the side of body goes to
        // the line, the custom section stays as drawn and in the clean stations.
        let planform = with_custom(2.25, 0.3);
        let lines = planform.inboard_edge_lines().expect("lines");
        assert!(lines.replaces_at(2.25), "{lines:?}");
        let clean = planform.without_blends();
        let at = |y: f64| {
            clean
                .edge_stations
                .iter()
                .find(|station| (station.y_m - y).abs() < 1e-12)
                .copied()
                .expect("listed")
        };
        assert_eq!(at(2.25), planform.custom_stations[0]);
        let side_of_body = clean.side_of_body.expect("side of body");
        assert!((side_of_body.leading_edge_x_m - 2.0).abs() < 1e-12);
        assert_eq!(clean.without_blends().edge_stations, clean.edge_stations);
    }

    #[test]
    fn a_section_with_neither_edge_replaced_comes_back_exactly() {
        let line = |run_start_y_m| InboardEdgeLine {
            x_at_centreline_m: 1.0,
            slope: 0.1,
            run_start_y_m,
        };
        let lines = InboardEdgeLines {
            leading: line(4.0),
            trailing: line(2.0),
            tolerance_m: 1.0e-9,
        };
        assert_eq!(lines.section_at(4.0, 7.3, 0.1 + 0.2), (7.3, 0.1 + 0.2));
        // Inside the leading run's start only: the drawn trailing edge stays.
        let (leading, chord) = lines.section_at(3.0, 7.3, 2.0);
        assert!((leading - 1.3).abs() < 1e-12);
        assert!((leading + chord - 9.3).abs() < 1e-12);
    }
}
