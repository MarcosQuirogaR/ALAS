// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from native aerodynamic model/aerodynamics/aero_3D/avl.py
// (parse_unformatted_data_output, which mses.py reuses) and
// alas/physics/mses_analysis.py (the BL-dump and flowfield readers).
// Upstream: native aerodynamic model 4.2.8, MIT.
// Reference: alas @ rust-port-baseline.

//! Reading MSES/mplot output back into numbers.
//!
//! Three text shapes are parsed. `mplot`'s polar summary is a ragged block of
//! `key = value` pairs, read by [`parse_unformatted_data_output`], the same
//! scanner native aerodynamic model uses for every Drela-code output. The boundary-layer
//! dump and the flowfield dump are whitespace-column tables, read by
//! [`parse_bl_dump`] and [`parse_flowfield`]. And whether a solve converged is
//! decided by a marker string in the `mses` run's stdout, [`is_converged`].
//!
//! Every value comes from a fixed-precision text field, so parsing it here and
//! with Python's `float()` yields the same `f64`, which is what lets the parity
//! test compare at `exact`.

use std::collections::HashMap;

/// True when an `mses` solve reported convergence.
///
/// native aerodynamic model decides this from exactly this substring in the run's stdout;
/// a run without it is treated as not converged and, in a polar sweep, triggers
/// a mesh reinitialization at the next angle.
pub fn is_converged(mses_stdout: &str) -> bool {
    mses_stdout.contains("Converged on tolerance")
}

/// The polar summary's key renamings, applied before [`parse_unformatted_data_output`].
///
/// `mplot` labels three quantities in words that the `key = value` scanner
/// cannot key on (`top Xtr`, `bot Xtr`, `at x,y`); native aerodynamic model rewrites them to
/// single tokens first, and this reproduces that rewrite in the same order.
pub fn apply_polar_replacements(raw: &str) -> String {
    raw.replace("top Xtr", "xtr_top")
        .replace("bot Xtr", "xtr_bot")
        .replace("at x,y", "x_ac")
}

/// Parse a block of ragged `key = value` data into a map, as
/// `AVL.parse_unformatted_data_output` does with its default `" = "` delimiter.
///
/// For each occurrence of `" = "`, the key is the token immediately to its left
/// (skipping intervening spaces, stopping at a space or newline) and the value
/// the token immediately to its right, cast to `f64` with a NaN on failure --
/// exactly upstream's back-scan/forward-scan. Two faithful-translation notes,
/// neither reachable from `mplot`'s summary: upstream keeps the *first* value
/// for a repeated key only if asked (its default raises), and this keeps the
/// first without raising, since the summary has no repeated key; and upstream's
/// scan reads `s[-1]` (a Python wrap) if a delimiter has no token before it,
/// where this stops at the string start instead -- the two agree on every line
/// `mplot` actually emits, none of which is that malformed.
pub fn parse_unformatted_data_output(input: &str) -> HashMap<String, f64> {
    const ID: &str = " = ";
    let mut items: HashMap<String, f64> = HashMap::new();
    let mut remaining = input;

    while let Some(index) = remaining.find(ID) {
        let bytes = remaining.as_bytes();

        // Key: back-scan from just left of the delimiter.
        let mut i = index as isize - 1;
        while i >= 0 && bytes[i as usize] == b' ' {
            i -= 1;
        }
        let key_end = (i + 1) as usize;
        while i >= 0 && bytes[i as usize] != b' ' && bytes[i as usize] != b'\n' {
            i -= 1;
        }
        let key_start = (i + 1) as usize;
        let key = remaining[key_start..key_end].to_owned();

        // Value: forward-scan from just right of the delimiter.
        let mut j = index + ID.len();
        while j < bytes.len() && bytes[j] == b' ' {
            j += 1;
        }
        let value_start = j;
        while j < bytes.len() && bytes[j] != b' ' && bytes[j] != b'\n' {
            j += 1;
        }
        let value = remaining[value_start..j].parse::<f64>().unwrap_or(f64::NAN);

        items.entry(key).or_insert(value);
        remaining = &remaining[index + ID.len()..];
    }

    items
}

/// The five parallel columns a boundary-layer dump yields: x, y, surface arc
/// length, Cp and local Mach, one entry per surface panel in MPlot's native
/// order.
///
/// MPlot restarts the arc-length coordinate at the leading edge for its second
/// surface. That reset, rather than the sign of the ordinate, is the stable
/// surface-topology boundary: a cambered section can cross the chord line near
/// its trailing edge without ceasing to be the upper or lower surface.
pub type BlDumpColumns = (Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>);

/// Parse an `mplot` boundary-layer dump into [`BlDumpColumns`].
///
/// Blank lines and `#` headers are skipped; a data line needs at least eight
/// whitespace-separated columns, of which columns 1, 2, 3, 5 and 8 (`x`,
/// `y`, `s`, `Cp`, `Me`) are kept. A line whose first eight columns do not all
/// parse as floats is skipped, matching upstream's `except ValueError:
/// continue`.
pub fn parse_bl_dump(text: &str) -> BlDumpColumns {
    let (mut xs, mut ys, mut arc_lengths, mut cps, mut mes) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 8 {
            continue;
        }
        let mut values = [0.0_f64; 8];
        let mut ok = true;
        for (slot, part) in values.iter_mut().zip(&parts[..8]) {
            match part.parse::<f64>() {
                Ok(value) => *slot = value,
                Err(_) => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }
        xs.push(values[0]);
        ys.push(values[1]);
        arc_lengths.push(values[2]);
        cps.push(values[4]);
        mes.push(values[7]);
    }
    (xs, ys, arc_lengths, cps, mes)
}

