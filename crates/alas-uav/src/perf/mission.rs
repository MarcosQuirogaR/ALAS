// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Flies a list of `spec::mission::Segment`s and returns the time series.
//!
//! Interpretation of the frozen segment fields (documented here because the
//! schema only names them):
//! - Mission time starts at throttle-up of the first `Takeoff`; `Loading` is
//!   ground time outside the simulated clock and only accumulates
//!   `loading_time_s`.
//! - `Climb` with `Until::Time { seconds }` ends at flight-time `seconds`
//!   measured from throttle-up when `flight_clock_from_throttle_up` is set
//!   (ACC: the 60 s climb includes the ground roll) and `seconds` after the
//!   start of the climb otherwise (XC: 60 s after lift-off). For every other
//!   kind `Until::Time` is the duration of the segment.
//! - `Cruise` is level flight at the altitude the climb reached; altitude
//!   traded for speed is not modelled (conservative for ACC).
//! - `Laps { lap_length_m, count }`: a lap is two straight legs of
//!   `lap_length_m / 2` ground distance joined by 180 degree turns at the
//!   configured bank (turn path length is added to the straights). A segment
//!   of `count` laps ends after its last straight, so it has `2 count - 1`
//!   turns (XC: ten 100 m passes, nine turns). Legs alternate +w and -w
//!   headwind; turns ignore the wind. `window_s` ends the segment at a time
//!   cap and only completed legs count: `lap_count = legs / 2`.
//! - Wind is applied only to the ground roll, the landing and the laps legs.

use std::collections::BTreeMap;
use std::f64::consts::PI;

use crate::spec::mission::{Segment, SegmentKind, Until};

use super::airframe::Airframe;
use super::flight::{End, PathModel, ThrottleMode, Vertical};
use super::ground::{takeoff_from, TakeoffSpec};
use super::integrate::{run, Sample, IE, IH, IPSI, IV, IX, NY, Y};
use super::landing::{land, LandingSpec};
use super::powertrain::Powertrain;
use super::{Environment, PerfError};

/// Throttle law on level segments.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CruiseControl {
    /// Hold a true airspeed (throttle solved each step).
    HoldSpeed(f64),
    /// Constant throttle: speed settles where thrust equals drag.
    Throttle(f64),
}

/// Mission flying parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MissionConfig {
    /// Integration step, s (<= 0.05).
    pub dt_s: f64,
    /// Take-off parameters.
    pub takeoff: TakeoffSpec,
    /// Climb speed, m/s (default 1.3 Vs).
    pub climb_speed_m_s: Option<f64>,
    /// Climb throttle.
    pub climb_throttle: f64,
    /// Level-flight law.
    pub cruise: CruiseControl,
    /// Bank of the 180 degree turns, rad.
    pub turn_bank_rad: f64,
    /// Glide speed, m/s (default best glide).
    pub glide_speed_m_s: Option<f64>,
    /// Landing parameters.
    pub landing: LandingSpec,
    /// Time the flight clock from throttle-up (ACC) rather than lift-off.
    pub flight_clock_from_throttle_up: bool,
    /// Time cap on `Laps` segments, s (DBF 300 s window).
    pub window_s: Option<f64>,
    /// Speed-schedule time constant, s.
    pub speed_tau_s: f64,
}

impl Default for MissionConfig {
    fn default() -> Self {
        Self {
            dt_s: 0.05,
            takeoff: TakeoffSpec::default(),
            climb_speed_m_s: None,
            climb_throttle: 1.0,
            cruise: CruiseControl::Throttle(1.0),
            turn_bank_rad: 45.0_f64.to_radians(),
            glide_speed_m_s: None,
            landing: LandingSpec::default(),
            flight_clock_from_throttle_up: false,
            window_s: None,
            speed_tau_s: 2.0,
        }
    }
}

/// Per-segment summary.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentSummary {
    /// Segment name.
    pub name: String,
    /// Segment kind tag.
    pub kind: &'static str,
    /// `Until::Time` seconds of the segment, if it has one.
    pub until_seconds: Option<f64>,
    /// Start time, s.
    pub t_start: f64,
    /// End time, s.
    pub t_end: f64,
    /// Ground distance at the start, m.
    pub x_start: f64,
    /// Ground distance at the end, m.
    pub x_end: f64,
    /// Height at the end, m.
    pub h_end: f64,
    /// Battery energy used in the segment, J.
    pub energy_j: f64,
    /// Largest battery current in the segment, A.
    pub peak_current_a: f64,
    /// Kind-specific numbers (take-off distance, laps, rollout, ...).
    pub detail: BTreeMap<&'static str, f64>,
}

