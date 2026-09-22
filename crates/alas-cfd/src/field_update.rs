// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Direct evidence that a solved field is still being updated.
//!
//! An outer residual is not the quantity of interest.  What the convergence
//! gate needs to know about a frozen equation is whether the solver is still
//! *updating its field*, and OpenFOAM writes exactly that: the field itself, at
//! every `writeInterval`.  Comparing the last two written times answers the
//! question directly, with no threshold and no inference from residual
//! behaviour.
//!
//! Why this replaced a residual-depth heuristic.  Measured on this host, over
//! the last written interval of each case:
//!
//! | case | `k` cells changed | `omega` cells changed | `p` cells changed |
//! |---|---:|---:|---:|
//! | `V1-inletoutlet-coarse` | 99.88 % | 99.90 % | live |
//! | `P1-fine-preltol001` | 99.94 % | 99.95 % | 100 % |
//! | `G3-fine-p404` | **0 of 436 389** | **0 of 436 389** | 99.99 % |
//! | `L2-medium-le2` | **0 of 183 721** | **0 of 183 721** | 100 % |
//! | `T3-gradfree-wallsolve` | **0 of 82 993** | **0 of 82 993** | 100 % |
//!
//! The separation is total and it is not a threshold: either the solver wrote a
//! different field or it wrote the same one.  Residual depth below the inner
//! solver tolerance does **not** separate those populations (`G3`'s omega sits
//! at `1.00x` the floor and `T3`'s at `9.8e-6x`, yet both fields are equally
//! frozen), which is why the depth factor that briefly stood here was withdrawn
//! rather than retuned.

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::result_io::{numeric_time_dirs, parse_scalar_list};

/// Solved fields this evidence covers for the incompressible `simpleFoam` path.
///
/// `U` is a `volVectorField` and must be here: on a converged fine case the
/// momentum residuals sit below the inner tolerance too (`G3-fine-p404` ends at
/// `Ux 8.47e-9`, `Uy 8.31e-9`), so leaving it out would make the guard report
/// *inconclusive* for ordinary healthy runs.
pub const FIELD_UPDATE_FIELDS: [&str; 4] = ["p", "U", "k", "omega"];

/// Solved fields this evidence covers for the compressible `rhoSimpleFoam`
/// path.  The extra energy field is required only when density and energy are
/// part of the selected equation set.
pub const COMPRESSIBLE_FIELD_UPDATE_FIELDS: [&str; 5] = ["p", "U", "e", "k", "omega"];

/// Whether one solved field changed between the last two written times.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldUpdateSample {
    /// OpenFOAM field name, e.g. `k`.
    pub field: String,
    /// Earlier written time directory.
    pub from_time: String,
    /// Later written time directory.
    pub to_time: String,
    /// Internal cells compared.
    pub cells: usize,
    /// Internal cells whose written value differs between the two writes.
    pub changed_cells: usize,
    /// Largest relative difference over those cells, `0.0` when none differ.
    pub max_relative_change: f64,
    /// `writePrecision` the two files were written at, when known.
    #[serde(default)]
    pub write_precision: Option<u32>,
}

impl FieldUpdateSample {
    /// Whether the written field differs between the two times.
    ///
    /// Exact at the stored precision, not thresholded: one differing decimal
    /// value is a difference.  Read this as an observation about the **written
    /// representation**, which is what [`FieldUpdateSample::observation`]
    /// states; it is neither proof that the equation is materially alive nor,
    /// when false, proof that it stopped being solved.
    pub fn updated(&self) -> bool {
        self.changed_cells > 0
    }

