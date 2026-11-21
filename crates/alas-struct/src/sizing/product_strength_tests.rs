// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;
use alas_config::{materials, presets, AlasConfig};
use alas_geom::{builder::AircraftBuilder, wing_structure::WingStructureGeometry};

fn probe(name: &str) -> (AlasConfig, WingStructureGeometry) {
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
        .unwrap_or_else(|error| panic!("{name} loads: {error}"));
    let design = presets::registry()
        .iter()
        .find(|preset| preset.name == name)
        .map(|preset| preset.design_vector)
        .unwrap_or_else(|| panic!("{name} is registered"));
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&design), false)
        .unwrap_or_else(|error| panic!("{name} builds: {error}"));
    let wing = plane
        .wings
        .iter()
        .find(|wing| wing.name == "Main Wing")
        .unwrap_or_else(|| panic!("{name} has a main wing"));
    let root = wing.xsecs.first().expect("a root section");
    let tip = wing.xsecs.last().expect("a tip section");
    let (fractions, full_span) = config.structures.resolved_spars();
    let geometry = WingStructureGeometry::new(
        &design,
        &config.geometry.wing,
        &root.airfoil,
        &tip.airfoil,
        &fractions,
        Some(&full_span),
    )
    .unwrap_or_else(|error| panic!("{name} wing builds: {error}"));
    (config, geometry)
}

fn size_probe(config: &AlasConfig, geometry: &WingStructureGeometry) -> WingboxSizing {
    let cfg = &config.structures;
    super::super::size_wingbox(
        geometry,
        cfg,
        &config.requirements,
        materials::get(&cfg.skin_material).expect("skin material"),
        materials::get(&cfg.spar_web_material).expect("web material"),
        materials::get(&cfg.spar_cap_material).expect("cap material"),
        materials::get(&cfg.rib_material).expect("rib material"),
    )
}

fn assert_sizing_bits_equal(actual: &WingboxSizing, expected: &WingboxSizing, label: &str) {
    let equal_bits = |left: &[f64], right: &[f64]| {
        for (index, (left, right)) in left.iter().zip(right).enumerate() {
            assert_eq!(left.to_bits(), right.to_bits(), "{label}, element {index}");
        }
    };
    equal_bits(&actual.y_stations, &expected.y_stations);
    equal_bits(&actual.eta_stations, &expected.eta_stations);
    equal_bits(&actual.chord, &expected.chord);
    equal_bits(&actual.spar_fracs, &expected.spar_fracs);
    equal_bits(
        &[
            actual.t_skin,
            actual.rib_spacing_m,
            actual.total_mass_kg,
            actual.mass_breakdown_kg.spar_caps,
            actual.mass_breakdown_kg.spar_webs,
            actual.mass_breakdown_kg.skin,
            actual.mass_breakdown_kg.ribs,
        ],
        &[
            expected.t_skin,
            expected.rib_spacing_m,
            expected.total_mass_kg,
            expected.mass_breakdown_kg.spar_caps,
            expected.mass_breakdown_kg.spar_webs,
            expected.mass_breakdown_kg.skin,
            expected.mass_breakdown_kg.ribs,
        ],
    );
    for (actual, expected) in actual.spars.iter().zip(&expected.spars) {
        equal_bits(&[actual.chord_fraction], &[expected.chord_fraction]);
        equal_bits(&actual.t_web, &expected.t_web);
        equal_bits(&actual.lower_extra_area, &expected.lower_extra_area);
        equal_bits(&actual.h, &expected.h);
        equal_bits(&actual.w_cap, &expected.w_cap);
        equal_bits(&actual.t_cap, &expected.t_cap);
        equal_bits(&actual.a_cap, &expected.a_cap);
        equal_bits(&actual.frac_moment, &expected.frac_moment);
        equal_bits(&actual.margin_of_safety, &expected.margin_of_safety);
    }
    assert_eq!(actual, expected, "{label}");
}