/// A flown mission.
#[derive(Debug, Clone, PartialEq)]
pub struct MissionResult {
    /// All samples in time order.
    pub samples: Vec<Sample>,
    /// One summary per segment.
    pub segments: Vec<SegmentSummary>,
    /// Sum of `Loading` segments, s.
    pub loading_time_s: f64,
    /// Time of lift-off, s.
    pub liftoff_time_s: Option<f64>,
}

impl MissionResult {
    /// Summary of the first segment with this name.
    pub fn segment(&self, name: &str) -> Option<&SegmentSummary> {
        self.segments.iter().find(|s| s.name == name)
    }

    /// Largest battery current, A.
    pub fn peak_current_a(&self) -> f64 {
        self.samples.iter().map(|s| s.current_a).fold(0.0, f64::max)
    }
}

fn until_seconds(kind: &SegmentKind) -> Option<f64> {
    match kind {
        SegmentKind::Climb {
            end: Until::Time { seconds },
        }
        | SegmentKind::Cruise {
            end: Until::Time { seconds },
            ..
        }
        | SegmentKind::Glide {
            end: Until::Time { seconds },
        } => Some(*seconds),
        _ => None,
    }
}

fn kind_tag(kind: &SegmentKind) -> &'static str {
    match kind {
        SegmentKind::Loading { .. } => "loading",
        SegmentKind::Takeoff { .. } => "takeoff",
        SegmentKind::Climb { .. } => "climb",
        SegmentKind::Cruise { .. } => "cruise",
        SegmentKind::Laps { .. } => "laps",
        SegmentKind::Glide { .. } => "glide",
        SegmentKind::Landing { .. } => "landing",
        SegmentKind::Delivery => "delivery",
        SegmentKind::Launch { .. } => "launch",
    }
}

struct Flight<'a> {
    af: &'a Airframe,
    pt: &'a dyn Powertrain,
    env: Environment,
    cfg: MissionConfig,
    samples: Vec<Sample>,
    t: f64,
    y: Y,
    flight_t0: Option<f64>,
}

impl<'a> Flight<'a> {
    fn path(
        &self,
        vertical: Vertical,
        throttle: ThrottleMode,
        v_cmd: f64,
        end: End,
        label: &'static str,
    ) -> PathModel<'a> {
        let mut m = PathModel::new(
            self.af, self.pt, self.env, vertical, throttle, v_cmd, end, label,
        );
        m.tau = self.cfg.speed_tau_s;
        m
    }

    fn level_law(&self) -> (ThrottleMode, f64) {
        match self.cfg.cruise {
            CruiseControl::HoldSpeed(v) => (ThrottleMode::HoldSpeed, v),
            CruiseControl::Throttle(th) => (ThrottleMode::Fixed(th), 0.0),
        }
    }

    fn go(&mut self, mut m: PathModel<'a>) -> Result<(), PerfError> {
        let end = run(
            &mut m,
            self.t,
            self.y,
            self.cfg.dt_s,
            3600.0,
            &mut self.samples,
        )?;
        self.t = end.t;
        self.y = end.y;
        Ok(())
    }

    fn end_for(
        &self,
        until: &Until,
        climbing: bool,
        descending: bool,
        what: &str,
    ) -> Result<End, PerfError> {
        Ok(match *until {
            Until::Time { seconds } => {
                if climbing && self.cfg.flight_clock_from_throttle_up {
                    End::Time(self.flight_t0.unwrap_or(0.0) + seconds)
                } else {
                    End::Time(self.t + seconds)
                }
            }
            Until::Distance { meters } => End::Distance(self.y[IX] + meters),
            Until::Altitude { meters } if descending => End::AltitudeDown(meters),
            Until::Altitude { meters } => End::AltitudeUp(meters),
            Until::Laps { .. } => {
                return Err(PerfError::InvalidInput(format!(
                    "{what}: Until::Laps is only valid for Laps segments"
                )))
            }
        })
    }
}

