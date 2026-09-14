// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The property these tests exist for is not the panel count. It is that a
//! planform feature survives being meshed, at every count.

use super::*;
use crate::aircraft::airfoil::Airfoil;

fn naca(name: &str) -> Airfoil {
    Airfoil::from_name(name).expect("a 4-digit NACA name resolves")
}

/// A transport semispan with the two stations that matter: a side-of-body at
/// 10 % and a Yehudi kink at 37 %, both with a chord slope discontinuity.
fn transport_wing() -> Wing {
    let section = naca("naca2412");
    Wing::new(
        "Main Wing",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], 6.0, 4.0, section.clone()),
            WingXSec::new([0.6, 1.8, 0.1], 5.6, 3.2, section.clone()),
            WingXSec::new([3.0, 6.66, 0.4], 3.1, 1.0, section.clone()),
            WingXSec::new([8.4, 18.0, 1.0], 1.4, -2.0, section),
        ],
        true,
    )
}

fn stations_of(wing: &Wing) -> Vec<f64> {
    wing.spanwise_stations()
}

fn contains_station(haystack: &[f64], needle: f64) -> bool {
    haystack
        .iter()
        .any(|candidate| (candidate - needle).abs() < 1.0e-12)
}

#[test]
fn every_planform_station_survives_every_panel_count() {
    // The kink is the case that motivated this: a mesh that averages across
    // it reports a different wing from the one the design vector describes.
    let wing = transport_wing();
    let original = stations_of(&wing);
    for panels in [1_usize, 2, 3, 4, 5, 7, 8, 12, 13, 24, 25, 48, 96] {
        let meshed = wing
            .mesh_spanwise(panels, SpacingFunction::Linspace)
            .expect("a single-airfoil wing needs no blend");
        let meshed_stations = stations_of(&meshed);
        for &station in &original {
            assert!(
                contains_station(&meshed_stations, station),
                "{panels} panels lost the station at {station}"
            );
        }
    }
}

#[test]
fn a_station_does_not_move_when_the_mesh_is_refined() {
    // Not merely present: present at the same place. A station that drifted
    // with panel count would make a mesh-refinement study measure the
    // planform moving rather than the discretisation converging.
    let wing = transport_wing();
    let coarse = stations_of(
        &wing
            .mesh_spanwise(4, SpacingFunction::Linspace)
            .expect("meshes"),
    );
    let fine = stations_of(
        &wing
            .mesh_spanwise(96, SpacingFunction::Linspace)
            .expect("meshes"),
    );
    for &station in &stations_of(&wing) {
        assert!(contains_station(&coarse, station));
        assert!(contains_station(&fine, station));
    }
}

#[test]
fn the_panel_count_is_the_number_asked_for() {
    let wing = transport_wing();
    for panels in [3_usize, 4, 5, 7, 8, 11, 24, 25, 96] {
        let meshed = wing
            .mesh_spanwise(panels, SpacingFunction::Linspace)
            .expect("meshes");
        assert_eq!(meshed.xsecs.len() - 1, panels, "asked for {panels} panels");
    }
}

#[test]
fn a_count_below_the_section_count_keeps_the_sections() {
    // Honouring the count would mean dropping a station, so the station wins
    // and the mesh is as coarse as the planform allows.
    let wing = transport_wing();
    for panels in [0_usize, 1, 2] {
        let meshed = wing
            .mesh_spanwise(panels, SpacingFunction::Linspace)
            .expect("meshes");
        assert_eq!(meshed.xsecs.len() - 1, 3);
    }
}

#[test]
fn the_budget_is_split_evenly_between_sections() {
    // Not proportionally to span: the short inboard sections are where a
    // swept transport's loading changes fastest, and the measurements in the
    // module doc show an even split converged at 24 panels where uniform
    // width needs about 96 for the same induced drag.
    let wing = transport_wing();
    let meshed = wing
        .mesh_spanwise(30, SpacingFunction::Linspace)
        .expect("meshes");
    let stations = stations_of(&meshed);
    let original = stations_of(&wing);

    let panels_between = |low: f64, high: f64| {
        stations
            .iter()
            .filter(|station| **station > low + 1.0e-12 && **station < high - 1.0e-12)
            .count()
            + 1
    };
    for section in 0..3 {
        assert_eq!(
            panels_between(original[section], original[section + 1]),
            10,
            "section {section} of a 30-panel three-section wing"
        );
    }
}

