// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Warning when the belly clearance of a turboprop outside its sample's layout
//! is an assumption. Warning only: the design stays feasible under it.

use alas_config::AlasConfig;
use alas_mass::stations::{clearance_sample, ground_plane_bases, ClearanceSample};

use super::{warning, FindingCode, PhysicalFinding};
use alas_geom::aircraft::airplane::Airplane;

/// Append the warning when no belly clearance is declared and the turboprop is
/// not in the sponson-gear layout of its two-aircraft sample (wing root above
/// the crown, main wheels inboard of the engines): the ground plane is then
/// judged at both ends of the full documented sample, 0.216-0.546 fuselage
/// diameters, which is an assumption a declared
/// `fuselage_ground_clearance_m` replaces.
pub(super) fn append(config: &AlasConfig, plane: &Airplane, findings: &mut Vec<PhysicalFinding>) {
    let (Some(wing), Some(fuselage)) = (plane.wings.first(), plane.fuselages.first()) else {
        return;
    };
    let sample = clearance_sample(wing, fuselage, &config.geometry, &config.landing_gear);
    if sample == ClearanceSample::OtherTurboprop
        && ground_plane_bases(&config.landing_gear).len() > 1
    {
        findings.push(warning(
            FindingCode::GroundClearanceSampleAssumed,
            "no belly clearance is declared and this turboprop is outside the layout of its \
             two-aircraft sample (wing root above the crown, main wheels inboard of the \
             engines): the ground plane is judged at both ends of the full documented sample \
             (0.216-0.546 fuselage diameters); declare fuselage_ground_clearance_m to replace it"
                .to_owned(),
            None,
            None,
            "",
        ));
    }
}

#[cfg(test)]
// A test unwrap is the assertion failing on a registered preset.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use alas_geom::builder::AircraftBuilder;

    fn case(name: &str, declared: bool, low_wing: bool) -> Vec<PhysicalFinding> {
        let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name })).unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let mut plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), true)
            .unwrap();
        if !declared {
            config.landing_gear.fuselage_ground_clearance_m = None;
        }
        if low_wing {
            let belly = plane.fuselages[0]
                .xsecs
                .iter()
                .map(|x| x.xyz_c[2] - x.height / 2.0)
                .fold(f64::INFINITY, f64::min);
            for xsec in &mut plane.wings[0].xsecs {
                xsec.xyz_le[2] = belly;
            }
            // The layout reads the design root (`Wing::design_root`), which
            // is the planform datum's when the wing carries one, not the
            // lofted root section: lower it with the sections.
            if let Some(datum) = &mut plane.wings[0].planform_datum {
                datum.root_le_m[2] = belly;
            }
        }
        let mut findings = Vec::new();
        append(&config, &plane, &mut findings);
        findings
    }

    #[test]
    fn only_an_undeclared_turboprop_outside_its_layout_is_flagged() {
        let flagged = case("ATR72-600", false, true);
        assert_eq!(flagged.len(), 1);
        assert_eq!(flagged[0].code, FindingCode::GroundClearanceSampleAssumed);
        assert_eq!(flagged[0].severity, crate::FindingSeverity::Warning);
        // Declared, in its own sponson layout, or a jet: nothing to flag.
        assert!(case("ATR72-600", true, true).is_empty());
        assert!(case("ATR72-600", false, false).is_empty());
        assert!(case("A320-200", false, false).is_empty());
    }
}
