// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py: figure_structures_modes
// (L5969-6080) and _nearest_freq_index (L5954-5966).

//! Rayleigh-versus-MSC/NASTRAN-95 normal-mode frequencies and spanwise shapes.
//!
//! Each solver's modes are paired to the analytical trial modes by MAC (a
//! greedy one-to-one assignment on modal assurance, falling back to frequency when no shapes exist), not by list position. A
//! real finite-element solve may contain torsional or local-panel modes that
//! have no Rayleigh counterpart.

use alas_pipeline::structural::StructuralAnalysisResult;
use alas_struct::nastran::ResultStatus;

use super::layout::{draw_bar, panel_rects};
use super::status::{resolve_structural_result, status_message_scene, with_alpha, TAB10};
use crate::chart_kit::{draw_legend, draw_title, LegendMarker};
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

const TITLE: &str = "Structural Analysis: Modes";
const BLUE: &str = "tab:blue";
const RED: &str = "tab:red";
const ORANGE: &str = "tab:orange";

/// Return the index of the NASTRAN frequency nearest to an analytical one.
///
/// This helper is retained for callers that need one independent lookup. The
/// renderer uses [`match_frequency_indices`] instead, because a mode identity
/// must not be reused for two analytical modes.
pub fn nearest_frequency_index(frequency_hz: f64, nastran_frequencies_hz: &[f64]) -> Option<usize> {
    nastran_frequencies_hz
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (*a - frequency_hz)
                .abs()
                .total_cmp(&(*b - frequency_hz).abs())
        })
        .map(|(index, _)| index)
}

#[derive(Debug, Clone)]
struct AssignmentScore {
    matched: usize,
    frequency_cost: f64,
    /// Stable tie-break key: frequency-sorted analytical/solver positions.
    /// Equal-cardinality/equal-cost assignments prefer the earliest
    /// analytical mode, then the earliest solver mode.
    matched_pairs: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, Copy)]
struct AssignmentParent {
    previous_i: usize,
    previous_j: usize,
    matched: bool,
}

fn score_is_better(candidate: &AssignmentScore, incumbent: &AssignmentScore) -> bool {
    candidate.matched > incumbent.matched
        || (candidate.matched == incumbent.matched
            && (candidate
                .frequency_cost
                .total_cmp(&incumbent.frequency_cost)
                .is_lt()
                || (candidate
                    .frequency_cost
                    .total_cmp(&incumbent.frequency_cost)
                    .is_eq()
                    && candidate.matched_pairs < incumbent.matched_pairs)))
}

fn update_assignment(
    scores: &mut [Vec<Option<AssignmentScore>>],
    parents: &mut [Vec<Option<AssignmentParent>>],
    next_i: usize,
    next_j: usize,
    candidate: AssignmentScore,
    parent: AssignmentParent,
) {
    let replace = scores[next_i][next_j]
        .as_ref()
        .is_none_or(|incumbent| score_is_better(&candidate, incumbent));
    if replace {
        scores[next_i][next_j] = Some(candidate);
        parents[next_i][next_j] = Some(parent);
    }
}

/// Pair analytical modes with solver modes using a deterministic one-to-one
/// frequency assignment.
///
/// A finite-element solution can contain local or torsional modes that have
/// no Rayleigh counterpart, so unmatched solver modes are allowed. The
/// assignment maximizes the number of matched analytical modes first and then
/// minimizes the sum of absolute frequency differences. Non-finite
/// frequencies remain unmatched. This solves mode identity before any sign
/// alignment; changing an eigenvector's sign cannot repair a duplicated or
/// swapped pairing.
pub fn match_frequency_indices(
    analytical_frequencies_hz: &[f64],
    solver_frequencies_hz: &[f64],
) -> Vec<Option<usize>> {
    let mut analytical_order: Vec<usize> = analytical_frequencies_hz
        .iter()
        .enumerate()
        .filter(|(_, frequency)| frequency.is_finite())
        .map(|(index, _)| index)
        .collect();
    analytical_order.sort_by(|&left, &right| {
        analytical_frequencies_hz[left]
            .total_cmp(&analytical_frequencies_hz[right])
            .then_with(|| left.cmp(&right))
    });
    let mut solver_order: Vec<usize> = solver_frequencies_hz
        .iter()
        .enumerate()
        .filter(|(_, frequency)| frequency.is_finite())
        .map(|(index, _)| index)
        .collect();
    solver_order.sort_by(|&left, &right| {
        solver_frequencies_hz[left]
            .total_cmp(&solver_frequencies_hz[right])
            .then_with(|| left.cmp(&right))
    });

    let n = analytical_order.len();
    let m = solver_order.len();
    let mut scores = vec![vec![None; m + 1]; n + 1];
    let mut parents = vec![vec![None; m + 1]; n + 1];
    scores[0][0] = Some(AssignmentScore {
        matched: 0,
        frequency_cost: 0.0,
        matched_pairs: Vec::new(),
    });

    for i in 0..=n {
        for j in 0..=m {
            let Some(score) = scores[i][j].clone() else {
                continue;
            };
            if i < n {
                update_assignment(
                    &mut scores,
                    &mut parents,
                    i + 1,
                    j,
                    score.clone(),
                    AssignmentParent {
                        previous_i: i,
                        previous_j: j,
                        matched: false,
                    },
                );
            }
            if j < m {
                update_assignment(
                    &mut scores,
                    &mut parents,
                    i,
                    j + 1,
                    score.clone(),
                    AssignmentParent {
                        previous_i: i,
                        previous_j: j,
                        matched: false,
                    },
                );
            }
            if i < n && j < m {
                let frequency_cost = (analytical_frequencies_hz[analytical_order[i]]
                    - solver_frequencies_hz[solver_order[j]])
                    .abs();
                update_assignment(
                    &mut scores,
                    &mut parents,
                    i + 1,
                    j + 1,
                    AssignmentScore {
                        matched: score.matched + 1,
                        frequency_cost: score.frequency_cost + frequency_cost,
                        matched_pairs: score
                            .matched_pairs
                            .iter()
                            .copied()
                            .chain(std::iter::once((i, j)))
                            .collect(),
                    },
                    AssignmentParent {
                        previous_i: i,
                        previous_j: j,
                        matched: true,
                    },
                );
            }
        }
    }

    let mut matches = vec![None; analytical_frequencies_hz.len()];
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        let Some(parent) = parents[i][j] else {
            break;
        };
        if parent.matched {
            matches[analytical_order[i - 1]] = Some(solver_order[j - 1]);
        }
        (i, j) = (parent.previous_i, parent.previous_j);
    }
    matches
}