#[test]
fn an_even_split_reproduces_the_mesh_the_per_section_multiplier_made() {
    // The distribution the shipped aircraft were converged on. An absolute
    // count of `ratio * sections` must give exactly the mesh the multiplier
    // gave at `ratio`, so making the count absolute changed which number the
    // user types and nothing about the aerodynamics.
    let wing = transport_wing();
    for ratio in [2_usize, 3, 8] {
        let ported = wing
            .subdivide_sections(ratio, SpacingFunction::Linspace)
            .expect("subdivides");
        let counted = wing
            .mesh_spanwise(ratio * 3, SpacingFunction::Linspace)
            .expect("meshes");
        assert_eq!(ported.xsecs.len(), counted.xsecs.len(), "ratio {ratio}");
        for (left, right) in ported.xsecs.iter().zip(&counted.xsecs) {
            assert!((left.chord - right.chord).abs() < 1.0e-12, "ratio {ratio}");
            assert!((left.twist - right.twist).abs() < 1.0e-12, "ratio {ratio}");
            for axis in 0..3 {
                assert!(
                    (left.xyz_le[axis] - right.xyz_le[axis]).abs() < 1.0e-12,
                    "ratio {ratio}"
                );
            }
        }
    }
}

#[test]
fn refining_the_mesh_does_not_move_the_projected_planform() {
    // Interpolating along a piecewise-linear loft is exact, so the reference
    // area and span a coefficient is normalized by must not depend on how
    // finely the surface was panelled. If they did, refining the mesh would
    // be changing the aircraft rather than describing it.
    let wing = transport_wing();
    let coarse = wing
        .mesh_spanwise(4, SpacingFunction::Linspace)
        .expect("meshes");
    let fine = wing
        .mesh_spanwise(96, SpacingFunction::Linspace)
        .expect("meshes");
    for (name, a, b) in [
        ("area", coarse.reference_area(), fine.reference_area()),
        ("span", coarse.reference_span(), fine.reference_span()),
    ] {
        assert!(
            (a - b).abs() < 1.0e-9 * a.abs().max(1.0),
            "{name} moved with the mesh: {a} against {b}"
        );
    }
}

#[test]
fn the_mean_chord_is_no_more_mesh_dependent_than_the_ported_subdivision() {
    // `mean_aerodynamic_chord` weighs each section by `sectional_spans_yz`,
    // which measures along the *quarter-chord* path. On a twisted wing the
    // quarter-chord point carries a `chord * sin(twist)` offset in Z, and
    // that is not linear along a section even though chord and twist each
    // are -- so the measured span, and with it the weighting, shifts very
    // slightly as stations are added.
    //
    // This is a property of the existing area measure, not of how the
    // stations are chosen: the ported multiplier shows it too. The test
    // pins both, so a future change to either cannot quietly make this
    // worse, and records the magnitude as physically irrelevant (about 1e-5
    // relative, against a reference chord in metres).
    let wing = transport_wing();
    let mine = |panels| {
        wing.mesh_spanwise(panels, SpacingFunction::Linspace)
            .expect("meshes")
            .mean_aerodynamic_chord()
    };
    let ported = |ratio| {
        wing.subdivide_sections(ratio, SpacingFunction::Linspace)
            .expect("subdivides")
            .mean_aerodynamic_chord()
    };

    let ported_drift = (ported(2) - ported(32)).abs() / ported(32);
    let my_drift = (mine(6) - mine(96)).abs() / mine(96);
    assert!(
        ported_drift > 0.0,
        "the ported subdivision was expected to drift too; if it no longer          does, the area measure was fixed and this test should tighten"
    );
    assert!(
        my_drift <= ported_drift * 2.0 + 1.0e-12,
        "counted mesh drifts {my_drift:e}, ported multiplier {ported_drift:e}"
    );
    assert!(
        my_drift < 1.0e-4,
        "drift {my_drift:e} is no longer negligible"
    );
}

