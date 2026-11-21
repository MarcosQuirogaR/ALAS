// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Wing sections digitised from printed drawings, for presets whose
//! manufacturers publish no section coordinates: the Boeing 747 stations
//! `B747-Y109`, `B747-Y206`, `B747-Y303`, `B747-Y400` and `B747-Y500` (2y/b
//! 0.109 to 0.50) and the Douglas DC-10 basic section `DC10-BASIC`.
//!
//! The coordinates are an embedded data file, `data/digitised_sections.txt`,
//! whose header states the source figures and the digitising method; it is
//! parsed once, on first use. Each section is 241 points in Selig order
//! (trailing edge, upper surface forward to the leading edge at the origin,
//! lower surface aft), closed at (1, 0).
//!
//! Lookup is exact-match, as in [`crate::airfoil_data`]. The sections have
//! no counterpart in the Python reference, so they stay out of that module's
//! registry, which its parity test pins to the reference.

use std::sync::OnceLock;

// Embedded at compile time, as `crate::selig` embeds its corpus.
const DIGITISED: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/data/digitised_sections.txt"
));

/// One section of the data file: its name and its `(x, z)` coordinates.
type Section = (&'static str, Vec<(f64, f64)>);

/// The sections of the data file, parsed once, in file order.
fn sections() -> &'static [Section] {
    static SECTIONS: OnceLock<Vec<Section>> = OnceLock::new();
    SECTIONS.get_or_init(|| parse(DIGITISED)).as_slice()
}

/// Parse the data-file format: `#` lines are comments, a line `@NAME` opens
/// a section, the line after it describes the section, and every further
/// line holds one `x z` pair. A line that is none of these is skipped; the
/// tests pin each section's point count.
fn parse(text: &'static str) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    let mut description_pending = false;
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('@') {
            sections.push((name.trim(), Vec::new()));
            description_pending = true;
            continue;
        }
        if description_pending {
            description_pending = false;
            continue;
        }
        let mut values = line.split_whitespace().map(str::parse::<f64>);
        if let (Some(Ok(x)), Some(Ok(z)), None, Some(section)) = (
            values.next(),
            values.next(),
            values.next(),
            sections.last_mut(),
        ) {
            section.1.push((x, z));
        }
    }
    sections
}

/// The coordinates of the digitised section `name` (exact match), in Selig
/// order, or `None` if no section has that name.
pub fn get(name: &str) -> Option<&'static [(f64, f64)]> {
    sections()
        .iter()
        .find(|(entry_name, _)| *entry_name == name)
        .map(|(_, coordinates)| coordinates.as_slice())
}

/// Every digitised section name, sorted.
pub fn names() -> &'static [&'static str] {
    static NAMES: OnceLock<Vec<&'static str>> = OnceLock::new();
    NAMES
        .get_or_init(|| {
            let mut names: Vec<&'static str> = sections().iter().map(|(name, _)| *name).collect();
            names.sort_unstable();
            names
        })
        .as_slice()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The digitised sections with the t/c printed beside each drawing.
    const PRINTED_TC: [(&str, f64); 6] = [
        ("B747-Y109", 0.135),
        ("B747-Y206", 0.104),
        ("B747-Y303", 0.089),
        ("B747-Y400", 0.080),
        ("B747-Y500", 0.080),
        ("DC10-BASIC", 0.104),
    ];

    #[test]
    fn the_sections_are_closed_selig_loops_with_their_printed_thickness() {
        for (name, printed) in PRINTED_TC {
            let coords = get(name).unwrap_or_else(|| panic!("{name} is registered"));
            assert_eq!(coords.len(), 241, "{name}: 121 stations per surface");
            assert_eq!(
                coords[0],
                (1.0, 0.0),
                "{name}: starts at the closed trailing edge"
            );
            assert_eq!(
                coords[120],
                (0.0, 0.0),
                "{name}: leading edge at the origin"
            );
            assert_eq!(
                coords[240],
                (1.0, 0.0),
                "{name}: ends at the closed trailing edge"
            );
            let mut t_max: f64 = 0.0;
            for (upper, lower) in coords[1..120].iter().zip(coords[121..240].iter().rev()) {
                assert_eq!(upper.0, lower.0, "{name}: surfaces share their stations");
                assert!(
                    upper.1 > lower.1,
                    "{name}: upper above lower at x {}",
                    upper.0
                );
                t_max = t_max.max(upper.1 - lower.1);
            }
            // The drawings resolve the thickness to a few per cent of itself:
            // the digitised t/c is within 4 % of the printed one.
            assert!(
                (t_max / printed - 1.0).abs() <= 0.04,
                "{name}: digitised t/c {t_max:.4} against printed {printed}"
            );
        }
    }

    #[test]
    fn names_lists_every_section_once_and_lookup_is_exact_case() {
        let mut expected: Vec<&str> = PRINTED_TC.iter().map(|&(name, _)| name).collect();
        expected.sort_unstable();
        assert_eq!(names(), expected.as_slice());
        assert!(get("b747-y109").is_none());
        assert!(get("SC2-0714").is_none());
    }
}