/// A pairing is called a match only at or above this modal assurance
/// criterion. MAC = (a.b)^2 / ((a.a)(b.b)) is 1 for identical shapes up to
/// scale and sign and 0 for orthogonal shapes; 0.8 is the customary floor for
/// "same mode" in test/analysis correlation.
pub const MAC_PASS: f64 = 0.8;

/// A solver mode is paired to a beam mode only at or above this MAC. A solver
/// mode whose MAC with *every* beam mode is below it is drawn as a local /
/// chordwise / torsion-dominated mode instead of being forced onto the beam
/// mode that happens to be least wrong.
pub const MAC_PAIRING_FLOOR: f64 = 0.5;

/// A solver mode is torsion-dominated when the peak front/rear half-difference
/// of its spar z, `twist * c / 2` with `c` the spar-to-spar distance, exceeds
/// the peak bending deflection `w` by this ratio: 1.0 means equal tip
/// contributions. It is a physical equal-contribution point, not a value tuned
/// per preset.
pub const TORSION_RATIO_LIMIT: f64 = 1.0;

/// Colours of the local-mode curves are not used any more: local and
/// torsion-dominated modes are listed as text, not drawn.
const CAPTION_NORMALISATION: &str = "Bending w = mean of the front- and rear-spar z (elastic axis taken at the midpoint between the spars), divided by max |w| along the span and sign-flipped so the tip is positive. Markers are mesh stations.";
const CAPTION_NORMALISATION_FRONT_ONLY: &str = "Solver shapes: out-of-plane z along the front-spar line, divided by max |z| along the spar and sign-flipped so the tip is positive.";
const CAPTION_MAC: &str = "MAC = (a.b)^2 / ((a.a)(b.b)) between a solver bending shape and a Rayleigh beam mode. A mode with MAC < 0.5 against every beam mode, or torsion-dominated, is listed below and not paired.";
const CAPTION_TORSION: &str = "Twist = (z_front - z_rear) / (x_rear - x_front). Torsion-dominated when twist*c/2 / w > 1.0 (peaks over the span, c = spar-to-spar distance), i.e. the front/rear difference exceeds the bending.";
const CAPTION_NO_TORSION: &str = "No torsion reference: the solver gave front-spar z only, so twist and twist*c/2 / w are not evaluated.";

/// Bending/torsion view of one solver's modes.
struct ModeView {
    /// Per mode, the bending shape: `w = (z_front + z_rear) / 2` when both
    /// spars were exported, else the front-spar shape of the reader.
    w: Vec<Vec<f64>>,
    /// Per mode, `peak|z_front - z_rear| / 2 / peak|w|` (`twist*c/2 / w`).
    /// `None` when no rear-spar data exist.
    torsion_ratio: Vec<Option<f64>>,
    /// Whether front and rear spars were both available.
    decomposed: bool,
}

fn mode_view(modes: &alas_struct::nastran::ModesResult) -> ModeView {
    let n = modes.mode_shapes.len();
    let decomposed = n > 0
        && modes.spar_z_front.len() == n
        && modes.spar_z_rear.len() == n
        && modes
            .spar_z_front
            .iter()
            .zip(&modes.spar_z_rear)
            .all(|(f, r)| f.len() == r.len());
    if !decomposed {
        return ModeView {
            w: modes.mode_shapes.clone(),
            torsion_ratio: vec![None; n],
            decomposed: false,
        };
    }
    let mut w = Vec::with_capacity(n);
    let mut torsion_ratio = Vec::with_capacity(n);
    for (front, rear) in modes.spar_z_front.iter().zip(&modes.spar_z_rear) {
        let mean: Vec<f64> = front.iter().zip(rear).map(|(f, r)| 0.5 * (f + r)).collect();
        let peak = |values: &mut dyn Iterator<Item = f64>| {
            values
                .filter(|v| v.is_finite())
                .map(f64::abs)
                .fold(0.0, f64::max)
        };
        let w_peak = peak(&mut mean.iter().copied());
        let half_diff = peak(&mut front.iter().zip(rear).map(|(f, r)| 0.5 * (f - r)));
        torsion_ratio.push(if w_peak > 0.0 {
            Some(half_diff / w_peak)
        } else if half_diff > 0.0 {
            Some(f64::INFINITY)
        } else {
            None
        });
        w.push(mean);
    }
    ModeView {
        w,
        torsion_ratio,
        decomposed,
    }
}

/// A solver mode with no beam counterpart.
#[derive(Debug, Clone, Copy)]
struct LocalMode {
    /// Index into the solver's frequency and shape lists.
    solver_index: usize,
    /// Its highest MAC against any beam mode, if one could be computed.
    best_mac: Option<f64>,
    /// `twist*c/2 / w` when it exceeds [`TORSION_RATIO_LIMIT`]: the mode is
    /// torsion-dominated rather than merely unmatched.
    torsion_ratio: Option<f64>,
}

/// Analytical-to-solver mode pairing with the evidence it rests on.
#[derive(Debug, Clone)]
struct ModePairing {
    /// Per analytical mode, the paired solver mode index.
    matches: Vec<Option<usize>>,
    /// Per analytical mode, the MAC of that pair. `None` when the pairing was
    /// made on frequency alone because the solver reported no shapes.
    mac: Vec<Option<f64>>,
    /// Solver modes that are torsion-dominated or whose MAC with every beam
    /// mode is below [`MAC_PAIRING_FLOOR`].
    local: Vec<LocalMode>,
}

impl ModePairing {
    fn by_mac(&self) -> bool {
        self.mac.iter().any(Option::is_some)
    }

    fn failing(&self) -> usize {
        self.mac
            .iter()
            .flatten()
            .filter(|mac| **mac < MAC_PASS)
            .count()
    }

    fn paired(&self) -> usize {
        self.matches.iter().flatten().count()
    }
}

/// Trapezoid-weighted inner product of two shapes on common stations.
fn weighted_dot(weights: &[f64], a: &[f64], b: &[f64]) -> f64 {
    weights
        .iter()
        .zip(a.iter().zip(b))
        .map(|(w, (x, y))| w * x * y)
        .sum()
}