/// Fly `segments` in order from a standing start.
pub fn fly_mission(
    af: &Airframe,
    pt: &dyn Powertrain,
    env: Environment,
    segments: &[Segment],
    cfg: &MissionConfig,
) -> Result<MissionResult, PerfError> {
    af.validate()?;
    if !(cfg.speed_tau_s > 0.0 && cfg.climb_throttle >= 0.0 && cfg.climb_throttle <= 1.0) {
        return Err(PerfError::InvalidInput(
            "mission configuration out of range".into(),
        ));
    }
    let vs = af.stall_speed(env.rho);
    let mut fl = Flight {
        af,
        pt,
        env,
        cfg: *cfg,
        samples: Vec::new(),
        t: 0.0,
        y: [0.0; NY],
        flight_t0: None,
    };
    let mut summaries = Vec::new();
    let mut loading = 0.0;
    let mut liftoff = None;
    for (idx, seg) in segments.iter().enumerate() {
        let (t0, x0, e0, first) = (fl.t, fl.y[IX], fl.y[IE], fl.samples.len());
        let mut detail: BTreeMap<&'static str, f64> = BTreeMap::new();
        if !matches!(
            seg.kind,
            SegmentKind::Loading { .. } | SegmentKind::Takeoff { .. }
        ) && fl.flight_t0.is_none()
        {
            return Err(PerfError::InvalidInput(format!(
                "segment '{}': the mission must take off before flying {}",
                seg.name,
                kind_tag(&seg.kind)
            )));
        }
        match &seg.kind {
            SegmentKind::Loading { seconds } => {
                if !(seconds.is_finite() && *seconds >= 0.0) {
                    return Err(PerfError::InvalidInput("loading time must be >= 0".into()));
                }
                loading += seconds;
            }
            SegmentKind::Takeoff { runway_m, .. } => {
                if fl.flight_t0.is_some() {
                    return Err(PerfError::InvalidInput("a mission has one take-off".into()));
                }
                fl.flight_t0 = Some(fl.t);
                fl.y[IV] = env.headwind_m_s;
                let mut spec = cfg.takeoff;
                spec.runway_m = Some(*runway_m);
                let end = takeoff_from(af, pt, env, spec, fl.t, fl.y, cfg.dt_s, &mut fl.samples)?;
                fl.t = end.t;
                fl.y = end.y;
                liftoff = Some(end.t);
                detail.insert("takeoff_distance_m", end.y[IX]);
                detail.insert("liftoff_airspeed_m_s", end.y[IV]);
                detail.insert("liftoff_over_stall", end.y[IV] / vs);
                detail.insert("within_runway", f64::from(u8::from(end.y[IX] <= *runway_m)));
            }
            SegmentKind::Climb { end } => {
                let v = cfg.climb_speed_m_s.unwrap_or(1.3 * vs);
                let e = fl.end_for(end, true, false, &seg.name)?;
                let m = fl.path(
                    Vertical::Balance,
                    ThrottleMode::Fixed(cfg.climb_throttle),
                    v,
                    e,
                    "climb",
                );
                fl.go(m)?;
            }
            SegmentKind::Cruise {
                end,
                altitude_window_m,
            } => {
                let (mode, v) = fl.level_law();
                let e = fl.end_for(end, false, false, &seg.name)?;
                if matches!(e, End::AltitudeUp(_) | End::AltitudeDown(_)) {
                    return Err(PerfError::InvalidInput(
                        "a level cruise cannot end on an altitude".into(),
                    ));
                }
                let h = fl.y[IH];
                let ok = h >= altitude_window_m[0] && h <= altitude_window_m[1];
                detail.insert("altitude_window_ok", f64::from(u8::from(ok)));
                let m = fl.path(Vertical::Level, mode, v, e, "cruise");
                fl.go(m)?;
            }
            SegmentKind::Laps {
                lap_length_m,
                turn_gate,
                count,
            } => {
                fly_laps(&mut fl, *lap_length_m, *turn_gate, *count, &mut detail)?;
            }
            SegmentKind::Glide { end } => {
                let v = cfg
                    .glide_speed_m_s
                    .unwrap_or_else(|| af.best_glide_speed(env.rho));
                let e = fl.end_for(end, false, true, &seg.name)?;
                let m = fl.path(Vertical::Balance, ThrottleMode::Fixed(0.0), v, e, "glide");
                fl.go(m)?;
            }
            SegmentKind::Landing { zone_m } => {
                let mut spec = cfg.landing;
                spec.zone_m = *zone_m;
                let r = land(af, pt, env, spec, fl.t, fl.y, cfg.dt_s)?;
                fl.samples.extend(r.samples.iter().skip(1).copied());
                fl.t = r.end_time_s;
                fl.y = r.end_state;
                detail.insert("rollout_m", r.rollout_m);
                detail.insert("flare_distance_m", r.flare_distance_m);
                detail.insert("touchdown_speed_m_s", r.touchdown_speed_m_s);
                detail.insert("touchdown_over_stall", r.touchdown_over_stall);
                if let Some(ok) = r.within_zone {
                    detail.insert("within_zone", f64::from(u8::from(ok)));
                }
            }
            SegmentKind::Delivery => {
                return Err(PerfError::Unsupported(
                    "delivery (SAE Advanced) is not simulated in v1".into(),
                ))
            }
            SegmentKind::Launch { method } => {
                return Err(PerfError::Unsupported(format!(
                    "launch '{method}' (decision D2: no hand launch in v1)"
                )))
            }
        }
        for s in &mut fl.samples[first..] {
            s.segment = idx;
        }
        let peak = fl.samples[first..]
            .iter()
            .map(|s| s.current_a)
            .fold(0.0, f64::max);
        summaries.push(SegmentSummary {
            name: seg.name.clone(),
            kind: kind_tag(&seg.kind),
            until_seconds: until_seconds(&seg.kind),
            t_start: t0,
            t_end: fl.t,
            x_start: x0,
            x_end: fl.y[IX],
            h_end: fl.y[IH],
            energy_j: fl.y[IE] - e0,
            peak_current_a: peak,
            detail,
        });
    }
    Ok(MissionResult {
        samples: fl.samples,
        segments: summaries,
        loading_time_s: loading,
        liftoff_time_s: liftoff,
    })
}

