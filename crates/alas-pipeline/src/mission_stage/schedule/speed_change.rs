// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Speed changes between mission legs.
//!
//! Every climb, cruise and descent leg is flown on its own speed schedule, and
//! two consecutive legs rarely share a speed at their junction: V2 at the end
//! of the take-off leg against 250 kt CAS in the initial climb, 300 kt CAS in
//! the upper climb against 250 kt below 10,000 ft in the descent, the cruise
//! Mach against the first descent rung. A speed cannot jump, so the change is
//! flown, by the total-energy model of the EUROCONTROL BADA 3.8 User Manual
//! (Sec. 3.2, printed pp. 14-16). Every difference above
//! [`SPEED_IDENTITY_M_S`] is flown (see [`super::junction`]):
//!
//! * **Along the path of the climb or descent next to the junction**, by the
//!   energy share factor (`alas_mission::segments::blend`), when the leg has
//!   the altitude for it and the engine can fly the change at a meaningful
//!   rate: the default for the acceleration in a climb and the deceleration in
//!   a descent.
//! * **In level flight** otherwise: a leg at the junction altitude changes the
//!   speed at the thrust the aircraft has, maximum climb thrust to accelerate
//!   and flight idle to decelerate. All the excess power goes into speed
//!   (`m dV/dt = T - D`), so the duration is not chosen: it follows from the
//!   excess force at each speed, which falls as the speed nears the
//!   equilibrium one, and the acceleration is limited to BADA's
//!   `a_l,max(civ)` = 2.0 ft/s^2 (Sec. 5.2, printed p. 33). The planner
//!   integrates it with the mission's own thrust and drag (`level_change`);
//!   until then a constant acceleration at that limit stands in.
//!
//! A level change runs to its target speed. One the aircraft cannot complete
//! (the excess force vanishes before the target) ends at the settling band of
//! its equilibrium speed, 98 % of the way (the usual 2 % convention for an
//! asymptotic approach), and the leg after the junction starts there: a cruise
//! leg is flown at the speed the aircraft can reach.

use std::collections::HashMap;

use alas_atmo::us1976_try_compute_values;
use alas_config::mission::{SpeedCaps, SpeedReference};
use alas_mission::segments::{SegmentKind, SegmentSpec};

/// Tag prefix of the legs this module inserts; the rest of the tag names the
/// leg that follows the junction.
pub(crate) const SPEED_CHANGE_PREFIX: &str = "speed_change_to_";

pub(crate) use alas_config::mission::MINIMUM_SPEED_CHANGE_M_S;

/// Two speeds closer than this are the same speed, m/s: every difference
/// above it is flown (the schedules of two legs are computed independently, so
/// this is numerical identity, not a tolerance on the aircraft).
pub(crate) const SPEED_IDENTITY_M_S: f64 = 0.05;

pub(crate) use alas_config::mission::LEVEL_ACCELERATION_LIMIT_M_S2;

/// Share of the speed difference at which an asymptotic level change is taken
/// as complete (2 % settling convention).
pub(crate) const SETTLED_SHARE: f64 = 0.98;

/// Resolution of the search for the shortest flyable duration of a level change:
/// a change the flown leg cannot hold inside the thrust envelope (the integrated
/// table is designed at the envelope, so discretisation of the table against the
/// segment's control points can push a node a few per cent outside it) is lengthened
/// by this factor, and flown again, until it holds; the duration found is the minimum
/// to within 2 %. (It replaces a one-shot 25 % lengthening that had no source and
/// fired on every preset.)
pub(crate) const LENGTHENING_STEP: f64 = 1.02;

/// The rule that resolves a declared cruise true airspeed to the level a cruise
/// leg is flown at: the one the priced route applies (`level`, `declared`).
#[derive(Clone)]
pub(crate) struct CruiseSpeedRule(std::sync::Arc<dyn Fn(f64, f64) -> Option<f64> + Send + Sync>);

impl CruiseSpeedRule {
    /// A rule from a function of the level, m, and the declared speed, m/s.
    pub(crate) fn new(rule: impl Fn(f64, f64) -> Option<f64> + Send + Sync + 'static) -> Self {
        Self(std::sync::Arc::new(rule))
    }
}

impl std::fmt::Debug for CruiseSpeedRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CruiseSpeedRule")
    }
}

/// A level speed change as integrated for one junction.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LevelProfile {
    /// Speed the leg starts at, m/s.
    pub(crate) from_m_s: f64,
    /// Speed it was asked to reach, m/s.
    pub(crate) target_m_s: f64,
    /// Speed it reaches, m/s (the target unless the excess force vanishes).
    pub(crate) reached_m_s: f64,
    /// Distance flown, m.
    pub(crate) distance_m: f64,
    /// Duration, s (zero when not integrated).
    pub(crate) duration_s: f64,
    /// Rows `[fraction of the duration, speed m/s]`.
    pub(crate) table: Vec<[f64; 2]>,
}

