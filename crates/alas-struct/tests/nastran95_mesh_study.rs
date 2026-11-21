// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! NASTRAN-95 mesh and layout study of the reference and product wingboxes
//!
//! # What the study shows, and what it does not claim
//!
//! The wingbox is sized once (the skin gauge, spar caps and webs do not depend on
//! the rib count; only the rib mass does, and the study prints the semi-wing
//! total). The mesh is then built from the sizing with a different number of rib
//! stations. In this model **the spanwise element density is the rib layout**:
//! `get_rib_stations(num_ribs, chordwise)` places the rib stations and the mesh
//! nodes on the same stations. There is no spanwise subdivision of a bay.
//!
//! * Raising the chordwise point count at a fixed layout changes the peak
//!   deflection by 3.1 %, 0.14 % and 1.3 % over 6, 10, 20 and 40 points: it is
//!   not a monotone sequence, so no convergence order can be derived from it.
//! * Raising the rib count scatters the peak by 14 % and not monotonically. The
//!   scatter follows the single long inboard skin strip: ribs 0 to `skin_start`
//!   form one spanwise row of `CQUAD4` elements 8-12 m long (22-33 % of the
//!   semi-span, with the transition ribs tied in only by `RBE3`), and its
//!   length changes with the rib count. The data run the way a discretisation
//!   artefact does: the longer that strip, the stiffer the wing (7 ribs, 33 %,
//!   4.57 m; 10 ribs, 22 %, 5.26 m). At equal strip length (5, 9 and 25 ribs,
//!   25 %) the peaks still differ by 5.6 %, so the strip length is not the only
//!   variable.
//!
//! The mesh is therefore **not converged in the spanwise direction** and the
//! scatter is a modelling defect (card MESH-SKIN-START: decouple the spanwise
//! element density from the rib stations, subdividing the root-`skin_start` skin
//! strip, then redo a spanwise-by-chordwise refinement study at a fixed layout).
//! The tests below characterise that state with sanity bounds; they do not assert
//! a convergence the model does not have.
//!
//! # Two different wingboxes
//!
//! The cross-solver fixture and `nastran95_build.json` use the **reference
//! compatibility** box (the frozen law ported from the reference scripts), whose
//! 9-rib pull-up tip deflection is about 5.1 m (14 % of the 35.875 m semi-span).
//! The **product** box (`size_wingbox` + `build_wing_mesh_bdf_product`, the deck the
//! application runs) deflects about 10.0 m (28 %) under the same ultimate load:
//! the 4.6 m that task N1G was asked about belongs to the frozen reference law,
//! not to the product.
//!
//! # Load level and plausibility band
//!
//! The `pull-up` case is the ultimate case (`n_ult` times MTOW/2, with inertial
//! relief). The project rule STRUCT-DEFLECTION (`plausibility-rules.md`) puts the
//! limit-load tip deflection of transport wings at 10-20 % of the semi-span
//! (Megson, *Aircraft Structures*, ch. 12), i.e. 15-30 % at ultimate load
//! (ultimate = 1.5 x limit). The product box is asserted against the rule's own criterion (below 25 % at limit, HIGH above 40 %); the 10-20 % source figure (Megson, not verified on the page) is printed only; the
//! frozen reference box (13-15 % at ultimate) is below it and is recorded, not
//! asserted.
//!
//! Full-scale wing tests, the figures as reported (none from a test report):
//! the aluminium 777 static wing was pulled "more than 24 ft" (7.3 m, about 24 %
//! of its 30.5 m semi-span) in the January 1995 destruct test (Aviation Week &
//! Space Technology, 30 January 1995, "777 passes full-scale wing destruct
//! test"); Boeing's archive caption and the Seattle Times give the load reached as
//! 154 % (of limit), which puts the limit-load deflection near 15 %. Composite
//! wings, from secondary reports: Boeing 787 about 7.6 m on about 30 m (25 %),
//! Airbus A380 about 7.5 m on about 40 m (19 %), Airbus A350 more than 5 m on
//! about 32.4 m. The product box (28-30 % at ultimate on a 35.9 m semi-span) is
//! more flexible than every cited test wing (777 aluminium 24 %, 787 25 %, A380 19 %); the
//! frozen reference box (13-15 %) is about 0.6 of the 777.
//!
//! All tests need `ALAS_NASTRAN95_DIR` (+ `ALAS_NASTRAN95_RUNTIME`,
//! `ALAS_NASTRAN95_RF_STAGE`) and honour `ALAS_NASTRAN95_SCRATCH`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)]

mod support;

use std::path::{Path, PathBuf};