#[test]
fn a_vertical_fin_is_measured_by_its_own_span() {
    // `hypot(dy, dz)` rather than a Y projection: a fin has no Y extent, and
    // a projection would give every section zero weight and collapse the
    // allocation into the degenerate branch.
    let section = naca("naca0012");
    let fin = Wing::new(
        "Vertical Stabilizer",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], 5.0, 0.0, section.clone()),
            WingXSec::new([1.0, 0.0, 2.0], 4.0, 0.0, section.clone()),
            WingXSec::new([4.0, 0.0, 8.0], 2.0, 0.0, section),
        ],
        false,
    );
    let meshed = fin
        .mesh_spanwise(12, SpacingFunction::Linspace)
        .expect("meshes");
    assert_eq!(meshed.xsecs.len() - 1, 12);
    for &station in &stations_of(&fin) {
        assert!(contains_station(&stations_of(&meshed), station));
    }
    // Two sections, twelve panels, six each -- and the station between them
    // is still exactly where the fin is cranked.
    let stations = stations_of(&meshed);
    let upper = stations.iter().filter(|s| **s > 0.25 + 1.0e-12).count();
    assert_eq!(upper, 6, "upper section stations");
}

#[test]
fn allocation_is_exact_and_deterministic() {
    let extents = [1.8_f64, 4.86, 11.34];
    for panels in 3..64_usize {
        let counts = allocate(&extents, panels);
        assert_eq!(counts.iter().sum::<usize>(), panels);
        assert!(counts.iter().all(|&count| count >= 1));
        assert_eq!(counts, allocate(&extents, panels), "not reproducible");
        // Even to within the remainder, which is the property the induced
        // drag evidence rests on.
        let (low, high) = (
            counts.iter().min().copied().unwrap_or(0),
            counts.iter().max().copied().unwrap_or(0),
        );
        assert!(high - low <= 1, "{counts:?} is not an even split");
    }
}

#[test]
fn a_spare_panel_goes_to_the_longest_section() {
    // Where the remainder lands is arbitrary in principle, so it is pinned:
    // the longest section gains the least by an extra panel per metre, but
    // it is also the one whose panels are widest, and an unpinned rule makes
    // the mesh depend on section ordering.
    assert_eq!(allocate(&[1.0, 5.0, 2.0], 4), vec![1, 2, 1]);
    assert_eq!(allocate(&[1.0, 5.0, 2.0], 5), vec![1, 2, 2]);
    assert_eq!(allocate(&[1.0, 5.0, 2.0], 6), vec![2, 2, 2]);
}

#[test]
fn meshing_reproduces_the_reference_subdivision_where_the_two_agree() {
    // On a wing whose sections are all the same length, an absolute count of
    // `ratio * sections` is the same mesh the ported multiplier produces.
    // Pinning that keeps this module honest against the routine it replaces.
    let section = naca("naca0012");
    let even = Wing::new(
        "Even",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], 2.0, 0.0, section.clone()),
            WingXSec::new([0.0, 5.0, 0.0], 1.5, 0.0, section.clone()),
            WingXSec::new([0.0, 10.0, 0.0], 1.0, 0.0, section),
        ],
        true,
    );
    let by_multiplier = even
        .subdivide_sections(4, SpacingFunction::Linspace)
        .expect("subdivides");
    let by_count = even
        .mesh_spanwise(8, SpacingFunction::Linspace)
        .expect("meshes");
    assert_eq!(by_multiplier.xsecs.len(), by_count.xsecs.len());
    for (left, right) in by_multiplier.xsecs.iter().zip(&by_count.xsecs) {
        assert!((left.chord - right.chord).abs() < 1.0e-12);
        assert!((left.twist - right.twist).abs() < 1.0e-12);
        for axis in 0..3 {
            assert!((left.xyz_le[axis] - right.xyz_le[axis]).abs() < 1.0e-12);
        }
    }
}
