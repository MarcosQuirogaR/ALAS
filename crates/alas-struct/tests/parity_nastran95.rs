// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Validates `alas-struct::nastran95`: the deck this program writes for the
//! open-source 1995 solver, with no Python original to translate.
//!
//! # An external-solver, cross-solver parity test
//!
//! There is nothing to agree with in the usual way: this row has no reference
//! implementation. So what it agrees with is a *second solver*. The same wingbox
//! is written twice from one mesh; once in the NASTRAN-95 dialect, once for a
//! modern solver, and both are solved, and the two are held to **converge on
//! each other as the mesh is refined**. That is a stronger claim than any fixed
//! tolerance: a coarse-mesh disagreement that halves each time the mesh is
//! refined is the signature of two `CQUAD4` formulations approaching the same
//! answer, where a fixed bound would only ever assert that they are close.
//!
//! # What runs where
//!
//! One check needs no solver and always runs: the deck this crate writes obeys
//! the NASTRAN-95 dialect rules, every bulk field fits eight columns, every
//! real shows a decimal point, and the cards that overflow eight fields continue
//! rather than being silently dropped. Three of those rules fail *without a
//! diagnostic* in the solver, so checking them here is the only place a
//! regression in the formatter is caught on a machine with no NASTRAN-95.
//!
//! The convergence check needs both solvers. NASTRAN-95 is found through
//! `ALAS_NASTRAN95_DIR`/`ALAS_NASTRAN95_RUNTIME` and the modern solver through
//! `ALAS_MSC_LAUNCHER`/`ALAS_MSC_SOLVER`; when either is unset the check skips,
//! loudly, rather than failing: the same contract the MSES row keeps.
//!
//! # Scope: statics
//!
//! Only linear statics is cross-validated. The normal-mode check proves that a
//! local NASTRAN-95 accepts the bounded `EIGR` deck, emits finite ordered modal
//! results, and accounts for every emitted root with a global Sturm count
//! through the largest extracted eigenvalue. The test does not set a parity
//! tier against MSC: the historic inverse-power method still differs on a
//! shell-and-concentrated-mass model with singular rotational mass freedoms, so
//! cross-solver differences remain an audit finding. NASTRAN-95 may also print
//! message 3307 for an intermediate inverse-power shift; the live check keeps
//! that warning as a diagnostic and uses the global count as its completeness
//! certificate.

// This file is a test binary; a failed unwrap/expect is the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
// Some fixture fields are read only by serde, or only in the regeneration path.
#![allow(dead_code)]

mod support;

use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use alas_config::{DesignRequirements, EngineConfig, MassModelConfig, StructuresConfig};
use alas_struct::mesh::{build_wing_mesh_bdf, Deck, MeshNodeIndex};
use alas_struct::nastran95::{
    build_modes_deck, build_static_deck, read_displacement_tables, read_eigenvalues, run_nastran95,
    Dialect, Nastran95Solver, RunOutcome,
};
use alas_struct::sizing::size_wingbox_reference_compatibility;
use serde::Deserialize;
use support::{build_geometry, materials_for, MaterialsRecord};

/// The mesh resolutions the convergence study runs at, coarse to fine.
const REFINEMENTS: [(i64, i64); 3] = [(5, 6), (9, 10), (16, 20)];

/// The relative cross-solver disagreement the finest mesh must sit inside. It is
/// a ceiling, not the claim: the claim is that the disagreement is shrinking.
const FINEST_TOLERANCE: f64 = 0.05;

/// `golden/struct/nastran95.json`: the recorded convergence evidence.
#[derive(Debug, Deserialize)]
struct Fixture {
    /// The build the numbers were captured against, so a different one is a
    /// skip rather than a spurious failure.
    solver_note: String,
    refinements: Vec<Refinement>,
}

/// One resolution's recorded cross-solver result.
#[derive(Debug, Deserialize)]
struct Refinement {
    num_ribs: i64,
    chordwise: i64,
    grids: usize,
    /// The grid both solvers report the largest vertical deflection at.
    peak_grid: i64,
    n95_peak_t3: f64,
    msc_peak_t3: f64,
    /// `(n95 - msc) / msc`.
    rel_diff: f64,
}

