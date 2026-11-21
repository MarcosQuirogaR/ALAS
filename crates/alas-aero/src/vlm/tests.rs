// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;
use alas_atmo::Atmosphere;
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::wing::{Wing, WingXSec};

fn naca(name: &str) -> Airfoil {
    Airfoil::from_name(name).expect("valid 4-digit NACA name")
}

fn flat_rectangular_wing(symmetric: bool) -> Wing {
    Wing::new(
        "Flat",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], 1.0, 0.0, naca("naca0012")),
            WingXSec::new([0.0, 5.0, 0.0], 1.0, 0.0, naca("naca0012")),
        ],
        symmetric,
    )
}

fn single_wing_airplane(symmetric: bool) -> Airplane {
    let wing = flat_rectangular_wing(symmetric);
    // The probe represents a product/reference aircraft, so its
    // coefficient scales must use the projected XY reference plane.
    let s_ref = wing.reference_area();
    let b_ref = wing.reference_span();
    let c_ref = wing.mean_aerodynamic_chord();
    Airplane {
        name: "Probe".to_owned(),
        xyz_ref: [0.25, 0.0, 0.0],
        wings: vec![wing],
        fuselages: Vec::new(),
        s_ref,
        c_ref,
        b_ref,
    }
}

fn level_flight_point(alpha: f64) -> OperatingPoint {
    OperatingPoint::new(Atmosphere::new(0.0), 50.0, alpha, 0.0, 0.0, 0.0, 0.0)
}

#[test]
fn a_symmetric_flat_plate_at_zero_alpha_produces_no_lift() {
    let airplane = single_wing_airplane(true);
    let op_point = level_flight_point(0.0);
    let result = run(&airplane, &op_point, 1, 4).expect("well-posed solve");
    assert!(
        result.lift.abs() < 1.0,
        "an uncambered symmetric wing at zero incidence should carry ~no lift, got {}",
        result.lift
    );
}

#[test]
fn positive_angle_of_attack_produces_positive_lift() {
    let airplane = single_wing_airplane(true);
    let op_point = level_flight_point(5.0);
    let result = run(&airplane, &op_point, 1, 4).expect("well-posed solve");
    assert!(result.lift > 0.0, "lift={}", result.lift);
    assert!(result.cl_lift > 0.0);
}

#[test]
fn a_symmetric_wing_at_zero_beta_produces_no_side_force_or_yaw_or_roll() {
    let airplane = single_wing_airplane(true);
    let op_point = level_flight_point(4.0);
    let result = run(&airplane, &op_point, 1, 4).expect("well-posed solve");
    assert!(result.side_force.abs() < 1e-6, "Y={}", result.side_force);
    assert!(result.yaw_moment.abs() < 1e-6, "n_b={}", result.yaw_moment);
    assert!(
        result.roll_moment.abs() < 1e-6,
        "l_b={}",
        result.roll_moment
    );
}

#[test]
fn vortex_strengths_has_one_entry_per_panel() {
    let airplane = single_wing_airplane(false);
    let op_point = level_flight_point(3.0);
    let result = run(&airplane, &op_point, 1, 4).expect("well-posed solve");
    // 1 spanwise interval * 4 chordwise panels, unmirrored.
    assert_eq!(result.vortex_strengths.len(), 4);
}

#[test]
fn spanwise_resolution_above_one_multiplies_the_panel_count() {
    let airplane = single_wing_airplane(false);
    let op_point = level_flight_point(3.0);
    let coarse = run(&airplane, &op_point, 1, 2).expect("well-posed solve");
    let fine = run(&airplane, &op_point, 3, 2).expect("well-posed solve");
    assert_eq!(coarse.vortex_strengths.len(), 2);
    assert_eq!(fine.vortex_strengths.len(), 3 * 2);
}

#[test]
fn doubling_the_freestream_velocity_leaves_the_lift_coefficient_unchanged() {
    // CL depends on alpha, not on the airspeed itself, for an inviscid
    // linear solve, doubling V quadruples both L and q, so CL should be
    // invariant.
    let airplane = single_wing_airplane(true);
    let slow = OperatingPoint::new(Atmosphere::new(0.0), 40.0, 4.0, 0.0, 0.0, 0.0, 0.0);
    let fast = OperatingPoint::new(Atmosphere::new(0.0), 80.0, 4.0, 0.0, 0.0, 0.0, 0.0);
    let slow_result = run(&airplane, &slow, 1, 4).expect("well-posed solve");
    let fast_result = run(&airplane, &fast, 1, 4).expect("well-posed solve");
    assert!(
        (slow_result.cl_lift - fast_result.cl_lift).abs() < 1e-9,
        "slow CL={} fast CL={}",
        slow_result.cl_lift,
        fast_result.cl_lift
    );
}

