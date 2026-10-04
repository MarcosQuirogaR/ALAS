// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Fail-closed parsing of static beam text results.

use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};

pub(super) struct Result {
    pub version: String,
    pub displacement: BTreeMap<usize, f64>,
    pub stress: BTreeMap<usize, f64>,
    pub moment: BTreeMap<usize, f64>,
    pub stress_ends: BTreeSet<(usize, usize, bool)>,
    pub force_ends: BTreeSet<(usize, usize, bool)>,
}

pub(super) fn read(text: &str) -> io::Result<Result> {
    if text.contains("FATAL") || !text.contains("END OF JOB") {
        return Err(io::Error::other(
            "F06 contains a fatal error or lacks END OF JOB",
        ));
    }
    let version = text
        .lines()
        .find_map(|line| {
            let (_, after) = line.split_once("Version ")?;
            after
                .split_whitespace()
                .next()
                .filter(|value| value.starts_with(|c: char| c.is_ascii_digit()))
                .map(str::to_owned)
        })
        .ok_or_else(|| io::Error::other("MSC version banner absent"))?;
    let mut displacement = BTreeMap::new();
    let mut stress = BTreeMap::new();
    let mut moment = BTreeMap::new();
    let mut stress_ends = BTreeSet::new();
    let mut force_ends = BTreeSet::new();
    let mut table = Table::Other;
    let mut element = None;
    for line in text.lines() {
        if line.contains("D I S P L A C E M E N T   V E C T O R") {
            table = Table::Displacement;
        } else if line.contains("S T R E S S E S   I N   B E A M") {
            table = Table::Stress;
        } else if line.contains("F O R C E S   I N   B E A M") {
            table = Table::Force;
        } else if line.contains("F O R C E")
            || line.contains("S T R E S S")
            || line.contains("D B D I C T")
        {
            table = Table::Other;
        }
        let values: Vec<_> = line.split_whitespace().collect();
        if table == Table::Displacement && values.len() == 8 && values[1] == "G" {
            if let Ok(id) = values[0].parse() {
                displacement.insert(id, number(values[4])?);
            }
        }
        if matches!(table, Table::Stress | Table::Force) {
            if values.len() == 2 && values[0] == "0" {
                element = values[1].parse::<usize>().ok();
            }
            if values.len() >= 8 && element.is_some() {
                if let (Ok(grid), Ok(distance)) =
                    (values[0].parse::<usize>(), values[1].parse::<f64>())
                {
                    if table == Table::Stress {
                        let maximum = values[2..8]
                            .iter()
                            .map(|value| number(value))
                            .collect::<io::Result<Vec<_>>>()?
                            .into_iter()
                            .map(f64::abs)
                            .fold(0.0_f64, f64::max);
                        stress
                            .entry(grid)
                            .and_modify(|value: &mut f64| *value = value.max(maximum))
                            .or_insert(maximum);
                        if let Some(id) = element {
                            if distance == 0.0 || distance == 1.0 {
                                stress_ends.insert((id, grid, distance == 1.0));
                            }
                        }
                    } else {
                        moment.insert(grid, number(values[2])?);
                        if let Some(id) = element {
                            if distance == 0.0 || distance == 1.0 {
                                force_ends.insert((id, grid, distance == 1.0));
                            }
                        }
                    }
                }
            }
        }
    }
    if displacement.is_empty() {
        return Err(io::Error::other("displacement table absent"));
    }
    Ok(Result {
        version,
        displacement,
        stress,
        moment,
        stress_ends,
        force_ends,
    })
}

#[derive(PartialEq, Eq)]
enum Table {
    Other,
    Displacement,
    Stress,
    Force,
}

fn number(value: &str) -> io::Result<f64> {
    let parsed: f64 = value.replace('D', "E").parse().map_err(io::Error::other)?;
    if !parsed.is_finite() {
        return Err(io::Error::other("nonfinite F06 value"));
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Parser fixtures are deliberate text inputs, rather than solver baselines.
    #[test]
    fn separates_displacement_reaction_and_beam_tables_across_pages() {
        let text = "* Version 2027.2-build *\nD I S P L A C E M E N T   V E C T O R\n1 G 0. 0. 2.5D-3 0. 0. 0.\nS P C   F O R C E S\n1 G 0. 0. 1000. 0. 0. 0.\nF O R C E S   I N   B E A M\n0 8\n1 0.000 10. 0. 1. 0. 0. 0. 0.\nS T R E S S E S   I N   B E A M\n0 8\n1 0.000 -2. 2. -2. 2. 2. -2.\n1 PAGE 2\nS T R E S S E S   I N   B E A M\n2 1.000 -1. 1. -1. 1. 1. -1.\nEND OF JOB";
        let result = read(text).unwrap();
        assert_eq!(result.version, "2027.2-build");
        assert_eq!(result.displacement[&1], 0.0025);
        assert_eq!(result.stress[&1], 2.0);
        assert_eq!(result.stress[&2], 1.0);
        assert_eq!(result.moment[&1], 10.0);
        assert_eq!(
            result.stress_ends,
            BTreeSet::from([(8, 1, false), (8, 2, true)])
        );
    }

    #[test]
    fn rejects_failed_truncated_and_nonfinite_results() {
        for text in ["FATAL\nEND OF JOB", "* Version 2026.1 *", "* Version 2026.1 *\nD I S P L A C E M E N T   V E C T O R\n1 G 0. 0. NaN 0. 0. 0.\nEND OF JOB"] {
            assert!(read(text).is_err());
        }
    }
}
