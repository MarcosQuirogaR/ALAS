// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from native aerodynamic model/library/weights/torenbeek_weights.py
// Upstream: native aerodynamic model 4.2.8, MIT.
// Reference: alas @ rust-port-baseline.

//! Torenbeek's empirical wing and fuselage weight methods, from "Synthesis
//! of Subsonic Airplane Design" (1976, Delft University Press), Chapter 8
//! and Appendix C -- reached through native aerodynamic model's translation of them,
//! since `alas-mass::breakdown` (`alas/physics/mass.py`, a separate,
//! not-yet-ported module) calls exactly two of its functions: [`mass_wing`]
//! and [`mass_fuselage_simple`].
//!
//! # Scope
//!
//! Upstream's `torenbeek_weights.py` has seven public functions; this module
//! has two, plus [`mass_wing`]'s three private helpers. Left untranslated,
//! because a grep of the whole `alas/` package (not only `mass.py`) finds no
//! caller:
//!
//! - `mass_wing_simple`: a cruder wing weight model (Eq. 8-12), superseded
//!   at its only prospective call site by the Appendix C method this module
//!   implements.
//! - `mass_fuselage`: dead code upstream -- it raises `NotImplementedError`
//!   partway through, after referencing `S_g`, `W_str` and `W_fr`, none of
//!   which the function ever assigns. Not a `deviation-candidate`; there is
//!   no behaviour here to reproduce, faithfully or otherwise, since the
//!   function cannot run to completion.
//! - `mass_propeller`: unused by the translated wing/fuselage path. Turboprop
//!   propulsion mass is now owned by [`crate::propulsion_mass`], where engine,
//!   propeller and installation evidence remain explicit.
//!
//! `mass_wing`'s and `mass_wing_basic_structure`'s `return_dict: bool` is
//! narrowed to the `float`-returning path: `mass.py` never passes
//! `return_dict=True` at either of its call sites (checked directly against
//! the reference), and `Union[float, Dict[str, float]]` has no natural Rust
//! type. Every intermediate value `return_dict=True` would have exposed is
//! still a named local in the functions below, not folded into one
//! expression.

use alas_geom::aircraft::fuselage::Fuselage;
use alas_geom::aircraft::wing::Wing;

/// `mass_wing_basic_structure`'s `k_e` default -- Torenbeek's weight
/// knockdown for a wing with no wing-mounted engines forward of the elastic
/// axis (see that function's doc). [`mass_wing`] never overrides it, since
/// it does not expose `k_e` as a parameter of its own -- matching upstream,
/// whose `mass_wing` never passes `k_e` through either.
pub const DEFAULT_K_E: f64 = 0.95;

/// Evenly spaced points from `start` to `stop`, inclusive -- NumPy's
/// `linspace(start, stop, num, endpoint=True)`. Duplicated from
/// `alas_geom::aircraft::spacing::linspace`, which is private to that crate's
/// aircraft module and not reachable from here (see that module's other
/// callers, `alas-geom::airfoil_library` and `alas-geom::wing_structure`,
/// which duplicate it for the same reason).
fn linspace(start: f64, stop: f64, num: usize) -> Vec<f64> {
    if num == 0 {
        return Vec::new();
    }
    if num == 1 {
        return vec![start];
    }
    let step = (stop - start) / (num - 1) as f64;
    let mut values: Vec<f64> = (0..num).map(|i| start + i as f64 * step).collect();
    let last = values.len() - 1;
    values[last] = stop;
    values
}

/// The root cross-section's thickness-to-chord ratio -- every helper below
/// reads `wing.xsecs[0].airfoil.max_thickness()` at upstream's default
/// sample, `np.linspace(0, 1, 101)`.
fn root_thickness_to_chord(wing: &Wing) -> f64 {
    let sample = linspace(0.0, 1.0, 101);
    wing.xsecs[0].airfoil.max_thickness(&sample)
}

/// The cosine of an angle given in degrees -- `native aerodynamic model.numpy.cosd`.
fn cosd(degrees: f64) -> f64 {
    degrees.to_radians().cos()
}

