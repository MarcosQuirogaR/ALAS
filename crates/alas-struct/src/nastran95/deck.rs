// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The cross-solver decks: one wingbox written for two solvers so the pair can
//! be held to converge on each other.
//!
//! Both decks are produced here, from the same mesh, differing only where the
//! two dialects genuinely differ -- which is the point, because a difference the
//! two solvers then report is one of *those* differences and nothing else. The
//! shared bulk is emitted once, in fixed eight-column fields, and the [`Dialect`]
//! selects the handful of cards that change:
//!
//! * **Executive and case control.** NASTRAN-95 opens `APP DISPLACEMENT` and
//!   `SOL 1,1`/`SOL 3,1`; the modern deck is `SOL 101`/`SOL 103`.
//! * **`PARAM,AUTOSPC`** is the integer `1` for NASTRAN-95 -- what gives its
//!   `CQUAD4` the drilling stiffness that keeps the stiffness matrix
//!   non-singular -- and `YES` for the modern solver.
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

use std::collections::HashMap;

use alas_config::{DesignRequirements, StructuresConfig};

use super::field::{Card, ContinuationTags, Field};
use super::section;
use crate::loads::{self, LoadCase};
use crate::mesh::{Deck, MeshNodeIndex, ParamValue, Pbarl};
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

/// The constraint set the mesh writes -- one `SPC1`, set 1.
const SPC_SET: i64 = 1;

/// The eigenvalue-extraction set the modes deck selects.
const METHOD_SET: i64 = 1;

/// The legacy inverse-power method needs a populated low-frequency search
/// interval before it can identify the desired roots.  The product wingbox
/// has its first thirty elastic modes below 100 Hz; using the 500 Hz SOL 111
/// response ceiling here made the solver skip the low roots altogether.
const NASTRAN95_MODAL_SEARCH_UPPER_HZ: f64 = 100.0;

/// The inverse solver's root estimate must exceed the requested modal count.
/// A two-to-one estimate gives it enough shift regions to locate the low
/// roots, without applying the impractical full-band count to the old solver.
const NASTRAN95_MODAL_ROOT_ESTIMATE_FACTOR: i64 = 2;

/// The inverse solver must extract a sufficiently broad low-frequency set
/// even when a caller only displays a handful of modes.  The analysis reader
/// trims this reliable set back to the configured count after filtering rigid
/// modes.
const NASTRAN95_MINIMUM_EXTRACTED_MODES: i64 = 30;

/// Load-set identifier bases, kept apart for the reason [`crate::nastran`]
/// records: an earlier scheme let a pull-up force set collide with a push-down
/// gravity set, and NASTRAN would have merged them silently.
const GRAVITY_SID_BASE: i64 = 100;
const FORCE_SID_BASE: i64 = 200;

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
    bulk(&mut out, &mut tags, deck, dialect);
    static_loads(&mut out, &mut tags, deck, node_index, req, &cases);
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
    bulk(&mut out, &mut tags, deck, dialect);
    eigenvalue_card(&mut out, &mut tags, cfg, dialect);
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

/// Reformulate every mesh card into fixed-field bulk data.
fn bulk(out: &mut String, tags: &mut ContinuationTags, deck: &Deck, dialect: Dialect) {
    params(out, deck, dialect);
    for grid in deck.grids() {
        card(
            out,
            tags,
            "GRID",
            vec![
                Field::Int(grid.nid),
                Field::Blank,
                Field::Real(grid.xyz[0]),
                Field::Real(grid.xyz[1]),
                Field::Real(grid.xyz[2]),
            ],
        );
    }
    for shell in deck.quads().iter().chain(deck.trias()) {
        let name = if shell.nodes.len() == 4 {
            "CQUAD4"
        } else {
            "CTRIA3"
        };
        let mut fields = vec![Field::Int(shell.eid), Field::Int(shell.pid)];
        fields.extend(shell.nodes.iter().map(|&nid| Field::Int(nid)));
        card(out, tags, name, fields);
    }
    bars(out, tags, deck);
    for mass in &deck.masses {
        card(
            out,
            tags,
            "CONM2",
            vec![
                Field::Int(mass.eid),
                Field::Int(mass.nid),
                Field::Int(mass.cid),
                Field::Real(mass.mass),
                Field::Real(mass.offset[0]),
                Field::Real(mass.offset[1]),
                Field::Real(mass.offset[2]),
            ],
        );
    }
    let rbe3_name = match dialect {
        Dialect::Nastran95 => "CRBE3",
        Dialect::Modern => "RBE3",
    };
    for rigid in &deck.rigid_elements {
        // The same layout in both dialects -- field 2 blank, then the reference
        // grid and components, the one weight and component group, and the
        // independent grids -- emitted small-field so MSC's input processor
        // accepts the continuation the mesh's large field does not survive.
        let mut fields = vec![
            Field::Int(rigid.eid),
            Field::Blank,
            Field::Int(rigid.refgrid),
            Field::Text(rigid.refc),
            Field::Real(rigid.weight),
            Field::Text(rigid.comp),
        ];
        fields.extend(rigid.gijs.iter().map(|&nid| Field::Int(nid)));
        card(out, tags, rbe3_name, fields);
    }
    for property in &deck.shell_properties {
        card(
            out,
            tags,
            "PSHELL",
            vec![
                Field::Int(property.pid),
                Field::Int(property.mid1),
                Field::Real(property.t),
                Field::Int(property.mid2),
            ],
        );
    }
    for material in &deck.materials {
        card(
            out,
            tags,
            "MAT1",
            vec![
                Field::Int(material.mid),
                Field::Real(material.e),
                Field::Real(material.g),
                Field::Real(material.nu),
                Field::Real(material.rho),
            ],
        );
    }
    for constraint in &deck.constraints {
        let mut fields = vec![
            Field::Int(constraint.sid),
            Field::Text(constraint.components),
        ];
        fields.extend(constraint.nodes.iter().map(|&nid| Field::Int(nid)));
        card(out, tags, "SPC1", fields);
    }
}

