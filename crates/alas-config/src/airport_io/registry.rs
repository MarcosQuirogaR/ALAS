// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Runtime registry and static lookup adapters for custom airports.

use std::sync::{Mutex, OnceLock, RwLock};

use crate::airports::{self, Airport};

use super::{AirportIoError, CustomAirport};

/// Register one custom airport for route and settings lookup.
pub fn register_custom_airport(mut airport: CustomAirport) -> Result<(), AirportIoError> {
    airport.validate_and_normalize()?;
    let mut registry = custom_registry()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    ensure_no_collision(&airport, &registry)?;
    let static_airport = intern_airport(to_airport(&airport));
    registry.push(RegisteredAirport {
        airport,
        static_airport,
    });
    Ok(())
}

/// Replace all custom airports atomically after validating every record.
pub fn replace_custom_airports(airports: Vec<CustomAirport>) -> Result<(), AirportIoError> {
    let mut replacement = Vec::with_capacity(airports.len());
    for mut airport in airports {
        airport.validate_and_normalize()?;
        ensure_no_collision(&airport, &replacement)?;
        replacement.push(RegisteredAirport {
            static_airport: intern_airport(to_airport(&airport)),
            airport,
        });
    }
    *custom_registry()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = replacement;
    Ok(())
}

/// Return all custom airport records in registration order.
pub fn registered_custom_airports() -> Vec<CustomAirport> {
    custom_registry()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .map(|registered| registered.airport.clone())
        .collect()
}

/// Resolve one custom airport by case-insensitive ICAO or display name.
pub fn find_custom(name_or_icao: &str) -> Option<CustomAirport> {
    custom_registry()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .find(|registered| same_key(&registered.airport, name_or_icao))
        .map(|registered| registered.airport.clone())
}

/// Resolve a custom airport as a static [`Airport`] without borrowing the
/// registry lock.
pub(crate) fn airport_by_name_or_icao(name_or_icao: &str) -> Option<&'static Airport> {
    custom_registry()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .find(|registered| same_key(&registered.airport, name_or_icao))
        .map(|registered| registered.static_airport)
}

/// Return a `&'static` record for `airport`, reusing the earlier leak when an
/// identical record was interned before, so repeated edits, rollbacks and file
/// loads leak at most one record per distinct airport.
fn intern_airport(airport: Airport) -> &'static Airport {
    static INTERNED: OnceLock<Mutex<Vec<&'static Airport>>> = OnceLock::new();
    let mut interned = INTERNED
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(existing) = interned.iter().find(|known| ***known == airport) {
        return existing;
    }
    let leaked: &'static Airport = Box::leak(Box::new(airport));
    interned.push(leaked);
    leaked
}

struct RegisteredAirport {
    airport: CustomAirport,
    static_airport: &'static Airport,
}

fn custom_registry() -> &'static RwLock<Vec<RegisteredAirport>> {
    static REGISTRY: OnceLock<RwLock<Vec<RegisteredAirport>>> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(Vec::new()))
}

fn ensure_no_collision<T>(airport: &CustomAirport, existing: &[T]) -> Result<(), AirportIoError>
where
    T: AirportRecordView,
{
    if airports::database().iter().any(|curated| {
        curated.icao.eq_ignore_ascii_case(&airport.icao)
            || curated.name.eq_ignore_ascii_case(&airport.name)
    }) || existing.iter().any(|registered| {
        let other = registered.record();
        other.icao.eq_ignore_ascii_case(&airport.icao)
            || other.name.eq_ignore_ascii_case(&airport.name)
    }) {
        return Err(AirportIoError::Collision(airport.icao.clone()));
    }
    Ok(())
}

trait AirportRecordView {
    fn record(&self) -> &CustomAirport;
}

impl AirportRecordView for RegisteredAirport {
    fn record(&self) -> &CustomAirport {
        &self.airport
    }
}

impl AirportRecordView for CustomAirport {
    fn record(&self) -> &CustomAirport {
        self
    }
}

fn same_key(airport: &CustomAirport, key: &str) -> bool {
    airport.icao.eq_ignore_ascii_case(key.trim()) || airport.name.eq_ignore_ascii_case(key.trim())
}

fn to_airport(airport: &CustomAirport) -> Airport {
    let physical_length = airport
        .runway_lengths_m
        .iter()
        .copied()
        .filter(|length| length.is_finite() && *length > 0.0)
        .fold(0.0, f64::max);
    let (toda, lda, runway_kind) = airport.declared_toda_m.zip(airport.declared_lda_m).map_or(
        (
            physical_length,
            physical_length,
            "longest physical runway used conservatively",
        ),
        |(toda, lda)| (toda, lda, "declared distances supplied"),
    );
    let provenance = airport
        .provenance
        .source
        .as_deref()
        .map_or_else(|| "user entry".to_owned(), ToOwned::to_owned);
    Airport {
        name: airport.name.clone(),
        icao: airport.icao.clone(),
        elevation_m: airport.altitude_m,
        toda_m: toda,
        lda_m: lda,
        isa_deviation_c: airport.isa_delta_c,
        notes: format!("Custom airport; {runway_kind}; provenance: {provenance}"),
        latitude_deg: airport.latitude_deg,
        longitude_deg: airport.longitude_deg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_records_share_one_leaked_airport() {
        let record = Airport {
            name: "Interning Test Field".to_owned(),
            icao: "ZZIN".to_owned(),
            elevation_m: 10.0,
            toda_m: 2000.0,
            lda_m: 1900.0,
            isa_deviation_c: 0.0,
            notes: "test".to_owned(),
            latitude_deg: 1.0,
            longitude_deg: 2.0,
        };
        let first = intern_airport(record.clone());
        let second = intern_airport(record.clone());
        assert!(std::ptr::eq(first, second));
        let mut other = record;
        other.toda_m = 2100.0;
        assert!(!std::ptr::eq(first, intern_airport(other)));
    }
}