#[test]
fn narrowed_cap_search_matches_the_legacy_section_to_the_search_tolerances() {
    for name in [
        "A320-200",
        "ATR72-600",
        "AVE",
        "B787-9",
        "A380-800",
        "A340-300",
        "A220-300",
        "DC-10",
    ] {
        let (config, geometry) = probe(name);
        let cfg = &config.structures;
        let skin = materials::get(&cfg.skin_material).expect("skin material");
        let web = materials::get(&cfg.spar_web_material).expect("web material");
        let cap = materials::get(&cfg.spar_cap_material).expect("cap material");
        let sizing = size_probe(&config, &geometry);
        for load_factor in [0.0, 0.01, 0.5, 1.0, 2.0, 100.0] {
            let load = crate::loads::elliptic_distributed_load(
                &sizing.y_stations,
                geometry.semi_span,
                load_factor * config.requirements.mtow_kg * config.requirements.gravity_m_s2,
            );
            let (shears, moments) =
                crate::loads::cantilever_shear_moment(&sizing.y_stations, &load);
            let mut expected = sizing.clone();
            for spar in &mut expected.spars {
                spar.t_web.fill(cfg.t_web_min_m);
            }
            let mut actual = expected.clone();
            super::reference::size(&mut expected, &moments, &shears, cfg, skin, web, cap);
            super::size(&mut actual, &moments, &shears, cfg, skin, web, cap, None);
            assert_sizing_close(
                &actual,
                &expected,
                &format!("{name}, load factor {load_factor}"),
            );
        }
    }
}

/// The product cap search stops at `CAP_AREA_RELATIVE_TOLERANCE` where the
/// legacy bisection ran to the last bit. A cap-area step can flip the web
/// bisection's last step and the skin search's golden-section comparison, so
/// station quantities agree to the gauge resolutions (skin 1e-4 relative) and
/// the box mass, stationary in the skin gauge, much closer.
fn assert_sizing_close(actual: &WingboxSizing, expected: &WingboxSizing, label: &str) {
    let close = |left: f64, right: f64, tolerance: f64, what: &str| {
        let scale = left.abs().max(right.abs()).max(1.0e-12);
        assert!(
            (left - right).abs() <= tolerance * scale,
            "{label}, {what}: {left} against {right}"
        );
    };
    close(
        actual.total_mass_kg,
        expected.total_mass_kg,
        1.0e-6,
        "box mass",
    );
    for (station, (left, right)) in actual
        .skin_gauge_m
        .iter()
        .zip(&expected.skin_gauge_m)
        .enumerate()
    {
        close(
            *left,
            *right,
            1.0e-3,
            &format!("skin gauge, station {station}"),
        );
    }
    // A cap is a small area beside its cover, and the two trade against each
    // other along a flat mass optimum: a cap's change is measured against the
    // station's cover area (gauge times chord), the material it trades with.
    for (left, right) in actual.spars.iter().zip(&expected.spars) {
        for station in 0..left.a_cap.len() {
            let cover = expected
                .skin_gauge_m
                .get(station)
                .map_or(expected.t_skin, |gauge| *gauge)
                * expected.chord[station];
            let scale = cover.max(left.a_cap[station].abs()).max(1.0e-12);
            assert!(
                (left.a_cap[station] - right.a_cap[station]).abs() <= 1.0e-3 * scale,
                "{label}, cap area, station {station}: {} against {} (cover {cover})",
                left.a_cap[station],
                right.a_cap[station]
            );
            close(
                left.t_web[station],
                right.t_web[station],
                1.0e-3,
                &format!("web gauge, station {station}"),
            );
        }
    }
}

