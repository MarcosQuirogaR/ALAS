// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::f64::consts::PI;

use alas_atmo::Atmosphere;
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::{Wing, WingXSec};

use super::trefftz_induced_drag_coefficient;
use crate::operating_point::OperatingPoint;
use crate::vlm;

fn airplane(wing: Wing, span: f64, area: f64) -> Airplane {
    Airplane {
        name: "trefftz probe".to_owned(),
        xyz_ref: [0.0; 3],
        wings: vec![wing],
        fuselages: vec![],
        s_ref: area,
        c_ref: area / span,
        b_ref: span,
    }
}

/// A planar, untwisted, unswept-quarter-chord elliptic wing: chord
/// `c0 sqrt(1 - eta^2)` at `stations` cosine-spaced sections per semispan,
/// the tip section closed to a thousandth of the root chord. Returns the
/// airplane and its aspect ratio.
fn elliptic(stations: usize) -> (Airplane, f64) {
    let foil = Airfoil::from_name("naca0012").unwrap();
    let span: f64 = 30.0;
    let root_chord: f64 = 3.0;
    let xsecs = (0..=stations)
        .map(|k| {
            let eta = (0.5 * PI * k as f64 / stations as f64).sin();
            let chord = (root_chord * (1.0 - eta * eta).max(0.0).sqrt()).max(1.0e-3 * root_chord);
            let x_le = 0.25 * (root_chord - chord);
            WingXSec::new([x_le, 0.5 * span * eta, 0.0], chord, 0.0, foil.clone())
        })
        .collect();
    let wing = Wing::new("Main Wing", xsecs, true);
    // Exact elliptic area; the closed tip changes it by about 1e-6.
    let area = PI * span * root_chord / 4.0;
    (airplane(wing, span, area), span * span / area)
}

/// A planar, untwisted, straight-tapered wing of aspect ratio 8 with the
/// given section, leading-edge sweep and taper ratio, `stations` sections
/// per semispan.
fn tapered(airfoil: &str, sweep_deg: f64, taper: f64, stations: usize) -> (Airplane, f64) {
    dihedralled(airfoil, sweep_deg, taper, stations, 0.0, 0.0)
}

/// [`tapered`] with every section raised at `dihedral_deg` and set at the
/// same incidence `incidence_deg` (no twist).
fn dihedralled(
    airfoil: &str,
    sweep_deg: f64,
    taper: f64,
    stations: usize,
    dihedral_deg: f64,
    incidence_deg: f64,
) -> (Airplane, f64) {
    let foil = Airfoil::from_name(airfoil).unwrap();
    let span: f64 = 40.0;
    let aspect_ratio = 8.0;
    let area = span * span / aspect_ratio;
    let root_chord = 2.0 * area / (span * (1.0 + taper));
    let xsecs = (0..=stations)
        .map(|k| {
            let eta = k as f64 / stations as f64;
            let y = 0.5 * span * eta;
            let chord = root_chord * (1.0 - (1.0 - taper) * eta);
            WingXSec::new(
                [
                    y * sweep_deg.to_radians().tan(),
                    y,
                    y * dihedral_deg.to_radians().tan(),
                ],
                chord,
                incidence_deg,
                foil.clone(),
            )
        })
        .collect();
    let wing = Wing::new("Main Wing", xsecs, true);
    (airplane(wing, span, area), aspect_ratio)
}

fn op(alpha_deg: f64) -> OperatingPoint {
    OperatingPoint::new(Atmosphere::new(0.0), 50.0, alpha_deg, 0.0, 0.0, 0.0, 0.0)
}

/// `(CL near field, CDi Trefftz, CL linear circulation)` of one solve.
fn solve(plane: &Airplane, alpha_deg: f64, chordwise: usize) -> (f64, f64, f64) {
    let op_point = op(alpha_deg);
    let result = vlm::run(plane, &op_point, 1, chordwise).unwrap();
    let cdi = trefftz_induced_drag_coefficient(&result, &op_point, plane.s_ref).unwrap();
    let mut start = 0;
    let mut integrated = 0.0;
    for (index, panel) in result.panels.iter().enumerate() {
        if panel.is_trailing_edge {
            let first = &result.panels[start];
            let width = (first.front_right[1] - first.front_left[1]).abs();
            integrated += result.vortex_strengths[start..=index].iter().sum::<f64>() * width;
            start = index + 1;
        }
    }
    let cl_linear = 2.0 * integrated / (50.0 * plane.s_ref);
    (result.cl_lift, cdi, cl_linear)
}

fn span_efficiency(cl: f64, cdi: f64, aspect_ratio: f64) -> f64 {
    cl * cl / (PI * aspect_ratio * cdi)
}

