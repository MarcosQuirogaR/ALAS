// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The acceptance criterion of each preset's design range.
//!
//! A design mission is flown at the preset's charted design point
//! ([`crate::PayloadRangeDesignPoint`]); this module says what the flown
//! range must reach. Two tab-separated tables under `crates/alas-config/data/`
//! are the repository's definition of the bands and grades (tolerance table
//! A 10 %, B 10 %, C 15 %, CG 2 / 3 %MAC, and the range-cell grades of the
//! reference files):
//!
//! - `reference_grade_bands.tsv`: the accepted relative half-width per
//!   reference grade (A and B +-10 %, C +-15 %);
//! - `design_range_cells.tsv`: per preset, the criterion (a band around the
//!   published range at the range cell's grade, or, for a notional aircraft,
//!   its own requirement as a floor), and the published range of a
//!   payload-defined point, whose design point declares none.
//!
//! The bands are acceptance criteria, not physics: nothing here feeds a model.
//! Ranges are still-air nautical miles (1 nmi = 1,852 m).

use std::sync::OnceLock;

use super::AircraftPreset;

const GRADE_BANDS_TSV: &str = include_str!("../../data/reference_grade_bands.tsv");
const RANGE_CELLS_TSV: &str = include_str!("../../data/design_range_cells.tsv");

/// Credibility grade of a matrix reference value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReferenceGrade {
    /// Manufacturer primary document with the conditions stated.
    A,
    /// Manufacturer document read from a chart, or with conditions incomplete.
    B,
    /// Secondary source.
    C,
}

impl ReferenceGrade {
    /// Stable report label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "A" => Some(Self::A),
            "B" => Some(Self::B),
            "C" => Some(Self::C),
            _ => None,
        }
    }
}

/// The accepted band of one reference grade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradeBand {
    /// The grade.
    pub grade: ReferenceGrade,
    /// Accepted `|model / reference - 1|`, dimensionless.
    pub relative_half_width: f64,
    /// Accepted absolute CG-limit difference, % MAC.
    pub cg_half_width_pct_mac: f64,
}

/// What the flown design range must reach.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DesignRangeCriterion {
    /// Within the grade's band of a published range.
    WithinBand {
        /// Published range at the design payload, nmi.
        published_range_nmi: f64,
        /// The band of the range cell's grade.
        band: GradeBand,
        /// Why the grade (and, for a payload-defined point, the published range).
        basis: &'static str,
    },
    /// At least the notional aircraft's own design requirement, no band.
    AtLeastRequirement {
        /// Required range at the design payload, nmi.
        required_range_nmi: f64,
        /// Where the requirement is declared.
        basis: &'static str,
    },
}

impl DesignRangeCriterion {
    /// Whether a flown range, nmi, meets the criterion. A non-finite range
    /// never does.
    #[must_use]
    pub fn accepts(&self, flown_range_nmi: f64) -> bool {
        if !flown_range_nmi.is_finite() {
            return false;
        }
        match *self {
            Self::WithinBand {
                published_range_nmi,
                band,
                ..
            } => (flown_range_nmi / published_range_nmi - 1.0).abs() <= band.relative_half_width,
            Self::AtLeastRequirement {
                required_range_nmi, ..
            } => flown_range_nmi >= required_range_nmi,
        }
    }

    /// The reference range the criterion is stated against, nmi.
    #[must_use]
    pub const fn reference_range_nmi(&self) -> f64 {
        match *self {
            Self::WithinBand {
                published_range_nmi,
                ..
            } => published_range_nmi,
            Self::AtLeastRequirement {
                required_range_nmi, ..
            } => required_range_nmi,
        }
    }

    /// One phrase a report can print, e.g. `"grade B, +-10 % of 5000 nmi"`.
    #[must_use]
    pub fn describe(&self) -> String {
        match *self {
            Self::WithinBand {
                published_range_nmi,
                band,
                ..
            } => format!(
                "grade {}, +-{:.0} % of {published_range_nmi:.0} nmi",
                band.grade.label(),
                band.relative_half_width * 100.0
            ),
            Self::AtLeastRequirement {
                required_range_nmi, ..
            } => format!("at least the {required_range_nmi:.0} nmi requirement"),
        }
    }
}

