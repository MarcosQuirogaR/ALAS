// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The throttle envelope a flown segment must stay inside.

/// Tolerance of the throttle envelope, in units of the maximum-climb force: a
/// request this far beyond the rating or below the flight-idle floor is the
/// discretisation of a leg designed at the envelope (the level speed changes are
/// integrated at full thrust and at idle), not a force the aeroplane lacks. 0.1 % of
/// the rated force is far inside the scatter of the thrust deck itself (the
/// `cruise_thrust` check's band is several per cent). Measured: the A340-300 and
/// E195-E2 speed changes miss the envelope by 2e-4 and fly at once with it.
pub const ENVELOPE_TOLERANCE: f64 = 1.0e-3;
/// The lowest command evaluated: a drag of one maximum-climb force, far below any flight-idle net force.
pub(crate) const LOWEST_COMMAND: f64 = -1.0;
