// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

/// Parse OpenFOAM residual lines from a captured utility log.
pub fn parse_residuals(log: &str) -> Vec<ResidualSample> {
    let mut samples = Vec::new();
    let mut fallback_iteration = 0_u64;
    let mut outer_iteration = None;
    for line in log.lines() {
        if let Some(time) = outer_time_after(line) {
            if time.is_finite() && time >= 0.0 && time <= u64::MAX as f64 {
                outer_iteration = Some(time.round() as u64);
            }
        }
        let Some((_, rest)) = line.split_once("Solving for ") else {
            continue;
        };
        let field = rest
            .split(|ch: char| ch == ',' || ch == ':' || ch.is_whitespace())
            .next()
            .unwrap_or("")
            .trim();
        if field.is_empty() {
            continue;
        }
        let Some(initial) = number_after(rest, "Initial residual =") else {
            continue;
        };
        let Some(final_residual) = number_after(rest, "Final residual =") else {
            continue;
        };
        samples.push(ResidualSample {
            iteration: outer_iteration.unwrap_or(fallback_iteration),
            field: field.to_owned(),
            initial,
            final_residual,
        });
        if outer_iteration.is_none() {
            fallback_iteration = fallback_iteration.wrapping_add(1);
        }
    }
    samples
}

/// Parse standard `forceCoeffs` columns.
///
/// OpenCFD's current output uses `Cd(f)`/`Cd(r)` and `Cl(f)`/`Cl(r)` for
/// front/rear surface contributions. Those are geometric partitions, not
/// pressure/viscous contributions. We therefore use the header to identify
/// total coefficients and only populate the pressure/viscous fields when a
/// release explicitly names those columns. A headerless eight-column fixture
/// is retained for backwards-compatible imports; ambiguous headerless rows
/// expose only their first four total coefficients.
pub fn parse_force_coefficients(text: &str) -> Vec<ForceSample> {
    let columns = text.lines().find_map(parse_force_header);
    let mut rows = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        let values = trimmed
            .split_whitespace()
            .filter_map(|token| token.parse::<f64>().ok())
            .collect::<Vec<_>>();
        if values.len() < 4 {
            continue;
        }
        let sample = if let Some(columns) = columns.as_deref() {
            let time = force_column_value(&values, columns, &["time"]);
            let cd = force_column_value(&values, columns, &["cd"]);
            let cl = force_column_value(&values, columns, &["cl"]);
            let cm = force_column_value(&values, columns, &["cmpitch", "cm"]);
            let (Some(time), Some(cd), Some(cl), Some(cm)) = (time, cd, cl, cm) else {
                continue;
            };
            ForceSample {
                time,
                cd,
                cl,
                cm,
                cd_pressure: force_column_value(
                    &values,
                    columns,
                    &["cdpressure", "pressurecd", "cdp"],
                ),
                cd_viscous: force_column_value(
                    &values,
                    columns,
                    &["cdviscous", "viscouscd", "cdv"],
                ),
                cl_pressure: force_column_value(
                    &values,
                    columns,
                    &["clpressure", "pressurecl", "clp"],
                ),
                cl_viscous: force_column_value(
                    &values,
                    columns,
                    &["clviscous", "viscouscl", "clv"],
                ),
            }
        } else {
            let (cd_pressure, cd_viscous, cl_pressure, cl_viscous) = if values.len() == 8 {
                // Legacy ALAS fixture layout:
                // Time Cd Cl Cm CdPressure CdViscous ClPressure ClViscous.
                (
                    Some(values[4]),
                    Some(values[5]),
                    Some(values[6]),
                    Some(values[7]),
                )
            } else {
                (None, None, None, None)
            };
            ForceSample {
                time: values[0],
                cd: values[1],
                cl: values[2],
                cm: values[3],
                cd_pressure,
                cd_viscous,
                cl_pressure,
                cl_viscous,
            }
        };
        rows.push(sample);
    }
    rows
}

fn parse_force_header(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();
    if !(trimmed.starts_with('#') || trimmed.starts_with("//")) {
        return None;
    }
    let columns = trimmed
        .trim_start_matches(|ch| ch == '#' || ch == '/')
        .split_whitespace()
        .map(normalize_force_column)
        .filter(|column| !column.is_empty())
        .collect::<Vec<_>>();
    (columns.iter().any(|column| column == "time")
        && columns.iter().any(|column| column == "cd")
        && columns.iter().any(|column| column == "cl"))
    .then_some(columns)
}

