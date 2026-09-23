// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/structural_sizing.py
// Reference: alas @ rust-port-baseline.

//! Direct strength-based wingbox sizing.
//!
//! [`size_wingbox`] sizes the spar caps directly from strength: margin of
//! safety zero by construction at the root, the bending-critical station,
//! with no mass-target bisection, then applies the spar-cap taper law and the
//! geometric cap width/height limits, sizes the webs from root shear, fixes
//! the skin at its configured minimum, and derives the rib spacing from a
//! panel-buckling criterion.
//!
//! Loads come from [`crate::loads`]: an elliptic aerodynamic distribution less
//! the inertia of the mass the wing carries itself, which is the wing-bending
//! design case (see [`crate::loads::WingInertiaRelief`] for why the no-relief
//! form is a different aircraft rather than a conservative version of this
//! one). Moment and shear are split across the spars weighted by each spar's
//! local section depth, so a deeper spar carries proportionally more of the
//! bending moment and a partial-span spar, zeroed outboard of the break,
//! carries none of it there.
//!
//! The relieved mass is the sized box itself plus the fuel in the integral
//! tanks the box encloses ([`crate::tanks`]). The box relieves its own loads,
//! so the product law solves that fixed point rather than reading a declared
//! wing mass: [`RELIEF_PASSES`] passes, each sizing against the previous pass's
//! running mass. It is a contraction - more relief gives a lighter box, which
//! gives less relief - and converges to well inside a kilogram.
//!
//! [`size_wingbox_reference_compatibility`] keeps the frozen no-relief,
//! whole-chord law the parity fixtures pin.
//!
//! None of these entry points says what loading envelope it sized to.
//! [`size_wingbox_with_scope`] returns the same box with a
//! [`crate::scope::SizingScope`] beside it: whether the relieving fuel was
//! bounded by a declared zero-fuel limit or assumed, which wing-carried items
//! could not be resolved, that no gust case was evaluated, and whether the
//! relieved-load fixed point settled. A consumer that publishes a box, a margin
//! or a solved deck should take that form.

use alas_config::materials::MaterialSpec;
use alas_config::{DesignRequirements, StructuresConfig};
use alas_geom::wing_structure::WingStructureGeometry;

use crate::loads::WingInertiaRelief;
use crate::tanks;

mod law;
mod scoped;
mod solve;
mod types;

pub use scoped::{size_wingbox_with_scope, SizedWingbox, WingFuelRelief};
pub use types::{
    margin_is_structurally_non_negative, CompositeProxyDeclaration, ControllingMargin,
    MassBreakdown, SparSizing, WingboxSizing, MARGIN_NUMERICAL_ZERO,
};

pub(crate) use law::{gradient_unit, trapezoid};
use law::{linspace, SizingLaw};
use solve::size_wingbox_with_law;

/// How many relieved-load passes the product law takes.
///
/// The box relieves its own bending, so its mass appears on both sides of the
/// sizing equation. The iteration is a strong contraction - the structure is
/// under a tenth of the relieved mass and the moment responds to it linearly -
/// so the total settles to inside a milligramme within the budget on every
/// registered aircraft.
///
/// **Two of them do not reach `RELIEF_TOLERANCE` inside it.** Measured at
/// their own nominal designs, the A340-300 is still moving by `4.10e-9` of its
/// box mass at the eighth pass and the A380-800 by `4.87e-9`, against a `1e-9`
/// relative tolerance - `6.1e-5 kg` and `1.3e-4 kg` in absolute terms, which is
/// structurally nothing and is why this was not visible before. Neither
/// constant is tuned to cover it: [`size_wingbox_with_scope`] reports the
/// verdict as [`crate::scope::ReliefConvergence`], so a box that did not settle
/// is published as one that did not settle rather than assumed to have.
pub const RELIEF_PASSES: usize = 8;

