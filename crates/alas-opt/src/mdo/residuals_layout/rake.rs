// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The tip end of the dihedral measurement (see [`dihedral_tip_station`]).
//!
//! The high-wing dihedral floor of `dihedral_residuals` is -5 deg: Raymer
//! 6th ed., Table 4.2 (p. 125), subsonic swept wing, high wing: -5 to -2 deg
//! (anhedral, offsetting the large positive effective dihedral of a high
//! swept wing). The A400M (high, swept) is built at -4.72 deg; the former
//! -3.0 deg floor rested on the unswept ATR72 alone.

use alas_config::AlasConfig;
use alas_geom::aircraft::airplane::Airplane;

use super::{tip_station, WingStation};

/// Outboard end of the straight wing for the dihedral measurement: the
/// rake-start section when the preset carries a swept-back tip rake
/// (`tip_rake_start_span_fraction` of the semispan, `y` from the centreline),
/// else the tip section. The rake panel is a planform device of the leading
/// edge, not a dihedral of the wing proper (the A220's 46 deg raked tip rises
/// 1.2 m over 1.1 m of span, which read as 10.4 deg mean dihedral against
/// 6.5 deg to the rake start). Wingtip devices (winglets, sharklets, fences)
/// are separate surfaces after the main wing and never reach this station.
pub(super) fn dihedral_tip_station(plane: &Airplane, config: &AlasConfig) -> Option<WingStation> {
    let wing = &config.geometry.wing;
    let sections = &plane.wings.first()?.xsecs;
    let tip_y = sections.last()?.xyz_le[1];
    let raked = wing.tip_rake_tip_chord_m.is_some();
    let Some(fraction) = wing.tip_rake_start_span_fraction.filter(|_| raked) else {
        return tip_station(plane);
    };
    let rake_y = fraction * tip_y;
    let section = sections
        .iter()
        .rfind(|s| s.xyz_le[1] <= rake_y + 1.0e-6)
        .filter(|s| s.xyz_le[1] > 0.0)?;
    Some(WingStation {
        xyz_le: section.xyz_le,
        chord: section.chord,
    })
}
