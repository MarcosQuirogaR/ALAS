// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Explicitly volatile procurement estimates for selected UAV components.
//!
//! Price is useful for a design trade, but it is not a physical rating and
//! must never enter [`crate::catalog::ComponentRecord`].  This module keeps a
//! small, dated set of manually reviewed quotes separate from the catalogue.
//! It intentionally does not convert currencies: a hidden exchange rate would
//! make a precise-looking total less traceable than the source prices.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

const REVIEWED_QUOTES_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/data/procurement_quotes.json"
));

/// ISO-style currency used by a reviewed price quote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Currency {
    /// Euro.
    Eur,
    /// Pound sterling.
    Gbp,
    /// United States dollar.
    Usd,
}

impl Currency {
    /// Three-letter display code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Eur => "EUR",
            Self::Gbp => "GBP",
            Self::Usd => "USD",
        }
    }
}

/// A component and installed quantity chosen for a procurement estimate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcurementSelection {
    /// Stable component catalogue identifier.
    pub component_id: String,
    /// Number of identical units required.
    pub quantity: u16,
}

/// One manually reviewed price, stored in minor currency units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcurementQuote {
    /// Stable component catalogue identifier.
    pub component_id: String,
    /// Currency in which the visible retailer or manufacturer price appeared.
    pub currency: Currency,
    /// Visible unit price in minor units, such as pence or cents.
    pub unit_price_minor: u64,
    /// Product page that exposed the price.
    pub source_url: String,
    /// Product-page title retained for audit.
    pub source_title: String,
    /// UTC calendar date on which the visible price was reviewed.
    pub observed_on_utc: String,
}

#[derive(Debug, Deserialize)]
struct StoredQuote {
    component_id: String,
    currency: String,
    unit_price_minor: u64,
    source_url: String,
    source_title: String,
    observed_on_utc: String,
}

/// One known or missing item in a procurement estimate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcurementLine {
    /// Requested component identifier.
    pub component_id: String,
    /// Requested quantity.
    pub quantity: u16,
    /// Dated source quote when one is available.
    pub quote: Option<ProcurementQuote>,
}

/// Currency subtotal retained without an invented exchange-rate conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrencySubtotal {
    /// Currency of the subtotal.
    pub currency: Currency,
    /// Amount in minor units.
    pub amount_minor: u64,
}

/// Transparent cost estimate for a selected set of components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcurementEstimate {
    /// Per-component lines, preserving the review source for every known price.
    pub lines: Vec<ProcurementLine>,
    /// Currency-separated known-price subtotals.
    pub subtotals: Vec<CurrencySubtotal>,
    /// Whether every requested item had a dated source quote.
    pub complete: bool,
}

/// Functional group for a line in an aircraft bill of materials.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BomCategory {
    /// Shared traction battery.
    Battery,
    /// One installed propulsion motor.
    Motor,
    /// One installed motor speed controller.
    Esc,
    /// One installed propeller.
    Propeller,
    /// Control-surface servo.
    Servo,
    /// Structural stock used by the generated airframe model.
    StructuralMaterial,
    /// Radio receiver.
    Receiver,
    /// Mission-specific sensor or avionics record.
    MissionElectronics,
    /// Landing-gear assembly.
    LandingGear,
}

impl BomCategory {
    /// Stable user-facing English label retained outside the translation seam.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Battery => "Battery",
            Self::Motor => "Motor",
            Self::Esc => "Speed controller",
            Self::Propeller => "Propeller",
            Self::Servo => "Servo",
            Self::StructuralMaterial => "Structural material",
            Self::Receiver => "Receiver",
            Self::MissionElectronics => "Mission electronics",
            Self::LandingGear => "Landing gear",
        }
    }
}

/// One categorized component quantity required by an optimized aircraft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AircraftBomLine {
    /// Functional system containing this item.
    pub category: BomCategory,
    /// Catalogue component and installed quantity.
    pub selection: ProcurementSelection,
}

/// Complete selected-hardware BOM before a volatile price lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AircraftBillOfMaterials {
    /// Every selected physical component family used by the optimizer.
    pub lines: Vec<AircraftBomLine>,
}

impl AircraftBillOfMaterials {
    /// Convert BOM quantities into a dated, currency-separated cost estimate.
    pub fn procurement_estimate(&self) -> ProcurementEstimate {
        let selections: Vec<ProcurementSelection> = self
            .lines
            .iter()
            .map(|line| line.selection.clone())
            .collect();
        estimate_procurement_cost(&selections)
    }
}

/// Look up a dated quote without treating it as a current price or stock claim.
pub fn reviewed_quote(component_id: &str) -> Option<ProcurementQuote> {
    let quote = stored_quotes()
        .iter()
        .find(|quote| quote.component_id == component_id)?;
    let currency = match quote.currency.as_str() {
        "EUR" => Currency::Eur,
        "GBP" => Currency::Gbp,
        "USD" => Currency::Usd,
        _ => return None,
    };
    Some(ProcurementQuote {
        component_id: quote.component_id.clone(),
        currency,
        unit_price_minor: quote.unit_price_minor,
        source_url: quote.source_url.clone(),
        source_title: quote.source_title.clone(),
        observed_on_utc: quote.observed_on_utc.clone(),
    })
}

