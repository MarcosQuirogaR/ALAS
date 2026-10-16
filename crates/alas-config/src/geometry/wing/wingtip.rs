// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Wingtip devices: blended winglets and sharklets, and tip fences.
//!
//! A [`WingtipDevice`] is an extra lifting surface attached at the main-wing
//! tip. It is `None` on [`super::WingConfig`] by default and then changes
//! nothing. A swept-back (raked) tip is not a device: it is the existing
//! `tip_rake_*` planform fields of [`super::WingConfig`].
//!
//! # Frame and signs
//!
//! Right half-wing, geometry axes of the aircraft: x aft, y to starboard, z
//! up, metres and degrees. The device is described by [`WingtipDevice::stations`],
//! offsets relative to the leading edge of the wing-tip section; the left
//! half is the mirror image (the built surface is symmetric).
//!
//! * `cant_deg` is measured from the vertical in the YZ plane: 0 is a vertical
//!   winglet, 90 a horizontal extension. Positive cant leans the device
//!   outboard (the tip leaves the wing plane outward as it rises).
//! * `height_m` is the vertical rise of the device above the wing-tip leading
//!   edge, not the length along its axis.
//! * `le_sweep_deg` is the leading-edge sweep in the device's own plane,
//!   positive aft, as for a wing.
//! * `toe_out_deg` positive turns the leading edge outward (toe-out, the
//!   Whitcomb convention); it reduces the inward lift incidence.
//! * The root chord's trailing edge is aligned with the wing-tip trailing edge
//!   and the root chord defaults to the wing-tip chord, so the root section
//!   coincides with the wing tip and the surfaces are continuous.
//!
//! # Blending
//!
//! A sharklet (`blend_radius_m > 0`) leaves the wing tangent to the wing
//! plane and turns through a circular arc of that radius onto the straight
//! device axis at `cant_deg`; the arc is sampled with [`ARC_STATIONS`]
//! stations. A winglet with `blend_radius_m == 0` has a sharp kink.
//!
//! # Span convention
//!
//! `span_includes_device = false` (default): the design-vector `span_m` is the
//! wing-proper span, tip to tip at the wing-tip chord, and the device adds
//! [`WingtipDevice::lateral_reach_m`] on each side. `true`: `span_m` is the
//! published overall span including the devices (A320neo-type sharklet
//! spans), and the wing semispan is shortened by the lateral reach so the
//! built overall span equals `span_m`. [`WingtipDevice::wing_span_m`] is the
//! single conversion, used by [`super::WingConfig::transport_planform`].

use serde::{Deserialize, Serialize};

/// Stations sampled along the blend arc of a sharklet, endpoints included.
pub const ARC_STATIONS: usize = 6;

/// Kind of wingtip device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WingtipDeviceKind {
    /// Canted tip surface with a sharp kink (or a small blend radius).
    #[default]
    Winglet,
    /// Large blended winglet: arc from the wing plane onto the device axis.
    Sharklet,
    /// Vertical plate through the tip, extending above and optionally below.
    Fence,
}

/// One section of the device, relative to the wing-tip leading edge (right
/// half), metres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeviceStation {
    /// Leading-edge x offset, aft positive.
    pub dx_m: f64,
    /// Outboard y offset.
    pub dy_m: f64,
    /// Vertical offset, up positive.
    pub dz_m: f64,
    /// Chord, metres.
    pub chord_m: f64,
    /// Section twist about the spanwise axis in the aircraft-builder
    /// convention, degrees (`-toe_out_deg`).
    pub twist_deg: f64,
}

/// Why a device cannot be built.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum WingtipDeviceError {
    /// A value is not finite.
    #[error("wingtip device {field} must be finite, got {value}")]
    NonFinite {
        /// Field name.
        field: &'static str,
        /// Offending value.
        value: f64,
    },
    /// A value is outside its valid range.
    #[error("wingtip device {field} = {value} is outside {range}")]
    OutOfRange {
        /// Field name.
        field: &'static str,
        /// Offending value.
        value: f64,
        /// Valid range text.
        range: &'static str,
    },
    /// The blend arc needs more height than the device has.
    #[error(
        "blend radius {radius_m} m needs {arc_rise_m} m of rise but the height is {height_m} m"
    )]
    BlendTooLarge {
        /// Blend radius.
        radius_m: f64,
        /// Vertical rise consumed by the arc.
        arc_rise_m: f64,
        /// Device height.
        height_m: f64,
    },
}