/// What the planner remembers about each junction between re-closings of the
/// route: the level change integrated for it.
#[derive(Debug, Clone, Default)]
pub(crate) struct SpeedMemory {
    profiles: HashMap<String, LevelProfile>,
    /// Rate, m/s, each piece cut for a path change was last flown at (a piece is
    /// merged into its leg whenever the speeds are joined again and would
    /// otherwise start from the leg's rate).
    piece_rates: HashMap<String, f64>,
    /// Pieces cut for a path change that are flown at the maximum-climb rating
    /// (`SegmentSpec::rating_limited`), kept across the same re-joining.
    rated_pieces: std::collections::HashSet<String>,
    /// The cruise-speed rule of the priced route, when the native flight follows it.
    cruise_rule: Option<CruiseSpeedRule>,
    /// The declared cruise speed of each cruise leg, m/s.
    declared_cruise: HashMap<String, f64>,
    /// Rate, m/s, each piece cut for a path change was last flown at (a piece is
    /// merged into its leg whenever the speeds are joined again and would
    /// otherwise start from the leg's rate).
    /// Path changes the engine cannot fly at a meaningful rate (see
    /// `junction::MINIMUM_PATH_CHANGE_RATE_M_S`): flown as level changes.
    level_changes: std::collections::HashSet<String>,
    /// Ceiling, m/s, on the speed of a cruise leg the aircraft could not hold at its level:
    /// the speed it is flown at is the lower of this and the declared one (lengthening a
    /// speed change cannot make sustainable a speed the thrust at the level cannot hold).
    cruise_caps: HashMap<String, f64>,
    /// Share of the route the altitude fit must leave for cruise, when it is not the
    /// default ([`MIN_CRUISE_ROUTE_FRACTION`]): half the share the priced route flew.
    min_cruise_fraction: Option<f64>,
    /// Whether a remembered level change is held to the speed the leg after it
    /// asks for *now*: a mission plan moves the altitude of a junction between
    /// re-closings, which moves the true airspeed a calibrated leg starts at, and
    /// a change remembered for the old altitude would end up to
    /// [`MINIMUM_SPEED_CHANGE_M_S`] away from it. The standard mission keeps its
    /// historical reuse.
    follow_nominal: bool,
}

impl SpeedMemory {
    /// This memory with remembered level changes held to the speed the leg
    /// after them asks for now (see the field `follow_nominal`).
    #[must_use]
    pub(crate) fn following_nominal(mut self) -> Self {
        self.follow_nominal = true;
        self
    }

    /// Whether remembered level changes follow the nominal speed of the leg
    /// after them.
    pub(crate) fn follows_nominal(&self) -> bool {
        self.follow_nominal
    }

    /// A memory whose cruise legs follow `rule`, with the cruise speeds `profile`
    /// declares for them.
    pub(crate) fn for_profile(
        profile: &alas_config::mission::MissionProfileConfig,
        rule: Option<CruiseSpeedRule>,
    ) -> Self {
        let declared = [
            ("cruise_step_1", profile.cruise_1_air_speed_m_s),
            ("cruise_step_2", profile.cruise_2_air_speed_m_s),
            ("cruise_step_3", profile.cruise_3_air_speed_m_s),
        ];
        Self {
            cruise_rule: rule,
            declared_cruise: declared
                .into_iter()
                .map(|(tag, speed)| (tag.to_owned(), speed))
                .collect(),
            ..Self::default()
        }
    }

    /// Follow the priced route's cruise: the altitude fit leaves it half the share of
    /// the route the priced route cruised (never more than the default minimum), so
    /// that the native speed-change legs, which the priced footprints do not count,
    /// do not lower a level the priced route flew.
    pub(crate) fn follow_priced_cruise(&mut self, priced_cruise_fraction: f64) {
        self.min_cruise_fraction = Some(
            (0.5 * priced_cruise_fraction)
                .clamp(0.0, alas_config::mission::MIN_CRUISE_ROUTE_FRACTION),
        );
    }

    /// The share of the route the altitude fit leaves for cruise.
    pub(super) fn min_cruise_fraction(&self) -> f64 {
        self.min_cruise_fraction
            .unwrap_or(alas_config::mission::MIN_CRUISE_ROUTE_FRACTION)
    }

    /// Declare the cruise speed of a cruise leg the profile does not name
    /// (a leg of a mission plan), so the rule resolves it at the leg's level.
    pub(crate) fn declare_cruise(&mut self, tag: &str, declared_m_s: f64) {
        self.declared_cruise.insert(tag.to_owned(), declared_m_s);
    }

    /// Forget the cruise speed declared for `tag` and any ceiling on it: the leg no longer flies
    /// a cruise speed.
    pub(crate) fn forget_cruise(&mut self, tag: &str) {
        self.declared_cruise.remove(tag);
        self.cruise_caps.remove(tag);
    }

    /// The speed the cruise leg `tag` flies at `level_m`, when a rule is set.
    pub(super) fn cruise_speed(&self, tag: &str, level_m: f64) -> Option<f64> {
        let declared = *self.declared_cruise.get(tag)?;
        (self.cruise_rule.as_ref()?.0)(level_m, declared)
    }

    /// Fly the path change that piece `key` would make as a level change from now on.
    pub(crate) fn prefer_level(&mut self, key: &str) {
        self.level_changes.insert(key.to_owned());
    }

    /// Whether the change of piece `key` is flown as a level change.
    pub(super) fn level_preferred(&self, key: &str) -> bool {
        self.level_changes.contains(key)
    }

