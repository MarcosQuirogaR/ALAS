// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Wake-convergence gate for a retained native VSPAERO iteration history.

use std::fs;
use std::path::Path;

/// Number of trailing history rows (wake iterations) the plateau gate spans.
const VSPAERO_WAKE_WINDOW: usize = 4;

/// Peak-to-peak tolerances over the trailing window, as
/// `(absolute, relative-to-scale)` pairs for `[CLtot, CDi, CMytot]`. The
/// tolerance of each coefficient is `absolute + relative * scale`, where the
/// scale is the largest `|CLtot|` (CL), `|CDi|` (CDi) or `max(|CLtot|, |CMytot|)`
/// (Cm) in the window.
///
/// A successful process and a parseable `.polar` do not establish that the
/// free wake reached a fixed point, so the retained history is checked. The
/// earlier gate compared only the last two rows against 1e-4, which is below
/// the iteration-to-iteration scatter that VSPAERO's free-wake relaxation
/// leaves on its converged plateau. Evidence (native reruns of saved cases,
/// 8 wake nodes, Mach 0.78): a 20-iteration A220 run settles by wake
/// iteration 6 and then scatters by ~5e-4 in CL, ~3e-5 in CDi and ~2e-3 in
/// Cm with no trend, and tightening the GMRES factor 10x leaves that scatter
/// unchanged, so it is wake roll-up noise rather than linear-solver error.
/// A 32-node wake changes the converged CL by 0.3 percent. Across 4 presets
/// the plateau scatter over the last four rows is at most 0.5 percent of CL
/// for CL, 3 percent of CDi for CDi and 1.2 percent of max(|CL|,|Cm|) for Cm;
/// the gate allows roughly 1.5x the observed CL and CDi levels, but for Cm
/// the limit (3e-3 + 1.5e-2 max(|CL|,|Cm|), about 1.05e-2 at CL 0.5) is
/// about 5x the observed 2e-3 scatter, because the fitted 1.2 percent relative
/// term is applied on top of the absolute floor. The tolerances were fitted on
/// the same four presets they are applied to (calibration, not validation),
/// and the test is a peak-to-peak range over four rows: a slow monotone drift
/// smaller than the limit passes. The limit is therefore a numerical
/// verification screen, not an error bound on the admitted coefficients. The
/// CL limit is ~0.05 deg of alpha at
/// CL_alpha ~ 0.1 per deg. The 47 saved 5-iteration histories fail it (their
/// window ranges reach 0.02 in CL and 0.1 in Cm: still drifting), so the gate
/// discriminates rather than rubber-stamps.
const VSPAERO_WAKE_PLATEAU_TOLERANCE: [(f64, f64); 3] =
    [(1.0e-3, 5.0e-3), (2.0e-4, 3.0e-2), (3.0e-3, 1.5e-2)];

/// A final induced drag more negative than this (absolute CDi) is non-physical
/// for a lifting-surface solve and is reported as a failed wake solution.
const VSPAERO_NEGATIVE_CDI_LIMIT: f64 = -5.0e-4;

pub(crate) fn assess_vspaero_wake_history(path: &Path) -> Result<(), String> {
    let text = fs::read_to_string(path).map_err(|error| {
        format!(
            "VSPAERO wake convergence history is unavailable at {}: {error}",
            path.display()
        )
    })?;
    assess_vspaero_wake_history_text(&text)
}