use alas_config::{DesignRequirements, EngineConfig, MassModelConfig, StructuresConfig};
use alas_struct::mesh::{build_wing_mesh_bdf, build_wing_mesh_bdf_product};
use alas_struct::nastran95::{
    build_static_deck, read_displacement_tables, run_nastran95, run_nastran95_analysis, Dialect,
    Nastran95Solver, RunOutcome,
};
use alas_struct::sizing::{size_wingbox, size_wingbox_reference_compatibility};
use support::{build_geometry, materials_for, MaterialsRecord};

/// Rib count the box is sized at; also the rib layout of the density series.
const SIZING_RIBS: i64 = 9;

/// Limit-load tip deflection of transport wings, share of the semi-span, and the
/// ultimate-to-limit factor (STRUCT-DEFLECTION in `plausibility-rules.md`).
const SOURCE_BAND: (f64, f64) = (0.10, 0.20);
const RULE_LIMIT: f64 = 0.25;
const ULTIMATE_OVER_LIMIT: f64 = 1.5;

struct Solved {
    grids: usize,
    peak_grid: i64,
    peak_t3: f64,
    semi_span: f64,
    mass_kg: f64,
}

fn materials() -> MaterialsRecord {
    MaterialsRecord {
        skin: "Al 7075-T6".to_string(),
        web: "Al 7075-T6".to_string(),
        cap: "Al 7075-T6".to_string(),
        rib: "Al 7075-T6".to_string(),
    }
}

/// An emptied work directory below the configured scratch root.
fn scratch(label: &str) -> PathBuf {
    let base = std::env::var("ALAS_NASTRAN95_SCRATCH").unwrap_or_else(|_| {
        if cfg!(windows) {
            "C:/nas-run".to_owned()
        } else {
            "/tmp/nas-run".to_owned()
        }
    });
    let path = Path::new(&base).join(label);
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("create the work directory");
    path
}

/// Size the reference box once at `SIZING_RIBS`, mesh at `mesh_ribs` ribs and
/// `chordwise` points, solve the static deck with NASTRAN-95 alone.
fn solve_reference(mesh_ribs: i64, chordwise: i64, solver: &Nastran95Solver) -> Solved {
    let req = DesignRequirements::default();
    let cfg = StructuresConfig {
        num_ribs_override: Some(SIZING_RIBS),
        mesh_chordwise_points: chordwise,
        n_modes: 6,
        ..StructuresConfig::default()
    };
    let wsg = build_geometry(&[0.15, 0.60], &[true, true]);
    let [skin, web, cap, rib] = materials_for(&materials());
    let mut sizing = size_wingbox_reference_compatibility(&wsg, &cfg, &req, skin, web, cap, rib);
    let mass_kg = sizing.total_mass_kg;
    sizing.num_ribs = mesh_ribs;
    let (deck, _report, node_index) = build_wing_mesh_bdf(
        &wsg,
        &sizing,
        &cfg,
        &EngineConfig::default(),
        &MassModelConfig::default(),
        &req,
        skin,
        web,
        cap,
        rib,
    )
    .expect("the wingbox meshes without a fatal health finding");
    let text = build_static_deck(&deck, &node_index, &req, &cfg, Dialect::Nastran95);
    let work = scratch(&format!("study_r{mesh_ribs}c{chordwise}"));
    let RunOutcome::Print(print) = run_nastran95(solver, &text, &work, 600.0) else {
        panic!("ribs={mesh_ribs} cw={chordwise}: NASTRAN-95 solve failed");
    };
    let (peak_grid, peak_t3) = read_displacement_tables(&print)[0]
        .iter()
        .map(|&(grid, row)| (grid, row[2]))
        .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
        .expect("a displacement table");
    Solved {
        grids: deck.grids().len(),
        peak_grid,
        peak_t3,
        semi_span: wsg.semi_span,
        mass_kg,
    }
}

fn solver() -> Nastran95Solver {
    Nastran95Solver::from_env().expect("set ALAS_NASTRAN95_DIR (+ ALAS_NASTRAN95_RUNTIME)")
}

