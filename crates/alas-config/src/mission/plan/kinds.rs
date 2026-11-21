// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The stages a mission plan is made of, and the events attached to them.
//!
//! Units follow the field names (`_ft`, `_km`, `_min`, `_m_s`, `_kg`, `_m2`):
//! altitudes and times are in the units a flight plan is written in, and the
//! mission engine converts them once, at its boundary. Every optional field
//! falls back to the aircraft's own [`crate::MissionProfileConfig`] speed and
//! rate schedule, so a plan states what the study wants and the aircraft
//! supplies how it flies.

use serde::{Deserialize, Serialize};

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

/// How far a cruise stage goes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CruiseExtent {
    /// A ground distance, km.
    DistanceKm(f64),
    /// A time at the cruise speed, min (still air).
    TimeMin(f64),
    /// The route distance the plan is flown over, less the horizontal
    /// footprint of every other stage: the standard mission's closure.
    Remainder,
}

/// A field a take-off is checked at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TakeoffField {
    /// Display name.
    #[serde(default)]
    pub name: String,
    /// Field elevation, ft.
    pub elevation_ft: f64,
    /// Take-off distance available, m (the runway length for a conservative
    /// check: TORA = TODA = ASDA).
    pub toda_m: f64,
    /// Temperature deviation from the standard atmosphere, K.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub isa_deviation_k: f64,
}

/// What a stage does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum StageKind {
    /// Ground time at idle thrust before take-off or after landing.
    Taxi {
        /// Duration, min.
        time_min: f64,
    },
    /// Lift-off and the climb out to the take-off acceleration height, in
    /// the take-off configuration.
    Takeoff {
        /// The field this take-off is checked at: the critical-field case, one
        /// engine failing at V1 and the take-off continued (CS 25.113), at the
        /// aircraft's mass when the stage starts, against the field's distance
        /// available. Absent: no field check beyond the departure airport's.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        field: Option<TakeoffField>,
    },
    /// Climb to an altitude on the aircraft's climb speed and rate schedule.
    Climb {
        /// Altitude reached, ft.
        altitude_ft: f64,
        /// Whether the altitude is above the departure field, not above mean
        /// sea level (flat terrain assumed).
        #[serde(default, skip_serializing_if = "is_false")]
        agl: bool,
        /// Rate of climb, m/s; the schedule's own when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rate_m_s: Option<f64>,
    },
    /// Level flight.
    Cruise {
        /// How far it goes.
        extent: CruiseExtent,
        /// Altitude held, ft; the altitude the stage starts at when absent.
        /// A different altitude inserts the climb or descent that reaches it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        altitude_ft: Option<f64>,
        /// Whether `altitude_ft` is above the departure field.
        #[serde(default, skip_serializing_if = "is_false")]
        agl: bool,
        /// True airspeed, m/s; the profile's cruise speed when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        speed_m_s: Option<f64>,
    },
    /// A hold or loiter: level flight for a time, at the speed of minimum
    /// drag (minimum power for a propeller) unless one is given.
    Loiter {
        /// Duration, min.
        time_min: f64,
        /// Altitude held, ft; the altitude the stage starts at when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        altitude_ft: Option<f64>,
        /// Whether `altitude_ft` is above the departure field.
        #[serde(default, skip_serializing_if = "is_false")]
        agl: bool,
        /// True airspeed, m/s.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        speed_m_s: Option<f64>,
    },
    /// Sustained level turning flight (Raymer's combat segment): lift is
    /// `load_factor` times the weight.
    Combat {
        /// Duration, min.
        time_min: f64,
        /// Load factor, g (at least 1).
        load_factor: f64,
        /// Altitude held, ft; the altitude the stage starts at when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        altitude_ft: Option<f64>,
        /// Whether `altitude_ft` is above the departure field.
        #[serde(default, skip_serializing_if = "is_false")]
        agl: bool,
        /// True airspeed, m/s; the profile's cruise speed when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        speed_m_s: Option<f64>,
    },
    /// Descent to an altitude on the aircraft's descent schedule.
    Descent {
        /// Altitude reached, ft.
        altitude_ft: f64,
        /// Whether the altitude is above the departure field.
        #[serde(default, skip_serializing_if = "is_false")]
        agl: bool,
    },
    /// The approach in the landing configuration down to the decision
    /// height; followed by a [`Self::Landing`] or a [`Self::GoAround`].
    Approach {
        /// Decision height above the landing field, ft.
        decision_height_ft: f64,
        /// Landing field elevation, ft; the arrival airport's when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        field_elevation_ft: Option<f64>,
    },
    /// The missed approach: climb back from the decision height.
    GoAround {
        /// Altitude reached, ft: above mean sea level, or above the landing
        /// field with `agl`. It must be above where the approach ended.
        altitude_ft: f64,
        /// Whether `altitude_ft` is above the landing field.
        #[serde(default, skip_serializing_if = "is_false")]
        agl: bool,
    },
    /// A flight to an alternate: climb, cruise and descent to the pattern
    /// altitude over a ground distance.
    Diversion {
        /// Ground distance to the alternate, km.
        distance_km: f64,
        /// Cruise altitude, ft.
        cruise_altitude_ft: f64,
        /// Alternate field elevation, ft; the arrival airport's when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        field_elevation_ft: Option<f64>,
    },
    /// The final descent to touchdown in the landing configuration.
    Landing {
        /// Landing field elevation, ft; the arrival airport's when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        field_elevation_ft: Option<f64>,
    },
}

