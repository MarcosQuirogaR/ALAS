// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Whether a run on a registered preset flies that preset's own mission.
//!
//! A design for a registered preset is held to the registered aircraft where
//! the model says the certified aircraft itself misses a published gate: "no
//! worse than the registered aircraft's model value". On a new mission (another range, payload, cruise Mach or
//! altitude, route or field length) every row the search can move takes the
//! full gate, and the rows it cannot (tail scrape and usable CG range, set by
//! the unsized gear and upsweep) keep that registered reference, so a new
//! mission is held exactly as the own mission is on them. The mission
//! compared is the one the preset loads with; the takeoff mass is left out
//! because the MTOW modes rewrite it during a run.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, PoisonError};

use crate::{AlasConfig, DesignMode};

/// The mission inputs a new brief changes, in the configuration's own units:
/// cruise Mach, cruise altitude (m), design range (nmi), passengers, freight
/// payload (kg), route and runway override (m).
#[derive(Debug, Clone, PartialEq)]
struct Mission {
    cruise_mach: f64,
    cruise_altitude_m: f64,
    design_range_nmi: f64,
    num_passengers: i64,
    cargo_payload_kg: f64,
    departure_airport: String,
    arrival_airport: String,
    toda_override_m: Option<f64>,
}

impl Mission {
    fn of(config: &AlasConfig) -> Self {
        let r = &config.requirements;
        Self {
            cruise_mach: r.cruise_mach,
            cruise_altitude_m: r.cruise_altitude_m,
            design_range_nmi: config.optimizer.objective.design_range_nmi,
            num_passengers: r.num_passengers,
            cargo_payload_kg: r.cargo_payload_kg,
            departure_airport: config.departure_airport.clone(),
            arrival_airport: config.arrival_airport.clone(),
            toda_override_m: config.toda_override_m,
        }
    }
}

/// The mission `preset` loads with, once per preset; `None` when it does not
/// load.
fn registered(preset: &str) -> Option<Mission> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<Mission>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(mission) = cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(preset)
    {
        return mission.clone();
    }
    let mission = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
        .ok()
        .map(|config| Mission::of(&config));
    cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(preset.to_owned(), mission.clone());
    mission
}

impl AlasConfig {
    /// Whether this configuration names a registered preset and flies the
    /// mission that preset loads with (see the module documentation).
    pub fn flies_registered_mission(&self) -> bool {
        !self.preset.is_empty()
            && registered(&self.preset).is_some_and(|mission| mission == Mission::of(self))
    }

    /// Whether a design of this run is held to the registered aircraft's
    /// modelled tail scrape and usable CG range (the rows the search cannot
    /// move): a reference adaptation, and a clean sheet that names a preset,
    /// on its own mission or a new one. A clean sheet with no preset has no
    /// registered aircraft and keeps those rows diagnostic.
    pub fn holds_to_registered_aircraft(&self) -> bool {
        match self.optimizer.design_space.mode {
            DesignMode::ReferenceAdaptation => true,
            DesignMode::CleanSheet => !self.preset.is_empty(),
            DesignMode::BaselineSandbox => false,
        }
    }
}

// A failed preset load is the assertion failing.
#[allow(clippy::unwrap_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn clean_sheet(preset: &str) -> AlasConfig {
        let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": preset })).unwrap();
        config.optimizer.design_space.mode = DesignMode::CleanSheet;
        config
    }

    /// The preset's own mission is recognised and a changed range, cruise
    /// Mach or payload is a new mission; both hold to the registered aircraft
    /// on the rows the search cannot move. No preset never holds.
    #[test]
    fn a_preset_clean_sheet_holds_to_the_registered_aircraft_on_any_mission() {
        let own = clean_sheet("A320-200");
        assert!(own.flies_registered_mission());
        assert!(own.holds_to_registered_aircraft());
        let mut range = own.clone();
        // The preset flies its route (design range 0); a stated range is new.
        range.optimizer.objective.design_range_nmi = 3000.0;
        let mut mach = own.clone();
        mach.requirements.cruise_mach += 0.02;
        let mut payload = own.clone();
        payload.requirements.cargo_payload_kg += 1000.0;
        for (label, new) in [("range", range), ("mach", mach), ("payload", payload)] {
            assert!(
                !new.flies_registered_mission(),
                "{label}: {:?} against {:?}",
                Mission::of(&new),
                Mission::of(&own)
            );
            assert!(new.holds_to_registered_aircraft(), "{label}");
        }
        let mut adaptation = own.clone();
        adaptation.optimizer.design_space.mode = DesignMode::ReferenceAdaptation;
        adaptation.optimizer.objective.design_range_nmi = 3000.0;
        assert!(adaptation.holds_to_registered_aircraft());
        let mut no_preset = AlasConfig::default();
        no_preset.optimizer.design_space.mode = DesignMode::CleanSheet;
        assert!(!no_preset.holds_to_registered_aircraft());
    }
}