    /// Hold the cruise leg `tag` to at most `speed_m_s` from now on.
    pub(crate) fn cap_cruise_speed(&mut self, tag: &str, speed_m_s: f64) {
        let cap = self.cruise_caps.entry(tag.to_owned()).or_insert(speed_m_s);
        *cap = cap.min(speed_m_s);
    }

    /// The ceiling on the speed of the cruise leg `tag`, when it has one.
    pub(super) fn cruise_cap(&self, tag: &str) -> Option<f64> {
        self.cruise_caps.get(tag).copied()
    }

    /// Remember the rate the piece tagged `tag` was flown at.
    pub(crate) fn remember_rate(&mut self, tag: &str, rate_m_s: f64) {
        if tag.contains(super::junction::PIECE_MARK) {
            self.piece_rates.insert(tag.to_owned(), rate_m_s);
        }
    }

    /// The rate the piece tagged `tag` was last flown at.
    pub(super) fn rate_of(&self, tag: &str) -> Option<f64> {
        self.piece_rates.get(tag).copied()
    }

    /// Remember that the piece tagged `tag` is flown at the maximum-climb rating.
    pub(crate) fn remember_rated(&mut self, tag: &str) {
        if tag.contains(super::junction::PIECE_MARK) {
            self.rated_pieces.insert(tag.to_owned());
        }
    }

    /// Whether the piece tagged `tag` is flown at the maximum-climb rating.
    pub(super) fn is_rated(&self, tag: &str) -> bool {
        self.rated_pieces.contains(tag)
    }

    /// Remember the level change integrated for the junction in front of the
    /// leg tagged `leg_tag`.
    pub(crate) fn insert(&mut self, leg_tag: &str, profile: LevelProfile) {
        self.profiles
            .insert(format!("{SPEED_CHANGE_PREFIX}{leg_tag}"), profile);
    }

    /// The speed the level change in front of the leg tagged `leg_tag` was
    /// asked to reach, when one was integrated.
    pub(crate) fn requested_target(&self, leg_tag: &str) -> Option<f64> {
        self.profiles
            .get(&format!("{SPEED_CHANGE_PREFIX}{leg_tag}"))
            .map(|profile| profile.target_m_s)
    }

    /// The stand-in for the junction `tag` from `from` to `to` m/s when its
    /// remembered integration is for other speeds ([`Self::valid_for`] fails):
    /// that integration mapped onto the new speeds, its speed-against-time
    /// shape kept and its duration scaled with the speed difference, i.e. the
    /// same acceleration at the same share of the change. A closure that moves
    /// the junction's altitude moves its start speed by a few m/s (a CAS climb
    /// ending lower); the constant BADA-limit stand-in that replaced the
    /// integration there asked a heavy aircraft at altitude for up to 1.5 times
    /// its maximum climb thrust, which the full flight's 2 % lengthening steps
    /// cannot recover, while the mapped integration is within a few percent of
    /// the aircraft's own excess thrust. `None` when there is no integration of
    /// the change, it stopped short of its target (an equilibrium speed does
    /// not map onto a new target) or it runs the other way.
    pub(super) fn rescaled_for(&self, tag: &str, from: f64, to: f64) -> Option<LevelProfile> {
        let remembered = self.profiles.get(tag)?;
        let span = remembered.reached_m_s - remembered.from_m_s;
        let completed =
            (remembered.reached_m_s - remembered.target_m_s).abs() <= SPEED_IDENTITY_M_S;
        let new_span = to - from;
        // A lengthened change carries its distance only: its duration is the
        // distance over the mean speed of its table.
        let mean_speed = |table: &[[f64; 2]]| {
            table
                .windows(2)
                .map(|pair| 0.5 * (pair[0][1] + pair[1][1]) * (pair[1][0] - pair[0][0]))
                .sum::<f64>()
        };
        let remembered_duration_s = if remembered.duration_s > 0.0 {
            remembered.duration_s
        } else {
            remembered.distance_m / mean_speed(&remembered.table)
        };
        if !completed
            || !(remembered_duration_s.is_finite() && remembered_duration_s > 0.0)
            || remembered.table.len() < 2
            || span.abs() <= SPEED_IDENTITY_M_S
            || new_span.abs() <= SPEED_IDENTITY_M_S
            || span.signum() != new_span.signum()
        {
            return None;
        }
        let scale = new_span / span;
        let table: Vec<[f64; 2]> = remembered
            .table
            .iter()
            .map(|row| [row[0], from + (row[1] - remembered.from_m_s) * scale])
            .collect();
        let duration_s = remembered_duration_s * scale;
        let distance_m = duration_s * mean_speed(&table);
        Some(LevelProfile {
            from_m_s: from,
            target_m_s: to,
            reached_m_s: to,
            distance_m,
            duration_s,
            table,
        })
    }

    pub(super) fn valid_for(&self, tag: &str, from: f64, to: f64) -> Option<&LevelProfile> {
        self.profiles.get(tag).filter(|profile| {
            (profile.from_m_s - from).abs() < MINIMUM_SPEED_CHANGE_M_S
                && ((profile.target_m_s - to).abs() < MINIMUM_SPEED_CHANGE_M_S
                    || (profile.reached_m_s - to).abs() < MINIMUM_SPEED_CHANGE_M_S)
        })
    }
}

