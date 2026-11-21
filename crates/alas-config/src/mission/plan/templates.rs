// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Raymer's example sizing mission profiles as stage lists.
//!
//! Source: Raymer, *Aircraft Design: A Conceptual Approach*, 6th ed.,
//! section 3.4.1 "Mission Profiles", Fig. 3.2 "Typical mission profiles for
//! sizing", p. 32, read from the rendered page. The figure draws four
//! profiles, with these segments:
//!
//! 1. Simple Cruise: take-off, climb, cruise, descend, loiter, land.
//! 2. Commercial Transport: take-off, climb, cruise, loiter, attempt to land,
//!    climb, divert, loiter, land.
//! 3. Low-Level Strike: take-off, climb, cruise out, loiter, weight drop
//!    (at low level), climb, cruise back, loiter, land.
//! 4. Air Superiority: take-off, climb, cruise out, combat, weight drop,
//!    cruise back, loiter, land.
//!
//! The figure fixes the structure; it states no distances, times, altitudes
//! or loads. Those are the study's inputs, set in [`TemplateInputs`] whose
//! defaults are placeholders to edit, not data of any aircraft. The low-level
//! leg of the strike profile is unlabelled in the figure; it is named
//! "low-level" here. A "weight drop" is a payload-release event of the stage
//! before it (release of every releasable item when no id is given).

use serde::{Deserialize, Serialize};

use crate::fuel_policy::FuelPolicyConfig;

use super::{CruiseExtent, EventAction, MissionPlan, PlanStage, StageKind};

/// Decision height of the approach a template attempts, ft: the Category I
/// limit. EASA CS-25 Amendment 27, CS 25.20(b)(2) (page 58 of 1495 of the Easy Access edition, Jan 2023): the Subpart B
/// requirements are written for approaches and landings with decision heights
/// down to 60 m (200 ft); lower decision heights need their own investigation.
pub const CATEGORY_I_DECISION_HEIGHT_FT: f64 = 200.0;

/// The four example mission profiles of Raymer's Fig. 3.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RaymerProfile {
    /// Take-off, climb, cruise, descend, loiter, land.
    SimpleCruise,
    /// Cruise, loiter, attempt to land, climb, divert, loiter, land.
    CommercialTransport,
    /// Cruise out, loiter, weight drop at low level, climb, cruise back.
    LowLevelStrike,
    /// Cruise out, combat, weight drop, cruise back, loiter.
    AirSuperiority,
}

impl RaymerProfile {
    /// Every profile, in the figure's order.
    pub const ALL: [Self; 4] = [
        Self::SimpleCruise,
        Self::CommercialTransport,
        Self::LowLevelStrike,
        Self::AirSuperiority,
    ];

    /// The name the figure gives the profile.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::SimpleCruise => "Simple Cruise",
            Self::CommercialTransport => "Commercial Transport",
            Self::LowLevelStrike => "Low-Level Strike",
            Self::AirSuperiority => "Air Superiority",
        }
    }
}

