// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The wingtip device of [`AircraftBuilder`]: a winglet, sharklet or tip fence
//! lofted as one extra symmetric lifting surface attached at the main-wing tip.
//!
//! The surface is appended after the three product surfaces, so the indices of
//! the main wing and both tails (`wings[0..3]`) never change, and a build
//! without a device is bit-identical to one before this module existed.
//! Frame, signs and the blend are defined in `alas_config::WingtipDevice`.
//! The section airfoil is the device's own or the built wing-tip section's; the
//! sections are positioned from the tip of the built main wing, so the device
//! follows any smoothing or rake applied to it.
//!
//! The surface is named [`WINGTIP_DEVICE_NAME`] when the device takes part in
//! the vortex-lattice model and [`WINGTIP_DEVICE_GEOMETRY_ONLY_NAME`]
//! otherwise (the default): the vortex-lattice mesher skips geometry-only
//! surfaces, while render, export and outline code see both.

use alas_config::WingtipDevice;

use super::{AircraftBuilder, BuildError, GeometryContract};
use crate::aircraft::airfoil::Airfoil;
use crate::aircraft::wing::{
    Wing, WingXSec, WINGTIP_DEVICE_GEOMETRY_ONLY_NAME, WINGTIP_DEVICE_NAME,
};
use crate::airfoil_library::{morph_airfoil, DEFAULT_MORPH_N_POINTS};

impl AircraftBuilder {
    /// The wingtip surface for the built `main_wing`, or `None` without a device
    /// (or under the reference-compatibility contract).
    ///
    /// # Errors
    ///
    /// [`BuildError::WingtipDevice`] for an invalid device, or
    /// [`BuildError::UnresolvedAirfoil`].
    pub(super) fn build_wingtip_device(
        &self,
        main_wing: &Wing,
        tip_airfoil: &Airfoil,
    ) -> Result<Option<Wing>, BuildError> {
        let Some(device) = self.geometry.wing.wingtip_device.as_ref() else {
            return Ok(None);
        };
        if self.geometry_contract != GeometryContract::Product {
            return Ok(None);
        }
        device.validate()?;
        // Without its own airfoil the device continues the built wing-tip section,
        // thickness stations included: the unscaled tip airfoil stood off the
        // wing's lower skin at the junction (B747-400: t/c 0.110 against the built
        // 0.080, 0.08 m below it in the front view).
        let airfoil = match device.airfoil.as_deref() {
            Some(name) => Self::resolve(name)?,
            None => main_wing
                .xsecs
                .last()
                .map_or_else(|| tip_airfoil.clone(), |tip| tip.airfoil.clone()),
        };
        Ok(Some(device_surface(device, main_wing, &airfoil)))
    }

    /// The chord of the built wing-tip section: the planform tip chord plus the
    /// device's trailing-edge flare, which the device root continues. The planform
    /// itself stays unflared.
    ///
    /// # Errors
    ///
    /// [`BuildError::WingtipDevice`] for an invalid device with a flare.
    pub(super) fn built_wing_tip_chord_m(&self, planform_chord_m: f64) -> Result<f64, BuildError> {
        Ok(planform_chord_m + self.built_wing_tip_flare_m()?)
    }

    /// The flare in the chord of the built wing-tip section, m.
    ///
    /// # Errors
    ///
    /// [`BuildError::WingtipDevice`] for an invalid device with a flare.
    pub(super) fn built_wing_tip_flare_m(&self) -> Result<f64, BuildError> {
        if self.geometry_contract != GeometryContract::Product {
            return Ok(0.0);
        }
        let device = self.geometry.wing.wingtip_device.as_ref();
        let Some((device, flare)) = device.and_then(|d| Some((d, d.root_te_flare?))) else {
            return Ok(0.0);
        };
        device.validate()?;
        Ok(flare.offset_m)
    }
}