/// Relative change in total box mass below which the relieved-load fixed point
/// is taken as converged.
const RELIEF_TOLERANCE: f64 = 1.0e-9;

/// Size the wingbox directly from strength: `size_wingbox`.
///
/// The load cases come from [`crate::loads::load_cases`], net of the inertia of
/// the mass the wing carries itself ([`crate::loads::WingInertiaRelief`]): the
/// sized box and the fuel in the integral tanks it encloses. The box is sized
/// to whichever case produces the larger relieved root bending moment, and
/// every station is left with a non-negative strength margin wherever the
/// section geometry admits one.
///
/// Wing-mounted engines, pylons and gear are **not** relieved here: this entry
/// point is not given the powerplant or gear configuration, so their relief is
/// omitted and the box comes out heavier than it would with it. That is a
/// recorded conservatism, not an approximation of zero. A caller that holds
/// the aircraft's powerplant layout, or its manufacturer-published wing-tank
/// capacity, should use [`size_wingbox_with_wing_carried_mass`] instead.
///
/// See [`size_wingbox_reference_compatibility`] for the frozen law.
#[allow(clippy::too_many_arguments)] // mirrors upstream's own signature
pub fn size_wingbox(
    wsg: &WingStructureGeometry,
    cfg: &StructuresConfig,
    req: &DesignRequirements,
    skin_mat: &MaterialSpec,
    web_mat: &MaterialSpec,
    cap_mat: &MaterialSpec,
    rib_mat: &MaterialSpec,
) -> WingboxSizing {
    size_wingbox_with_wing_carried_mass(
        wsg,
        cfg,
        req,
        skin_mat,
        web_mat,
        cap_mat,
        rib_mat,
        None,
        &[],
    )
}

/// [`size_wingbox`] with the wing-carried masses the default entry point
/// cannot see.
///
/// `integral_fuel_kg_m` replaces the geometric tank estimate of
/// [`crate::tanks`] with the caller's own running fuel mass, kg/m, on the
/// sizing station grid: an aircraft whose manufacturer publishes a usable
/// wing-tank capacity should distribute that capacity rather than accept a
/// volume estimate that lands anywhere between two thirds and four thirds of
/// it across the registered fleet. A length other than `cfg.spanwise_stations`
/// is ignored in favour of the geometric estimate rather than silently
/// truncated.
///
/// `wing_mounted_point_masses` are `(spanwise station m, mass kg)` for the
/// modelled positive-`y` semi-wing only - the engines, pylons, nacelles and
/// wing-mounted gear legs. Passing the full aircraft's symmetric pair would
/// relieve one semi-wing with both of them;
/// [`crate::loads::engine_point_loads_n`] already applies that filter.
#[allow(clippy::too_many_arguments)] // mirrors upstream's own signature
pub fn size_wingbox_with_wing_carried_mass(
    wsg: &WingStructureGeometry,
    cfg: &StructuresConfig,
    req: &DesignRequirements,
    skin_mat: &MaterialSpec,
    web_mat: &MaterialSpec,
    cap_mat: &MaterialSpec,
    rib_mat: &MaterialSpec,
    integral_fuel_kg_m: Option<&[f64]>,
    wing_mounted_point_masses: &[(f64, f64)],
) -> WingboxSizing {
    let n = cfg.spanwise_stations.max(0) as usize;
    let y = linspace(0.0, wsg.semi_span, n);
    let (front, rear) = box_chord_band(wsg);
    let fuel = match integral_fuel_kg_m {
        Some(declared) if declared.len() == n => declared.to_vec(),
        _ => tanks::integral_fuel_running_mass_kg_m(wsg, &y, front, rear),
    };

    scoped::solve_relieved(
        wsg,
        cfg,
        req,
        skin_mat,
        web_mat,
        cap_mat,
        rib_mat,
        &fuel,
        wing_mounted_point_masses,
        front,
        rear,
    )
    .0
}

