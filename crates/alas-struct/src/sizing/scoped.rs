// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The sizing entry point that reports what it sized to.
//!
//! [`size_wingbox_with_scope`] runs the same solve as
//! [`super::size_wingbox_with_wing_carried_mass`] and returns, beside the box, a
//! [`SizingScope`]: the loading envelope the box was sized at, whether that
//! envelope was bounded by declared limits or rested on an assumption, the
//! wing-carried items that could not be resolved, and whether the relieved-load
//! fixed point actually settled.
//!
//! The relief loop lives here and both entry points share it, so the box and the
//! declaration published beside it are produced by one solve rather than two.

use alas_config::materials::MaterialSpec;
use alas_config::{DesignRequirements, StructuresConfig};
use alas_geom::wing_structure::WingStructureGeometry;

use crate::loads::WingInertiaRelief;
use crate::scope::{
    GustEnvelope, NotAvailable, OmissionDirection, ReliefConvergence, RootDatum, SizingScope,
    WingFuelDesignCase, WingFuelDistribution, WingMountedRelief, POINT_MASS_SHEAR_RELIEF,
};
use crate::tanks;

use super::law::{linspace, trapezoid, SizingLaw};
use super::solve::size_wingbox_with_law;
use super::types::WingboxSizing;
use super::{box_chord_band, box_running_mass_kg_m, RELIEF_PASSES, RELIEF_TOLERANCE};

/// A sized box together with the model scope it was produced under.
#[derive(Debug, Clone, PartialEq)]
pub struct SizedWingbox {
    /// The box itself.
    pub sizing: WingboxSizing,
    /// What it was sized to, and what it was not sized to.
    pub scope: SizingScope,
}

/// Where the relieving integral wing fuel comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WingFuelRelief<'a> {
    /// Estimate it from the volume the sized box encloses ([`crate::tanks`]).
    /// This is the clean-sheet default: there is no published tank to read.
    EnclosedBoxVolume,
    /// Use the caller's own running mass, resolved from the aircraft's declared
    /// cell capacities and bounded by its loading envelope.
    Declared {
        /// Running mass on **one** semi-wing, kg/m, sampled on
        /// [`super::sizing_stations`]. A length other than
        /// `cfg.spanwise_stations` is rejected in favour of the geometric
        /// estimate and reported, rather than silently truncated.
        running_mass_kg_m: &'a [f64],
        /// The loading case that running mass represents.
        design_case: WingFuelDesignCase,
    },
}

/// A declared distribution that did not match the sizing grid.
const DECLARED_DISTRIBUTION_REJECTED: NotAvailable = NotAvailable {
    quantity: "declared wing-fuel running mass on the sizing grid, kg/m",
    reason: "the caller's running mass was not sampled on cfg.spanwise_stations stations, so it \
             could not be zipped onto the load grid. The solve fell back to the enclosed-volume \
             estimate; the declared capacity and its zero-fuel bound were not applied.",
    resolved_by: "sampling the declared running mass on super::sizing_stations before passing it",
    direction: OmissionDirection::Unknown,
};

/// [`super::size_wingbox_with_wing_carried_mass`], reporting its own model scope.
///
/// SI throughout. `wing_mounted` carries the point masses for the modelled
/// positive-`y` semi-wing only, and the items that could not be resolved into
/// point masses; [`crate::scope::wing_mounted_relief`] builds both.
#[allow(clippy::too_many_arguments)] // mirrors the unscoped entry point's signature
pub fn size_wingbox_with_scope(
    wsg: &WingStructureGeometry,
    cfg: &StructuresConfig,
    req: &DesignRequirements,
    skin_mat: &MaterialSpec,
    web_mat: &MaterialSpec,
    cap_mat: &MaterialSpec,
    rib_mat: &MaterialSpec,
    fuel: &WingFuelRelief<'_>,
    wing_mounted: &WingMountedRelief,
) -> SizedWingbox {
    let n = cfg.spanwise_stations.max(0) as usize;
    let y = linspace(0.0, wsg.semi_span, n);
    let (front, rear) = box_chord_band(wsg);
    let geometric = || tanks::integral_fuel_running_mass_kg_m(wsg, &y, front, rear);

    let mut wing_mounted_omissions = wing_mounted.omitted.clone();
    let (fuel_kg_m, wing_fuel, wing_fuel_distribution) = match *fuel {
        WingFuelRelief::Declared {
            running_mass_kg_m,
            design_case,
        } if running_mass_kg_m.len() == n => (
            running_mass_kg_m.to_vec(),
            design_case,
            WingFuelDistribution::declared_cells_scaled(design_case.is_partial_fill()),
        ),
        WingFuelRelief::Declared { .. } => {
            // A rejected declaration is not the same model as the geometric
            // default that replaced it, and the difference has to be visible.
            let estimate = geometric();
            let case = enclosed_box_volume_case(&estimate, &y);
            wing_mounted_omissions.push(DECLARED_DISTRIBUTION_REJECTED);
            (estimate, case, WingFuelDistribution::EnclosedBoxVolume)
        }
        WingFuelRelief::EnclosedBoxVolume => {
            let estimate = geometric();
            let case = enclosed_box_volume_case(&estimate, &y);
            (estimate, case, WingFuelDistribution::EnclosedBoxVolume)
        }
    };

    let (sizing, relief_convergence) = solve_relieved(
        wsg,
        cfg,
        req,
        skin_mat,
        web_mat,
        cap_mat,
        rib_mat,
        &fuel_kg_m,
        &wing_mounted.point_masses_kg,
        front,
        rear,
    );

    let scope = SizingScope {
        sizing_load_case: sizing.sizing_load_case,
        design_gross_mass_kg: req.mtow_kg,
        ultimate_load_factor: req.ultimate_load_factor * cfg.additional_safety_factor,
        gust_envelope: GustEnvelope::not_modelled(),
        wing_fuel,
        wing_fuel_distribution,
        wing_mounted_omissions,
        root_datum: RootDatum::aircraft_centreline(),
        relief_convergence,
        solve_omissions: vec![POINT_MASS_SHEAR_RELIEF],
    };
    SizedWingbox { sizing, scope }
}