/// Lifting-line theory gives the elliptic wing a span efficiency of exactly
/// one (Prandtl; Anderson, *Fundamentals of Aerodynamics*, 6th ed., 2017,
/// section 5.3.1). The lattice approaches it as its strips are refined: the
/// error at 48 sections per semispan is below a third of that at 12, and
/// below half a percent. Convergence uses the linear circulation lift of the
/// same Trefftz idealization; the near-field force adds nonlinear projection
/// terms of order alpha^2. Both efficiencies retain the planar bound.
#[test]
fn elliptic_wing_converges_to_the_lifting_line_span_efficiency() {
    let errors: Vec<f64> = [12, 24, 48]
        .into_iter()
        .map(|stations| {
            let (plane, aspect_ratio) = elliptic(stations);
            let (cl, cdi, linear_cl) = solve(&plane, 4.0, 4);
            assert!(cl > 0.2, "{stations} sections: CL {cl}");
            let efficiency = span_efficiency(cl, cdi, aspect_ratio);
            assert!(
                efficiency <= 1.0,
                "elliptic {stations} sections: e {efficiency}"
            );
            (span_efficiency(linear_cl, cdi, aspect_ratio) - 1.0).abs()
        })
        .collect();
    assert!(errors[2] < errors[0] / 3.0, "{errors:?}");
    assert!(errors[2] < 5.0e-3, "{errors:?}");
}

/// Munk's bound (NACA Report 121, 1923): no loading of a planar wing has a
/// smaller induced drag than the elliptic one, so `e <= 1`, and the drag is
/// never negative. The cambered, swept cases are where the near-field force
/// sum fails; the far field holds the bound at every angle, including close
/// to zero lift where cambered sections still carry a spanwise loading.
#[test]
fn planar_untwisted_wings_keep_munks_bound_at_every_angle() {
    for (airfoil, sweep_deg, taper) in [
        ("naca0012", 0.0, 1.0),
        ("naca0012", 30.0, 0.25),
        ("naca4412", 0.0, 0.4),
        ("naca4412", 35.0, 0.2),
        ("naca6409", 32.0, 0.25),
    ] {
        let (plane, aspect_ratio) = tapered(airfoil, sweep_deg, taper, 24);
        for alpha_deg in [-6.0, -4.0, -2.0, 0.0, 2.0, 4.0, 8.0] {
            let (cl, cdi, _) = solve(&plane, alpha_deg, 8);
            let label = format!("{airfoil} sweep {sweep_deg} taper {taper} alpha {alpha_deg}");
            assert!(cdi >= 0.0, "{label}: CDi {cdi}");
            assert!(
                cl * cl <= PI * aspect_ratio * cdi,
                "{label}: e {}",
                span_efficiency(cl, cdi, aspect_ratio)
            );
        }
    }
}

/// A few degrees of dihedral make the wake trace a shallow V, which changes
/// the span efficiency by a fraction of a percent (Cone, *The Theory of
/// Induced Lift and Minimum Induced Drag of Nonplanar Lifting Systems*, NASA
/// TR R-139, 1962). With the sections set at an incidence, the two halves'
/// root trailing-edge corners no longer coincide; the far-field sheet must
/// still run unbroken across the root rather than shed a root vortex.
#[test]
fn a_dihedralled_wing_at_incidence_is_one_sheet_across_its_root() {
    let efficiency = |dihedral_deg: f64, incidence_deg: f64| {
        let (plane, aspect_ratio) =
            dihedralled("naca4412", 30.0, 0.25, 24, dihedral_deg, incidence_deg);
        let (cl, cdi, _) = solve(&plane, 4.0 - incidence_deg, 8);
        span_efficiency(cl, cdi, aspect_ratio)
    };
    let planar = efficiency(0.0, 0.0);
    let dihedralled = efficiency(6.0, 4.0);
    assert!(
        (dihedralled - planar).abs() < 0.01,
        "planar {planar}, dihedralled {dihedralled}"
    );
}

#[test]
fn every_elliptic_mesh_retains_munks_bound_at_every_lifting_angle() {
    for stations in [6, 12, 24, 48] {
        let (plane, aspect_ratio) = elliptic(stations);
        for alpha_deg in [-8.0, -4.0, -0.5, 0.5, 4.0, 8.0] {
            let (cl, cdi, linear_cl) = solve(&plane, alpha_deg, 4);
            for lift in [cl, linear_cl] {
                assert!(cdi >= 0.0);
                assert!(
                    lift * lift <= PI * aspect_ratio * cdi,
                    "elliptic {stations} sections alpha {alpha_deg}: e {}",
                    span_efficiency(lift, cdi, aspect_ratio)
                );
            }
        }
    }
}

fn panel(left: [f64; 2], right: [f64; 2], trailing: bool, wing_index: usize) -> vlm::PanelSample {
    let front_left = [0.0, left[0], left[1]];
    let front_right = [0.0, right[0], right[1]];
    vlm::PanelSample {
        front_left,
        back_left: [1.0, left[0], left[1]],
        back_right: [1.0, right[0], right[1]],
        front_right,
        left_vortex_vertex: [0.25, left[0], left[1]],
        right_vortex_vertex: [0.25, right[0], right[1]],
        vortex_center: [0.25, 0.5 * (left[0] + right[0]), 0.5 * (left[1] + right[1])],
        is_trailing_edge: trailing,
        wing_index,
    }
}