#[test]
fn narrowed_cap_search_preserves_floor_widening_and_packaging_limits() {
    let (config, geometry) = probe("A320-200");
    let cfg = &config.structures;
    let skin = materials::get(&cfg.skin_material).expect("skin material");
    let web = materials::get(&cfg.spar_web_material).expect("web material");
    let cap = materials::get(&cfg.spar_cap_material).expect("cap material");
    let sizing = size_probe(&config, &geometry);
    for chord in [0.1, 2.0, 10.0] {
        for height in [0.001, 0.1, 1.0] {
            for minimum_gauge in [0.0, 1.0e-6, 0.005, 0.1] {
                for moment in [0.0, 1.0, 1.0e3, 1.0e6, 1.0e12] {
                    let mut cfg = cfg.clone();
                    cfg.t_skin_min_m = minimum_gauge;
                    let mut expected = sizing.clone();
                    expected.chord[0] = chord;
                    expected.t_skin = minimum_gauge;
                    for (index, spar) in expected.spars.iter_mut().enumerate() {
                        spar.h[0] = height * 0.5_f64.powi(index as i32);
                    }
                    let mut actual = expected.clone();
                    super::reference::size_station_caps(
                        &mut expected,
                        0,
                        moment,
                        &cfg,
                        skin,
                        web,
                        cap,
                        None,
                    );
                    super::size_station_caps(&mut actual, 0, moment, &cfg, skin, web, cap, None);
                    assert_sizing_bits_equal(
                        &actual,
                        &expected,
                        &format!(
                        "chord={chord}, height={height}, floor={minimum_gauge}, moment={moment}"),
                    );
                }
            }
        }
    }
}

#[test]
fn exhausted_cap_bracket_retains_the_last_evaluated_section() {
    let (config, geometry) = probe("A320-200");
    let mut cfg = config.structures.clone();
    cfg.t_skin_min_m = 0.0;
    let skin = materials::get(&cfg.skin_material).expect("skin material");
    let web = materials::get(&cfg.spar_web_material).expect("web material");
    let cap = materials::get(&cfg.spar_cap_material).expect("cap material");
    let mut expected = size_probe(&config, &geometry);
    expected.chord[0] = 10_000.0;
    expected.t_skin = 0.0;
    for spar in &mut expected.spars {
        spar.h[0] = 10_000.0;
    }
    let mut actual = expected.clone();
    super::reference::size_station_caps(&mut expected, 0, 1.0e40, &cfg, skin, web, cap, None);
    super::size_station_caps(&mut actual, 0, 1.0e40, &cfg, skin, web, cap, None);
    assert_sizing_bits_equal(&actual, &expected, "exhausted doubling bracket");
    let area = 1.0e-12 * 2.0_f64.powi(63);
    let (width, thickness) = super::station_cap_dimensions(area, 10_000.0, 10_000.0, 0.0);
    assert_eq!(actual.spars[0].w_cap[0].to_bits(), width.to_bits());
    assert_eq!(actual.spars[0].t_cap[0].to_bits(), thickness.to_bits());
}

fn sized_with_root_moment(moment: f64) -> WingboxSizing {
    let (config, geometry) = probe("A320-200");
    let cfg = &config.structures;
    let skin = materials::get(&cfg.skin_material).expect("skin material");
    let web = materials::get(&cfg.spar_web_material).expect("web material");
    let cap = materials::get(&cfg.spar_cap_material).expect("cap material");
    let mut sizing = size_probe(&config, &geometry);
    let count = sizing.y_stations.len();
    let mut moments = vec![0.0; count];
    moments[0] = moment;
    let shears = vec![0.0; count];
    super::size(&mut sizing, &moments, &shears, cfg, skin, web, cap, None);
    sizing
}

#[test]
fn an_undefined_station_moment_is_an_undefined_margin_and_a_failed_sizing() {
    let sizing = sized_with_root_moment(f64::NAN);
    for spar in &sizing.spars {
        assert!(spar.margin_of_safety[0].is_nan());
        // The gauge search stopped at its first trial instead of doubling.
        assert!(spar.t_web[0].is_finite());
        assert!(spar.t_web[0] < 1.0);
    }
    assert!(sizing.minimum_margin_of_safety().is_nan());
    assert!(!sizing.strength_margins_pass());
}

#[test]
fn max_or_nan_keeps_a_nan_operand_and_matches_max_for_numbers() {
    use super::super::section::max_or_nan;
    assert!(max_or_nan(f64::NAN, 2.0).is_nan());
    assert!(max_or_nan(2.0, f64::NAN).is_nan());
    for (a, b) in [(0.0, 1.5), (3.0, -1.0), (f64::INFINITY, 2.0), (0.7, 0.7)] {
        assert_eq!(max_or_nan(a, b).to_bits(), f64::max(a, b).to_bits());
    }
}

