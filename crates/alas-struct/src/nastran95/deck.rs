// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The cross-solver decks: one wingbox written for two solvers so the pair can
//! be held to converge on each other.
//!
//! Both decks are produced here, from the same mesh, differing only where the
//! two dialects genuinely differ, which is the point, because a difference the
//! two solvers then report is one of *those* differences and nothing else. The
//! shared bulk is emitted once, in fixed eight-column fields, and the [`Dialect`]
//! selects the handful of cards that change:
//!
//! * **Executive and case control.** NASTRAN-95 opens `APP DISPLACEMENT` and
//!   `SOL 1,1`/`SOL 3,1`; the modern deck is `SOL 101`/`SOL 103`.
//! * **`PARAM,AUTOSPC`** is the integer `1` for NASTRAN-95: what gives its
//!   `CQUAD4` the drilling stiffness that keeps the stiffness matrix
//!   non-singular, and `YES` for the modern solver.
//! * **`RBE3` is `CRBE3`** in NASTRAN-95: the same element, the same fields,
//!   a different name, as that solver's own manual documents.
//! * **The eigensolver.** The modern `EIGRL` extracts the lowest modes of a
//!   possibly-singular mass matrix directly; NASTRAN-95's Givens methods cannot,
//!   so normal modes use `EIGR,,INV` over a frequency band, the inverse-power
//!   method that finds roots in a range without a positive-definite mass.
//!
//! Two cards the modern deck would normally keep are written the NASTRAN-95 way
//! for *both*, deliberately. The spar caps are `PBAR` on both, from
//! [`super::section`]'s reduction, so the two decks model an identical beam and
//! the reduction itself is validated separately against a modern `PBARL`. And
//! the mesh's large-field `RBE3` is not reused for the modern deck: MSC's input
//! processor rejects that card's bare continuation (`USER FATAL 316`), so this
//! module emits it small-field, which both solvers accept.

mod beam_cards;
mod cards;
#[cfg(test)]
mod tests;

pub(super) use cards::{bulk as benchmark_bulk, card as benchmark_card};
use cards::{bulk, card, eigenvalue_card, param, static_loads};

use alas_config::{DesignRequirements, StructuresConfig};

pub(crate) use super::units::UnitScale;

use super::field::{Card, ContinuationTags, Field};
use crate::loads::{self, LoadCase};
use crate::mesh::{Deck, MeshNodeIndex, ParamValue};
use crate::nastran::elliptic_forces_by_y;

/// Which solver a deck is written for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dialect {
    /// The open-source 1995 solver: `APP DISPLACEMENT`, `SOL 1,1`/`SOL 3,1`,
    /// `CRBE3`, `PARAM,AUTOSPC,1`, `EIGR,,INV`.
    Nastran95,
    /// A modern solver: `SOL 101`/`SOL 103`, `RBE3`, `PARAM,AUTOSPC,YES`,
    /// `EIGRL`.
    Modern,
}

/// The constraint set the mesh writes, one `SPC1`, set 1.
const SPC_SET: i64 = 1;

/// The eigenvalue-extraction set the modes deck selects.
const METHOD_SET: i64 = 1;

/// The legacy default modal band for the public deck builders: the lowest
/// sixteen roots estimated over 0-100 Hz. It is *not* what the product path
/// uses (see [`ModalBand`] and `analysis::modal_band`): a band this wide lets
/// inverse power skip the lowest elastic roots.
const NASTRAN95_MODAL_SEARCH_UPPER_HZ: f64 = 100.0;

/// The inverse solver's root estimate must exceed the requested modal count;
/// a four-to-one estimate is the legacy default.
const NASTRAN95_MODAL_ROOT_ESTIMATE_FACTOR: i64 = 4;

/// The legacy builders extract at least sixteen roots.
const NASTRAN95_MINIMUM_EXTRACTED_MODES: i64 = 16;

/// One `EIGR,,INV` search: roots in `[0, upper_hz]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ModalBand {
    /// Upper edge of the search interval, Hz.
    pub upper_hz: f64,
    /// `NE`: estimated number of roots in the interval.
    pub estimate: i64,
    /// `ND`: number of roots wanted.
    pub extract: i64,
}

