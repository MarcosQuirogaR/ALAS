// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/cabin_layout.py (`build_passenger_layout`, the
// monument, exit and baggage passes)
// Reference: alas @ rust-port-baseline.

//! Everything the cabin carries that is not a seat: the galley and lavatory
//! complexes, the emergency exits, and the checked baggage in the holds below.
//!
//! All three hang off what the seating pass already decided. The monuments and
//! the exits go into the bays it carved out, so an exit always lands on floor a
//! seat row left free rather than on a position computed independently that
//! would drift out of alignment with it. The baggage is trimmed toward the
//! *seating* centre of gravity, which is what airlines do with bags, so the
//! payload balance stays driven by where the passengers are.

include!("fittings_parts/part_01.rs");
include!("fittings_parts/part_02.rs");