/// Loft `device` onto the last section of `main_wing`.
fn device_surface(device: &WingtipDevice, main_wing: &Wing, airfoil: &Airfoil) -> Wing {
    let tip = &main_wing.xsecs[main_wing.xsecs.len() - 1];
    let flare_m = device.root_te_flare.map_or(0.0, |flare| flare.offset_m);
    let (_, tip_span_axis, _) = main_wing.frame_of_xsec(main_wing.xsecs.len() - 1);
    let stations = device.stations(tip.chord - flare_m);
    let root_chord_m = stations.first().map_or(tip.chord, |s| s.chord_m);
    let xsecs = stations
        .iter()
        .map(|s| {
            // A path station cuts the rounded corner of the same tip section: its
            // chord shortens as the leading edge runs aft, its depth does not (the
            // drawn front view keeps the tip-edge skins level to the device). So it
            // keeps the root section's thickness and camber in metres.
            let section = if s.in_wing_plane && s.chord_m > 0.0 && s.chord_m != root_chord_m {
                let scale = root_chord_m / s.chord_m;
                morph_airfoil(&airfoil.coordinates, scale, scale, DEFAULT_MORPH_N_POINTS)
            } else {
                airfoil.clone()
            };
            let mut xsec = WingXSec::new(
                [
                    tip.xyz_le[0] + s.dx_m,
                    tip.xyz_le[1] + s.dy_m,
                    tip.xyz_le[2] + s.dz_m,
                ],
                s.chord_m,
                // The root and path stations are the wing-tip section itself and
                // keep its twist (B747-400 washout -2 deg: at zero the device root
                // trailing edge stood 0.13 m off the wing's); the toe-out turns the
                // device proper about its own span.
                if s.in_wing_plane {
                    tip.twist
                } else {
                    s.twist_deg
                },
                section,
            );
            if s.in_wing_plane {
                xsec.span_axis = Some(tip_span_axis);
            }
            xsec
        })
        .collect();
    let name = if device.include_in_vlm {
        WINGTIP_DEVICE_NAME
    } else {
        WINGTIP_DEVICE_GEOMETRY_ONLY_NAME
    };
    Wing::new(name, xsecs, true)
}

#[cfg(test)]
// A failed expect in a test is the assertion failing.
#[allow(clippy::expect_used)]
mod tests {
    use alas_config::{
        DesignVector, GeometryConfig, ReferenceWingConfig, WingtipDeviceKind, WingtipTeFlare,
    };

    use super::*;

    fn device(kind: WingtipDeviceKind) -> WingtipDevice {
        WingtipDevice {
            kind,
            height_m: 2.0,
            down_height_m: 0.0,
            cant_deg: 10.0,
            root_chord_m: None,
            root_te_flare: None,
            tip_chord_m: 0.5,
            le_sweep_deg: 40.0,
            toe_out_deg: 1.0,
            arc_start_deg: None,
            path_stations: Vec::new(),
            blend_radius_m: if kind == WingtipDeviceKind::Sharklet {
                1.0
            } else {
                0.0
            },
            airfoil: None,
            span_includes_device: false,
            include_in_vlm: false,
        }
    }

    #[test]
    fn no_device_leaves_three_surfaces_and_a_device_appends_a_fourth() {
        let dv = DesignVector::default();
        let plain = AircraftBuilder::new(None)
            .build(Some(&dv), false)
            .expect("plain");
        assert_eq!(plain.wings.len(), 3);

        let mut geometry = GeometryConfig::default();
        geometry.wing.wingtip_device = Some(device(WingtipDeviceKind::Sharklet));
        let with = AircraftBuilder::new(Some(geometry))
            .build(Some(&dv), false)
            .expect("device");
        assert_eq!(with.wings.len(), 4);
        assert_eq!(with.wings[..3], plain.wings[..3]);
        assert_eq!(with.s_ref, plain.s_ref);
        let surface = &with.wings[3];
        assert_eq!(surface.name, WINGTIP_DEVICE_GEOMETRY_ONLY_NAME);
        let wing_tip = plain.wings[0].xsecs.last().expect("tip");
        assert_eq!(surface.xsecs[0].xyz_le[1], wing_tip.xyz_le[1]);
        assert_eq!(surface.xsecs[0].chord, wing_tip.chord);
        // The device root continues the built wing-tip airfoil.
        assert_eq!(surface.xsecs[0].airfoil, wing_tip.airfoil);
        assert!(surface.xsecs.last().expect("tip").xyz_le[2] > wing_tip.xyz_le[2] + 1.99);
    }

    #[test]
    fn a_root_te_flare_moves_only_the_built_tip_trailing_edge() {
        let dv = DesignVector::default();
        let build = |offset_m: Option<f64>| {
            let mut d = device(WingtipDeviceKind::Sharklet);
            d.root_te_flare = offset_m.map(|offset_m| WingtipTeFlare {
                offset_m,
                fade_length_m: 0.6,
            });
            let mut geometry = GeometryConfig::default();
            geometry.wing.wingtip_device = Some(d);
            geometry.wing.reference_wing = ReferenceWingConfig::published(100.0, Some(4.0), None);
            AircraftBuilder::new(Some(geometry))
                .build(Some(&dv), false)
                .expect("device")
        };
        let (plain, flared) = (build(None), build(Some(0.2)));
        assert_eq!(flared.s_ref, plain.s_ref);
        assert_eq!(flared.c_ref, plain.c_ref);
        assert_eq!(flared.b_ref, plain.b_ref);
        let a = plain.wings[0].xsecs.last().expect("tip");
        let b = flared.wings[0].xsecs.last().expect("tip");
        assert_eq!(a.xyz_le, b.xyz_le);
        assert!((b.chord - a.chord - 0.2).abs() < 1e-12);
        let root = &flared.wings[3].xsecs[0];
        assert_eq!(root.xyz_le, b.xyz_le);
        assert_eq!(root.chord, b.chord);
        let (a, b) = (
            plain.wings[3].xsecs.last().expect("tip"),
            flared.wings[3].xsecs.last().expect("tip"),
        );
        assert_eq!(a.xyz_le, b.xyz_le);
        assert_eq!(a.chord, b.chord);
    }

