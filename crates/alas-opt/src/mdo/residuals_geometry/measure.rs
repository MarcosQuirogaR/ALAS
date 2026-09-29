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