    /// What was observed, with its validity domain, in one sentence.
    ///
    /// Deliberately not a causal claim.  Equal ASCII text at
    /// `writePrecision` decimal digits means no change was *persisted* at that
    /// precision; a sub-quantum update, or a change confined to a boundary
    /// patch, would look the same.
    pub fn observation(&self) -> String {
        let precision = self.write_precision.map_or_else(
            || "the written".to_owned(),
            |digits| format!("{digits}-digit"),
        );
        if self.updated() {
            format!(
                "`{}` differs in {} of {} internal cells between written times {} and {} at {precision} ASCII precision (largest relative difference {:.3e})",
                self.field, self.changed_cells, self.cells, self.from_time, self.to_time,
                self.max_relative_change,
            )
        } else {
            format!(
                "`{}` is unchanged in all {} internal cells between written times {} and {} at {precision} ASCII precision",
                self.field, self.cells, self.from_time, self.to_time,
            )
        }
    }
}

/// Field-update evidence for one case, or the reason there is none.
///
/// [`Default`] is **not** an empty success.  A record deserialized from an
/// archive written before this evidence existed must identify itself as *not
/// recorded*, or an absent observation reads as an unremarkable one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldUpdateEvidence {
    /// One entry per field that could be compared.
    #[serde(default)]
    pub samples: Vec<FieldUpdateSample>,
    /// Why evidence is absent or partial, when it is.
    #[serde(default = "not_recorded_reason")]
    pub unavailable_reason: Option<String>,
    /// `writePrecision` from the case's own `controlDict`, when readable.
    ///
    /// The validity domain of every comparison below: two writes that agree to
    /// this many decimal digits are indistinguishable here.
    #[serde(default)]
    pub write_precision: Option<u32>,
    /// `writeInterval` from the case's own `controlDict`, when readable.
    #[serde(default)]
    pub write_interval: Option<f64>,
    /// Whether the two compared times are separated by exactly
    /// [`Self::write_interval`].
    ///
    /// `Some(false)` means the pair is not a regular consecutive write (a
    /// final short interval, a restart, or a missing directory), so the
    /// comparison spans an unknown amount of solver progress.
    #[serde(default)]
    pub write_pair_regular: Option<bool>,
}

/// Reason attached to a `FieldUpdateEvidence` that was never written.
fn not_recorded_reason() -> Option<String> {
    Some(
        "not recorded: this result predates written-field evidence, so nothing was observed either way"
            .to_owned(),
    )
}

impl Default for FieldUpdateEvidence {
    fn default() -> Self {
        Self {
            samples: Vec::new(),
            unavailable_reason: not_recorded_reason(),
            write_precision: None,
            write_interval: None,
            write_pair_regular: None,
        }
    }
}

impl FieldUpdateEvidence {
    /// Evidence for the equation named in a residual line, if any.
    ///
    /// `Ux` and `Uy` both resolve to `U`, which is not covered, so they return
    /// `None` and the caller must treat them as inconclusive.
    pub fn sample(&self, equation: &str) -> Option<&FieldUpdateSample> {
        let field = match equation {
            other if other.eq_ignore_ascii_case("Ux") || other.eq_ignore_ascii_case("Uy") => "U",
            other => other,
        };
        self.samples
            .iter()
            .find(|sample| sample.field.eq_ignore_ascii_case(field))
    }

    /// Whether any *other* solved field was still being updated.
    ///
    /// This is what keeps a genuinely stationary solution acceptable: when
    /// nothing at all is moving, the case has reached a fixed point and a
    /// frozen field is the correct answer, not a dead equation.  A dead
    /// equation is recognised only by the contrast.
    pub fn another_field_updated(&self, equation: &str) -> bool {
        let own = self.sample(equation).map(|sample| sample.field.as_str());
        self.samples
            .iter()
            .filter(|sample| Some(sample.field.as_str()) != own)
            .any(FieldUpdateSample::updated)
    }
}

/// Compare the last two written times of `case_dir` for the solved fields.
///
/// Returns evidence with `unavailable_reason` set, rather than an empty
/// success, when fewer than two times were written or a field could not be
/// parsed: a caller must be able to tell "not updated" from "not observed".
pub fn read_field_update_evidence(case_dir: &Path) -> FieldUpdateEvidence {
    read_field_update_evidence_with_fields(case_dir, &FIELD_UPDATE_FIELDS)
}