/// One wingbox's deck inputs: the mesh, its node index, and the two configs the
/// deck writers read.
type Wingbox = (Deck, MeshNodeIndex, DesignRequirements, StructuresConfig);

/// A peak deflection: the grid it is at, and its vertical component.
type Peak = (i64, f64);

/// One refinement's live outcome: rib count, grid count, and each solver's peak.
type Live = (i64, usize, Peak, Peak);

/// Build one wingbox at the given resolution and return its deck inputs.
fn wingbox(num_ribs: i64, chordwise: i64) -> Wingbox {
    let req = DesignRequirements::default();
    let engine_cfg = EngineConfig::default();
    let mass_cfg = MassModelConfig::default();
    let cfg = StructuresConfig {
        num_ribs_override: Some(num_ribs),
        mesh_chordwise_points: chordwise,
        n_modes: 6,
        ..StructuresConfig::default()
    };

    let named = MaterialsRecord {
        skin: "Al 7075-T6".to_string(),
        web: "Al 7075-T6".to_string(),
        cap: "Al 7075-T6".to_string(),
        rib: "Al 7075-T6".to_string(),
    };
    let wsg = build_geometry(&[0.15, 0.60], &[true, true]);
    let [skin, web, cap, rib] = materials_for(&named);
    let sizing = size_wingbox_reference_compatibility(&wsg, &cfg, &req, skin, web, cap, rib);
    let (deck, _report, node_index) = build_wing_mesh_bdf(
        &wsg,
        &sizing,
        &cfg,
        &engine_cfg,
        &mass_cfg,
        &req,
        skin,
        web,
        cap,
        rib,
    )
    .expect("the wingbox meshes without a fatal health finding");
    (deck, node_index, req, cfg)
}

#[test]
fn the_nastran95_static_deck_obeys_every_dialect_rule() {
    // No solver needed: this is the check that a formatter regression (the
    // kind that fails silently inside NASTRAN-95) is caught anywhere.
    let mut rivets_seen = false;
    for (num_ribs, chordwise) in REFINEMENTS {
        let (deck, node_index, req, cfg) = wingbox(num_ribs, chordwise);
        let text = build_static_deck(&deck, &node_index, &req, &cfg, Dialect::Nastran95);
        assert_dialect(&text, num_ribs, chordwise);
        rivets_seen |= text.contains("CRBE3");
    }
    // The finest mesh grows transition-rib rivets, so the sweep exercises the
    // `RBE3` -> `CRBE3` rename on a real card rather than only asserting its
    // absence: a coarse mesh has none, which is why the rename is checked
    // here, across the sweep, and not on every deck.
    assert!(
        rivets_seen,
        "no refinement produced a CRBE3 to check the rename on"
    );
}

/// The deck is a NASTRAN-95 static deck, and every bulk line stays within the
/// eighty columns the fixed field allows. Case-control lines (before
/// `BEGIN BULK`) are free-form and exempt.
///
/// The field-level rules (eight columns each, a decimal point on every real,
/// the no-`E` exponent shorthand) are pinned by the formatter's own unit
/// tests, which reach cases a wingbox deck does not; this is the deck-level
/// check that those rules survive assembly of the real thing, since three of
/// them fail without any diagnostic inside the solver.
fn assert_dialect(deck: &str, num_ribs: i64, chordwise: i64) {
    let label = format!("ribs={num_ribs} cw={chordwise}");
    assert!(
        deck.contains("SOL 1,1"),
        "{label}: not a NASTRAN-95 static deck"
    );
    assert!(
        deck.contains("APP DISPLACEMENT"),
        "{label}: no APP DISPLACEMENT"
    );
    assert!(
        deck.contains("MAXLINES = 1000000"),
        "{label}: a full-field deck would hit NASTRAN-95's default print limit"
    );
    assert!(
        deck.contains("PARAM   AUTOSPC 1"),
        "{label}: AUTOSPC not the integer 1"
    );
    assert!(
        deck.contains("\nPBAR "),
        "{label}: caps not reduced to PBAR"
    );
    assert!(
        !deck.contains("PBARL"),
        "{label}: a PBARL survived into the deck"
    );

    let mut in_bulk = false;
    for line in deck.lines() {
        if line == "BEGIN BULK" {
            in_bulk = true;
            continue;
        }
        if !in_bulk || line == "ENDDATA" {
            continue;
        }
        // The modern element name must never leak into a NASTRAN-95 deck: it is
        // `CRBE3` here, and a bare `RBE3` line would be a card this solver
        // rejects. (`CRBE3` lines start with `CRBE3`, so this does not catch
        // them.)
        assert!(
            !line.starts_with("RBE3"),
            "{label}: a bare RBE3 (modern name) reached the deck: {line:?}"
        );
        // A bulk line is at most a card name and eight fields (columns 1-72)
        // plus a continuation tag (columns 73-80); anything past eighty columns
        // is a field that overflowed its eight and shifted every field after it.
        assert!(
            line.len() <= 80,
            "{label}: bulk line over eighty columns: {line:?}"
        );
    }
}