/// A wingtip device (see the module documentation for frame and signs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WingtipDevice {
    /// Device kind.
    #[serde(default)]
    pub kind: WingtipDeviceKind,
    /// Vertical rise above the wing-tip leading edge, m (> 0).
    pub height_m: f64,
    /// Fence only: extension below the wing, m (>= 0).
    #[serde(default)]
    pub down_height_m: f64,
    /// Cant from the vertical, deg, in `[0, 80]`; fences are always 0.
    #[serde(default)]
    pub cant_deg: f64,
    /// Root chord, m; `None` takes the wing-tip chord.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_chord_m: Option<f64>,
    /// Tip chord, m (> 0, not above the root chord).
    pub tip_chord_m: f64,
    /// Leading-edge sweep in the device plane, deg, positive aft, in `(-30, 80)`.
    #[serde(default)]
    pub le_sweep_deg: f64,
    /// Toe-out of the sections, deg, positive leading edge outward.
    #[serde(default)]
    pub toe_out_deg: f64,
    /// Radius of the blend arc, m; 0 for a sharp kink. Ignored by fences.
    #[serde(default)]
    pub blend_radius_m: f64,
    /// Airfoil name; `None` takes the wing-tip airfoil.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub airfoil: Option<String>,
    /// Whether the design-vector `span_m` already includes the devices.
    #[serde(default)]
    pub span_includes_device: bool,
    /// Whether the aerodynamic (VLM) model carries the device. Phase 1: false.
    #[serde(default)]
    pub include_in_vlm: bool,
}

impl WingtipDevice {
    /// Validate the parameters.
    ///
    /// # Errors
    ///
    /// [`WingtipDeviceError`] for non-finite, non-physical or inconsistent values.
    pub fn validate(&self) -> Result<(), WingtipDeviceError> {
        let finite = |field, value: f64| {
            if value.is_finite() {
                Ok(())
            } else {
                Err(WingtipDeviceError::NonFinite { field, value })
            }
        };
        let range = |field, value: f64, ok: bool, range| {
            if ok {
                Ok(())
            } else {
                Err(WingtipDeviceError::OutOfRange {
                    field,
                    value,
                    range,
                })
            }
        };
        for (field, value) in [
            ("height_m", self.height_m),
            ("down_height_m", self.down_height_m),
            ("cant_deg", self.cant_deg),
            ("tip_chord_m", self.tip_chord_m),
            ("le_sweep_deg", self.le_sweep_deg),
            ("toe_out_deg", self.toe_out_deg),
            ("blend_radius_m", self.blend_radius_m),
        ] {
            finite(field, value)?;
        }
        range("height_m", self.height_m, self.height_m > 0.0, "(0, inf)")?;
        range(
            "down_height_m",
            self.down_height_m,
            self.down_height_m >= 0.0,
            "[0, inf)",
        )?;
        range(
            "cant_deg",
            self.cant_deg,
            (0.0..=80.0).contains(&self.cant_deg),
            "[0, 80]",
        )?;
        range(
            "tip_chord_m",
            self.tip_chord_m,
            self.tip_chord_m > 0.0,
            "(0, inf)",
        )?;
        range(
            "le_sweep_deg",
            self.le_sweep_deg,
            self.le_sweep_deg > -30.0 && self.le_sweep_deg < 80.0,
            "(-30, 80)",
        )?;
        range(
            "toe_out_deg",
            self.toe_out_deg,
            self.toe_out_deg.abs() <= 20.0,
            "[-20, 20]",
        )?;
        range(
            "blend_radius_m",
            self.blend_radius_m,
            self.blend_radius_m >= 0.0,
            "[0, inf)",
        )?;
        if let Some(root) = self.root_chord_m {
            finite("root_chord_m", root)?;
            range("root_chord_m", root, root > 0.0, "(0, inf)")?;
            range(
                "tip_chord_m",
                self.tip_chord_m,
                self.tip_chord_m <= root,
                "<= root chord",
            )?;
        }
        if self.kind == WingtipDeviceKind::Fence {
            range(
                "cant_deg",
                self.cant_deg,
                self.cant_deg == 0.0,
                "0 for a fence",
            )?;
        } else if self.blend_radius_m > 0.0 {
            let rise = self.arc_rise_m();
            if rise >= self.height_m {
                return Err(WingtipDeviceError::BlendTooLarge {
                    radius_m: self.blend_radius_m,
                    arc_rise_m: rise,
                    height_m: self.height_m,
                });
            }
        }
        Ok(())
    }

