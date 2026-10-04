// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The ATR 72-600 flies one assigned cruise level on a regional sector, not
//! the long-haul cruise-climb `MissionProfileConfig::default()` assumes.
//! Left at the defaults, the first cruise leg fell at 0.795 of a 17,000 ft
//! operational cruise altitude (FL135), where the denser air pushed the
//! level-flight thrust requirement to 99% of the deck's rated power and the
//! schedule ran out of trip fuel mid-climb.

use crate::MissionProfileConfig;

/// Fly one climb straight to the declared cruise altitude and one cruise leg
/// there, disabling the two step-climb legs `MissionProfileConfig` defaults
/// to. `0.999`, not `1.0`, because `MdoMissionProfile::validate`
/// (`crates/alas-opt/src/mdo/mission_model.rs`) rejects a fraction at or
/// above `1.0`; the cruise leg's own altitude (inherited from wherever the
/// climb before it ends) closes the resulting sub-6-m gap.
pub(crate) fn fly_single_assigned_level(profile: &mut MissionProfileConfig) {
    profile.initial_climb_altitude_fraction = 0.999;
    profile.cruise_2_distance_fraction = 0.0;
    profile.cruise_3_distance_fraction = 0.0;
}