fn assess_vspaero_wake_history_text(text: &str) -> Result<(), String> {
    let mut cases: Vec<Vec<[f64; 3]>> = Vec::new();
    let mut current: Option<Vec<[f64; 3]>> = None;
    let mut columns: Option<[usize; 3]> = None;

    let finish_case = |current: &mut Option<Vec<[f64; 3]>>, cases: &mut Vec<Vec<[f64; 3]>>| {
        if let Some(rows) = current.take() {
            if !rows.is_empty() {
                cases.push(rows);
            }
        }
    };

    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("Solver Case:") {
            finish_case(&mut current, &mut cases);
            current = Some(Vec::new());
            columns = None;
            continue;
        }
        if trimmed.starts_with("Iter ") || trimmed == "Iter" {
            let headers = trimmed.split_whitespace().collect::<Vec<_>>();
            let required = ["CLtot", "CDi", "CMytot"];
            let Some(indices) = required
                .map(|name| headers.iter().position(|header| *header == name))
                .into_iter()
                .collect::<Option<Vec<_>>>()
                .and_then(|indices| indices.try_into().ok())
            else {
                return Err("VSPAERO wake history is missing CLtot/CDi/CMytot columns".to_owned());
            };
            columns = Some(indices);
            if current.is_none() {
                current = Some(Vec::new());
            }
            continue;
        }
        let Some(indices) = columns else {
            continue;
        };
        let fields = trimmed.split_whitespace().collect::<Vec<_>>();
        if fields
            .first()
            .and_then(|value| value.parse::<usize>().ok())
            .is_none()
        {
            continue;
        }
        let mut row = [0.0; 3];
        for (slot, &index) in indices.iter().enumerate() {
            let Some(token) = fields.get(index) else {
                return Err("VSPAERO wake history contains a truncated iteration row".to_owned());
            };
            row[slot] = token.parse::<f64>().map_err(|_| {
                format!("VSPAERO wake history contains a malformed coefficient '{token}'")
            })?;
            if !row[slot].is_finite() {
                return Err("VSPAERO wake history contains a non-finite coefficient".to_owned());
            }
        }
        current.get_or_insert_with(Vec::new).push(row);
    }
    finish_case(&mut current, &mut cases);
    if cases.is_empty() {
        return Err("VSPAERO wake history contains no iteration cases".to_owned());
    }
    for (case_index, rows) in cases.iter().enumerate() {
        if rows.len() < VSPAERO_WAKE_WINDOW {
            return Err(format!(
                "VSPAERO wake history case {} has {} iterations; at least {} are needed to test a plateau",
                case_index + 1,
                rows.len(),
                VSPAERO_WAKE_WINDOW
            ));
        }
        let window = &rows[rows.len() - VSPAERO_WAKE_WINDOW..];
        let largest = |slot: usize| window.iter().fold(0.0_f64, |m, row| m.max(row[slot].abs()));
        let scales = [largest(0), largest(1), largest(0).max(largest(2))];
        for (slot, name) in ["CLtot", "CDi", "CMytot"].into_iter().enumerate() {
            let (low, high) = window
                .iter()
                .fold((f64::MAX, f64::MIN), |(low, high), row| {
                    (low.min(row[slot]), high.max(row[slot]))
                });
            let range = high - low;
            let (absolute, relative) = VSPAERO_WAKE_PLATEAU_TOLERANCE[slot];
            let limit = absolute + relative * scales[slot];
            if range > limit {
                return Err(format!(
                    "VSPAERO wake case {} is not converged: {name} varies by {range:.6e} over the last {} iterations (limit {limit:.6e})",
                    case_index + 1,
                    VSPAERO_WAKE_WINDOW
                ));
            }
        }
        let final_cdi = window[window.len() - 1][1];
        if final_cdi < VSPAERO_NEGATIVE_CDI_LIMIT {
            return Err(format!(
                "VSPAERO wake case {} has non-physical negative induced drag CDi={final_cdi:.6e}",
                case_index + 1
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str = "  Iter Mach AoA Beta CLo CLi CLtot CDo CDi CDtot CMytot
";

    /// One history case from `(CLtot, CDi, CMytot)` rows, numbered from 0.
    fn case(number: usize, rows: &[(f64, f64, f64)]) -> String {
        let mut text = format!(
            "Solver Case: {number}
{HEADER}"
        );
        for (index, (cl, cdi, cm)) in rows.iter().enumerate() {
            text.push_str(&format!(
                "  {index} 0.8 3 0 0 0 {cl} 0 {cdi} 0 {cm}
"
            ));
        }
        text
    }

    #[test]
    fn wake_history_rejects_a_drifting_tail() {
        // Shape of the former 5-iteration production histories: CL and Cm
        // still moving by 1e-2 and 5e-2 across the last four rows.
        let text = case(
            1,
            &[
                (0.0, 0.0, 0.0),
                (0.8368, 0.0305, 0.1906),
                (0.8366, 0.0311, 0.1938),
                (0.8469, 0.0316, 0.1428),
                (0.8503, 0.0317, 0.1252),
            ],
        );
        let error = match assess_vspaero_wake_history_text(&text) {
            Err(error) => error,
            Ok(_) => panic!("the tail is still drifting"),
        };
        assert!(error.contains("case 1 is not converged"), "{error}");
    }

    #[test]
    fn wake_history_accepts_a_noisy_plateau_with_a_settling_transient() {
        // Scatter at the observed noise floor (CL 5e-4, CDi 3e-5, Cm 2e-3)
        // after a transient: not below the old 1e-4 gate, but converged.
        let plateau = [
            (0.0, 0.0, 0.0),
            (0.4743, 0.01394, 0.4353),
            (0.4737, 0.01443, 0.4411),
            (0.4790, 0.01445, 0.4163),
            (0.4808, 0.01445, 0.4075),
            (0.4811, 0.01446, 0.4060),
            (0.4812, 0.01447, 0.4065),
            (0.4810, 0.01445, 0.4066),
            (0.4814, 0.01446, 0.4052),
        ];
        let text = format!("{}{}", case(1, &plateau), case(2, &plateau));
        assert!(assess_vspaero_wake_history_text(&text).is_ok());
    }

    #[test]
    fn wake_history_rejects_negative_induced_drag_and_short_histories() {
        let negative = case(
            1,
            &[
                (0.0, 0.0, 0.0),
                (0.17, -0.482, -0.6),
                (0.17, -0.482, -0.6),
                (0.17, -0.482, -0.6),
                (0.17, -0.482, -0.6),
            ],
        );
        let error = match assess_vspaero_wake_history_text(&negative) {
            Err(error) => error,
            Ok(_) => panic!("negative induced drag is non-physical"),
        };
        assert!(error.contains("negative induced drag"), "{error}");
        let short = case(1, &[(0.0, 0.0, 0.0), (0.5, 0.02, 0.1)]);
        let error = match assess_vspaero_wake_history_text(&short) {
            Err(error) => error,
            Ok(_) => panic!("two rows cannot show a plateau"),
        };
        assert!(error.contains("at least"), "{error}");
    }

    /// Four-row case whose last four rows ramp monotonically by `range` in the
    /// chosen slot (0 CL, 1 CDi, 2 Cm) around CL 0.5, CDi 0.015, Cm 0.3.
    fn drift(slot: usize, range: f64) -> String {
        let base = [0.5, 0.015, 0.3];
        let rows: Vec<(f64, f64, f64)> = (0..4)
            .map(|i| {
                let mut v = base;
                v[slot] += range * i as f64 / 3.0;
                (v[0], v[1], v[2])
            })
            .collect();
        case(1, &rows)
    }

    #[test]
    fn monotonic_drift_just_above_each_plateau_limit_fails_and_just_below_passes() {
        for (slot, name, limit) in [
            // limit = absolute + relative * scale, scale = largest coefficient
            // in the window (CL ramps to 0.5 + range, so use the ramp end).
            (0, "CLtot", 1.0e-3 + 5.0e-3 * 0.504),
            (1, "CDi", 2.0e-4 + 3.0e-2 * 0.0165),
            (2, "CMytot", 3.0e-3 + 1.5e-2 * 0.5),
        ] {
            let Err(error) = assess_vspaero_wake_history_text(&drift(slot, limit * 1.10)) else {
                panic!("{name}: a drift above the limit must fail");
            };
            assert!(error.contains(name), "{name}: {error}");
            assert!(
                assess_vspaero_wake_history_text(&drift(slot, limit * 0.90)).is_ok(),
                "{name}: a drift below the limit passes (range-only screen)"
            );
        }
    }

    #[test]
    fn wake_history_requires_a_fresh_iteration_table() {
        let error = match assess_vspaero_wake_history_text("Solver Case: 1\n") {
            Err(error) => error,
            Ok(_) => panic!("a headerless history is not convergence evidence"),
        };
        assert!(error.contains("no iteration cases"), "{error}");
    }
}