#[test]
fn one_assembled_system_solves_like_one_run_per_point() {
    let airplane = single_wing_airplane(true);
    let system = VlmSystem::assemble(&airplane, 2, 3).expect("well-posed mesh");
    assert_eq!(system.panel_count(), 2 * 3 * 2);
    for (index, alpha) in [-2.0, 0.0, 3.0, 7.5].into_iter().enumerate() {
        let op_point = level_flight_point(alpha);
        let shared = system.solve(&op_point).expect("well-posed solve");
        let alone = run(&airplane, &op_point, 2, 3).expect("well-posed solve");
        if index == 0 {
            // The first solve of a system is the direct one `run` makes.
            assert_eq!(shared, alone, "alpha={alpha}");
            continue;
        }
        // Later solves combine the system's basis solves: the same solve
        // to rounding.
        let close = |a: f64, b: f64| (a - b).abs() <= 1.0e-12 * (1.0 + b.abs());
        assert!(close(shared.cl_lift, alone.cl_lift), "alpha={alpha}");
        assert!(close(shared.cd_drag, alone.cd_drag), "alpha={alpha}");
        assert!(close(shared.cm_pitch, alone.cm_pitch), "alpha={alpha}");
        for (a, b) in shared.vortex_strengths.iter().zip(&alone.vortex_strengths) {
            assert!(close(*a, *b), "alpha={alpha}: {a} against {b}");
        }
    }
}

#[test]
fn rate_derivatives_are_invariant_to_a_rigid_translation_about_xyz_ref() {
    let airplane = single_wing_airplane(true);
    // A symmetric wing is mirrored about the global XZ plane, so keep the
    // translation in the aircraft's symmetry-preserving x/z directions.
    let translation = [37.0, 0.0, 4.5];
    let mut translated = airplane.clone();
    translated.xyz_ref = [
        airplane.xyz_ref[0] + translation[0],
        airplane.xyz_ref[1] + translation[1],
        airplane.xyz_ref[2] + translation[2],
    ];
    translated.wings = airplane
        .wings
        .iter()
        .map(|wing| {
            let xsecs = wing
                .xsecs
                .iter()
                .map(|xsec| xsec.translate(translation))
                .collect();
            Wing::new(wing.name.clone(), xsecs, wing.symmetric)
        })
        .collect();

    let op_point = OperatingPoint::new(Atmosphere::new(0.0), 50.0, 4.0, 1.0, 0.003, 0.004, 0.005);
    let original = run_with_stability_derivatives(&airplane, &op_point, 2, 3)
        .expect("original solve should be well posed");
    let shifted = run_with_stability_derivatives(&translated, &op_point, 2, 3)
        .expect("translated solve should be well posed");

    let differences = [
        ("Clp", original.d_p.cl_lift, shifted.d_p.cl_lift),
        ("Cmp", original.d_p.cm_pitch, shifted.d_p.cm_pitch),
        ("CLq", original.d_q.cl_lift, shifted.d_q.cl_lift),
        ("Cmq", original.d_q.cm_pitch, shifted.d_q.cm_pitch),
        ("CYr", original.d_r.cy_side, shifted.d_r.cy_side),
        ("Cnr", original.d_r.cn_yaw, shifted.d_r.cn_yaw),
    ];
    for (name, before, after) in differences {
        assert!(
            (before - after).abs() < 1e-8,
            "{name}: {before} changed to {after}"
        );
    }
}

#[test]
fn a_zero_area_panel_is_rejected_before_normalization() {
    let result = system::Panel::from_quad(
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
        false,
        3,
    );
    assert!(matches!(
        result,
        Err(VlmError::DegeneratePanel { wing_index: 3 })
    ));
}

#[test]
fn nonpositive_or_nonfinite_derivative_steps_are_rejected() {
    let airplane = single_wing_airplane(true);
    let op_point = level_flight_point(4.0);
    for (angle_step, rate_step) in [(0.0, 0.001), (-0.001, 0.001), (f64::NAN, 0.001)] {
        assert_eq!(
            run_with_stability_derivatives_with_steps(
                &airplane, &op_point, 2, 3, angle_step, rate_step,
            ),
            Err(VlmError::InvalidDerivativeStep)
        );
    }
}

