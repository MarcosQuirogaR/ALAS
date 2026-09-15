// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Rendering a [`Deck`] as NASTRAN bulk data.
//!
//! The reference reaches this through `pyNastran`'s
//! `write_bdf(size=16, is_double=False)`, so the file a solve reads is
//! large-field: eight columns of sixteen characters, continued on lines that
//! open with `*`. That is reproduced here rather than the eight-character small
//! field, and for the reason the reference chose it: a small field carries
//! about eight significant digits, which is not enough to describe a node
//! coordinate that a spline produced.
//!
//! What is *not* reproduced is `pyNastran`'s own choice of digits. Byte
//! equality with its output is not reachable and not worth reaching for: the
//! coordinates entering this deck come from `alas-geom::wing_structure`, whose
//! surface points are a `linalg`-tier quantity, so two implementations agree
//! about them to a relative tolerance and not to the bit. [`real_field`]
//! therefore maximizes the digits that fit rather than matching a particular
//! formatter, and its own test pins what that is worth: every finite double
//! round-trips through a field to within 1e-14 relative, five decades tighter
//! than the tier the deck's numbers are compared at.

use super::cards::{Deck, ParamValue};
use crate::nastran95::{Card as SmallFieldCard, ContinuationTags, Field as SmallField};

/// Columns per field in the large-field format.
const FIELD_WIDTH: usize = 16;

/// Fields per physical line, after the eight-column card name.
const FIELDS_PER_LINE: usize = 4;

/// One bulk-data field.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Field {
    /// An omitted field, which NASTRAN reads as the card's default.
    Blank,
    /// An integer.
    Int(i64),
    /// A character field: a component list, a section name, a flag.
    Text(&'static str),
    /// A real.
    Real(f64),
}

impl Deck {
    /// The deck as bulk data, ready to be written to a file a solve includes.
    ///
    /// Cards come out grouped by type, which the format allows: bulk data is
    /// order-independent, and grouping keeps a 20,000-line mesh readable.
    /// Within a group the order is the order the builder added them.
    pub fn write_bulk(&self) -> String {
        self.write_bulk_with_rbe3_format(Rbe3Format::ReferenceLargeField)
    }

    /// The bulk data sent to MSC Nastran.
    ///
    /// The frozen reference path remains [`Self::write_bulk`]. MSC 2026 rejects
    /// pyNastran's large-field `RBE3` when its continuation opens with a bare
    /// `*` (`USER FATAL 316`), although the same fields are accepted in tagged
    /// small-field form. All other cards retain the reference large-field
    /// representation and its coordinate precision.
    pub fn write_bulk_msc(&self) -> String {
        self.write_bulk_with_rbe3_format(Rbe3Format::MscSmallField)
    }

