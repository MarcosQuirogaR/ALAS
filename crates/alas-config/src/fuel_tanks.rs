// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Where the fuel is carried: the tank arrangement of a transport aircraft.
//!
//! A single wing-volume correlation cannot say whether a mission's fuel
//! fits, where its centre of gravity sits as it burns, or how much of it is
//! usable. Transport fuel systems are built from a small, recurring set of
//! tank families, and this group declares which of them an aircraft has and
//! where each one sits: integral wing tanks split into an inner and an outer
//! (and, on the largest wings, a mid) cell between the spars, a centre tank
//! in the wing carry-through box, a trim tank in the horizontal stabiliser,
//! and an auxiliary tank in the fuselage. Every wing tank is bounded by the
//! configured front and rear spars and by two semispan stations, so its
//! volume and centroid follow from the built wing rather than from a number
//! typed in.
//!
//! Published capacities are kept beside the geometric estimate, never in
//! place of it: a registered aircraft declares the usable volume of each
//! tank its manufacturer publishes, and the geometric volume then supplies
//! the centroid and a calibration factor that keeps a redesigned wing's
//! capacity consistent with the baseline.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// One integral tank between the spars, bounded by two semispan stations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(default, deny_unknown_fields)]
pub struct WingTankConfig {
    /// Whether the aircraft carries this tank.
    #[config(
        label = "Installed",
        help = "Whether this wing tank exists on the aircraft. A tank that is not installed contributes no capacity, no centroid and no unusable fuel."
    )]
    pub enabled: bool,

    /// Inboard boundary of the tank.
    #[config(
        label = "Span start",
        unit = "fraction of semi-span",
        help = "Inboard boundary of the tank as a fraction of the semi-span from the aircraft centreline. The innermost wing tank normally starts at the side of the body; an outer tank starts where the inner one ends."
    )]
    pub span_start_fraction: f64,

    /// Outboard boundary of the tank.
    #[config(
        label = "Span end",
        unit = "fraction of semi-span",
        help = "Outboard boundary of the tank as a fraction of the semi-span. Transport wing tanks end short of the tip, leaving the outer bays dry for the ailerons and the surge tank."
    )]
    pub span_end_fraction: f64,

    /// Share of the spar-box volume that holds fuel.
    #[config(
        label = "Usable volume fraction",
        help = "Share of the geometric spar-box volume between the two stations that is usable tank volume, after ribs, stringers, systems and the expansion space. Ninety to ninety-five percent is typical of an integral wing tank."
    )]
    pub usable_fraction: f64,

    /// Order in which the tank is emptied.
    #[config(
        label = "Burn priority",
        help = "Order in which the tank is consumed in normal operation: lower numbers are used first. Transports burn the centre tank first and keep the outer wing fuel longest, because fuel in the outer wing relieves the root bending moment."
    )]
    pub burn_priority: i64,

    /// Manufacturer-published usable volume, when the aircraft is registered.
    #[config(
        label = "Published usable volume",
        unit = "L",
        help = "Usable volume published by the manufacturer for this tank, both sides together. When set it is the authoritative capacity and the geometric estimate only supplies the centroid; leave unset for a notional design."
    )]
    pub published_usable_volume_l: Option<f64>,
}

impl Default for WingTankConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            span_start_fraction: 0.10,
            span_end_fraction: 0.65,
            usable_fraction: 0.92,
            burn_priority: 2,
            published_usable_volume_l: None,
        }
    }
}

