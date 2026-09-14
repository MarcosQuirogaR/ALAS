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
pub(crate) fn read_y_plus_summary(case_dir: &Path, patch_name: &str) -> Option<ParsedYPlusSummary> {
    if patch_name.trim().is_empty() {
        return None;
    }
    let root = case_dir.join("postProcessing/yPlus");
    for (time, directory) in numeric_time_dirs(&root).into_iter().rev() {
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
            && (row_time - time).abs() <= 1.0e-8 * row_time.abs().max(1.0)
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

fn numeric_time_dirs(root: &Path) -> Vec<(f64, PathBuf)> {
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
        let summary = case.join("postProcessing/yPlus/7");
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
        let summary = case.join("postProcessing/yPlus/7");
        fs::create_dir_all(&summary).expect("summary directory");
        fs::write(
            summary.join("yPlus.dat"),
            "# Time patch min max average\n7 airfoil 0 0 0\n",
        )
        .expect("summary output");
        assert!(read_y_plus_summary(&case, "airfoil").is_none());
        let _ = fs::remove_dir_all(case);
    }
}