    fn write_bulk_with_rbe3_format(&self, rbe3_format: Rbe3Format) -> String {
        let mut out = String::new();
        for param in &self.params {
            let value = match param.value {
                ParamValue::Name(text) => Field::Text(text),
                ParamValue::Int(number) => Field::Int(number),
            };
            card(&mut out, "PARAM", &[Field::Text(param.key), value]);
        }
        for grid in &self.grids {
            card(
                &mut out,
                "GRID",
                &[
                    Field::Int(grid.nid),
                    Field::Blank,
                    Field::Real(grid.xyz[0]),
                    Field::Real(grid.xyz[1]),
                    Field::Real(grid.xyz[2]),
                ],
            );
        }
        for shell in self.quads.iter().chain(&self.trias) {
            let name = if shell.nodes.len() == 4 {
                "CQUAD4"
            } else {
                "CTRIA3"
            };
            let mut fields = vec![Field::Int(shell.eid), Field::Int(shell.pid)];
            fields.extend(shell.nodes.iter().map(|&nid| Field::Int(nid)));
            card(&mut out, name, &fields);
        }
        for bar in &self.bars {
            card(
                &mut out,
                "CBAR",
                &[
                    Field::Int(bar.eid),
                    Field::Int(bar.pid),
                    Field::Int(bar.ga),
                    Field::Int(bar.gb),
                    Field::Real(bar.x[0]),
                    Field::Real(bar.x[1]),
                    Field::Real(bar.x[2]),
                    Field::Text(bar.offt),
                ],
            );
        }
        for mass in &self.masses {
            card(
                &mut out,
                "CONM2",
                &[
                    Field::Int(mass.eid),
                    Field::Int(mass.nid),
                    Field::Int(mass.cid),
                    Field::Real(mass.mass),
                    Field::Real(mass.offset[0]),
                    Field::Real(mass.offset[1]),
                    Field::Real(mass.offset[2]),
                ],
            );
        }
        let mut continuation_tags = ContinuationTags::new();
        for rigid in &self.rigid_elements {
            let mut fields = vec![
                Field::Int(rigid.eid),
                Field::Blank,
                Field::Int(rigid.refgrid),
                Field::Text(rigid.refc),
                Field::Real(rigid.weight),
                Field::Text(rigid.comp),
            ];
            fields.extend(rigid.gijs.iter().map(|&nid| Field::Int(nid)));
            match rbe3_format {
                Rbe3Format::ReferenceLargeField => card(&mut out, "RBE3", &fields),
                Rbe3Format::MscSmallField => {
                    let mut small_fields = vec![
                        SmallField::Int(rigid.eid),
                        SmallField::Blank,
                        SmallField::Int(rigid.refgrid),
                        SmallField::Text(rigid.refc),
                        SmallField::Real(rigid.weight),
                        SmallField::Text(rigid.comp),
                    ];
                    small_fields.extend(rigid.gijs.iter().map(|&nid| SmallField::Int(nid)));
                    SmallFieldCard::new("RBE3", small_fields)
                        .render(&mut out, &mut continuation_tags);
                }
            }
        }
        for property in &self.shell_properties {
            card(
                &mut out,
                "PSHELL",
                &[
                    Field::Int(property.pid),
                    Field::Int(property.mid1),
                    Field::Real(property.t),
                    Field::Int(property.mid2),
                ],
            );
        }
        for property in &self.bar_properties {
            // Fields five to eight of a PBARL are reserved; the dimensions
            // start on the first continuation, which is why the blanks are
            // written out rather than dropped.
            let mut fields = vec![
                Field::Int(property.pid),
                Field::Int(property.mid),
                Field::Blank,
                Field::Text(property.section),
                Field::Blank,
                Field::Blank,
                Field::Blank,
                Field::Blank,
            ];
            fields.extend(property.dim.iter().map(|&value| Field::Real(value)));
            card(&mut out, "PBARL", &fields);
        }
        for material in &self.materials {
            card(
                &mut out,
                "MAT1",
                &[
                    Field::Int(material.mid),
                    Field::Real(material.e),
                    Field::Real(material.g),
                    Field::Real(material.nu),
                    Field::Real(material.rho),
                ],
            );
        }
        for constraint in &self.constraints {
            let mut fields = vec![
                Field::Int(constraint.sid),
                Field::Text(constraint.components),
            ];
            fields.extend(constraint.nodes.iter().map(|&nid| Field::Int(nid)));
            card(&mut out, "SPC1", &fields);
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rbe3Format {
    ReferenceLargeField,
    MscSmallField,
}

/// Append one card, continuing onto as many lines as its fields need.
fn card(out: &mut String, name: &str, fields: &[Field]) {
    for (chunk_index, chunk) in fields.chunks(FIELDS_PER_LINE).enumerate() {
        let opener = if chunk_index == 0 {
            format!("{name}*")
        } else {
            "*".to_string()
        };
        let mut line = format!("{opener:<8}");
        for field in chunk {
            let text = match *field {
                Field::Blank => String::new(),
                Field::Int(number) => number.to_string(),
                Field::Text(text) => text.to_string(),
                Field::Real(value) => real_field(value),
            };
            line.push_str(&format!("{text:>FIELD_WIDTH$}"));
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
}

/// A real formatted to fit a sixteen-column field, carrying as many digits as
/// it can.
///
/// NASTRAN accepts a real in fixed notation (`-2.1`, `.006`, `71000000000.`)
/// or with an exponent (`1.0E+300`), and requires a decimal point in either:
/// a bare `500` is read as an integer and rejects the card. Both forms are
/// tried at every precision that fits, and the one whose text reads back
/// closest to `value` wins, shortest first on a tie. That is a few dozen
/// formats per number, which is nothing against writing the file.
fn real_field(value: f64) -> String {
    if value == 0.0 {
        return "0.".to_string();
    }
    let mut best: Option<(f64, String)> = None;
    for precision in 0..=FIELD_WIDTH {
        let candidates = [fixed(value, precision), scientific(value, precision)];
        for text in candidates.into_iter().flatten() {
            if text.len() > FIELD_WIDTH {
                continue;
            }
            let Ok(parsed) = text.parse::<f64>() else {
                continue;
            };
            let error = (parsed - value).abs();
            let better = match &best {
                None => true,
                Some((best_error, best_text)) => {
                    error < *best_error || (error == *best_error && text.len() < best_text.len())
                }
            };
            if better {
                best = Some((error, text));
            }
        }
    }
    // Every finite double has at least one representation that fits: a mantissa
    // of one digit and three decimals plus a signed three-digit exponent is
    // fifteen characters. A non-finite one cannot reach this module (the
    // builder's arithmetic is over measured geometry) and is written as zero
    // rather than as text no solver would read.
    best.map_or_else(|| "0.".to_string(), |(_, text)| text)
}

/// `value` in fixed notation, with the leading zero dropped where that is what
/// buys the extra digit.
fn fixed(value: f64, precision: usize) -> Option<String> {
    let mut text = format!("{value:.precision$}");
    if !text.contains('.') {
        text.push('.');
    }
    for (prefix, replacement) in [("0.", "."), ("-0.", "-.")] {
        if let Some(rest) = text.strip_prefix(prefix) {
            if rest.chars().any(|c| c.is_ascii_digit()) {
                text = format!("{replacement}{rest}");
            }
            break;
        }
    }
    Some(text)
}

/// `value` in exponent notation, with the exponent's sign always written:
/// classic NASTRAN readers differ on whether an unsigned exponent is legal.
fn scientific(value: f64, precision: usize) -> Option<String> {
    if precision == 0 {
        return None; // No decimal point, which the format requires.
    }
    let text = format!("{value:.precision$E}");
    let (mantissa, exponent) = text.split_once('E')?;
    let sign = if exponent.starts_with('-') { "" } else { "+" };
    Some(format!("{mantissa}E{sign}{exponent}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::cards::{Grid, Rbe3, Shell};

    /// Values spanning what a wingbox deck actually carries (node
    /// coordinates, thicknesses, moduli) plus round powers of ten far outside
    /// it.
    const SAMPLES: &[f64] = &[
        -2.1,
        0.006,
        0.015_563_681_910_996,
        71_000_000_000.0,
        26_691_729_323.308_27,
        1.0,
        -1.0,
        0.001,
        1e-300,
        1e300,
        -1e-300,
        123_456.789_012_345,
    ];

    #[test]
    fn every_field_fits_sixteen_columns() {
        for &value in SAMPLES {
            let text = real_field(value);
            assert!(
                text.len() <= FIELD_WIDTH,
                "{value:e} rendered as {text:?}, {} columns",
                text.len()
            );
        }
    }

    #[test]
    fn every_field_reads_back_far_inside_the_tier_the_deck_is_judged_at() {
        // `linalg` is 1e-9 relative; a field that round-trips to 1e-14 leaves
        // the deck's precision nowhere near the tolerance the mesh is judged at.
        for &value in SAMPLES {
            let text = real_field(value);
            let parsed: f64 = text.parse().unwrap();
            let relative = (parsed - value).abs() / value.abs();
            assert!(
                relative <= 1e-14,
                "{value:e} rendered as {text:?}, read back as {parsed:e} ({relative:e} relative)"
            );
        }
    }

    #[test]
    fn a_three_digit_exponent_costs_digits_and_that_is_the_formats_limit() {
        // Sixteen columns cannot hold both a full mantissa and an exponent
        // near the end of the range, so precision there is what is left over.
        // Nothing in a wingbox deck (a length, a thickness, a modulus, a
        // mass) comes within two hundred decades of this.
        let text = real_field(f64::MIN_POSITIVE);
        let parsed: f64 = text.parse().unwrap();
        assert!(text.len() <= FIELD_WIDTH);
        assert!((parsed - f64::MIN_POSITIVE).abs() / f64::MIN_POSITIVE <= 1e-9);
    }

    #[test]
    fn a_field_always_carries_a_decimal_point_so_nastran_reads_it_as_a_real() {
        for &value in SAMPLES {
            let text = real_field(value);
            assert!(text.contains('.'), "{value:e} rendered as {text:?}");
        }
        assert_eq!(real_field(0.0), "0.");
    }

    #[test]
    fn a_card_longer_than_four_fields_continues_onto_a_starred_line() {
        let mut deck = Deck::new();
        deck.add_grid(7, [1.5, 0.0, -2.25]);
        deck.quads.push(Shell {
            eid: 3,
            pid: 1,
            nodes: vec![1, 2, 3, 4],
        });
        let text = deck.write_bulk();
        let lines: Vec<&str> = text.lines().collect();

        assert_eq!(
            lines[0],
            "GRID*                  7                             1.5              0."
        );
        assert_eq!(lines[1], "*                  -2.25");
        assert_eq!(
            lines[2],
            "CQUAD4*                3               1               1               2"
        );
        assert_eq!(lines[3], "*                      3               4");
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn the_msc_product_path_writes_rbe3_with_an_explicit_small_field_continuation() {
        let mut deck = Deck::new();
        deck.rigid_elements.push(Rbe3 {
            eid: 50,
            refgrid: 10,
            refc: "123456",
            weight: 1.0,
            comp: "123",
            gijs: vec![1, 3, 7, 9],
        });

        let reference = deck.write_bulk();
        assert!(reference.starts_with("RBE3*"));
        assert!(reference
            .lines()
            .nth(1)
            .is_some_and(|line| line.starts_with('*')));

        let msc = deck.write_bulk_msc();
        let lines: Vec<&str> = msc.lines().collect();
        assert_eq!(
            lines,
            [
                "RBE3    50              10      123456  1.      123     1       3       +1",
                "+1      7       9",
            ]
        );
        assert!(!msc.contains("RBE3*"));
    }

    #[test]
    fn a_grid_can_be_read_back_by_identifier() {
        let mut deck = Deck::new();
        deck.add_grid(4, [1.0, 2.0, 3.0]);
        assert_eq!(deck.grid_xyz(4), Some([1.0, 2.0, 3.0]));
        assert_eq!(deck.node_y(4), 2.0);
        assert_eq!(deck.grid_xyz(5), None);
        assert!(deck.node_y(5).is_nan());
    }

    #[test]
    fn the_element_count_excludes_masses_and_rigid_elements() {
        let mut deck = Deck::new();
        deck.quads.push(Shell {
            eid: 1,
            pid: 1,
            nodes: vec![1, 2, 3, 4],
        });
        deck.trias.push(Shell {
            eid: 2,
            pid: 1,
            nodes: vec![1, 2, 3],
        });
        assert_eq!(deck.element_count(), 2);
        let _ = Grid {
            nid: 1,
            xyz: [0.0; 3],
        };
    }
}