/// A level change that completed its own target, held to the target `to` the
/// leg after it asks for now: the speeds of its table are scaled between the
/// start and `to`, the duration kept and the distance recomputed from the scaled
/// speeds (the two targets differ by less than [`MINIMUM_SPEED_CHANGE_M_S`], so
/// the distance moves by the same small share).
/// A change that stopped short of its target (the excess force vanished) is left
/// as it was integrated.
pub(super) fn retarget(profile: &mut LevelProfile, to: f64) {
    let span = profile.reached_m_s - profile.from_m_s;
    let completed = (profile.reached_m_s - profile.target_m_s).abs() <= SPEED_IDENTITY_M_S;
    if !completed
        || span.abs() <= SPEED_IDENTITY_M_S
        || (profile.reached_m_s - to).abs() <= SPEED_IDENTITY_M_S
    {
        return;
    }
    let scale = (to - profile.from_m_s) / span;
    for row in &mut profile.table {
        row[1] = profile.from_m_s + (row[1] - profile.from_m_s) * scale;
    }
    if profile.duration_s > 0.0 {
        profile.distance_m = profile.duration_s
            * profile
                .table
                .windows(2)
                .map(|pair| 0.5 * (pair[0][1] + pair[1][1]) * (pair[1][0] - pair[0][0]))
                .sum::<f64>();
    }
    profile.target_m_s = to;
    profile.reached_m_s = to;
}

/// Whether `spec` is a speed-change leg.
pub(crate) fn is_speed_change(spec: &SegmentSpec) -> bool {
    spec.tag.starts_with(SPEED_CHANGE_PREFIX)
}

/// True airspeed a climb or descent leg flies at `altitude_m` on its own
/// schedule, or a cruise leg's speed, m/s. `None` when the state cannot be
/// resolved.
pub(crate) fn leg_speed_m_s(spec: &SegmentSpec, altitude_m: f64) -> Option<f64> {
    if matches!(spec.kind, SegmentKind::Cruise { .. }) {
        return Some(spec.air_speed_m_s);
    }
    if spec.air_speed_reference == SpeedReference::TrueAirspeed
        && spec.speed_caps == SpeedCaps::default()
    {
        return Some(spec.air_speed_m_s);
    }
    let ambient = us1976_try_compute_values(altitude_m, spec.temperature_deviation_k).ok()?;
    spec.speed_caps
        .true_airspeed_m_s(
            spec.air_speed_reference,
            spec.air_speed_m_s,
            ambient.pressure_pa,
            ambient.temperature_k,
        )
        .ok()
}

