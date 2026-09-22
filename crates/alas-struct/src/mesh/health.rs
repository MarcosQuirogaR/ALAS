// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The five geometric checks that catch a corrupt wingbox mesh.
//!
//! These are not optional diagnostics. Each one caught a real defect in the
//! reference scripts this mesh generalizes, and two of them are fatal: a
//! triangle with no area means the zipper bridging produced nonsense, and a
//! grid inboard of the root plane means the rib truncation did. The other
//! three come back as warnings, which is the reference's own severity split,
//! only the two fatal ones ever stopped a run there.

use alas_geom::wing_structure::{RibStation, WingStructureGeometry};

use super::build::RibRegions;
use super::cards::Deck;
use super::{MeshError, MeshHealthReport, WARPING_THRESHOLD};

/// Deviation from straight, in metres, at which a spar is reported.
const SPAR_STRAIGHTNESS_LIMIT_M: f64 = 1e-3;

/// How far inboard of the root plane a grid may sit before the mesh is
/// declared corrupt, metres. A grid exactly on the plane rounds either way.
const ROOT_PLANE_TOLERANCE_M: f64 = 0.001;

/// Run every health check over a finished deck: `_check_mesh_health`.
///
/// # Errors
///
/// [`MeshError`] for the two findings that mean the mesh cannot be solved.
pub(super) fn check_mesh_health(
    deck: &Deck,
    stations: &[RibStation],
    regions: &RibRegions,
    spar_upper: &[Vec<i64>],
    wsg: &WingStructureGeometry,
) -> Result<MeshHealthReport, MeshError> {
    let mut report = MeshHealthReport {
        n_cquad4: deck.quads().len(),
        n_ctria3: deck.trias().len(),
        ..Default::default()
    };
    let mut warnings = Vec::new();

    check_perpendicularity(stations, regions, wsg, &mut report, &mut warnings);
    check_warping(deck, &mut report, &mut warnings);
    check_degenerate_triangles(deck, &mut report)?;
    check_spar_straightness(deck, spar_upper, wsg, &mut report, &mut warnings);
    check_root_plane(deck)?;

    report.warnings = warnings;
    Ok(report)
}

/// Each rib's realized cut should come out perpendicular to the local leading
/// edge.
///
/// This is a self-consistency regression check rather than a geometric
/// requirement: the rib direction and the leading-edge tangent are formulated
/// independently, so a nonzero dot product means the two formulas have drifted
/// apart, or that grids were not placed along the direction the rib was cut in.
/// The root rib is exempt, being deliberately streamwise.
fn check_perpendicularity(
    stations: &[RibStation],
    regions: &RibRegions,
    wsg: &WingStructureGeometry,
    report: &mut MeshHealthReport,
    warnings: &mut Vec<String>,
) {
    for &rib in &regions.all_structural {
        let station = &stations[rib];
        if station.eta <= 1e-9 {
            continue;
        }
        let (Some(first), Some(last)) = (station.extrados.first(), station.extrados.last()) else {
            continue;
        };
        if station.extrados.len() < 2 {
            continue;
        }
        let chordwise = [last[0] - first[0], last[1] - first[1]];
        let length = (chordwise[0].powi(2) + chordwise[1].powi(2)).sqrt();
        if length < 1e-10 {
            continue;
        }
        let (le_x, le_y) = wsg.le_direction(station.eta);
        let dot = (chordwise[0] / length) * le_x + (chordwise[1] / length) * le_y;
        if dot.abs() < 0.05 {
            continue;
        }
        report.n_perp_warnings += 1;
        warnings.push(format!(
            "Rib {rib} (y={:.2} m) not perpendicular to local LE (dot={dot:+.3}).",
            station.y_station
        ));
    }
}

