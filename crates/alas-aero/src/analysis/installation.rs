// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The span efficiency the installed airframe loses against the lattice.
//!
//! The vortex lattice is built from the lifting surfaces alone and carries
//! the wing through the fuselage to the plane of symmetry: it holds the span
//! efficiency of the clean planform, its twist and its wingtip device, but no
//! loss from the bodies, nacelles or propellers. Torenbeek, *Advanced
//! Aircraft Design* (2013), p. 91, Fig. 4.4, gives the span efficiency factor
//! `e_v` of Eq. 4.12, `CD_i = CL^2 / (pi A e_v)`, of one wing's lateral lift
//! distribution: 0.976 for the trapezoidal wing alone, 0.948 with fuselage
//! interference, 0.884 with fuselage plus wing-mounted engine interference
//! and 0.925 with fuselage-mounted engine interference; p. 100 takes the
//! wing-mounted nacelles into account through that factor. A wing in the
//! slipstream of tractor propellers loses a further 4 % of it, typically
//! (p. 100, propeller wake effects).
//!
//! The figure covers the wing-and-body spanload only, so the installation
//! adds the difference of the two factors on the wing's own lift:
//! `Delta CD_i = (1 / e_installed - 1 / e_wing alone) CL_w^2 / (pi A)`, `CL_w`
//! the lift of the main wing and its tip device on the reference area. The
//! tail load and trim, the wing-tail interference and the zero-lift induced
//! drag of the twist stay the lattice's.
//!
//! Winglets add 5 to 10 % (p. 49), but that is no factor here: a device that
//! takes part in the lattice already carries its own benefit, and a
//! geometry-only device carries none by its contract. Torenbeek's `k_L`
//! (p. 96, Eq. 4.22) is an alternative to the lift-dependent viscous term of
//! this crate, not an addition to it, and is not applied.
//!
//! An airplane without a fuselage is the lattice itself, and the frozen
//! reference path keeps the bare lattice its fixtures were made with.

use alas_config::engines::PropulsionTechnology;
use alas_config::geometry::GeometryConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::WINGTIP_DEVICE_NAME;

use super::AeroAnalysis;
use crate::operating_point::{AxisFrame, OperatingPoint};
use crate::vlm::VlmResult;

/// Fig. 4.4: the trapezoidal wing alone.
const WING_ALONE: f64 = 0.976;
/// Fig. 4.4: with fuselage interference.
const WITH_FUSELAGE: f64 = 0.948;
/// Fig. 4.4: with fuselage plus wing-mounted engine interference.
const WITH_WING_ENGINES: f64 = 0.884;
/// Fig. 4.4: with fuselage-mounted engine interference.
const WITH_FUSELAGE_ENGINES: f64 = 0.925;
/// p. 100: the further loss of a propeller transport.
const PROPELLER_WAKE: f64 = 0.96;

/// How the engines sit on the airframe, as Fig. 4.4 tells them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineInstallation {
    /// No engine is modelled.
    None,
    /// At least one engine is podded on the wing.
    Wing,
    /// Every engine sits on the fuselage or the tail.
    Fuselage,
}

impl EngineInstallation {
    /// The installation of `geometry`'s engines: any podded off-centreline
    /// engine without a rear-fuselage mounting is on the wing; a trijet's
    /// centre engine alone does not move the wing case.
    #[must_use]
    pub fn of(geometry: &GeometryConfig) -> Self {
        let engine = &geometry.engine;
        if !engine.spanwise_positions_m.is_empty() && engine.aft_fuselage.is_none() {
            Self::Wing
        } else if !engine.spanwise_positions_m.is_empty() || engine.centre_engine.is_some() {
            Self::Fuselage
        } else {
            Self::None
        }
    }
}

/// The installed span efficiency factor of Fig. 4.4 (with the propeller
/// loss of p. 100), `None` without a fuselage.
#[must_use]
pub fn installed_span_efficiency(geometry: &GeometryConfig, has_fuselage: bool) -> Option<f64> {
    if !has_fuselage {
        return None;
    }
    let installation = EngineInstallation::of(geometry);
    let installed = match installation {
        EngineInstallation::None => WITH_FUSELAGE,
        EngineInstallation::Wing => WITH_WING_ENGINES,
        EngineInstallation::Fuselage => WITH_FUSELAGE_ENGINES,
    };
    let propeller = installation == EngineInstallation::Wing
        && geometry.engine.propulsion_technology == PropulsionTechnology::Turboprop;
    Some(installed * if propeller { PROPELLER_WAKE } else { 1.0 })
}