fn fly_laps(
    fl: &mut Flight<'_>,
    lap_length_m: f64,
    turn_gate: bool,
    count: u32,
    detail: &mut BTreeMap<&'static str, f64>,
) -> Result<(), PerfError> {
    if !(lap_length_m.is_finite() && lap_length_m > 0.0 && count > 0) {
        return Err(PerfError::InvalidInput(
            "laps need a positive length and count".into(),
        ));
    }
    if turn_gate && !(fl.cfg.turn_bank_rad > 0.0 && fl.cfg.turn_bank_rad < 1.5) {
        return Err(PerfError::InvalidInput(
            "turn bank must be in (0, 1.5) rad".into(),
        ));
    }
    if turn_gate && 1.0 / fl.cfg.turn_bank_rad.cos() > fl.af.n_max {
        return Err(PerfError::InvalidInput(format!(
            "turn load factor {:.2} exceeds n_max {:.2}",
            1.0 / fl.cfg.turn_bank_rad.cos(),
            fl.af.n_max
        )));
    }
    let leg = 0.5 * lap_length_m;
    let seg_t0 = fl.t;
    let cap = fl.cfg.window_s.map(|w| seg_t0 + w);
    let (mode, v) = fl.level_law();
    let legs_total = 2 * count;
    let mut legs_done = 0_u32;
    for i in 0..legs_total {
        let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
        let mut m = fl.path(
            Vertical::Level,
            mode,
            v,
            End::Distance(fl.y[IX] + leg),
            "laps straight",
        );
        m.wind = sign * fl.env.headwind_m_s;
        m.cap_time = cap;
        fl.go(m)?;
        if cap.is_some_and(|c| fl.t >= c - 1e-9) {
            break;
        }
        legs_done += 1;
        if turn_gate && i + 1 < legs_total {
            let mut m = fl.path(
                Vertical::Level,
                mode,
                v,
                End::Heading(fl.y[IPSI] + PI),
                "laps turn",
            );
            m.bank = fl.cfg.turn_bank_rad;
            m.cap_time = cap;
            fl.go(m)?;
            if cap.is_some_and(|c| fl.t >= c - 1e-9) {
                break;
            }
        }
    }
    detail.insert("legs", f64::from(legs_done));
    detail.insert("laps", f64::from(legs_done / 2));
    detail.insert("lap_length_m", lap_length_m);
    Ok(())
}