/// How far each quadrilateral's corners sit out of their own mean plane, as a
/// fraction of the panel's diagonal length.
fn check_warping(deck: &Deck, report: &mut MeshHealthReport, warnings: &mut Vec<String>) {
    let mut coefficients = Vec::new();
    for shell in deck.quads() {
        let Some(corners) = corner_coordinates(deck, &shell.nodes) else {
            continue;
        };
        let first_diagonal = subtract(corners[2], corners[0]);
        let second_diagonal = subtract(corners[3], corners[1]);
        let normal = cross(first_diagonal, second_diagonal);
        let magnitude = norm(normal);
        if magnitude < 1e-12 {
            continue;
        }
        let unit = [
            normal[0] / magnitude,
            normal[1] / magnitude,
            normal[2] / magnitude,
        ];
        let centre = [
            corners.iter().map(|c| c[0]).sum::<f64>() / 4.0,
            corners.iter().map(|c| c[1]).sum::<f64>() / 4.0,
            corners.iter().map(|c| c[2]).sum::<f64>() / 4.0,
        ];
        let offset = corners
            .iter()
            .map(|corner| dot(subtract(*corner, centre), unit).abs())
            .fold(f64::NEG_INFINITY, f64::max);
        coefficients.push(offset / (2.0 * (norm(first_diagonal) + norm(second_diagonal))));
    }

    if coefficients.is_empty() {
        return;
    }
    report.warping_max = coefficients
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    report.warping_mean = coefficients.iter().sum::<f64>() / coefficients.len() as f64;
    report.n_warping_bad = coefficients
        .iter()
        .filter(|&&value| value > WARPING_THRESHOLD)
        .count();
    if report.n_warping_bad > 0 {
        warnings.push(format!(
            "{} CQUAD4 elements exceed the warping threshold ({:.0}%); max={:.3}.",
            report.n_warping_bad,
            WARPING_THRESHOLD * 100.0,
            report.warping_max
        ));
    }
}

/// A triangle with two identical corners, or with no area, means the zipper
/// bridging built an invalid panel.
fn check_degenerate_triangles(deck: &Deck, report: &mut MeshHealthReport) -> Result<(), MeshError> {
    let mut degenerate = 0;
    for shell in deck.trias() {
        let unique: std::collections::HashSet<i64> = shell.nodes.iter().copied().collect();
        if unique.len() < 3 {
            degenerate += 1;
            continue;
        }
        let Some(corners) = corner_coordinates(deck, &shell.nodes) else {
            continue;
        };
        let area = 0.5
            * norm(cross(
                subtract(corners[1], corners[0]),
                subtract(corners[2], corners[0]),
            ));
        if area < 1e-9 {
            degenerate += 1;
        }
    }
    if report.n_ctria3 > 0 {
        report.triangle_ratio =
            report.n_ctria3 as f64 / (report.n_ctria3 + report.n_cquad4).max(1) as f64;
    }
    if degenerate > 0 {
        return Err(MeshError::DegenerateTriangles { count: degenerate });
    }
    Ok(())
}

/// Each spar's upper grid line should be straight in plan, in two pieces: root
/// to break, and break to tip.
fn check_spar_straightness(
    deck: &Deck,
    spar_upper: &[Vec<i64>],
    wsg: &WingStructureGeometry,
    report: &mut MeshHealthReport,
    warnings: &mut Vec<String>,
) {
    for (spar, nids) in spar_upper.iter().enumerate() {
        let Some(&fraction) = wsg.spar_fracs.get(spar) else {
            continue;
        };
        if nids.len() < 3 {
            continue;
        }
        let points: Vec<[f64; 3]> = nids
            .iter()
            .map(|&nid| deck.grid_xyz(nid).unwrap_or([f64::NAN; 3]))
            .collect();

        let mut kink = 0;
        let mut closest = f64::INFINITY;
        for (index, point) in points.iter().enumerate() {
            let gap = (point[1] - wsg.y_break).abs();
            if gap < closest {
                closest = gap;
                kink = index;
            }
        }
        let kink = kink.clamp(1, points.len() - 2);

        let mut deviation: f64 = 0.0;
        for segment in [&points[..=kink], &points[kink..]] {
            if segment.len() < 3 {
                continue;
            }
            let (Some(start), Some(end)) = (segment.first(), segment.last()) else {
                continue;
            };
            let along = [end[0] - start[0], end[1] - start[1]];
            let length_squared = along[0] * along[0] + along[1] * along[1];
            if length_squared < 1e-20 {
                continue;
            }
            for point in &segment[1..segment.len() - 1] {
                let offset = [point[0] - start[0], point[1] - start[1]];
                let t = (offset[0] * along[0] + offset[1] * along[1]) / length_squared;
                let foot = [start[0] + t * along[0], start[1] + t * along[1]];
                let gap = ((point[0] - foot[0]).powi(2) + (point[1] - foot[1]).powi(2)).sqrt();
                deviation = deviation.max(gap);
            }
        }

        report
            .spar_straightness_max_dev_m
            .push((fraction, deviation));
        if deviation >= SPAR_STRAIGHTNESS_LIMIT_M {
            report.n_spar_straightness_warnings += 1;
            warnings.push(format!(
                "Spar x/c={fraction:.2} deviates {:.2} mm from straight.",
                deviation * 1000.0
            ));
        }
    }
}