    #[test]
    fn a_published_span_that_includes_the_device_is_matched() {
        let dv = DesignVector::default();
        let mut geometry = GeometryConfig::default();
        let mut d = device(WingtipDeviceKind::Winglet);
        d.span_includes_device = true;
        let reach = d.lateral_reach_m();
        geometry.wing.wingtip_device = Some(d);
        let airplane = AircraftBuilder::new(Some(geometry))
            .build(Some(&dv), false)
            .expect("b");
        let overall = airplane.wings[0].projected_span() + 2.0 * reach;
        assert!(
            (overall - dv.span_m).abs() < 1e-9,
            "{overall} vs {}",
            dv.span_m
        );
    }

    #[test]
    fn path_stations_keep_the_wing_tip_plane_and_the_turn_starts_at_the_last() {
        let dv = DesignVector::default();
        let mut d = device(WingtipDeviceKind::Winglet);
        d.toe_out_deg = 0.0;
        let station = |dy_m, dz_m| alas_config::WingtipPathStation {
            dx_m: 0.3,
            dy_m,
            dz_m,
            chord_m: 1.0,
        };
        d.path_stations = vec![station(0.1, 0.02), station(0.2, 0.08)];
        let mut geometry = GeometryConfig::default();
        geometry.wing.wingtip_device = Some(d);
        let airplane = AircraftBuilder::new(Some(geometry))
            .build(Some(&dv), false)
            .expect("device");
        let (wing, surface) = (&airplane.wings[0], &airplane.wings[3]);
        let tip_axis = wing.frame_of_xsec(wing.xsecs.len() - 1).1;
        let wing_tip = wing.xsecs.last().expect("tip");
        for index in 0..3 {
            assert_eq!(surface.xsecs[index].span_axis, Some(tip_axis));
            assert_eq!(surface.frame_of_xsec(index).1, tip_axis);
            // A cut of the wing-tip section: its twist, and its depth in metres.
            assert_eq!(surface.xsecs[index].twist, wing_tip.twist);
            let depth = |xsec: &WingXSec| {
                let samples: Vec<f64> = (0..=200).map(|k| f64::from(k) / 200.0).collect();
                xsec.airfoil.max_thickness(&samples) * xsec.chord
            };
            let (a, b) = (depth(&surface.xsecs[index]), depth(wing_tip));
            assert!((a / b - 1.0).abs() < 0.01, "{index} {a} {b}");
        }
        assert_eq!(surface.xsecs[3].span_axis, None);
        // The lower skin of the last path station sits under its leading edge in
        // the wing-tip plane rather than standing off outboard on the outside of
        // the turn into the device.
        let outlines = surface.section_outlines(Some(40));
        let le = surface.xsecs[2].xyz_le;
        let lowest = outlines[2]
            .iter()
            .copied()
            .fold(
                [0.0, 0.0, f64::INFINITY],
                |m, p| if p[2] < m[2] { p } else { m },
            );
        let lean = (lowest[1] - le[1]) / (le[2] - lowest[2]);
        assert!((lean - tip_axis[2] / tip_axis[1]).abs() < 1e-9, "{lean}");

        // A section inserted between two path stations keeps their plane; one
        // inserted on the turn into the device derives its frame.
        use crate::aircraft::wing::SpacingFunction;
        let fine = surface
            .subdivide_sections(2, SpacingFunction::Cosspace)
            .expect("subdivides");
        assert_eq!(fine.xsecs.len(), 7);
        for index in 0..5 {
            assert_eq!(fine.xsecs[index].span_axis, Some(tip_axis), "{index}");
        }
        assert_eq!(fine.xsecs[5].span_axis, None);
        let meshed = surface
            .mesh_spanwise(8, SpacingFunction::Cosspace)
            .expect("meshes");
        let in_plane = meshed
            .xsecs
            .iter()
            .filter(|xsec| xsec.span_axis == Some(tip_axis))
            .count();
        assert!(in_plane > 3, "{in_plane}");
    }

    #[test]
    fn the_reference_contract_ignores_the_device() {
        let mut geometry = GeometryConfig::default();
        geometry.wing.wingtip_device = Some(device(WingtipDeviceKind::Fence));
        let airplane = AircraftBuilder::new_reference_compatibility(Some(geometry))
            .build(None, false)
            .expect("reference");
        assert_eq!(airplane.wings.len(), 3);
    }
}