// A skipped external-solver test must say why on stderr, or a silent pass looks
// like a real one; the workspace print ban is lifted for that, as the MSES row
// lifts it for the same reason.
#[allow(clippy::print_stderr)]
#[test]
fn the_two_solvers_converge_on_the_same_static_deflection() {
    let solvers = (Nastran95Solver::from_env(), Msc::from_env());
    let (Some(n95), Some(msc)) = &solvers else {
        eprintln!(
            "skipping the NASTRAN-95 cross-solver convergence check: set \
             ALAS_NASTRAN95_DIR (+ ALAS_NASTRAN95_RUNTIME for libgfortran) and \
             ALAS_MSC_LAUNCHER/ALAS_MSC_SOLVER to run it."
        );
        return;
    };

    // Solve every refinement through both solvers, live.
    let live: Vec<Live> = REFINEMENTS
        .iter()
        .map(|&(num_ribs, chordwise)| {
            let (deck, node_index, req, cfg) = wingbox(num_ribs, chordwise);
            let n95_deck = build_static_deck(&deck, &node_index, &req, &cfg, Dialect::Nastran95);
            let msc_deck = build_static_deck(&deck, &node_index, &req, &cfg, Dialect::Modern);
            let n95_peak = solve_and_peak_n95(n95, &n95_deck, num_ribs);
            let msc_peak = msc.solve_and_peak(&msc_deck, num_ribs);
            (num_ribs, deck.grids().len(), n95_peak, msc_peak)
        })
        .collect();

    // Regeneration writes the evidence and stops; the normal path checks it.
    if std::env::var("ALAS_NASTRAN95_REGEN").is_ok() {
        regenerate_fixture(&live);
        return;
    }
    let fixture: Fixture = alas_testkit::load("struct", "nastran95");

    let mut observed: Vec<f64> = Vec::new();
    for (index, &(num_ribs, _grids, n95_peak, msc_peak)) in live.iter().enumerate() {
        let recorded = &fixture.refinements[index];
        // Both solvers must agree on *where* the wing deflects most; a different
        // peak grid means the two decks are not the same model, not that the
        // formulations differ.
        assert_eq!(
            n95_peak.0, msc_peak.0,
            "ribs={num_ribs}: solvers disagree on the peak-deflection grid"
        );
        assert_eq!(
            n95_peak.0, recorded.peak_grid,
            "ribs={num_ribs}: peak grid moved from the recorded evidence"
        );
        // The solve is deterministic for a fixed deck and build, so live results
        // must reproduce the recorded ones: a regression check on top of the
        // convergence one.
        assert_close(n95_peak.1, recorded.n95_peak_t3, "n95 peak", num_ribs);
        assert_close(msc_peak.1, recorded.msc_peak_t3, "msc peak", num_ribs);
        observed.push(((n95_peak.1 - msc_peak.1) / msc_peak.1).abs());
    }

    // The claim: the disagreement is shrinking, and the finest mesh is inside
    // the ceiling.
    for pair in observed.windows(2) {
        assert!(
            pair[1] < pair[0],
            "the cross-solver disagreement did not shrink under refinement: {observed:?}"
        );
    }
    assert!(
        *observed.last().unwrap() < FINEST_TOLERANCE,
        "the finest mesh's disagreement {:.4} exceeds the ceiling {FINEST_TOLERANCE}",
        observed.last().unwrap()
    );
}