/// Rectangular wing (semispan 5 m, chord 1 m) with a vertical winglet of
/// `height` metres at the tip; coefficients stay on the bare wing's reference
/// quantities. `name` selects whether the vortex-lattice model carries it.
fn winglet_airplane(height: f64, name: &str) -> Airplane {
    let mut airplane = single_wing_airplane(true);
    if height > 0.0 {
        airplane.wings.push(Wing::new(
            name,
            vec![
                WingXSec::new([0.0, 5.0, 0.0], 1.0, 0.0, naca("naca0012")),
                WingXSec::new([0.2, 5.0, height], 0.6, 0.0, naca("naca0012")),
            ],
            true,
        ));
    }
    airplane
}

#[test]
fn a_vertical_winglet_raises_the_span_efficiency_and_a_geometry_only_one_changes_nothing() {
    use alas_geom::aircraft::wing::{WINGTIP_DEVICE_GEOMETRY_ONLY_NAME, WINGTIP_DEVICE_NAME};
    let op_point = level_flight_point(5.0);
    let efficiency = |airplane: &Airplane| {
        let result = run(airplane, &op_point, 6, 6).expect("well-posed solve");
        let aspect_ratio = airplane.b_ref * airplane.b_ref / airplane.s_ref;
        result.cl_lift * result.cl_lift / (std::f64::consts::PI * aspect_ratio * result.cd_drag)
    };
    let bare = winglet_airplane(0.0, "");
    let with = winglet_airplane(1.0, WINGTIP_DEVICE_NAME);
    let hidden = winglet_airplane(1.0, WINGTIP_DEVICE_GEOMETRY_ONLY_NAME);
    let (e_bare, e_with, e_hidden) = (efficiency(&bare), efficiency(&with), efficiency(&hidden));
    // Analytic expectation: a winglet adds effective span (non-planar wake),
    // so the induced drag at a given lift falls and e rises (Whitcomb 1976;
    // for h/b = 0.1 the gain is of order 5-10 %).
    assert!(
        e_with > e_bare * 1.01,
        "e bare {e_bare}, e winglet {e_with}"
    );
    assert!(
        e_with < e_bare * 1.4,
        "implausible gain: {e_bare} -> {e_with}"
    );
    assert_eq!(
        e_hidden, e_bare,
        "a geometry-only device must not enter the lattice"
    );
}

#[test]
fn the_drawn_b747_winglet_sections_solve_and_barely_move_the_lift() {
    // The tip edge is a strip with almost no span in y and a chord step along it:
    // a well-posed solve, and the winglet's lift stays a small share of the total.
    let preset = alas_config::presets::get("B747-400").expect("preset");
    let build = |clear: bool| {
        let mut geometry = preset.geometry.clone();
        let device = geometry.wing.wingtip_device.as_mut().expect("winglet");
        assert!(!device.path_stations.is_empty());
        if clear {
            device.path_stations.clear();
        }
        alas_geom::builder::AircraftBuilder::new(Some(geometry))
            .build(Some(&preset.design_vector), true)
            .expect("builds")
    };
    let op_point = level_flight_point(3.0);
    let (drawn_airplane, plain_airplane) = (build(false), build(true));
    // Every spanwise refinement: the sections of the tip edge are narrower than
    // the camber height of their lattice, which once made the matrix singular
    // above one spanwise panel (CL 69 at 2x4, ill-conditioned at 8x8).
    for (spanwise, chordwise) in [(1, 4), (2, 4), (4, 4), (4, 8), (8, 8)] {
        let lift = |airplane: &Airplane| {
            let result = run(airplane, &op_point, spanwise, chordwise).expect("well-posed solve");
            assert!(result.cl_lift.is_finite() && result.cd_drag.is_finite());
            result.cl_lift
        };
        let (drawn, plain) = (lift(&drawn_airplane), lift(&plain_airplane));
        assert!(
            drawn > 0.0 && (drawn / plain - 1.0).abs() < 0.02,
            "{spanwise}x{chordwise}: {drawn} {plain}"
        );
    }
}

