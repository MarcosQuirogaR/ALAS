// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares `alas-aero::mses` against `alas/physics/mses_analysis.py`, via
//! `golden/generators/gen_aero_mses.py`.
//!
//! # An external-solver parity test
//!
//! Every other parity test compares numbers this workspace computed against
//! numbers the reference computed. This one is different: both sides drive the
//! same compiled `mset`/`mses`/`mplot` binaries, so the numbers come out of the
//! tool, not out of either implementation. That is checkable at `Tier::Exact`
//! because MSES is deterministic -- an identical mesh and `mses.case` deck
//! produce byte-identical output -- so what is really being compared is the
//! orchestration and the parsing: does a Rust-built deck driving the same
//! binary reproduce the reference's result exactly.
//!
//! # What runs where
//!
//! The deck check needs no binary and always runs: the section the reference
//! actually fed to `mset` (its post-repanel coordinates) is in the fixture, and
//! `Airfoil::write_dat` on it must reproduce, byte for byte, the `airfoil.dat`
//! AeroSandbox wrote. That is the one `exact` check available on a machine
//! without MSES.
//!
//! The end-to-end check needs the binaries. MSES is licensed separately by MIT
//! and is not bundled with this repository, so the test finds them through the
//! `ALAS_MSES_DIR` environment variable and skips (rather than fails) when it
//! is unset. It also skips, loudly, when the local binaries' digests do not
//! match the build the fixture was generated against: the `exact` claim holds
//! only for the same binary, and a different MSES build is not a translation
//! error to report as forty wrong numbers.

// This file is a test binary; a failed unwrap/expect is the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::path::{Path, PathBuf};

use alas_aero::mses::{Mses, MsesPressureResult};
use alas_config::MsesConfig;
use alas_geom::asb::airfoil::Airfoil;
use alas_testkit::{Comparison, Tier};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct Fixture {
    binaries: HashMap<String, BinaryInfo>,
    polar: Vec<PolarCase>,
    pressure: Vec<PressureCase>,
}

#[derive(Deserialize)]
struct BinaryInfo {
    fnv1a64: String,
}

#[derive(Deserialize)]
struct CfgFixture {
    n_crit: f64,
    xtr_upper: f64,
    xtr_lower: f64,
    max_iterations: i64,
    mset_n: i64,
    mset_e: f64,
    timeout_mset_s: f64,
    timeout_mses_s: f64,
    alpha_sweep_halfwidth_deg: f64,
    alpha_sweep_n_points: i64,
}

#[derive(Deserialize)]
struct PolarCase {
    name: String,
    airfoil_name: String,
    mach: f64,
    reynolds: f64,
    config: CfgFixture,
    alphas_requested: Vec<f64>,
    repaneled_coordinates: Vec<[f64; 2]>,
    airfoil_dat: String,
    result: PolarResultFixture,
}

#[derive(Deserialize)]
struct PolarResultFixture {
    status: String,
    airfoil_name: String,
    mach: f64,
    reynolds: f64,
    alpha_deg: Vec<f64>,
    #[serde(rename = "CL")]
    cl: Vec<f64>,
    #[serde(rename = "CD")]
    cd: Vec<f64>,
    #[serde(rename = "CM")]
    cm: Vec<f64>,
    #[serde(rename = "CDv")]
    cdv: Vec<f64>,
    #[serde(rename = "CDw")]
    cdw: Vec<f64>,
    xtr_top: Vec<f64>,
    xtr_bot: Vec<f64>,
}

#[derive(Deserialize)]
struct PressureCase {
    name: String,
    airfoil_name: String,
    mach: f64,
    reynolds: f64,
    alpha_deg: f64,
    config: CfgFixture,
    repaneled_coordinates: Vec<[f64; 2]>,
    airfoil_dat: String,
    result: PressureResultFixture,
}