/// The centre tank in the wing carry-through box under the cabin floor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(default, deny_unknown_fields)]
pub struct CenterTankConfig {
    /// Whether the aircraft carries a centre tank.
    #[config(
        label = "Installed",
        help = "Whether the wing carry-through box under the cabin floor is a fuel tank. Most transports have one; some short-range variants leave it dry."
    )]
    pub enabled: bool,

    /// Share of the carry-through box volume that holds fuel.
    #[config(
        label = "Usable volume fraction",
        help = "Share of the geometric carry-through spar-box volume between the two body sides that is usable tank volume. The box is crossed by the keel beam, gear support structure and systems, so this is lower than a wing tank's."
    )]
    pub usable_fraction: f64,

    /// Order in which the tank is emptied.
    #[config(
        label = "Burn priority",
        help = "Order in which the centre tank is consumed: lower numbers are used first. It is normally the first tank burned, because centre fuel neither relieves wing bending nor moves the centre of gravity much."
    )]
    pub burn_priority: i64,

    /// Manufacturer-published usable volume, when the aircraft is registered.
    #[config(
        label = "Published usable volume",
        unit = "L",
        help = "Usable volume published by the manufacturer for the centre tank. When set it is the authoritative capacity and the geometric estimate only supplies the centroid; leave unset for a notional design."
    )]
    pub published_usable_volume_l: Option<f64>,
}

impl Default for CenterTankConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            usable_fraction: 0.80,
            burn_priority: 1,
            published_usable_volume_l: None,
        }
    }
}

/// A trim tank in the horizontal stabiliser used for cruise balance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(default, deny_unknown_fields)]
pub struct TrimTankConfig {
    /// Whether the aircraft carries a trim tank.
    #[config(
        label = "Installed",
        help = "Whether the horizontal stabiliser box is a fuel tank. Long-range Airbus types use one to hold the cruise centre of gravity aft and cut trim drag; most twins do not have one."
    )]
    pub enabled: bool,

    /// Share of the stabiliser box volume that holds fuel.
    #[config(
        label = "Usable volume fraction",
        help = "Share of the geometric stabiliser spar-box volume between the two stations that is usable tank volume."
    )]
    pub usable_fraction: f64,

    /// Inboard boundary of the tank.
    #[config(
        label = "Span start",
        unit = "fraction of semi-span",
        help = "Inboard boundary of the trim tank as a fraction of the stabiliser semi-span from the aircraft centreline."
    )]
    pub span_start_fraction: f64,

    /// Outboard boundary of the tank.
    #[config(
        label = "Span end",
        unit = "fraction of semi-span",
        help = "Outboard boundary of the trim tank as a fraction of the stabiliser semi-span."
    )]
    pub span_end_fraction: f64,

    /// Order in which the tank is emptied.
    #[config(
        label = "Burn priority",
        help = "Order in which the trim tank is consumed: lower numbers are used first. In service it is transferred forward late in cruise so the aircraft lands with it empty."
    )]
    pub burn_priority: i64,

    /// Manufacturer-published usable volume, when the aircraft is registered.
    #[config(
        label = "Published usable volume",
        unit = "L",
        help = "Usable volume published by the manufacturer for the trim tank. When set it is the authoritative capacity and the geometric estimate only supplies the centroid."
    )]
    pub published_usable_volume_l: Option<f64>,
}

impl Default for TrimTankConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            usable_fraction: 0.85,
            span_start_fraction: 0.10,
            span_end_fraction: 0.85,
            burn_priority: 3,
            published_usable_volume_l: None,
        }
    }
}

/// An auxiliary tank in the fuselage, which has no wing geometry to size it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(default, deny_unknown_fields)]
pub struct AuxiliaryTankConfig {
    /// Whether the aircraft carries an auxiliary tank.
    #[config(
        label = "Installed",
        help = "Whether a fuselage auxiliary tank is installed, such as the DC-10-30 centre-wing auxiliary cell or an additional centre tank in the cargo hold. Its capacity is a declared volume, because no spar box sizes it."
    )]
    pub enabled: bool,

    /// Declared usable volume.
    #[config(
        label = "Usable volume",
        unit = "L",
        help = "Usable volume of the auxiliary tank. It is a declared quantity rather than a geometric estimate."
    )]
    pub usable_volume_l: f64,

    /// Where the tank sits along the fuselage.
    #[config(
        label = "Longitudinal position",
        unit = "fraction of fuselage length",
        help = "Centroid of the auxiliary tank as a fraction of fuselage length from the nose. A cargo-hold auxiliary tank sits near the wing carry-through box so it moves the centre of gravity as little as possible."
    )]
    pub x_position_fraction: f64,

    /// Order in which the tank is emptied.
    #[config(
        label = "Burn priority",
        help = "Order in which the auxiliary tank is consumed: lower numbers are used first. Fuselage fuel is transferred into the wing tanks early in cruise."
    )]
    pub burn_priority: i64,
}

