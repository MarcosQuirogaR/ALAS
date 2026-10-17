// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Tailplane/fin split: which wings are written, and in what pose.

use super::super::*;

/// Tolerance for the geometric tests in the tailplane/fin split (metres,
/// radians-free: lengths and dimensionless twist in degrees).
const SPLIT_TOLERANCE: f64 = 1.0e-9;

/// Share of the first tailplane section's span left out of each tailplane
/// half at the fin. One fifth is one spanwise panel row: OpenVSP's default
/// discretization puts five panels in a section (0.317 m rows on A400M).
const TAILPLANE_FIN_CLEARANCE_FRACTION: f64 = 0.2;

/// Largest mismatch between an airfoil's upper and mirrored lower surface,
/// in chord units, for it to count as symmetric.
const SYMMETRIC_AIRFOIL_TOLERANCE: f64 = 1.0e-4;

/// Where a written wing sits. A left tailplane half is the right half turned
/// 180 degrees about the fuselage x axis.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::openvsp) enum WingPose {
    Standard,
    RightHalf,
    LeftHalf,
}

/// A wing as it is written to the script. `index` names the OpenVSP geometry.
#[derive(Clone)]
pub(in crate::openvsp) struct ExportedWing {
    pub(in crate::openvsp) index: usize,
    pub(in crate::openvsp) wing: Wing,
    pub(in crate::openvsp) pose: WingPose,
}

/// Every wing as it is exported. A symmetric tailplane that crosses a fin
/// is written as two half-wings instead (see [`split_tailplane_at_fin`]).
pub(in crate::openvsp) fn exported_wings(airplane: &Airplane) -> Vec<ExportedWing> {
    let mut exported = Vec::with_capacity(airplane.wings.len());
    for (index, wing) in airplane.wings.iter().enumerate() {
        match split_tailplane_at_fin(airplane, wing) {
            Some([right, left]) => {
                exported.push(ExportedWing {
                    index,
                    wing: right,
                    pose: WingPose::RightHalf,
                });
                exported.push(ExportedWing {
                    index: airplane.wings.len() + index,
                    wing: left,
                    pose: WingPose::LeftHalf,
                });
            }
            None => exported.push(ExportedWing {
                index,
                wing: wing.clone(),
                pose: WingPose::Standard,
            }),
        }
    }
    exported
}

/// Split a symmetric horizontal tailplane whose root passes through a fin.
///
/// OpenVSP's thin-surface mesh trims the two lifting surfaces along their
/// intersection. Where the mirrored tailplane passes through the fin plane
/// (y = 0) the trim leaves non-quadrilateral cells on both surfaces, and
/// VSPAERO refuses the mesh. The tailplane root is therefore cut back to a
/// clearance from the fin plane, and the two halves are exported as separate
/// wings. The left half is the right half turned 180 degrees about the x
/// axis; that reproduces the mirror image only for an unswept-dihedral,
/// untwisted tailplane with a symmetric airfoil, which is checked here. Any
/// other wing returns `None` and is exported unchanged.
///
/// The planform outside the clearance is unchanged; the cut removes one
/// spanwise panel row of the tailplane inside the fin's own footprint.
fn split_tailplane_at_fin(airplane: &Airplane, wing: &Wing) -> Option<[Wing; 2]> {
    if !wing.symmetric || wing.xsecs.len() < 2 {
        return None;
    }
    let root = &wing.xsecs[0];
    let outside = &wing.xsecs[1];
    if root.xyz_le[1].abs() >= SPLIT_TOLERANCE {
        return None;
    }
    let first_span = outside.xyz_le[1] - root.xyz_le[1];
    if first_span.is_nan() || first_span <= 0.0 || !fin_crosses_root(airplane, root) {
        return None;
    }
    let mirror_exact = wing.xsecs.iter().all(|section| {
        (section.xyz_le[2] - root.xyz_le[2]).abs() < SPLIT_TOLERANCE
            && is_symmetric_airfoil(&section.airfoil.coordinates)
    });
    if !mirror_exact {
        return None;
    }
    let clearance = TAILPLANE_FIN_CLEARANCE_FRACTION * first_span;
    let right = clip_root(wing, clearance);
    let mut left = right.clone();
    left.name = format!("{} (left half)", wing.name);
    for section in &mut left.xsecs {
        section.xyz_le[1] = -section.xyz_le[1];
        section.twist = -section.twist;
    }
    Some([right, left])
}

