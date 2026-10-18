// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Geometric twist measured independently of uniform trimmed incidence.
use alas_geom::aircraft::airplane::Airplane;

pub(super) fn tip_washout_deg(plane: &Airplane) -> Option<f64> {
    let wing = plane.wings.first()?;
    let root = wing.xsecs.first()?;
    let tip = wing.xsecs.last()?;
    let washout = tip.twist - root.twist;
    washout.is_finite().then_some(washout)
}

/// The largest twist increase, tip-ward, between any two built sections of
/// the main wing, in degrees: the worst wash-in of any spanwise run.
///
/// Twist is the section incidence, positive nose-up, so a positive value means
/// some outboard section is set at a higher incidence than an inboard one.
/// Zero is returned for a wing whose twist never rises toward the tip. For the
/// root, break and tip stations this is `max(break - root, tip - break,
/// tip - root, 0)`, which is what makes an outboard panel that turns to
/// wash-in visible even when the root-to-tip difference is still washout.
/// Like [`tip_washout_deg`], it is unchanged by the uniform trim incidence.
///
/// With `stations_y_m` (the spanwise `y`, m, of the twist stations: root,
/// break, tip and any custom section) only the sections at those stations
/// count. The C2 twist profile is a natural cubic through the stations, which
/// overshoots between them (a flat outer panel, tip twist equal to the break
/// twist, bulges 0.26 deg above both ends); that bulge is a fairing of the
/// accepted loft, not a station set at a higher incidence than its neighbour,
/// and the design variable is the station value. The linear loft is a
/// polyline through the same stations, so its extremes are the stations too.
pub(super) fn max_washin_rise_deg(plane: &Airplane, stations_y_m: Option<&[f64]>) -> Option<f64> {
    let wing = plane.wings.first()?;
    let semispan = wing.xsecs.last()?.xyz_le[1].abs();
    let mut lowest_inboard = f64::INFINITY;
    let mut rise = 0.0_f64;
    for section in wing.xsecs.iter().filter(|section| {
        stations_y_m.is_none_or(|ys| {
            ys.iter()
                .any(|y| (y - section.xyz_le[1]).abs() <= 1.0e-6 * semispan.max(1.0))
        })
    }) {
        rise = rise.max(section.twist - lowest_inboard.min(section.twist));
        lowest_inboard = lowest_inboard.min(section.twist);
    }
    rise.is_finite().then_some(rise)
}