/// The frozen reference form of [`size_wingbox`], for the parity fixtures.
///
/// The reference sizes the root cap only, with its thickness clipped at a
/// fifth of the spar height, and tapers the outboard caps regardless of the
/// local moment, so a shallow spar with a low-allowable alloy at a high root
/// moment comes out under strength; the fixtures pin that behaviour. It also
/// carries no inertia relief and charges skin and rib area over the whole
/// chord rather than over the box. Product callers use [`size_wingbox`], whose
/// result the mass reconciliation gate accepts only with non-negative margins.
#[allow(clippy::too_many_arguments)] // mirrors upstream's own signature
pub fn size_wingbox_reference_compatibility(
    wsg: &WingStructureGeometry,
    cfg: &StructuresConfig,
    req: &DesignRequirements,
    skin_mat: &MaterialSpec,
    web_mat: &MaterialSpec,
    cap_mat: &MaterialSpec,
    rib_mat: &MaterialSpec,
) -> WingboxSizing {
    size_wingbox_with_law(
        wsg,
        cfg,
        req,
        skin_mat,
        web_mat,
        cap_mat,
        rib_mat,
        SizingLaw::Frozen,
        &WingInertiaRelief::default(),
    )
}

/// The spanwise station grid the sizing integrals run on.
///
/// A caller assembling a running mass for
/// [`size_wingbox_with_wing_carried_mass`] has to sample it on exactly the grid
/// the sizer will use, or the length check there rejects it and silently falls
/// back to the geometric estimate. This is that grid, so the two cannot be
/// derived independently and drift.
pub fn sizing_stations(wsg: &WingStructureGeometry, cfg: &StructuresConfig) -> Vec<f64> {
    linspace(0.0, wsg.semi_span, cfg.spanwise_stations.max(0) as usize)
}

/// The chordwise band the structural box occupies: the outermost two spars.
pub fn box_chord_band(wsg: &WingStructureGeometry) -> (f64, f64) {
    let front = wsg.spar_fracs.iter().copied().fold(f64::INFINITY, f64::min);
    let rear = wsg
        .spar_fracs
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    (front, rear)
}

/// Running mass of a sized box, kg/m, at its own stations.
///
/// The four terms are the per-station integrands of
/// [`WingboxSizing::mass_breakdown_kg`], so the mass this reports and the mass
/// the sizing integrates are one quantity rather than two statements of it.
/// The ribs are discrete and are conserved as a uniform spanwise density, the
/// same convention [`crate::analytical`] uses for inertial relief and for the
/// Rayleigh modal denominator.
pub fn box_running_mass_kg_m(
    sizing: &WingboxSizing,
    skin_mat: &MaterialSpec,
    web_mat: &MaterialSpec,
    cap_mat: &MaterialSpec,
    front: f64,
    rear: f64,
) -> Vec<f64> {
    let n = sizing.chord.len();
    let skin_width = (rear - front).max(0.0);
    let mut running: Vec<f64> = (0..n)
        .map(|j| 2.0 * skin_width * sizing.chord[j] * sizing.t_skin * skin_mat.rho_kg_m3)
        .collect();
    for spar in &sizing.spars {
        for (mass, (&h, &a)) in running.iter_mut().zip(spar.h.iter().zip(&spar.a_cap)) {
            *mass += spar.t_web * h * web_mat.rho_kg_m3 + 2.0 * a * cap_mat.rho_kg_m3;
        }
    }
    let span = match (sizing.y_stations.first(), sizing.y_stations.last()) {
        (Some(&first), Some(&last)) => last - first,
        _ => 0.0,
    };
    if span > 0.0 && sizing.mass_breakdown_kg.ribs.is_finite() {
        let rib_density = sizing.mass_breakdown_kg.ribs / span;
        for mass in &mut running {
            *mass += rib_density;
        }
    }
    running
}

#[cfg(test)]
mod tests {
    use super::law::{
        cap_taper, gradient_unit, linspace, rib_count_from_max_spacing, root_cap_dimensions,
        trapezoid,
    };
    use super::*;