/// Induced drag the installation adds to a wing lifting `wing_cl` on the
/// reference area at aspect ratio `aspect_ratio`:
/// `(1 / e_installed - 1 / 0.976) CL_w^2 / (pi A)`; zero without a fuselage.
#[must_use]
pub fn installation_induced_drag(
    geometry: &GeometryConfig,
    has_fuselage: bool,
    wing_cl: f64,
    aspect_ratio: f64,
) -> f64 {
    match installed_span_efficiency(geometry, has_fuselage) {
        Some(installed) if aspect_ratio > 0.0 && wing_cl.is_finite() => {
            (1.0 / installed - 1.0 / WING_ALONE) * wing_cl * wing_cl
                / (std::f64::consts::PI * aspect_ratio)
        }
        _ => 0.0,
    }
}

/// Lift coefficient, on `plane`'s reference area, of the main wing
/// (`plane.wings[0]`) and its lattice wingtip device in the solve `solved`
/// at `op_point`: the share of the lift whose spanload Fig. 4.4 describes.
#[must_use]
pub fn wing_lift_coefficient(
    solved: &VlmResult,
    op_point: &OperatingPoint,
    plane: &Airplane,
) -> f64 {
    let is_wing = |index: usize| {
        index == 0
            || plane
                .wings
                .get(index)
                .is_some_and(|wing| wing.name == WINGTIP_DEVICE_NAME)
    };
    let mut force = [0.0; 3];
    for (panel, panel_force) in solved.panels.iter().zip(&solved.panel_forces_geometry) {
        if is_wing(panel.wing_index) {
            for (total, component) in force.iter_mut().zip(panel_force) {
                *total += component;
            }
        }
    }
    let (bx, by, bz) = op_point.convert_axes(
        force[0],
        force[1],
        force[2],
        AxisFrame::Geometry,
        AxisFrame::Body,
    );
    let (_, _, wz) = op_point.convert_axes(bx, by, bz, AxisFrame::Body, AxisFrame::Wind);
    -wz / (op_point.dynamic_pressure() * plane.s_ref)
}

impl AeroAnalysis<'_> {
    /// [`installation_induced_drag`] of the analysed airplane at wing lift
    /// `wing_cl`: exactly 0 on the frozen reference path.
    #[must_use]
    pub fn installation_induced_drag(&self, wing_cl: f64) -> f64 {
        if self.reference_compatibility {
            return 0.0;
        }
        let aspect_ratio = self.plane.b_ref * self.plane.b_ref / self.plane.s_ref;
        installation_induced_drag(
            &self.geometry,
            !self.plane.fuselages.is_empty(),
            wing_cl,
            aspect_ratio,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry(wing_engines: usize, aft: bool, turboprop: bool) -> GeometryConfig {
        let mut geometry = GeometryConfig::default();
        geometry.engine.spanwise_positions_m =
            (0..wing_engines).map(|i| 3.0 * (i as f64 + 1.0)).collect();
        if aft {
            geometry.engine.aft_fuselage = Some(alas_config::AftFuselageEngineInstallation {
                inlet_ahead_of_tail_m: 5.0,
                axis_z_m: 0.7,
            });
        } else {
            geometry.engine.aft_fuselage = None;
        }
        geometry.engine.centre_engine = None;
        if turboprop {
            geometry.engine.propulsion_technology = PropulsionTechnology::Turboprop;
        }
        geometry
    }

    #[test]
    fn the_installation_adds_the_fig_4_4_difference_on_the_wing_lift() {
        let e = |g: &GeometryConfig| installed_span_efficiency(g, true).unwrap_or(f64::NAN);
        assert_eq!(e(&geometry(0, false, false)), 0.948);
        assert_eq!(e(&geometry(2, false, false)), 0.884);
        assert!((e(&geometry(2, false, true)) - 0.96 * 0.884).abs() < 1e-15);
        let rear = geometry(2, true, false);
        assert_eq!(EngineInstallation::of(&rear), EngineInstallation::Fuselage);
        assert_eq!(e(&rear), 0.925);
        assert!(installed_span_efficiency(&rear, false).is_none());
        // CL_w 0.5 at A 9.5 with wing engines: (1/0.884 - 1/0.976) 0.25 / (9.5 pi).
        let added = installation_induced_drag(&geometry(2, false, false), true, 0.5, 9.5);
        let expected = (1.0 / 0.884 - 1.0 / 0.976) * 0.25 / (9.5 * std::f64::consts::PI);
        assert!((added - expected).abs() < 1e-15);
        // Quadratic in the wing lift, zero without a fuselage or a lift.
        let double = installation_induced_drag(&geometry(2, false, false), true, 1.0, 9.5);
        assert!((double / added - 4.0).abs() < 1e-12);
        assert_eq!(
            installation_induced_drag(&geometry(2, false, false), false, 0.5, 9.5),
            0.0
        );
        assert_eq!(
            installation_induced_drag(&geometry(2, false, false), true, 0.0, 9.5),
            0.0
        );
    }
}
