// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

pub(crate) fn read_force_history(case_dir: &Path) -> Vec<ForceSample> {
    let root = case_dir.join("postProcessing/forceCoeffs");
    let mut samples = Vec::new();
    for (_, directory) in numeric_time_dirs(&root) {
        for filename in ["coefficient.dat", "forceCoeffs.dat"] {
            let Ok(text) = fs::read_to_string(directory.join(filename)) else {
                continue;
            };
            let parsed = parse_force_coefficients(&text);
            if parsed.is_empty() {
                continue;
            }
            for sample in parsed {
                replace_force_at_time(&mut samples, sample);
            }
            break;
        }
    }
    samples.sort_by(|left, right| left.time.total_cmp(&right.time));
    samples
}

pub(crate) fn read_force_decomposition(case_dir: &Path) -> Vec<ForceDecompositionSample> {
    let root = case_dir.join("postProcessing/forces");
    let mut samples = Vec::new();
    for (_, directory) in numeric_time_dirs(&root) {
        for filename in ["forces.dat", "force.dat"] {
            let Ok(text) = fs::read_to_string(directory.join(filename)) else {
                continue;
            };
            let parsed = parse_force_decomposition(&text);
            if parsed.is_empty() {
                continue;
            }
            for sample in parsed {
                replace_decomposition_at_time(&mut samples, sample);
            }
            break;
        }
    }
    samples.sort_by(|left, right| left.time.total_cmp(&right.time));
    samples
}

/// Parse the solver-attached yPlus summary for the latest available time.
///
/// OpenCFD writes one row per patch in `postProcessing/yPlus/<time>/yPlus.dat`.
/// The summary is preferred over a generic `postProcess -func yPlus` result:
/// the latter can exit successfully while warning that no turbulence model is
/// available and writing an all-zero field.  The native field list is used
/// only to recover a face-value count and to make the source auditable.
/// OpenCFD may place a row whose solved `Time` is 10 in
/// `postProcessing/yPlus/0/` when the function object writes at `writeTime`,
/// so the output directory name is deliberately not compared with the row.
pub(crate) fn read_y_plus_summary(case_dir: &Path, patch_name: &str) -> Option<ParsedYPlusSummary> {
    if patch_name.trim().is_empty() {
        return None;
    }
    let root = case_dir.join("postProcessing/yPlus");
    for (_output_directory_time, directory) in numeric_time_dirs(&root).into_iter().rev() {
        let path = directory.join("yPlus.dat");
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        // A summary may contain multiple patches in future OpenFOAM versions;
        // select the requested wall patch explicitly.
        let Some(row) = text
            .lines()
            .filter(|line| !line.trim_start().starts_with('#'))
            .filter_map(parse_y_plus_row_with_patch)
            .find(|row| row.patch_name == patch_name)
        else {
            continue;
        };
        let (row_time, min_y_plus, max_y_plus, average_y_plus) =
            (row.time, row.min_y_plus, row.max_y_plus, row.average_y_plus);
        if row_time.is_finite()
            && min_y_plus.is_finite()
            && max_y_plus.is_finite()
            && average_y_plus.is_finite()
            && min_y_plus >= 0.0
            && max_y_plus >= min_y_plus
            && average_y_plus >= min_y_plus
            && average_y_plus <= max_y_plus
            // For the supported no-slip, positive-speed SST case an entirely
            // zero yPlus summary indicates that the function object had no
            // turbulence model/field to evaluate.  Treat it as unavailable
            // rather than presenting a successful zero wall-resolution
            // diagnostic.
            && max_y_plus > f64::EPSILON
        {
            let native_time = format_time_for_path(case_dir, row_time);
            let sample_count = native_time
                .as_deref()
                .and_then(|name| read_native_y_plus_count(case_dir, name, patch_name));
            return Some(ParsedYPlusSummary {
                time: row_time,
                patch_name: patch_name.to_owned(),
                sample_count,
                min_y_plus,
                max_y_plus,
                average_y_plus,
                source: path
                    .strip_prefix(case_dir)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/"),
            });
        }
        // Keep trying older outputs when a newest row is malformed.  The
        // caller receives None if no finite, ordered summary can be trusted.
    }
    None
}

