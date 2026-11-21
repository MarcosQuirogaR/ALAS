// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Loft stations for the spanwise dihedral and thickness profiles of the main
//! wing (see `alas_config::WingDihedralStation`).
//!
//! With no profile configured nothing here runs, so the wing is exactly the
//! root/break/tip loft. With a profile, every station gets a loft section of
//! its own whose leading-edge x, chord and twist lie on the straight planform
//! panels (linear in span between the existing sections), whose height is the
//! dihedral polyline and whose section is the root-kink-tip blend (or the
//! section law's section, where the wing names sections) scaled to the
//! thickness profile.

use super::section_law::Sections;
use super::{AircraftBuilder, BuildError};
use crate::aircraft::airfoil::Airfoil;
use crate::aircraft::wing::WingXSec;
use crate::airfoil_library::{morph_airfoil, DEFAULT_MORPH_N_POINTS};
use alas_config::{TransportPlanform, WingHeights};

/// Span-fraction tolerance below which a profile station reuses a section.
const SAME_STATION: f64 = 5.0e-4; // equals `alas_config::STATION_SPAN_TOLERANCE`

impl AircraftBuilder {
    /// The loft section at an outboard station (a tip-rake start, a trailing-edge
    /// break, an edge station): leading edge, chord and height from the planform,
    /// twist linear from the kink to the tip and the root-kink-tip section blend.
    pub(super) fn tip_rake_section(
        &self,
        dv: &alas_config::DesignVector,
        planform: &TransportPlanform,
        heights: &WingHeights,
        rake: &alas_config::MainWingStation,
        root_section: &Airfoil,
        tip_airfoil: &Airfoil,
    ) -> Result<WingXSec, BuildError> {
        let wing = &self.geometry.wing;
        let t = (rake.y_m - planform.kink.y_m) / (planform.tip.y_m - planform.kink.y_m);
        let twist = wing.break_twist_deg + t * (dv.tip_twist_deg - wing.break_twist_deg);
        let airfoil = root_section.blend_with_another_airfoil(
            tip_airfoil,
            (rake.span_fraction - planform.kink.span_fraction)
                / (1.0 - planform.kink.span_fraction),
            DEFAULT_MORPH_N_POINTS,
        )?;
        Ok(WingXSec::new(
            [
                rake.leading_edge_x_m,
                rake.y_m,
                heights.at(planform, rake.y_m),
            ],
            rake.chord_m,
            twist,
            airfoil,
        ))
    }