/// The quantities a template leaves to the study.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateInputs {
    /// Cruise altitude, ft.
    pub cruise_altitude_ft: f64,
    /// Outbound or only cruise distance, km; `None` closes the route.
    pub cruise_km: Option<f64>,
    /// Return cruise distance of the strike profiles, km.
    pub return_km: f64,
    /// Loiter duration, min.
    pub loiter_min: f64,
    /// Altitude of the low-level leg, ft above the departure field.
    pub low_level_ft: f64,
    /// Duration of the low-level run, min.
    pub low_level_min: f64,
    /// Combat duration, min.
    pub combat_min: f64,
    /// Combat load factor, g.
    pub combat_load_factor: f64,
    /// Alternate distance of the commercial profile, km.
    pub alternate_km: f64,
    /// Holding altitude above the field, ft: the level of the loiter that
    /// follows the descent of the simple-cruise profile.
    pub hold_altitude_ft: f64,
    /// Altitude above mean sea level where the approach is intercepted, ft:
    /// the profile's fourth descent rung, which the plan engine flies as an
    /// altitude above mean sea level ([`Self::with_profile`] reads it; 3 000 ft
    /// is that rung's default). The missed approach of the commercial profile
    /// climbs to this height above the field it was attempted at, so it goes
    /// up from the decision height over a field of any elevation. Raymer
    /// Fig. 3.2 draws the climb after "Attempt to land" ending at the diversion
    /// level, not at the cruise level, and gives no figure for it.
    pub missed_approach_ft: f64,
    /// Ceiling of the diversion's cruise, ft above mean sea level. The figure
    /// gives no level; the main cruise altitude is the ceiling, and the plan
    /// engine flies the highest level the diversion distance allows under it:
    /// it lowers the cruise until the climb and descent leave
    /// `MIN_CRUISE_ROUTE_FRACTION` of the distance for cruise
    /// (`fit_altitude_profile`; that constant is the engine's own, with no
    /// source of its own yet). A study states a level to override it.
    pub alternate_cruise_ft: f64,
    /// Altitude of the commercial profile's loiter before the attempt to land,
    /// ft above mean sea level. Raymer, 6th ed., p. 32 (section 3.4.1): "a
    /// loiter of typically 20-30 min [at 10,000 ft {3048 m}] is added"; Fig. 3.2
    /// draws it at a level below the cruise. 10 000 ft by default.
    pub loiter_ft: f64,
}

impl TemplateInputs {
    /// These inputs with the mission profile's own approach height: the
    /// missed approach climbs back to the rung the profile intercepted the
    /// approach at.
    #[must_use]
    pub fn with_profile(mut self, profile: &crate::MissionProfileConfig) -> Self {
        self.missed_approach_ft = profile.descent_4_altitude_ft;
        self
    }

    /// Placeholders drawn from the fuel policy where it has the quantity
    /// (the loiter and the alternate), and round numbers elsewhere.
    #[must_use]
    pub fn from_policy(policy: &FuelPolicyConfig, cruise_altitude_ft: f64) -> Self {
        Self {
            cruise_altitude_ft,
            cruise_km: None,
            return_km: 300.0,
            loiter_min: policy.final_reserve_hold_min,
            low_level_ft: 500.0,
            low_level_min: 5.0,
            combat_min: 5.0,
            combat_load_factor: 3.0,
            alternate_km: policy.alternate_distance_nmi * 1.852,
            hold_altitude_ft: policy.holding_altitude_ft,
            missed_approach_ft: 3_000.0,
            alternate_cruise_ft: cruise_altitude_ft,
            loiter_ft: 10_000.0,
        }
    }
}

fn takeoff() -> PlanStage {
    PlanStage::new("takeoff", StageKind::Takeoff { field: None })
}

fn climb(id: &str, altitude_ft: f64) -> PlanStage {
    PlanStage::new(
        id,
        StageKind::Climb {
            altitude_ft,
            agl: false,
            rate_m_s: None,
        },
    )
}

fn cruise(id: &str, extent: CruiseExtent) -> PlanStage {
    PlanStage::new(
        id,
        StageKind::Cruise {
            extent,
            altitude_ft: None,
            agl: false,
            speed_m_s: None,
        },
    )
}

fn loiter(id: &str, minutes: f64) -> PlanStage {
    PlanStage::new(
        id,
        StageKind::Loiter {
            time_min: minutes,
            altitude_ft: None,
            agl: false,
            speed_m_s: None,
        },
    )
}

/// A loiter at `altitude_ft` above mean sea level: the stage inserts the
/// descent or climb that reaches it.
fn loiter_at(id: &str, minutes: f64, altitude_ft: f64) -> PlanStage {
    PlanStage::new(
        id,
        StageKind::Loiter {
            time_min: minutes,
            altitude_ft: Some(altitude_ft),
            agl: false,
            speed_m_s: None,
        },
    )
}

fn landing(id: &str) -> PlanStage {
    PlanStage::new(
        id,
        StageKind::Landing {
            field_elevation_ft: None,
        },
    )
}

