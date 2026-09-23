// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What the validity-domain quantities actually measure on every registered
//! aircraft, from the built geometry rather than from the design vector.
//!
//! Dispatch-local probe. The plausibility limits are windows on the model's
//! own quantities, so they have to be set from what the model produces for a
//! real aeroplane, not from the published dimensions of that aeroplane. This
//! prints both, so a disagreement between them is visible as a geometry-model
//! finding instead of being absorbed into a widened limit.
//!
//! Lengths m, areas m^2, ratios dimensionless.

// A diagnostic example: its output is the printed report, and a failed
// unwrap is the probe stopping on an input it cannot run.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use alas_config::AlasConfig;
use alas_geom::builder::AircraftBuilder;

fn main() {
    println!(
        "{:<12} {:>8} {:>8} {:>7} {:>9} {:>9} {:>8} {:>8} {:>8} {:>8}",
        "preset",
        "b_ref",
        "S_ref",
        "AR",
        "L_fus",
        "d_equiv",
        "finenes",
        "tailarm",
        "tip/rt",
        "t/c_rt"
    );
    for preset in alas_config::presets::available() {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
            .unwrap_or_else(|error| panic!("{preset}: {error}"));
        let design = alas_config::presets::get(preset)
            .expect("registered preset")
            .design_vector;
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), false)
            .unwrap_or_else(|error| panic!("{preset}: {error}"));

        let aspect_ratio = plane.b_ref.powi(2) / plane.s_ref.max(1e-9);
        let (length, diameter) = match plane.fuselages.first() {
            Some(fuselage) => {
                let first = fuselage.xsecs.first().map(|s| s.xyz_c[0]).unwrap_or(0.0);
                let last = fuselage.xsecs.last().map(|s| s.xyz_c[0]).unwrap_or(0.0);
                let diameter = fuselage
                    .xsecs
                    .iter()
                    .map(|section| (section.width * section.height).max(0.0).sqrt())
                    .fold(0.0_f64, f64::max);
                (last - first, diameter)
            }
            None => (f64::NAN, f64::NAN),
        };
        let tail_arm = match (plane.wings.first(), plane.wings.get(1)) {
            (Some(wing), Some(hstab)) => {
                (hstab.aerodynamic_center(0.25)[0] - wing.aerodynamic_center(0.25)[0]) / length
            }
            _ => f64::NAN,
        };
        let root_thickness = plane
            .wings
            .first()
            .and_then(|wing| wing.xsecs.first())
            .map(|section| {
                section
                    .airfoil
                    .max_thickness(&alas_geom::aircraft::spacing::linspace(0.0, 1.0, 101))
            })
            .unwrap_or(f64::NAN);

        println!(
            "{:<12} {:>8.2} {:>8.1} {:>7.2} {:>9.2} {:>9.3} {:>8.2} {:>8.3} {:>8.3} {:>8.4}   (config diameter_m {:.3}, design L_fus {:.2}, published-style fineness {:.2})",
            preset,
            plane.b_ref,
            plane.s_ref,
            aspect_ratio,
            length,
            diameter,
            length / diameter,
            tail_arm,
            design.tip_chord_m / design.root_chord_m,
            root_thickness,
            config.geometry.fuselage.diameter_m,
            design.fuselage_length_m,
            design.fuselage_length_m / config.geometry.fuselage.diameter_m,
        );
    }

    // The same quantities as the sizing path sees them, read back from the
    // residual table. A disagreement with the table above is introduced by the
    // candidate cabin load case, not by how the quantity is measured.
    println!();
    println!("as the sizing path sees them (residual `actual` values):");
    for preset in alas_config::presets::available() {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
            .unwrap_or_else(|error| panic!("{preset}: {error}"));
        let design = alas_config::presets::get(preset)
            .expect("registered preset")
            .design_vector;
        match alas_opt::assess_product_candidate(&config, &design) {
            Ok(assessment) => {
                let read = |id: &str| {
                    assessment
                        .residuals
                        .iter()
                        .find(|residual| residual.id == id)
                        .map(|residual| residual.actual)
                        .unwrap_or(f64::NAN)
                };
                println!(
                    "{:<12} AR {:>6.2}  fineness {:>6.2}  tail_arm {:>6.3}  tip/root {:>6.3}  t/c_root {:>7.4}",
                    preset,
                    read("aspect_ratio_max"),
                    read("fuselage_fineness_max"),
                    read("tail_arm_fraction_max"),
                    read("tip_root_chord_ratio_max"),
                    read("root_thickness_ratio_max"),
                );
            }
            Err(reason) => println!("{preset:<12} not sizeable: {reason}"),
        }
    }
}