    /// Turn angle of the blend arc, rad: from the wing plane to the axis.
    fn arc_turn_rad(&self) -> f64 {
        (90.0 - self.cant_deg).to_radians()
    }

    /// Vertical rise consumed by the blend arc, m.
    fn arc_rise_m(&self) -> f64 {
        self.blend_radius_m * (1.0 - self.arc_turn_rad().cos())
    }

    /// Outboard reach of the device tip beyond the wing tip, m. For a fence it
    /// is 0 (the plate lies in the plane of the tip).
    ///
    /// Requires a valid device ([`Self::validate`]).
    #[must_use]
    pub fn lateral_reach_m(&self) -> f64 {
        self.stations(1.0)
            .last()
            .map_or(0.0, |station| station.dy_m)
    }

    /// The wing-proper span for a design-vector span: unchanged unless
    /// `span_includes_device`, then reduced by the reach on both sides.
    #[must_use]
    pub fn wing_span_m(&self, design_span_m: f64) -> f64 {
        if self.span_includes_device {
            2.0f64.mul_add(-self.lateral_reach_m(), design_span_m)
        } else {
            design_span_m
        }
    }

    /// Sections of the right-half device from the root (wing tip) outward and
    /// upward; a fence lists its lower tip first, then root, then upper tip.
    ///
    /// `wing_tip_chord_m` is the chord of the wing-tip section the device is
    /// attached to. Offsets are relative to the wing-tip leading edge, with the
    /// root trailing edge aligned to the wing-tip trailing edge, so the root
    /// section's `dx_m` is `wing_tip_chord_m - root_chord`.
    ///
    /// Requires a valid device ([`Self::validate`]).
    #[must_use]
    pub fn stations(&self, wing_tip_chord_m: f64) -> Vec<DeviceStation> {
        let root_chord_m = self.root_chord_m.unwrap_or(wing_tip_chord_m);
        let root_dx_m = wing_tip_chord_m - root_chord_m;
        let twist_deg = -self.toe_out_deg;
        let tan_sweep = self.le_sweep_deg.to_radians().tan();
        if self.kind == WingtipDeviceKind::Fence {
            let station = |dz_m: f64, chord_m: f64, run: f64| DeviceStation {
                dx_m: root_dx_m + run * tan_sweep,
                dy_m: 0.0,
                dz_m,
                chord_m,
                twist_deg,
            };
            let mut stations = Vec::with_capacity(3);
            if self.down_height_m > 0.0 {
                // Lower tip: same taper law, swept as the upper.
                stations.push(station(
                    -self.down_height_m,
                    self.tip_chord_m,
                    self.down_height_m,
                ));
            }
            stations.push(station(0.0, root_chord_m, 0.0));
            stations.push(station(self.height_m, self.tip_chord_m, self.height_m));
            return stations;
        }
        let cant = self.cant_deg.to_radians();
        // Path (y, z) with arclength s from the root.
        let mut path: Vec<(f64, f64, f64)> = vec![(0.0, 0.0, 0.0)];
        let mut s_end_arc = 0.0;
        let (mut y, mut z) = (0.0, 0.0);
        if self.blend_radius_m > 0.0 {
            let turn = self.arc_turn_rad();
            let r = self.blend_radius_m;
            for i in 1..ARC_STATIONS {
                let theta = turn * i as f64 / (ARC_STATIONS - 1) as f64;
                path.push((r * theta.sin(), r * (1.0 - theta.cos()), r * theta));
            }
            let last = path[path.len() - 1];
            (y, z, s_end_arc) = last;
        }
        let straight = (self.height_m - z) / cant.cos();
        y += straight * cant.sin();
        path.push((y, self.height_m, s_end_arc + straight));
        let total = s_end_arc + straight;
        path.into_iter()
            .map(|(y_m, z_m, s)| {
                let t = if total > 0.0 { s / total } else { 0.0 };
                DeviceStation {
                    dx_m: root_dx_m + s * tan_sweep,
                    dy_m: y_m,
                    dz_m: z_m,
                    chord_m: root_chord_m + t * (self.tip_chord_m - root_chord_m),
                    twist_deg,
                }
            })
            .collect()
    }
}

