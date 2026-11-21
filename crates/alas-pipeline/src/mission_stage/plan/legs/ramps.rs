// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The climb, descent and diversion legs of the plan builder.

use super::{ft, Builder, ClosureGroup, Rung};
use crate::mission_stage::schedule::cruise;

impl Builder<'_, '_> {
    /// The climb rungs of the profile above the current field, for a climb to
    /// `target_m`: the initial climb to its share of the target (at least
    /// 3,000 m above the field, as the standard mission has it), then the two
    /// step-climb rungs.
    pub(super) fn climb_rungs(&self, target_m: f64) -> [Rung; 3] {
        let p = self.ctx.profile;
        let first = (target_m * p.initial_climb_altitude_fraction).max(self.ground_m + 3000.0);
        let second = (target_m * p.step_climb_1_altitude_fraction).max(first + 300.0);
        [
            Rung {
                end_altitude_m: first,
                speed_m_s: p.initial_climb_air_speed_m_s,
                rate_m_s: p.initial_climb_rate_m_s,
            },
            Rung {
                end_altitude_m: second,
                speed_m_s: p.step_climb_1_air_speed_m_s,
                rate_m_s: p.step_climb_1_rate_m_s,
            },
            Rung {
                end_altitude_m: target_m,
                speed_m_s: p.step_climb_2_air_speed_m_s,
                rate_m_s: p.step_climb_2_rate_m_s,
            },
        ]
    }

    pub(super) fn climb_to(&mut self, target_m: f64, rate_override: Option<f64>) {
        let names = ["initial_climb", "step_climb_1", "step_climb_2"];
        for (rung, name) in self.climb_rungs(target_m).into_iter().zip(names) {
            if self.altitude_m >= target_m - 1.0 {
                break;
            }
            let end = rung.end_altitude_m.min(target_m);
            if end > self.altitude_m + 1.0 {
                let rate = rate_override
                    .unwrap_or(rung.rate_m_s)
                    .min(0.95 * rung.speed_m_s);
                self.push_climb(name, end, rung.speed_m_s, rate, false);
            }
        }
    }

    pub(super) fn descent_rungs(&self) -> [Rung; 4] {
        let p = self.ctx.profile;
        [
            (
                p.descent_1_altitude_ft,
                p.descent_1_air_speed_m_s,
                p.descent_1_rate_m_s,
            ),
            (
                p.descent_2_altitude_ft,
                p.descent_2_air_speed_m_s,
                p.descent_2_rate_m_s,
            ),
            (
                p.descent_3_altitude_ft,
                p.descent_3_air_speed_m_s,
                p.descent_3_rate_m_s,
            ),
            (
                p.descent_4_altitude_ft,
                p.descent_4_air_speed_m_s,
                p.descent_4_rate_m_s,
            ),
        ]
        .map(|(altitude_ft, speed_m_s, rate_m_s)| Rung {
            end_altitude_m: ft(altitude_ft),
            speed_m_s,
            rate_m_s,
        })
    }

    /// Where the ladder hands over to the landing configuration over a field at
    /// `ground_m`, for a landing leg down to `floor_m`, as the standard schedule
    /// does it: at the last rung above the floor, and no higher than the landing
    /// configuration's height above the field.
    pub(super) fn ladder_end_m(&self, ground_m: f64, floor_m: f64) -> f64 {
        let last_rung_m = self
            .descent_rungs()
            .iter()
            .map(|rung| rung.end_altitude_m)
            .filter(|altitude_m| *altitude_m > floor_m + 1.0)
            .fold(f64::INFINITY, f64::min);
        last_rung_m.min(ground_m + alas_config::mission::LANDING_CONFIGURATION_HEIGHT_M)
    }

    /// Descend the profile's ladder to `target_m`; a target between two rungs
    /// is flown on the rung that band belongs to.
    pub(super) fn descend_to(&mut self, target_m: f64) {
        let rungs = self.descent_rungs();
        for (index, rung) in rungs.iter().enumerate() {
            if self.altitude_m <= target_m + 1.0 {
                return;
            }
            let end = rung.end_altitude_m.max(target_m);
            if rung.end_altitude_m < self.altitude_m - 1.0 && end < self.altitude_m - 1.0 {
                self.push_descent(
                    &format!("descent_{}", index + 1),
                    end,
                    rung.speed_m_s,
                    rung.rate_m_s,
                );
            }
        }
        if self.altitude_m > target_m + 1.0 {
            let last = &rungs[3];
            self.push_descent("descent_4", target_m, last.speed_m_s, last.rate_m_s);
        }
    }

    /// Level legs at `altitude`, first reaching it when it differs from the
    /// current altitude.
    pub(super) fn reach(&mut self, altitude_ft: Option<f64>, agl: bool) {
        let Some(value) = altitude_ft else { return };
        let target = ft(value) + if agl { self.ground_m } else { 0.0 };
        if target > self.altitude_m + 30.0 {
            self.climb_to(target, None);
        } else if target < self.altitude_m - 30.0 {
            self.descend_to(target);
        }
    }

    /// Climb, cruise and descent over `distance_km` to a field. The cruise takes
    /// what the climb and the descent leave of the distance; the closure lowers
    /// the cruise altitude when they leave less than the profile's minimum share.
    pub(super) fn diversion(
        &mut self,
        distance_km: f64,
        cruise_ft: f64,
        elevation_ft: Option<f64>,
    ) {
        let total_m = distance_km * 1000.0;
        let destination_m = self.field_m(elevation_ft);
        let end_m = self.ladder_end_m(destination_m, destination_m);
        let (start_altitude, top_m) = (self.altitude_m, ft(cruise_ft));
        let before = self.specs.len();
        if top_m > self.altitude_m + 1.0 {
            self.climb_to(top_m, None);
        }
        let climb_end = self.specs.len();
        self.descend_to(end_m.min(top_m.max(self.altitude_m)));
        let footprint: f64 = crate::mission_stage::plan::closure::horizontal_distances(
            &self.specs[before..],
            start_altitude,
        )
        .iter()
        .sum();
        let tag = self.tag("divert_cruise");
        let declared = self.declared_cruise_speed();
        self.memory.declare_cruise(&tag, declared);
        let distance_m = (total_m - footprint).max(0.0);
        let spec = cruise(&tag, None, distance_m, declared, self.ctx.isa_deviation_k);
        self.specs.insert(climb_end, spec);
        self.groups.push(ClosureGroup {
            cruise_tags: vec![tag],
            stage: Some(self.stage),
            total_m,
            end_elevation_m: destination_m,
            fit: true,
        });
        self.ground_m = destination_m;
    }
}