/// The sine of an angle given in degrees -- `native aerodynamic model.numpy.sind`.
fn sind(degrees: f64) -> f64 {
    degrees.to_radians().sin()
}

/// The mass of a wing's high-lift devices (flaps only; leading-edge devices
/// are stubbed to zero upstream) -- `mass_wing_high_lift_devices`, Torenbeek
/// Eq. C-10.
///
/// `k_f1` and `k_f2` (upstream's flap-configuration factors) are hardcoded
/// to `1.0`, their upstream defaults: [`mass_wing`], this function's only
/// caller, never overrides them.
#[allow(dead_code)]
fn mass_wing_high_lift_devices(
    wing: &Wing,
    max_airspeed_for_flaps: f64,
    flap_deflection_angle: f64,
) -> f64 {
    mass_wing_high_lift_devices_with_area(
        wing,
        max_airspeed_for_flaps,
        flap_deflection_angle,
        wing.control_surface_area(),
    )
}

/// High-lift-device mass with the configured flap planform area.
///
/// `Wing` deliberately has no drawing/control-surface subgeometry, so its
/// legacy `control_surface_area()` method is always zero. Product mass
/// analysis supplies the area derived from `ControlSurfacesConfig` through
/// this seam instead of silently assigning zero high-lift mass.
fn mass_wing_high_lift_devices_with_area(
    wing: &Wing,
    max_airspeed_for_flaps: f64,
    flap_deflection_angle: f64,
    s_flaps: f64,
) -> f64 {
    let k_f1 = 1.0;
    let k_f2 = 1.0;

    // Torenbeek's structural correlations use the modeled wing's developed
    // span (the historical YZ/unfolded quantity), not the aircraft reference
    // span used for coefficient normalization.
    let span = wing.unfolded_span();
    let sweep_half_chord = wing.mean_sweep_angle(0.5);
    let span_structural = span / cosd(sweep_half_chord);
    let root_t_over_c = root_thickness_to_chord(wing);

    let k_f = k_f1 * k_f2;

    let mass_trailing_edge_flaps = s_flaps
        * (2.706
            * k_f
            * (s_flaps * span_structural).powf(3.0 / 16.0)
            * ((max_airspeed_for_flaps / 100.0).powi(2)
                * sind(flap_deflection_angle)
                * cosd(wing.mean_sweep_angle(1.0))
                / root_t_over_c)
                .powf(3.0 / 4.0));

    let mass_leading_edge_devices = 0.0;

    mass_trailing_edge_flaps + mass_leading_edge_devices
}

/// The mass of the wing's basic structure -- the cantilever spar box, skin
/// and ribs, without any movable surfaces -- `mass_wing_basic_structure`,
/// Torenbeek Appendix C.
///
/// `return_dict` is narrowed away; see the module doc. `strut_y_location` is
/// `None` at every call site `mass.py` reaches today, but stays a real
/// `Option<f64>` because Torenbeek Eq. C-5's branch on it is physically
/// meaningful for a strut-braced wing, not merely an unused parameter.
#[allow(clippy::too_many_arguments)] // mirrors upstream's own nine-parameter signature
fn mass_wing_basic_structure(
    wing: &Wing,
    design_mass_togw: f64,
    ultimate_load_factor: f64,
    suspended_mass: f64,
    never_exceed_airspeed: f64,
    main_gear_mounted_to_wing: bool,
    strut_y_location: Option<f64>,
    k_e: f64,
) -> f64 {
    // Preserve the translated Torenbeek structural-span convention here;
    // `reference_span()` belongs to aircraft-level aerodynamic references.
    let span = wing.unfolded_span();
    let sweep_half_chord = wing.mean_sweep_angle(0.5);
    let cos_sweep_half_chord = cosd(sweep_half_chord);
    let span_structural = span / cos_sweep_half_chord;
    let root_t_over_c = root_thickness_to_chord(wing);

    // Torenbeek Eq. C-2: penalties from skin joints, non-tapered skin,
    // minimum gauge.
    let k_no = 1.0 + (1.905 / span_structural).sqrt();

    // Torenbeek Eq. C-3: penalty from taper ratio.
    let k_lambda = (1.0 + wing.taper_ratio()).powf(0.4);

    let k_uc = if main_gear_mounted_to_wing { 1.0 } else { 0.95 };

    // Torenbeek Eq. C-4: weight excrescence for structural stiffness against
    // flutter.
    let k_st = 1.0
        + 9.06e-4
            * ((span * cosd(wing.mean_sweep_angle(0.0))).powi(3) / design_mass_togw)
            * (never_exceed_airspeed / 100.0 / root_t_over_c).powi(2)
            * cos_sweep_half_chord;

    // Torenbeek Eq. C-5: bending-moment relief from a strut.
    let k_b = match strut_y_location {
        None => 1.0,
        Some(y) => 1.0 - (y / (wing.unfolded_span() / 2.0)).powi(2),
    };

    4.58e-3
        * k_no
        * k_lambda
        * k_e
        * k_uc
        * k_st
        * (k_b * ultimate_load_factor * (0.8 * suspended_mass + 0.2 * design_mass_togw)).powf(0.55)
        * span.powf(1.675)
        * root_t_over_c.powf(-0.45)
        * cos_sweep_half_chord.powf(-1.325)
}

