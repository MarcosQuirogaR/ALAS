// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The cruise buffet-onset margin.
//!
//! A transport is operated so that, at its cruise Mach number, altitude and
//! mass, a load factor of at least 1.3 g is available before buffet onset.
//! CS 25.251(e) (EASA, *Certification Specifications for Large Aeroplanes*,
//! CS-25, "Vibration and buffeting") requires the buffet-onset boundaries to
//! be determined for the cruise configuration; the 0.3 g margin is the
//! operating practice that sets the maximum cruise altitude from those
//! boundaries (Airbus, *Getting to Grips with Aircraft Performance*, Airbus
//! Customer Services, 2002, maximum-altitude discussion). The 1.3 g figure is
//! therefore an operating criterion applied to the certified boundary, not a
//! number CS-25 itself prescribes.
//!
//! Without the margin the search can raise the cruise lift coefficient (less
//! wing area at a given span, i.e. a higher aspect ratio at a higher wing
//! loading) or thicken the wing at no cost until wave drag rises, while the
//! real aircraft would lose its cruise altitude to buffet first.

use alas_aero::analysis::AeroAnalysis;
use alas_config::{AlasConfig, ConstraintPolicy, DesignMode};
use alas_geom::aircraft::airplane::Airplane;

use super::sizing::SizingOutcome;
use super::types::ConstraintFamily::Performance;
use super::types::ConstraintResidual;

/// Load factor that must be reachable before buffet onset at the cruise
/// point, g: the 0.3 g operating margin (Airbus 2002; see the module doc).
pub(crate) const BUFFET_LOAD_FACTOR: f64 = 1.3;

/// Buffet-onset lift coefficient at Mach `mach` for a wing of quarter-chord
/// sweep `sweep_quarter_chord_deg` and representative thickness ratio
/// `thickness`, under Korn technology factor `kappa`.
///
/// The Korn relation (Korn, as presented in Mason, *Configuration
/// Aerodynamics*, Virginia Tech, transonic-aerodynamics chapter; Raymer,
/// *Aircraft Design: A Conceptual Approach*, Korn equation)
/// `M_dd = kappa/cos L - (t/c)/cos^2 L - CL/(10 cos^3 L)`, solved for the lift
/// coefficient at which the drag-divergence Mach equals `mach`:
///
/// `CL_b = 10 cos^3 L (kappa/cos L - (t/c)/cos^2 L - M)`.
///
/// Taking the drag-divergence boundary as the buffet-onset boundary is an
/// engineering estimate. On a transport wing shock-induced separation, and
/// with it buffet, sets in at or beyond drag divergence at a given Mach
/// (Obert, *Aerodynamic Design of Transport Aircraft*, IOS Press, 2009,
/// transonic drag-rise and buffet discussion). The estimate is not a buffet
/// boundary and is not conservative-biased: against the Fokker 100 flight-test
/// 1 g buffet-onset boundary (Obert, Fokker report, 1991, tabulated in
/// M. van Eijndhoven, MSc thesis, TU Delft, 2012, table 4.6) it is off by
/// about 0.3 to 0.65 in CL at M 0.70 to 0.75 depending on `kappa`, and its
/// Mach slope is about 2.3 times the measured one. It therefore acts only
/// through the reference-adaptation floor `min(1.3, nominal)`; the absolute
/// reading is diagnostic. It is the relation, sweep basis and thickness basis the
/// wave-drag build-up uses (`AeroAnalysis::wave_drag`), so the margin and
/// the cruise drag see one transonic model.
///
/// Negative when the section, sweep and Mach leave no attached-flow lift.
pub(crate) fn buffet_onset_cl(
    mach: f64,
    sweep_quarter_chord_deg: f64,
    thickness: f64,
    kappa: f64,
) -> f64 {
    let cos_sweep = sweep_quarter_chord_deg.to_radians().cos();
    10.0 * cos_sweep.powi(3) * (kappa / cos_sweep - thickness / cos_sweep.powi(2) - mach)
}

/// The wing quantities the buffet estimate reads: reference-trapezoid
/// quarter-chord sweep, deg; area-weighted thickness ratio; reference area,
/// m^2.
fn wing_basis(plane: &Airplane, fallback_sweep_deg: f64) -> Option<(f64, f64, f64)> {
    let wing = plane.wings.first()?;
    Some((
        AeroAnalysis::quarter_chord_sweep_deg(plane, fallback_sweep_deg),
        AeroAnalysis::area_weighted_thickness(wing),
        plane.s_ref,
    ))
}

/// [`wing_basis`] of the registered aircraft a reference adaptation
/// redesigns, or `None` in any other mode. Resolved once per complete
/// configuration ([`super::nominal_cache`]), since the configured geometry
/// scaffold shapes the reference wing.
pub(super) fn reference_wing_basis(config: &AlasConfig) -> Option<(f64, f64, f64)> {
    if config.optimizer.design_space.mode != DesignMode::ReferenceAdaptation {
        return None;
    }
    REFERENCE_WING_BASIS.get_or_resolve(config, || {
        let design = alas_config::presets::get(&config.preset)
            .ok()?
            .design_vector;
        let plane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), false)
            .ok()?;
        wing_basis(&plane, design.sweep_deg)
    })
}