/// Whether a vertical fin (a non-symmetric wing lying in the y = 0 plane)
/// occupies the point where the tailplane root chord meets the fin plane.
fn fin_crosses_root(airplane: &Airplane, root: &WingXSec) -> bool {
    let z = root.xyz_le[2];
    let x_lo = root.xyz_le[0];
    let x_hi = x_lo + root.chord;
    airplane.wings.iter().any(|fin| {
        if fin.symmetric
            || fin.xsecs.len() < 2
            || fin
                .xsecs
                .iter()
                .any(|s| s.xyz_le[1].abs() >= SPLIT_TOLERANCE)
        {
            return false;
        }
        let mut sections: Vec<&WingXSec> = fin.xsecs.iter().collect();
        sections.sort_by(|a, b| a.xyz_le[2].total_cmp(&b.xyz_le[2]));
        sections.windows(2).any(|pair| {
            let (a, b) = (pair[0], pair[1]);
            let (za, zb) = (a.xyz_le[2], b.xyz_le[2]);
            if !(za <= z && z <= zb) || zb - za <= SPLIT_TOLERANCE {
                return false;
            }
            let t = (z - za) / (zb - za);
            let x_le = a.xyz_le[0] + t * (b.xyz_le[0] - a.xyz_le[0]);
            let chord = a.chord + t * (b.chord - a.chord);
            x_le < x_hi && x_le + chord > x_lo
        })
    })
}

/// The wing with its root moved outboard to `clearance` along its span. The
/// new root keeps the planform of the original section pair it lies on.
fn clip_root(wing: &Wing, clearance: f64) -> Wing {
    let inside = &wing.xsecs[0];
    let outside = &wing.xsecs[1];
    let fraction = clearance / (outside.xyz_le[1] - inside.xyz_le[1]);
    let lerp = |a: f64, b: f64| a + (b - a) * fraction;
    let root = WingXSec::new(
        [
            lerp(inside.xyz_le[0], outside.xyz_le[0]),
            inside.xyz_le[1] + clearance,
            lerp(inside.xyz_le[2], outside.xyz_le[2]),
        ],
        lerp(inside.chord, outside.chord),
        lerp(inside.twist, outside.twist),
        inside.airfoil.clone(),
    );
    let mut xsecs = vec![root];
    xsecs.extend(wing.xsecs[1..].iter().cloned());
    Wing {
        name: wing.name.clone(),
        xsecs,
        symmetric: false,
        reference: wing.reference,
    }
}

/// Whether the mean line of an airfoil given as one closed `(x, y)` loop
/// (upper surface from the trailing edge forward, then lower surface aft
/// again) is straight, to [`SYMMETRIC_AIRFOIL_TOLERANCE`] of chord.
fn is_symmetric_airfoil(coordinates: &[(f64, f64)]) -> bool {
    if coordinates.len() < 3 {
        return false;
    }
    let leading_edge = coordinates
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.0.total_cmp(&b.0))
        .map_or(0, |(index, _)| index);
    let upper = &coordinates[..=leading_edge];
    let lower = &coordinates[leading_edge..];
    upper.iter().all(|&(x, y)| {
        lower
            .windows(2)
            .find(|pair| (pair[0].0 - x) * (pair[1].0 - x) <= 0.0 && pair[0].0 != pair[1].0)
            .is_some_and(|pair| {
                let t = (x - pair[0].0) / (pair[1].0 - pair[0].0);
                let y_lower = pair[0].1 + t * (pair[1].1 - pair[0].1);
                (y + y_lower).abs() <= SYMMETRIC_AIRFOIL_TOLERANCE
            })
    })
}