impl StageKind {
    /// The stage's name in the stage list.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Taxi { .. } => "Taxi",
            Self::Takeoff { .. } => "Takeoff",
            Self::Climb { .. } => "Climb",
            Self::Cruise { .. } => "Cruise",
            Self::Loiter { .. } => "Loiter",
            Self::Combat { .. } => "Combat",
            Self::Descent { .. } => "Descent",
            Self::Approach { .. } => "Approach",
            Self::GoAround { .. } => "Go Around",
            Self::Diversion { .. } => "Diversion",
            Self::Landing { .. } => "Landing",
        }
    }
}

/// Where in its stage an event happens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventTime {
    /// Before the stage's first leg.
    Start,
    /// After the stage's last leg.
    #[default]
    End,
}

/// What an event changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum EventAction {
    /// Release declared point masses (`releasable`): mass, centre of gravity
    /// and the carried drag change at once. Empty releases every releasable
    /// item still carried.
    PayloadRelease {
        /// Point-mass ids.
        #[serde(default)]
        items: Vec<String>,
    },
    /// Hang a new store on the aircraft.
    StoreAdd {
        /// Identifier (a later release refers to it).
        id: String,
        /// Mass, kg.
        mass_kg: f64,
        /// Station, m aft of the nose tip.
        x_m: f64,
        /// Lateral position, m.
        #[serde(default)]
        y_m: f64,
        /// Height, m.
        z_m: f64,
        /// Parasite drag area while carried, m2.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        drag_area_m2: Option<f64>,
    },
    /// Take on (positive) or give away (negative) fuel in flight.
    AerialRefuel {
        /// Fuel mass, kg; positive received, negative offloaded.
        mass_kg: f64,
    },
    /// Move fuel from one tank to another.
    FuelTransfer {
        /// Source tank id.
        from: String,
        /// Destination tank id.
        to: String,
        /// Mass moved, kg.
        mass_kg: f64,
    },
    /// Hold the aircraft's centre of gravity inside a band with the trim
    /// tank from this event until the next trim event: fuel is moved between
    /// the trim tank and the feed tanks as the fuel burns, within the tank's
    /// capacity.
    TrimTankControl {
        /// Forward edge of the band, percent MAC.
        target_min_pct_mac: f64,
        /// Aft edge of the band, percent MAC.
        target_max_pct_mac: f64,
    },
    /// Release the trim-tank control: the trim fuel is moved forward into the
    /// feed tanks at once.
    TrimTankOff,
}

/// One event of a stage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageEvent {
    /// When in the stage it happens.
    #[serde(default)]
    pub at: EventTime,
    /// What it does.
    pub action: EventAction,
}

/// One stage of a mission plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanStage {
    /// Unique identifier, kept when the stage is reordered.
    pub id: String,
    /// What the stage does.
    pub kind: StageKind,
    /// Mass and tank events; they travel with the stage.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<StageEvent>,
    /// Whether the stage is a reserve (its fuel is carried, not trip fuel).
    #[serde(default, skip_serializing_if = "is_false")]
    pub reserve: bool,
    /// Parasite drag area added while the stage is flown, m2 (an open ramp,
    /// a deployed refuelling probe).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub extra_drag_area_m2: f64,
    /// Where the added drag area comes from: a measurement, a source, or "study
    /// input" when it is the author's own assumption. Carried with the plan so
    /// a reader of the results sees what the figure rests on.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub extra_drag_basis: String,
}

impl PlanStage {
    /// A stage with no events.
    #[must_use]
    pub fn new(id: &str, kind: StageKind) -> Self {
        Self {
            id: id.to_owned(),
            kind,
            events: Vec::new(),
            reserve: false,
            extra_drag_area_m2: 0.0,
            extra_drag_basis: String::new(),
        }
    }

    /// This stage with `action` at its end.
    #[must_use]
    pub fn then(mut self, action: EventAction) -> Self {
        self.events.push(StageEvent {
            at: EventTime::End,
            action,
        });
        self
    }

    /// This stage marked as a reserve.
    #[must_use]
    pub fn as_reserve(mut self) -> Self {
        self.reserve = true;
        self
    }

    /// This stage with `area_m2` of added parasite drag while it is flown, and
    /// the `basis` of that figure.
    #[must_use]
    pub fn with_drag_area(mut self, area_m2: f64, basis: &str) -> Self {
        self.extra_drag_area_m2 = area_m2;
        self.extra_drag_basis = basis.to_owned();
        self
    }
}
