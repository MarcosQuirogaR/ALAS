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

include!("deck_parts/part_01.rs");
include!("deck_parts/part_02.rs");