/// A rectangular cambered wing (semispan 5 m, chord 1 m, NACA 4412) ending in a
/// tip section `tip_m` wide, coefficients on fixed reference quantities.
fn wing_with_a_narrow_tip_section(tip_m: f64) -> Airplane {
    let airfoil = naca("naca4412");
    let mut sections = vec![
        WingXSec::new([0.0, 0.0, 0.0], 1.0, 0.0, airfoil.clone()),
        WingXSec::new([0.0, 5.0, 0.0], 1.0, 0.0, airfoil.clone()),
    ];
    if tip_m > 0.0 {
        sections.push(WingXSec::new([0.0, 5.0 + tip_m, 0.0], 1.0, 0.0, airfoil));
    }
    let mut airplane = single_wing_airplane(true);
    airplane.wings = vec![Wing::new("Narrow tip", sections, true)];
    airplane.s_ref = 10.0;
    airplane.b_ref = 10.0;
    airplane.c_ref = 1.0;
    airplane
}

#[test]
fn a_cambered_section_narrower_than_its_camber_height_is_not_split() {
    // A 0.01 m tip section of a 4 %-cambered 1 m chord: cut into eight
    // cosine-spaced strips it once gave an ill-conditioned matrix; kept whole
    // it adds its 0.2 % of area and nothing else at every resolution.
    let op_point = level_flight_point(4.0);
    let (narrow, plain) = (
        wing_with_a_narrow_tip_section(0.01),
        wing_with_a_narrow_tip_section(0.0),
    );
    for (spanwise, chordwise) in [(2, 4), (4, 4), (8, 4), (8, 8), (8, 16)] {
        let with = run(&narrow, &op_point, spanwise, chordwise).expect("well-posed solve");
        let without = run(&plain, &op_point, spanwise, chordwise).expect("well-posed solve");
        assert!(
            (with.cl_lift / without.cl_lift - 1.0).abs() < 0.01,
            "{spanwise}x{chordwise}: {} {}",
            with.cl_lift,
            without.cl_lift
        );
    }
    // The wide section still takes the full request, the narrow one stays whole.
    let system = VlmSystem::assemble(&plain, 8, 4).expect("assembles");
    assert_eq!(system.panel_count(), 2 * 8 * 4);
    let system = VlmSystem::assemble(&narrow, 8, 4).expect("assembles");
    assert_eq!(system.panel_count(), 2 * (8 + 1) * 4);
}

#[test]
fn a_surface_at_a_dynamic_pressure_ratio_carries_that_share_of_its_lift_and_drag() {
    // circulation scales with the square root of the ratio and the
    // force with the ratio, so one surface carries exactly `ratio` of the lift
    // and of the Trefftz induced drag; a ratio of 1 is the unchanged solve.
    let airplane = single_wing_airplane(true);
    let op_point = level_flight_point(4.0);
    let system = VlmSystem::assemble(&airplane, 4, 4).expect("assembles");
    let free = system.solve(&op_point).expect("solves");
    let mut weak = airplane.clone();
    weak.wings[0].dynamic_pressure_ratio = 0.6;
    let weak_system = VlmSystem::assemble(&weak, 4, 4).expect("assembles");
    let reduced = weak_system.solve(&op_point).expect("solves");
    assert!((reduced.lift / free.lift - 0.6).abs() < 1.0e-12);
    assert!((reduced.cm_pitch / free.cm_pitch - 0.6).abs() < 1.0e-12);
    let drag_free = system
        .trefftz_induced_drag_coefficient(&free, &op_point)
        .expect("Trefftz");
    let drag_reduced = weak_system
        .trefftz_induced_drag_coefficient(&reduced, &op_point)
        .expect("Trefftz");
    assert!(
        (drag_reduced / drag_free - 0.6).abs() < 1.0e-9,
        "{}",
        drag_reduced / drag_free
    );
    let mut unit = airplane.clone();
    unit.wings[0].dynamic_pressure_ratio = 1.0;
    let same = VlmSystem::assemble(&unit, 4, 4)
        .expect("assembles")
        .solve(&op_point)
        .expect("solves");
    assert_eq!(same.lift.to_bits(), free.lift.to_bits());
}