/// The cache of [`reference_wing_basis`].
pub(super) static REFERENCE_WING_BASIS: super::nominal_cache::NominalCache<(f64, f64, f64)> =
    super::nominal_cache::NominalCache::new();

/// The `buffet_margin` residual: the load factor to buffet onset at the
/// cruise point, `n = CL_b / CL_cruise`, must reach the floor.
///
/// The cruise point is the design cruise Mach and altitude
/// (`requirements.cruise_mach`, `requirements.cruise_altitude_m`) at the
/// closed takeoff mass. The takeoff mass bounds the start-of-cruise mass from
/// above by the climb fuel, which the fuel plan does not separate, so the
/// check is conservative by that fraction. Sweep and thickness are the
/// reference-trapezoid quarter-chord sweep and area-weighted thickness
/// ratio, not the leading-edge sweep and root thickness, because the Korn
/// relation is written for a representative section of a swept wing.
///
/// # Floor
///
/// The floor is [`BUFFET_LOAD_FACTOR`]. In a reference adaptation it is the
/// lower of that and the load factor the registered wing reaches at the
/// candidate's own mass and cruise point: the Korn estimate is not calibrated
/// on buffet data, and on the registered wide-bodies it reads 0.94-1.20 g at
/// their declared cruise point, where the real aircraft hold the operating
/// margin. Comparing the model with itself cancels that bias, as the tank
/// capacity check does: a candidate may not carry less lift at buffet onset
/// than the registered wing it redesigns. The absolute 1.3 g reading is
/// always reported beside it as `buffet_margin_absolute`, never ranked.
///
/// Below the configured wave-drag onset Mach the relation is outside its
/// domain and low-speed buffet is a stall-margin question the field
/// performance family covers, so no residual is emitted.
pub(super) fn buffet_residuals(
    outcome: &SizingOutcome,
    config: &AlasConfig,
    policy: ConstraintPolicy,
) -> Vec<ConstraintResidual> {
    let req = &config.requirements;
    let mach = req.cruise_mach;
    if policy == ConstraintPolicy::Off || mach < config.drag_model.wave_drag_onset_mach {
        return Vec::new();
    }
    let kappa = config.geometry.wing.airfoil_class.korn_technology_factor();
    let Some((sweep_deg, thickness, s_ref)) =
        wing_basis(&outcome.plane, outcome.history.dv.sweep_deg)
    else {
        return Vec::new();
    };
    let atmosphere = alas_atmo::Atmosphere::new(req.cruise_altitude_m);
    let v_m_s = mach * atmosphere.speed_of_sound();
    let q_pa = 0.5 * atmosphere.density() * v_m_s.powi(2);
    let weight_n = outcome.sized.takeoff_mass_kg * req.gravity_m_s2;
    let load_factor = |sweep: f64, tc: f64, area: f64| {
        buffet_onset_cl(mach, sweep, tc, kappa) * q_pa * area / weight_n
    };
    let n = load_factor(sweep_deg, thickness, s_ref);
    let floor = reference_wing_basis(config).map_or(BUFFET_LOAD_FACTOR, |(sweep, tc, area)| {
        let reference = load_factor(sweep, tc, area);
        if reference.is_finite() {
            reference.min(BUFFET_LOAD_FACTOR)
        } else {
            BUFFET_LOAD_FACTOR
        }
    });
    vec![
        ConstraintResidual::scaled(
            "buffet_margin",
            Performance,
            n,
            floor,
            "g",
            floor - n,
            policy,
        ),
        ConstraintResidual::scaled(
            "buffet_margin_absolute",
            Performance,
            n,
            BUFFET_LOAD_FACTOR,
            "g",
            BUFFET_LOAD_FACTOR - n,
            ConstraintPolicy::Diagnostic,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::buffet_onset_cl;

    #[test]
    fn buffet_lift_falls_with_mach_and_thickness_and_rises_with_sweep() {
        let base = buffet_onset_cl(0.78, 25.0, 0.12, 0.95);
        assert!(base > 0.5 && base < 1.5, "{base}");
        assert!(buffet_onset_cl(0.80, 25.0, 0.12, 0.95) < base);
        assert!(buffet_onset_cl(0.78, 25.0, 0.13, 0.95) < base);
        assert!(buffet_onset_cl(0.78, 30.0, 0.12, 0.95) > base);
    }

    #[test]
    fn the_buffet_lift_is_the_korn_drag_divergence_inverse() {
        // At CL_b the Korn drag-divergence Mach equals the flight Mach.
        let (mach, sweep, tc, kappa) = (0.85, 32.0_f64, 0.11, 0.95);
        let cl = buffet_onset_cl(mach, sweep, tc, kappa);
        let c = sweep.to_radians().cos();
        let mach_dd = kappa / c - tc / c.powi(2) - cl / (10.0 * c.powi(3));
        assert!((mach_dd - mach).abs() < 1e-12);
    }
}