/// One parsed row of `design_range_cells.tsv`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct RangeCell {
    preset: &'static str,
    at_least: bool,
    grade: Option<ReferenceGrade>,
    published_range_nmi: Option<f64>,
    basis: &'static str,
}

/// Data rows of a table: comments and blank lines dropped, split on tabs.
fn rows(table: &'static str) -> impl Iterator<Item = (usize, Vec<&'static str>)> {
    table
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|(index, line)| (index + 1, line.split('\t').map(str::trim).collect()))
}

fn parse_fraction(text: &str, line: usize, what: &str) -> Result<f64, String> {
    text.parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| format!("line {line}: {what} '{text}' is not a positive number"))
}

fn parse_grade_bands() -> Result<Vec<GradeBand>, String> {
    rows(GRADE_BANDS_TSV)
        .map(|(line, fields)| {
            let [grade, relative, cg] = fields.as_slice() else {
                return Err(format!(
                    "reference_grade_bands.tsv line {line}: expected 3 fields"
                ));
            };
            Ok(GradeBand {
                grade: ReferenceGrade::parse(grade).ok_or_else(|| {
                    format!("reference_grade_bands.tsv line {line}: unknown grade '{grade}'")
                })?,
                relative_half_width: parse_fraction(relative, line, "relative half-width")?,
                cg_half_width_pct_mac: parse_fraction(cg, line, "CG half-width")?,
            })
        })
        .collect()
}

fn parse_range_cells() -> Result<Vec<RangeCell>, String> {
    rows(RANGE_CELLS_TSV)
        .map(|(line, fields)| {
            let [preset, criterion, grade, range, basis] = fields.as_slice() else {
                return Err(format!("design_range_cells.tsv line {line}: expected 5 fields"));
            };
            let at_least = match *criterion {
                "band" => false,
                "at_least" => true,
                other => {
                    return Err(format!(
                        "design_range_cells.tsv line {line}: unknown criterion '{other}'"
                    ))
                }
            };
            let grade = match *grade {
                "-" => None,
                text => Some(ReferenceGrade::parse(text).ok_or_else(|| {
                    format!("design_range_cells.tsv line {line}: unknown grade '{text}'")
                })?),
            };
            if at_least == grade.is_some() {
                return Err(format!(
                    "design_range_cells.tsv line {line}: a band row needs a grade, an at_least row none"
                ));
            }
            let published_range_nmi = match *range {
                "-" => None,
                text => Some(parse_fraction(text, line, "published range")?),
            };
            Ok(RangeCell {
                preset,
                at_least,
                grade,
                published_range_nmi,
                basis,
            })
        })
        .collect()
}

/// The grade bands, in table order.
///
/// # Errors
///
/// The parse error of `reference_grade_bands.tsv`.
pub fn reference_grade_bands() -> Result<&'static [GradeBand], String> {
    static BANDS: OnceLock<Result<Vec<GradeBand>, String>> = OnceLock::new();
    BANDS
        .get_or_init(parse_grade_bands)
        .as_deref()
        .map_err(Clone::clone)
}

fn range_cells() -> Result<&'static [RangeCell], String> {
    static CELLS: OnceLock<Result<Vec<RangeCell>, String>> = OnceLock::new();
    CELLS
        .get_or_init(parse_range_cells)
        .as_deref()
        .map_err(Clone::clone)
}

/// The band of one grade.
///
/// # Errors
///
/// When the table does not parse or holds no row for `grade`.
pub fn grade_band(grade: ReferenceGrade) -> Result<GradeBand, String> {
    reference_grade_bands()?
        .iter()
        .find(|band| band.grade == grade)
        .copied()
        .ok_or_else(|| format!("reference_grade_bands.tsv has no grade {}", grade.label()))
}

