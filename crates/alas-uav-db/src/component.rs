// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The component row: common header plus one typed body.
//!
//! A component id names a physical product. Spec corrections are new rows with
//! the same id and a later `valid_from`; the previous row gets a `valid_to`
//! (append-only revisions). Compatibility between components is computed by
//! the optimiser and never stored.

use serde::{Deserialize, Serialize};

use crate::date::IsoDate;
use crate::ids::ComponentId;
use crate::parts::{
    AvionicsSpec, BatterySpec, ConnectorSpec, EscSpec, GearSpec, KitSpec, LaminateSpec,
    MaterialSpec, MotorSpec, PropellerSpec, ServoSpec, WheelSpec,
};

/// Current schema version written into every row.
pub const SCHEMA_VERSION: u32 = 2;

/// Product category; also the inventory key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Brushless motor.
    Motor,
    /// Propeller.
    Propeller,
    /// Speed controller.
    Esc,
    /// Battery pack.
    Battery,
    /// Servo.
    Servo,
    /// Radio receiver.
    Receiver,
    /// Flight controller.
    FlightController,
    /// Other avionics (GPS, telemetry, BEC, kill switch, ...).
    Electronics,
    /// Wheel.
    Wheel,
    /// Landing-gear assembly.
    LandingGear,
    /// Structural material.
    Material,
    /// Composite laminate stock.
    Laminate,
    /// Airframe kit.
    Kit,
    /// Connector family.
    Connector,
}

impl Category {
    /// Every category, in documentation order.
    pub const ALL: [Self; 14] = [
        Self::Motor,
        Self::Propeller,
        Self::Esc,
        Self::Battery,
        Self::Servo,
        Self::Receiver,
        Self::FlightController,
        Self::Electronics,
        Self::Wheel,
        Self::LandingGear,
        Self::Material,
        Self::Laminate,
        Self::Kit,
        Self::Connector,
    ];

    /// The snake_case key used in JSON and the inventory.
    pub fn key(self) -> &'static str {
        match self {
            Self::Motor => "motor",
            Self::Propeller => "propeller",
            Self::Esc => "esc",
            Self::Battery => "battery",
            Self::Servo => "servo",
            Self::Receiver => "receiver",
            Self::FlightController => "flight_controller",
            Self::Electronics => "electronics",
            Self::Wheel => "wheel",
            Self::LandingGear => "landing_gear",
            Self::Material => "material",
            Self::Laminate => "laminate",
            Self::Kit => "kit",
            Self::Connector => "connector",
        }
    }

    /// Parse a snake_case key.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.key() == key)
    }
}

/// Market status of the product.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Currently sold.
    Active,
    /// No longer sold; kept for reproducibility of older designs.
    Discontinued,
}

/// The typed body, tagged by category in the serialized form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
// Rows are parsed once and held in a Vec; boxing every body would add an
// allocation per row for no gain, and the size spread is between spec structs.
#[allow(clippy::large_enum_variant)]
#[serde(tag = "category", content = "spec", rename_all = "snake_case")]
pub enum Body {
    /// Motor.
    Motor(MotorSpec),
    /// Propeller.
    Propeller(PropellerSpec),
    /// ESC.
    Esc(EscSpec),
    /// Battery.
    Battery(BatterySpec),
    /// Servo.
    Servo(ServoSpec),
    /// Receiver.
    Receiver(AvionicsSpec),
    /// Flight controller.
    FlightController(AvionicsSpec),
    /// Other avionics.
    Electronics(AvionicsSpec),
    /// Wheel.
    Wheel(WheelSpec),
    /// Landing gear.
    LandingGear(GearSpec),
    /// Material.
    Material(MaterialSpec),
    /// Laminate stock.
    Laminate(LaminateSpec),
    /// Kit.
    Kit(KitSpec),
    /// Connector.
    Connector(ConnectorSpec),
}

impl Body {
    /// The category of this body.
    pub fn category(&self) -> Category {
        match self {
            Self::Motor(_) => Category::Motor,
            Self::Propeller(_) => Category::Propeller,
            Self::Esc(_) => Category::Esc,
            Self::Battery(_) => Category::Battery,
            Self::Servo(_) => Category::Servo,
            Self::Receiver(_) => Category::Receiver,
            Self::FlightController(_) => Category::FlightController,
            Self::Electronics(_) => Category::Electronics,
            Self::Wheel(_) => Category::Wheel,
            Self::LandingGear(_) => Category::LandingGear,
            Self::Material(_) => Category::Material,
            Self::Laminate(_) => Category::Laminate,
            Self::Kit(_) => Category::Kit,
            Self::Connector(_) => Category::Connector,
        }
    }
}

/// One revision of one product.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Component {
    /// Stable product id (shared by all revisions).
    pub id: ComponentId,
    /// Manufacturer part number or model designation.
    pub mpn: String,
    /// Manufacturer name as printed.
    pub manufacturer: String,
    /// Market status.
    pub status: Status,
    /// Schema version of this row ([`SCHEMA_VERSION`]).
    pub schema_version: u32,
    /// First day this revision applies (inclusive).
    pub valid_from: IsoDate,
    /// First day it no longer applies (exclusive); `None` means current.
    #[serde(default)]
    pub valid_to: Option<IsoDate>,
    /// Date the row was created.
    pub created_at: IsoDate,
    /// Date the row was last edited.
    pub updated_at: IsoDate,
    /// Typed body.
    #[serde(flatten)]
    pub body: Body,
}

impl Component {
    /// The category of the body.
    pub fn category(&self) -> Category {
        self.body.category()
    }

    /// Whether this revision applies on `date`.
    pub fn valid_on(&self, date: &IsoDate) -> bool {
        &self.valid_from <= date && self.valid_to.as_ref().is_none_or(|end| date < end)
    }
}

impl Component {
    /// Inventory members that physics model `model` requires and this row
    /// leaves unknown (empty means the row can feed the model).
    pub fn missing_for(&self, model: &str) -> Vec<&'static str> {
        let Ok(value) = serde_json::to_value(self) else {
            return Vec::new();
        };
        crate::inventory::fields(self.category())
            .iter()
            .filter(|info| info.required_for.contains(&model))
            .filter(|info| {
                value["spec"]
                    .get(info.name)
                    .is_none_or(serde_json::Value::is_null)
            })
            .map(|info| info.name)
            .collect()
    }
}