/// Compare the fields required by the selected equation set.
pub fn read_field_update_evidence_for_config(
    case_dir: &Path,
    compressible: bool,
) -> FieldUpdateEvidence {
    if compressible {
        read_field_update_evidence_with_fields(case_dir, &COMPRESSIBLE_FIELD_UPDATE_FIELDS)
    } else {
        read_field_update_evidence(case_dir)
    }
}

fn read_field_update_evidence_with_fields(
    case_dir: &Path,
    required_fields: &[&str],
) -> FieldUpdateEvidence {
    let (write_precision, write_interval) = write_controls(case_dir);
    let times = numeric_time_dirs(case_dir);
    if times.len() < 2 {
        return FieldUpdateEvidence {
            samples: Vec::new(),
            unavailable_reason: Some(format!(
                "field-update evidence needs two written times; {} found in {}",
                times.len(),
                case_dir.display()
            )),
            write_precision,
            write_interval,
            write_pair_regular: None,
        };
    }
    let (earlier_time, earlier) = &times[times.len() - 2];
    let (later_time, later) = &times[times.len() - 1];
    // Whether the pair is a regular consecutive write.  A final short interval
    // or a restart makes the comparison span an unknown amount of solver
    // progress, which is worth recording even though it does not by itself
    // invalidate the observation.
    let write_pair_regular = write_interval
        .filter(|interval| *interval > 0.0)
        .map(|interval| ((later_time - earlier_time) - interval).abs() <= 1.0e-6 * interval);
    let label = |path: &Path| {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let mut samples = Vec::new();
    let mut missing = Vec::new();
    for field in required_fields.iter().copied() {
        match compare_field(earlier, later, field) {
            Some((cells, changed_cells, max_relative_change)) => samples.push(FieldUpdateSample {
                field: field.to_owned(),
                from_time: label(earlier),
                to_time: label(later),
                cells,
                changed_cells,
                max_relative_change,
                write_precision,
            }),
            None => missing.push(field),
        }
    }
    let mut reasons = Vec::new();
    if !missing.is_empty() {
        // Missing, malformed, mismatched-length or non-finite: all of them mean
        // the field was NOT OBSERVED, and none of them means it was unchanged.
        reasons.push(format!(
            "no usable internal field for {} between {} and {} (absent, malformed, of a different length, or containing non-finite values)",
            missing.join(", "),
            label(earlier),
            label(later)
        ));
    }
    if write_pair_regular == Some(false) {
        reasons.push(format!(
            "written times {} and {} are not one writeInterval apart, so the pair spans an unknown amount of solver progress",
            label(earlier),
            label(later)
        ));
    }
    if write_precision.is_none() {
        reasons.push(
            "writePrecision could not be read from the case, so the precision the comparison is valid at is unknown".to_owned(),
        );
    }
    FieldUpdateEvidence {
        unavailable_reason: (!reasons.is_empty()).then(|| reasons.join("; ")),
        samples,
        write_precision,
        write_interval,
        write_pair_regular,
    }
}

/// `(values, components per cell)` of a nonuniform `internalField`.
///
/// Scalars use the shared list grammar; vectors carry three parenthesised
/// components per cell, which the scalar reader cannot express.  The declared
/// count is honoured in both cases, so a truncated file is rejected instead of
/// becoming a short-but-plausible comparison.
fn parse_internal_field(text: &str) -> Option<(Vec<f64>, usize)> {
    if !text.contains("List<vector>") {
        return parse_scalar_list(text, "internalField").map(|values| (values, 1));
    }
    let start = text.find("internalField")?;
    let tail = &text[start..];
    let after = &tail[tail.find("nonuniform")? + "nonuniform".len()..];
    let open = after.find('(')?;
    let declared = after[..open]
        .split_whitespace()
        .find_map(|token| token.parse::<usize>().ok())?;
    let mut depth = 0_i32;
    let mut end = None;
    for (index, character) in after[open..].char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(open + index);
                    break;
                }
            }
            _ => {}
        }
    }
    let values = after[open + 1..end?]
        .split(|character: char| character.is_whitespace() || character == '(' || character == ')')
        .filter(|token| !token.is_empty())
        .filter_map(|token| token.parse::<f64>().ok())
        .collect::<Vec<_>>();
    (values.len() == declared * 3).then_some((values, 3))
}

