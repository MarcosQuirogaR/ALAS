// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The standard mission as a plan.

use crate::fuel_policy::FuelPolicyConfig;
use crate::mission::MissionProfileConfig;

use super::{CruiseExtent, MissionPlan, PlanStage, StageKind};

impl MissionPlan {
    /// The standard mission sequence as a plan: take-off, the climb to the
    /// cruise altitude, a cruise that closes the route, the descent to the
    /// last rung of the profile's descent ladder and the landing.
    ///
    /// The standard mission itself (an empty plan) keeps its three cruise
    /// legs with step climbs between them, the climb and descent ladders'
    /// speed changes and the exact route closure, and is what registered
    /// aircraft fly; this plan is the editable starting point that an edit of
    /// the mission begins from. Flown by the plan engine it uses the same
    /// profile speeds, rates and speed caps.
    #[must_use]
    pub fn from_profile(profile: &MissionProfileConfig, cruise_altitude_ft: f64) -> Self {
        Self {
            name: "Standard mission".to_owned(),
            stages: vec![
                PlanStage::new("takeoff", StageKind::Takeoff { field: None }),
                PlanStage::new(
                    "climb",
                    StageKind::Climb {
                        altitude_ft: cruise_altitude_ft,
                        agl: false,
                        rate_m_s: None,
                    },
                ),
                PlanStage::new(
                    "cruise",
                    StageKind::Cruise {
                        extent: CruiseExtent::Remainder,
                        altitude_ft: None,
                        agl: false,
                        speed_m_s: None,
                    },
                ),
                PlanStage::new(
                    "descent",
                    StageKind::Descent {
                        altitude_ft: profile.descent_4_altitude_ft,
                        agl: false,
                    },
                ),
                PlanStage::new(
                    "landing",
                    StageKind::Landing {
                        field_elevation_ft: None,
                    },
                ),
            ],
            ..Self::default()
        }
    }

    /// This plan with the reserves of `policy` flown as stages, in the order
    /// of Raymer's commercial transport profile (Fig. 3.2): after the cruise
    /// the aircraft holds, attempts to land, goes around, climbs, diverts to
    /// the alternate, holds again and lands. The contingency fraction is the
    /// policy's.
    ///
    /// The go-around climbs to `missed_approach_altitude_ft` above the field
    /// (the height the approach was intercepted at: the profile's fourth
    /// descent rung, ICAO Doc 8168 PANS-OPS), the approach attempts the Category I decision height
    /// ([`super::templates::CATEGORY_I_DECISION_HEIGHT_FT`]), and the
    /// diversion cruises at `alternate_cruise_altitude_ft`.
    ///
    /// The plan must end in a landing; the reserve stages go before it and
    /// that landing becomes the alternate's.
    #[must_use]
    pub fn with_policy_reserves(
        mut self,
        policy: &FuelPolicyConfig,
        alternate_cruise_altitude_ft: f64,
        missed_approach_altitude_ft: f64,
    ) -> Self {
        if !matches!(
            self.stages.last().map(|s| &s.kind),
            Some(StageKind::Landing { .. })
        ) {
            return self;
        }
        let landing = self.stages.pop();
        let hold_min = policy.final_reserve_hold_min;
        let reserves = [
            PlanStage::new(
                "approach",
                StageKind::Approach {
                    decision_height_ft: super::templates::CATEGORY_I_DECISION_HEIGHT_FT,
                    field_elevation_ft: None,
                },
            ),
            PlanStage::new(
                "go-around",
                StageKind::GoAround {
                    altitude_ft: missed_approach_altitude_ft,
                    agl: true,
                },
            ),
            PlanStage::new(
                "divert",
                StageKind::Diversion {
                    distance_km: policy.alternate_distance_nmi * 1.852,
                    cruise_altitude_ft: alternate_cruise_altitude_ft,
                    field_elevation_ft: None,
                },
            ),
            PlanStage::new(
                "final-reserve",
                StageKind::Loiter {
                    time_min: hold_min,
                    altitude_ft: Some(policy.holding_altitude_ft),
                    agl: true,
                    speed_m_s: None,
                },
            ),
        ];
        self.stages.extend(reserves.map(PlanStage::as_reserve));
        self.stages.extend(landing.map(PlanStage::as_reserve));
        self.contingency_fraction = policy.contingency_trip_fraction;
        self
    }
}
