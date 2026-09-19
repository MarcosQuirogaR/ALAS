// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::{AircraftBuilder, BuildError};
use crate::aircraft::fuselage::FuselageXSec;
use crate::aircraft::wing::WingXSec;
use alas_config::{FuselageSectionError, TransportPlanform};

impl AircraftBuilder {
    /// Append validated user-controlled wing sections before the common mesh
    /// builder interpolates them into the surrounding planform.
    pub(super) fn append_custom_wing_sections(
        &self,
        planform: &TransportPlanform,
        xsecs: &mut Vec<WingXSec>,
    ) -> Result<(), BuildError> {
        let wing = &self.geometry.wing;
        wing.validate_custom_sections_against_planform(planform)?;
        for section in wing.custom_sections_sorted()? {
            let airfoil = Self::resolve(&section.airfoil)?;
            xsecs.push(WingXSec::new(
                [
                    section.leading_edge_x_m,
                    section.span_fraction * planform.tip.y_m,
                    section.z_m,
                ],
                section.chord_m,
                section.twist_deg,
                airfoil,
            ));
        }
        xsecs.sort_by(|left, right| left.xyz_le[1].total_cmp(&right.xyz_le[1]));
        Ok(())
    }

    /// Append validated user-controlled fuselage sections and retain the
    /// nose-to-tail station ordering required by the loft and wetted-area
    /// integration.
    pub(super) fn append_custom_fuselage_sections(
        &self,
        fuselage_length_m: f64,
        stations: &mut Vec<FuselageXSec>,
    ) -> Result<(), BuildError> {
        let fuselage = &self.geometry.fuselage;
        for section in fuselage.custom_sections_sorted()? {
            let x_m = section.x_fraction * fuselage_length_m;
            if stations.iter().any(|station| {
                (station.xyz_c[0] - x_m).abs() <= 1.0e-9 * fuselage_length_m.abs().max(1.0)
            }) {
                return Err(FuselageSectionError::DuplicateGeneratedStation {
                    x_fraction: section.x_fraction,
                }
                .into());
            }
            stations.push(FuselageXSec::new(
                [x_m, 0.0, section.z_m],
                None,
                Some(section.width_m),
                Some(section.height_m),
                section.shape,
            )?);
        }
        stations.sort_by(|left, right| left.xyz_c[0].total_cmp(&right.xyz_c[0]));
        Ok(())
    }
}

#[cfg(test)]
mod custom_section_tests {
    use super::*;
    use alas_config::{DesignVector, GeometryConfig};

    #[test]
    fn a_custom_wing_section_is_retained_by_the_loft() {
        let mut geometry = GeometryConfig::default();
        let design = DesignVector::default();
        let planform = geometry
            .wing
            .transport_planform(&design)
            .expect("default planform is valid");
        geometry
            .wing
            .custom_sections
            .push(alas_config::WingSection {
                span_fraction: 0.20,
                leading_edge_x_m: 0.40,
                chord_m: (planform.root.chord_m + planform.kink.chord_m) / 2.0,
                z_m: -1.0,
                twist_deg: 2.5,
                airfoil: "naca2410".to_owned(),
            });
        let airplane = AircraftBuilder::new(Some(geometry))
            .build(Some(&design), false)
            .expect("valid custom wing section builds");
        let section = airplane.wings[0]
            .xsecs
            .iter()
            .find(|section| (section.xyz_le[1] - 0.20 * design.span_m / 2.0).abs() < 1.0e-12)
            .expect("custom section remains a mesh edge");
        assert!(
            (section.chord - (planform.root.chord_m + planform.kink.chord_m) / 2.0).abs() < 1.0e-12
        );
        assert_eq!(section.airfoil.name, "naca2410");
    }

    #[test]
    fn an_invalid_custom_wing_section_is_a_typed_build_error() {
        let mut geometry = GeometryConfig::default();
        geometry
            .wing
            .custom_sections
            .push(alas_config::WingSection {
                span_fraction: 1.0,
                leading_edge_x_m: 0.0,
                chord_m: 1.0,
                z_m: 0.0,
                twist_deg: 0.0,
                airfoil: "naca2410".to_owned(),
            });
        assert!(matches!(
            AircraftBuilder::new(Some(geometry)).build(None, false),
            Err(BuildError::WingSection(_))
        ));
    }

    #[test]
    fn a_custom_fuselage_section_is_sorted_into_the_body_loft() {
        let mut geometry = GeometryConfig::default();
        geometry
            .fuselage
            .custom_sections
            .push(alas_config::FuselageSection {
                x_fraction: 0.55,
                width_m: 6.0,
                height_m: 5.8,
                z_m: 0.4,
                shape: 2.0,
            });
        let design = DesignVector::default();
        let airplane = AircraftBuilder::new(Some(geometry))
            .build(Some(&design), false)
            .expect("valid custom fuselage section builds");
        let section = airplane.fuselages[0]
            .xsecs
            .iter()
            .find(|section| (section.xyz_c[0] - 0.55 * design.fuselage_length_m).abs() < 1.0e-12)
            .expect("custom fuselage section remains in the loft");
        assert_eq!(section.width, 6.0);
        assert_eq!(section.height, 5.8);
        assert!(airplane.fuselages[0]
            .xsecs
            .windows(2)
            .all(|pair| pair[1].xyz_c[0] > pair[0].xyz_c[0]));
    }

    #[test]
    fn an_invalid_custom_fuselage_shape_is_a_typed_build_error() {
        let mut geometry = GeometryConfig::default();
        geometry
            .fuselage
            .custom_sections
            .push(alas_config::FuselageSection {
                x_fraction: 0.55,
                width_m: 6.0,
                height_m: 5.8,
                z_m: 0.4,
                shape: 0.5,
            });
        assert!(matches!(
            AircraftBuilder::new(Some(geometry)).build(None, false),
            Err(BuildError::FuselageSection(_))
        ));
    }
}