    /// Give the defining sections the section law's sections (when the wing
    /// names sections), apply the thickness profile to them and insert the
    /// profile and section stations. A no-op when all three lists are empty.
    pub(super) fn apply_wing_profiles(
        &self,
        planform: &TransportPlanform,
        heights: &WingHeights,
        (root_section, tip_airfoil, law): Sections<'_>,
        xsecs: &mut Vec<WingXSec>,
    ) -> Result<(), BuildError> {
        let wing = &self.geometry.wing;
        if law.is_none() && wing.dihedral_stations.is_empty() && wing.thickness_stations.is_empty()
        {
            return Ok(());
        }
        let kink_fraction = planform.kink.span_fraction;
        wing.validate_profiles(kink_fraction)?;
        if let Some(law) = law {
            law.apply(planform.tip.y_m, xsecs);
        }
        let tip_y_m = planform.tip.y_m;
        let thickness_active = !wing.thickness_stations.is_empty();
        let base_at = |eta: f64| -> Result<Airfoil, BuildError> {
            let base = if let Some(law) = law {
                law.at(eta)
            } else if eta <= kink_fraction + SAME_STATION {
                root_section.clone()
            } else {
                let blend = (eta - kink_fraction) / (1.0 - kink_fraction);
                root_section.blend_with_another_airfoil(
                    tip_airfoil,
                    blend,
                    DEFAULT_MORPH_N_POINTS,
                )?
            };
            if !thickness_active {
                return Ok(base);
            }
            Ok(morph_airfoil(
                &base.coordinates,
                wing.thickness_scale_at(eta),
                1.0,
                DEFAULT_MORPH_N_POINTS,
            ))
        };
        if thickness_active {
            for xsec in xsecs.iter_mut() {
                let eta = xsec.xyz_le[1] / tip_y_m;
                xsec.airfoil = morph_airfoil(
                    &xsec.airfoil.coordinates,
                    wing.thickness_scale_at(eta),
                    1.0,
                    DEFAULT_MORPH_N_POINTS,
                );
            }
        }
        xsecs.sort_by(|left, right| left.xyz_le[1].total_cmp(&right.xyz_le[1]));
        for eta in wing.profile_span_fractions() {
            let y_m = eta * tip_y_m;
            if xsecs
                .iter()
                .any(|xsec| (xsec.xyz_le[1] / tip_y_m - eta).abs() <= SAME_STATION)
            {
                continue;
            }
            let outboard = xsecs
                .iter()
                .position(|xsec| xsec.xyz_le[1] > y_m)
                .unwrap_or(xsecs.len() - 1);
            let (a, b) = (&xsecs[outboard - 1], &xsecs[outboard]);
            let t = (y_m - a.xyz_le[1]) / (b.xyz_le[1] - a.xyz_le[1]);
            let lerp = |u: f64, v: f64| u + t * (v - u);
            let section = WingXSec::new(
                [
                    lerp(a.xyz_le[0], b.xyz_le[0]),
                    y_m,
                    heights.at(planform, y_m),
                ],
                lerp(a.chord, b.chord),
                lerp(a.twist, b.twist),
                base_at(eta)?,
            );
            xsecs.insert(outboard, section);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::{DesignVector, GeometryConfig, WingDihedralStation, WingThicknessStation};

    fn max_thickness(airfoil: &Airfoil) -> f64 {
        airfoil
            .coordinates
            .iter()
            .map(|c| c.1)
            .fold(f64::MIN, f64::max)
            - airfoil
                .coordinates
                .iter()
                .map(|c| c.1)
                .fold(f64::MAX, f64::min)
    }

    fn profiled() -> GeometryConfig {
        let mut geometry = GeometryConfig::default();
        geometry.wing.dihedral_stations = vec![WingDihedralStation {
            span_fraction: 0.2,
            z_m: -1.0,
            flight_rise_m: None,
        }];
        geometry.wing.thickness_stations = vec![
            WingThicknessStation {
                span_fraction: 0.0,
                thickness_scale: 1.15,
            },
            WingThicknessStation {
                span_fraction: 0.37,
                thickness_scale: 0.85,
            },
        ];
        geometry
    }

    #[test]
    fn stations_bend_the_height_polyline_without_moving_the_planform_or_reference_quantities() {
        let design = DesignVector::default();
        let plain = AircraftBuilder::new(Some(GeometryConfig::default()))
            .build(Some(&design), false)
            .expect("plain wing builds");
        let bent = AircraftBuilder::new(Some(profiled()))
            .build(Some(&design), false)
            .expect("profiled wing builds");
        // Planform-derived reference quantities do not depend on heights or sections.
        assert!((plain.s_ref - bent.s_ref).abs() < 1e-9 * plain.s_ref);
        assert!((plain.b_ref - bent.b_ref).abs() < 1e-9 * plain.b_ref);
        let y = 0.2 * design.span_m / 2.0;
        let station = bent.wings[0]
            .xsecs
            .iter()
            .find(|x| (x.xyz_le[1] - y).abs() < 1e-9)
            .expect("the dihedral station is a loft section");
        assert!((station.xyz_le[2] + 1.0).abs() < 1e-12);
        // Its chord and leading edge lie on the straight root-kink panel.
        let root = &plain.wings[0].xsecs[0];
        assert!(station.chord < root.chord && station.chord > 0.0);
    }

    #[test]
    fn thickness_stations_scale_the_root_and_kink_sections() {
        let design = DesignVector::default();
        let plain = AircraftBuilder::new(Some(GeometryConfig::default()))
            .build(Some(&design), false)
            .expect("plain wing builds");
        let thick = AircraftBuilder::new(Some(profiled()))
            .build(Some(&design), false)
            .expect("profiled wing builds");
        let root_ratio = max_thickness(&thick.wings[0].xsecs[0].airfoil)
            / max_thickness(&plain.wings[0].xsecs[0].airfoil);
        // The centreline section copies the side-of-body section, so it
        // carries the thickness scale of the side-of-body station: the 1.15 root and 0.85 kink
        // scales interpolated at the side-of-body span fraction.
        let eta = GeometryConfig::default()
            .wing
            .side_of_body_span_fraction
            .expect("side of body");
        let at_side_of_body = 1.15 + (0.85 - 1.15) * eta / 0.37;
        assert!((root_ratio - at_side_of_body).abs() < 5e-3, "{root_ratio}");
        let kink_y = GeometryConfig::default()
            .wing
            .transport_planform(&design)
            .expect("planform")
            .kink
            .y_m;
        let kink = |a: &crate::aircraft::airplane::Airplane| {
            max_thickness(
                &a.wings[0]
                    .xsecs
                    .iter()
                    .find(|x| (x.xyz_le[1] - kink_y).abs() < 1e-9)
                    .expect("kink section")
                    .airfoil,
            )
        };
        assert!((kink(&thick) / kink(&plain) - 0.85).abs() < 5e-3);
    }

    #[test]
    fn an_interior_station_on_the_kink_is_rejected() {
        let mut geometry = GeometryConfig::default();
        geometry.wing.dihedral_stations = vec![WingDihedralStation {
            span_fraction: geometry.wing.kink_span_fraction.expect("kink"),
            z_m: 0.0,
            flight_rise_m: None,
        }];
        assert!(matches!(
            AircraftBuilder::new(Some(geometry)).build(None, false),
            Err(BuildError::WingProfile(_))
        ));
    }

    #[test]
    fn a_tip_rake_keeps_the_trailing_edge_line_and_shrinks_the_tip_chord() {
        let design = DesignVector::default();
        let mut geometry = GeometryConfig::default();
        let plain = geometry
            .wing
            .transport_planform(&design)
            .expect("default planform");
        geometry.wing.tip_rake_tip_chord_m = Some(0.6 * plain.tip.chord_m);
        geometry.wing.tip_rake_start_span_fraction = Some(0.9);
        let raked = geometry
            .wing
            .transport_planform(&design)
            .expect("raked planform");
        let rake = raked.tip_rake.expect("rake station");
        // Leading edge: straight design sweep to the rake station, more sweep outboard.
        assert!(
            (rake.leading_edge_x_m
                - (0.9 * raked.tip.y_m - raked.kink.y_m).mul_add(
                    design.sweep_deg.to_radians().tan(),
                    raked.kink.leading_edge_x_m
                ))
            .abs()
                < 1e-9
        );
        assert!(raked.tip.leading_edge_x_m > plain.tip.leading_edge_x_m);
        // The trailing edge is one straight line kink -> rake -> tip.
        let te = |s: &alas_config::MainWingStation| s.leading_edge_x_m + s.chord_m;
        let slope_a = (te(&rake) - te(&raked.kink)) / (rake.y_m - raked.kink.y_m);
        let slope_b = (te(&raked.tip) - te(&rake)) / (raked.tip.y_m - rake.y_m);
        assert!((slope_a - slope_b).abs() < 1e-12);
        // The rake is part of the loft and of the reference area.
        let airplane = AircraftBuilder::new(Some(geometry))
            .build(Some(&design), false)
            .expect("raked wing builds");
        assert!(airplane.wings[0]
            .xsecs
            .iter()
            .any(|x| (x.xyz_le[1] - rake.y_m).abs() < 1e-9));
        assert!((airplane.s_ref - raked.reference_area_m2()).abs() < 1e-6 * airplane.s_ref);
    }

    #[test]
    fn a_kink_station_carries_only_the_flight_rise_of_the_kink() {
        use alas_config::WingShape;
        let design = DesignVector::default();
        let mut geometry = GeometryConfig::default();
        let kink = geometry.wing.kink_span_fraction.expect("kink");
        geometry.wing.flight_tip_rise_semispan_fraction = Some(0.05);
        geometry.wing.dihedral_stations = vec![WingDihedralStation {
            span_fraction: kink,
            z_m: geometry.wing.break_z_m,
            flight_rise_m: Some(0.5),
        }];
        let planform = geometry.wing.transport_planform(&design).expect("planform");
        let ground = geometry
            .wing
            .heights(WingShape::Ground, &planform)
            .expect("ground heights");
        let flight = geometry
            .wing
            .heights(WingShape::Flight, &planform)
            .expect("flight heights");
        assert!((ground.break_z_m - geometry.wing.break_z_m).abs() < 1e-12);
        assert!((flight.break_z_m - ground.break_z_m - 0.5).abs() < 1e-12);
        assert!((flight.tip_z_m - ground.tip_z_m - 0.05 * planform.tip.y_m).abs() < 1e-12);
    }

    #[test]
    fn a_station_next_to_an_existing_section_is_rejected_as_a_sliver() {
        let mut geometry = GeometryConfig::default();
        let kink = geometry.wing.kink_span_fraction.expect("kink");
        geometry.wing.dihedral_stations = vec![WingDihedralStation {
            span_fraction: kink - 0.01,
            z_m: -0.5,
            flight_rise_m: None,
        }];
        assert!(matches!(
            AircraftBuilder::new(Some(geometry)).build(None, false),
            Err(BuildError::WingProfile(_))
        ));
    }

    /// The vortex lattice cannot condition a sliver panel (a 0.27 m panel at the A320 side of
    /// body gave a pivot ratio above 1e5), so no preset may loft two sections closer than 0.5 %
    /// of the semispan.
    #[test]
    fn no_preset_wing_has_a_sliver_panel() {
        for preset in alas_config::presets::registry() {
            let airplane = AircraftBuilder::new(Some(preset.geometry.clone()))
                .build(Some(&preset.design_vector), false)
                .unwrap_or_else(|error| panic!("{}: {error}", preset.name));
            let wing = &airplane.wings[0];
            // The extents the mesh allocator uses (lofted length of each section), against the
            // allocator's own floor: the same constant is the single source of truth.
            let extent = |pair: &[crate::aircraft::wing::WingXSec]| {
                (pair[1].xyz_le[1] - pair[0].xyz_le[1]).hypot(pair[1].xyz_le[2] - pair[0].xyz_le[2])
            };
            let total: f64 = wing.xsecs.windows(2).map(extent).sum();
            for pair in wing.xsecs.windows(2) {
                let gap = extent(pair) / total;
                assert!(
                    gap > crate::aircraft::spanwise::MIN_PANEL_EXTENT_FRACTION - 1.0e-9,
                    "{}: panel of {gap} of the lofted extent",
                    preset.name
                );
            }
        }
    }
}
