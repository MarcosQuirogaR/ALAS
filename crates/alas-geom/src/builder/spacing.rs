// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Point spacing helpers shared by the builder.

use crate::aircraft::spacing::linspace;

/// `n_subdivisions` clamped to `usize`, so a negative or overflowing
/// configuration value becomes `0` rather than wrapping to a huge unsigned
/// value on the `as` cast.
///
/// Zero reaches [`Wing::mesh_spanwise`] as a request for fewer panels than
/// the surface has sections, which it answers with one panel per section:
/// the coarsest mesh that still carries every planform station. A nonsense
/// configuration therefore degrades to the coarsest honest mesh instead of
/// panicking or silently dropping the kink; `alas_config::validation` rejects
/// it at the configuration boundary, which is where a user can act on it.
pub(super) fn n_subdivisions_usize(n: i64) -> usize {
    usize::try_from(n).unwrap_or(0)
}

/// Sine-spaced points from `start` to `stop`, bunched near `start`:
/// `native aerodynamic model.numpy.spacing.sinspace` at its default `reverse_spacing =
/// False`: `start + (stop - start) * (1 - cos(linspace(0, pi/2, num)))`, with
/// both endpoints then forced exact to correct the trigonometric round trip,
/// exactly as upstream's own endpoint fixup does.
pub(super) fn sinspace(start: f64, stop: f64, num: usize) -> Vec<f64> {
    if num == 0 {
        return Vec::new();
    }
    let mut spaced: Vec<f64> = linspace(0.0, std::f64::consts::FRAC_PI_2, num)
        .into_iter()
        .map(|t| start + (stop - start) * (1.0 - t.cos()))
        .collect();
    spaced[0] = start;
    let last = spaced.len() - 1;
    spaced[last] = stop;
    spaced
}