#[derive(Deserialize)]
struct PressureResultFixture {
    status: String,
    alpha_deg: f64,
    x_upper: Vec<f64>,
    cp_upper: Vec<f64>,
    mach_upper: Vec<f64>,
    x_lower: Vec<f64>,
    cp_lower: Vec<f64>,
    mach_lower: Vec<f64>,
    field_x: Vec<f64>,
    field_y: Vec<f64>,
    field_mach: Vec<f64>,
    airfoil_x: Vec<f64>,
    airfoil_y: Vec<f64>,
}

/// FNV-1a, 64-bit, as `gen_aero_mses.py`'s `_fnv1a64` computes it -- the same
/// digest `parity_selig.rs`/`parity_neuralfoil.rs` use.
fn fnv1a64(data: &[u8]) -> String {
    const OFFSET_BASIS: u64 = 0xCBF2_9CE4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01B3;
    let mut digest = OFFSET_BASIS;
    for &byte in data {
        digest ^= u64::from(byte);
        digest = digest.wrapping_mul(PRIME);
    }
    format!("{digest:016x}")
}

fn airfoil_from(name: &str, coordinates: &[[f64; 2]]) -> Airfoil {
    Airfoil::from_coordinates(name, coordinates.iter().map(|&[x, y]| (x, y)).collect())
}

fn config(cfg: &CfgFixture, mses_dir: &Path) -> MsesConfig {
    MsesConfig {
        enabled: true,
        mses_dir: mses_dir.display().to_string(),
        n_crit: cfg.n_crit,
        xtr_upper: cfg.xtr_upper,
        xtr_lower: cfg.xtr_lower,
        max_iterations: cfg.max_iterations,
        mset_n: cfg.mset_n,
        mset_e: cfg.mset_e,
        timeout_mset_s: cfg.timeout_mset_s,
        timeout_mses_s: cfg.timeout_mses_s,
        alpha_sweep_halfwidth_deg: cfg.alpha_sweep_halfwidth_deg,
        alpha_sweep_n_points: cfg.alpha_sweep_n_points,
    }
}

/// The MSES executables folder, or `None` when it is not configured/present.
fn locate_mses_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var("ALAS_MSES_DIR").ok()?);
    (dir.join("mset.exe").exists() && dir.join("mses.exe").exists()).then_some(dir)
}

/// Whether the local binaries are the build the fixture was generated against.
fn binaries_match(fixture: &Fixture, mses_dir: &Path) -> bool {
    fixture.binaries.iter().all(|(name, info)| {
        std::fs::read(mses_dir.join(name)).is_ok_and(|bytes| fnv1a64(&bytes) == info.fnv1a64)
    })
}

#[test]
fn generated_airfoil_dat_matches_aerosandbox_byte_for_byte() {
    // No binary needed: the recorded coordinates are what the reference fed to
    // mset, so `write_dat` on them must reproduce the exact file it wrote. This
    // is the deck-generation half of the row's `exact` tier.
    let fixture: Fixture = alas_testkit::load("aero", "mses");
    let mut deck = Comparison::new("mses.airfoil_dat", Tier::Exact);
    for case in &fixture.polar {
        let dat = airfoil_from(&case.airfoil_name, &case.repaneled_coordinates).write_dat();
        deck.exact(&format!("polar/{}", case.name), &dat, &case.airfoil_dat);
    }
    for case in &fixture.pressure {
        let dat = airfoil_from(&case.airfoil_name, &case.repaneled_coordinates).write_dat();
        deck.exact(&format!("pressure/{}", case.name), &dat, &case.airfoil_dat);
    }
    deck.finish();
}

