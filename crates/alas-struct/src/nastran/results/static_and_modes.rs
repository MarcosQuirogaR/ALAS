// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reading the static (SOL 101) and normal-modes (SOL 103) results.

use super::*;

/// Modes at or below this frequency are rigid-body modes, and are not reported.
///
/// A free-free-ish model returns a handful of near-zero modes that are numerical
/// artefacts of the constraint set rather than structural modes. The threshold
/// is upstream's, and upstream took it from the reference scripts' own
/// `_get_structural_freqs_nastran`.
const RIGID_BODY_CUTOFF_HZ: f64 = 0.5;

/// Read a SOL 101 result: tip deflection and peak corner stress per load case.
///
/// Subcase identifiers are the load cases' positions, one-based, which is how
/// the deck writer assigned them. A case whose subcase the solve did not write
/// is skipped rather than reported as zero.
pub fn read_static(op2: &Op2, node_index: &MeshNodeIndex, cases: &[LoadCase]) -> StaticResult {
    let mut result = StaticResult {
        status: ResultStatus::Ok,
        ..StaticResult::default()
    };
    for (index, case) in cases.iter().enumerate() {
        let sid = index as i64 + 1;
        let Some(table) = op2.displacements.get(&sid) else {
            continue;
        };
        if let Some(row) = position_of(&table.node_ids, node_index.tip_nid) {
            if let Some(values) = table.data.get(row) {
                result.tip_deflection_m.push(case.name, values[2]);
            }
        }
        // An empty stress table has no maximum, which upstream reaches as a
        // raised ValueError that it catches and ignores.
        if let Some(stress) = op2.cquad4_stress.get(&sid).filter(|s| !s.data.is_empty()) {
            // Column 7 is von Mises in the CQUAD4 corner output layout, and it
            // is not the last column in general.
            let peak = stress
                .data
                .iter()
                .map(|row| row[7].abs())
                .fold(f64::NEG_INFINITY, f64::max);
            result.root_von_mises_max_pa.push(case.name, peak);
        }
    }
    result
}

/// Where each grid sits along the span.
///
/// [`read_modes`] sorts the front-spar line by span station, which upstream
/// reads off the BDF model it is handed. A trait rather than the concrete
/// [`Deck`] because that is what the reader actually wants (one coordinate
/// per grid, not a deck) and because it lets the parity test supply the
/// reference's own grid table instead of rebuilding a mesh around it.
pub trait SpanStations {
    /// The span station of grid `nid`, or `NaN` if there is no such grid.
    fn span_station(&self, nid: i64) -> f64;
}

impl SpanStations for Deck {
    fn span_station(&self, nid: i64) -> f64 {
        self.node_y(nid)
    }
}

/// Read a SOL 103 result: elastic frequencies and their front-spar shapes.
///
/// `stations` supplies the span station of each grid, which the shapes are
/// sorted and sampled by. Modes at or below [`RIGID_BODY_CUTOFF_HZ`] are
/// dropped.
pub fn read_modes(
    op2: &Op2,
    stations: &impl SpanStations,
    node_index: &MeshNodeIndex,
) -> ModesResult {
    let mut result = ModesResult {
        status: ResultStatus::Ok,
        ..ModesResult::default()
    };
    let Some(eigenvectors) = op2.eigenvectors.get(&1) else {
        return result;
    };
    let kept: Vec<usize> = eigenvectors
        .mode_cycles
        .iter()
        .enumerate()
        .filter(|&(_, &cycles)| cycles > RIGID_BODY_CUTOFF_HZ)
        .map(|(index, _)| index)
        .collect();
    result.frequencies_hz = kept
        .iter()
        .map(|&index| eigenvectors.mode_cycles[index])
        .collect();
    if kept.is_empty() {
        return result;
    }

    // The front spar alone: it is the line the deck applies its aerodynamic
    // FORCE cards along, and the line the analytical estimate this is compared
    // against is written for.
    let Some(front_spar) = node_index.spar_upper_nids.first() else {
        result.status = ResultStatus::Error;
        result.error = Some("the mesh index has no spar node lines".to_owned());
        return result;
    };
    let front_spar: BTreeSet<i64> = front_spar.iter().copied().collect();
    let on_front_spar: Vec<usize> = eigenvectors
        .node_ids
        .iter()
        .enumerate()
        .filter(|&(_, nid)| front_spar.contains(nid))
        .map(|(index, _)| index)
        .collect();
    if on_front_spar.is_empty() {
        return result;
    }

    let mut ordered: Vec<(f64, usize)> = on_front_spar
        .iter()
        .map(|&row| (stations.span_station(eigenvectors.node_ids[row]), row))
        .collect();
    ordered.sort_by(|(left, _), (right, _)| left.total_cmp(right));
    result.mode_shape_y_m = Some(ordered.iter().map(|&(y, _)| y).collect());

    for &mode in &kept {
        // Keep one shape slot for every retained frequency, even when an OP2
        // omits that mode's vector table.  `frequencies_hz` and
        // `mode_shapes` are parallel by contract; silently skipping a vector
        // would shift every later shape onto the wrong frequency and could
        // make the report compare unrelated modes.
        let out_of_plane: Vec<f64> = eigenvectors
            .data
            .get(mode)
            .map(|shape| {
                ordered
                    .iter()
                    .map(|&(_, row)| shape.get(row).map_or(f64::NAN, |values| values[2]))
                    .collect()
            })
            .unwrap_or_else(|| vec![f64::NAN; ordered.len()]);
        // Upstream's `float(np.max(np.abs(t3))) or 1.0`: a mode that is
        // identically zero on this line normalizes by one rather than by zero.
        let peak = out_of_plane
            .iter()
            .map(|value| value.abs())
            .fold(f64::NEG_INFINITY, f64::max);
        let peak = if peak == 0.0 || !peak.is_finite() {
            1.0
        } else {
            peak
        };
        result
            .mode_shapes
            .push(out_of_plane.iter().map(|value| value / peak).collect());
    }
    result
}
