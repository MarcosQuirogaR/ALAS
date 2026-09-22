// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! NASTRAN-95 statics and normal modes: the deck this program writes for an
//! open-source solver that predates the modern one, and the run that solves it.
//!
//! This is the one row in the port with no Python original; there is nothing
//! to translate and so nothing to agree with in the usual way. What it agrees
//! with instead is a *second solver*. The same wingbox is solved twice: once as
//! the modern [`crate::nastran`] deck through MSC Nastran, and once as the
//! NASTRAN-95 deck [`deck`] writes through the built 1995 solver, and the two
//! results are held to converge on each other as the mesh is refined. That is a
//! stronger claim than a fixed tolerance; it measures what the dialect
//! differences cost rather than asserting a bound, and it is the only claim
//! available where neither side is a reference.
//!
//! Three dialect facts shape everything here, each found by a run that failed,
//! several of them *silently*, until it was obeyed. They are stated once, here,
//! and the modules enforce them:
//!
//! 1. **Fixed eight-column fields.** Every field is at most eight characters,
//!    and a real must show a decimal point. [`field`] is the formatter; the
//!    modern deck's sixteen-column large field does not exist here.
//! 2. **At most eight fields before a continuation.** A longer card: the
//!    root-rib `SPC1`, every `CRBE3`, is quoted back and *dropped without a
//!    message* unless it continues onto a tagged line. [`field::Card`] does that.
//! 3. **Three cards change shape.** `PBARL` has no equivalent and becomes a
//!    computed `PBAR` ([`section`]); `RBE3` is the same element under the name
//!    `CRBE3`; and `PARAM,AUTOSPC` is the integer `1`, not `YES`, and is what
//!    gives this solver's `CQUAD4` the drilling stiffness that keeps its
//!    stiffness matrix non-singular.
//!
//! # What is validated, and what is not
//!
//! **Linear statics is cross-validated and converges.** Solved through both
//! solvers over a refinement sequence, the two peak deflections agree to about
//! 7% on the coarsest mesh and close to about 2% on the finest, the disagreement
//! halving as the mesh refines: the signature of two `CQUAD4` formulations
//! (1995 without drilling stiffness, `AUTOSPC`-constrained; modern with it)
//! approaching one answer. That convergence is the row's tier;
//! `parity_nastran95.rs` holds it.
//!
//! **Normal modes are written but not cross-validated, and that is a finding.**
//! [`build_modes_deck`] produces a valid `EIGR` deck both solvers run without a
//! fatal, but NASTRAN-95's 1970s real-eigenvalue methods: Givens, inverse
//! power, FEER, do not reliably return a modern solver's lowest modes for this
//! model. Its mass matrix is singular on the rotational freedoms (shells and
//! concentrated masses give translational inertia and little rotary), which
//! Givens cannot reduce, and inverse power finds a band-dependent, incomplete
//! set of roots rather than the fundamentals: the modern solver's first mode is
//! near 2 Hz where inverse power over `[0, 20]` reports its lowest near 8. The
//! total mass the two build is identical to five figures, so this is the
//! eigensolver and not the model. Matching the lowest modes across the two
//! solvers is left as an open question for the ledger rather than asserted here.
//!
//! Modal frequency response -- the modern deck's two `SOL 111` solutions -- has
//! no NASTRAN-95 equivalent at all and is not written.

mod analysis;
mod deck;
mod field;
mod run;
mod section;

pub use analysis::{
    run_nastran95_analysis, run_nastran95_from_config_or_env, run_nastran95_from_env,
};
pub use deck::{build_modes_deck, build_static_deck, Dialect};
pub use field::{real, Card, ContinuationTags, Field};
pub use run::{
    displacement_of, read_displacement_tables, read_eigenvalues, read_eigenvector_tables,
    run_nastran95, Mode, Nastran95Solver, RunOutcome,
};
pub use section::{i_section, BarConstants};