/// Modal assurance criterion of two shapes sampled at `stations`.
fn mac_on_stations(stations: &[f64], a: &[f64], b: &[f64]) -> Option<f64> {
    let n = stations.len().min(a.len()).min(b.len());
    if n < 2 {
        return None;
    }
    let weights: Vec<f64> = (0..n)
        .map(|i| {
            let lo = if i == 0 { stations[0] } else { stations[i - 1] };
            let hi = if i + 1 == n {
                stations[n - 1]
            } else {
                stations[i + 1]
            };
            0.5 * (hi - lo).abs()
        })
        .collect();
    let (a, b) = (&a[..n], &b[..n]);
    let (ab, aa, bb) = (
        weighted_dot(&weights, a, b),
        weighted_dot(&weights, a, a),
        weighted_dot(&weights, b, b),
    );
    let mac = ab * ab / (aa * bb);
    mac.is_finite().then_some(mac)
}

/// Pair each analytical mode with the solver mode of highest MAC, using each
/// solver mode at most once (greedy on descending MAC, ties to the lowest
/// indices). Solver shapes are compared with the analytical shapes
/// interpolated onto the solver's own stations. Without solver shapes the
/// pairing falls back to the frequency assignment and carries no MAC.
fn pair_modes(
    analysis: &alas_struct::analytical::StructuralAnalysisReport,
    modes: Option<&alas_struct::nastran::ModesResult>,
    solver_frequencies: &[f64],
) -> ModePairing {
    let n = analysis.modal.frequencies_hz.len();
    let by_frequency = || ModePairing {
        matches: match_frequency_indices(&analysis.modal.frequencies_hz, solver_frequencies),
        mac: vec![None; n],
        local: Vec::new(),
    };
    let Some(modes) = modes else {
        return by_frequency();
    };
    let Some(stations) = modes.mode_shape_y_m.as_ref() else {
        return by_frequency();
    };
    let view = mode_view(modes);
    let mut candidates: Vec<(f64, usize, usize)> = Vec::new();
    for (i, shape) in analysis.modal.mode_shapes.iter().enumerate().take(n) {
        let reference: Vec<f64> = stations
            .iter()
            .map(|&y| interpolate_shape(&analysis.y, shape, y).unwrap_or(f64::NAN))
            .collect();
        for (j, solver_shape) in view.w.iter().enumerate() {
            let pairs: Vec<(f64, f64, f64)> = stations
                .iter()
                .zip(reference.iter().zip(solver_shape))
                .filter(|(y, (r, s))| y.is_finite() && r.is_finite() && s.is_finite())
                .map(|(y, (r, s))| (*y, *r, *s))
                .collect();
            let ys: Vec<f64> = pairs.iter().map(|p| p.0).collect();
            let rs: Vec<f64> = pairs.iter().map(|p| p.1).collect();
            let ss: Vec<f64> = pairs.iter().map(|p| p.2).collect();
            if let Some(mac) = mac_on_stations(&ys, &rs, &ss) {
                candidates.push((mac, i, j));
            }
        }
    }
    if candidates.is_empty() {
        return by_frequency();
    }
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let mut matches = vec![None; n];
    let mut mac = vec![None; n];
    let mut used = std::collections::BTreeSet::new();
    let mut best_by_solver: std::collections::BTreeMap<usize, f64> = Default::default();
    for &(value, _, j) in &candidates {
        let best = best_by_solver.entry(j).or_insert(value);
        *best = best.max(value);
    }
    let torsion_of = |j: usize| {
        view.torsion_ratio
            .get(j)
            .copied()
            .flatten()
            .filter(|ratio| *ratio > TORSION_RATIO_LIMIT)
    };
    for (value, i, j) in candidates {
        if value >= MAC_PAIRING_FLOOR
            && torsion_of(j).is_none()
            && matches[i].is_none()
            && used.insert(j)
        {
            matches[i] = Some(j);
            mac[i] = Some(value);
        }
    }
    let local = (0..view.w.len())
        .filter_map(|solver_index| {
            let best_mac = best_by_solver.get(&solver_index).copied();
            let torsion_ratio = torsion_of(solver_index);
            (torsion_ratio.is_some() || best_mac.is_some_and(|mac| mac < MAC_PAIRING_FLOOR))
                .then_some(LocalMode {
                    solver_index,
                    best_mac,
                    torsion_ratio,
                })
        })
        .collect();
    ModePairing {
        matches,
        mac,
        local,
    }
}

/// Legend label for a solver's frequency bars, stating what the pairing is.
fn pairing_label(solver: &str, pairing: &ModePairing) -> String {
    if !pairing.by_mac() {
        return format!("{solver} (paired by frequency only: no mode shapes, unverified)");
    }
    let failing = pairing.failing();
    if failing == 0 {
        format!(
            "{solver} (paired by MAC, all {} >= {MAC_PASS:.2})",
            pairing.paired()
        )
    } else {
        format!(
            "{solver} (paired by MAC, {failing} of {} below {MAC_PASS:.2})",
            pairing.paired()
        )
    }
}

