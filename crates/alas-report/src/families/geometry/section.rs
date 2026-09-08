// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Dimensioned transverse cabin section, drawn from a resolved cabin scene.
//!
//! The input is `alas.cabin-scene/v2`, the renderer-neutral scene the pipeline
//! already exports: station contours, decks, seat rows and seats, overhead
//! runs, nominal window apertures and the resolved cargo, all in metres in the
//! aircraft frame. Nothing here re-derives that geometry, so the drawing and
//! the exported scene cannot disagree about where a seat is.
//!
//! Three rules separate this from a pretty picture of a fuselage:
//!
//! 1. One plane. A transverse section has exactly one longitudinal coordinate.
//!    The station is selected for coverage -- seat rows on every passenger
//!    deck first, then cargo, a hold contour, overhead runs and seat count --
//!    and every drawn part is cut by that plane. A deck with no row there is
//!    drawn empty rather than filled from a neighbouring frame.
//! 2. Nothing is resized to fit. Seats keep their solved width, ULDs keep the
//!    proportions of their standard contour, and the scale figure keeps its
//!    1.75 m. A part that does not fit its container is drawn where the solver
//!    put it, and the overflow is reported.
//! 3. What is missing says so. Window apertures are nominal until an aircraft
//!    window schedule exists, and the figure prints that rather than implying
//!    a certified aperture.
//!
//! `docs/cabin-renderer-sources.md` holds the evidence register these rules
//! come from, including what the published sources do and do not license.

include!("section_parts/part_01.rs");
include!("section_parts/part_02.rs");
include!("section_parts/part_03.rs");
include!("section_parts/part_04.rs");