impl Default for AuxiliaryTankConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            usable_volume_l: 0.0,
            x_position_fraction: 0.45,
            burn_priority: 0,
        }
    }
}

/// The complete tank arrangement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(default, deny_unknown_fields)]
pub struct FuelTankLayoutConfig {
    /// The innermost integral wing cell, present on every transport.
    #[config(
        nested,
        help = "The inner wing tank, from the side of the body outboard. Every transport has one; on a twin with three tanks this is the main wing tank."
    )]
    pub inner_wing: WingTankConfig,

    /// A mid-span cell on wings large enough to divide the span three ways.
    #[config(
        nested,
        help = "A mid-span wing tank between the inner and outer cells, found on the largest wings such as the A380."
    )]
    pub mid_wing: WingTankConfig,

    /// The outboard cell that relieves root bending and is burned last.
    #[config(
        nested,
        help = "The outer wing tank, burned last because outboard fuel relieves the wing root bending moment."
    )]
    pub outer_wing: WingTankConfig,

    /// The carry-through box tank under the cabin floor.
    #[config(
        nested,
        help = "The centre tank in the wing carry-through box, burned first."
    )]
    pub center: CenterTankConfig,

    /// The horizontal-stabiliser tank used for cruise balance.
    #[config(
        nested,
        help = "A trim tank in the horizontal stabiliser, used on long-range types to hold the cruise centre of gravity aft."
    )]
    pub trim: TrimTankConfig,

    /// A declared-volume fuselage tank.
    #[config(nested, help = "A fuselage auxiliary tank of declared volume.")]
    pub auxiliary: AuxiliaryTankConfig,

    /// Whether the geometric tank volumes are scaled to a published total.
    #[config(
        label = "Calibrate geometry to published capacity",
        help = "When the aircraft is a registered preset with a published total usable volume, scale every geometric tank estimate by one common factor so their sum reproduces it. The factor is retained and applied to redesigned wings so a changed planform keeps a consistent capacity; individual published tank volumes are never rescaled."
    )]
    pub calibrate_to_published_capacity: bool,
}

impl Default for FuelTankLayoutConfig {
    fn default() -> Self {
        Self {
            inner_wing: WingTankConfig {
                enabled: true,
                ..WingTankConfig::default()
            },
            mid_wing: WingTankConfig {
                enabled: false,
                span_start_fraction: 0.45,
                span_end_fraction: 0.70,
                burn_priority: 3,
                ..WingTankConfig::default()
            },
            outer_wing: WingTankConfig {
                enabled: false,
                span_start_fraction: 0.65,
                span_end_fraction: 0.88,
                burn_priority: 4,
                ..WingTankConfig::default()
            },
            center: CenterTankConfig::default(),
            trim: TrimTankConfig::default(),
            auxiliary: AuxiliaryTankConfig::default(),
            calibrate_to_published_capacity: true,
        }
    }
}

