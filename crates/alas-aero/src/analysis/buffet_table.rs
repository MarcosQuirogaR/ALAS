// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Buffet-onset tables: the section lift at which MSES puts the pre-shock
//! local Mach number at 1.38, tabulated offline per section family.
//!
//! The shipped data file (`data/buffet_onset_tables.txt`) carries its own
//! provenance (MSES version, criterion, source, generation command). One
//! table per family (a library airfoil name) over three axes, all in the
//! plane normal to the 0.55 c shock line (simple sweep theory):
//!
//! * `mach`: normal Mach number `M cos L_s`;
//! * `tc`: maximum thickness over chord of the normal section;
//! * `camber`: maximum camber over chord of the normal section.
//!
//! Each cell is the normal-section lift coefficient `cl_n` at onset. A
//! lookup is trilinear inside the axes; below the Mach axis and outside the
//! thickness or camber axis it is clamped to the edge, above the Mach axis it
//! continues with the slope of the last Mach interval, and either is reported
//! as [`TableLookup::Extrapolated`]; a missing or failed
//! cell is [`TableLookup::MissingCell`]. Nothing here falls back to another
//! model.

use alas_geom::aircraft::airfoil::Airfoil;
use std::sync::OnceLock;

/// The shipped data file.
const TABLE_TEXT: &str = include_str!("../../data/buffet_onset_tables.txt");

/// Pre-shock Mach number at buffet onset (Obert 2009, Figs. 17.5-17.6).
pub const BUFFET_PRE_SHOCK_MACH: f64 = 1.38;

/// Smallest share of the interpolation weight that must sit on valid cells for a
/// box with failed cells to be answered (flagged `partial-cell`) instead of
/// reported missing.
const PARTIAL_COVERAGE: f64 = 0.5;

/// One family's onset table.
#[derive(Debug, Clone, PartialEq)]
pub struct FamilyTable {
    /// Library airfoil name of the family.
    pub family: String,
    /// Normal Mach axis, ascending.
    pub mach: Vec<f64>,
    /// Normal-section thickness axis, ascending.
    pub thickness: Vec<f64>,
    /// Normal-section camber axis, ascending.
    pub camber: Vec<f64>,
    /// Onset `cl_n`, index `(i_mach * n_tc + i_tc) * n_camber + i_camber`;
    /// `NaN` where the cell has no value.
    pub onset_cl: Vec<f64>,
}

/// Result of a table lookup.
#[derive(Debug, Clone, PartialEq)]
pub enum TableLookup {
    /// Inside all three axes.
    Interpolated(f64),
    /// Outside at least one axis: the value at the clamped point, with the
    /// names of the axes that were clamped.
    Extrapolated {
        /// Onset `cl_n` at the clamped point.
        cl_n: f64,
        /// Axes that were outside the table (`mach`, `tc`, `camber`), or
        /// `partial-cell` when failed cells were left out of the interpolation.
        axes: Vec<&'static str>,
    },
    /// A cell of the interpolation box has no value.
    MissingCell,
}

impl FamilyTable {
    /// Trilinear lookup at `(mach_n, tc_n, camber_n)`.
    pub fn lookup(&self, mach_n: f64, tc_n: f64, camber_n: f64) -> TableLookup {
        let mut axes = Vec::new();
        let mut locate = |axis: &[f64], value: f64, name: &'static str| -> (usize, f64) {
            let (lo, hi) = (axis[0], axis[axis.len() - 1]);
            if value < lo || value > hi {
                axes.push(name);
            }
            // Above the Mach axis the onset keeps the slope of the last
            // interval: held at the edge value it would stop falling with
            // Mach, and a wing with less sweep would read a larger margin.
            let v = if name == "mach" {
                value.max(lo)
            } else {
                value.clamp(lo, hi)
            };
            if axis.len() == 1 {
                return (0, 0.0);
            }
            let k = axis.partition_point(|&a| a <= v).clamp(1, axis.len() - 1) - 1;
            (k, (v - axis[k]) / (axis[k + 1] - axis[k]))
        };
        let (im, fm) = locate(&self.mach, mach_n, "mach");
        let (it, ft) = locate(&self.thickness, tc_n, "tc");
        let (ic, fc) = locate(&self.camber, camber_n, "camber");
        let (nt, nc) = (self.thickness.len(), self.camber.len());
        let mut missing = false;
        let (mut value, mut covered) = (0.0, 0.0);
        for (dm, wm) in [(0, 1.0 - fm), (1, fm)] {
            for (dt, wt) in [(0, 1.0 - ft), (1, ft)] {
                for (dc, wc) in [(0, 1.0 - fc), (1, fc)] {
                    let weight = wm * wt * wc;
                    if weight == 0.0 {
                        continue;
                    }
                    let (a, b, c) = (
                        (im + dm).min(self.mach.len() - 1),
                        (it + dt).min(nt - 1),
                        (ic + dc).min(nc - 1),
                    );
                    let cell = self.onset_cl[(a * nt + b) * nc + c];
                    if !cell.is_finite() {
                        missing = true;
                        continue;
                    }
                    value += weight * cell;
                    covered += weight;
                }
            }
        }
        // An extrapolation needs every cell of its box: the weights are of
        // both signs and cannot be renormalised over the valued ones.
        if covered < PARTIAL_COVERAGE - 1e-9 || (missing && fm > 1.0) {
            return TableLookup::MissingCell;
        }
        value = (value / covered).max(0.0);
        if missing {
            axes.push("partial-cell");
        }
        if axes.is_empty() {
            TableLookup::Interpolated(value)
        } else {
            TableLookup::Extrapolated { cl_n: value, axes }
        }
    }
}

