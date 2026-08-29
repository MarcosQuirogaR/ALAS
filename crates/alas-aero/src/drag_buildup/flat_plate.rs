// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from
// mission analysis model/Methods/Aerodynamics/Common/Fidelity_Zero/Helper_Functions/
//   compressible_mixed_flat_plate.py and compressible_turbulent_flat_plate.py
// Upstream: mission analysis model 2.5.2, LGPL-2.1.
// Reference: alas @ rust-port-baseline.

//! Flat-plate skin friction, with the compressibility and Reynolds
//! corrections every parasite-drag component applies.
//!
//! Both functions are the Stanford AA241 course notes' correlation, valid for
//! Reynolds numbers between about 1e5 and 1e9. The wing uses the mixed
//! laminar/turbulent form, because a wing has a transition point; the
//! fuselage and the nacelles use the fully turbulent one.

/// What a flat-plate correlation reports.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkinFriction {
    /// The compressible skin-friction coefficient.
    pub cf: f64,
    /// The compressibility correction that produced it.
    pub k_comp: f64,
    /// The Reynolds-number correction that produced it.
    pub k_reyn: f64,
}

/// The compressibility and Reynolds corrections, which both forms share.
fn corrections(re: f64, mach: f64, temp_k: f64) -> (f64, f64) {
    let wall = temp_k * (1.0 + 0.178 * mach * mach);
    let reference = temp_k * (1.0 + 0.035 * mach * mach + 0.45 * (wall / temp_k - 1.0));
    let k_comp = temp_k / reference;

    let re_reference =
        re * (reference / temp_k).powf(1.5) * ((reference + 216.0) / (temp_k + 216.0));
    let k_reyn = (re / re_reference).powf(0.2);

    (k_comp, k_reyn)
}

/// Skin friction on a plate that transitions from laminar to turbulent at
/// `xt`, a fraction of the chord.
///
/// `xt` outside `[0, 1]` is not physical and upstream raises on it. Nothing
/// here may panic, and no caller can supply one -- `transition_x_upper` and
/// `transition_x_lower` are wing fields that `vehicle_builder.py` leaves at
/// their `0.0` default -- so the bound is left to that caller rather than
/// checked here, and the `xt == 0.0` degeneracy upstream guards against is
/// reproduced below.
pub fn compressible_mixed_flat_plate(re: f64, mach: f64, temp_k: f64, xt: f64) -> SkinFriction {
    // Upstream's `Rex[Rex==0.0] = 0.0001`. At `xt == 0` -- which is every
    // wing this program builds -- `Rex` is zero and the laminar terms below
    // would be a division by it; the substitute keeps them finite and they
    // are then multiplied by `xt`, which is zero, so nothing it contributes
    // survives. Reproduced rather than short-circuited, because `xeff`
    // *does* read it and does not vanish.
    let mut rex = re * xt;
    if rex == 0.0 {
        rex = 0.0001;
    }

    let momentum_thickness = 0.671 * xt / rex.sqrt();
    let x_effective = (27.78 * momentum_thickness * re.powf(0.2)).powf(1.25);
    let re_transition = re * (1.0 - xt + x_effective);

    let cf_turbulent = 0.455 / re_transition.log10().powf(2.58);
    let cf_laminar = 1.328 / rex.sqrt();

    let cf_start = if xt > 0.0 {
        0.455 / (re * x_effective).log10().powf(2.58)
    } else {
        0.0
    };

    let cf_incompressible =
        cf_laminar * xt + cf_turbulent * (1.0 - xt + x_effective) - cf_start * x_effective;

    let (k_comp, k_reyn) = corrections(re, mach, temp_k);

    SkinFriction {
        cf: cf_incompressible * k_comp * k_reyn,
        k_comp,
        k_reyn,
    }
}

/// Skin friction on a fully turbulent plate.
pub fn compressible_turbulent_flat_plate(re: f64, mach: f64, temp_k: f64) -> SkinFriction {
    let cf_incompressible = 0.455 / re.log10().powf(2.58);
    let (k_comp, k_reyn) = corrections(re, mach, temp_k);

    SkinFriction {
        cf: cf_incompressible * k_comp * k_reyn,
        k_comp,
        k_reyn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The bound the tests below read "agrees" as. Every quantity here is
    // closed-form f64 arithmetic, so this is the row's own tier.
    const CLOSED: f64 = 1e-12;

    fn close(actual: f64, expected: f64) -> bool {
        (actual - expected).abs() <= CLOSED * expected.abs()
    }

    #[test]
    fn both_corrections_are_the_identity_at_mach_zero() {
        // With no Mach number the reference temperature is the static one, so
        // neither correction can move the incompressible coefficient. This is
        // what makes the two families of `k_comp`/`k_reyn` values in the
        // fixture readable: any departure from 1 is compressibility.
        let mixed = compressible_mixed_flat_plate(1e7, 0.0, 216.0, 0.6);
        let turbulent = compressible_turbulent_flat_plate(1e7, 0.0, 216.0);

        assert!(close(mixed.k_comp, 1.0));
        assert!(close(mixed.k_reyn, 1.0));
        assert!(close(turbulent.k_comp, 1.0));
        assert!(close(turbulent.k_reyn, 1.0));
    }

    #[test]
    fn a_fully_turbulent_transition_point_still_differs_from_the_turbulent_form() {
        // At `xt = 1` the mixed form is laminar over the whole plate but
        // upstream keeps the `xeff` turbulent-restart terms, so it does not
        // reduce to the turbulent correlation. Stated because the names
        // invite the opposite assumption.
        let mixed = compressible_mixed_flat_plate(1e7, 0.5, 250.0, 1.0);
        let turbulent = compressible_turbulent_flat_plate(1e7, 0.5, 250.0);

        assert!(mixed.cf != turbulent.cf);
    }

    #[test]
    fn a_zero_transition_point_leaves_the_laminar_terms_out() {
        // The `Rex == 0` substitute keeps `cf_lam` finite, and `xt` then
        // zeroes its contribution. Every wing this program builds takes this
        // path, so a port that had propagated the division by zero instead
        // would produce NaN on the whole aircraft.
        let at_zero = compressible_mixed_flat_plate(6e7, 0.82, 216.77, 0.0);

        assert!(at_zero.cf.is_finite());
        assert!(at_zero.cf > 0.0);
    }

    #[test]
    fn skin_friction_falls_as_the_reynolds_number_rises() {
        let low = compressible_turbulent_flat_plate(1e6, 0.0, 288.15);
        let high = compressible_turbulent_flat_plate(1e8, 0.0, 288.15);

        assert!(high.cf < low.cf);
    }

    #[test]
    fn the_upstream_module_test_is_reproduced() {
        // `compressible_mixed_flat_plate.py`'s own `__main__`, which is the
        // only worked example upstream ships for it.
        let result = compressible_mixed_flat_plate(1e7, 2.0, 216.0, 0.6);

        assert!(result.cf.is_finite());
        // The reference temperature is above the static one, so both
        // corrections reduce the incompressible coefficient.
        assert!(result.k_comp < 1.0);
        assert!(result.k_reyn < 1.0);
    }
}