/// The mass of the wing's spoilers and speedbrakes --
/// `mass_wing_spoilers_and_speedbrakes`.
///
/// Upstream's signature takes a `wing` parameter that its body never reads:
/// the commented-out alternative, `np.softmax(12.2 * wing.area(), 0.015 *
/// mass_basic_wing)`, was replaced with a flat `0.015 * mass_basic_wing`
/// (upstream's own comment: the wetted-area figure "comes out too high"),
/// leaving the parameter dead. Reproduced faithfully, including the ignored
/// argument.
fn mass_wing_spoilers_and_speedbrakes(_wing: &Wing, mass_basic_wing: f64) -> f64 {
    0.015 * mass_basic_wing
}

/// The mass of a wing, according to Torenbeek's "Synthesis of Subsonic
/// Airplane Design", 1976, Appendix C: "Prediction of Wing Structural
/// Weight" -- `mass_wing`.
///
/// `return_dict` is narrowed away; see the module doc. `k_e`
/// (`mass_wing_basic_structure`'s engine-mounting knockdown) is fixed at
/// [`DEFAULT_K_E`], since `mass_wing` itself never exposes it as a
/// parameter, matching upstream.
///
/// # Arguments
///
/// - `design_mass_togw`: the aircraft's design takeoff gross weight, `kg`.
/// - `ultimate_load_factor`: 1.5x the limit load factor.
/// - `suspended_mass`: the mass suspended from the wing, `kg`.
/// - `never_exceed_airspeed`: `m/s`, used for the flutter stiffness penalty.
/// - `max_airspeed_for_flaps`: `m/s`, the flap placard speed.
/// - `flap_deflection_angle`: the maximum flap deflection, degrees.
#[allow(clippy::too_many_arguments)] // mirrors upstream's own signature
pub fn mass_wing(
    wing: &Wing,
    design_mass_togw: f64,
    ultimate_load_factor: f64,
    suspended_mass: f64,
    never_exceed_airspeed: f64,
    max_airspeed_for_flaps: f64,
    main_gear_mounted_to_wing: bool,
    flap_deflection_angle: f64,
    strut_y_location: Option<f64>,
) -> f64 {
    mass_wing_with_control_surface_area(
        wing,
        design_mass_togw,
        ultimate_load_factor,
        suspended_mass,
        never_exceed_airspeed,
        max_airspeed_for_flaps,
        main_gear_mounted_to_wing,
        flap_deflection_angle,
        strut_y_location,
        wing.control_surface_area(),
    )
}

