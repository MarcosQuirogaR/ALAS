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
    let output_nodes = node_index
        .spar_upper_nids
        .first()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let requested = config.n_modes.max(1);
    let target = requested.min(MODAL_TARGET_CONFIRMED) as usize;
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
        let print = match solve_and_retain(solver, work_dir, SOL103, &text, config) {
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
        if confirmed >= target {
            break;
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