    #[test]
    fn a_root_cap_that_does_not_fit_its_height_clip_widens_instead_of_falling_short() {
        // Fits: the flange stays at 0.6 h and carries the area.
        let (w, t) = root_cap_dimensions(0.01, 6.0, 0.5);
        assert!((w - 0.3).abs() < 1e-12 && (w * t - 0.01).abs() < 1e-12);
        // Does not fit at 0.6 h: the thickness clips at 0.2 h and the flange
        // widens until the area is carried.
        let (w, t) = root_cap_dimensions(0.05, 6.0, 0.5);
        assert!((t - 0.1).abs() < 1e-12);
        assert!((w - 0.5).abs() < 1e-12 && (w * t - 0.05).abs() < 1e-12);
        // Past the half-chord bound the area cannot be carried: reported short.
        let (w, t) = root_cap_dimensions(0.5, 6.0, 0.5);
        assert!((w - 3.0).abs() < 1e-12 && w * t < 0.5);
    }

    #[test]
    fn linspace_pins_both_endpoints_and_spaces_evenly() {
        let v = linspace(0.0, 1.0, 5);
        assert_eq!(v, vec![0.0, 0.25, 0.5, 0.75, 1.0]);
        assert_eq!(linspace(2.0, 3.0, 1), vec![2.0]);
        assert!(linspace(0.0, 1.0, 0).is_empty());
    }

    #[test]
    fn gradient_unit_is_the_constant_spacing_for_a_uniform_grid() {
        // NumPy's gradient at unit spacing on a uniform ramp is the step at
        // every station, endpoints included.
        let g = gradient_unit(&[0.0, 2.0, 4.0, 6.0]);
        assert_eq!(g, vec![2.0, 2.0, 2.0, 2.0]);
        // Fewer than two points has no derivative to take.
        assert_eq!(gradient_unit(&[5.0]), vec![0.0]);
    }

    #[test]
    fn trapezoid_integrates_a_line_to_its_exact_area() {
        // Area under y = x from 0 to 1 is 1/2, exact for the trapezoidal rule
        // on a straight line at any sampling.
        let x = vec![0.0, 0.25, 0.5, 0.75, 1.0];
        let y = x.clone();
        assert!((trapezoid(&y, &x) - 0.5).abs() < 1e-15);
    }

    #[test]
    fn automatic_rib_count_uses_ceiling_panels_and_includes_both_end_ribs() {
        assert_eq!(rib_count_from_max_spacing(5.0, 2.0), 4);
        assert_eq!(rib_count_from_max_spacing(6.0, 2.0), 4);
    }

    #[test]
    fn cap_taper_is_flat_inboard_and_reaches_the_tip_fraction_at_the_tip() {
        let taper = cap_taper(&[0.0, 0.5, 1.0], 0.5, 0.3);
        assert_eq!(taper[0], 1.0);
        assert_eq!(taper[1], 1.0);
        assert!((taper[2] - 0.3).abs() < 1e-12);
    }

    #[test]
    fn installed_rib_spacing_uses_the_selected_count_not_the_allowable_limit() {
        let mut sizing = WingboxSizing {
            y_stations: vec![0.0, 35.875],
            num_ribs: 25,
            rib_spacing_m: 0.96738451494986,
            ..test_sizing()
        };
        assert!((sizing.installed_rib_spacing_m() - 35.875 / 24.0).abs() < 1e-12);
        assert_ne!(sizing.installed_rib_spacing_m(), sizing.rib_spacing_m);
        assert!(!sizing.rib_spacing_pass());
        sizing.num_ribs = 39;
        assert!(sizing.rib_spacing_pass());
    }