impl ModalBand {
    /// The historical 0-100 Hz band for `n_modes` requested modes.
    pub(crate) fn legacy(n_modes: i64) -> Self {
        let extract = n_modes.max(1).max(NASTRAN95_MINIMUM_EXTRACTED_MODES);
        Self {
            upper_hz: NASTRAN95_MODAL_SEARCH_UPPER_HZ,
            estimate: extract.saturating_mul(NASTRAN95_MODAL_ROOT_ESTIMATE_FACTOR),
            extract,
        }
    }
}

/// Load-set identifier bases, kept apart for the reason [`crate::nastran`]
/// records: an earlier scheme let a pull-up force set collide with a push-down
/// gravity set, and NASTRAN would have merged them silently.
use crate::nastran::{FORCE_SID_BASE, GRAVITY_SID_BASE};

/// Full-field displacement and eigenvector output is intentionally retained so
/// the report can overlay NASTRAN-95 and MSC deformations. NASTRAN-95's
/// historic 20,000-line print default otherwise aborts a normal full-wing run.
const NASTRAN95_MAX_PRINT_LINES: u64 = 1_000_000;

/// The linear-static deck: one subcase per design load case over the shared
/// mesh, in `dialect`'s spelling.
pub fn build_static_deck(
    deck: &Deck,
    node_index: &MeshNodeIndex,
    req: &DesignRequirements,
    cfg: &StructuresConfig,
    dialect: Dialect,
) -> String {
    static_deck(deck, node_index, req, cfg, dialect, false, UnitScale::SI)
}

/// Product static output includes actual stresses as well as displacements.
/// Only case-control output requests differ from the historical builder.
pub fn build_static_deck_product(
    deck: &Deck,
    node_index: &MeshNodeIndex,
    req: &DesignRequirements,
    cfg: &StructuresConfig,
    dialect: Dialect,
) -> String {
    static_deck(deck, node_index, req, cfg, dialect, true, UnitScale::SI)
}

/// The product static deck in an explicit unit system. The production
/// NASTRAN-95 path writes [`UnitScale::DECIMETRE`]; see [`super::units`].
pub(crate) fn build_static_deck_product_in(
    deck: &Deck,
    node_index: &MeshNodeIndex,
    req: &DesignRequirements,
    cfg: &StructuresConfig,
    dialect: Dialect,
    units: UnitScale,
) -> String {
    static_deck(deck, node_index, req, cfg, dialect, true, units)
}

/// The product static deck written in a coherent system of `length_per_m`
/// deck length units per metre (mass unit `1 / length_per_m`, so a force unit
/// stays one newton). This exists for scale-sensitivity studies of the
/// 1995 solver ([`super::units`]); the product path always writes
/// [`UnitScale::DECIMETRE`]. A non-finite or non-positive scale is written as
/// SI.
pub fn build_static_deck_product_at_length_scale(
    deck: &Deck,
    node_index: &MeshNodeIndex,
    req: &DesignRequirements,
    cfg: &StructuresConfig,
    length_per_m: f64,
) -> String {
    let length = if length_per_m.is_finite() && length_per_m > 0.0 {
        length_per_m
    } else {
        1.0
    };
    let units = UnitScale {
        length,
        mass: 1.0 / length,
    };
    static_deck(deck, node_index, req, cfg, Dialect::Nastran95, true, units)
}

fn static_deck(
    deck: &Deck,
    node_index: &MeshNodeIndex,
    req: &DesignRequirements,
    cfg: &StructuresConfig,
    dialect: Dialect,
    stresses: bool,
    units: UnitScale,
) -> String {
    let cases = loads::load_cases(req, cfg.additional_safety_factor);

    let mut out = String::new();
    match dialect {
        Dialect::Nastran95 => {
            out.push_str("ID ALAS,WINGBOX\n");
            out.push_str("APP DISPLACEMENT\n");
            out.push_str("SOL 1,1\n");
            time_limit(&mut out, cfg.timeout_s);
            out.push_str("CEND\n");
            max_print_lines(&mut out);
        }
        Dialect::Modern => {
            out.push_str("SOL 101\n");
            out.push_str("CEND\n");
        }
    }
    out.push_str("TITLE = ALAS WINGBOX -- STATIC ANALYSIS\n");
    out.push_str(&format!("  SPC = {SPC_SET}\n"));
    out.push_str("  DISPLACEMENT = ALL\n");
    if stresses {
        out.push_str("  STRESS = ALL\n");
    }
    for (index, case) in cases.iter().enumerate() {
        let sid = index as i64 + 1;
        out.push_str(&format!("SUBCASE {sid}\n"));
        out.push_str(&format!(
            "  LABEL = {} (N={:+.2})\n",
            case.name.to_uppercase(),
            case.load_factor
        ));
        out.push_str(&format!("  LOAD = {sid}\n"));
    }
    out.push_str("BEGIN BULK\n");

    let mut tags = ContinuationTags::new();
    param(&mut out, "COUPMASS", Field::Int(1));
    bulk(&mut out, &mut tags, deck, dialect, units);
    static_loads(&mut out, &mut tags, deck, node_index, req, &cases, units);
    out.push_str("ENDDATA\n");
    out
}

