// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The NASTRAN-95 normal-modes band search: narrow inverse-power bands audited
//! by Sturm counts (see [`super::completeness`]).

use std::path::Path;

use alas_config::StructuresConfig;

use super::analysis::{read_modes_print_limited, solve_and_retain, PRODUCT_UNITS};
use super::completeness::{confirmed_prefix, read_sturm_counts, roots_missed_below_lowest};
use super::deck::{build_modes_deck_banded, ModalBand};
use super::{read_eigenvalues, Dialect, Nastran95Solver};
use crate::mesh::{Deck, MeshNodeIndex};
use crate::nastran::{ModesResult, ResultStatus};

const SOL103: &str = "sol103";

/// Search bands tried by [`solve_modes`], Hz, narrowest first. Inverse power
/// places its shifts across the whole band, so a band that contains the roots
/// wanted *and few others* is what returns the lowest set; a wide band returns
/// an arbitrary subset (see `eigenvalue_card`).
const MODAL_BAND_LADDER_HZ: [f64; 5] = [4.0, 8.0, 16.0, 32.0, 64.0];

/// Sturm-confirmed lowest roots that end the band search. Four covers the
/// first bending, torsion and the next two modes the structural report
/// compares; a larger target forces wide bands whose inverse-power sets are
/// patchy and, on a B747-400 wingbox, cost minutes per band.
const MODAL_TARGET_CONFIRMED: i64 = 4;

/// Roots the structural-modes figure would like confirmed, beyond
/// [`MODAL_TARGET_CONFIRMED`]. The search only reaches for them after the
/// default four are confirmed (so a wider band never costs the default result),
/// only while [`MODAL_FIGURE_BUDGET_S`] allows another band, and falls back to
/// the best confirmed prefix when it does not get there. The environment
/// variable [`MODAL_FIGURE_TARGET_ENV`] overrides the count; `0` or any value
/// at or below the default disables the extension.
const MODAL_FIGURE_TARGET: i64 = 6;

/// Environment override of [`MODAL_FIGURE_TARGET`].
const MODAL_FIGURE_TARGET_ENV: &str = "ALAS_N95_FIGURE_MODES";

/// Wall-clock allowance, seconds, for the whole band search before the
/// figure extension stops widening. A further band is started only if the
/// time already spent plus twice the last band's run time fits inside it.
const MODAL_FIGURE_BUDGET_S: f64 = 420.0;

static FIGURE_REQUESTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Declare that the structural-modes figure will be generated from this
/// process's NASTRAN-95 modal solves (the GUI, or the CLI with plots). Only
/// then does the search reach for [`MODAL_FIGURE_TARGET`] roots; matrix,
/// optimisation and other batch runs keep the default
/// [`MODAL_TARGET_CONFIRMED`].
pub fn request_modes_figure(requested: bool) {
    FIGURE_REQUESTED.store(requested, std::sync::atomic::Ordering::Relaxed);
}