/// The semi-wing model has no meaning inboard of the root plane, so a grid
/// there means the rib truncation failed.
fn check_root_plane(deck: &Deck) -> Result<(), MeshError> {
    let below: Vec<f64> = deck
        .grids()
        .iter()
        .map(|grid| grid.xyz[1])
        .filter(|&y| y < -ROOT_PLANE_TOLERANCE_M)
        .collect();
    if below.is_empty() {
        return Ok(());
    }
    let worst = below.iter().copied().fold(f64::INFINITY, f64::min);
    Err(MeshError::NodeBelowRoot {
        count: below.len(),
        worst,
    })
}

/// The coordinates of an element's corners, or `None` if the deck has lost one.
fn corner_coordinates(deck: &Deck, nids: &[i64]) -> Option<Vec<[f64; 3]>> {
    nids.iter().map(|&nid| deck.grid_xyz(nid)).collect()
}

fn subtract(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::cards::Shell;

    fn deck_with(points: &[[f64; 3]]) -> Deck {
        let mut deck = Deck::new();
        for (index, xyz) in points.iter().enumerate() {
            deck.add_grid(index as i64 + 1, *xyz);
        }
        deck
    }

    #[test]
    fn a_flat_panel_has_no_warping_and_a_folded_one_does() {
        let mut deck = deck_with(&[
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ]);
        deck.quads.push(Shell {
            eid: 1,
            pid: 1,
            nodes: vec![1, 2, 3, 4],
        });
        let mut report = MeshHealthReport::default();
        let mut warnings = Vec::new();
        check_warping(&deck, &mut report, &mut warnings);
        assert!(report.warping_max < 1e-15);
        assert_eq!(report.n_warping_bad, 0);
        assert!(warnings.is_empty());

        // A saddle: the two diagonals stay unit length while their midpoints
        // separate, which is exactly what the coefficient measures.
        let mut folded = deck_with(&[
            [0.0, 0.0, 0.0],
            [0.5, -0.5, 0.5],
            [1.0, 0.0, 0.0],
            [0.5, 0.5, 0.5],
        ]);
        folded.quads.push(Shell {
            eid: 1,
            pid: 1,
            nodes: vec![1, 2, 3, 4],
        });
        let mut report = MeshHealthReport::default();
        let mut warnings = Vec::new();
        check_warping(&folded, &mut report, &mut warnings);
        assert!(report.warping_max > WARPING_THRESHOLD);
        assert_eq!(report.n_warping_bad, 1);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn a_triangle_with_a_repeated_corner_is_fatal() {
        let mut deck = deck_with(&[[0.0; 3], [1.0, 0.0, 0.0]]);
        deck.trias.push(Shell {
            eid: 1,
            pid: 1,
            nodes: vec![1, 2, 2],
        });
        let mut report = MeshHealthReport {
            n_ctria3: 1,
            ..Default::default()
        };
        assert_eq!(
            check_degenerate_triangles(&deck, &mut report),
            Err(MeshError::DegenerateTriangles { count: 1 })
        );
    }

    #[test]
    fn three_collinear_corners_are_fatal_even_though_they_are_distinct() {
        let mut deck = deck_with(&[[0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]]);
        deck.trias.push(Shell {
            eid: 1,
            pid: 1,
            nodes: vec![1, 2, 3],
        });
        let mut report = MeshHealthReport {
            n_ctria3: 1,
            ..Default::default()
        };
        assert!(check_degenerate_triangles(&deck, &mut report).is_err());
    }

    #[test]
    fn a_grid_inboard_of_the_root_plane_is_fatal_and_reports_the_worst_one() {
        let deck = deck_with(&[[0.0, 0.0, 0.0], [0.0, -0.5, 0.0], [0.0, -1.25, 0.0]]);
        assert_eq!(
            check_root_plane(&deck),
            Err(MeshError::NodeBelowRoot {
                count: 2,
                worst: -1.25
            })
        );
    }

    #[test]
    fn a_grid_a_hair_inboard_is_within_tolerance() {
        let deck = deck_with(&[[0.0, -0.0005, 0.0]]);
        assert!(check_root_plane(&deck).is_ok());
    }
}