// A skipped external-solver test has to say why on stderr -- "no MSES here",
// "wrong MSES build" -- or a silent pass looks like a real one. That is what
// eprintln is for here, so the workspace print ban is lifted for this test.
#[allow(clippy::print_stderr)]
#[ignore = "requires the exact installed MSES/mset/mplot build used for the golden fixture"]
#[test]
fn mses_runs_reproduce_the_reference_exactly() {
    let fixture: Fixture = alas_testkit::load("aero", "mses");

    let Some(mses_dir) = locate_mses_dir() else {
        eprintln!(
            "skipping MSES end-to-end parity: set ALAS_MSES_DIR to the folder \
             holding mset.exe/mses.exe/mplot.exe to run it (MSES is licensed \
             separately by MIT and is not bundled here)"
        );
        return;
    };
    if !binaries_match(&fixture, &mses_dir) {
        eprintln!(
            "skipping MSES end-to-end parity: the binaries under {} are not the \
             build golden/aero/mses.json was generated against, so an exact \
             numeric comparison would be meaningless",
            mses_dir.display()
        );
        return;
    }

    for case in &fixture.polar {
        let airfoil = airfoil_from(&case.airfoil_name, &case.repaneled_coordinates);
        let cfg = config(&case.config, &mses_dir);
        let got = Mses::new(airfoil, &cfg, &mses_dir).polar(
            &case.alphas_requested,
            case.reynolds,
            case.mach,
        );
        let reference = &case.result;

        let mut compare = Comparison::new(format!("mses.polar/{}", case.name), Tier::Exact);
        compare
            .exact("status", &got.status.as_str().to_owned(), &reference.status)
            .exact("airfoil_name", &got.airfoil_name, &reference.airfoil_name)
            .scalar("mach", got.mach, reference.mach)
            .scalar("reynolds", got.reynolds, reference.reynolds)
            .slice("alpha_deg", &got.alpha_deg, &reference.alpha_deg)
            .slice("CL", &got.cl, &reference.cl)
            .slice("CD", &got.cd, &reference.cd)
            .slice("CM", &got.cm, &reference.cm)
            .slice("CDv", &got.cdv, &reference.cdv)
            .slice("CDw", &got.cdw, &reference.cdw)
            .slice("xtr_top", &got.xtr_top, &reference.xtr_top)
            .slice("xtr_bot", &got.xtr_bot, &reference.xtr_bot);
        compare.finish();
    }

    for case in &fixture.pressure {
        let airfoil = airfoil_from(&case.airfoil_name, &case.repaneled_coordinates);
        let cfg = config(&case.config, &mses_dir);
        let got = Mses::new(airfoil, &cfg, &mses_dir).pressure(
            case.alpha_deg,
            case.reynolds,
            case.mach,
            &[0.0, 0.5, -0.5, 1.0, -1.0],
        );
        let reference = &case.result;

        let mut compare = Comparison::new(format!("mses.pressure/{}", case.name), Tier::Exact);
        compare
            .exact("status", &got.status.as_str().to_owned(), &reference.status)
            .scalar("alpha_deg", got.alpha_deg, reference.alpha_deg)
            .slice("x_upper", &got.x_upper, &reference.x_upper)
            .slice("cp_upper", &got.cp_upper, &reference.cp_upper)
            .slice("mach_upper", &got.mach_upper, &reference.mach_upper)
            .slice("x_lower", &got.x_lower, &reference.x_lower)
            .slice("cp_lower", &got.cp_lower, &reference.cp_lower)
            .slice("mach_lower", &got.mach_lower, &reference.mach_lower)
            .slice("field_x", &got.field_x, &reference.field_x)
            .slice("field_y", &got.field_y, &reference.field_y)
            .slice("field_mach", &got.field_mach, &reference.field_mach)
            .slice("airfoil_x", &got.airfoil_x, &reference.airfoil_x)
            .slice("airfoil_y", &got.airfoil_y, &reference.airfoil_y);
        compare.finish();

        assert!(!got.raw_bl_dump.is_empty());
        assert!(!got.raw_flowfield_dump.is_empty());
        assert!(!got.field_row_offsets.is_empty());
        let airfoil_coordinates = got
            .airfoil_x
            .iter()
            .copied()
            .zip(got.airfoil_y.iter().copied())
            .collect::<Vec<_>>();
        let replayed = MsesPressureResult::replay_raw_exports(
            got.alpha_deg,
            got.raw_bl_dump.clone(),
            got.raw_flowfield_dump.clone(),
            &airfoil_coordinates,
        )
        .unwrap_or_else(|error| panic!("replay retained mplot exports: {error}"));
        assert_eq!(replayed, got);
    }
}