fn figure_target() -> i64 {
    if !FIGURE_REQUESTED.load(std::sync::atomic::Ordering::Relaxed) {
        return MODAL_TARGET_CONFIRMED;
    }
    std::env::var(MODAL_FIGURE_TARGET_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .unwrap_or(MODAL_FIGURE_TARGET)
        .min(8)
}

/// Fewest roots requested per band (`ND` and `NE`).
const MODAL_MIN_EXTRACTED: i64 = 12;

/// How many times a band that skipped its lowest roots may be re-aimed at the
/// frequency of its lowest reported root.
const MODAL_MAX_NARROWINGS: usize = 2;

/// Extract the lowest elastic modes with NASTRAN-95 inverse power and prove
/// that they are the lowest.
///
/// Bands are tried narrowest first ([`MODAL_BAND_LADDER_HZ`]). After each run
/// the printed Sturm `ROOTS BELOW` counts audit the reported roots
/// ([`confirmed_prefix`]): the run is accepted once the lowest
/// [`MODAL_TARGET_CONFIRMED`] roots (or the requested number, if smaller) are
/// confirmed gap-free. A band that skipped its lowest roots is re-aimed at the
/// frequency of its lowest reported root; a band holding too few confirmed
/// roots is widened. Only the confirmed prefix is reported, so the result is
/// never an arbitrary subset presented as the lowest modes.
pub(super) fn solve_modes(
    solver: &Nastran95Solver,
    work_dir: &Path,
    deck: &Deck,
    node_index: &MeshNodeIndex,
    config: &StructuresConfig,
) -> ModesResult {
    // Eigenvectors are printed at the front-spar grids (the shape the report
    // has always drawn) and at the rear-spar grids, so bending and twist can
    // be separated.
    let mut output_nodes: Vec<i64> = node_index
        .spar_upper_nids
        .first()
        .cloned()
        .unwrap_or_default();
    if node_index.spar_upper_nids.len() >= 2 {
        if let Some(rear) = node_index.spar_upper_nids.last() {
            output_nodes.extend(rear.iter().copied());
        }
    }
    let output_nodes = output_nodes.as_slice();
    let requested = config.n_modes.max(1);
    let target = requested.min(MODAL_TARGET_CONFIRMED) as usize;
    let stretch = requested.min(figure_target()).max(target as i64) as usize;
    let started = std::time::Instant::now();
    let mut last_run_s: f64;
    let mut extract = (target as i64 + 4).max(MODAL_MIN_EXTRACTED);
    let mut ladder = 0;
    let mut upper_hz = MODAL_BAND_LADDER_HZ[0];
    let mut narrowings = 0;
    let mut best: Option<(String, usize)> = None;
    let mut skipped_below_lowest = None;
    loop {
        let band = ModalBand {
            upper_hz,
            estimate: extract,
            extract,
        };
        let text = build_modes_deck_banded(
            deck,
            config,
            Dialect::Nastran95,
            output_nodes,
            PRODUCT_UNITS,
            band,
        );
        let run_started = std::time::Instant::now();
        let solved = solve_and_retain(solver, work_dir, SOL103, &text, config);
        last_run_s = run_started.elapsed().as_secs_f64();
        let print = match solved {
            Ok(print) => print,
            Err(detail) => {
                return ModesResult {
                    status: ResultStatus::Error,
                    error: Some(detail),
                    ..ModesResult::default()
                }
            }
        };
        let modes = read_eigenvalues(&print);
        let counts = read_sturm_counts(&print);
        let confirmed = confirmed_prefix(&modes, &counts);
        let lowest_hz = modes
            .iter()
            .map(|mode| mode.cyclic_hz)
            .filter(|hz| hz.is_finite() && *hz > 0.0)
            .fold(f64::INFINITY, f64::min);
        let missed = roots_missed_below_lowest(&modes, &counts);
        if best.as_ref().is_none_or(|(_, count)| confirmed > *count) {
            best = Some((print, confirmed));
        }
        if confirmed >= stretch {
            break;
        }
        if confirmed >= target {
            // The default is met; a wider band is attempted for the figure
            // only if the budget allows, and `best` already holds the
            // confirmed prefix to fall back on.
            if started.elapsed().as_secs_f64() + 2.0 * last_run_s > MODAL_FIGURE_BUDGET_S {
                break;
            }
        }
        if confirmed == 0 && missed > 0 && lowest_hz.is_finite() {
            skipped_below_lowest = Some((missed, lowest_hz));
            if narrowings >= MODAL_MAX_NARROWINGS {
                break;
            }
            narrowings += 1;
            upper_hz = lowest_hz;
            extract = (missed as i64 + 4).max(MODAL_MIN_EXTRACTED);
            continue;
        }
        ladder += 1;
        match MODAL_BAND_LADDER_HZ.get(ladder) {
            Some(&next) if next > upper_hz => upper_hz = next,
            _ => break,
        }
    }
    match best {
        Some((print, confirmed)) if confirmed > 0 => {
            read_modes_print_limited(&print, deck, node_index, requested, confirmed)
        }
        _ => ModesResult {
            status: ResultStatus::Error,
            error: Some(match skipped_below_lowest {
                Some((missed, hz)) => format!(
                    "NASTRAN-95 inverse power skipped {missed} root(s) below its lowest reported \
                     root ({hz:.3} Hz) and the Sturm audit did not recover them; no modal set is \
                     reported"
                ),
                None => format!(
                    "NASTRAN-95 inverse power returned no Sturm-confirmed eigenvalue below {:.0} Hz",
                    MODAL_BAND_LADDER_HZ[MODAL_BAND_LADDER_HZ.len() - 1]
                ),
            }),
            ..ModesResult::default()
        },
    }
}