/// Whether a component has one dated, source-backed unit price.
///
/// This does not claim stock or a current checkout price.  It is the
/// procurement admission gate used by automatic catalogue selection.
pub fn has_reviewed_quote(component_id: &str) -> bool {
    reviewed_quote(component_id).is_some()
}

fn stored_quotes() -> &'static [StoredQuote] {
    static QUOTES: OnceLock<Result<Vec<StoredQuote>, serde_json::Error>> = OnceLock::new();
    match QUOTES.get_or_init(|| serde_json::from_str(REVIEWED_QUOTES_JSON)) {
        Ok(quotes) => quotes.as_slice(),
        Err(_) => &[],
    }
}

/// Sum dated quotes for the requested components while exposing missing prices.
pub fn estimate_procurement_cost(selection: &[ProcurementSelection]) -> ProcurementEstimate {
    let mut totals = BTreeMap::<Currency, u64>::new();
    let lines: Vec<ProcurementLine> = selection
        .iter()
        .map(|selection| {
            let quote = reviewed_quote(&selection.component_id);
            if selection.quantity > 0 {
                if let Some(quote) = &quote {
                    let line_total = quote
                        .unit_price_minor
                        .saturating_mul(u64::from(selection.quantity));
                    let subtotal = totals.entry(quote.currency).or_insert(0);
                    *subtotal = subtotal.saturating_add(line_total);
                }
            }
            ProcurementLine {
                component_id: selection.component_id.clone(),
                quantity: selection.quantity,
                quote,
            }
        })
        .collect();
    let complete = lines
        .iter()
        .all(|line| line.quantity > 0 && line.quote.is_some());
    ProcurementEstimate {
        lines,
        subtotals: totals
            .into_iter()
            .map(|(currency, amount_minor)| CurrencySubtotal {
                currency,
                amount_minor,
            })
            .collect(),
        complete,
    }
}

/// Estimate the selected battery and all identical propulsors without re-entry.
///
/// A battery is one shared pack; each selected motor requires one ESC and one
/// propeller.  The returned lines preserve unavailable quotes, so the caller
/// cannot mistake a partial supplier-price view for a final aircraft cost.
pub fn estimate_powertrain_procurement_cost(
    selection: &crate::propulsion_electric::CataloguePowertrainSelection,
) -> ProcurementEstimate {
    estimate_procurement_cost(&[
        ProcurementSelection {
            component_id: selection.battery_id.clone(),
            quantity: 1,
        },
        ProcurementSelection {
            component_id: selection.motor_id.clone(),
            quantity: selection.motor_count,
        },
        ProcurementSelection {
            component_id: selection.esc_id.clone(),
            quantity: selection.motor_count,
        },
        ProcurementSelection {
            component_id: selection.propeller_id.clone(),
            quantity: selection.motor_count,
        },
    ])
}

/// Build the exact selected-hardware BOM for one optimized UAV.
///
/// The optimizer's current conventional-tail generator installs three primary
/// servos. The stored `motor_count` and `servo_count` are carried through as
/// quantities, making a distributed-electric price estimate use the same
/// physical count as its mass and current checks. Consumables, shipping,
/// taxes, machining, and a user-supplied payload remain deliberately absent:
/// no source-backed price or quantity exists for them.
pub fn optimized_aircraft_bom(
    components: &crate::optimizer::SelectedComponents,
) -> AircraftBillOfMaterials {
    let lines = [
        (BomCategory::Battery, &components.battery_id, 1),
        (
            BomCategory::Motor,
            &components.motor_id,
            components.motor_count,
        ),
        (BomCategory::Esc, &components.esc_id, components.motor_count),
        (
            BomCategory::Propeller,
            &components.propeller_id,
            components.motor_count,
        ),
        (
            BomCategory::Servo,
            &components.servo_id,
            components.servo_count,
        ),
        (BomCategory::StructuralMaterial, &components.material_id, 1),
        (BomCategory::Receiver, &components.receiver_id, 1),
        (
            BomCategory::MissionElectronics,
            &components.electronics_id,
            1,
        ),
        (BomCategory::LandingGear, &components.landing_gear_id, 1),
    ]
    .into_iter()
    .map(|(category, component_id, quantity)| AircraftBomLine {
        category,
        selection: ProcurementSelection {
            component_id: component_id.clone(),
            quantity,
        },
    })
    .collect();
    AircraftBillOfMaterials { lines }
}

/// Estimate the procurement cost of every component in one optimizer selection.
///
/// This preserves every missing price as a visible line, so a GUI can show a
/// partial drone estimate without asking the designer to re-enter the selected
/// hardware or implying that an unquoted item is free.
pub fn estimate_optimized_aircraft_cost(
    components: &crate::optimizer::SelectedComponents,
) -> ProcurementEstimate {
    optimized_aircraft_bom(components).procurement_estimate()
}