#[test]
fn a_station_no_web_gauge_can_carry_stops_at_the_chord_and_fails() {
    let sizing = sized_with_root_moment(1.0e30);
    let ceiling = sizing.chord[0] / sizing.spars.len() as f64;
    for spar in &sizing.spars {
        assert!(spar.t_web[0] <= ceiling, "{} > {ceiling}", spar.t_web[0]);
        assert!(spar.margin_of_safety[0] < 0.0);
    }
    assert!(!sizing.strength_margins_pass());
}

#[test]
#[ignore = "single-thread release timing probe; run explicitly with --nocapture"]
// Explicitly requested timing probe owns its console output.
#[allow(clippy::print_stdout)]
fn wingbox_sizing_microtiming() {
    for name in ["A320-200", "ATR72-600", "AVE", "B787-9"] {
        let (config, geometry) = probe(name);
        std::hint::black_box(size_probe(&config, &geometry));
        let mut best = f64::INFINITY;
        for _ in 0..3 {
            let start = std::time::Instant::now();
            for _ in 0..5 {
                std::hint::black_box(size_probe(&config, &geometry));
            }
            best = best.min(start.elapsed().as_secs_f64() / 5.0);
        }
        let sizing = size_probe(&config, &geometry);
        println!(
            "{name}: {:.6} ms, total_mass_kg={:.17}",
            best * 1000.0,
            sizing.total_mass_kg
        );
    }
}

#[test]
#[ignore = "single-thread release timing probe; run explicitly with --nocapture"]
// Explicitly requested timing probe owns its console output.
#[allow(clippy::print_stdout)]
fn product_strength_microtiming() {
    for name in ["A320-200", "ATR72-600", "AVE", "B787-9"] {
        let (config, geometry) = probe(name);
        let cfg = &config.structures;
        let skin = materials::get(&cfg.skin_material).expect("skin material");
        let web = materials::get(&cfg.spar_web_material).expect("web material");
        let cap = materials::get(&cfg.spar_cap_material).expect("cap material");
        let sizing = size_probe(&config, &geometry);
        let load = crate::loads::elliptic_distributed_load(
            &sizing.y_stations,
            geometry.semi_span,
            config.requirements.mtow_kg
                * config.requirements.gravity_m_s2
                * config.requirements.ultimate_load_factor
                * cfg.additional_safety_factor,
        );
        let (shears, moments) = crate::loads::cantilever_shear_moment(&sizing.y_stations, &load);
        for legacy in [true, false] {
            let mut candidate = sizing.clone();
            let mut best = f64::INFINITY;
            for _ in 0..3 {
                let start = std::time::Instant::now();
                for _ in 0..5 {
                    for spar in &mut candidate.spars {
                        spar.t_web.fill(cfg.t_web_min_m);
                    }
                    if legacy {
                        super::reference::size(
                            &mut candidate,
                            &moments,
                            &shears,
                            cfg,
                            skin,
                            web,
                            cap,
                        );
                    } else {
                        super::size(&mut candidate, &moments, &shears, cfg, skin, web, cap, None);
                    }
                    std::hint::black_box(&candidate);
                }
                best = best.min(start.elapsed().as_secs_f64() / 5.0);
            }
            println!(
                "{name}: legacy={legacy}, strength_pass_ms={:.6}",
                best * 1000.0
            );
        }
    }
}

#[test]
fn a_long_rib_bay_makes_the_stiffened_cover_critical_and_buys_flange_area() {
    // Torenbeek (2013) Eq. 11.71: the compressive allowable of the stiffened
    // cover falls with the rib pitch, so the same loads need more flange
    // material in a box with fewer ribs. Nothing here is tuned to a preset.
    let (config, geometry) = probe("A320-200");
    let base = size_probe(&config, &geometry);
    let mut sparse_config = config.clone();
    sparse_config.structures.num_ribs_override = Some(6);
    let sparse = size_probe(&sparse_config, &geometry);
    assert!(sparse.installed_rib_spacing_m() > 2.0 * base.installed_rib_spacing_m());
    // The covers' bending material is caps plus skin: the station gauge
    // search may buy the extra area as skin rather than flange.
    let cover =
        |sizing: &WingboxSizing| sizing.mass_breakdown_kg.spar_caps + sizing.mass_breakdown_kg.skin;
    assert!(
        cover(&sparse) > 1.2 * cover(&base),
        "caps and skin {} vs {}",
        cover(&sparse),
        cover(&base)
    );
    assert!(sparse.total_mass_kg.is_finite() && base.total_mass_kg.is_finite());
}

