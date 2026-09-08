// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The tank, layout and error types [`super::resolve`] and [`super::distribute`]
//! build and consume.
//!
//! Keeping the data types apart from the two modules that populate and
//! traverse them means a reviewer can read the shape of a resolved layout
//! without also reading the spar-box integration or the fill order.

use std::fmt;

/// Which family of tank a [`FuelTank`] belongs to.
///
/// Every variant corresponds to one nested group of
/// [`alas_config::FuelTankLayoutConfig`], so a caller matching on this can
/// find the configuration that produced the tank.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TankKind {
    /// The innermost integral wing cell, present on every transport.
    WingInner,
    /// A mid-span cell on wings large enough to divide the span three ways.
    WingMid,
    /// The outboard cell burned last to relieve root bending.
    WingOuter,
    /// The wing carry-through box under the cabin floor.
    Center,
    /// The horizontal-stabiliser cruise-balance tank.
    Trim,
    /// A declared-volume fuselage tank.
    Auxiliary,
}

impl TankKind {
    /// Stable report label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::WingInner => "inner wing tank",
            Self::WingMid => "mid wing tank",
            Self::WingOuter => "outer wing tank",
            Self::Center => "center tank",
            Self::Trim => "trim tank",
            Self::Auxiliary => "auxiliary tank",
        }
    }

    /// The identifier stem a [`FuelTank::id`] is built from.
    ///
    /// Kept apart from [`Self::label`] because the id is machine-read (a
    /// stable snake-case token another module matches on) while the label is
    /// prose for a report.
    pub(super) const fn id_prefix(self) -> &'static str {
        match self {
            Self::WingInner => "wing_inner",
            Self::WingMid => "wing_mid",
            Self::WingOuter => "wing_outer",
            Self::Center => "center",
            Self::Trim => "trim",
            Self::Auxiliary => "auxiliary",
        }
    }
}

/// Which half of the aircraft a [`FuelTank`] sits in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TankSide {
    /// Port side, negative y.
    Left,
    /// Starboard side, positive y.
    Right,
    /// On the aircraft centreline: the centre, trim and auxiliary tanks.
    Centerline,
}

impl TankSide {
    /// The identifier suffix a wing tank's two sides are distinguished by.
    pub(super) const fn suffix(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Centerline => "centerline",
        }
    }
}

/// Where a [`FuelTank`]'s usable capacity came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapacitySource {
    /// A manufacturer-published usable volume, taken as given.
    Published,
    /// The geometric estimate, scaled by the layout's calibration factor.
    GeometricCalibrated,
    /// The geometric estimate, uncalibrated.
    Geometric,
    /// A declared volume with no geometry to size it.
    Declared,
}

/// One resolved fuel tank: its capacity, centroid, extent and burn order.
#[derive(Debug, Clone, PartialEq)]
pub struct FuelTank {
    /// Stable identifier, unique within a [`FuelTankLayout`].
    pub id: String,
    /// Which family of tank this is.
    pub kind: TankKind,
    /// Which half of the aircraft it sits in.
    pub side: TankSide,
    /// Spar-box (or declared) volume of this tank alone, m^3.
    pub geometric_volume_m3: f64,
    /// Volume actually available to fuel, after the usable fraction,
    /// expansion space, a published override or calibration, m^3.
    pub usable_volume_m3: f64,
    /// [`Self::usable_volume_m3`] times the layout's fuel density, kg.
    pub usable_capacity_kg: f64,
    /// Fuel that cannot be delivered to the engines from this tank, kg.
    pub unusable_kg: f64,
    /// Volume centroid, aircraft geometry axes (x aft, y starboard, z up), m.
    pub centroid_m: [f64; 3],
    /// Extent of the equivalent rectangular prism: length (x), width (y),
    /// height (z), m.
    pub extent_m: [f64; 3],
    /// Order this tank is burned in normal operation; lower burns first.
    pub burn_priority: i64,
    /// Where [`Self::usable_volume_m3`] came from.
    pub capacity_source: CapacitySource,
}

/// One point of a fuel-loading centre-of-gravity curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FuelCgPoint {
    /// Fuel on board at this point, kg.
    pub fuel_kg: f64,
    /// Centre of gravity of that fuel alone, aircraft geometry axes, m.
    pub cg_m: [f64; 3],
}