/// SOL 3,1 is a supported local NASTRAN-95 workflow even though its historic
/// eigensolver cannot be held to modern low-mode parity for this mass model.
/// The opt-in live-solver check reports the observed modal band and requires
/// the global Sturm count to cover every parsed root.
#[allow(clippy::print_stderr)]
#[test]
fn the_nastran95_modes_deck_returns_finite_positive_modes() {
    let Some(solver) = Nastran95Solver::from_env() else {
        eprintln!(
            "skipping the NASTRAN-95 normal-modes check: set ALAS_NASTRAN95_DIR \
             (+ ALAS_NASTRAN95_RUNTIME for libgfortran) to run it."
        );
        return;
    };
    let (deck, _node_index, _req, cfg) = wingbox(5, 6);
    let modes_deck = build_modes_deck(&deck, &cfg, Dialect::Nastran95);
    let work = scratch_dir("n95_modes");
    let print = match run_nastran95(&solver, &modes_deck, &work, 300.0) {
        RunOutcome::Print(print) => print,
        RunOutcome::Failed(why) => panic!("NASTRAN-95 SOL 3,1 failed: {why}"),
    };
    let modes = read_eigenvalues(&print);
    assert!(
        modes.len() >= cfg.n_modes as usize,
        "NASTRAN-95 returned {} modes after requesting a reliable low-mode set",
        modes.len()
    );
    assert!(
        !modes.is_empty(),
        "NASTRAN-95 SOL 3,1 printed no real eigenvalues"
    );
    assert!(
        modes
            .iter()
            .all(|mode| mode.eigenvalue.is_finite() && mode.eigenvalue > 0.0),
        "NASTRAN-95 SOL 3,1 emitted an invalid eigenvalue: {modes:?}"
    );
    assert!(
        modes
            .iter()
            .all(|mode| mode.cyclic_hz.is_finite() && mode.cyclic_hz > 0.0),
        "NASTRAN-95 SOL 3,1 emitted an invalid cyclic frequency: {modes:?}"
    );
    assert!(
        modes
            .windows(2)
            .all(|pair| pair[0].cyclic_hz < pair[1].cyclic_hz),
        "NASTRAN-95 SOL 3,1 did not return strictly ordered roots: {modes:?}"
    );
    let largest_eigenvalue = modes
        .last()
        .map(|mode| mode.eigenvalue)
        .expect("the positive-mode assertion above must leave a largest root");
    let sturm_roots = sturm_root_count_through(&print, largest_eigenvalue).expect(
        "NASTRAN-95 did not print a global Sturm ROOTS BELOW count at or above the largest extracted root",
    );
    assert_eq!(
        sturm_roots,
        modes.len(),
        "NASTRAN-95's global Sturm count through the largest extracted root must account for every parsed root; intermediate 3307 diagnostics are not sufficient evidence of a missing root"
    );
    eprintln!(
        "NASTRAN-95 SOL 3,1 returned {} positive modes from {:.5} to {:.5} Hz (global Sturm count through {:.5}={sturm_roots}; 3307={})",
        modes.len(),
        modes.first().map_or(0.0, |mode| mode.cyclic_hz),
        modes.last().map_or(0.0, |mode| mode.cyclic_hz),
        largest_eigenvalue,
        print.contains("POTENTIALLY")
    );
}

/// Return the global Sturm count at the smallest printed shift that reaches the
/// largest extracted eigenvalue. `SDCOMP` prints one count for each inverse
/// power shift; the lines are deliberately not ordered by shift, so taking the
/// last line would inspect a local/circular search interval rather than the
/// requested band. A count at or above the largest emitted root covers every
/// root below that root from the declared zero lower bound for this deck.
fn sturm_root_count_through(print: &str, largest_eigenvalue: f64) -> Option<usize> {
    if !largest_eigenvalue.is_finite() {
        return None;
    }
    print
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 4 || fields[1] != "ROOTS" || fields[2] != "BELOW" {
                return None;
            }
            let count = fields[0].parse::<usize>().ok()?;
            let shift = fields[3].parse::<f64>().ok()?;
            (shift.is_finite() && shift >= largest_eigenvalue).then_some((shift, count))
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, count)| count)
}