/// Parse an `mplot` flowfield dump into `(x, y, local Mach)`.
///
/// Blank lines and `#` headers are skipped; a data line needs at least eight
/// columns, of which columns 1, 2 and 8 (`x`, `y`, `M`) are kept. Upstream
/// appends the three fields inside one `try`, so a partial parse could
/// misalign the arrays; this keeps a row only when all three parse, which
/// cannot differ on a flowfield dump (every data row is numeric in those three
/// columns) -- the same translate-the-harmless-latent-bug call `CLAUDE.md`
/// records for the reference's `mesh_line`.
/// Parse a flowfield while retaining the row boundaries written by `mplot`.
///
/// Option 11 separates structured grid rows with blank lines. The scalar
/// parser intentionally exposes only values for parity with Python; figures
/// additionally need these offsets to reconstruct filled native-grid cells.
pub fn parse_flowfield(text: &str) -> (Vec<f64>, Vec<f64>, Vec<f64>, Vec<usize>) {
    let (mut xs, mut ys, mut ms) = (Vec::new(), Vec::new(), Vec::new());
    let mut row_offsets = Vec::new();
    let mut row_open = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            row_open = false;
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 8 {
            continue;
        }
        match (
            parts[0].parse::<f64>(),
            parts[1].parse::<f64>(),
            parts[7].parse::<f64>(),
        ) {
            (Ok(x), Ok(y), Ok(m)) => {
                if !row_open {
                    row_offsets.push(xs.len());
                    row_open = true;
                }
                xs.push(x);
                ys.push(y);
                ms.push(m);
            }
            _ => continue,
        }
    }
    (xs, ys, ms, row_offsets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_value_scanner_reads_a_drela_summary_line() {
        let raw = " CL  =   0.48599     CD  =  0.005189    CM  =  -0.05327 L/D =    93.661";
        let parsed = parse_unformatted_data_output(raw);
        assert_eq!(parsed["CL"], 0.48599);
        assert_eq!(parsed["CD"], 0.005189);
        assert_eq!(parsed["CM"], -0.05327);
        assert_eq!(parsed["L/D"], 93.661);
    }

    #[test]
    fn replacements_make_the_worded_labels_keyable() {
        let raw = " top Xtr =  0.3669  bot Xtr =   0.5998";
        let parsed = parse_unformatted_data_output(&apply_polar_replacements(raw));
        assert_eq!(parsed["xtr_top"], 0.3669);
        assert_eq!(parsed["xtr_bot"], 0.5998);
    }

    #[test]
    fn scanner_returns_nan_for_an_unparseable_value() {
        let parsed = parse_unformatted_data_output(" Run case = -unnamed-");
        assert!(parsed["case"].is_nan());
    }

    #[test]
    fn convergence_is_the_marker_substring() {
        assert!(is_converged("  ... Converged on tolerance   1.00\n"));
        assert!(!is_converged("  ... did not converge\n"));
    }

    #[test]
    fn bl_dump_keeps_x_y_cp_and_mach_and_skips_headers() {
        let text = "#  x   y   s   b0   Cp   Ue   rho   Me\n\
                    #  1   2   3   4    5    6    7     8\n\
                    -0.59192E-04  0.22845E-02  0.60E-02  1.0  0.73807  0.60  1.02  0.15598  extra\n";
        let (xs, ys, arc_lengths, cps, mes) = parse_bl_dump(text);
        assert_eq!(xs, vec![-0.59192e-4]);
        assert_eq!(ys, vec![0.22845e-2]);
        assert_eq!(arc_lengths, vec![0.60e-2]);
        assert_eq!(cps, vec![0.73807]);
        assert_eq!(mes, vec![0.15598]);
    }

    #[test]
    fn flowfield_keeps_x_y_and_mach() {
        let text = "#  x  y  rho  p  u  v  q  M  Cp\n\n\
                    -1.8506  -2.3545  1.0008  1.0011  0.99  0.041  0.99  0.29735  0.017\n";
        let (xs, ys, ms, rows) = parse_flowfield(text);
        assert_eq!(xs, vec![-1.8506]);
        assert_eq!(ys, vec![-2.3545]);
        assert_eq!(ms, vec![0.29735]);
        assert_eq!(rows, vec![0]);
    }

    #[test]
    fn flowfield_row_offsets_preserve_blank_line_grid_boundaries() {
        let text = "# x y rho p u v q M Cp\n\
                    0.0 0.0 1 1 1 0 1 0.8 0\n\
                    1.0 0.0 1 1 1 0 1 0.9 0\n\n\
                    0.0 1.0 1 1 1 0 1 1.0 0\n\
                    1.0 1.0 1 1 1 0 1 1.1 0\n";
        let (x, _, _, rows) = parse_flowfield(text);
        assert_eq!(x.len(), 4);
        assert_eq!(rows, vec![0, 2]);
    }
}