/// Read cell-quality fields emitted by `checkMesh -writeAllFields`.
///
/// The fields are native `volScalarField` data.  Only finite internal-cell
/// values are summarized, so a missing field or malformed list remains
/// unavailable rather than being inferred from a max/min line in the log.  If
/// a case contains several written times, the latest native field containing
/// each metric is selected.
pub(crate) fn read_mesh_quality_distributions(case_dir: &Path) -> Vec<ScalarDistribution> {
    const FIELDS: [(&str, &str, &str); 4] = [
        ("nonOrthoAngle", "non-orthogonality", "deg"),
        ("skewness", "skewness", "-"),
        ("aspectRatio", "aspect ratio", "-"),
        ("cellVolume", "cell volume", "m^3"),
    ];
    let time_directories = numeric_time_dirs(case_dir);
    FIELDS
        .into_iter()
        .filter_map(|(field, label, unit)| {
            time_directories.iter().rev().find_map(|(_, directory)| {
                let path = directory.join(field);
                let text = fs::read_to_string(&path).ok()?;
                let values = parse_scalar_list(&text, "internalField")?;
                scalar_distribution(field, label, unit, relative_path(case_dir, &path), values)
            })
        })
        .collect()
}

/// Largest boundary-face skewness written by `checkMesh -writeAllFields`, with
/// the patch it occurs on.
///
/// `checkMesh`'s headline `Max skewness` is the **internal**-face maximum; the
/// boundary faces live in the `boundaryField` of the written `skewness` field
/// and are the only place the boundary maximum can be read exactly.  Without
/// this, `max_boundary_skewness` can be demonstrated only as a count of faces
/// in error, never as the value the declared limit is written against.
///
/// Skewness is dimensionless.  An `empty` patch carries no faces and is skipped
/// rather than counted as a zero maximum.  Returns `None` when the field was
/// not written or contains no usable boundary list, so an absent measurement
/// stays absent.
pub fn read_boundary_skewness_max(case_dir: &Path) -> Option<(f64, String)> {
    let path = numeric_time_dirs(case_dir)
        .into_iter()
        .map(|(_, directory)| directory.join("skewness"))
        .find(|candidate| candidate.is_file())?;
    let text = fs::read_to_string(path).ok()?;
    let body = &text[text.find("boundaryField")?..];
    let mut best: Option<(f64, String)> = None;
    let mut cursor = 0_usize;
    while let Some(offset) = body[cursor..].find("nonuniform") {
        let at = cursor + offset;
        // The patch name is the last identifier before this entry's dictionary.
        let name = body[..at]
            .rsplit('{')
            .nth(1)
            .and_then(|segment| segment.split_whitespace().last())
            .unwrap_or("unknown")
            .to_owned();
        cursor = at + "nonuniform".len();
        let Some(values) = parse_scalar_list(&body[at..], "nonuniform") else {
            continue;
        };
        let Some(max) = values
            .into_iter()
            .filter(|value| value.is_finite())
            .fold(None, |acc: Option<f64>, value| {
                Some(acc.map_or(value, |current: f64| current.max(value)))
            })
        else {
            continue;
        };
        if best.as_ref().is_none_or(|(current, _)| max > *current) {
            best = Some((max, name));
        }
    }
    best
}

/// Read the solved wall-face y+ field for a named patch.
///
/// The y+ field is kept separate from cell-quality distributions because it is
/// a solution diagnostic and its values live on the wall faces.  The latest
/// native time containing a finite patch list is selected.
pub(crate) fn read_y_plus_distribution(
    case_dir: &Path,
    patch_name: &str,
) -> Option<ScalarDistribution> {
    if patch_name.trim().is_empty() {
        return None;
    }
    numeric_time_dirs(case_dir)
        .into_iter()
        .rev()
        .find_map(|(_, directory)| {
            let path = directory.join("yPlus");
            let text = fs::read_to_string(&path).ok()?;
            let values = parse_scalar_list(&text, patch_name)?;
            scalar_distribution(
                "yPlus",
                "wall y+",
                "-",
                relative_path(case_dir, &path),
                values,
            )
        })
}

const DISTRIBUTION_PERCENTILES: [f64; 21] = [
    0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 45.0, 50.0, 55.0, 60.0, 65.0, 70.0, 75.0,
    80.0, 85.0, 90.0, 95.0, 100.0,
];