/// The resolved fuel-tank arrangement of a built aircraft.
#[derive(Debug, Clone, PartialEq)]
pub struct FuelTankLayout {
    /// Every tank, in the order [`super::resolve`] built them.
    ///
    /// `pub(super)` rather than private: [`super::resolve`] constructs this
    /// struct directly and [`super::distribute`] walks it by burn order, and
    /// both live under this module. A caller outside it reads tanks only
    /// through [`Self::tanks`], which is what keeps burn order and capacity
    /// bookkeeping from being mutated out from under a resolved layout.
    pub(super) tanks: Vec<FuelTank>,
    /// Factor the geometric tanks were scaled by to reproduce a published
    /// total, or `1.0` when calibration is not enabled or no published total
    /// is available.
    pub geometric_calibration_factor: f64,
    /// Fuel density every tank's capacity was computed with, kg/m^3.
    pub density_kg_m3: f64,
}

impl FuelTankLayout {
    /// Every resolved tank, in burn-priority-independent build order.
    pub fn tanks(&self) -> &[FuelTank] {
        &self.tanks
    }

    /// Total usable capacity across every tank, kg.
    pub fn usable_capacity_kg(&self) -> f64 {
        self.tanks.iter().map(|tank| tank.usable_capacity_kg).sum()
    }

    /// Total unusable fuel across every tank, kg.
    pub fn unusable_fuel_kg(&self) -> f64 {
        self.tanks.iter().map(|tank| tank.unusable_kg).sum()
    }
}

/// Why a fuel-tank layout could not be resolved, or a fuel load applied to it.
#[derive(Debug, Clone, PartialEq)]
pub enum TankLayoutError {
    /// [`alas_config::FuelTankLayoutConfig::validate`] rejected the layout.
    InvalidConfig(String),
    /// [`alas_config::FuelPolicyConfig::validate`] rejected the policy.
    InvalidPolicy(String),
    /// The fuel density is not finite and positive.
    InvalidDensity(f64),
    /// Fewer than two full-span spars bound the wingbox.
    InvalidSparLayout,
    /// A tank's configured span interval has no overlap with a finite,
    /// positive built wing volume.
    DegenerateWingSpan,
    /// The trim tank's host stabiliser is missing from the built aircraft.
    MissingHorizontalStabilizer,
    /// The auxiliary tank's host fuselage is missing from the built aircraft.
    MissingFuselage,
    /// No geometric tank remained to absorb a published-capacity calibration.
    NothingToCalibrate,
    /// Calibrating to a published total produced a non-finite or
    /// non-positive factor.
    InvalidCalibrationFactor {
        /// The factor that was computed.
        factor: f64,
    },
    /// A requested fuel mass is negative or non-finite.
    InvalidFuelMass {
        /// The offending mass, kg.
        fuel_kg: f64,
    },
    /// A requested fuel load exceeds the layout's usable capacity.
    Overflow {
        /// How far over capacity the request was, kg.
        excess_kg: f64,
    },
    /// A burn removed more fuel than the state held.
    InsufficientFuel {
        /// How far short of the request the state's fuel fell, kg.
        shortfall_kg: f64,
    },
}

impl fmt::Display for TankLayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(reason) => {
                write!(formatter, "fuel tank layout is invalid: {reason}")
            }
            Self::InvalidPolicy(reason) => {
                write!(formatter, "fuel policy is invalid: {reason}")
            }
            Self::InvalidDensity(density) => {
                write!(formatter, "fuel density {density} kg/m^3 is not positive")
            }
            Self::InvalidSparLayout => {
                write!(formatter, "wingbox needs at least two full-span spars")
            }
            Self::DegenerateWingSpan => {
                write!(
                    formatter,
                    "a tank's span interval has no finite, positive volume on the built wing"
                )
            }
            Self::MissingHorizontalStabilizer => {
                write!(
                    formatter,
                    "the trim tank's horizontal stabiliser is missing from the built aircraft"
                )
            }
            Self::MissingFuselage => {
                write!(
                    formatter,
                    "the auxiliary tank's fuselage is missing from the built aircraft"
                )
            }
            Self::NothingToCalibrate => {
                write!(
                    formatter,
                    "no geometric tank is available to absorb the published-capacity calibration"
                )
            }
            Self::InvalidCalibrationFactor { factor } => {
                write!(
                    formatter,
                    "published-capacity calibration factor {factor} is not finite and positive"
                )
            }
            Self::InvalidFuelMass { fuel_kg } => {
                write!(
                    formatter,
                    "fuel mass {fuel_kg} kg is negative or not finite"
                )
            }
            Self::Overflow { excess_kg } => {
                write!(
                    formatter,
                    "fuel load exceeds usable capacity by {excess_kg} kg"
                )
            }
            Self::InsufficientFuel { shortfall_kg } => {
                write!(
                    formatter,
                    "burn exceeds the fuel state by {shortfall_kg} kg"
                )
            }
        }
    }
}

impl std::error::Error for TankLayoutError {}
