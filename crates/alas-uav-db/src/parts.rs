// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Typed component bodies, one struct per category.

mod electrical;
mod materials;
mod mechanical;

pub use electrical::{BatterySpec, EscSpec, MotorSpec};
pub use materials::{KitSpec, LaminateSpec, MaterialSpec};
pub use mechanical::{
    AvionicsSpec, ConnectorSpec, GearSpec, PropGeometry, PropellerSpec, ServoSpec, WheelSpec,
};