impl super::WingConfig {
    /// The wing-proper span for a design-vector span: the design span unless a
    /// device with `span_includes_device` is set (see the module documentation).
    #[must_use]
    pub fn wing_span_m(&self, design_span_m: f64) -> f64 {
        self.wingtip_device
            .as_ref()
            .map_or(design_span_m, |device| device.wing_span_m(design_span_m))
    }
}

#[cfg(test)]
// A failed expect in a test is the assertion failing.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn sharklet() -> WingtipDevice {
        WingtipDevice {
            kind: WingtipDeviceKind::Sharklet,
            height_m: 2.4,
            down_height_m: 0.0,
            cant_deg: 15.0,
            root_chord_m: Some(1.7),
            tip_chord_m: 0.6,
            le_sweep_deg: 45.0,
            toe_out_deg: 2.0,
            blend_radius_m: 1.2,
            airfoil: None,
            span_includes_device: true,
            include_in_vlm: false,
        }
    }

    #[test]
    fn sharklet_stations_end_at_the_height_and_start_on_the_wing() {
        let device = sharklet();
        device.validate().expect("valid");
        let stations = device.stations(1.2);
        assert_eq!(stations.len(), ARC_STATIONS + 1);
        let first = stations[0];
        assert!(first.dy_m == 0.0 && first.dz_m == 0.0);
        assert!((first.dx_m - (1.2 - 1.7)).abs() < 1e-12);
        let last = stations[stations.len() - 1];
        assert!((last.dz_m - 2.4).abs() < 1e-12);
        assert!((last.chord_m - 0.6).abs() < 1e-12);
        assert!(stations
            .windows(2)
            .all(|p| p[1].dz_m >= p[0].dz_m && p[1].dy_m >= p[0].dy_m));
        // The final straight leg leans at the cant angle.
        let (a, b) = (stations[stations.len() - 2], last);
        let angle = ((b.dy_m - a.dy_m) / (b.dz_m - a.dz_m)).atan().to_degrees();
        assert!((angle - 15.0).abs() < 1e-9, "{angle}");
        assert!((last.twist_deg + 2.0).abs() < 1e-12);
    }

    #[test]
    fn wing_span_subtracts_the_reach_only_when_the_published_span_includes_it() {
        let mut device = sharklet();
        let reach = device.lateral_reach_m();
        assert!(reach > 0.0);
        assert!((device.wing_span_m(35.8) - (35.8 - 2.0 * reach)).abs() < 1e-12);
        device.span_includes_device = false;
        assert_eq!(device.wing_span_m(35.8), 35.8);
    }

    #[test]
    fn fence_extends_above_and_below_at_zero_cant() {
        let device = WingtipDevice {
            kind: WingtipDeviceKind::Fence,
            height_m: 2.4,
            down_height_m: 1.0,
            cant_deg: 0.0,
            root_chord_m: None,
            tip_chord_m: 1.0,
            le_sweep_deg: 0.0,
            toe_out_deg: 0.0,
            blend_radius_m: 0.0,
            airfoil: None,
            span_includes_device: false,
            include_in_vlm: false,
        };
        device.validate().expect("valid");
        let s = device.stations(3.0);
        assert_eq!(s.len(), 3);
        assert!((s[0].dz_m + 1.0).abs() < 1e-12 && (s[2].dz_m - 2.4).abs() < 1e-12);
        assert!(s.iter().all(|p| p.dy_m == 0.0));
        assert_eq!(device.lateral_reach_m(), 0.0);
    }

    #[test]
    fn invalid_devices_are_rejected() {
        let mut device = sharklet();
        device.blend_radius_m = 40.0;
        assert!(matches!(
            device.validate(),
            Err(WingtipDeviceError::BlendTooLarge { .. })
        ));
        let mut device = sharklet();
        device.height_m = f64::NAN;
        assert!(device.validate().is_err());
        let mut device = sharklet();
        device.tip_chord_m = 2.0;
        assert!(device.validate().is_err());
    }
}
