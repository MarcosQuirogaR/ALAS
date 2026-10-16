// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The wingtip device of [`AircraftBuilder`]: a winglet, sharklet or tip fence
//! lofted as one extra symmetric lifting surface attached at the main-wing tip.
//!
//! The surface is appended after the three product surfaces, so the indices of
//! the main wing and both tails (`wings[0..3]`) never change, and a build
//! without a device is bit-identical to one before this module existed.
//! Frame, signs and the blend are defined in `alas_config::WingtipDevice`.
//! The section airfoil is the device's own or the wing-tip airfoil; the
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
        let airfoil = match device.airfoil.as_deref() {
            Some(name) => Self::resolve(name)?,
            None => tip_airfoil.clone(),
        };
        Ok(Some(device_surface(device, main_wing, &airfoil)))
    }
}

/// Loft `device` onto the last section of `main_wing`.
fn device_surface(device: &WingtipDevice, main_wing: &Wing, airfoil: &Airfoil) -> Wing {
    let tip = &main_wing.xsecs[main_wing.xsecs.len() - 1];
    let xsecs = device
        .stations(tip.chord)
        .iter()
        .map(|s| {
            WingXSec::new(
                [
                    tip.xyz_le[0] + s.dx_m,
                    tip.xyz_le[1] + s.dy_m,
                    tip.xyz_le[2] + s.dz_m,
                ],
                s.chord_m,
                s.twist_deg,
                airfoil.clone(),
            )
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
    use alas_config::{DesignVector, GeometryConfig, WingtipDeviceKind};

    use super::*;

    fn device(kind: WingtipDeviceKind) -> WingtipDevice {
        WingtipDevice {
            kind,
            height_m: 2.0,
            down_height_m: 0.0,
            cant_deg: 10.0,
            root_chord_m: None,
            tip_chord_m: 0.5,
            le_sweep_deg: 40.0,
            toe_out_deg: 1.0,
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
        assert!(surface.xsecs.last().expect("tip").xyz_le[2] > wing_tip.xyz_le[2] + 1.99);
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
    fn the_reference_contract_ignores_the_device() {
        let mut geometry = GeometryConfig::default();
        geometry.wing.wingtip_device = Some(device(WingtipDeviceKind::Fence));
        let airplane = AircraftBuilder::new_reference_compatibility(Some(geometry))
            .build(None, false)
            .expect("reference");
        assert_eq!(airplane.wings.len(), 3);
    }
}