#[test]
fn the_mirrored_halves_of_a_twisted_dihedral_surface_stay_smooth_through_the_plane_of_symmetry() {
    // A long-root-chord surface with 7 deg dihedral, twisted nose down: each
    // section turns about its own dihedral-inclined axis, so the root
    // trailing edge moves sideways by c sin(twist) sin(dihedral), 0.13 m at
    // 6 deg, and the two mirrored halves cross the plane of symmetry. Their
    // trailing legs then pass the other half's root collocation points; the
    // pitching moment against twist must stay as smooth as the solve.
    // Uniform 0.21 m strips, as the builder subdivides a tail surface.
    let dihedral = 7.0_f64.to_radians();
    let surface = |twist: f64| {
        let xsecs = (0..=24)
            .map(|k| {
                let eta = f64::from(k) / 24.0;
                let le = [
                    4.0 * eta,
                    5.0 * eta * dihedral.cos(),
                    5.0 * eta * dihedral.sin(),
                ];
                WingXSec::new(le, 10.0 - 6.0 * eta, twist, naca("naca0012"))
            })
            .collect();
        let mut airplane = single_wing_airplane(true);
        airplane.wings[0] = Wing::new("Tail", xsecs, true);
        airplane.xyz_ref = [2.5, 0.0, 0.0];
        run(&airplane, &level_flight_point(3.0), 1, 4)
            .expect("solves")
            .cm_pitch
    };
    let step = 0.05;
    let cm: Vec<f64> = (0..=60)
        .map(|k| surface(-7.5 + step * f64::from(k)))
        .collect();
    let slope = (cm[60] - cm[0]) / (60.0 * step);
    let worst = cm
        .windows(3)
        .map(|w| (w[2] - 2.0 * w[1] + w[0]).abs() / step)
        .fold(0.0_f64, f64::max);
    assert!(
        worst < 0.02 * slope.abs(),
        "slope {slope} per deg, worst slope jump {worst}"
    );
}

#[test]
fn a_tailplane_at_the_height_of_the_wing_trailing_legs_stays_smooth_against_incidence() {
    // C295: the stabilizer sits at the height of the wing's trailing legs,
    // which pass a few centimetres from its collocation points at 24
    // spanwise tail panels. Stepping the incidence through the cruise trim
    // band moves those points among the legs; the pitching moment against
    // incidence must stay as smooth as the solve (it had slope jumps of a
    // third of the slope, which stopped the cruise trim converging).
    use alas_geom::builder::AircraftBuilder;
    let config = alas_config::AlasConfig::from_value(&serde_json::json!({ "preset": "C295" }))
        .expect("preset");
    let design = config.configured_nominal_design();
    let mut geometry = config.geometry.clone();
    geometry.empennage.n_subdivisions = 24;
    let mut airplane = AircraftBuilder::new(Some(geometry))
        .build(Some(&design), true)
        .expect("builds");
    let frame = airplane.mac_frame().expect("mac frame");
    airplane.xyz_ref = [
        frame.x_lemac_m + 0.25 * airplane.c_ref,
        0.0,
        airplane.wings[0].xsecs[0].xyz_le[2],
    ];
    let atmosphere = Atmosphere::new(config.requirements.cruise_altitude_m);
    let speed = config.requirements.cruise_mach * atmosphere.speed_of_sound();
    let op_point = OperatingPoint::new(atmosphere, speed, 7.0, 0.0, 0.0, 0.0, 0.0);
    let spanwise = config.analysis.spanwise_resolution.max(1) as usize;
    let chordwise = config.analysis.chordwise_resolution.max(1) as usize;
    let moment =
        |incidence: f64| {
            let mut plane = airplane.clone();
            for wing in plane.wings.iter_mut().filter(|wing| {
                wing.name == alas_geom::aircraft::airplane::HORIZONTAL_STABILIZER_NAME
            }) {
                wing.twist_about_pitch_axis = true;
                for xsec in &mut wing.xsecs {
                    xsec.twist = incidence;
                }
            }
            run(&plane, &op_point, spanwise, chordwise)
                .expect("solves")
                .cm_pitch
        };
    let step = 0.2;
    let cm: Vec<f64> = (0..=40)
        .map(|k| moment(-9.0 + step * f64::from(k)))
        .collect();
    let slope = (cm[40] - cm[0]) / (40.0 * step);
    let worst = cm
        .windows(3)
        .map(|w| (w[2] - 2.0 * w[1] + w[0]).abs() / step)
        .fold(0.0_f64, f64::max);
    assert!(
        worst < 0.02 * slope.abs(),
        "slope {slope} per deg, worst slope jump {worst}"
    );
}

