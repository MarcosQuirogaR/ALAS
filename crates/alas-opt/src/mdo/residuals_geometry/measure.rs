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
pub(super) fn max_washin_rise_deg(plane: &Airplane) -> Option<f64> {
    let wing = plane.wings.first()?;
    let mut lowest_inboard = f64::INFINITY;
    let mut rise = 0.0_f64;
    for section in &wing.xsecs {
        rise = rise.max(section.twist - lowest_inboard.min(section.twist));
        lowest_inboard = lowest_inboard.min(section.twist);
    }
    rise.is_finite().then_some(rise)
}