/// Characterise the reference box: chordwise series at nine ribs, rib series at
/// 20 points. Only sanity bounds are asserted (see the module note): the series
/// are not convergent, and the bounds say how large the recorded scatter is.
#[test]
#[ignore = "needs ALAS_NASTRAN95_DIR: a live NASTRAN-95 install"]
fn the_reference_box_mesh_series_are_characterised_not_converged() {
    let solver = solver();
    let mut density = Vec::new();
    for cw in [6, 10, 20, 40] {
        let s = solve_reference(SIZING_RIBS, cw, &solver);
        eprintln!(
            "DENSITY ribs={SIZING_RIBS} cw={cw} grids={} peak_grid={} peak_t3={:.6} m",
            s.grids, s.peak_grid, s.peak_t3
        );
        density.push(s.peak_t3.abs());
    }
    let mut layout = Vec::new();
    for ribs in [5, 7, 9, 10, 12, 14, 16, 20, 25] {
        let s = solve_reference(ribs, 20, &solver);
        eprintln!(
            "LAYOUT ribs={ribs} grids={} semi-wing sizing mass={:.1} kg peak_grid={} peak_t3={:.6} m ({:.2} % of the {:.3} m semi-span)",
            s.grids,
            s.mass_kg,
            s.peak_grid,
            s.peak_t3,
            100.0 * s.peak_t3.abs() / s.semi_span,
            s.semi_span
        );
        layout.push(s.peak_t3.abs());
    }
    let spread = |values: &[f64]| {
        let (min, max) = values
            .iter()
            .fold((f64::MAX, 0.0_f64), |(lo, hi), &p| (lo.min(p), hi.max(p)));
        (max - min) / (0.5 * (max + min))
    };
    eprintln!(
        "SPREAD density {:.1} %, layout {:.1} %",
        100.0 * spread(&density),
        100.0 * spread(&layout)
    );
    // Recorded: 4.6 % over the chordwise series and 14.0 % over the rib series.
    assert!(
        spread(&density) < 0.08,
        "chordwise sensitivity grew (MESH-SKIN-START owns this ratchet)"
    );
    assert!(
        spread(&layout) < 0.20,
        "rib-layout scatter grew (MESH-SKIN-START owns this ratchet)"
    );
}

/// The product box (the deck the application runs) through the production
/// NASTRAN-95 analysis: the ultimate-load tip deflection against the
/// STRUCT-DEFLECTION band, with the stresses printed beside the allowable.
#[test]
#[ignore = "needs ALAS_NASTRAN95_DIR: a live NASTRAN-95 install"]
fn the_product_box_tip_deflection_is_inside_the_limit_load_band() {
    let solver = solver();
    let req = DesignRequirements::default();
    let wsg = build_geometry(&[0.15, 0.60], &[true, true]);
    let [skin, web, cap, rib] = materials_for(&materials());
    for (ribs, chordwise) in [(9, 10), (16, 20)] {
        let cfg = StructuresConfig {
            num_ribs_override: Some(ribs),
            mesh_chordwise_points: chordwise,
            run_sol_static: true,
            run_sol_modes: false,
            run_sol_vibration_sine: false,
            run_sol_vibration_random: false,
            n_modes: 6,
            ..StructuresConfig::default()
        };
        let sizing = size_wingbox(&wsg, &cfg, &req, skin, web, cap, rib);
        let (deck, _report, node_index) = build_wing_mesh_bdf_product(
            &wsg,
            &sizing,
            &cfg,
            &EngineConfig::default(),
            &MassModelConfig::default(),
            &req,
            skin,
            web,
            cap,
            rib,
        )
        .expect("the product wingbox meshes");
        let work = scratch(&format!("product_r{ribs}c{chordwise}"));
        let results = run_nastran95_analysis(&deck, &node_index, &cfg, &req, &work, &solver);
        let st = &results.static_solve;
        assert!(st.error.is_none(), "{:?}", st.error);
        let tip = st
            .tip_deflection_m
            .get("pull-up")
            .expect("a pull-up tip deflection")
            .abs();
        let assessed = st.assessed_von_mises_max_pa.get("pull-up");
        let fraction = tip / wsg.semi_span;
        eprintln!(
            "PRODUCT ribs={ribs} cw={chordwise} semi-wing sizing mass={:.1} kg tip={tip:.3} m ({:.1} % of the semi-span at ultimate, {:.1} % at limit) assessed von Mises {:?} MPa against the {:.0} MPa allowable",
            sizing.total_mass_kg,
            100.0 * fraction,
            100.0 * fraction / ULTIMATE_OVER_LIMIT,
            assessed.map(|v| (v / 1.0e6).round()),
            skin.f_allow_pa / 1.0e6
        );
        let at_limit = fraction / ULTIMATE_OVER_LIMIT;
        eprintln!(
            "source band {:.0}-{:.0} % at limit; asserted criterion: below {:.0} %",
            100.0 * SOURCE_BAND.0,
            100.0 * SOURCE_BAND.1,
            100.0 * RULE_LIMIT
        );
        assert!(
            at_limit < RULE_LIMIT,
            "ribs={ribs} cw={chordwise}: the limit-load tip deflection is {:.1} % of the semi-span, above the {:.0} % criterion of STRUCT-DEFLECTION",
            100.0 * at_limit,
            100.0 * RULE_LIMIT
        );
    }
}