fn normalize_force_column(column: &str) -> String {
    column
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn force_column_value(values: &[f64], columns: &[String], aliases: &[&str]) -> Option<f64> {
    aliases.iter().find_map(|alias| {
        let normalized = normalize_force_column(alias);
        columns
            .iter()
            .position(|column| column == &normalized)
            .and_then(|index| values.get(index).copied())
    })
}

/// Parse the vector rows written by OpenFOAM's `forces` function object.
///
/// The v2606 format starts each data row with time, followed by pressure,
/// viscous and (when configured) porous force vectors, then moment vectors.
/// Parentheses and whitespace are deliberately ignored so this remains
/// compatible with the compact and expanded serializers used by OpenFOAM
/// releases.
pub fn parse_force_decomposition(text: &str) -> Vec<ForceDecompositionSample> {
    let columns = text.lines().find_map(parse_force_decomposition_header);
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
                return None;
            }
            let values = extract_numeric_values(trimmed);
            if values.len() < 7 {
                return None;
            }
            if let Some(columns) = columns.as_deref() {
                let time = force_column_value(&values, columns, &["time"])?;
                let pressure = decomposition_vector(
                    &values,
                    columns,
                    ["pressurex", "pressurey", "pressurez"],
                )?;
                let viscous =
                    decomposition_vector(&values, columns, ["viscousx", "viscousy", "viscousz"])?;
                Some(ForceDecompositionSample {
                    time,
                    pressure_force_n: pressure,
                    viscous_force_n: viscous,
                    porous_force_n: decomposition_vector(
                        &values,
                        columns,
                        ["porousx", "porousy", "porousz"],
                    ),
                })
            } else {
                // Headerless compact serializers are interpreted as time,
                // pressure vector, viscous vector, optional porous vector.
                // The official OpenCFD force.dat header is handled above and
                // has the unambiguous total, pressure, viscous order.
                let vector =
                    |offset: usize| [values[offset], values[offset + 1], values[offset + 2]];
                Some(ForceDecompositionSample {
                    time: values[0],
                    pressure_force_n: vector(1),
                    viscous_force_n: vector(4),
                    porous_force_n: (values.len() >= 10).then(|| vector(7)),
                })
            }
        })
        .collect()
}

fn parse_force_decomposition_header(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();
    if !(trimmed.starts_with('#') || trimmed.starts_with("//")) {
        return None;
    }
    let columns = trimmed
        .trim_start_matches(|ch| ch == '#' || ch == '/')
        .split_whitespace()
        .map(normalize_force_column)
        .filter(|column| !column.is_empty())
        .collect::<Vec<_>>();
    (columns.iter().any(|column| column == "time")
        && columns.iter().any(|column| column == "pressurex")
        && columns.iter().any(|column| column == "viscousx"))
    .then_some(columns)
}

fn decomposition_vector(
    values: &[f64],
    columns: &[String],
    aliases: [&str; 3],
) -> Option<[f64; 3]> {
    Some([
        force_column_value(values, columns, &[aliases[0]])?,
        force_column_value(values, columns, &[aliases[1]])?,
        force_column_value(values, columns, &[aliases[2]])?,
    ])
}

/// Add pressure/viscous coefficient components parsed from a `forces` output
/// file to the corresponding total coefficient samples. Missing or malformed
/// decomposition data is left as `None` so a front/rear partition can never
/// be presented as a physical pressure/viscous split.
pub fn apply_force_decomposition(
    samples: &mut [ForceSample],
    decomposition: &[ForceDecompositionSample],
    density_kg_m3: f64,
    speed_m_s: f64,
    reference_area_m2: f64,
    drag_dir: [f64; 3],
    lift_dir: [f64; 3],
) {
    let denominator = 0.5 * density_kg_m3 * speed_m_s * speed_m_s * reference_area_m2;
    if !denominator.is_finite() || denominator <= 0.0 {
        return;
    }
    for sample in samples {
        let Some(component) = decomposition.iter().rev().find(|component| {
            (component.time - sample.time).abs() <= 1.0e-7 * sample.time.abs().max(1.0)
        }) else {
            continue;
        };
        sample.cd_pressure = Some(dot3(component.pressure_force_n, drag_dir) / denominator);
        sample.cd_viscous = Some(dot3(component.viscous_force_n, drag_dir) / denominator);
        sample.cl_pressure = Some(dot3(component.pressure_force_n, lift_dir) / denominator);
        sample.cl_viscous = Some(dot3(component.viscous_force_n, lift_dir) / denominator);
    }
}

fn dot3(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

pub(crate) fn extract_numeric_values(text: &str) -> Vec<f64> {
    text.split(|ch: char| !(ch.is_ascii_digit() || matches!(ch, '+' | '-' | '.' | 'e' | 'E')))
        .filter_map(|token| token.trim_end_matches('.').parse::<f64>().ok())
        .collect()
}

/// Parse continuity-error summaries from a simpleFoam log.
pub fn parse_mass_balance(log: &str) -> Vec<MassBalanceSample> {
    let mut rows = Vec::new();
    let mut outer_time = None;
    for line in log.lines() {
        if let Some(time) = outer_time_after(line) {
            if time.is_finite() {
                outer_time = Some(time);
            }
        }
        let lower = line.to_ascii_lowercase();
        if !lower.contains("continuity") || !lower.contains("sum local") {
            continue;
        }
        let sum_local = number_after(line, "sum local =");
        let global = number_after(line, "global =");
        let cumulative = number_after(line, "cumulative =");
        rows.push(MassBalanceSample {
            time: outer_time,
            sum_local,
            global,
            cumulative,
        });
    }
    rows
}

fn number_after(text: &str, marker: &str) -> Option<f64> {
    let (_, rest) = text.split_once(marker)?;
    rest.split_whitespace().find_map(|token| {
        token
            .trim_matches(|ch: char| {
                !(ch.is_ascii_digit() || matches!(ch, '+' | '-' | '.' | 'e' | 'E'))
            })
            .parse::<f64>()
            .ok()
    })
}

/// Parse an OpenFOAM outer-iteration marker without matching
/// `ExecutionTime = ...` lines, which contain the same `Time =` substring.
fn outer_time_after(line: &str) -> Option<f64> {
    let tail = line.trim_start().strip_prefix("Time =")?;
    tail.split_whitespace().find_map(|token| {
        token
            .trim_matches(|ch: char| {
                !(ch.is_ascii_digit() || matches!(ch, '+' | '-' | '.' | 'e' | 'E'))
            })
            .parse::<f64>()
            .ok()
    })
}
