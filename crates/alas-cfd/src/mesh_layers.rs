// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Geometric prism-stack arithmetic for the boundary-layer field.

use super::super::MAX_DERIVED_BOUNDARY_LAYERS;

pub(in crate::mesh) fn geometric_layer_sum(first: f64, ratio: f64, n_layers: u32) -> f64 {
    if n_layers == 0 {
        return 0.0;
    }
    first * (ratio.powi(n_layers as i32) - 1.0) / (ratio - 1.0)
}

/// Smallest layer count whose geometric sum reaches `target`.
///
/// Inverting the geometric sum analytically keeps this exact for the sizes in
/// use; the result is still confirmed by [`geometric_layer_sum`] so a rounding
/// step down cannot leave the stack short of `target`.  Non-finite or
/// non-growing inputs fall back to one layer rather than an unbounded count.
pub(in crate::mesh) fn layers_spanning_thickness(first: f64, ratio: f64, target: f64) -> u32 {
    if !first.is_finite() || first <= 0.0 || !target.is_finite() || target <= 0.0 {
        return 1;
    }
    if !ratio.is_finite() || ratio <= 1.0 {
        return ((target / first).ceil() as u32).clamp(1, MAX_DERIVED_BOUNDARY_LAYERS);
    }
    let exact = (1.0 + target * (ratio - 1.0) / first).ln() / ratio.ln();
    if !exact.is_finite() {
        return MAX_DERIVED_BOUNDARY_LAYERS;
    }
    let mut count = (exact.ceil() as i64).clamp(1, i64::from(MAX_DERIVED_BOUNDARY_LAYERS)) as u32;
    while count < MAX_DERIVED_BOUNDARY_LAYERS && geometric_layer_sum(first, ratio, count) < target {
        count += 1;
    }
    count
}

/// Smallest layer count whose outermost layer, `first * ratio^(n - 1)`, is
/// at least `size`.
pub(in crate::mesh) fn layers_reaching_size(first: f64, ratio: f64, size: f64) -> u32 {
    if !first.is_finite() || first <= 0.0 || !size.is_finite() || size <= first {
        return 1;
    }
    if !ratio.is_finite() || ratio <= 1.0 {
        return MAX_DERIVED_BOUNDARY_LAYERS;
    }
    let exact = 1.0 + (size / first).ln() / ratio.ln();
    let mut count = (exact.ceil() as i64).clamp(1, i64::from(MAX_DERIVED_BOUNDARY_LAYERS)) as u32;
    while count < MAX_DERIVED_BOUNDARY_LAYERS && first * ratio.powi(count as i32 - 1) < size {
        count += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stack_reaches_the_isotropic_size_without_overshooting_a_layer() {
        // Medium preset, NACA 0012 at Re 6e6 on a 1.738 m chord: 15.5 um
        // first layer, 1 % chord isotropic size.
        let (first, ratio, size) = (1.549_455e-5, 1.2, 0.017_38);
        let n = layers_reaching_size(first, ratio, size);
        assert_eq!(n, 40);
        assert!(first * ratio.powi(n as i32 - 1) >= size);
        assert!(first * ratio.powi(n as i32 - 2) < size);
        assert_eq!(layers_reaching_size(first, ratio, first * 0.5), 1);
    }
}