/// The parsed data file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BuffetTables {
    /// Normal chord Reynolds number the tables were generated at.
    pub reynolds_n: f64,
    /// One table per family.
    pub families: Vec<FamilyTable>,
}

impl BuffetTables {
    /// The family called `name`.
    pub fn family(&self, name: &str) -> Option<&FamilyTable> {
        self.families.iter().find(|f| f.family == name)
    }

    /// Parse the data-file text (see the file header for the format).
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut tables = Self::default();
        let mut current: Option<FamilyTable> = None;
        let floats = |parts: &[&str]| -> Result<Vec<f64>, String> {
            parts
                .iter()
                .map(|p| p.parse::<f64>().map_err(|e| format!("{p}: {e}")))
                .collect()
        };
        for (number, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            let err = |m: &str| format!("line {}: {m}", number + 1);
            match parts[0] {
                "reynolds_n" => tables.reynolds_n = floats(&parts[1..])?[0],
                "criterion" => {}
                "family" => {
                    current = Some(FamilyTable {
                        family: parts.get(1).ok_or_else(|| err("family name"))?.to_string(),
                        mach: Vec::new(),
                        thickness: Vec::new(),
                        camber: Vec::new(),
                        onset_cl: Vec::new(),
                    });
                }
                "axis" => {
                    let f = current.as_mut().ok_or_else(|| err("axis before family"))?;
                    let values = floats(&parts[2..])?;
                    match parts.get(1).copied() {
                        Some("mach") => f.mach = values,
                        Some("tc") => f.thickness = values,
                        Some("camber") => f.camber = values,
                        _ => return Err(err("unknown axis")),
                    }
                    if f.mach.len() * f.thickness.len() * f.camber.len() > 0
                        && f.onset_cl.is_empty()
                    {
                        f.onset_cl =
                            vec![f64::NAN; f.mach.len() * f.thickness.len() * f.camber.len()];
                    }
                }
                "cell" => {
                    let f = current.as_mut().ok_or_else(|| err("cell before family"))?;
                    if parts.len() != 5 {
                        return Err(err("cell needs 4 fields"));
                    }
                    let idx: Vec<usize> = parts[1..4]
                        .iter()
                        .map(|p| p.parse().map_err(|_| err("index")))
                        .collect::<Result<_, _>>()?;
                    let (nt, nc) = (f.thickness.len(), f.camber.len());
                    if idx[0] >= f.mach.len() || idx[1] >= nt || idx[2] >= nc {
                        return Err(err("cell index outside the axes"));
                    }
                    f.onset_cl[(idx[0] * nt + idx[1]) * nc + idx[2]] =
                        parts[4].parse().unwrap_or(f64::NAN);
                }
                "end" => {
                    let f = current.take().ok_or_else(|| err("end before family"))?;
                    if f.onset_cl.is_empty() {
                        return Err(err("family without axes"));
                    }
                    tables.families.push(f);
                }
                other => return Err(err(&format!("unknown record {other}"))),
            }
        }
        Ok(tables)
    }
}

/// The shipped tables, parsed once. An unreadable data file yields no
/// families, so every lookup is then reported unavailable rather than
/// answered by another model.
pub fn shipped_tables() -> &'static BuffetTables {
    static TABLES: OnceLock<BuffetTables> = OnceLock::new();
    TABLES.get_or_init(|| BuffetTables::parse(TABLE_TEXT).unwrap_or_default())
}