/// The certified speed limits (VMO and MMO of the type certificate data
/// sheet, when the preset declares them, at the pressure altitude of
/// `altitude_m`) as a true airspeed at `altitude_m`, m/s; `None` when none is
/// declared.
pub(super) fn certified_limit_m_s(
    caps: &SpeedCaps,
    temperature_deviation_k: f64,
    altitude_m: f64,
) -> Option<f64> {
    let ambient = us1976_try_compute_values(altitude_m, temperature_deviation_k).ok()?;
    let vmo_m_s = caps.max_cas_m_s_at(ambient.pressure_pa);
    let mmo = caps.max_mach_at(ambient.pressure_pa);
    let by_cas = (vmo_m_s > 0.0)
        .then(|| {
            alas_atmo::airspeed::true_from_calibrated(
                vmo_m_s,
                ambient.pressure_pa,
                ambient.temperature_k,
            )
            .ok()
        })
        .flatten();
    let by_mach = (mmo > 0.0)
        .then(|| alas_atmo::airspeed::true_from_mach(mmo, ambient.temperature_k).ok())
        .flatten();
    match (by_cas, by_mach) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// Constant-acceleration stand-in for a level change not yet integrated: the
/// BADA limit.
pub(super) fn fallback_profile(from: f64, to: f64) -> LevelProfile {
    LevelProfile {
        from_m_s: from,
        target_m_s: to,
        reached_m_s: to,
        distance_m: 0.5 * (from + to) * (to - from).abs() / LEVEL_ACCELERATION_LIMIT_M_S2,
        duration_s: (to - from).abs() / LEVEL_ACCELERATION_LIMIT_M_S2,
        table: vec![[0.0, from], [1.0, to]],
    }
}

/// Set the configuration drag of each speed-change leg to the mean of the legs
/// around it. Call after the configuration drag of the legs has been set.
pub(crate) fn set_junction_configuration(schedule: &mut [SegmentSpec]) {
    for index in 1..schedule.len().saturating_sub(1) {
        if is_speed_change(&schedule[index]) {
            schedule[index].configuration_delta_cd = 0.5
                * (schedule[index - 1].configuration_delta_cd
                    + schedule[index + 1].configuration_delta_cd);
        }
    }
}

/// Lengthen the level change at `index` that the aircraft could not fly as
/// integrated, by [`LENGTHENING_STEP`] in duration and distance, and
/// remember it. Returns `false` when the leg at `index` is not a speed-change
/// leg.
pub(crate) fn lengthen(
    schedule: &mut [SegmentSpec],
    index: usize,
    memory: &mut SpeedMemory,
) -> bool {
    let Some(next_tag) = schedule.get(index + 1).map(|spec| spec.tag.clone()) else {
        return false;
    };
    let Some(spec) = schedule.get_mut(index) else {
        return false;
    };
    if !is_speed_change(spec) {
        return false;
    }
    tracing::warn!(
        leg = %spec.tag,
        "lengthening a level speed change the aircraft could not fly as integrated"
    );
    let SegmentKind::Cruise {
        altitude_m,
        distance_m,
    } = spec.kind
    else {
        return false;
    };
    let lengthened = distance_m * LENGTHENING_STEP;
    spec.kind = SegmentKind::Cruise {
        altitude_m,
        distance_m: lengthened,
    };
    // Remember the change as the leg now stands: its own speeds are what the
    // next re-joining looks for.
    if let (Some(first), Some(last)) = (spec.speed_profile.first(), spec.speed_profile.last()) {
        let (from, to) = (first[1], last[1]);
        memory.insert(
            &next_tag,
            LevelProfile {
                from_m_s: from,
                target_m_s: to,
                reached_m_s: to,
                distance_m: lengthened,
                duration_s: 0.0,
                table: spec.speed_profile.clone(),
            },
        );
    }
    true
}

// Tests assert on schedules they just built, so a failed unwrap is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mission_stage::schedule::junction::join_speeds;
    use alas_config::mission::MissionProfileConfig;

    fn leg(tag: &str, kind: SegmentKind, speed: f64) -> SegmentSpec {
        SegmentSpec {
            tag: tag.to_owned(),
            kind,
            air_speed_m_s: speed,
            air_speed_reference: SpeedReference::TrueAirspeed,
            speed_caps: SpeedCaps::default(),
            true_course_rad: 0.0,
            temperature_deviation_k: 0.0,
            number_control_points: 16,
            takeoff_rated: false,
            rating_limited: false,
            configuration_delta_cd: 0.0,
            load_factor: 1.0,
            entry_speed_m_s: 0.0,
            exit_speed_m_s: 0.0,
            speed_profile: Vec::new(),
        }
    }

    fn climb(tag: &str, end_m: f64, speed: f64) -> SegmentSpec {
        leg(
            tag,
            SegmentKind::Climb {
                altitude_start_m: None,
                altitude_end_m: end_m,
                climb_rate_m_s: 10.0,
            },
            speed,
        )
    }

    fn cruise(tag: &str, speed: f64) -> SegmentSpec {
        leg(
            tag,
            SegmentKind::Cruise {
                altitude_m: None,
                distance_m: 0.0,
            },
            speed,
        )
    }

    fn descent(tag: &str, end_m: f64, speed: f64) -> SegmentSpec {
        leg(
            tag,
            SegmentKind::Descent {
                altitude_start_m: None,
                altitude_end_m: end_m,
                descent_rate_m_s: 8.0,
            },
            speed,
        )
    }

    fn tags(schedule: &[SegmentSpec]) -> Vec<&str> {
        schedule.iter().map(|spec| spec.tag.as_str()).collect()
    }

    fn level_speed_change_distance(spec: &SegmentSpec) -> f64 {
        let SegmentKind::Cruise { distance_m, .. } = spec.kind else {
            panic!("a speed change is a level leg");
        };
        distance_m
    }

    #[test]
    fn a_climb_that_accelerates_flies_the_change_along_its_path_by_the_energy_share_factor() {
        let mut schedule = vec![
            climb("takeoff", 500.0, 80.0),
            climb("initial_climb", 3000.0, 130.0),
        ];
        join_speeds(&mut schedule, 0.0, &SpeedMemory::default());
        assert_eq!(
            tags(&schedule),
            ["takeoff", "initial_climb_path_entry", "initial_climb"]
        );
        let (entry, rest) = (&schedule[1], &schedule[2]);
        assert_eq!(entry.entry_speed_m_s, 80.0);
        assert_eq!(rest.entry_speed_m_s, 0.0);
        // 80 -> 130 m/s by V^2 = V0^2 + 2 g (1 - f)/f dh with f = 0.3:
        let expected_m = (130.0_f64.powi(2) - 80.0_f64.powi(2))
            / (2.0 * alas_atmo::STANDARD_GRAVITY * (1.0 - 0.3) / 0.3);
        let SegmentKind::Climb {
            altitude_start_m,
            altitude_end_m,
            ..
        } = entry.kind
        else {
            panic!("the change is a climb piece");
        };
        assert_eq!(altitude_start_m, Some(500.0));
        assert!((altitude_end_m - 500.0 - expected_m).abs() < 1.0);
        let SegmentKind::Climb {
            altitude_start_m,
            altitude_end_m,
            ..
        } = rest.kind
        else {
            panic!("the rest is a climb");
        };
        assert!((altitude_start_m.unwrap() - 500.0 - expected_m).abs() < 1.0);
        assert_eq!(altitude_end_m, 3000.0);
    }

    #[test]
    fn matching_speeds_need_no_piece_and_rejoining_is_idempotent() {
        let memory = SpeedMemory::default();
        let mut schedule = vec![climb("a", 500.0, 80.0), climb("b", 3000.0, 80.0)];
        join_speeds(&mut schedule, 0.0, &memory);
        assert_eq!(tags(&schedule), ["a", "b"]);
        let mut jump = vec![climb("a", 500.0, 80.0), climb("b", 3000.0, 130.0)];
        join_speeds(&mut jump, 0.0, &memory);
        let once = jump.clone();
        join_speeds(&mut jump, 0.0, &memory);
        assert_eq!(jump, once);
        // A difference of a few tenths of a m/s is flown too.
        let mut small = vec![climb("a", 500.0, 80.0), climb("b", 3000.0, 80.4)];
        join_speeds(&mut small, 0.0, &memory);
        assert_eq!(tags(&small), ["a", "b_path_entry", "b"]);
    }

    #[test]
    fn a_descent_changes_speed_along_its_path_in_both_directions() {
        let memory = SpeedMemory::default();
        let mut gaining = vec![
            cruise("cruise_step_3", 140.0),
            descent("descent_1", 3000.0, 150.0),
        ];
        join_speeds(&mut gaining, 10_000.0, &memory);
        assert_eq!(
            tags(&gaining),
            ["cruise_step_3", "descent_1_path_entry", "descent_1"]
        );
        assert_eq!(gaining[1].entry_speed_m_s, 140.0);
        let mut slowing = vec![
            cruise("cruise_step_3", 240.0),
            descent("descent_1", 3000.0, 200.0),
        ];
        join_speeds(&mut slowing, 10_000.0, &memory);
        assert_eq!(
            tags(&slowing),
            ["cruise_step_3", "descent_1_path_entry", "descent_1"]
        );
        assert_eq!(slowing[1].entry_speed_m_s, 240.0);
    }

    #[test]
    fn a_climb_that_ends_below_the_cruise_speed_accelerates_in_its_last_piece() {
        let mut schedule = vec![climb("a", 3000.0, 118.0), cruise("cruise_step_1", 139.0)];
        join_speeds(&mut schedule, 0.0, &SpeedMemory::default());
        assert_eq!(tags(&schedule), ["a", "a_path_exit", "cruise_step_1"]);
        assert_eq!(schedule[1].exit_speed_m_s, 139.0);
        // Rejoining restores the same pieces from the merged leg.
        let once = schedule.clone();
        join_speeds(&mut schedule, 0.0, &SpeedMemory::default());
        assert_eq!(schedule, once);
    }

    #[test]
    fn a_short_climb_that_cannot_hold_the_change_leaves_it_to_a_level_leg() {
        let mut schedule = vec![climb("a", 3000.0, 118.0), climb("b", 3100.0, 118.0)];
        schedule.push(cruise("cruise_step_1", 160.0));
        join_speeds(&mut schedule, 0.0, &SpeedMemory::default());
        assert!(
            tags(&schedule).contains(&"speed_change_to_cruise_step_1"),
            "{:?}",
            tags(&schedule)
        );
    }

    #[test]
    fn a_change_marked_level_is_flown_level() {
        let mut memory = SpeedMemory::default();
        memory.prefer_level("a_path_exit");
        let mut schedule = vec![climb("a", 3000.0, 118.0), cruise("cruise_step_1", 139.0)];
        join_speeds(&mut schedule, 0.0, &memory);
        assert_eq!(
            tags(&schedule),
            ["a", "speed_change_to_cruise_step_1", "cruise_step_1"]
        );
    }

    #[test]
    fn a_retargeted_level_change_keeps_its_distance_consistent_with_its_speeds() {
        let mut profile = LevelProfile {
            from_m_s: 100.0,
            target_m_s: 140.0,
            reached_m_s: 140.0,
            distance_m: 120.0 * 60.0,
            duration_s: 60.0,
            table: vec![[0.0, 100.0], [0.5, 120.0], [1.0, 140.0]],
        };
        retarget(&mut profile, 141.0);
        assert_eq!(profile.duration_s, 60.0);
        // Speeds rise linearly from 100 to 141 m/s: mean 120.5 m/s over 60 s.
        assert!((profile.distance_m - 120.5 * 60.0).abs() < 1.0e-9);
        assert_eq!(profile.table.last().unwrap()[1], 141.0);
    }

    #[test]
    fn an_integrated_level_change_is_reused_and_what_it_cannot_reach_is_left_to_the_next_leg() {
        let mut memory = SpeedMemory::default();
        let mut schedule = vec![
            cruise("cruise_step_1", 118.0),
            cruise("cruise_step_2", 139.0),
        ];
        join_speeds(&mut schedule, 3000.0, &memory);
        // The deck reaches 134 m/s in 6 km and cannot do more.
        memory.insert(
            "cruise_step_2",
            LevelProfile {
                from_m_s: 118.0,
                target_m_s: 139.0,
                reached_m_s: 134.0,
                distance_m: 6_000.0,
                duration_s: 0.0,
                table: vec![[0.0, 118.0], [0.4, 128.0], [1.0, 134.0]],
            },
        );
        join_speeds(&mut schedule, 3000.0, &memory);
        assert_eq!(
            tags(&schedule),
            [
                "cruise_step_1",
                "speed_change_to_cruise_step_2",
                "cruise_step_2"
            ]
        );
        assert_eq!(level_speed_change_distance(&schedule[1]), 6_000.0);
        assert_eq!(
            schedule[2].air_speed_m_s, 134.0,
            "cruise at the speed it can reach"
        );
        // Re-joining keeps it (the cruise speed is now the reached one).
        join_speeds(&mut schedule, 3000.0, &memory);
        assert_eq!(schedule[1].speed_profile.len(), 3);
    }

    #[test]
    fn lengthening_multiplies_the_duration_and_is_remembered() {
        let mut memory = SpeedMemory::default();
        let mut schedule = vec![
            cruise("cruise_step_1", 118.0),
            cruise("cruise_step_2", 139.0),
        ];
        join_speeds(&mut schedule, 3000.0, &memory);
        let before = level_speed_change_distance(&schedule[1]);
        assert!(lengthen(&mut schedule, 1, &mut memory));
        assert!(
            !lengthen(&mut schedule, 0, &mut memory),
            "only speed-change legs lengthen"
        );
        join_speeds(&mut schedule, 3000.0, &memory);
        let after = level_speed_change_distance(&schedule[1]);
        assert!((after - before * LENGTHENING_STEP).abs() < 1.0e-9);
    }

    #[test]
    fn a_level_leg_is_held_to_the_certified_limits_and_only_to_them() {
        let mut climb_leg = climb("initial_climb", 3_600.0, 129.0);
        climb_leg.air_speed_reference = SpeedReference::CalibratedAirspeed;
        let limited = SpeedCaps {
            mach_hold: 0.0,
            max_mach: 0.82,
            max_cas_m_s: 180.0,
            ..SpeedCaps::default()
        };
        climb_leg.speed_caps = limited;
        let mut schedule = vec![climb_leg.clone(), cruise("cruise_step_1", 260.0)];
        join_speeds(&mut schedule, 0.0, &SpeedMemory::default());
        let limit = certified_limit_m_s(&limited, 0.0, 3_600.0).unwrap();
        assert!(limit < 260.0);
        assert!((schedule.last().unwrap().air_speed_m_s - limit).abs() < 1.0e-9);
        // No VMO / MMO declared: the declared cruise speed stands.
        climb_leg.speed_caps = SpeedCaps::default();
        let mut free = vec![climb_leg, cruise("cruise_step_1", 260.0)];
        join_speeds(&mut free, 0.0, &SpeedMemory::default());
        assert_eq!(free.last().unwrap().air_speed_m_s, 260.0);
    }

    #[test]
    fn a_level_leg_is_held_to_its_own_certified_limits_after_an_uncapped_leg() {
        let profile = MissionProfileConfig {
            climb_mach: 0.78,
            max_operating_mach: 0.81,
            max_operating_cas_m_s: 300.0 * 1_852.0 / 3_600.0,
            ..MissionProfileConfig::default()
        };
        let mut schedule = vec![
            climb("initial_climb", 1_900.0, 129.0),
            cruise("cruise_step_1", 240.0),
        ];
        crate::mission_stage::schedule::speed::apply_speed_caps(&mut schedule, &profile);
        let level_caps = schedule[1].speed_caps;
        assert_eq!(level_caps.mach_hold, 0.0);
        assert_eq!(level_caps.max_mach, 0.81);
        // The climb before it flies with no limits at all.
        schedule[0].speed_caps = SpeedCaps::default();
        join_speeds(&mut schedule, 0.0, &SpeedMemory::default());
        let limit = certified_limit_m_s(&level_caps, 0.0, 1_900.0).unwrap();
        assert!(limit < 240.0, "VMO at 1,900 m is {limit} m/s TAS");
        assert!((schedule.last().unwrap().air_speed_m_s - limit).abs() < 1.0e-9);
    }

    #[test]
    fn a_speed_leg_carries_the_mean_configuration_drag_of_its_neighbours() {
        let mut first = cruise("cruise_step_1", 118.0);
        first.configuration_delta_cd = 0.015;
        let mut schedule = vec![first, cruise("cruise_step_2", 139.0)];
        join_speeds(&mut schedule, 3000.0, &SpeedMemory::default());
        assert!((schedule[1].configuration_delta_cd - 0.0075).abs() < 1.0e-12);
        schedule[0].configuration_delta_cd = 0.02;
        set_junction_configuration(&mut schedule);
        assert!((schedule[1].configuration_delta_cd - 0.01).abs() < 1.0e-12);
    }

    #[test]
    fn a_cruise_leg_flies_the_rule_speed_at_its_own_level_whatever_the_policy() {
        // The rule stands in for the priced route's: the declared speed less 0.01 m/s per metre of
        // level (any monotone function shows the leg follows it).
        let rule = CruiseSpeedRule::new(|level_m, declared| Some(declared - 0.01 * level_m));
        let profile = alas_config::mission::MissionProfileConfig {
            cruise_1_air_speed_m_s: 200.0,
            cruise_2_air_speed_m_s: 210.0,
            ..alas_config::mission::MissionProfileConfig::default()
        };
        let memory = SpeedMemory::for_profile(&profile, Some(rule));
        let mut schedule = vec![
            climb("initial_climb", 8_000.0, 150.0),
            cruise("cruise_step_1", 999.0),
            climb("step_climb_1", 9_000.0, 150.0),
            cruise("cruise_step_2", 999.0),
        ];
        join_speeds(&mut schedule, 0.0, &memory);
        let speed_of = |tag: &str| {
            schedule
                .iter()
                .find(|spec| spec.tag == tag)
                .map(|spec| spec.air_speed_m_s)
                .unwrap()
        };
        assert!((speed_of("cruise_step_1") - (200.0 - 80.0)).abs() < 1.0e-9);
        assert!((speed_of("cruise_step_2") - (210.0 - 90.0)).abs() < 1.0e-9);
    }

    /// A completed integration maps onto a change between other speeds with its
    /// shape kept and its duration scaled by the speed difference; one that
    /// stopped short of its target, or runs the other way, does not map.
    #[test]
    fn a_remembered_change_maps_onto_new_speeds_at_the_same_acceleration() {
        let mut memory = SpeedMemory::default();
        let profile = |reached_m_s: f64| LevelProfile {
            from_m_s: 150.0,
            target_m_s: 170.0,
            reached_m_s,
            distance_m: 16_000.0,
            duration_s: 100.0,
            table: vec![[0.0, 150.0], [0.5, 165.0], [1.0, reached_m_s]],
        };
        memory.insert("leg", profile(170.0));
        let key = format!("{SPEED_CHANGE_PREFIX}leg");
        let mapped = memory.rescaled_for(&key, 154.0, 170.0).unwrap();
        assert_eq!(mapped.table, vec![[0.0, 154.0], [0.5, 166.0], [1.0, 170.0]]);
        assert!((mapped.duration_s - 80.0).abs() < 1.0e-9);
        // Distance is the duration times the table's mean speed.
        assert!(
            (mapped.distance_m - 80.0 * 0.5 * (0.5 * (154.0 + 166.0) + 0.5 * (166.0 + 170.0)))
                .abs()
                < 1.0e-6
        );
        assert!(memory.rescaled_for(&key, 170.0, 150.0).is_none());
        assert!(memory
            .rescaled_for(SPEED_CHANGE_PREFIX, 154.0, 170.0)
            .is_none());
        memory.insert("leg", profile(160.0));
        assert!(memory.rescaled_for(&key, 154.0, 170.0).is_none());
    }

    /// An exit change cut from a whole descent before the descent was cut into
    /// pieces (MM8) merges back into the last piece, so a later joining can fly
    /// it level when the level change is preferred (Commercial Transport plan:
    /// `descent_2@03_path_exit` after `descent_2@03_piece_3` stayed an orphan
    /// leg the preference never reached).
    #[test]
    fn an_exit_change_merges_back_into_the_last_descent_piece() {
        let span = |tag: &str, start_m: f64, end_m: f64| {
            let mut spec = descent(tag, end_m, 150.0);
            spec.kind = SegmentKind::Descent {
                altitude_start_m: Some(start_m),
                altitude_end_m: end_m,
                descent_rate_m_s: 8.0,
            };
            spec
        };
        let mut exit = span("descent_2@03_path_exit", 2_000.0, 1_800.0);
        exit.exit_speed_m_s = 130.0;
        let merged = crate::mission_stage::schedule::junction::merge_pieces(vec![
            span("descent_2@03", 3_000.0, 2_500.0),
            span("descent_2@03_piece_1", 2_500.0, 2_000.0),
            exit,
        ]);
        assert_eq!(tags(&merged), ["descent_2@03", "descent_2@03_piece_1"]);
        let SegmentKind::Descent {
            altitude_start_m,
            altitude_end_m,
            ..
        } = merged[1].kind
        else {
            panic!("the piece is a descent");
        };
        assert_eq!((altitude_start_m, altitude_end_m), (Some(2_500.0), 1_800.0));
    }

    /// The same exit change after the lower piece the guidance cut where the descent first
    /// reached flight idle merges into that piece: left an orphan (A350-900 Commercial Transport,
    /// `descent_2@03_piece_10_path_exit` after `descent_2@03_piece_10_lower`), its level
    /// preference never took effect and the rate pass stopped at it on every call.
    #[test]
    fn an_exit_change_merges_back_into_the_lower_idle_piece() {
        let span = |tag: &str, start_m: f64, end_m: f64| {
            let mut spec = descent(tag, end_m, 150.0);
            spec.kind = SegmentKind::Descent {
                altitude_start_m: Some(start_m),
                altitude_end_m: end_m,
                descent_rate_m_s: 8.0,
            };
            spec
        };
        let mut exit = span("descent_2@03_piece_1_path_exit", 2_000.0, 1_800.0);
        exit.exit_speed_m_s = 130.0;
        let merged = crate::mission_stage::schedule::junction::merge_pieces(vec![
            span("descent_2@03", 3_000.0, 2_500.0),
            span("descent_2@03_piece_1", 2_500.0, 2_200.0),
            span("descent_2@03_piece_1_lower", 2_200.0, 2_000.0),
            exit,
        ]);
        assert_eq!(
            tags(&merged),
            [
                "descent_2@03",
                "descent_2@03_piece_1",
                "descent_2@03_piece_1_lower"
            ]
        );
        let SegmentKind::Descent {
            altitude_start_m,
            altitude_end_m,
            ..
        } = merged[2].kind
        else {
            panic!("the piece is a descent");
        };
        assert_eq!((altitude_start_m, altitude_end_m), (Some(2_200.0), 1_800.0));
    }
}