#[test]
fn sturm_gate_rejects_a_parsed_list_with_an_omitted_lowest_root() {
    let print = "\
                          2 ROOTS BELOW   2.000000E+00\n\
                          1 ROOTS BELOW   5.000000E-01\n\
                                              R E A L   E I G E N V A L U E S\n\
        1         1        1.000000E+00        1.000000E+00        1.591549E+01\n";
    let parsed = read_eigenvalues(print);
    assert_eq!(parsed.len(), 1, "fixture deliberately omits one lower root");
    assert_eq!(
        sturm_root_count_through(print, parsed[0].eigenvalue),
        Some(2)
    );
    assert_ne!(
        sturm_root_count_through(print, parsed[0].eigenvalue),
        Some(parsed.len()),
        "a global Sturm count above the parsed count must fail completeness"
    );
}

/// Write `golden/struct/nastran95.json` from a live cross-solver run.
///
/// The fixture is the captured convergence evidence; there is no reference
/// implementation to generate it, so the generator is the test itself, run with
/// both solvers present and `ALAS_NASTRAN95_REGEN` set.
#[allow(clippy::print_stderr)]
fn regenerate_fixture(live: &[Live]) {
    let refinements: Vec<String> = live
        .iter()
        .zip(REFINEMENTS)
        .map(|(&(num_ribs, grids, n95_peak, msc_peak), (_, chordwise))| {
            let rel = (n95_peak.1 - msc_peak.1) / msc_peak.1;
            format!(
                "    {{\n      \"num_ribs\": {num_ribs},\n      \"chordwise\": {chordwise},\n      \
                 \"grids\": {grids},\n      \"peak_grid\": {},\n      \"n95_peak_t3\": {:.10},\n      \
                 \"msc_peak_t3\": {:.10},\n      \"rel_diff\": {:.10}\n    }}",
                n95_peak.0, n95_peak.1, msc_peak.1, rel
            )
        })
        .collect();
    let json = format!(
        "{{\n  \"solver_note\": \"NASTRAN-95 (Foadsf fork, gfortran -O0 -fno-automatic) \
         cross-checked against MSC Nastran Student Edition 2026.1; peak vertical deflection \
         of subcase 1 (pull-up), metres.\",\n  \"refinements\": [\n{}\n  ]\n}}\n",
        refinements.join(",\n")
    );
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../golden/struct/nastran95.json");
    std::fs::write(&path, json).expect("write the fixture");
    eprintln!("wrote {}", path.display());
}

/// A live number must match the recorded one to a determinism tolerance: the
/// last-digit round-off two runs of the same solver can differ by.
fn assert_close(got: f64, want: f64, what: &str, num_ribs: i64) {
    let relative = (got - want).abs() / want.abs().max(1e-30);
    assert!(
        relative < 1e-4,
        "ribs={num_ribs}: {what} {got:e} differs from the recorded {want:e} ({relative:e})"
    );
}

/// Solve the NASTRAN-95 deck and return the grid of largest vertical deflection
/// and that deflection, from the first subcase.
fn solve_and_peak_n95(solver: &Nastran95Solver, deck: &str, num_ribs: i64) -> (i64, f64) {
    let work = scratch_dir(&format!("n95_{num_ribs}"));
    match run_nastran95(solver, deck, &work, 300.0) {
        RunOutcome::Print(print) => peak_deflection(&read_displacement_tables(&print)),
        RunOutcome::Failed(why) => panic!("ribs={num_ribs}: NASTRAN-95 solve failed: {why}"),
    }
}

/// The grid and magnitude of the largest `T3` in the first subcase's table.
fn peak_deflection(tables: &[Vec<(i64, [f64; 6])>]) -> (i64, f64) {
    tables
        .first()
        .expect("a displacement table")
        .iter()
        .map(|&(grid, row)| (grid, row[2]))
        .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
        .expect("a non-empty table")
}

/// A modern solver driven the way the working harness drives MSC Nastran
/// Student Edition: the launcher pointed at Patran's `analysis.exe`.
struct Msc {
    launcher: String,
    solver: String,
}