#[test]
// A probe prints its grouping.
#[allow(clippy::print_stdout)]
fn every_preset_groups_its_wingtip_devices_and_t_tails_with_their_parent_surface() {
    use alas_geom::aircraft::airplane::HORIZONTAL_STABILIZER_NAME;
    use alas_geom::aircraft::wing::WINGTIP_DEVICE_NAME;
    use alas_geom::builder::AircraftBuilder;
    // The presets whose stabilizer sits on the fin (their preset descriptions
    // and drawings): every other stabilizer is mounted on the fuselage.
    const T_TAILS: [&str; 3] = ["ATR72-600", "A400M", "LJ60"];
    for name in alas_config::presets::available() {
        let entry = alas_config::presets::get(name).expect("preset");
        let plane = AircraftBuilder::new(Some(entry.geometry.clone()))
            .build(Some(&entry.design_vector), true)
            .expect("builds");
        let component = system::lattice_components(&plane.wings);
        let of = |surface: &str| {
            plane
                .wings
                .iter()
                .position(|wing| wing.name == surface)
                .map(|index| component[index])
        };
        let wing = of("Main Wing");
        let stabilizer = of(HORIZONTAL_STABILIZER_NAME);
        let fin = of("Vertical Stabilizer");
        let grouping: Vec<(&str, usize)> = plane
            .wings
            .iter()
            .zip(&component)
            .map(|(surface, &c)| (surface.name.as_str(), c))
            .collect();
        println!("{name}: {grouping:?}");
        assert!(
            wing.is_some() && stabilizer.is_some() && fin.is_some(),
            "{name}"
        );
        if let Some(device) = of(WINGTIP_DEVICE_NAME) {
            assert_eq!(device, wing.expect("wing"), "{name}");
        }
        assert_ne!(stabilizer, wing, "{name}");
        assert_eq!(
            stabilizer == fin,
            T_TAILS.contains(&name),
            "{name}: {grouping:?}"
        );
        let diagnostics = system::lattice_grouping_diagnostics(&plane.wings);
        assert!(diagnostics.is_empty(), "{name}: {diagnostics:?}");
    }
}

/// A stabilizer lifted 5 cm off its fin tip is left ungrouped, and the
/// diagnostic names the fin tip that misses it.
#[test]
fn a_stabilizer_just_off_its_fin_is_reported_ungrouped() {
    use alas_geom::aircraft::airplane::HORIZONTAL_STABILIZER_NAME;
    use alas_geom::builder::AircraftBuilder;
    let entry = alas_config::presets::get("ATR72-600").expect("preset");
    let mut plane = AircraftBuilder::new(Some(entry.geometry.clone()))
        .build(Some(&entry.design_vector), true)
        .expect("builds");
    let fin_top = plane
        .wings
        .iter()
        .find(|wing| wing.name == "Vertical Stabilizer")
        .and_then(|fin| fin.xsecs.last())
        .map(|tip| tip.xyz_le[2])
        .expect("a fin");
    let stabilizer = plane
        .wings
        .iter_mut()
        .find(|wing| wing.name == HORIZONTAL_STABILIZER_NAME)
        .expect("a stabilizer");
    let lift = fin_top - stabilizer.xsecs[0].xyz_le[2] + 0.05;
    for section in &mut stabilizer.xsecs {
        section.xyz_le[2] += lift;
    }
    let component = system::lattice_components(&plane.wings);
    let index = |surface: &str| {
        plane
            .wings
            .iter()
            .position(|wing| wing.name == surface)
            .expect("a surface")
    };
    assert_ne!(
        component[index(HORIZONTAL_STABILIZER_NAME)],
        component[index("Vertical Stabilizer")]
    );
    let diagnostics = system::lattice_grouping_diagnostics(&plane.wings);
    assert!(
        diagnostics
            .iter()
            .any(|d| d.surface == "Vertical Stabilizer"
                && d.other == HORIZONTAL_STABILIZER_NAME
                && (d.gap_m - 0.05).abs() < 1.0e-9),
        "{diagnostics:?}"
    );
}