/// The `CBAR`s and the `PBAR`s reduced from their `PBARL` sections.
///
/// Both dialects get `PBAR`, not just NASTRAN-95: modelling the caps as the
/// identical explicit beam on both sides is what lets the cross-solver
/// comparison isolate the shell and eigensolver differences, and the reduction
/// [`super::section`] performs is validated against a modern `PBARL` on its own.
/// Field 9 -- the mesh's `OFFT` string -- is dropped, because this dialect's
/// `CBAR` spends that column on an integer flag and reads the vector directly.
fn bars(out: &mut String, tags: &mut ContinuationTags, deck: &Deck) {
    let sections: HashMap<i64, &Pbarl> = deck.bar_properties.iter().map(|p| (p.pid, p)).collect();
    for bar in &deck.bars {
        card(
            out,
            tags,
            "CBAR",
            vec![
                Field::Int(bar.eid),
                Field::Int(bar.pid),
                Field::Int(bar.ga),
                Field::Int(bar.gb),
                Field::Real(bar.x[0]),
                Field::Real(bar.x[1]),
                Field::Real(bar.x[2]),
            ],
        );
        let Some(property) = sections.get(&bar.pid) else {
            continue;
        };
        // Every cap this mesh builds is the symmetric I `i_section` reduces; a
        // section it cannot reduce is omitted rather than emitted wrong, which
        // makes the end-to-end run fail loudly on a bar with no property.
        if let Some(bar_constants) = section::i_section(&property.dim) {
            card(
                out,
                tags,
                "PBAR",
                vec![
                    Field::Int(property.pid),
                    Field::Int(property.mid),
                    Field::Real(bar_constants.area),
                    Field::Real(bar_constants.i1),
                    Field::Real(bar_constants.i2),
                    Field::Real(bar_constants.j),
                ],
            );
        }
    }
}

/// The `PARAM`s the mesh carried, translated to `dialect`.
///
/// `AUTOSPC` is the load-bearing one and its value is where the two dialects
/// disagree -- an integer here, `YES` there. `GRDPNT` passes through; `POST`
/// selects an output file neither of these runs needs and is dropped.
fn params(out: &mut String, deck: &Deck, dialect: Dialect) {
    let autospc = match dialect {
        Dialect::Nastran95 => Field::Int(1),
        Dialect::Modern => Field::Text("YES"),
    };
    for card in &deck.params {
        match card.key {
            "AUTOSPC" => param(out, "AUTOSPC", autospc),
            "POST" => {}
            other => param(
                out,
                other,
                match card.value {
                    ParamValue::Int(number) => Field::Int(number),
                    ParamValue::Name(text) => Field::Text(text),
                },
            ),
        }
    }
}

/// The gravity, force and combination cards for every subcase, identical in both
/// dialects.
fn static_loads(
    out: &mut String,
    tags: &mut ContinuationTags,
    deck: &Deck,
    node_index: &MeshNodeIndex,
    req: &DesignRequirements,
    cases: &[LoadCase],
) {
    let front_upper: &[i64] = node_index
        .spar_upper_nids
        .first()
        .map_or(&[], |nids| nids.as_slice());
    let semi_span = deck.node_y(node_index.tip_nid);
    let nid_y: Vec<(i64, f64)> = front_upper
        .iter()
        .map(|&nid| (nid, deck.node_y(nid)))
        .collect();

    for (index, case) in cases.iter().enumerate() {
        let sid = index as i64 + 1;
        let gravity_sid = GRAVITY_SID_BASE + sid;
        let force_sid = FORCE_SID_BASE + sid;
        let gravity_magnitude = case.load_factor.abs() * req.gravity_m_s2;
        let lift_sign = if case.total_force_n >= 0.0 { 1.0 } else { -1.0 };
        let gravity_sign: f64 = if case.load_factor >= 0.0 { -1.0 } else { 1.0 };

        card(
            out,
            tags,
            "LOAD",
            vec![
                Field::Int(sid),
                Field::Real(1.0),
                Field::Real(1.0),
                Field::Int(gravity_sid),
                Field::Real(1.0),
                Field::Int(force_sid),
            ],
        );
        card(
            out,
            tags,
            "GRAV",
            vec![
                Field::Int(gravity_sid),
                Field::Int(0),
                Field::Real(gravity_magnitude),
                Field::Real(0.0),
                Field::Real(0.0),
                Field::Real(gravity_sign),
            ],
        );
        for (nid, force) in elliptic_forces_by_y(&nid_y, case.total_force_n.abs(), semi_span) {
            card(
                out,
                tags,
                "FORCE",
                vec![
                    Field::Int(force_sid),
                    Field::Int(nid),
                    Field::Int(0),
                    Field::Real(force.abs() * lift_sign),
                    Field::Real(0.0),
                    Field::Real(0.0),
                    Field::Real(1.0),
                ],
            );
        }
    }
}