#[test]
fn the_default_aluminium_root_cover_is_nearly_strength_limited_at_its_own_rib_pitch() {
    // At the installed rib pitch the stiffened-panel curve stays below the
    // 380 MPa material allowable at the A380-class root, so panel buckling and
    // strength are both active there. Measured 374.8 MPa (98.6 %) with the
    // pull-up case alone; 345.9 MPa (91.0 %) once the torque shear flow
    // thickens the webs, which then share the bending and leave a thinner
    // cap and cover (CaseSet::ALL, torque of the aerodynamic load about the
    // box axis, crate::torsion).
    let (config, geometry) = probe("A380-800");
    let sizing = size_probe(&config, &geometry);
    let cfg = &config.structures;
    let skin = materials::get(&cfg.skin_material).expect("skin material");
    let panel = super::super::panel::CoverPanel::new(&sizing, 0, skin);
    let flange = super::super::panel::flange_area_m2(&sizing, 0);
    let allowable = panel.allowable_pa(skin, flange);
    assert!(allowable <= skin.f_allow_pa);
    assert!(allowable > 0.85 * skin.f_allow_pa, "{allowable}");
}

#[test]
fn carbon_covers_are_gauged_for_their_shear_and_every_gauge_stays_above_the_minimum() {
    for name in ["B787-9", "A350-900"] {
        let (config, geometry) = probe(name);
        let sizing = size_probe(&config, &geometry);
        let skin = materials::get(&config.structures.skin_material).expect("skin material");
        assert!(crate::allowables::is_carbon_laminate(skin), "{name}");
        assert_eq!(sizing.skin_gauge_m.len(), sizing.y_stations.len());
        assert!(sizing.skin_gauge_m.iter().all(|&t| t >= sizing.t_skin));
        // The shear flow of the minimum gauge exceeds the laminate's pure-shear
        // strength somewhere, so the skin must thicken there.
        let pure_shear = crate::allowables::laminate_shear_strength_pa(skin);
        let overloaded = (0..sizing.y_stations.len())
            .filter(|&j| sizing.cover_shear_flow_at(j) / sizing.t_skin > 0.5 * pure_shear)
            .count();
        assert!(overloaded > 0, "{name}: no station's shear matters");
        let thickened = sizing
            .skin_gauge_m
            .iter()
            .filter(|&&t| t > sizing.t_skin)
            .count();
        assert!(thickened > 0, "{name}: no station thickened");
        // Every station's skin shear stays inside the laminate envelope.
        for j in 0..sizing.y_stations.len() {
            let tau = sizing.cover_shear_flow_at(j) / sizing.skin_at(j);
            assert!(tau < pure_shear, "{name} station {j}: {tau} Pa");
        }
        assert!(sizing.strength_margins_pass(), "{name} sizes");
        // The box mass integrates the station gauges.
        let (front, rear) = (sizing.spar_fracs[0], *sizing.spar_fracs.last().unwrap());
        let uniform = trapezoid(
            &sizing
                .chord
                .iter()
                .map(|c| 2.0 * (rear - front) * c * sizing.t_skin * skin.rho_kg_m3)
                .collect::<Vec<_>>(),
            &sizing.y_stations,
        );
        assert!(sizing.mass_breakdown_kg.skin > uniform, "{name}");
    }
    for name in ["A320-200", "B747-400"] {
        let (config, geometry) = probe(name);
        let sizing = size_probe(&config, &geometry);
        assert!(
            sizing.skin_gauge_m.iter().all(|&t| t >= sizing.t_skin),
            "{name}"
        );
        assert!(sizing.strength_margins_pass(), "{name} sizes");
    }
}