fn scalar_distribution(
    field: &str,
    label: &str,
    unit: &str,
    source: String,
    values: Vec<f64>,
) -> Option<ScalarDistribution> {
    let mut finite = values
        .into_iter()
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    if finite.is_empty() {
        return None;
    }
    finite.sort_by(f64::total_cmp);
    let mean = finite.iter().sum::<f64>() / finite.len() as f64;
    let values = DISTRIBUTION_PERCENTILES
        .into_iter()
        .map(|percentile| quantile(&finite, percentile / 100.0))
        .collect();
    Some(ScalarDistribution {
        field: field.to_owned(),
        label: label.to_owned(),
        unit: unit.to_owned(),
        source,
        sample_count: finite.len(),
        min: finite[0],
        mean,
        max: finite[finite.len() - 1],
        percentiles: DISTRIBUTION_PERCENTILES.to_vec(),
        values,
    })
}

fn quantile(sorted: &[f64], fraction: f64) -> f64 {
    if sorted.len() == 1 {
        return sorted[0];
    }
    let position = fraction.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        sorted[lower]
    } else {
        let ratio = position - lower as f64;
        sorted[lower] + ratio * (sorted[upper] - sorted[lower])
    }
}

/// Parse a nonuniform scalar list below a named OpenFOAM dictionary entry.
///
/// Both `internalField` and a patch `value` entry use the same list grammar.
/// The declared count is checked when present, preventing a truncated file
/// from becoming a plausible-looking distribution.
pub(crate) fn parse_scalar_list(text: &str, marker: &str) -> Option<Vec<f64>> {
    let start = text.find(marker)?;
    let tail = &text[start..];
    let nonuniform_offset = tail.find("nonuniform")?;
    let after = &tail[nonuniform_offset + "nonuniform".len()..];
    let open = after.find('(')?;
    let expected = after[..open]
        .split_whitespace()
        .find_map(|token| token.parse::<usize>().ok());
    let close = after[open + 1..].find(')')? + open + 1;
    let values = after[open + 1..close]
        .split_whitespace()
        .filter_map(|token| token.parse::<f64>().ok())
        .collect::<Vec<_>>();
    if expected.is_some_and(|count| count != values.len()) {
        return None;
    }
    (!values.is_empty()).then_some(values)
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Parsed yPlus data before configured study values are attached.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParsedYPlusSummary {
    pub time: f64,
    pub patch_name: String,
    pub sample_count: Option<usize>,
    pub min_y_plus: f64,
    pub max_y_plus: f64,
    pub average_y_plus: f64,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq)]
struct ParsedYPlusRow {
    time: f64,
    patch_name: String,
    min_y_plus: f64,
    max_y_plus: f64,
    average_y_plus: f64,
}

fn parse_y_plus_row_with_patch(line: &str) -> Option<ParsedYPlusRow> {
    let values = line.split_whitespace().collect::<Vec<_>>();
    if values.len() < 5 {
        return None;
    }
    let time = values[0].parse::<f64>().ok()?;
    let min_y_plus = values[2].parse::<f64>().ok()?;
    let max_y_plus = values[3].parse::<f64>().ok()?;
    let average_y_plus = values[4].parse::<f64>().ok()?;
    Some(ParsedYPlusRow {
        time,
        patch_name: values[1].to_owned(),
        min_y_plus,
        max_y_plus,
        average_y_plus,
    })
}

fn format_time_for_path(case_dir: &Path, time: f64) -> Option<String> {
    let exact = format!("{time}");
    if case_dir.join(&exact).join("yPlus").is_file() {
        return Some(exact);
    }
    numeric_time_dirs(case_dir)
        .into_iter()
        .find(|(candidate, path)| {
            (candidate - time).abs() <= 1.0e-8 * candidate.abs().max(1.0)
                && path.join("yPlus").is_file()
        })
        .and_then(|(_, path)| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
}

fn read_native_y_plus_count(case_dir: &Path, time: &str, patch_name: &str) -> Option<usize> {
    let text = fs::read_to_string(case_dir.join(time).join("yPlus")).ok()?;
    let mut in_patch = false;
    let mut waiting_for_count = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if !in_patch {
            if trimmed == patch_name {
                in_patch = true;
            }
            continue;
        }
        if trimmed == "}" {
            break;
        }
        if trimmed.starts_with("value") && trimmed.contains("nonuniform") {
            waiting_for_count = true;
            continue;
        }
        if waiting_for_count && !trimmed.is_empty() {
            return trimmed.parse::<usize>().ok();
        }
    }
    None
}