/// `(cells, changed_cells, max_relative_change)` for one field, if comparable.
///
/// A vector field is compared as three components per cell and reported as one
/// changed cell if any component moved, so the count stays a cell count.
fn compare_field(earlier: &Path, later: &Path, field: &str) -> Option<(usize, usize, f64)> {
    let read = |directory: &Path| {
        let text = std::fs::read_to_string(directory.join(field)).ok()?;
        parse_internal_field(&text)
    };
    let (before, components) = read(earlier)?;
    let (after, after_components) = read(later)?;
    if before.len() != after.len() || before.is_empty() || components != after_components {
        return None;
    }
    let cells = before.len() / components;
    if cells == 0 || cells * components != before.len() {
        return None;
    }
    let mut changed = 0_usize;
    let mut worst = 0.0_f64;
    for cell in 0..cells {
        let mut moved = false;
        for component in 0..components {
            let left = before[cell * components + component];
            let right = after[cell * components + component];
            // A non-finite value is not evidence of anything.  Skipping it
            // would silently count a NaN/Inf pair, or a finite-versus-Inf
            // pair, as "unchanged", which is exactly the reading that must
            // never be manufactured.  The whole field becomes unavailable.
            if !left.is_finite() || !right.is_finite() {
                return None;
            }
            if left == right {
                continue;
            }
            moved = true;
            let scale = left.abs().max(right.abs());
            if scale > 0.0 {
                worst = worst.max((left - right).abs() / scale);
            }
        }
        if moved {
            changed += 1;
        }
    }
    Some((cells, changed, worst))
}