/// Mass of a wing using an explicitly configured trailing-edge flap area.
///
/// The ordinary [`mass_wing`] entry point remains reference-compatible. This
/// product seam is used by the checked mass path, where control-surface
/// configuration is available and must contribute to the high-lift mass.
#[allow(clippy::too_many_arguments)]
pub fn mass_wing_with_control_surface_area(
    wing: &Wing,
    design_mass_togw: f64,
    ultimate_load_factor: f64,
    suspended_mass: f64,
    never_exceed_airspeed: f64,
    max_airspeed_for_flaps: f64,
    main_gear_mounted_to_wing: bool,
    flap_deflection_angle: f64,
    strut_y_location: Option<f64>,
    control_surface_area_m2: f64,
) -> f64 {
    let mass_high_lift_devices = mass_wing_high_lift_devices_with_area(
        wing,
        max_airspeed_for_flaps,
        flap_deflection_angle,
        control_surface_area_m2.max(0.0),
    );

    let mass_basic_wing = mass_wing_basic_structure(
        wing,
        design_mass_togw,
        ultimate_load_factor,
        suspended_mass,
        never_exceed_airspeed,
        main_gear_mounted_to_wing,
        strut_y_location,
        DEFAULT_K_E,
    );

    let mass_spoilers_speedbrakes = mass_wing_spoilers_and_speedbrakes(wing, mass_basic_wing);

    mass_basic_wing + 1.2 * (mass_high_lift_devices + mass_spoilers_speedbrakes)
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

/// native aerodynamic model's `numpy.softmax`, restricted to the `softness`-parameterized
/// path with two or more arguments -- the only way [`mass_fuselage_simple`],
/// this module's one caller, ever invokes it (`hardness` is never supplied
/// upstream, and its `n_specified_arguments` validation and the empty/
/// single-argument `ValueError` are accordingly not reproduced).
fn softmax(values: &[f64], softness: f64) -> f64 {
    let scaled: Vec<f64> = values.iter().map(|&v| v / softness).collect();
    let max = scaled.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let sum: f64 = scaled.iter().map(|&v| (v - max).max(-500.0).exp()).sum();
    (max + sum.ln()) * softness
}

/// The mass of the fuselage, using Torenbeek's simple version of the
/// calculation -- `mass_fuselage_simple`, Eq. 8-16.
///
/// `wing_to_tail_distance` is the distance from the wing's quarter-chord to
/// the tail's quarter-chord, `m`.
pub fn mass_fuselage_simple(
    fuselage: &Fuselage,
    never_exceed_airspeed: f64,
    wing_to_tail_distance: f64,
) -> f64 {
    let widths: Vec<f64> = fuselage.xsecs.iter().map(|xsec| xsec.width).collect();
    let heights: Vec<f64> = fuselage.xsecs.iter().map(|xsec| xsec.height).collect();

    let max_width = softmax(&widths, mean(&widths) * 0.01);
    let max_height = softmax(&heights, mean(&heights) * 0.01);

    0.23 * (never_exceed_airspeed * wing_to_tail_distance / (max_width + max_height)).sqrt()
        * fuselage.area_wetted().powf(1.2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::fuselage::FuselageXSec;
    use alas_geom::aircraft::wing::WingXSec;

    fn naca(name: &str) -> Airfoil {
        Airfoil::from_name(name).expect("valid 4-digit NACA name")
    }

    fn rectangular_wing() -> Wing {
        Wing::new(
            "Rectangular",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 3.0, 0.0, naca("naca2412")),
                WingXSec::new([0.0, 15.0, 0.0], 3.0, 0.0, naca("naca2412")),
            ],
            true,
        )
    }

    #[test]
    fn a_strut_reduces_the_basic_wing_mass_below_the_cantilever_case() {
        let wing = rectangular_wing();
        let cantilever = mass_wing_basic_structure(
            &wing,
            60_000.0,
            3.75,
            20_000.0,
            180.0,
            true,
            None,
            DEFAULT_K_E,
        );
        let strutted = mass_wing_basic_structure(
            &wing,
            60_000.0,
            3.75,
            20_000.0,
            180.0,
            true,
            Some(4.0),
            DEFAULT_K_E,
        );
        assert!(
            strutted < cantilever,
            "strutted={strutted}, cantilever={cantilever}"
        );
    }

    #[test]
    fn a_strut_at_the_root_leaves_the_basic_wing_mass_unchanged() {
        let wing = rectangular_wing();
        let cantilever = mass_wing_basic_structure(
            &wing,
            60_000.0,
            3.75,
            20_000.0,
            180.0,
            true,
            None,
            DEFAULT_K_E,
        );
        let strut_at_root = mass_wing_basic_structure(
            &wing,
            60_000.0,
            3.75,
            20_000.0,
            180.0,
            true,
            Some(0.0),
            DEFAULT_K_E,
        );
        assert!((strut_at_root - cantilever).abs() < 1e-9);
    }

    #[test]
    fn a_wing_mounted_main_gear_is_never_lighter_than_a_non_wing_mounted_one() {
        let wing = rectangular_wing();
        let wing_mounted = mass_wing_basic_structure(
            &wing,
            60_000.0,
            3.75,
            20_000.0,
            180.0,
            true,
            None,
            DEFAULT_K_E,
        );
        let not_wing_mounted = mass_wing_basic_structure(
            &wing,
            60_000.0,
            3.75,
            20_000.0,
            180.0,
            false,
            None,
            DEFAULT_K_E,
        );
        assert!(not_wing_mounted < wing_mounted);
    }

    #[test]
    fn control_surface_area_is_zero_so_high_lift_mass_is_zero() {
        // `Wing::control_surface_area` always returns 0.0 in this crate
        // (`alas-geom::aircraft::wing`'s module doc), so the trailing-edge flap
        // term's leading `S_flaps` factor zeroes the whole result.
        let wing = rectangular_wing();
        assert_eq!(
            mass_wing_high_lift_devices(&wing, 90.0, 30.0),
            0.0,
            "S_flaps is always zero in this crate"
        );
    }

    #[test]
    fn configured_flap_area_adds_high_lift_mass_to_the_product_wing() {
        let wing = rectangular_wing();
        let legacy = mass_wing(
            &wing, 60_000.0, 3.75, 20_000.0, 180.0, 90.0, false, 30.0, None,
        );
        let product = mass_wing_with_control_surface_area(
            &wing, 60_000.0, 3.75, 20_000.0, 180.0, 90.0, false, 30.0, None, 10.0,
        );
        assert!(product > legacy, "product={product}, legacy={legacy}");
    }

    #[test]
    fn spoilers_and_speedbrakes_mass_is_one_and_a_half_percent_of_the_basic_wing() {
        let wing = rectangular_wing();
        assert_eq!(mass_wing_spoilers_and_speedbrakes(&wing, 1000.0), 15.0);
    }

    #[test]
    fn mass_wing_totals_the_basic_structure_plus_scaled_movables() {
        let wing = rectangular_wing();
        let basic = mass_wing_basic_structure(
            &wing,
            60_000.0,
            3.75,
            20_000.0,
            180.0,
            false,
            None,
            DEFAULT_K_E,
        );
        let high_lift = mass_wing_high_lift_devices(&wing, 90.0, 30.0);
        let spoilers = mass_wing_spoilers_and_speedbrakes(&wing, basic);
        let expected = basic + 1.2 * (high_lift + spoilers);

        let total = mass_wing(
            &wing, 60_000.0, 3.75, 20_000.0, 180.0, 90.0, false, 30.0, None,
        );
        assert!((total - expected).abs() < 1e-9);
    }

    #[test]
    fn softmax_of_n_equal_values_is_that_value_plus_softness_times_ln_n() {
        // softmax(x, x, ..., x) [n times] = x + softness * ln(n): each
        // scaled argument sits exactly at the max, so the log-sum-exp term
        // is just log(n).
        let expected = 5.0 + 0.1 * 3.0_f64.ln();
        assert!((softmax(&[5.0, 5.0, 5.0], 0.1) - expected).abs() < 1e-9);
    }

    #[test]
    fn softmax_approaches_the_true_maximum_as_values_separate() {
        let soft = softmax(&[1.0, 100.0], 0.01);
        assert!((soft - 100.0).abs() < 1e-6, "soft={soft}");
    }

    #[test]
    fn mass_fuselage_simple_is_positive_for_a_representative_fuselage() {
        let fuselage = Fuselage::new(
            "Fuselage",
            vec![
                FuselageXSec::new([0.0, 0.0, 0.0], Some(0.5), None, None, 2.0)
                    .expect("radius alone is valid"),
                FuselageXSec::new([10.0, 0.0, 0.0], Some(2.0), None, None, 2.0)
                    .expect("radius alone is valid"),
                FuselageXSec::new([40.0, 0.0, 0.0], Some(2.0), None, None, 2.0)
                    .expect("radius alone is valid"),
                FuselageXSec::new([50.0, 0.0, 0.0], Some(0.3), None, None, 2.0)
                    .expect("radius alone is valid"),
            ],
        );
        let mass = mass_fuselage_simple(&fuselage, 180.0, 25.0);
        assert!(mass > 0.0, "mass={mass}");
    }
}