/// A triangular loading of peak circulation Gamma0 has
/// `D_i / (rho Gamma0^2) = ln(2) / pi`, obtained by inserting its two
/// constant-vorticity segments into the logarithmic energy integral.
/// A single strip's conservative hat has peak Gamma0 = 2 Gamma_strip.
/// Scaling and translating the wake must leave this dimensionless energy
/// unchanged; the logarithm's dimensional offset cancels because the total
/// shed vorticity is zero.
#[test]
fn triangular_loading_matches_its_analytic_energy_at_any_span_and_origin() {
    let expected = 4.0 * 2.0_f64.ln() / PI;
    for (origin, semispan) in [([0.0, 0.0], 1.0), ([10.0, 7.0], 3.0), ([-2.0, 1.0], 0.2)] {
        let mesh = [panel(
            [origin[0] - semispan, origin[1]],
            [origin[0] + semispan, origin[1]],
            true,
            0,
        )];
        let operator = super::TrefftzOperator::from_panels(&mesh).unwrap();
        assert!(
            (operator.form[0] - expected).abs() < 1.0e-8,
            "origin {origin:?} semispan {semispan}: energy {} vs {expected}",
            operator.form[0]
        );
    }
}

#[test]
fn midpoint_hats_conserve_integrated_circulation_on_a_nonuniform_mesh() {
    let boundaries = [-2.0, -1.2, -0.3, 0.5, 2.0];
    let strips: Vec<super::Strip> = boundaries
        .windows(2)
        .map(|pair| super::Strip {
            left: [pair[0], 0.0],
            right: [pair[1], 0.0],
            wing_index: 0,
        })
        .collect();
    let (segments, mapping) = super::wake_segments(&strips).unwrap();
    for circulation in [
        [1.0, 1.0, 1.0, 1.0],
        [0.5, 1.5, -0.3, 0.7],
        [-1.0, 0.0, 0.0, 2.0],
    ] {
        let expected: f64 = strips
            .iter()
            .zip(circulation)
            .map(|(strip, gamma)| gamma * super::length(strip.left, strip.right))
            .sum();
        let mut at_start = 0.0;
        let mut reconstructed = 0.0;
        let n = strips.len();
        let nodal: Vec<f64> = (0..n)
            .map(|i| (0..n).map(|j| mapping[i * n + j] * circulation[j]).sum())
            .collect();
        let mut strip_integrals = vec![0.0; n];
        for (index, segment) in segments.iter().enumerate() {
            let width = super::length(segment.start, segment.end);
            let vorticity: f64 = segment
                .gamma
                .iter()
                .map(|&(k, weight)| weight * nodal[k])
                .sum();
            let at_end = at_start - vorticity * width;
            let integral = 0.5 * (at_start + at_end) * width;
            reconstructed += integral;
            strip_integrals[index / 2] += integral;
            at_start = at_end;
        }
        for (i, strip) in strips.iter().enumerate() {
            let expected_strip = circulation[i] * super::length(strip.left, strip.right);
            assert!(
                (strip_integrals[i] - expected_strip).abs() < 1.0e-12,
                "strip {i}: reconstructed {} vs mean integral {expected_strip}",
                strip_integrals[i]
            );
        }
        assert!(at_start.abs() < 1.0e-12, "free-end circulation {at_start}");
        assert!(
            (reconstructed - expected).abs() < 1.0e-12,
            "reconstructed {reconstructed} vs strip integral {expected}"
        );
    }
}

#[test]
fn malformed_or_degenerate_strips_return_no_drag_operator() {
    assert!(super::TrefftzOperator::from_panels(&[]).is_none());
    for mesh in [
        vec![panel([-1.0, 0.0], [1.0, 0.0], false, 0)],
        vec![panel([0.0, 0.0], [0.0, 0.0], true, 0)],
        vec![panel([f64::NAN, 0.0], [1.0, 0.0], true, 0)],
        vec![
            panel([-1.0, 0.0], [1.0, 0.0], false, 0),
            panel([-1.0, 0.0], [1.0, 0.0], true, 1),
        ],
    ] {
        assert!(super::TrefftzOperator::from_panels(&mesh).is_none());
    }
}

#[test]
fn a_closed_wake_trace_cannot_silently_drop_its_energy() {
    let strips = [
        super::Strip {
            left: [-1.0, 0.0],
            right: [1.0, 0.0],
            wing_index: 0,
        },
        super::Strip {
            left: [1.0, 0.0],
            right: [-1.0, 0.0],
            wing_index: 0,
        },
    ];
    assert!(super::wake_segments(&strips).is_none());
}