/// The normal-modes deck in `dialect`'s spelling.
pub fn build_modes_deck(deck: &Deck, cfg: &StructuresConfig, dialect: Dialect) -> String {
    build_modes_deck_for_nodes(deck, cfg, dialect, &[])
}

/// The normal-modes deck while limiting printed eigenvectors to `output_nodes`.
///
/// An empty slice retains the public builder's historical all-grid output.
pub(crate) fn build_modes_deck_for_nodes(
    deck: &Deck,
    cfg: &StructuresConfig,
    dialect: Dialect,
    output_nodes: &[i64],
) -> String {
    build_modes_deck_banded(
        deck,
        cfg,
        dialect,
        output_nodes,
        UnitScale::SI,
        ModalBand::legacy(cfg.n_modes),
    )
}

/// The normal-modes deck in an explicit unit system and `EIGR` search band.
pub(crate) fn build_modes_deck_banded(
    deck: &Deck,
    cfg: &StructuresConfig,
    dialect: Dialect,
    output_nodes: &[i64],
    units: UnitScale,
    band: ModalBand,
) -> String {
    let mut out = String::new();
    match dialect {
        Dialect::Nastran95 => {
            out.push_str("ID ALAS,WINGBOX\n");
            out.push_str("APP DISPLACEMENT\n");
            out.push_str("SOL 3,1\n");
            time_limit(&mut out, cfg.timeout_s);
            out.push_str("CEND\n");
            max_print_lines(&mut out);
        }
        Dialect::Modern => {
            out.push_str("SOL 103\n");
            out.push_str("CEND\n");
        }
    }
    out.push_str("TITLE = ALAS WINGBOX -- NORMAL MODES\n");
    out.push_str(&format!("  SPC = {SPC_SET}\n"));
    out.push_str(&format!("  METHOD = {METHOD_SET}\n"));
    if output_nodes.is_empty() {
        out.push_str("  DISPLACEMENT = ALL\n");
    } else {
        case_control_set(&mut out, 9500, output_nodes);
        out.push_str("  DISPLACEMENT = 9500\n");
    }
    out.push_str("BEGIN BULK\n");

    let mut tags = ContinuationTags::new();
    param(&mut out, "COUPMASS", Field::Int(1));
    bulk(&mut out, &mut tags, deck, dialect, units);
    eigenvalue_card(&mut out, &mut tags, cfg, dialect, band);
    out.push_str("ENDDATA\n");
    out
}

fn case_control_set(out: &mut String, set_id: i64, values: &[i64]) {
    let mut line = format!("  SET {set_id} = ");
    for value in values {
        let token = value.to_string();
        let separator = usize::from(!line.ends_with(' '));
        if line.len() + separator + token.len() > 71 {
            line.push(',');
            out.push_str(&line);
            out.push('\n');
            line = format!("    {token}");
        } else {
            if separator != 0 {
                line.push(',');
            }
            line.push_str(&token);
        }
    }
    out.push_str(&line);
    out.push('\n');
}

/// Keep NASTRAN-95's executive time card in minutes aligned with ALAS's
/// per-solution wall-clock timeout. A fixed `TIME 30` could reject a job
/// immediately even when the caller had deliberately granted it more time.
fn time_limit(out: &mut String, timeout_seconds: f64) {
    let minutes = if timeout_seconds.is_finite() && timeout_seconds > 0.0 {
        (timeout_seconds / 60.0).ceil().max(1.0) as u64
    } else {
        30
    };
    out.push_str(&format!("TIME {minutes}\n"));
}

/// Extend NASTRAN-95's default printer limit without changing the requested
/// result fields. The limit is a case-control card, not an executable-memory
/// setting, and is required for all-grid report overlays on the full mesh.
fn max_print_lines(out: &mut String) {
    out.push_str(&format!("MAXLINES = {NASTRAN95_MAX_PRINT_LINES}\n"));
}