/// The acceptance criterion of `preset`'s design range.
///
/// # Errors
///
/// When the preset has no design point, no row, a band row without a
/// published range (a payload-defined point with no number in the table),
/// or a table that does not parse.
pub fn design_range_criterion(preset: &AircraftPreset) -> Result<DesignRangeCriterion, String> {
    let point = preset
        .reference
        .design_point
        .ok_or_else(|| format!("{} declares no design point", preset.name))?;
    let cell = range_cells()?
        .iter()
        .find(|cell| cell.preset == preset.name)
        .ok_or_else(|| format!("design_range_cells.tsv has no row for {}", preset.name))?;
    let declared_range_nmi =
        (!point.is_payload_defined() && point.range_nmi > 0.0).then_some(point.range_nmi);
    let reference_nmi = cell.published_range_nmi.or(declared_range_nmi).ok_or_else(|| {
        format!(
            "{}: a payload-defined design point needs its published range in design_range_cells.tsv",
            preset.name
        )
    })?;
    if cell.at_least {
        return Ok(DesignRangeCriterion::AtLeastRequirement {
            required_range_nmi: reference_nmi,
            basis: cell.basis,
        });
    }
    let grade = cell.grade.ok_or_else(|| {
        format!(
            "design_range_cells.tsv: band row for {} has no grade",
            preset.name
        )
    })?;
    Ok(DesignRangeCriterion::WithinBand {
        published_range_nmi: reference_nmi,
        band: grade_band(grade)?,
        basis: cell.basis,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn grade_bands_are_the_owner_directive() {
        let bands = reference_grade_bands().unwrap();
        let half = |grade| grade_band(grade).unwrap().relative_half_width;
        assert_eq!(bands.len(), 3);
        assert_eq!(half(ReferenceGrade::A), 0.10);
        assert_eq!(half(ReferenceGrade::B), 0.10);
        assert_eq!(half(ReferenceGrade::C), 0.15);
    }

    #[test]
    fn every_registered_preset_has_exactly_one_range_criterion() {
        let cells = range_cells().unwrap();
        for preset in super::super::registry() {
            let rows = cells
                .iter()
                .filter(|cell| cell.preset == preset.name)
                .count();
            assert_eq!(rows, 1, "{} rows for {}", rows, preset.name);
            let criterion = design_range_criterion(preset).unwrap();
            assert!(criterion.reference_range_nmi() > 0.0, "{}", preset.name);
        }
        for cell in cells {
            assert!(
                super::super::get(cell.preset).is_ok(),
                "row for unregistered preset {}",
                cell.preset
            );
        }
    }

    #[test]
    fn a_number_in_the_table_is_only_for_a_payload_defined_point() {
        for cell in range_cells().unwrap() {
            let preset = super::super::get(cell.preset).unwrap();
            let payload_defined = preset
                .reference
                .design_point
                .is_some_and(|point| point.is_payload_defined());
            assert_eq!(
                cell.published_range_nmi.is_some(),
                payload_defined,
                "{}: a declared range is read from the design point, not repeated",
                cell.preset
            );
        }
    }

    #[test]
    fn only_the_notional_aircraft_is_graded_on_its_own_requirement() {
        let at_least: Vec<_> = range_cells()
            .unwrap()
            .iter()
            .filter(|cell| cell.at_least)
            .map(|cell| cell.preset)
            .collect();
        assert_eq!(at_least, ["AVE"]);
    }

    #[test]
    fn band_and_floor_accept_inside_refuse_outside_and_never_nan() {
        let band = DesignRangeCriterion::WithinBand {
            published_range_nmi: 1_000.0,
            band: GradeBand {
                grade: ReferenceGrade::C,
                relative_half_width: 0.15,
                cg_half_width_pct_mac: 3.0,
            },
            basis: "test",
        };
        assert!(band.accepts(851.0) && band.accepts(1_149.0));
        assert!(!band.accepts(849.0) && !band.accepts(1_151.0) && !band.accepts(f64::NAN));
        let floor = DesignRangeCriterion::AtLeastRequirement {
            required_range_nmi: 7_600.0,
            basis: "test",
        };
        assert!(floor.accepts(7_600.0) && floor.accepts(9_000.0) && !floor.accepts(7_599.0));
    }
}