    #[test]
    fn installed_rib_spacing_uses_the_station_span_when_the_grid_has_a_datum_offset() {
        let sizing = WingboxSizing {
            y_stations: vec![4.0, 14.0],
            num_ribs: 6,
            rib_spacing_m: 2.0,
            ..test_sizing()
        };
        assert!((sizing.installed_rib_spacing_m() - 2.0).abs() < 1e-12);
    }

    #[test]
    fn negative_or_non_finite_strength_margins_do_not_pass() {
        let mut sizing = test_sizing();
        sizing.spars = vec![SparSizing {
            chord_fraction: 0.25,
            h: vec![1.0],
            w_cap: vec![1.0],
            t_cap: vec![1.0],
            a_cap: vec![1.0],
            t_web: 0.1,
            frac_moment: vec![1.0],
            margin_of_safety: vec![-0.1],
        }];
        assert!(!sizing.strength_margins_pass());
        assert_eq!(sizing.minimum_margin_of_safety(), -0.1);
        sizing.spars[0].margin_of_safety[0] = f64::NAN;
        assert!(!sizing.strength_margins_pass());
        assert!(sizing.minimum_margin_of_safety().is_nan());
    }

    #[test]
    fn controlling_margin_locates_the_smallest_finite_margin_across_spars_and_stations() {
        let mut sizing = test_sizing();
        sizing.spars = vec![
            SparSizing {
                chord_fraction: 0.25,
                h: vec![1.0, 1.0],
                w_cap: vec![1.0, 1.0],
                t_cap: vec![1.0, 1.0],
                a_cap: vec![1.0, 1.0],
                t_web: 0.1,
                frac_moment: vec![1.0, 1.0],
                margin_of_safety: vec![0.5, 1.0],
            },
            SparSizing {
                chord_fraction: 0.75,
                h: vec![1.0, 1.0],
                w_cap: vec![1.0, 1.0],
                t_cap: vec![1.0, 1.0],
                a_cap: vec![1.0, 1.0],
                t_web: 0.1,
                frac_moment: vec![1.0, 1.0],
                margin_of_safety: vec![-4.2e-7, 2.0],
            },
        ];
        let controlling = sizing.controlling_margin().expect("spar stations present");
        assert_eq!(controlling.margin, -4.2e-7);
        assert_eq!(controlling.spar_index, 1);
        assert_eq!(controlling.station_index, 0);
        assert!((controlling.chord_fraction - 0.75).abs() < 1e-12);
        assert_eq!(controlling.y_m, 0.0);
        assert_eq!(controlling.eta, 0.0);
        // The raw value survives at full precision; this is exactly what a
        // `{:.6}`-rounded display collapses to the ambiguous "-0.000000".
        assert_ne!(controlling.margin, 0.0);
        assert_eq!(sizing.minimum_margin_of_safety(), controlling.margin);
    }

    #[test]
    fn controlling_margin_prefers_a_nan_station_over_any_finite_margin() {
        let mut sizing = test_sizing();
        sizing.spars = vec![
            SparSizing {
                chord_fraction: 0.25,
                h: vec![1.0, 1.0],
                w_cap: vec![1.0, 1.0],
                t_cap: vec![1.0, 1.0],
                a_cap: vec![1.0, 1.0],
                t_web: 0.1,
                frac_moment: vec![1.0, 1.0],
                margin_of_safety: vec![-1.0, f64::NAN],
            },
            SparSizing {
                chord_fraction: 0.75,
                h: vec![1.0],
                w_cap: vec![1.0],
                t_cap: vec![1.0],
                a_cap: vec![1.0],
                t_web: 0.1,
                frac_moment: vec![1.0],
                margin_of_safety: vec![-5.0],
            },
        ];
        let controlling = sizing.controlling_margin().expect("spar stations present");
        assert!(controlling.margin.is_nan());
        assert_eq!(controlling.spar_index, 0);
        assert_eq!(controlling.station_index, 1);
        assert!(sizing.minimum_margin_of_safety().is_nan());
    }

