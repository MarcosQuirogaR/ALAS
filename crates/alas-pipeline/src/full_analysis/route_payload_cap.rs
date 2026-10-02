// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The route payload a registered aircraft may carry: at most its published
//! maximum zero-fuel mass less the modeled operating empty mass.
//!
//! The payload layout bounds only the revenue freight by the structural
//! payload limit it is handed; the seated passengers and their checked bags
//! are placed whatever their mass. When a modeled operating empty mass is
//! heavier than the published one, seats and bags alone can exceed
//! `MZFW - OEW`, and the analysis would then fly a zero-fuel mass above the
//! certified maximum. A real operation offloads passengers with their bags in
//! that case. This module applies the same limit to a passenger layout: the
//! occupancy of every mass-bearing position is reduced by one common factor,
//! so the installed cabin (seat count, monuments, holds) and the payload
//! center of gravity are unchanged and only the carried mass falls to the
//! limit. The offloaded mass is returned so the report can state it.
//!
//! An explicitly requested load is never reduced, consistent with the layout
//! itself: revenue belly freight (`belly_cargo_kg > 0`) and a freighter
//! deck are deliberate load cases, and a zero-fuel mass they take above the
//! published maximum is reported by the feasibility check rather than
//! removed here.

use alas_payload::layout::{mass_properties, LayoutSummary, PayloadLayout};

/// The `AnalysisReport::geometry_summary` key that carries the offloaded
/// route payload, kg, when the limit was applied. Absent when it was not.
pub(crate) const ROUTE_PAYLOAD_OFFLOADED_KEY: &str = "route_payload_offloaded_kg";

/// Kilograms per tonne, for the layout summary's tonne-valued fields.
const KG_PER_TONNE: f64 = 1_000.0;

/// Reduce a passenger `layout` to at most `limit_kg` of payload.
///
/// Returns the layout and the offloaded payload, kg. The layout is returned
/// unchanged with zero offload when there is no finite positive limit, the
/// payload already respects it, revenue belly freight was requested
/// (`requested_belly_cargo_kg > 0`), or the layout is a freighter deck.
/// Otherwise every mass-bearing item and every mass field of the summary is
/// scaled by `limit_kg / total_mass`, which keeps the longitudinal and
/// lateral payload center of gravity exactly where the layout put it.
pub(super) fn cap_route_payload(
    mut layout: PayloadLayout,
    limit_kg: Option<f64>,
    requested_belly_cargo_kg: f64,
) -> (PayloadLayout, f64) {
    let Some(limit_kg) = limit_kg.filter(|limit| limit.is_finite() && *limit > 0.0) else {
        return (layout, 0.0);
    };
    let carried_kg = layout.total_mass;
    let LayoutSummary::Passenger(summary) = &mut layout.summary else {
        return (layout, 0.0);
    };
    if requested_belly_cargo_kg > 0.0 || !carried_kg.is_finite() || carried_kg <= limit_kg {
        return (layout, 0.0);
    }
    let factor = limit_kg / carried_kg;
    summary.seat_mass_t *= factor;
    summary.bag_mass_t *= factor;
    summary.belly_cargo_t *= factor;
    summary.hold_used_t *= factor;
    summary.overload_kg *= factor;
    for (_, compartment_kg) in &mut summary.hold_compartment_masses_kg {
        *compartment_kg *= factor;
    }
    for item in &mut layout.items {
        if item.mass > 0.0 {
            item.mass *= factor;
        }
    }
    let (total_mass, cg_x, cg_y) = mass_properties(&layout.items);
    layout.total_mass = total_mass;
    layout.cg_x = cg_x;
    layout.cg_y = cg_y;
    if let LayoutSummary::Passenger(summary) = &mut layout.summary {
        summary.payload_t = total_mass / KG_PER_TONNE;
    }
    (layout, carried_kg - total_mass)
}

// A test asserts on values it constructed, so a failed unwrap is the
// assertion failing.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::{presets, AlasConfig};
    use alas_geom::builder::AircraftBuilder;

    fn atr_layout() -> PayloadLayout {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": "ATR72-600" })).unwrap();
        let preset = presets::get("ATR72-600").unwrap();
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .unwrap();
        alas_payload::build::build_payload_layout(&plane, &config, 13_450.0, 11.8).unwrap()
    }

    #[test]
    fn a_payload_above_the_limit_is_offloaded_to_it_at_the_same_center_of_gravity() {
        let layout = atr_layout();
        let carried_kg = layout.total_mass;
        let limit_kg = carried_kg - 500.0;
        let (capped, offloaded_kg) = cap_route_payload(layout.clone(), Some(limit_kg), 0.0);
        assert!((capped.total_mass - limit_kg).abs() < 1e-6 * limit_kg);
        assert!((offloaded_kg - 500.0).abs() < 1e-6 * limit_kg);
        assert!((capped.cg_x - layout.cg_x).abs() < 1e-9);
        assert!((capped.cg_y - layout.cg_y).abs() < 1e-9);
        let (LayoutSummary::Passenger(before), LayoutSummary::Passenger(after)) =
            (&layout.summary, &capped.summary)
        else {
            panic!("the ATR layout is a passenger cabin");
        };
        assert_eq!(
            after.seated_pax, before.seated_pax,
            "the installed cabin stays"
        );
        assert!((after.payload_t * KG_PER_TONNE - limit_kg).abs() < 1e-6 * limit_kg);
    }

    #[test]
    fn a_payload_within_the_limit_without_one_or_requested_is_untouched() {
        let layout = atr_layout();
        let carried_kg = layout.total_mass;
        for limit in [
            None,
            Some(carried_kg),
            Some(carried_kg + 1.0),
            Some(f64::NAN),
        ] {
            let (capped, offloaded_kg) = cap_route_payload(layout.clone(), limit, 0.0);
            assert_eq!(offloaded_kg, 0.0);
            assert_eq!(capped, layout);
        }
        // Requested revenue freight is a deliberate load case: it is not
        // reduced, and the feasibility check reports any MZFW excess.
        let (capped, offloaded_kg) =
            cap_route_payload(layout.clone(), Some(carried_kg - 500.0), 1_000.0);
        assert_eq!(offloaded_kg, 0.0);
        assert_eq!(capped, layout);
    }
}