/// Parity coverage for `mass_wing`'s three private helpers, which
/// `tests/parity_torenbeek.rs` (an integration test, which cannot see
/// private items) cannot check directly -- only their composition through
/// `mass_wing` reaches it. Kept in-crate rather than making the helpers
/// `pub(crate)` and moving this to `tests/`, since nothing outside this
/// module needs to call them.
#[cfg(test)]
mod parity_helpers {
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::wing::WingXSec;
    use alas_testkit::{Comparison, Tier};
    use serde::Deserialize;

    /// The same three wings `golden/generators/gen_mass_torenbeek.py` builds,
    /// rebuilt from its docstring's literal values -- duplicated from
    /// `tests/parity_torenbeek.rs` because that file is a separate
    /// compilation unit with no path back into this one.
    fn build_main_wing() -> Wing {
        let root_z_m = -2.1;
        let break_z_m = -0.3;
        let tip_z_m = 2.5;
        let root_twist_deg = 4.0;
        let break_twist_deg = 2.0;
        let break_span_fraction = 0.35;
        let outboard_sweep_decrement_deg: f64 = 2.0;

        let span_m: f64 = 71.75;
        let root_chord_m = 16.50;
        let break_chord_m = 7.80;
        let tip_chord_m = 1.60;
        let sweep_deg: f64 = 34.00;
        let tip_twist_deg = 0.00;

        let semi_span = span_m / 2.0;
        let y_break = break_span_fraction * semi_span;
        let sweep_in = sweep_deg.to_radians();
        let sweep_out = (sweep_deg - outboard_sweep_decrement_deg).to_radians();
        let dx_break = y_break * sweep_in.tan();
        let dx_tip = dx_break + (semi_span - y_break) * sweep_out.tan();

        let root_section = Airfoil::from_name("naca4412").expect("naca4412 parses");
        let tip_airfoil = Airfoil::from_name("naca2410").expect("naca2410 parses");

        Wing::new(
            "Main Wing",
            vec![
                WingXSec::new(
                    [0.0, 0.0, root_z_m],
                    root_chord_m,
                    root_twist_deg,
                    root_section.clone(),
                ),
                WingXSec::new(
                    [dx_break, y_break, break_z_m],
                    break_chord_m,
                    break_twist_deg,
                    root_section,
                ),
                WingXSec::new(
                    [dx_tip, semi_span, tip_z_m],
                    tip_chord_m,
                    tip_twist_deg,
                    tip_airfoil,
                ),
            ],
            true,
        )
    }