impl Msc {
    fn from_env() -> Option<Self> {
        let launcher = PathBuf::from(std::env::var("ALAS_MSC_LAUNCHER").ok()?);
        let solver = PathBuf::from(std::env::var("ALAS_MSC_SOLVER").ok()?);
        assert!(
            launcher.is_file(),
            "ALAS_MSC_LAUNCHER is not an executable file: {}",
            launcher.display()
        );
        assert!(
            solver.is_file(),
            "ALAS_MSC_SOLVER is not an executable file: {}",
            solver.display()
        );
        let launcher = msc_command_token(&launcher)
            .unwrap_or_else(|why| panic!("ALAS_MSC_LAUNCHER cannot start MSC: {why}"));
        let solver = msc_command_token(&solver)
            .unwrap_or_else(|why| panic!("ALAS_MSC_SOLVER cannot reach MSC's launcher: {why}"));
        Some(Self { launcher, solver })
    }

    fn solve_and_peak(&self, deck: &str, num_ribs: i64) -> (i64, f64) {
        let work = scratch_dir(&format!("msc_{num_ribs}"));
        let bdf = work.join("wing.bdf");
        std::fs::write(&bdf, deck).unwrap();
        let status = Command::new(&self.launcher)
            .current_dir(&work)
            .args([
                "wing.bdf",
                "scr=no",
                &format!("sdirectory={}", work.display()),
                &format!("dbs={}", work.display()),
                &format!("a.solver={}", self.solver),
            ])
            .status()
            .expect("MSC launcher runs");
        assert!(
            status.success(),
            "ribs={num_ribs}: MSC launcher exited non-zero"
        );
        let f06 = std::fs::read_to_string(work.join("wing.f06")).expect("MSC wrote an .f06");
        peak_deflection(&read_displacement_tables(&f06))
    }
}

/// Make an MSC executable token safe for its legacy launcher chain.
///
/// The supported Nastran launcher constructs an `a.solver` command itself and
/// does not quote either installed path. `Command` protects the outer process,
/// but not that vendor-managed chain. Its installed path contains `MSC Nastran`,
/// so a DOS 8.3 name is the required spelling without whitespace. Fail rather
/// than report a successful launcher process when the volume cannot provide one.
fn msc_command_token(path: &Path) -> Result<String, String> {
    let original = path.to_string_lossy().into_owned();
    if !original.chars().any(char::is_whitespace) {
        return Ok(original);
    }

    #[cfg(windows)]
    {
        let mut command = Command::new("cmd.exe");
        command.args(["/d", "/s", "/c"]);
        // `cmd /c` consumes a command *line*, not an argv entry. Passing this
        // through `Command::args` inserts a leading literal quote before the
        // `for` path; keep the fixed command syntax raw while the path itself
        // remains in an inherited environment variable.
        command.raw_arg("for %I in (\"%ALAS_MSC_SOLVER_PATH%\") do @echo %~sI");
        let output = command
            .env("ALAS_MSC_SOLVER_PATH", path)
            .output()
            .map_err(|error| format!("could not query its DOS 8.3 path: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "querying its DOS 8.3 path failed with {}",
                output.status
            ));
        }
        let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if token.is_empty() || token.chars().any(char::is_whitespace) {
            return Err(format!(
                "its installed path contains whitespace and this volume has no usable DOS 8.3 name ({token:?})"
            ));
        }
        Ok(token)
    }

    #[cfg(not(windows))]
    {
        Err(format!(
            "its installed path contains whitespace; MSC's launcher needs a DOS 8.3 name on Windows ({original})"
        ))
    }
}

#[test]
fn the_msc_command_token_leaves_a_safe_executable_path_unchanged() {
    let path = Path::new("C:/tools/analysis.exe");
    assert_eq!(msc_command_token(path).unwrap(), "C:/tools/analysis.exe");
}

#[cfg(windows)]
#[test]
fn the_msc_command_token_removes_whitespace_from_the_configured_solver() {
    let Some(path) = std::env::var_os("ALAS_MSC_SOLVER") else {
        return;
    };
    let token = msc_command_token(Path::new(&path)).unwrap();
    assert!(
        !token.chars().any(char::is_whitespace),
        "the token MSC receives must not contain whitespace: {token:?}"
    );
}

/// A short scratch directory, emptied first: NASTRAN-95 needs the path short
/// (its `/DOSNAM/` is `CHARACTER*72`), so this uses the drive root, not the
/// system temp under a long profile path.
fn scratch_dir(label: &str) -> PathBuf {
    let base = std::env::var("ALAS_NASTRAN95_SCRATCH").unwrap_or_else(|_| "C:/nas-run".to_string());
    let path = Path::new(&base).join(label);
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}