/// Zero-lift angle, deg, of a high-aspect-ratio untwisted rectangular wing of
/// `foil` from the lattice (CL at 1 and 3 deg) and from thin-airfoil theory,
/// `-(1/pi) int_0^pi z'(x) (cos t - 1) dt` (Anderson, Fundamentals of
/// Aerodynamics, eq. 4.61) on the section's own mean camber line.
fn lattice_and_thin_airfoil_zero_lift_angles(
    foil: Airfoil,
    chordwise: usize,
    reference_lattice: bool,
) -> (f64, f64) {
    let wing = Wing::new(
        "Rect",
        vec![
            WingXSec::new([0.0, 0.0, 0.0], 1.0, 0.0, foil.clone()),
            WingXSec::new([0.0, 20.0, 0.0], 1.0, 0.0, foil.clone()),
        ],
        true,
    );
    let airplane = Airplane {
        name: "Rect".to_owned(),
        xyz_ref: [0.25, 0.0, 0.0],
        wings: vec![wing],
        fuselages: Vec::new(),
        s_ref: 40.0,
        c_ref: 1.0,
        b_ref: 40.0,
    };
    let cl = |alpha: f64| {
        let op = level_flight_point(alpha);
        if reference_lattice {
            run_reference_compatibility(&airplane, &op, 1, chordwise)
        } else {
            run(&airplane, &op, 1, chordwise)
        }
        .expect("well-posed solve")
        .cl_lift
    };
    let (low, high) = (cl(1.0), cl(3.0));
    let lattice = 1.0 - low / ((high - low) / 2.0);
    let n = 4000_usize;
    let h = 1.0e-5;
    let mut sum = 0.0;
    for k in 0..n {
        let theta = (k as f64 + 0.5) * std::f64::consts::PI / n as f64;
        let x = 0.5 * (1.0 - theta.cos());
        let z = foil.local_camber(&[x - h, x + h]);
        sum += (z[1] - z[0]) / (2.0 * h) * (theta.cos() - 1.0) * std::f64::consts::PI / n as f64;
    }
    (lattice, -(sum / std::f64::consts::PI).to_degrees())
}

#[test]
fn the_camber_boundary_condition_reproduces_thin_airfoil_theory() {
    // The slope the normal carries is the one at the collocation point, not
    // the panel mean a quarter panel forward of it: with the panel mean the
    // zero-lift angle of a NACA 4412 wing is 5.6 % short at 8 chordwise
    // panels (Anderson eq. 4.61 on the section's own camber line).
    for chordwise in [8, 16] {
        let (lattice, theory) =
            lattice_and_thin_airfoil_zero_lift_angles(naca("naca4412"), chordwise, false);
        assert!(
            (lattice / theory - 1.0).abs() < 0.02,
            "{chordwise} panels: lattice {lattice:.3} deg vs theory {theory:.3} deg"
        );
    }
}

#[test]
fn the_frozen_reference_lattice_keeps_the_panel_mean_camber_slope() {
    let (lattice, theory) = lattice_and_thin_airfoil_zero_lift_angles(naca("naca4412"), 8, true);
    assert!(
        lattice / theory - 1.0 < -0.03,
        "lattice {lattice:.3} deg vs theory {theory:.3} deg"
    );
}

/// A T-tail's fin takes a strip boundary at the height of the stabilizer
/// root, so the stabilizer's root legs lie on the fin's own legs (AVL 3.52
/// vortex-spacing rule 1). The meshed fin has panel corners at exactly that
/// height; the same fin without the rule has none (the A400M's stabilizer
/// sits between two of its sections).
#[test]
fn a_t_tails_fin_has_a_strip_boundary_at_the_stabilizer_root() {
    use alas_geom::aircraft::airplane::HORIZONTAL_STABILIZER_NAME;
    use alas_geom::builder::AircraftBuilder;
    for name in ["A400M", "ATR72-600", "LJ60"] {
        let entry = alas_config::presets::get(name).expect("preset");
        let plane = AircraftBuilder::new(Some(entry.geometry.clone()))
            .build(Some(&entry.design_vector), true)
            .expect("builds");
        let index = |surface: &str| {
            plane
                .wings
                .iter()
                .position(|wing| wing.name == surface)
                .expect("a surface")
        };
        let (fin, stabilizer) = (
            index("Vertical Stabilizer"),
            index(HORIZONTAL_STABILIZER_NAME),
        );
        let height = plane.wings[stabilizer].xsecs[0].xyz_le[2];
        let system = VlmSystem::assemble(&plane, 1, 4).expect("assembles");
        let op = OperatingPoint::new(Atmosphere::new(0.0), 100.0, 2.0, 0.0, 0.0, 0.0, 0.0);
        let solved = system.solve(&op).expect("solves");
        let on_height = |corner: &[f64; 3]| (corner[2] - height).abs() < 1e-6;
        assert!(
            solved
                .panels
                .iter()
                .filter(|panel| panel.wing_index == fin)
                .any(|panel| on_height(&panel.front_left) || on_height(&panel.back_left)),
            "{name}: the fin has no strip edge at the stabilizer height {height}"
        );
    }
}

