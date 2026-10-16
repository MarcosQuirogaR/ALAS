// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! NASTRAN-95 statics and normal modes: the deck this program writes for an
//! open-source solver that predates the modern one, and the run that solves it.
//!
//! This deck has no Python original to agree with. What it agrees
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
//! **Normal modes are extracted in a narrow band and audited with Sturm counts.**
//! NASTRAN-95's inverse power (`EIGR,,INV`) returns whichever roots its shifts
//! converge on inside the requested band. Over the former fixed `[0, 100]` Hz
//! band it skipped the lowest elastic roots (A320 wingbox: the first reported
//! root was 15.5 Hz with 19 roots below it, the first wing bending at 1.84 Hz
//! among them, against MSC's 1.83 Hz), and its "low frequency end not found"
//! message cannot be trusted either way. The total mass of the two decks is
//! identical to five figures, so the model was not the cause. The adapter now
//! tries bands of 4, 8, 16, ... Hz and accepts the lowest roots only when the
//! printed Sturm `ROOTS BELOW` counts account for every reported root
//! ([`completeness`]); only that gap-free prefix is returned, and a run that
//! cannot confirm any root is a modal `Error`, never a set of arbitrary modes.
//! Higher modes of the 1995 shell (no drilling stiffness) still differ from
//! MSC's by several percent; only the first modes are compared.
//!
//! **The product deck is written in decimetre - 10 kg - newton** ([`units`]):
//! NASTRAN-95's `CQUAD4` rejects, with an absolute length-unit-dependent
//! threshold, elements a modern solver accepts (trailing-edge rib panels of
//! 0.5 mm height on the B747-400, DC-10 and E195-E2 wingboxes), and its results
//! move with the length unit, so the unit is chosen where it matches MSC.
//! The public `build_*_deck` builders keep SI.
//!
//! Modal frequency response -- the modern deck's two `SOL 111` solutions -- has
//! no NASTRAN-95 equivalent at all and is not written.

mod analysis;
mod closed_form;
mod completeness;
mod deck;
mod field;
mod modal_search;
mod quad_guard;
mod run;
mod section;
mod static_spanwise;
mod static_stress;
mod units;

pub use analysis::{
    run_nastran95_analysis, run_nastran95_from_config_or_env, run_nastran95_from_env,
};
pub use closed_form::{run_cantilever_benchmark, BenchmarkOutcome, CantileverBox, LoadCaseOutcome};
pub use deck::{
    build_modes_deck, build_static_deck, build_static_deck_product,
    build_static_deck_product_at_length_scale, Dialect,
};
pub use field::{real, Card, ContinuationTags, Field};
pub use quad_guard::{
    degeneracy_message, degenerate_quads, DegenerateQuad, GUARD_MARGIN, QUAD4_DEGENERACY_LIMIT,
};
pub use run::{
    displacement_of, read_displacement_tables, read_eigenvalues, read_eigenvector_tables,
    run_nastran95, EigenvectorModeError, EigenvectorTables, Mode, Nastran95Solver, RunOutcome,
    MAX_EIGENVECTOR_MODE,
};
pub use section::{i_section, rectangle, BarConstants};
pub use static_spanwise::read_static_spanwise_print;
pub use static_stress::read_static_shell_stress_print;