/// `writePrecision` and `writeInterval` from the case's own `controlDict`.
///
/// These are the validity domain of every comparison in this module and are
/// read from the case rather than assumed: a comparison at 12 decimal digits
/// and one at 6 are not the same observation.
fn write_controls(case_dir: &Path) -> (Option<u32>, Option<f64>) {
    let Ok(text) = std::fs::read_to_string(case_dir.join("system/controlDict")) else {
        return (None, None);
    };
    let entry = |key: &str| -> Option<&str> {
        let start = text.find(key)? + key.len();
        text[start..].split(';').next().map(str::trim)
    };
    (
        entry("writePrecision").and_then(|value| value.parse().ok()),
        entry("writeInterval").and_then(|value| value.parse().ok()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence(samples: &[(&str, usize)]) -> FieldUpdateEvidence {
        FieldUpdateEvidence {
            samples: samples
                .iter()
                .map(|(field, changed)| FieldUpdateSample {
                    field: (*field).to_owned(),
                    from_time: "800".to_owned(),
                    to_time: "821".to_owned(),
                    cells: 82_993,
                    changed_cells: *changed,
                    max_relative_change: if *changed > 0 { 1.0e-3 } else { 0.0 },
                    write_precision: Some(12),
                })
                .collect(),
            unavailable_reason: None,
            write_precision: Some(12),
            write_interval: Some(100.0),
            write_pair_regular: Some(true),
        }
    }

    /// `T3-gradfree-wallsolve`, verbatim shape: turbulence frozen, pressure
    /// live.  This is the case the contrast exists for.
    #[test]
    fn a_frozen_field_beside_a_moving_one_is_distinguishable() {
        let measured = evidence(&[("p", 82_993), ("k", 0), ("omega", 0)]);
        for dead in ["k", "omega"] {
            let sample = measured.sample(dead).unwrap_or_else(|| panic!("{dead}"));
            assert!(!sample.updated(), "{dead}");
            assert!(measured.another_field_updated(dead), "{dead}");
        }
        let live = measured.sample("p").unwrap_or_else(|| panic!("p"));
        assert!(live.updated());
    }

    /// An exactly stationary solution reproduces every field, and must stay
    /// acceptable: being steady is the answer, not a defect.
    #[test]
    fn a_wholly_stationary_solution_has_no_moving_contrast() {
        let stationary = evidence(&[("p", 0), ("k", 0), ("omega", 0)]);
        for field in FIELD_UPDATE_FIELDS {
            assert!(
                !stationary.another_field_updated(field),
                "{field}: nothing is moving, so nothing is dead"
            );
        }
    }

    /// A non-finite or malformed list is NOT OBSERVED, never "unchanged".
    ///
    /// Skipping non-finite values would let a NaN/Inf pair, or a finite versus
    /// non-finite pair, count as an unchanged field and so as a dead equation.
    #[test]
    fn a_non_finite_or_malformed_list_yields_no_evidence() {
        let scalar = |values: &str, count: usize| {
            format!(
                "internalField   nonuniform List<scalar>
{count}
(
{values}
)
;
"
            )
        };
        let good = scalar("1.0 2.0 3.0", 3);
        for (label, other) in [
            ("nan", scalar("1.0 nan 3.0", 3)),
            ("inf", scalar("1.0 inf 3.0", 3)),
            ("short list", scalar("1.0 2.0", 3)),
            ("unparseable token", scalar("1.0 x 3.0", 3)),
        ] {
            let directory = std::env::temp_dir()
                .join(format!("alas-cfd-fieldcmp-{label}-{}", std::process::id()));
            let later = directory.join("later");
            let earlier = directory.join("earlier");
            let _ = std::fs::create_dir_all(&later);
            let _ = std::fs::create_dir_all(&earlier);
            let _ = std::fs::write(earlier.join("k"), &good);
            let _ = std::fs::write(later.join("k"), &other);
            assert_eq!(
                compare_field(&earlier, &later, "k"),
                None,
                "{label} must be unavailable, not unchanged"
            );
            let _ = std::fs::remove_dir_all(&directory);
        }
    }

    /// A record that predates this evidence must say so rather than look like
    /// an empty successful observation.
    #[test]
    fn default_evidence_identifies_itself_as_not_recorded() {
        let legacy = FieldUpdateEvidence::default();
        assert!(legacy.samples.is_empty());
        let reason = legacy
            .unavailable_reason
            .unwrap_or_else(|| panic!("a default must carry a reason"));
        assert!(reason.contains("not recorded"), "{reason}");
        // And the same when it arrives through serde from an archived record
        // that has none of these fields.
        let archived: FieldUpdateEvidence =
            serde_json::from_str("{}").unwrap_or_else(|error| panic!("{error}"));
        assert!(archived
            .unavailable_reason
            .is_some_and(|reason| reason.contains("not recorded")));
    }

    /// The momentum equations map to a field this module does not parse, so
    /// they must read as *no evidence* rather than as evidence of a freeze.
    /// Both momentum residual names resolve to the single `U` field, so a live
    /// velocity field answers for `Ux` and `Uy` alike.
    #[test]
    fn the_momentum_equations_resolve_to_the_velocity_field() {
        let measured = evidence(&[("p", 82_993), ("U", 82_990), ("k", 0), ("omega", 0)]);
        for equation in ["Ux", "Uy"] {
            let sample = measured
                .sample(equation)
                .unwrap_or_else(|| panic!("{equation}"));
            assert_eq!(sample.field, "U");
            assert!(sample.updated(), "{equation}");
        }
        assert!(measured.sample("nut").is_none());
    }
}