/// A wing and a tailplane behind it whose tip taper starts `offset_m`
/// outboard of the wing's trailing leg at y = 3 m; the tailplane carries
/// lift through its incidence.
fn wing_and_kinked_tailplane(offset_m: f64) -> Airplane {
    let main = Wing::new(
        "Main Wing",
        (0..=8)
            .map(|k| WingXSec::new([0.0, f64::from(k), 0.0], 2.0, 0.0, naca("naca0012")))
            .collect(),
        true,
    );
    let kink = 3.0 + offset_m;
    let mut tail = Wing::new(
        "Horizontal Stabilizer",
        vec![
            WingXSec::new([10.0, 0.0, 1.0], 1.2, 4.0, naca("naca0012")),
            WingXSec::new([10.0, 0.5 * kink, 1.0], 1.1, 4.0, naca("naca0012")),
            WingXSec::new([10.0, kink, 1.0], 1.0, 4.0, naca("naca0012")),
            WingXSec::new([10.3, kink + 0.2, 1.0], 0.5, 4.0, naca("naca0012")),
        ],
        true,
    );
    tail.twist_about_pitch_axis = true;
    let (s_ref, b_ref, c_ref) = (
        main.reference_area(),
        main.reference_span(),
        main.mean_aerodynamic_chord(),
    );
    Airplane {
        name: "Wing and kinked tailplane".to_owned(),
        xyz_ref: [0.5, 0.0, 0.0],
        wings: vec![main, tail],
        fuselages: vec![],
        s_ref,
        c_ref,
        b_ref,
    }
}

/// A wing trailing leg a fraction of a millimetre from a loft kink of the
/// tailplane must not cut a strip of that width: the leg makes no boundary
/// and the kink keeps its station. With the sliver the lattice cannot determine that strip's
/// circulation and the wake energy of the same loading jumps (a redesigned
/// A220-300: 24 to 109 counts at zero lift for 0.5 mm, which the design
/// search then selected as its best aircraft).
#[test]
fn a_wing_leg_beside_a_tailplane_loft_kink_does_not_cut_a_sliver_strip() {
    let op = level_flight_point(2.0);
    let solve = |offset_m: f64| {
        let plane = wing_and_kinked_tailplane(offset_m);
        let solved = run(&plane, &op, 1, 4).expect("solves");
        let narrowest = solved
            .panels
            .iter()
            .filter(|panel| panel.wing_index == 1)
            .map(|panel| (panel.front_right[1] - panel.front_left[1]).abs())
            .fold(f64::INFINITY, f64::min);
        let drag = trefftz_induced_drag_coefficient(&solved, &op, plane.s_ref).expect("wake drag");
        (narrowest, drag)
    };
    let (clear_strip, clear_drag) = solve(0.25);
    // Either side of the leg, down to a fraction of a millimetre: the tip
    // strip is 0.2 m wide, so a kink within 0.1 m gives way to the leg.
    for offset_m in [0.0005, -0.0003, 0.005, -0.004, 0.05] {
        let (strip, drag) = solve(offset_m);
        assert!(
            strip >= 0.09,
            "kink {offset_m} m from the leg: narrowest tail strip {strip} m"
        );
        assert!(
            (drag / clear_drag - 1.0).abs() < 0.02,
            "kink {offset_m} m from the leg: wake drag {drag} against {clear_drag}"
        );
    }
    assert!(clear_strip >= 0.09, "{clear_strip}");
}

#[test]
fn the_uncached_first_solve_matches_the_cached_later_solves_bitwise() {
    use alas_geom::aircraft::wing::WINGTIP_DEVICE_NAME;
    // The first solve of a system streams the near-field kernel row by row;
    // later solves read the stored table. Both must give the same bits, with
    // inter-surface cores in play (a winglet at the wing tip).
    // A pitch rate keeps every solve direct (no superposition), so the
    // comparison isolates the kernel.
    let airplane = winglet_airplane(1.0, WINGTIP_DEVICE_NAME);
    let system = VlmSystem::assemble(&airplane, 4, 4).expect("well-posed mesh");
    let mut op_point = level_flight_point(4.0);
    op_point.q = 0.02;
    let first = system.solve(&op_point).expect("well-posed solve");
    let second = system.solve(&op_point).expect("well-posed solve");
    let third = system.solve(&op_point).expect("well-posed solve");
    assert_eq!(first, second);
    assert_eq!(second, third);
}