/// The eigenvalue-extraction card each dialect uses to ask for the lowest
/// `n_modes`.
///
/// `EIGRL,,,,N` asks the modern solver for the lowest `N` directly. NASTRAN-95's
/// Givens methods need a positive-definite mass matrix, which a shell-and-mass
/// model does not have, so it uses `EIGR,,INV` over a bounded low-frequency
/// interval.  `NE` is an estimate of the roots in that interval, not the
/// requested output count: supplying `n_modes` for both while retaining the
/// 500 Hz SOL 111 response ceiling made NASTRAN-95 skip the elastic roots.
/// Extracting at least thirty modes with a two-to-one root estimate gives the
/// historic solver enough shifts; the analysis reader then retains the user
/// requested elastic subset. The continuation card is required by the `EIGR`
/// format.
fn eigenvalue_card(
    out: &mut String,
    tags: &mut ContinuationTags,
    cfg: &StructuresConfig,
    dialect: Dialect,
) {
    let n_modes = cfg.n_modes.max(1);
    let local_extract_count = n_modes.max(NASTRAN95_MINIMUM_EXTRACTED_MODES);
    let local_root_estimate =
        local_extract_count.saturating_mul(NASTRAN95_MODAL_ROOT_ESTIMATE_FACTOR);
    let local_upper_hz = NASTRAN95_MODAL_SEARCH_UPPER_HZ;
    match dialect {
        Dialect::Modern => card(
            out,
            tags,
            "EIGRL",
            vec![
                Field::Int(METHOD_SET),
                Field::Blank,
                Field::Blank,
                Field::Int(n_modes),
            ],
        ),
        Dialect::Nastran95 => card(
            out,
            tags,
            "EIGR",
            vec![
                Field::Int(METHOD_SET),
                Field::Text("INV"),
                Field::Real(0.0),
                Field::Real(local_upper_hz),
                Field::Int(local_root_estimate),
                Field::Int(local_extract_count),
                Field::Blank,
                Field::Blank,
                Field::Text("MASS"),
            ],
        ),
    }
}

/// One `PARAM` card in fixed field.
fn param(out: &mut String, key: &'static str, value: Field) {
    let mut tags = ContinuationTags::new();
    Card::new("PARAM", vec![Field::Text(key), value]).render(out, &mut tags);
}

/// Render one card into `out`.
fn card(out: &mut String, tags: &mut ContinuationTags, name: &'static str, fields: Vec<Field>) {
    Card::new(name, fields).render(out, tags);
}

#[cfg(test)]
mod tests {
    use super::{build_modes_deck, build_modes_deck_for_nodes, time_limit, Dialect};
    use crate::mesh::Deck;
    use alas_config::StructuresConfig;

    #[test]
    fn legacy_time_card_tracks_the_per_solution_timeout_in_whole_minutes() {
        let mut card = String::new();
        time_limit(&mut card, 121.0);
        assert_eq!(card, "TIME 3\n");
    }

    #[test]
    fn local_modes_use_a_bounded_search_with_a_sufficient_root_estimate() {
        let config = StructuresConfig {
            n_modes: 6,
            ..StructuresConfig::default()
        };
        let deck = build_modes_deck(&Deck::default(), &config, Dialect::Nastran95);
        assert!(deck.contains("EIGR    1       INV     0.      100.    60      30"));
    }

    #[test]
    fn local_modes_can_limit_eigenvector_printing_to_required_nodes() {
        let nodes: Vec<i64> = (1..=40).map(|value| value * 10_000).collect();
        let deck = build_modes_deck_for_nodes(
            &Deck::default(),
            &StructuresConfig::default(),
            Dialect::Nastran95,
            &nodes,
        );
        assert!(deck.contains("  SET 9500 = "));
        assert!(deck.contains("  DISPLACEMENT = 9500\n"));
        assert!(!deck.contains("  DISPLACEMENT = ALL\n"));
        for line in deck
            .lines()
            .skip_while(|line| !line.starts_with("  SET 9500"))
        {
            if line == "  DISPLACEMENT = 9500" {
                break;
            }
            assert!(
                line.len() <= 72,
                "case-control line exceeds 72 columns: {line}"
            );
        }
    }
}