pub(crate) fn numeric_time_dirs(root: &Path) -> Vec<(f64, PathBuf)> {
    let mut directories = fs::read_dir(root)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            let time = path.file_name()?.to_str()?.parse::<f64>().ok()?;
            time.is_finite().then_some((time, path))
        })
        .collect::<Vec<_>>();
    directories.sort_by(|left, right| left.0.total_cmp(&right.0));
    directories
}

fn replace_force_at_time(samples: &mut Vec<ForceSample>, sample: ForceSample) {
    let tolerance = 1.0e-10 * sample.time.abs().max(1.0);
    if let Some(index) = samples
        .iter()
        .position(|old| (old.time - sample.time).abs() <= tolerance)
    {
        samples[index] = sample;
    } else {
        samples.push(sample);
    }
}

fn replace_decomposition_at_time(
    samples: &mut Vec<ForceDecompositionSample>,
    sample: ForceDecompositionSample,
) {
    let tolerance = 1.0e-10 * sample.time.abs().max(1.0);
    if let Some(index) = samples
        .iter()
        .position(|old| (old.time - sample.time).abs() <= tolerance)
    {
        samples[index] = sample;
    } else {
        samples.push(sample);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn case_dir() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |value| value.as_nanos());
        std::env::temp_dir().join(format!("alas-cfd-result-io-{stamp}"))
    }

    #[test]
    fn force_outputs_are_aggregated_and_duplicate_times_are_replaced() {
        let case = case_dir();
        let first = case.join("postProcessing/forceCoeffs/1");
        let second = case.join("postProcessing/forceCoeffs/2");
        let duplicate = case.join("postProcessing/forceCoeffs/3");
        fs::create_dir_all(&duplicate).expect("test directories");
        fs::create_dir_all(&first).expect("test directories");
        fs::create_dir_all(&second).expect("test directories");
        let header = "# Time Cd Cl Cm\n";
        fs::write(
            first.join("coefficient.dat"),
            format!("{header}1 0.1 0.2 0.3\n"),
        )
        .expect("first force output");
        fs::write(
            second.join("coefficient.dat"),
            format!("{header}2 0.2 0.3 0.4\n"),
        )
        .expect("second force output");
        fs::write(
            duplicate.join("coefficient.dat"),
            format!("{header}2 0.9 0.8 0.7\n"),
        )
        .expect("duplicate force output");
        let samples = read_force_history(&case);
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].time, 1.0);
        assert_eq!(samples[1].time, 2.0);
        assert_eq!(samples[1].cd, 0.9);
        let _ = fs::remove_dir_all(case);
    }

    #[test]
    fn decomposition_outputs_are_aggregated_by_time() {
        let case = case_dir();
        let first = case.join("postProcessing/forces/1");
        let second = case.join("postProcessing/forces/2");
        fs::create_dir_all(&first).expect("test directories");
        fs::create_dir_all(&second).expect("test directories");
        let header = "# Time total_x total_y total_z pressure_x pressure_y pressure_z viscous_x viscous_y viscous_z\n";
        let row = |time: u32| format!("{time} 1 2 3 4 5 6 7 8 9\n");
        fs::write(first.join("force.dat"), format!("{header}{}", row(1)))
            .expect("first force output");
        fs::write(second.join("force.dat"), format!("{header}{}", row(2)))
            .expect("second force output");
        let samples = read_force_decomposition(&case);
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].time, 1.0);
        assert_eq!(samples[1].time, 2.0);
        assert_eq!(samples[1].pressure_force_n, [4.0, 5.0, 6.0]);
        let _ = fs::remove_dir_all(case);
    }

    #[test]
    fn y_plus_summary_requires_requested_patch_and_recovers_native_count() {
        let case = case_dir();
        // OpenFOAM groups a write-time row under the function object's start
        // directory when writeControl is writeTime.  The row's Time value is
        // the solved field time and need not equal that directory name.
        let summary = case.join("postProcessing/yPlus/0");
        fs::create_dir_all(&summary).expect("summary directory");
        fs::create_dir_all(case.join("7")).expect("native time directory");
        fs::write(
            summary.join("yPlus.dat"),
            "# Time patch min max average\n7 airfoil 0.25 4.5 1.75\n",
        )
        .expect("summary output");
        fs::write(
            case.join("7/yPlus"),
            "boundaryField\n{\n    airfoil\n    {\n        value nonuniform List<scalar>\n        3\n        (\n            1\n            2\n            3\n        )\n    }\n}\n",
        )
        .expect("native yPlus output");
        let parsed = read_y_plus_summary(&case, "airfoil").expect("finite summary");
        assert_eq!(parsed.time, 7.0);
        assert_eq!(parsed.sample_count, Some(3));
        assert_eq!(parsed.min_y_plus, 0.25);
        assert!(read_y_plus_summary(&case, "missing").is_none());
        let _ = fs::remove_dir_all(case);
    }

    #[test]
    fn y_plus_summary_rejects_all_zero_diagnostic() {
        let case = case_dir();
        let summary = case.join("postProcessing/yPlus/0");
        fs::create_dir_all(&summary).expect("summary directory");
        fs::write(
            summary.join("yPlus.dat"),
            "# Time patch min max average\n7 airfoil 0 0 0\n",
        )
        .expect("summary output");
        assert!(read_y_plus_summary(&case, "airfoil").is_none());
        let _ = fs::remove_dir_all(case);
    }

    #[test]
    fn native_quality_fields_produce_actual_percentile_distributions() {
        let case = case_dir();
        let time = case.join("0");
        fs::create_dir_all(&time).expect("quality field directory");
        let field = |name: &str, values: &str| {
            fs::write(
                time.join(name),
                format!(
                    "FoamFile {{ class volScalarField; }}\ninternalField nonuniform List<scalar>\n4\n(\n{values}\n)\nboundaryField {{}}\n"
                ),
            )
            .expect("quality field");
        };
        field("nonOrthoAngle", "1\n2\n3\n4");
        field("skewness", "0.1\n0.2\n0.3\n0.4");
        field("aspectRatio", "2\n4\n6\n8");
        field("cellVolume", "1e-6\n2e-6\n3e-6\n4e-6");
        let later = case.join("12");
        fs::create_dir_all(&later).expect("later quality field directory");
        fs::write(
            later.join("aspectRatio"),
            "FoamFile { class volScalarField; }\ninternalField nonuniform List<scalar>\n4\n(\n10\n20\n30\n40\n)\nboundaryField {}\n",
        )
        .expect("later aspect ratio field");
        let distributions = read_mesh_quality_distributions(&case);
        assert_eq!(distributions.len(), 4);
        let aspect = distributions
            .iter()
            .find(|distribution| distribution.field == "aspectRatio")
            .expect("aspect ratio distribution");
        assert_eq!(aspect.sample_count, 4);
        assert_eq!(aspect.min, 10.0);
        assert_eq!(aspect.max, 40.0);
        assert_eq!(aspect.values[10], 25.0);
        assert_eq!(aspect.source, "12/aspectRatio");
        let _ = fs::remove_dir_all(case);
    }

    #[test]
    fn native_y_plus_distribution_is_separate_from_mesh_quality() {
        let case = case_dir();
        let time = case.join("12");
        fs::create_dir_all(&time).expect("y plus field directory");
        fs::write(
            time.join("yPlus"),
            "FoamFile { class volScalarField; }\nboundaryField\n{\n    farField { value uniform 0; }\n    airfoil\n    {\n        value nonuniform List<scalar>\n        4\n        (\n            0.5\n            1.0\n            2.0\n            4.0\n        )\n    }\n}\n",
        )
        .expect("y plus field");
        let distribution =
            read_y_plus_distribution(&case, "airfoil").expect("wall y plus distribution");
        assert_eq!(distribution.field, "yPlus");
        assert_eq!(distribution.sample_count, 4);
        assert_eq!(distribution.min, 0.5);
        assert_eq!(distribution.max, 4.0);
        assert_eq!(distribution.source, "12/yPlus");
        let _ = fs::remove_dir_all(case);
    }
}