fn drop_run(inputs: &TemplateInputs) -> PlanStage {
    PlanStage::new(
        "weight-drop",
        StageKind::Cruise {
            extent: CruiseExtent::TimeMin(inputs.low_level_min),
            altitude_ft: Some(inputs.low_level_ft),
            agl: true,
            speed_m_s: None,
        },
    )
    .then(EventAction::PayloadRelease { items: Vec::new() })
}

impl MissionPlan {
    /// The stage list of one of Raymer's four example sizing profiles.
    #[must_use]
    pub fn raymer(profile: RaymerProfile, inputs: &TemplateInputs) -> Self {
        let out = cruise(
            "cruise-out",
            inputs
                .cruise_km
                .map_or(CruiseExtent::Remainder, CruiseExtent::DistanceKm),
        );
        let back = cruise("cruise-back", CruiseExtent::DistanceKm(inputs.return_km));
        let stages = match profile {
            RaymerProfile::SimpleCruise => vec![
                takeoff(),
                climb("climb", inputs.cruise_altitude_ft),
                out,
                PlanStage::new(
                    "descend",
                    StageKind::Descent {
                        altitude_ft: inputs.hold_altitude_ft,
                        agl: true,
                    },
                ),
                loiter("loiter", inputs.loiter_min),
                landing("land"),
            ],
            RaymerProfile::CommercialTransport => vec![
                takeoff(),
                climb("climb", inputs.cruise_altitude_ft),
                out,
                // The trip is the route down to the decision height of the
                // attempt to land: it descends to the loiter level, and the
                // approach follows the loiter, so both close the route
                // distance. The loiter itself (a level hold between route
                // legs), the missed approach, the diversion and its landing
                // are the profile's reserve: their fuel is carried, not trip
                // fuel, and they are flown beyond the route.
                PlanStage::new(
                    "descend",
                    StageKind::Descent {
                        altitude_ft: inputs.loiter_ft,
                        agl: false,
                    },
                ),
                loiter_at("loiter", inputs.loiter_min, inputs.loiter_ft).as_reserve(),
                PlanStage::new(
                    "attempt-to-land",
                    StageKind::Approach {
                        decision_height_ft: CATEGORY_I_DECISION_HEIGHT_FT,
                        field_elevation_ft: None,
                    },
                ),
                PlanStage::new(
                    "climb-after-go-around",
                    StageKind::GoAround {
                        // Above the field the approach was attempted at, the
                        // ground the approach stage has just set.
                        altitude_ft: inputs.missed_approach_ft,
                        agl: true,
                    },
                )
                .as_reserve(),
                PlanStage::new(
                    "divert",
                    StageKind::Diversion {
                        distance_km: inputs.alternate_km,
                        cruise_altitude_ft: inputs.alternate_cruise_ft,
                        field_elevation_ft: None,
                    },
                )
                .as_reserve(),
                loiter("loiter-at-alternate", inputs.loiter_min).as_reserve(),
                landing("land").as_reserve(),
            ],
            RaymerProfile::LowLevelStrike => vec![
                takeoff(),
                climb("climb", inputs.cruise_altitude_ft),
                out,
                loiter("loiter-out", inputs.loiter_min),
                drop_run(inputs),
                climb("climb-out", inputs.cruise_altitude_ft),
                back,
                loiter("loiter", inputs.loiter_min),
                landing("land"),
            ],
            RaymerProfile::AirSuperiority => vec![
                takeoff(),
                climb("climb", inputs.cruise_altitude_ft),
                out,
                PlanStage::new(
                    "combat",
                    StageKind::Combat {
                        time_min: inputs.combat_min,
                        load_factor: inputs.combat_load_factor,
                        altitude_ft: None,
                        agl: false,
                        speed_m_s: None,
                    },
                )
                .then(EventAction::PayloadRelease { items: Vec::new() }),
                back,
                loiter("loiter", inputs.loiter_min),
                landing("land"),
            ],
        };
        Self {
            name: profile.title().to_owned(),
            stages,
            ..Self::default()
        }
    }
}
