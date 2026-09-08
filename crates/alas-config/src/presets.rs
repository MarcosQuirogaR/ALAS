// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/presets.py
// Reference: alas @ rust-port-baseline.

//! Real aircraft, as complete starting points.
//!
//! The other three registries in this crate bundle a handful of overrides on
//! one configuration struct. An entry here is a whole aeroplane: a design
//! vector, a geometry scaffold, a set of requirements, an engine, and -- for
//! the types the global assumptions do not fit -- its own mass-model and
//! field-performance calibration. Selecting one is how a user starts from
//! something that flies rather than from a blank form, and it is how this
//! program is checked against reality: an A320 that comes out sixteen tonnes
//! heavy is a visible failure in a way that a notional design never is.
//!
//! Dimensions come from published specification sheets. Where a manufacturer
//! does not publish root and tip chords, they are estimated from wing area,
//! aspect ratio, taper and sweep by standard planform relations, which is why
//! the two chord fields of a real type are the least certain numbers here.
//!
//! A registered type also carries its exact variant identity, revision-locked
//! reference data, and landing-gear topology. Those records distinguish a
//! documented aircraft configuration from an optimizer bound or a project
//! assumption; a missing public AFM/WBM value remains missing rather than
//! being inferred from unrelated planning data.
//!
//! # What a preset does not settle
//!
//! The registry binds each entry to the engine it names, at registration,
//! before anything downstream can read it. It used to record only the name and
//! leave the built-in GE9X cycle in place, on the understanding that the
//! configuration-loading boundary would resolve it. Exactly one caller did.
//! The geometry builder, the full analysis and the acceptance matrix all read
//! `preset.geometry` directly, so every turbofan preset was weighed, drawn and
//! flown as a 467 kN GE9X regardless of what it declared -- a constant 10.3 t
//! of propulsion mass per engine, and a 2.1 m-radius nacelle on an A320.
//! Resolving it here makes the declared engine the one every discipline sees;
//! the loading boundary still applies saved or user-provided overrides on top,
//! and downstream product solvers consume those live fields without another
//! database lookup.
//!
//! Nor does a preset fit the design space it is offered in. The bounds in
//! [`crate::design_variables`] are one global set describing AVE's family, so
//! every published type here starts outside at least one of them -- an A320's
//! fuselage is twenty-eight metres shorter than the shortest the search will
//! consider. That is upstream's arrangement and not an oversight: whoever runs
//! a search narrows the bounds around the design it starts from, and the
//! optimizer reports an initial design that falls outside whatever bounds it
//! was handed.
//!
//! The remaining boundary is the two per-aircraft calibrations. They are
//! [`Option`]s, and
//! `None` means "use the global default" rather than "no calibration" --
//! [`crate::AlasConfig::from_value`] is where they are applied, because a
//! headless run that skipped them would silently revert an A220 to the
//! widebody-calibrated mass fractions its entry exists to correct.

include!("presets_parts/part_01.rs");
include!("presets_parts/part_02.rs");