    #[test]
    fn controlling_margin_is_none_when_there_are_no_spar_stations() {
        let sizing = test_sizing();
        assert!(sizing.spars.is_empty());
        assert!(sizing.controlling_margin().is_none());
        assert!(sizing.minimum_margin_of_safety().is_nan());
    }

    #[test]
    fn composite_declaration_structure_and_uncalibrated_uncertainty_contract() {
        let decl = CompositeProxyDeclaration {
            source: "Open source gap...",
            applicability: "Effective isotropic proxy...",
            relative_uncertainty: None,
        };
        assert!(decl.relative_uncertainty.is_none());
        assert!(decl.applicability.contains("Effective isotropic proxy"));
    }

    // --- The relieved, box-extent product law -------------------------------

    /// A transport-sized wing with a two-spar box, enough to exercise the
    /// product law against the frozen one on the same geometry.
    fn probe_case() -> (
        WingStructureGeometry,
        StructuresConfig,
        DesignRequirements,
        &'static MaterialSpec,
    ) {
        use alas_config::{DesignVector, WingConfig};
        use alas_geom::airfoil_library::AirfoilLibrary;
        let section = AirfoilLibrary::get("naca2412").expect("the reference section resolves");
        let wsg = WingStructureGeometry::new(
            &DesignVector::default(),
            &WingConfig::default(),
            &section,
            &section,
            &[0.25, 0.70],
            None,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let cfg = StructuresConfig {
            spanwise_stations: 120,
            ..StructuresConfig::default()
        };
        let requirements = DesignRequirements::default();
        let aluminium =
            alas_config::materials::get("Al 7075-T6").unwrap_or_else(|error| panic!("{error}"));
        (wsg, cfg, requirements, aluminium)
    }

    #[test]
    fn the_zero_margin_noise_floor_accepts_round_off_and_rejects_a_real_deficit() {
        // The two values a fully stressed box actually produces, and the one
        // and a half units in the last place between them.
        assert!(margin_is_structurally_non_negative(-f64::EPSILON));
        assert!(margin_is_structurally_non_negative(-1.5 * f64::EPSILON));
        assert!(margin_is_structurally_non_negative(-0.5 * f64::EPSILON));
        assert!(margin_is_structurally_non_negative(0.0));
        assert!(margin_is_structurally_non_negative(1.0));
        // The band is exactly four units in the last place and stops there.
        assert!(margin_is_structurally_non_negative(-MARGIN_NUMERICAL_ZERO));
        assert!(!margin_is_structurally_non_negative(
            -MARGIN_NUMERICAL_ZERO * 1.001
        ));
        // Anything a reader would call a small deficit still fails, by nine
        // orders of magnitude or more.
        for deficit in [-1.0e-15, -1.0e-12, -1.0e-9, -1.0e-3, -0.14] {
            assert!(
                !margin_is_structurally_non_negative(deficit),
                "{deficit} must not be accepted"
            );
        }
        // An incomplete calculation is never a pass.
        assert!(!margin_is_structurally_non_negative(f64::NAN));
        // The band is four units in the last place of unity, nothing else.
        assert!((MARGIN_NUMERICAL_ZERO - 8.881_784_197_001_252e-16).abs() < 1.0e-30);
    }

    #[test]
    fn a_fully_stressed_box_passes_its_own_strength_gate() {
        // Every station of a fully stressed box sits on the zero-margin
        // boundary, so a predicate written as `margin >= 0.0` rejects a
        // correctly sized wing on the sign of a rounding error. That is what
        // was making the pipeline report an infeasible wingbox and skip
        // NASTRAN and Patran on aircraft whose structure is exactly as
        // designed.
        let (wsg, cfg, req, al) = probe_case();
        let sized = size_wingbox(&wsg, &cfg, &req, al, al, al, al);
        let minimum = sized.minimum_margin_of_safety();
        assert!(minimum < 0.0, "expected the boundary, got {minimum}");
        assert!(
            minimum >= -MARGIN_NUMERICAL_ZERO,
            "round-off exceeded its derived band: {minimum}"
        );
        assert!(sized.strength_margins_pass());
        assert!(sized.controlling_margin_is_numerical_zero());
        // The exact predicate the band replaced would have rejected it.
        let exact_predicate = sized
            .spars
            .iter()
            .flat_map(|spar| spar.margin_of_safety.iter())
            .all(|&margin| !margin.is_nan() && margin >= 0.0);
        assert!(!exact_predicate, "the regression this band exists for");
    }

    #[test]
    fn a_genuinely_under_strength_station_still_fails_and_is_not_called_numerical() {
        let mut sized = test_sizing();
        sized.spars = vec![SparSizing {
            chord_fraction: 0.25,
            h: vec![1.0],
            w_cap: vec![1.0],
            t_cap: vec![1.0],
            a_cap: vec![1.0],
            t_web: 0.1,
            frac_moment: vec![1.0],
            margin_of_safety: vec![-0.14],
        }];
        assert!(!sized.strength_margins_pass());
        assert!(!sized.controlling_margin_is_numerical_zero());
    }

    #[test]
    fn the_relieved_product_box_is_lighter_than_the_frozen_unrelieved_one() {
        let (wsg, cfg, req, al) = probe_case();
        let product = size_wingbox(&wsg, &cfg, &req, al, al, al, al);
        let frozen = size_wingbox_reference_compatibility(&wsg, &cfg, &req, al, al, al, al);
        assert!(
            product.total_mass_kg < frozen.total_mass_kg,
            "product {} kg vs frozen {} kg",
            product.total_mass_kg,
            frozen.total_mass_kg
        );
        // Every station still carries its own load: the relieved box is not
        // simply a scaled-down version left under strength. The root is sized
        // to a margin of exactly zero, so the tolerance here is the same
        // round-off allowance the mass reconciliation gate applies at that
        // boundary, not a weakened acceptance.
        let minimum = product.minimum_margin_of_safety();
        assert!(
            !minimum.is_nan() && minimum >= -1.0e-10,
            "minimum margin {minimum}"
        );
    }

    #[test]
    fn the_product_box_charges_skin_and_ribs_over_the_box_not_the_whole_chord() {
        let (wsg, cfg, req, al) = probe_case();
        let product = size_wingbox(&wsg, &cfg, &req, al, al, al, al);
        let frozen = size_wingbox_reference_compatibility(&wsg, &cfg, &req, al, al, al, al);
        // The spars sit at 0.25 and 0.70, so the cover is 45 % of the chord
        // the frozen law charges. Skin is linear in that width.
        let ratio = product.mass_breakdown_kg.skin / frozen.mass_breakdown_kg.skin;
        assert!((ratio - 0.45).abs() < 1e-9, "skin ratio {ratio}");
        // Ribs are the section area between the spars rather than the whole
        // aerofoil, which is a smaller share but not a fixed one.
        assert!(
            product.mass_breakdown_kg.ribs < frozen.mass_breakdown_kg.ribs,
            "product ribs {} vs frozen {}",
            product.mass_breakdown_kg.ribs,
            frozen.mass_breakdown_kg.ribs
        );
    }

    #[test]
    fn declared_wing_carried_mass_relieves_more_than_the_geometric_estimate_alone() {
        let (wsg, cfg, req, al) = probe_case();
        let default = size_wingbox(&wsg, &cfg, &req, al, al, al, al);
        // A wing-mounted engine at a third of the semi-span relieves the root.
        let engine = [(wsg.semi_span / 3.0, 6_000.0)];
        let with_engine =
            size_wingbox_with_wing_carried_mass(&wsg, &cfg, &req, al, al, al, al, None, &engine);
        assert!(
            with_engine.total_mass_kg < default.total_mass_kg,
            "with engine {} kg vs without {} kg",
            with_engine.total_mass_kg,
            default.total_mass_kg
        );
        // A declared fuel distribution of the wrong length falls back to the
        // geometric estimate rather than silently disabling the relief.
        let short = vec![0.0; 3];
        let fallback = size_wingbox_with_wing_carried_mass(
            &wsg,
            &cfg,
            &req,
            al,
            al,
            al,
            al,
            Some(&short),
            &[],
        );
        assert!((fallback.total_mass_kg - default.total_mass_kg).abs() < 1e-9);
        // A declared dry wing is heavier than one carrying its own fuel.
        let dry = vec![0.0; cfg.spanwise_stations as usize];
        let dry_wing =
            size_wingbox_with_wing_carried_mass(&wsg, &cfg, &req, al, al, al, al, Some(&dry), &[]);
        assert!(dry_wing.total_mass_kg > default.total_mass_kg);
    }

    #[test]
    fn the_relieved_load_fixed_point_has_settled_by_the_time_it_is_reported() {
        // The box relieves its own bending, so the reported result must be a
        // fixed point of that map: resizing it against its own running mass
        // must return the same box.
        let (wsg, cfg, req, al) = probe_case();
        let sized = size_wingbox(&wsg, &cfg, &req, al, al, al, al);
        let (front, rear) = box_chord_band(&wsg);
        let fuel = tanks::integral_fuel_running_mass_kg_m(&wsg, &sized.y_stations, front, rear);
        let structure = box_running_mass_kg_m(&sized, al, al, al, front, rear);
        let relief = WingInertiaRelief {
            running_mass_kg_m: fuel.iter().zip(&structure).map(|(&f, &s)| f + s).collect(),
            point_masses_kg: Vec::new(),
        };
        let resized = size_wingbox_with_law(
            &wsg,
            &cfg,
            &req,
            al,
            al,
            al,
            al,
            SizingLaw::Product,
            &relief,
        );
        let drift = (resized.total_mass_kg - sized.total_mass_kg).abs() / sized.total_mass_kg;
        assert!(drift < 1.0e-6, "relief fixed point drifted by {drift}");
    }

    #[test]
    fn an_outboard_cap_is_floored_at_the_minimum_gauge_rather_than_at_the_root_taper() {
        let (wsg, cfg, req, al) = probe_case();
        let product = size_wingbox(&wsg, &cfg, &req, al, al, al, al);
        let frozen = size_wingbox_reference_compatibility(&wsg, &cfg, &req, al, al, al, al);
        let tip = product.y_stations.len() - 1;
        for spar in &product.spars {
            assert!(
                spar.t_cap[tip] >= cfg.t_skin_min_m - 1e-12
                    || spar.t_cap[tip] >= spar.h[tip] / 3.0 - 1e-12,
                "tip cap {} m is below the minimum gauge",
                spar.t_cap[tip]
            );
        }
        // The frozen law leaves a fifth of the root flange at the tip, which
        // is what makes its caps the heavier of the two.
        let product_caps = product.mass_breakdown_kg.spar_caps;
        let frozen_caps = frozen.mass_breakdown_kg.spar_caps;
        assert!(
            product_caps < frozen_caps,
            "product caps {product_caps} kg vs frozen {frozen_caps} kg"
        );
    }

    fn test_sizing() -> WingboxSizing {
        WingboxSizing {
            y_stations: vec![0.0, 35.875],
            eta_stations: vec![0.0, 1.0],
            chord: vec![1.0, 1.0],
            spar_fracs: vec![0.25, 0.75],
            spars: Vec::new(),
            t_skin: 0.01,
            num_ribs: 2,
            rib_spacing_m: 1.0,
            mass_breakdown_kg: MassBreakdown {
                spar_caps: 0.0,
                spar_webs: 0.0,
                skin: 0.0,
                ribs: 0.0,
            },
            total_mass_kg: 0.0,
            sizing_load_case: "test",
            composite_declaration: None,
        }
    }
}