#[test]
fn a_metal_station_whose_shear_exhausts_the_minimum_gauge_is_gauged_until_its_checks_hold() {
    // Knock-down audit: the unchanged constants (Niu 1988 p. 119 interaction,
    // 5.34/4.0, the Torenbeek cap, F_su and von Mises from Niu's gauge
    // columns) at a station whose torsional shear flow the 6 mm sheet cannot
    // carry: tau = 1.1 F_su(sheet) puts R_s above one and the von Mises limit
    // F_tu / sqrt(3) below tau, so no cap closes the station at the minimum.
    use crate::allowables::{shell_von_mises_allowable_at_pa, skin_shear_ultimate_at_pa};
    let (config, geometry) = probe("A320-200");
    let cfg = &config.structures;
    let skin = materials::get(&cfg.skin_material).expect("skin material");
    let web = materials::get(&cfg.spar_web_material).expect("web material");
    let cap = materials::get(&cfg.spar_cap_material).expect("cap material");
    assert_eq!(skin.name, "Al 7075-T6");
    let mut sizing = size_probe(&config, &geometry);
    let count = sizing.y_stations.len();
    let minimum = sizing.t_skin;
    let station = 0;
    let flow = 1.1 * skin_shear_ultimate_at_pa(skin, minimum) * minimum;
    sizing.cover_shear_flow_n_per_m[station] = flow;
    let at_minimum = flow / minimum;
    assert!(at_minimum > skin_shear_ultimate_at_pa(skin, minimum));
    assert!(at_minimum > shell_von_mises_allowable_at_pa(skin, minimum) / 3.0_f64.sqrt());
    let mut moments = vec![0.0; count];
    moments[station] = 1.0e6;
    let shears = vec![0.0; count];
    super::size(&mut sizing, &moments, &shears, cfg, skin, web, cap, None);
    let gauge = sizing.skin_at(station);
    assert!(gauge > minimum, "{gauge}");
    let tau = flow / gauge;
    // Above 0.249 in the T651 plate column governs: F_su 41 ksi, F_tu 77 ksi.
    assert!(gauge > 0.249 * 0.0254);
    assert!((skin_shear_ultimate_at_pa(skin, gauge) / 6.894_757_293_168e6 - 41.0).abs() < 1e-9);
    assert!(tau < skin_shear_ultimate_at_pa(skin, gauge));
    assert!(tau < shell_von_mises_allowable_at_pa(skin, gauge) / 3.0_f64.sqrt());
    for spar in &sizing.spars {
        assert!(
            spar.margin_of_safety[station] >= -super::super::MARGIN_NUMERICAL_ZERO,
            "{}",
            spar.margin_of_safety[station]
        );
    }
}

#[test]
fn stations_sized_in_parallel_are_bit_identical_to_stations_sized_in_turn() {
    for name in [
        "A320-200",
        "A220-300",
        "A340-300",
        "A380-800",
        "B787-9",
        "DC-10",
        "ATR72-600",
        "AVE",
        "LJ60",
        "C295",
        "A400M",
        "B747-400",
        "A350-900",
        "E195-E2",
        "C919",
    ] {
        let (config, geometry) = probe(name);
        let parallel = size_probe(&config, &geometry);
        SERIAL_STATIONS.with(|serial| serial.set(true));
        let in_turn = size_probe(&config, &geometry);
        SERIAL_STATIONS.with(|serial| serial.set(false));
        // `{:?}` prints every float in its shortest round-trip form, so equal
        // strings are equal bits throughout the section.
        assert_eq!(format!("{parallel:?}"), format!("{in_turn:?}"), "{name}");
    }
}