    fn build_hstab() -> Wing {
        let tail_airfoil = Airfoil::from_name("naca0012").expect("naca0012 parses");
        Wing::new(
            "Horizontal Stabilizer",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 8.0, -2.0, tail_airfoil.clone()),
                WingXSec::new([7.5, 11.0, 1.0], 2.2, -2.0, tail_airfoil),
            ],
            true,
        )
    }

    fn build_vstab() -> Wing {
        let tail_airfoil = Airfoil::from_name("naca0012").expect("naca0012 parses");
        Wing::new(
            "Vertical Stabilizer",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 9.5, 0.0, tail_airfoil.clone()),
                WingXSec::new([9.0, 0.0, 9.8], 3.2, 0.0, tail_airfoil),
            ],
            false,
        )
    }

    fn wing_by_name(name: &str) -> Wing {
        match name {
            "main_wing" => build_main_wing(),
            "hstab" => build_hstab(),
            "vstab" => build_vstab(),
            other => panic!("fixture named an unexpected wing: {other}"),
        }
    }

    #[derive(Debug, Deserialize)]
    struct HighLiftCase {
        wing: String,
        max_airspeed_for_flaps: f64,
        flap_deflection_angle: f64,
        mass_high_lift_devices: f64,
    }

    #[derive(Debug, Deserialize)]
    struct BasicStructureCase {
        wing: String,
        #[serde(rename = "design_mass_TOGW")]
        design_mass_togw: f64,
        ultimate_load_factor: f64,
        suspended_mass: f64,
        never_exceed_airspeed: f64,
        main_gear_mounted_to_wing: bool,
        strut_y_location: Option<f64>,
        mass_wing_basic: f64,
    }

    #[derive(Debug, Deserialize)]
    struct SpoilersCase {
        wing: String,
        mass_basic_wing: f64,
        mass_spoilers_and_speedbrakes: f64,
    }

    #[derive(Debug, Deserialize)]
    struct Fixture {
        mass_wing_high_lift_devices: Vec<HighLiftCase>,
        mass_wing_basic_structure: Vec<BasicStructureCase>,
        mass_wing_spoilers_and_speedbrakes: Vec<SpoilersCase>,
    }

    #[test]
    fn mass_wing_high_lift_devices_matches_native_aerodynamic_model() {
        let fixture: Fixture = alas_testkit::load("mass", "torenbeek");

        let mut comparison = Comparison::new(
            "alas-mass::torenbeek::mass_wing_high_lift_devices",
            Tier::Closed,
        );
        for (index, case) in fixture.mass_wing_high_lift_devices.iter().enumerate() {
            let wing = wing_by_name(&case.wing);
            let actual = mass_wing_high_lift_devices(
                &wing,
                case.max_airspeed_for_flaps,
                case.flap_deflection_angle,
            );
            comparison.scalar(
                &format!("mass_wing_high_lift_devices[{index}] ({})", case.wing),
                actual,
                case.mass_high_lift_devices,
            );
        }
        comparison.finish();
    }

    #[test]
    fn mass_wing_basic_structure_matches_native_aerodynamic_model() {
        let fixture: Fixture = alas_testkit::load("mass", "torenbeek");

        let mut comparison = Comparison::new(
            "alas-mass::torenbeek::mass_wing_basic_structure",
            Tier::Closed,
        );
        for (index, case) in fixture.mass_wing_basic_structure.iter().enumerate() {
            let wing = wing_by_name(&case.wing);
            let actual = mass_wing_basic_structure(
                &wing,
                case.design_mass_togw,
                case.ultimate_load_factor,
                case.suspended_mass,
                case.never_exceed_airspeed,
                case.main_gear_mounted_to_wing,
                case.strut_y_location,
                DEFAULT_K_E,
            );
            comparison.scalar(
                &format!("mass_wing_basic_structure[{index}] ({})", case.wing),
                actual,
                case.mass_wing_basic,
            );
        }
        comparison.finish();
    }

    #[test]
    fn mass_wing_spoilers_and_speedbrakes_matches_native_aerodynamic_model() {
        let fixture: Fixture = alas_testkit::load("mass", "torenbeek");

        let mut comparison = Comparison::new(
            "alas-mass::torenbeek::mass_wing_spoilers_and_speedbrakes",
            Tier::Closed,
        );
        for (index, case) in fixture
            .mass_wing_spoilers_and_speedbrakes
            .iter()
            .enumerate()
        {
            let wing = wing_by_name(&case.wing);
            let actual = mass_wing_spoilers_and_speedbrakes(&wing, case.mass_basic_wing);
            comparison.scalar(
                &format!(
                    "mass_wing_spoilers_and_speedbrakes[{index}] ({})",
                    case.wing
                ),
                actual,
                case.mass_spoilers_and_speedbrakes,
            );
        }
        comparison.finish();
    }
}