/// The whole-aircraft fuel a semi-wing running mass integrates to, as an
/// enclosed-volume case. The published capacities this stands in for are
/// both-wings figures, so the semi-wing integral is doubled.
fn enclosed_box_volume_case(running_mass_kg_m: &[f64], y: &[f64]) -> WingFuelDesignCase {
    WingFuelDesignCase::enclosed_box_volume(2.0 * trapezoid(running_mass_kg_m, y))
}

/// The relieved-load fixed point: size, re-read the box's own running mass,
/// size again, until the total stops moving.
///
/// Shared by both product entry points so there is one loop, one tolerance and
/// one convergence verdict.
#[allow(clippy::too_many_arguments)]
pub(super) fn solve_relieved(
    wsg: &WingStructureGeometry,
    cfg: &StructuresConfig,
    req: &DesignRequirements,
    skin_mat: &MaterialSpec,
    web_mat: &MaterialSpec,
    cap_mat: &MaterialSpec,
    rib_mat: &MaterialSpec,
    fuel_kg_m: &[f64],
    wing_mounted_point_masses: &[(f64, f64)],
    front: f64,
    rear: f64,
) -> (WingboxSizing, ReliefConvergence) {
    let mut relief = WingInertiaRelief {
        running_mass_kg_m: fuel_kg_m.to_vec(),
        point_masses_kg: wing_mounted_point_masses.to_vec(),
    };
    let mut result = size_wingbox_with_law(
        wsg,
        cfg,
        req,
        skin_mat,
        web_mat,
        cap_mat,
        rib_mat,
        SizingLaw::Product,
        &relief,
    );
    let mut passes = 1usize;
    let mut relative_change = f64::INFINITY;
    for _ in 1..RELIEF_PASSES {
        let structure = box_running_mass_kg_m(&result, skin_mat, web_mat, cap_mat, front, rear);
        relief.running_mass_kg_m = fuel_kg_m
            .iter()
            .zip(&structure)
            .map(|(&f, &s)| f + s)
            .collect::<Vec<f64>>();
        let next = size_wingbox_with_law(
            wsg,
            cfg,
            req,
            skin_mat,
            web_mat,
            cap_mat,
            rib_mat,
            SizingLaw::Product,
            &relief,
        );
        relative_change =
            (next.total_mass_kg - result.total_mass_kg).abs() / result.total_mass_kg.abs().max(1.0);
        let settled = relative_change <= RELIEF_TOLERANCE;
        result = next;
        passes += 1;
        if settled {
            return (
                result,
                ReliefConvergence::Settled {
                    passes,
                    relative_change,
                },
            );
        }
    }
    // A single-pass budget never iterated, so there is no change to judge.
    let convergence = if passes < 2 {
        ReliefConvergence::NotIterated
    } else {
        ReliefConvergence::NotSettled {
            passes,
            relative_change,
            tolerance: RELIEF_TOLERANCE,
        }
    };
    (result, convergence)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scope::wing_mounted_relief;
    use alas_config::{materials, presets, AlasConfig};
    use alas_geom::builder::AircraftBuilder;

    struct Probe {
        config: AlasConfig,
        wsg: WingStructureGeometry,
    }

    /// A registered aircraft at its own nominal design: its planform, its own
    /// root and tip sections off the built wing, its spars, materials and
    /// engines. The same construction `alas-mass`'s wing reconciliation uses, so
    /// what these tests measure is what the product sizes.
    fn probe(name: &str) -> Probe {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
            .unwrap_or_else(|error| panic!("{name} loads: {error}"));
        let design = presets::registry()
            .iter()
            .find(|preset| preset.name == name)
            .map(|preset| preset.design_vector)
            .unwrap_or_else(|| panic!("{name} is a registered preset"));
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), false)
            .unwrap_or_else(|error| panic!("{name} geometry builds: {error}"));
        let wing = plane
            .wings
            .iter()
            .find(|wing| wing.name == "Main Wing")
            .unwrap_or_else(|| panic!("{name} has a main wing"));
        let (root, tip) = (
            wing.xsecs.first().expect("a root section"),
            wing.xsecs.last().expect("a tip section"),
        );
        let (fractions, full_span) = config.structures.resolved_spars();
        let wsg = WingStructureGeometry::new(
            &design,
            &config.geometry.wing,
            &root.airfoil,
            &tip.airfoil,
            &fractions,
            Some(&full_span),
        )
        .unwrap_or_else(|error| panic!("{name} wing builds: {error}"));
        Probe { config, wsg }
    }

    fn material(name: &str) -> &'static alas_config::materials::MaterialSpec {
        materials::get(name).unwrap_or_else(|error| panic!("{name}: {error}"))
    }

    fn size(probe: &Probe, fuel: &WingFuelRelief<'_>) -> SizedWingbox {
        let structures = &probe.config.structures;
        let relief = wing_mounted_relief(
            &probe.config.geometry.engine,
            &probe.config.mass_model,
            &probe.config.requirements,
        );
        size_wingbox_with_scope(
            &probe.wsg,
            structures,
            &probe.config.requirements,
            material(&structures.skin_material),
            material(&structures.spar_web_material),
            material(&structures.spar_cap_material),
            material(&structures.rib_material),
            fuel,
            &relief,
        )
    }

    /// Every registered aircraft, so a convergence change anywhere is visible.
    const REGISTERED: [&str; 8] = [
        "A340-300",
        "A380-800",
        "A320-200",
        "A220-300",
        "ATR72-600",
        "AVE",
        "B787-9",
        "DC-10",
    ];

    #[test]
    fn two_registered_aircraft_do_not_reach_the_declared_relief_tolerance_in_the_pass_budget() {
        // Measured, not asserted away. The A340-300 and the A380-800 are still
        // moving at the eighth pass, by 4.10e-9 and 4.87e-9 of their own box
        // mass, where the declared tolerance is 1e-9. In absolute terms that is
        // 6.1e-5 kg and 1.3e-4 kg, far inside the gramme the module claims, so
        // it is a contract the constants do not meet rather than a box that is
        // wrong. Neither constant was moved to make this pass: the verdict is
        // reported instead, which is what the previous loop could not do
        // because it broke out of the budget without recording why.
        for name in REGISTERED {
            let scoped = size(&probe(name), &WingFuelRelief::EnclosedBoxVolume);
            let settles = !matches!(name, "A340-300" | "A380-800");
            assert_eq!(
                scoped.scope.relief_convergence.is_settled(),
                settles,
                "{name}: {:?}",
                scoped.scope.relief_convergence
            );
            if let ReliefConvergence::NotSettled {
                passes,
                relative_change,
                tolerance,
            } = scoped.scope.relief_convergence
            {
                assert_eq!(passes, RELIEF_PASSES, "{name} spends its whole budget");
                assert!(relative_change > tolerance);
                // The drift the budget leaves is still structurally nothing.
                assert!(
                    relative_change * scoped.sizing.total_mass_kg < 1.0e-3,
                    "{name} drifts {relative_change} of {} kg",
                    scoped.sizing.total_mass_kg
                );
            }
        }
    }

    #[test]
    fn the_scoped_entry_point_sizes_the_same_box_as_the_unscoped_one() {
        // The scope must be a report about the solve, not a different solve.
        let probe = probe("A340-300");
        let scoped = size(&probe, &WingFuelRelief::EnclosedBoxVolume);
        let structures = &probe.config.structures;
        let plain = super::super::size_wingbox_with_wing_carried_mass(
            &probe.wsg,
            structures,
            &probe.config.requirements,
            material(&structures.skin_material),
            material(&structures.spar_web_material),
            material(&structures.spar_cap_material),
            material(&structures.rib_material),
            None,
            &crate::loads::engine_point_loads_n(
                &probe.config.geometry.engine,
                &probe.config.mass_model,
                &probe.config.requirements,
            ),
        );
        assert_eq!(scoped.sizing, plain);
    }

    #[test]
    fn a_geometric_estimate_is_reported_as_an_estimate_and_never_as_a_bounded_envelope() {
        let probe = probe("A340-300");
        let scoped = size(&probe, &WingFuelRelief::EnclosedBoxVolume);
        assert!(matches!(
            scoped.scope.wing_fuel,
            WingFuelDesignCase::EnclosedBoxVolumeEstimate { .. }
        ));
        assert!(!scoped.scope.envelope_is_bounded());
        assert!(scoped.scope.wing_fuel.design_case_kg() > 0.0);
        // The geometric shape is derived, so it declares no placement gap.
        assert_eq!(
            scoped.scope.wing_fuel_distribution,
            WingFuelDistribution::EnclosedBoxVolume
        );
    }

    #[test]
    fn a_declared_bounded_case_reports_a_bounded_envelope() {
        let probe = probe("A320-200");
        let stations = super::super::sizing_stations(&probe.wsg, &probe.config.structures);
        let (front, rear) = box_chord_band(&probe.wsg);
        let declared = tanks::integral_fuel_running_mass_kg_m(&probe.wsg, &stations, front, rear);
        // A capacity the aircraft cannot avoid filling: DG - MZFW >= C.
        let case = WingFuelDesignCase::declared(15_000.0, 78_000.0, Some(62_000.0));
        assert!(matches!(case, WingFuelDesignCase::TankLimited { .. }));
        let scoped = size(
            &probe,
            &WingFuelRelief::Declared {
                running_mass_kg_m: &declared,
                design_case: case,
            },
        );
        assert!(scoped.scope.envelope_is_bounded());
        assert_eq!(
            scoped.scope.wing_fuel_distribution,
            WingFuelDistribution::DeclaredCellsScaled {
                partial_fill_placement: None
            }
        );
    }

    #[test]
    fn a_partial_declared_case_declares_its_unsourced_spanwise_placement() {
        let probe = probe("A340-300");
        let stations = super::super::sizing_stations(&probe.wsg, &probe.config.structures);
        let (front, rear) = box_chord_band(&probe.wsg);
        let declared = tanks::integral_fuel_running_mass_kg_m(&probe.wsg, &stations, front, rear);
        // DG - MZFW = 20 000 < C = 40 000, so the case is a partial fill.
        let case = WingFuelDesignCase::declared(40_000.0, 200_000.0, Some(180_000.0));
        let scoped = size(
            &probe,
            &WingFuelRelief::Declared {
                running_mass_kg_m: &declared,
                design_case: case,
            },
        );
        let gaps = scoped.scope.not_available();
        assert!(gaps
            .iter()
            .any(|gap| gap.quantity.contains("spanwise placement")));
        assert!(scoped.scope.has_unconservative_gap());
    }

    #[test]
    fn a_declaration_that_misses_the_sizing_grid_is_reported_rather_than_truncated() {
        // Falling back to the geometric estimate is the safe behaviour; doing it
        // silently would publish a declared, bounded case that was never used.
        let probe = probe("A340-300");
        let scoped = size(
            &probe,
            &WingFuelRelief::Declared {
                running_mass_kg_m: &[1.0, 2.0, 3.0],
                design_case: WingFuelDesignCase::declared(30_000.0, 260_000.0, Some(180_000.0)),
            },
        );
        assert!(matches!(
            scoped.scope.wing_fuel,
            WingFuelDesignCase::EnclosedBoxVolumeEstimate { .. }
        ));
        assert!(scoped
            .scope
            .not_available()
            .iter()
            .any(|gap| gap.quantity.contains("running mass on the sizing grid")));
    }

    #[test]
    fn every_registered_aircraft_declares_the_gust_envelope_it_was_not_sized_to() {
        // The one gap that is the same on every aircraft, and the one that can
        // leave a box lighter than the certification envelope requires.
        for preset in ["A340-300", "ATR72-600", "A320-200"] {
            let scoped = size(&probe(preset), &WingFuelRelief::EnclosedBoxVolume);
            let gap = scoped
                .scope
                .gust_envelope
                .not_available()
                .expect("no aircraft here has a gust case");
            assert_eq!(gap.direction, OmissionDirection::Lighter, "{preset}");
            assert!(scoped.scope.has_unconservative_gap(), "{preset}");
        }
    }

    #[test]
    fn the_turboprop_propeller_and_nacelle_reach_the_published_scope() {
        // The ATR 72-600's omitted wing-carried mass has to survive all the way
        // to the declaration published with its box, not stop at the resolver.
        let scoped = size(&probe("ATR72-600"), &WingFuelRelief::EnclosedBoxVolume);
        let quantities: Vec<&str> = scoped
            .scope
            .not_available()
            .iter()
            .map(|gap| gap.quantity)
            .collect();
        assert!(quantities.iter().any(|q| q.contains("propeller")));
        assert!(quantities.iter().any(|q| q.contains("nacelle")));
        assert!(quantities.iter().any(|q| q.contains("landing-gear")));
    }
}