#[test]
fn a_hinted_web_search_retains_the_cold_gauge_to_the_search_tolerance() {
    let (config, geometry) = probe("A320-200");
    let cfg = &config.structures;
    let skin = materials::get(&cfg.skin_material).expect("skin material");
    let web = materials::get(&cfg.spar_web_material).expect("web material");
    let cap = materials::get(&cfg.spar_cap_material).expect("cap material");
    let sizing = size_probe(&config, &geometry);
    let feasible = 1.0 + super::super::MARGIN_NUMERICAL_ZERO;
    let mut sized_above_minimum = 0;
    for station in [0, sizing.chord.len() / 3, sizing.chord.len() / 2] {
        for (moment, shear) in [(2.0e5, 5.0e4), (2.0e6, 3.0e5), (6.0e6, 1.5e6)] {
            let mut cold = sizing.clone();
            let cold_gauge = super::size_station(
                &mut cold,
                station,
                moment,
                shear,
                cfg,
                skin,
                web,
                cap,
                super::size_station_caps,
                None,
            );
            sized_above_minimum += usize::from(cold_gauge > cfg.t_web_min_m);
            for factor in [0.3, 0.999, 1.0, 1.001, 3.0, 50.0] {
                let mut warm = sizing.clone();
                let gauge = super::size_station(
                    &mut warm,
                    station,
                    moment,
                    shear,
                    cfg,
                    skin,
                    web,
                    cap,
                    super::size_station_caps,
                    Some(factor * cold_gauge),
                );
                let label = format!("station {station}, moment {moment}, hint x{factor}");
                assert!(
                    (gauge - cold_gauge).abs() <= 2.0 * WEB_GAUGE_RELATIVE_TOLERANCE * cold_gauge,
                    "{label}: {gauge} against {cold_gauge}"
                );
                assert_eq!(
                    warm.spars[0].t_web[station].to_bits(),
                    gauge.to_bits(),
                    "{label}"
                );
                let peak = station_utilizations(&warm, station, moment, shear, skin, web, cap).0;
                let cold_peak =
                    station_utilizations(&cold, station, moment, shear, skin, web, cap).0;
                assert_eq!(peak <= feasible, cold_peak <= feasible, "{label}");
            }
        }
    }
    assert!(
        sized_above_minimum > 0,
        "every case stayed at the minimum gauge"
    );
}

#[test]
fn a_hinted_cap_search_retains_the_cold_area_to_the_search_tolerance() {
    let (config, geometry) = probe("A320-200");
    let cfg = &config.structures;
    let skin = materials::get(&cfg.skin_material).expect("skin material");
    let web = materials::get(&cfg.spar_web_material).expect("web material");
    let cap = materials::get(&cfg.spar_cap_material).expect("cap material");
    let sizing = size_probe(&config, &geometry);
    let feasible = 1.0 + super::super::MARGIN_NUMERICAL_ZERO;
    let tolerance = super::super::cap_search::CAP_AREA_RELATIVE_TOLERANCE;
    let mut capped = 0;
    for station in [0, sizing.chord.len() / 3, sizing.chord.len() / 2] {
        for (moment, shear) in [(2.0e5, 5.0e4), (2.0e6, 3.0e5), (6.0e6, 1.5e6)] {
            let mut cold = sizing.clone();
            let cold_area =
                super::size_station_caps(&mut cold, station, moment, cfg, skin, web, cap, None)
                    .expect("the production search returns its area");
            capped += usize::from(cold_area > 0.0);
            for factor in [0.3, 0.999, 1.0, 1.001, 3.0, 50.0] {
                let mut warm = sizing.clone();
                let hint = Some(factor * cold_area).filter(|hint| *hint > 0.0);
                let area =
                    super::size_station_caps(&mut warm, station, moment, cfg, skin, web, cap, hint)
                        .expect("the production search returns its area");
                let label = format!("station {station}, moment {moment}, hint x{factor}");
                assert!(
                    (area - cold_area).abs() <= 2.0 * tolerance * cold_area,
                    "{label}: {area} against {cold_area}"
                );
                let peak = station_utilizations(&warm, station, moment, shear, skin, web, cap).0;
                let cold_peak =
                    station_utilizations(&cold, station, moment, shear, skin, web, cap).0;
                assert_eq!(peak <= feasible, cold_peak <= feasible, "{label}");
            }
        }
    }
    assert!(capped > 0, "no case needed caps");
}