/// Maximum thickness and maximum camber over chord of `airfoil`, sampled on
/// 201 stations.
pub fn thickness_and_camber(airfoil: &Airfoil) -> (f64, f64) {
    let x: Vec<f64> = (0..=200).map(|i| f64::from(i) / 200.0).collect();
    let t = airfoil
        .local_thickness(&x)
        .into_iter()
        .fold(0.0_f64, f64::max);
    let c =
        airfoil
            .local_camber(&x)
            .into_iter()
            .fold(0.0_f64, |m, v| if v.abs() > m.abs() { v } else { m });
    (t, c)
}

/// The family shape `base` with its thickness distribution scaled to maximum
/// thickness `tc` and its mean line scaled to maximum camber `camber`
/// (cosine-spaced, 101 stations per surface). `None` when `base` has no
/// thickness, or no camber to scale.
pub fn family_section(base: &Airfoil, tc: f64, camber: f64) -> Option<Airfoil> {
    let (t_max, c_max) = thickness_and_camber(base);
    if t_max <= 1e-6 || c_max.abs() <= 1e-6 {
        return None;
    }
    let x: Vec<f64> = (0..=100)
        .map(|i| 0.5 * (1.0 - (std::f64::consts::PI * f64::from(i) / 100.0).cos()))
        .collect();
    let t = base.local_thickness(&x);
    let c = base.local_camber(&x);
    let (ts, cs) = (tc / t_max, camber / c_max);
    let mut coordinates: Vec<(f64, f64)> = (0..x.len())
        .rev()
        .map(|i| (x[i], c[i] * cs + 0.5 * t[i] * ts))
        .collect();
    coordinates.extend((1..x.len()).map(|i| (x[i], c[i] * cs - 0.5 * t[i] * ts)));
    Some(Airfoil::from_coordinates(base.name.clone(), coordinates))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn table() -> FamilyTable {
        let text = "reynolds_n 2e7\nfamily f\naxis mach 0.6 0.7\naxis tc 0.08 0.12\n\
                    axis camber 0.01\ncell 0 0 0 1.0\ncell 0 1 0 0.8\ncell 1 0 0 0.6\n\
                    cell 1 1 0 0.4\nend\n";
        BuffetTables::parse(text).unwrap().families.remove(0)
    }

    #[test]
    fn lookup_is_trilinear_inside_and_flags_clamped_axes() {
        let t = table();
        match t.lookup(0.65, 0.10, 0.01) {
            TableLookup::Interpolated(v) => assert!((v - 0.7).abs() < 1e-12, "{v}"),
            other => panic!("{other:?}"),
        }
        // Above the Mach axis the last interval's slope continues (0.9 at
        // 0.6, 0.5 at 0.7); camber is clamped.
        match t.lookup(0.75, 0.10, 0.02) {
            TableLookup::Extrapolated { cl_n, axes } => {
                assert!((cl_n - 0.3).abs() < 1e-12, "{cl_n}");
                assert_eq!(axes, vec!["mach", "camber"]);
            }
            other => panic!("{other:?}"),
        }
        // Below the Mach axis the edge value is held; far above it the onset
        // does not go negative.
        match t.lookup(0.55, 0.10, 0.01) {
            TableLookup::Extrapolated { cl_n, axes } => {
                assert!((cl_n - 0.9).abs() < 1e-12, "{cl_n}");
                assert_eq!(axes, vec!["mach"]);
            }
            other => panic!("{other:?}"),
        }
        match t.lookup(0.95, 0.10, 0.01) {
            TableLookup::Extrapolated { cl_n, .. } => assert!(cl_n == 0.0, "{cl_n}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_missing_cell_is_reported_or_flagged_partial() {
        let text =
            "family f\naxis mach 0.6 0.7\naxis tc 0.1\naxis camber 0.01\ncell 0 0 0 1\nend\n";
        let t = BuffetTables::parse(text).unwrap().families.remove(0);
        assert_eq!(t.lookup(0.69, 0.1, 0.01), TableLookup::MissingCell);
        // Half the weight on the valid cell: answered, flagged partial.
        match t.lookup(0.65, 0.1, 0.01) {
            TableLookup::Extrapolated { cl_n, axes } => {
                assert!((cl_n - 1.0).abs() < 1e-12);
                assert_eq!(axes, vec!["partial-cell"]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_family_section_has_the_requested_thickness_and_camber() {
        let base = Airfoil::from_name("naca2412").unwrap();
        let section = family_section(&base, 0.10, 0.03).unwrap();
        let (t, c) = thickness_and_camber(&section);
        assert!((t - 0.10).abs() < 2e-3, "{t}");
        assert!((c - 0.03).abs() < 2e-3, "{c}");
    }

    #[test]
    fn the_shipped_file_parses() {
        assert!(BuffetTables::parse(TABLE_TEXT).is_ok());
    }
}
