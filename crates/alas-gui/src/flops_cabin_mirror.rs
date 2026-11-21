// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The FLOPS transport inputs that follow the cabin: the class counts and the
//! cabin crew.
//!
//! The analysis resolves the FLOPS class counts from the cabin layout and
//! raises the flight-attendant count to the regulatory minimum for the seated
//! cabin (one cabin crew member per 50 passenger seats or fraction, EASA
//! ORO.CC.100 and 14 CFR 121.391(a)). The form shows the same numbers, so a
//! two-class cabin never reads as all tourist and the crew never reads below
//! the rule. A larger attendant count entered by the user is kept, as in the
//! analysis; the class counts are always the cabin's.

use alas_config::cabin::PassengerCabinConfig;
use serde_json::{json, Value};

/// Write the cabin-derived FLOPS class counts and the minimum cabin crew into
/// `values`; `true` when anything changed. Cargo aircraft and an unreadable
/// cabin are left alone.
pub(crate) fn sync(values: &mut Value) -> bool {
    if values["mass_model"]["flops_transport"]["manual_cabin_inputs"].as_bool() == Some(true) {
        return false;
    }
    if values["requirements"]["aircraft_type"].as_str() != Some("passenger") {
        return false;
    }
    let Ok(cabin) =
        serde_json::from_value::<PassengerCabinConfig>(values["cabin"]["passenger"].clone())
    else {
        return false;
    };
    let requested = values["requirements"]["num_passengers"]
        .as_i64()
        .unwrap_or(0);
    let counts = cabin
        .canonicalized_for_product()
        .resolved_flops_counts(requested);
    if !counts.is_nonempty() {
        return false;
    }
    let Some(transport) = values["mass_model"]["flops_transport"].as_object_mut() else {
        return false;
    };
    let minimum_crew = i64::try_from(alas_config::minimum_cabin_crew(
        usize::try_from(counts.total().max(0)).unwrap_or(0),
    ))
    .unwrap_or(0);
    let crew = transport
        .get("flight_attendant_count")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(minimum_crew);
    let mut changed = false;
    for (key, value) in [
        ("first_class_passenger_count", counts.first),
        ("business_class_passenger_count", counts.business),
        ("tourist_class_passenger_count", counts.tourist),
        ("flight_attendant_count", crew),
    ] {
        let wanted = json!(value.max(0));
        if transport.get(key) != Some(&wanted) {
            transport.insert(key.to_owned(), wanted);
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values_for(preset: &str) -> Value {
        let config = alas_config::AlasConfig::from_value(&json!({ "preset": preset }))
            .expect("preset loads");
        serde_json::to_value(config).expect("config serializes")
    }

    #[test]
    fn the_form_shows_the_cabin_classes_and_at_least_one_attendant_per_50_seats() {
        let mut values = values_for("AVE");
        let transport = &mut values["mass_model"]["flops_transport"];
        transport["first_class_passenger_count"] = json!(0);
        transport["business_class_passenger_count"] = json!(0);
        transport["tourist_class_passenger_count"] = json!(350);
        transport["flight_attendant_count"] = json!(1);
        sync(&mut values);
        let transport = &values["mass_model"]["flops_transport"];
        let seats: i64 = ["first", "business", "tourist"]
            .iter()
            .map(|class| {
                transport[format!("{class}_class_passenger_count")]
                    .as_i64()
                    .unwrap_or(0)
            })
            .sum();
        assert!(seats > 0);
        let crew = transport["flight_attendant_count"].as_i64().unwrap_or(0);
        assert!(
            crew >= (seats + 49) / 50,
            "{crew} attendants for {seats} seats"
        );
        assert!(!sync(&mut values), "a second pass changes nothing");
    }

    #[test]
    fn a_larger_declared_crew_is_kept() {
        let mut values = values_for("AVE");
        values["mass_model"]["flops_transport"]["flight_attendant_count"] = json!(40);
        sync(&mut values);
        assert_eq!(
            values["mass_model"]["flops_transport"]["flight_attendant_count"],
            json!(40)
        );
    }

    #[test]
    fn manual_mode_leaves_the_entered_values_alone_and_auto_restores_the_cabin() {
        let mut values = values_for("AVE");
        let transport = &mut values["mass_model"]["flops_transport"];
        transport["manual_cabin_inputs"] = json!(true);
        transport["tourist_class_passenger_count"] = json!(12);
        transport["flight_attendant_count"] = json!(1);
        assert!(!sync(&mut values), "manual inputs are not touched");
        assert_eq!(
            values["mass_model"]["flops_transport"]["tourist_class_passenger_count"],
            json!(12)
        );
        values["mass_model"]["flops_transport"]["manual_cabin_inputs"] = json!(false);
        assert!(sync(&mut values), "Auto derives the cabin again");
        assert_ne!(
            values["mass_model"]["flops_transport"]["tourist_class_passenger_count"],
            json!(12)
        );
    }
}