impl FuelTankLayoutConfig {
    /// Whether the serialized group equals the defaults.
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Reject a layout whose stations cannot describe physical tanks.
    ///
    /// Adjacent wing cells may share a boundary but may not overlap, because
    /// two tanks claiming the same bay would count its volume twice.
    pub fn validate(&self) -> Result<(), String> {
        let wing_cells = [
            ("inner_wing", &self.inner_wing),
            ("mid_wing", &self.mid_wing),
            ("outer_wing", &self.outer_wing),
        ];
        let mut active: Vec<(&str, f64, f64)> = Vec::new();
        for (name, cell) in wing_cells {
            if !cell.enabled {
                continue;
            }
            validate_interval(name, cell.span_start_fraction, cell.span_end_fraction)?;
            validate_fraction(name, "usable_fraction", cell.usable_fraction)?;
            if let Some(volume) = cell.published_usable_volume_l {
                if !volume.is_finite() || volume <= 0.0 {
                    return Err(format!("{name} published usable volume must be positive"));
                }
            }
            active.push((name, cell.span_start_fraction, cell.span_end_fraction));
        }
        for (index, (name, start, end)) in active.iter().enumerate() {
            for (other_name, other_start, other_end) in active.iter().skip(index + 1) {
                let overlap = start.max(*other_start) < end.min(*other_end);
                if overlap {
                    return Err(format!(
                        "{name} and {other_name} wing tanks overlap in span"
                    ));
                }
            }
        }
        if self.center.enabled {
            validate_fraction("center", "usable_fraction", self.center.usable_fraction)?;
        }
        if self.trim.enabled {
            validate_interval(
                "trim",
                self.trim.span_start_fraction,
                self.trim.span_end_fraction,
            )?;
            validate_fraction("trim", "usable_fraction", self.trim.usable_fraction)?;
        }
        if self.auxiliary.enabled {
            if !self.auxiliary.usable_volume_l.is_finite() || self.auxiliary.usable_volume_l <= 0.0
            {
                return Err("auxiliary tank usable volume must be positive".to_owned());
            }
            validate_fraction(
                "auxiliary",
                "x_position_fraction",
                self.auxiliary.x_position_fraction,
            )?;
        }
        Ok(())
    }
}

fn validate_interval(name: &str, start: f64, end: f64) -> Result<(), String> {
    if !start.is_finite() || !end.is_finite() || !(0.0..=1.0).contains(&start) || end > 1.0 {
        return Err(format!("{name} span fractions must lie in [0, 1]"));
    }
    if end <= start {
        return Err(format!("{name} span end must lie outboard of its start"));
    }
    Ok(())
}

fn validate_fraction(name: &str, field: &str, value: f64) -> Result<(), String> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(format!("{name} {field} must lie in [0, 1]"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_layout_is_a_twin_with_inner_wing_and_centre_tanks() {
        let layout = FuelTankLayoutConfig::default();
        assert!(layout.inner_wing.enabled);
        assert!(layout.center.enabled);
        assert!(!layout.outer_wing.enabled);
        assert!(!layout.trim.enabled);
        assert!(!layout.auxiliary.enabled);
        assert!(layout.validate().is_ok());
        assert!(layout.is_default());
    }

    #[test]
    fn overlapping_wing_cells_are_rejected_because_they_would_double_count_a_bay() {
        let layout = FuelTankLayoutConfig {
            outer_wing: WingTankConfig {
                enabled: true,
                span_start_fraction: 0.50,
                span_end_fraction: 0.90,
                ..WingTankConfig::default()
            },
            ..Default::default()
        };
        assert!(layout.validate().is_err());
    }

    #[test]
    fn adjacent_wing_cells_may_share_a_boundary() {
        let layout = FuelTankLayoutConfig {
            outer_wing: WingTankConfig {
                enabled: true,
                span_start_fraction: 0.65,
                span_end_fraction: 0.90,
                ..WingTankConfig::default()
            },
            ..Default::default()
        };
        assert!(layout.validate().is_ok());
    }

    #[test]
    fn an_auxiliary_tank_needs_a_declared_volume() {
        let layout = FuelTankLayoutConfig {
            auxiliary: AuxiliaryTankConfig {
                enabled: true,
                ..AuxiliaryTankConfig::default()
            },
            ..Default::default()
        };
        assert!(layout.validate().is_err());
    }

    #[test]
    fn a_reversed_span_interval_is_rejected() {
        let layout = FuelTankLayoutConfig {
            inner_wing: WingTankConfig {
                enabled: true,
                span_start_fraction: 0.6,
                span_end_fraction: 0.2,
                ..WingTankConfig::default()
            },
            ..Default::default()
        };
        assert!(layout.validate().is_err());
    }
}