/// Natural frequencies and their Rayleigh/MSC/NASTRAN-95 mode-shape comparison.
pub fn figure_structures_modes(
    result: Option<&StructuralAnalysisResult>,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let result = match resolve_structural_result(result, TITLE, pal) {
        Ok(result) => result,
        Err(scene) => return scene,
    };
    let Some(analysis) = result.analysis.as_ref() else {
        return status_message_scene(
            TITLE,
            "Structural analysis reported \"ok\" but has no modal data.",
            false,
            pal,
        );
    };
    if analysis.modal.frequencies_hz.is_empty() {
        return status_message_scene(
            TITLE,
            "The public structural result contains no analytical modal frequencies to plot.",
            false,
            pal,
        );
    }

    let msc_modes = result.nastran.as_ref().map(|n| &n.modes);
    let has_msc =
        msc_modes.is_some_and(|m| m.status == ResultStatus::Ok && !m.frequencies_hz.is_empty());
    let msc_frequencies = msc_modes
        .filter(|_| has_msc)
        .map(|m| m.frequencies_hz.as_slice())
        .unwrap_or(&[]);
    let n95_modes = result.nastran95.as_ref().map(|n| &n.modes);
    let has_n95 =
        n95_modes.is_some_and(|m| m.status == ResultStatus::Ok && !m.frequencies_hz.is_empty());
    let n95_frequencies = n95_modes
        .filter(|_| has_n95)
        .map(|m| m.frequencies_hz.as_slice())
        .unwrap_or(&[]);
    // Pair modes before changing their arbitrary global signs.  Independent
    // nearest-frequency lookups can reuse one solver mode for two Rayleigh
    // modes when frequencies are close, which makes the second curve look
    // like a sign error while it is actually a mode-identity error.  The
    // assignment is one-to-one and deterministic; a sign flip is applied only
    // after this pairing has been selected.
    let msc_pairing = pair_modes(analysis, msc_modes.filter(|_| has_msc), msc_frequencies);
    let n95_pairing = pair_modes(analysis, n95_modes.filter(|_| has_n95), n95_frequencies);
    let msc_view = msc_modes.filter(|_| has_msc).map(mode_view);
    let n95_view = n95_modes.filter(|_| has_n95).map(mode_view);
    let decomposed = msc_view.iter().chain(n95_view.iter()).any(|v| v.decomposed);
    let msc_name = if msc_modes.is_some_and(|_| {
        result
            .nastran
            .as_ref()
            .is_some_and(|n| n.solver_is_nastran95)
    }) {
        "NASTRAN-95"
    } else if has_n95 {
        "MSC NASTRAN"
    } else {
        "NASTRAN"
    };
    let msc_matches = msc_pairing.matches.clone();
    let n95_matches = n95_pairing.matches.clone();
    let msc_shape_available = has_matched_shape(msc_modes, &msc_matches);
    let n95_shape_available = has_matched_shape(n95_modes, &n95_matches);

    let mut scene = Scene::new(1600.0, 585.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Structural normal modes".to_owned());
    draw_title(&mut scene, "Structural normal modes", pal);
    scene.suppress_derived_title();
    let rects = panel_rects(3, (60.0, 55.0, 1500.0, 420.0), 32.0);
    let n_modes = analysis.modal.frequencies_hz.len();
    let x_range = if n_modes == 0 {
        (0.5, 1.5)
    } else {
        (0.5, n_modes as f64 + 0.5)
    };
    let max_frequency = analysis
        .modal
        .frequencies_hz
        .iter()
        .copied()
        .chain(msc_frequencies.iter().copied())
        .chain(n95_frequencies.iter().copied())
        .filter(|v| v.is_finite() && *v >= 0.0)
        .fold(0.0, f64::max)
        .max(1.0)
        * 1.1;

    let frequency_axes = Axes2D::new(rects[0], x_range, (0.0, max_frequency));
    frequency_axes.draw_frame_with_labels(&mut scene, pal, "Mode", "Frequency [Hz]");
    let solver_count = usize::from(has_msc) + usize::from(has_n95);
    let bar_width = if solver_count == 0 {
        0.5
    } else {
        0.8 / (solver_count + 1) as f64
    };
    let group_width = bar_width * (solver_count + 1) as f64;
    for (index, &frequency) in analysis.modal.frequencies_hz.iter().enumerate() {
        let center = index as f64 + 1.0;
        let group_left = center - group_width / 2.0;
        let left = group_left;
        draw_bar(
            &mut scene,
            &frequency_axes,
            left,
            left + bar_width,
            frequency.max(0.0),
            Fill::new(Color::rgba(31, 119, 180, 224)),
        );
        if let Some(Some(msc_index)) = msc_matches.get(index) {
            let right = group_left + bar_width;
            draw_bar(
                &mut scene,
                &frequency_axes,
                right,
                right + bar_width,
                msc_frequencies[*msc_index].max(0.0),
                Fill::new(Color::rgba(214, 39, 40, 224)),
            );
            annotate_mac(
                &mut scene,
                &frequency_axes,
                right + bar_width / 2.0,
                msc_frequencies[*msc_index],
                msc_pairing.mac[index],
                pal,
            );
        }
        if let Some(Some(n95_index)) = n95_matches.get(index) {
            let local_left = group_left + bar_width * (1.0 + usize::from(has_msc) as f64);
            draw_bar(
                &mut scene,
                &frequency_axes,
                local_left,
                local_left + bar_width,
                n95_frequencies[*n95_index].max(0.0),
                Fill::new(Color::rgba(255, 127, 14, 224)),
            );
            annotate_mac(
                &mut scene,
                &frequency_axes,
                local_left + bar_width / 2.0,
                n95_frequencies[*n95_index],
                n95_pairing.mac[index],
                pal,
            );
        }
    }
    add_panel_title(
        &mut scene,
        &frequency_axes,
        "Natural frequencies (bending)",
        pal,
    );
    let mut entries = vec![(
        "Rayleigh (analytical)".to_owned(),
        LegendMarker::Patch(Color::from_hex(BLUE)),
    )];
    if has_msc {
        entries.push((
            pairing_label(&format!("{msc_name} SOL 103"), &msc_pairing),
            LegendMarker::Patch(Color::from_hex(RED)),
        ));
    }
    if has_n95 {
        entries.push((
            pairing_label("NASTRAN-95 SOL 103", &n95_pairing),
            LegendMarker::Patch(Color::from_hex(ORANGE)),
        ));
    }
    draw_legend(
        &mut scene,
        [rects[0].0 + 8.0, rects[0].1 + 8.0],
        &entries,
        pal,
        8.0,
    );

    let y_range = span_range(&analysis.y);
    let rayleigh_axes = Axes2D::new(rects[1], y_range, (-1.1, 1.1));
    let nastran_axes = Axes2D::new(rects[2], y_range, (-1.1, 1.1));
    rayleigh_axes.draw_frame_with_labels(&mut scene, pal, "Spanwise position Y [m]", "");
    nastran_axes.draw_frame_with_labels(&mut scene, pal, "Spanwise position Y [m]", "");
    for (index, (shape, &frequency)) in analysis
        .modal
        .mode_shapes
        .iter()
        .zip(&analysis.modal.frequencies_hz)
        .enumerate()
    {
        let color = Color::from_hex(TAB10[index % TAB10.len()]);
        let rayleigh_points: Vec<(f64, f64)> = analysis
            .y
            .iter()
            .copied()
            .zip(shape.iter().copied())
            .filter(|(y, value)| y.is_finite() && value.is_finite())
            .collect();
        rayleigh_axes.add_line_series(
            &mut scene,
            &rayleigh_points,
            Stroke::dashed(color, 2.0, 6.0, 4.0),
        );
        if let (Some(modes), Some(Some(msc_index))) = (msc_modes, msc_matches.get(index)) {
            if let (Some(stations), Some(nastran_shape)) = (
                modes.mode_shape_y_m.as_ref(),
                msc_view.as_ref().and_then(|view| view.w.get(*msc_index)),
            ) {
                let aligned_shape = display_shape(stations, nastran_shape);
                let points: Vec<(f64, f64)> = stations
                    .iter()
                    .copied()
                    .zip(aligned_shape)
                    .filter(|(y, value)| y.is_finite() && value.is_finite())
                    .collect();
                nastran_axes.add_line_series(
                    &mut scene,
                    &points,
                    // Colour carries the Rayleigh mode match.  The solid
                    // stroke remains MSC-specific, so the two solvers are
                    // distinguishable without relying only on colour.
                    Stroke::new(color, 2.0),
                );
                draw_station_markers(&mut scene, &nastran_axes, &points, color);
            }
        }
        if let (Some(modes), Some(Some(n95_index))) = (n95_modes, n95_matches.get(index)) {
            if let (Some(stations), Some(n95_shape)) = (
                modes.mode_shape_y_m.as_ref(),
                n95_view.as_ref().and_then(|view| view.w.get(*n95_index)),
            ) {
                let aligned_shape = display_shape(stations, n95_shape);
                let points: Vec<(f64, f64)> = stations
                    .iter()
                    .copied()
                    .zip(aligned_shape)
                    .filter(|(y, value)| y.is_finite() && value.is_finite())
                    .collect();
                nastran_axes.add_line_series(
                    &mut scene,
                    &points,
                    // NASTRAN-95 uses the same matched-mode colour and a
                    // dotted stroke. This makes a missing local vector
                    // visible instead of silently looking like MSC only.
                    Stroke::dashed(color, 2.0, 2.0, 2.0),
                );
                draw_station_markers(&mut scene, &nastran_axes, &points, color);
            }
        }
        let rayleigh_label = format!("Mode {}: {frequency:.2} Hz", index + 1);
        add_legend_line(
            &mut scene,
            [rects[1].0 + 8.0, rects[1].1 + 8.0 + index as f64 * 14.0],
            &rayleigh_label,
            Stroke::dashed(color, 2.0, 6.0, 4.0),
            pal,
        );
    }
    // Torsion-dominated and unmatched modes are listed as text, not drawn.
    let mut local_lines: Vec<String> = Vec::new();
    for (solver_name, pairing, frequencies) in [
        (msc_name, &msc_pairing, msc_frequencies),
        ("NASTRAN-95", &n95_pairing, n95_frequencies),
    ] {
        if std::ptr::eq(frequencies, n95_frequencies) && !has_n95 {
            continue;
        }
        if std::ptr::eq(frequencies, msc_frequencies) && !has_msc {
            continue;
        }
        for local in &pairing.local {
            let frequency = frequencies
                .get(local.solver_index)
                .copied()
                .unwrap_or(f64::NAN);
            let mac = local
                .best_mac
                .map_or("n/a".to_owned(), |mac| format!("{mac:.2}"));
            local_lines.push(match local.torsion_ratio {
                Some(ratio) if ratio.is_finite() => format!(
                    "{solver_name} {frequency:.2} Hz: torsion-dominated, twist*c/2 / w = {ratio:.2} (MAC {mac})"
                ),
                Some(_) => format!(
                    "{solver_name} {frequency:.2} Hz: torsion-dominated, twist*c/2 / w = inf (MAC {mac})"
                ),
                None => format!(
                    "{solver_name} {frequency:.2} Hz: local / chordwise, no beam counterpart (MAC {mac})"
                ),
            });
        }
    }
    add_zero_line(&mut scene, &rayleigh_axes, pal);
    add_zero_line(&mut scene, &nastran_axes, pal);
    add_panel_title(
        &mut scene,
        &rayleigh_axes,
        "Rayleigh trial mode shapes",
        pal,
    );
    let nastran_title = match (msc_shape_available, n95_shape_available) {
        (true, true) => "MSC / NASTRAN-95 mode shapes",
        (true, false) => "NASTRAN mode shapes",
        (false, true) => "NASTRAN-95 mode shapes",
        (false, false) => "NASTRAN mode shapes (unavailable)",
    };
    add_panel_title(&mut scene, &nastran_axes, nastran_title, pal);
    if !has_msc && !has_n95 {
        let reason = if let Some(m) = msc_modes {
            match m.status {
                ResultStatus::Error => format!(
                    "NASTRAN SOL 103 failed: {}",
                    m.error.as_deref().unwrap_or("unknown error")
                ),
                ResultStatus::NotRun => "NASTRAN SOL 103 was not run for this design.".to_owned(),
                ResultStatus::NotConfigured => format!(
                    "NASTRAN SOL 103 is not configured: {}",
                    m.error.as_deref().unwrap_or("backend missing")
                ),
                ResultStatus::Ok => {
                    "NASTRAN SOL 103 returned no elastic mode frequencies.".to_owned()
                }
            }
        } else if let Some(m) = n95_modes {
            match m.status {
                ResultStatus::Error => format!(
                    "NASTRAN-95 SOL 103 failed: {}",
                    m.error.as_deref().unwrap_or("unknown error")
                ),
                ResultStatus::NotRun => {
                    "NASTRAN-95 SOL 103 was not run for this design.".to_owned()
                }
                ResultStatus::NotConfigured => format!(
                    "NASTRAN-95 SOL 103 is not configured: {}",
                    m.error.as_deref().unwrap_or("backend missing")
                ),
                ResultStatus::Ok => {
                    "NASTRAN-95 SOL 103 returned no elastic mode frequencies.".to_owned()
                }
            }
        } else {
            "NASTRAN SOL 103 was not run for this design.".to_owned()
        };
        scene.add(SceneElement::Text {
            text: wrap_text(&reason, 54),
            pos: [nastran_axes.left + 8.0, nastran_axes.top + 34.0],
            font_size: 8.0,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: false,
        });
    }
    let mut solver_entries = Vec::new();
    if msc_shape_available {
        solver_entries.push((
            format!("{msc_name} - solid line"),
            LegendMarker::Line(Stroke::new(Color::from_hex(pal.tick), 2.0)),
        ));
    }
    if n95_shape_available {
        solver_entries.push((
            "NASTRAN-95 - dotted line".to_owned(),
            LegendMarker::Line(Stroke::dashed(Color::from_hex(pal.tick), 2.0, 2.0, 2.0)),
        ));
    }
    if !solver_entries.is_empty() {
        solver_entries.push((
            "Colour = matched Rayleigh mode".to_owned(),
            LegendMarker::Line(Stroke::new(Color::from_hex(TAB10[0]), 2.0)),
        ));
    }
    if !solver_entries.is_empty() {
        draw_legend(
            &mut scene,
            [nastran_axes.left + 8.0, nastran_axes.top + 8.0],
            &solver_entries,
            pal,
            8.0,
        );
    }
    // Torsion-dominated and unmatched modes: a compact two-column list.
    let mut caption_top = 524.0;
    for (k, line) in local_lines.iter().enumerate() {
        let (column, row) = (k % 2, k / 2);
        scene.add(SceneElement::Text {
            text: line.clone(),
            pos: [
                rects[0].0 + 750.0 * column as f64,
                caption_top + 14.0 * row as f64,
            ],
            font_size: 8.0,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: false,
        });
    }
    if !local_lines.is_empty() {
        caption_top += 14.0 * local_lines.len().div_ceil(2) as f64 + 8.0;
    }
    let caption_lines: Vec<&str> = if decomposed {
        vec![CAPTION_NORMALISATION, CAPTION_MAC, CAPTION_TORSION]
    } else {
        vec![
            CAPTION_NORMALISATION_FRONT_ONLY,
            CAPTION_MAC,
            CAPTION_NO_TORSION,
        ]
    };
    for (line, text) in caption_lines.iter().copied().enumerate() {
        scene.add(SceneElement::Text {
            text: text.to_owned(),
            pos: [rects[0].0, caption_top + 17.0 * line as f64],
            font_size: 9.0,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: false,
        });
    }
    scene.height = caption_top + 17.0 * 3.0 + 6.0;
    scene
}

/// Small filled circles at the mesh stations a shape is sampled at.
fn draw_station_markers(scene: &mut Scene, axes: &Axes2D, points: &[(f64, f64)], color: Color) {
    for &(y, value) in points {
        scene.add(SceneElement::Circle {
            center: axes.map_point(y, value),
            radius: 2.2,
            fill: Some(Fill::new(color)),
            stroke: None,
        });
    }
}

fn wrap_text(text: &str, max_chars: usize) -> String {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_owned()
        } else {
            format!("{current} {word}")
        };
        if candidate.chars().count() <= max_chars || current.is_empty() {
            current = candidate;
        } else {
            lines.push(current);
            current = word.to_owned();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines.join("\n")
}

fn span_range(values: &[f64]) -> (f64, f64) {
    let lo = values
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .fold(f64::INFINITY, f64::min);
    let hi = values
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .fold(f64::NEG_INFINITY, f64::max);
    if lo.is_finite() && hi.is_finite() && hi > lo {
        (lo, hi)
    } else {
        (0.0, 1.0)
    }
}

const MODE_SIGN_EPS: f64 = 1.0e-9;

/// The shape as drawn: divided by its largest finite |z| along the spar and
/// sign-flipped so the value at the largest-y station (the tip) is positive.
/// Normal-mode eigenvectors have an arbitrary global sign and scale, so this
/// changes neither frequency nor shape. When the tip is effectively zero the
/// sign is taken from the largest-magnitude sample instead, so the result is
/// still deterministic. A shape with no finite non-zero sample is returned
/// unchanged (non-finite values stay non-finite and are not drawn).
fn display_shape(stations: &[f64], shape: &[f64]) -> Vec<f64> {
    let peak = shape
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .map(f64::abs)
        .fold(0.0, f64::max);
    if peak <= 0.0 {
        return shape.to_vec();
    }
    let tip = stations
        .iter()
        .copied()
        .zip(shape.iter().copied())
        .filter(|(y, value)| y.is_finite() && value.is_finite())
        .max_by(|(left, _), (right, _)| left.total_cmp(right))
        .map(|(_, value)| value / peak)
        .unwrap_or(0.0);
    let sign = if tip.abs() > MODE_SIGN_EPS {
        tip.signum()
    } else {
        shape
            .iter()
            .copied()
            .filter(|value| value.is_finite())
            .max_by(|a, b| a.abs().total_cmp(&b.abs()))
            .map_or(1.0, f64::signum)
    };
    shape.iter().map(|value| sign * value / peak).collect()
}

/// Interpolate a finite shape at a span station without extrapolating beyond
/// the reference grid. The NASTRAN and Rayleigh grids need not have identical
/// station counts, but they are both expressed in the same positive-right Y
/// frame.
fn interpolate_shape(y: &[f64], shape: &[f64], query: f64) -> Option<f64> {
    let mut samples: Vec<(f64, f64)> = y
        .iter()
        .copied()
        .zip(shape.iter().copied())
        .filter(|(station, value)| station.is_finite() && value.is_finite())
        .collect();
    if !query.is_finite() || samples.is_empty() {
        return None;
    }
    samples.sort_by(|(left, _), (right, _)| left.total_cmp(right));
    if query < samples[0].0 || query > samples[samples.len() - 1].0 {
        return None;
    }
    for &(station, value) in &samples {
        if station == query {
            return Some(value);
        }
    }
    for window in samples.windows(2) {
        let [(left_station, left_value), (right_station, right_value)] = window else {
            continue;
        };
        if *left_station <= query && query <= *right_station && right_station > left_station {
            let fraction = (query - left_station) / (right_station - left_station);
            return Some(left_value + fraction * (right_value - left_value));
        }
    }
    None
}

/// A solver is only said to have shapes when a one-to-one frequency pairing
/// yields a drawable curve. A frequency list alone must not produce a legend
/// that promises a curve which the parser could not supply.
fn has_matched_shape(
    modes: Option<&alas_struct::nastran::ModesResult>,
    matches: &[Option<usize>],
) -> bool {
    let Some(modes) = modes else {
        return false;
    };
    let Some(stations) = modes.mode_shape_y_m.as_ref() else {
        return false;
    };
    matches.iter().flatten().any(|&index| {
        modes.mode_shapes.get(index).is_some_and(|shape| {
            stations
                .iter()
                .zip(shape)
                .filter(|(y, value)| y.is_finite() && value.is_finite())
                .take(2)
                .count()
                == 2
        })
    })
}

/// Write the pair's MAC above its bar, in the failure red below [`MAC_PASS`].
fn annotate_mac(
    scene: &mut Scene,
    axes: &Axes2D,
    x: f64,
    frequency_hz: f64,
    mac: Option<f64>,
    pal: &crate::theme::Palette,
) {
    let Some(mac) = mac else {
        return;
    };
    let failing = mac < MAC_PASS;
    let [px, py] = axes.map_point(x, frequency_hz.max(0.0));
    scene.add(SceneElement::Text {
        text: format!("MAC {mac:.2}"),
        pos: [px, py - 3.0],
        font_size: 7.0,
        color: Color::from_hex(if failing { "#c0392b" } else { pal.tick }),
        align: TextAlign::Center,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: failing,
    });
}

fn add_panel_title(scene: &mut Scene, axes: &Axes2D, title: &str, pal: &crate::theme::Palette) {
    scene.add(SceneElement::Text {
        text: title.to_owned(),
        pos: [axes.left + axes.width / 2.0, axes.top - 10.0],
        font_size: 11.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Center,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });
}

fn add_zero_line(scene: &mut Scene, axes: &Axes2D, pal: &crate::theme::Palette) {
    scene.add(SceneElement::Line {
        p1: axes.map_point(axes.x_min, 0.0),
        p2: axes.map_point(axes.x_max, 0.0),
        stroke: Stroke::new(with_alpha(pal.spine, 102), 0.7),
    });
}

fn add_legend_line(
    scene: &mut Scene,
    pos: [f64; 2],
    label: &str,
    stroke: Stroke,
    pal: &crate::theme::Palette,
) {
    draw_legend(
        scene,
        pos,
        &[(label.to_owned(), LegendMarker::Line(stroke))],
        pal,
        8.0,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_struct::analytical::{ModalResult, StructuralAnalysisReport};
    use alas_struct::nastran::{ModesResult, NastranResults};

    #[test]
    fn nearest_frequency_matching_chooses_the_physically_closest_mode() {
        assert_eq!(nearest_frequency_index(9.7, &[3.0, 10.0, 22.0]), Some(1));
        assert_eq!(nearest_frequency_index(1.0, &[]), None);
    }

    #[test]
    fn mode_assignment_does_not_reuse_one_solver_mode() {
        let matches = match_frequency_indices(&[10.0, 10.2], &[10.1]);
        assert_eq!(matches, vec![Some(0), None]);

        let matches = match_frequency_indices(&[10.0, 10.2], &[10.11, 10.01]);
        assert_eq!(matches, vec![Some(1), Some(0)]);

        let matches = match_frequency_indices(&[10.0, 11.0], &[9.0, 10.1, 11.1]);
        assert_eq!(matches, vec![Some(1), Some(2)]);

        let matches = match_frequency_indices(&[10.0, 10.2, 11.0], &[10.1, 11.1]);
        assert_eq!(matches, vec![Some(0), None, Some(1)]);
    }

    #[test]
    fn an_unrequested_mode_solve_is_not_mislabeled_as_empty_solver_output() {
        let result = StructuralAnalysisResult {
            status: "ok".to_owned(),
            analysis: Some(StructuralAnalysisReport {
                y: vec![0.0, 1.0],
                ei_nm2: Vec::new(),
                load_cases: Vec::new(),
                modal: ModalResult {
                    frequencies_hz: vec![2.0],
                    mode_shapes: vec![vec![0.0, 1.0]],
                },
            }),
            nastran: Some(NastranResults::default()),
            ..StructuralAnalysisResult::default()
        };

        let scene = figure_structures_modes(Some(&result), Some("light"));
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } if text.contains("was not run"))
        }));
    }

    #[test]
    fn both_solver_mode_frequencies_and_shapes_are_rendered_together() {
        let result = StructuralAnalysisResult {
            status: "ok".to_owned(),
            analysis: Some(StructuralAnalysisReport {
                y: vec![0.0, 1.0, 2.0],
                ei_nm2: Vec::new(),
                load_cases: Vec::new(),
                modal: ModalResult {
                    frequencies_hz: vec![2.0, 4.0],
                    mode_shapes: vec![vec![0.0, 0.5, 1.0], vec![0.0, -0.5, -1.0]],
                },
            }),
            nastran: Some(NastranResults {
                modes: ModesResult {
                    status: ResultStatus::Ok,
                    frequencies_hz: vec![2.1, 4.2],
                    mode_shapes: vec![vec![0.0, 0.4, 1.0], vec![0.0, -0.4, -1.0]],
                    mode_shape_y_m: Some(vec![0.0, 1.0, 2.0]),
                    ..Default::default()
                },
                ..Default::default()
            }),
            nastran95: Some(NastranResults {
                modes: ModesResult {
                    status: ResultStatus::Ok,
                    frequencies_hz: vec![1.9, 4.1],
                    mode_shapes: vec![vec![0.0, 0.6, 1.0], vec![0.0, -0.6, -1.0]],
                    mode_shape_y_m: Some(vec![0.0, 1.0, 2.0]),
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        };

        let scene = figure_structures_modes(Some(&result), Some("light"));
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } if text.contains("MSC NASTRAN SOL 103"))
        }));
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } if text.contains("NASTRAN-95 SOL 103"))
        }));
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } if text == "MSC / NASTRAN-95 mode shapes")
        }));
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Polyline { stroke, .. }
                if stroke.color == Color::from_hex(TAB10[0]) && stroke.dash_array.is_none())
        }));
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Polyline { stroke, .. }
                if stroke.color == Color::from_hex(TAB10[0])
                    && stroke.dash_array == Some(vec![2.0, 2.0]))
        }));
    }

    #[test]
    fn mac_pairing_skips_a_torsional_mode_that_frequency_would_pick() {
        let y = vec![0.0, 1.0, 2.0, 3.0];
        let analysis = StructuralAnalysisReport {
            y: y.clone(),
            ei_nm2: Vec::new(),
            load_cases: Vec::new(),
            modal: ModalResult {
                frequencies_hz: vec![10.0],
                mode_shapes: vec![vec![0.0, 0.1, 0.4, 1.0]],
            },
        };
        // Mode 0 is closest in frequency but has a zigzag shape; mode 1 is
        // the bending mode.
        let modes = ModesResult {
            status: ResultStatus::Ok,
            frequencies_hz: vec![9.9, 14.0],
            mode_shapes: vec![vec![0.0, 1.0, -1.0, 0.5], vec![0.0, -0.1, -0.4, -1.0]],
            mode_shape_y_m: Some(y),
            ..Default::default()
        };
        let pairing = pair_modes(&analysis, Some(&modes), &modes.frequencies_hz);
        assert_eq!(pairing.matches, vec![Some(1)]);
        assert!(pairing.mac[0].unwrap() > 0.99);
        assert_eq!(
            pairing_label("FE", &pairing),
            "FE (paired by MAC, all 1 >= 0.80)"
        );
    }

    #[test]
    fn without_shapes_the_pairing_says_it_is_unverified() {
        let analysis = StructuralAnalysisReport {
            y: vec![0.0, 1.0],
            ei_nm2: Vec::new(),
            load_cases: Vec::new(),
            modal: ModalResult {
                frequencies_hz: vec![10.0],
                mode_shapes: vec![vec![0.0, 1.0]],
            },
        };
        let pairing = pair_modes(&analysis, None, &[10.5]);
        assert_eq!(pairing.matches, vec![Some(0)]);
        assert!(!pairing.by_mac());
        assert!(pairing_label("FE", &pairing).contains("unverified"));
    }

    #[test]
    fn display_shape_is_peak_normalised_with_a_positive_tip() {
        let stations = [0.0, 1.0, 2.0];
        assert_eq!(
            display_shape(&stations, &[0.0, -2.0, -4.0]),
            vec![0.0, 0.5, 1.0]
        );
        assert_eq!(
            display_shape(&stations, &[0.0, 0.3, 0.1]),
            vec![0.0, 1.0, 0.1 / 0.3]
        );
        // Zero tip: the largest-magnitude sample is made positive.
        assert_eq!(
            display_shape(&stations, &[0.0, -1.0, 0.0]),
            vec![0.0, 1.0, 0.0]
        );
    }

    fn local_mode_fixture() -> (StructuralAnalysisReport, ModesResult) {
        let y = vec![0.0, 1.0, 2.0, 3.0];
        let analysis = StructuralAnalysisReport {
            y: y.clone(),
            ei_nm2: Vec::new(),
            load_cases: Vec::new(),
            modal: ModalResult {
                frequencies_hz: vec![10.0],
                mode_shapes: vec![vec![0.0, 0.1, 0.4, 1.0]],
            },
        };
        // Mode 0: bending-free zigzag (low MAC). Mode 1: beam bending. Mode 2:
        // front and rear spars move in antiphase (twist), no mean deflection
        // beyond a small residue.
        let bend = vec![0.0, -0.1, -0.4, -1.0];
        let zig = vec![0.0, 1.0, -1.0, 0.0];
        let twist_front = vec![0.0, 0.5, 1.0, 1.5];
        let twist_rear: Vec<f64> = twist_front.iter().map(|v| -v).collect();
        let modes = ModesResult {
            status: ResultStatus::Ok,
            frequencies_hz: vec![9.9, 14.0, 20.0],
            mode_shapes: vec![zig.clone(), bend.clone(), twist_front.clone()],
            mode_shape_y_m: Some(y),
            spar_z_front: vec![zig.clone(), bend.clone(), twist_front],
            spar_z_rear: vec![zig, bend, twist_rear],
            spar_chord_m: Some(vec![1.0; 4]),
            ..Default::default()
        };
        (analysis, modes)
    }

    #[test]
    fn bending_is_the_spar_mean_and_torsion_dominance_is_the_half_difference_ratio() {
        let (_, modes) = local_mode_fixture();
        let view = mode_view(&modes);
        assert!(view.decomposed);
        assert!(view.torsion_ratio[1].unwrap() < 1.0e-12);
        assert!(
            view.torsion_ratio[2].unwrap() > 1.0e6 || view.torsion_ratio[2] == Some(f64::INFINITY)
        );
    }

    #[test]
    fn low_mac_and_torsion_dominated_modes_are_listed_and_not_paired() {
        let (analysis, modes) = local_mode_fixture();
        let pairing = pair_modes(&analysis, Some(&modes), &modes.frequencies_hz);
        assert_eq!(pairing.matches, vec![Some(1)]);
        let local: Vec<usize> = pairing.local.iter().map(|l| l.solver_index).collect();
        assert_eq!(local, vec![0, 2]);
        assert!(pairing.local[0].torsion_ratio.is_none());
        assert!(pairing.local[1].torsion_ratio.is_some());
    }

    #[test]
    fn local_modes_are_labelled_and_the_caption_states_the_torsion_definition() {
        let (analysis, modes) = local_mode_fixture();
        let result = StructuralAnalysisResult {
            status: "ok".to_owned(),
            analysis: Some(analysis),
            nastran95: Some(NastranResults {
                modes,
                solver_is_nastran95: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        let scene = figure_structures_modes(Some(&result), Some("light"));
        let has = |needle: &str| {
            scene.elements.iter().any(|element| {
                matches!(element, SceneElement::Text { text, .. } if text.contains(needle))
            })
        };
        assert!(has(
            "NASTRAN-95 9.90 Hz: local / chordwise, no beam counterpart (MAC 0."
        ));
        assert!(has(
            "NASTRAN-95 20.00 Hz: torsion-dominated, twist*c/2 / w = "
        ));
        assert!(has("twist*c/2 / w > 1.0"));
        assert!(has("midpoint between the spars"));
    }

    #[test]
    fn front_spar_only_results_say_there_is_no_torsion_reference() {
        let (analysis, mut modes) = local_mode_fixture();
        modes.spar_z_front.clear();
        modes.spar_z_rear.clear();
        let result = StructuralAnalysisResult {
            status: "ok".to_owned(),
            analysis: Some(analysis),
            nastran95: Some(NastranResults {
                modes,
                ..Default::default()
            }),
            ..Default::default()
        };
        let scene = figure_structures_modes(Some(&result), Some("light"));
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } if text.contains("No torsion reference"))
        }));
    }
}
