// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The wing-carried masses the structural sizing is relieved by, resolved from
//! the aircraft's own declarations rather than estimated from its box.
//!
//! [`alas_struct::sizing::size_wingbox`] can only see the wing it is handed, so
//! its default relief estimates the integral fuel from the geometric volume of
//! the spar box. That is the right model for a clean-sheet wing and a poor one
//! for a registered aircraft, because how much of an aeroplane's fuel sits in
//! the wing rather than in a centre or auxiliary tank is not a property of the
//! box. Measured across the registered fleet the geometric estimate lands
//! between `0.80` and `2.24` times the declared integral wing capacity.
//!
//! This module is the seam that hands the sizer what the aircraft actually
//! declares: the published usable volume of each integral wing cell, and the
//! wing-mounted powerplant.
//!
//! # The declared capacity is a tank, not a load case
//!
//! A published capacity says how much fuel the wing *can* hold. The structural
//! design case asks how little it is *guaranteed* to hold while the aircraft is
//! at its design gross mass, because that is the loading that leaves the most
//! mass to be carried across the root. The two differ whenever the aircraft can
//! reach its design gross mass without filling its wings, and
//! [`alas_struct::loads::design_case_wing_fuel_kg`] carries that bound: the
//! zero-fuel limit `MZFW` is what makes `DG − MZFW` the least fuel on board, and
//! `min(C, DG − MZFW)` the least of it that can be in the wings. Applying the
//! full capacity regardless credited the ATR 72-600 with 5 065.2 kg of relief
//! where its envelope guarantees 2 000 kg, and the A380-800 with 236 488 kg
//! where it guarantees 199 000 kg.

use alas_config::{AlasConfig, DesignRequirements};
use alas_geom::wing_structure::WingStructureGeometry;
use alas_struct::tanks;

/// The declared integral wing fuel resolved as a structural load case.
///
/// Masses are kilogrammes. [`Self::capacity_kg`] and [`Self::design_case_kg`]
/// are whole-aircraft figures, as the published capacities are; the running mass
/// is one semi-wing's, because that is the extent the sizing grid covers.
#[derive(Debug, Clone, PartialEq)]
pub struct DeclaredWingFuelCase {
    /// Running mass of the design-case wing fuel on **one** semi-wing, kg/m,
    /// sampled on the caller's station grid.
    pub running_mass_kg_m: Vec<f64>,
    /// Declared usable integral wing-tank capacity, both wings, kg.
    pub capacity_kg: f64,
    /// Fuel the loading envelope guarantees in the wings at the design gross
    /// mass, both wings, kg. Equal to [`Self::capacity_kg`] when the wings
    /// cannot be avoided, and less when they can.
    pub design_case_kg: f64,
    /// The design gross mass the structural case is applied at, kg — the same
    /// mass [`alas_struct::loads::load_cases`] reads.
    pub design_gross_mass_kg: f64,
    /// The declared maximum zero-fuel mass the bound was taken against, kg.
    ///
    /// `None` when the configuration names no registered aircraft, or the
    /// registered aircraft publishes no zero-fuel limit. The case is then the
    /// full capacity, which is an **assumption about the loading** and not a
    /// bounded load case; a caller that reports the sized box has to say so.
    pub max_zero_fuel_mass_kg: Option<f64>,
}

impl DeclaredWingFuelCase {
    /// Whether the zero-fuel limit, rather than the tank, sets the relief.
    pub fn zero_fuel_limited(&self) -> bool {
        self.design_case_kg < self.capacity_kg
    }

    /// Whether the case rests on an unbounded full-tank assumption because no
    /// zero-fuel limit is declared.
    pub fn assumes_full_tanks(&self) -> bool {
        self.max_zero_fuel_mass_kg.is_none()
    }
}

/// Running mass of the declared integral wing fuel carried by **one**
/// semi-wing at the structural design case, kg/m, sampled on `stations`.
///
/// The thin form of [`declared_wing_fuel_case`], for a caller that needs only
/// the relief itself. See that function for what the case is and how it is
/// bounded.
pub fn declared_integral_wing_fuel_kg_m(
    config: &AlasConfig,
    design: &alas_config::design_variables::DesignVector,
    requirements: &DesignRequirements,
    wsg: &WingStructureGeometry,
    stations: &[f64],
    front: f64,
    rear: f64,
) -> Option<Vec<f64>> {
    declared_wing_fuel_case(config, design, requirements, wsg, stations, front, rear)
        .map(|case| case.running_mass_kg_m)
}

/// The declared integral wing fuel, resolved into the load case that sizes the
/// box.
///
/// Each cell's `published_usable_volume_l` is the manufacturer's figure for
/// **both** wings, so one semi-wing carries half of it. Within a cell the mass
/// is distributed in proportion to the local enclosed box section, which is
/// where the fuel physically is, so a cell that spans a deepening inboard box
/// is correctly weighted inboard.
///
/// `requirements` must be the same requirements the sizing loads are built from
/// — [`super::design_gross_mass_kg`] applied, not the raw mission mass — because
/// the bound is taken against the mass the manoeuvre is applied at.
///
/// `None` when the aircraft declares no enabled integral wing cell with a
/// published volume — a notional or clean-sheet configuration — in which case
/// the sizer's geometric estimate is the right model and stands.
///
/// # How a partial case is distributed
///
/// When the envelope guarantees less than the full capacity, every declared cell
/// is scaled by the same factor, so the spanwise shape stays the aircraft's own
/// declared one and no refuelling sequence is invented. That is a bracket, not a
/// certainty: a fill that favoured the inboard cells would relieve less and give
/// a heavier box, one that favoured the outboard cells more and a lighter one.
/// No source states the sequence, so the declared shape is kept and the bracket
/// is recorded. On the ATR 72-600 the question does not arise — it declares one
/// wing cell — and on the A380-800 the factor is a uniform `0.841`.
///
/// # What is deliberately not counted
///
/// Only `inner_wing`, `mid_wing` and `outer_wing` are integral wing cells. A
/// centre tank is the wing carry-through box at `y ≈ 0` on the A320-200,
/// A340-300 and A380-800 and relieves nothing there; on the A220-300, B787-9
/// and AVE `alas_config::preset_fuel_tanks` records that the centre tank runs
/// into the inboard wing, so some of that volume **is** wing-carried and is not
/// credited here. No source gives that split, so the uncounted part is left
/// uncounted: it makes those three boxes heavier, which is the safe direction,
/// and inventing a fraction is the one thing that would not be defensible.
pub fn declared_wing_fuel_case(
    config: &AlasConfig,
    design: &alas_config::design_variables::DesignVector,
    requirements: &DesignRequirements,
    wsg: &WingStructureGeometry,
    stations: &[f64],
    front: f64,
    rear: f64,
) -> Option<DeclaredWingFuelCase> {
    let semi_span = wsg.semi_span;
    if !semi_span.is_finite() || semi_span <= 0.0 || stations.len() < 2 {
        return None;
    }
    // The same density the fuel load itself is resolved at, so the mass that
    // relieves the wing and the mass that is loaded into it are one number.
    let (density_kg_m3, _) = crate::product_stations::tank_reference(config, design);
    if !density_kg_m3.is_finite() || density_kg_m3 <= 0.0 {
        return None;
    }

    let layout = &config.fuel_tanks;
    let cells = [&layout.inner_wing, &layout.mid_wing, &layout.outer_wing];
    let mut running = vec![0.0; stations.len()];
    let mut capacity_kg = 0.0;
    for cell in cells {
        if !cell.enabled {
            continue;
        }
        let Some(volume_l) = cell.published_usable_volume_l else {
            continue;
        };
        if !volume_l.is_finite() || volume_l <= 0.0 {
            continue;
        }
        let cell_kg = volume_l * 1.0e-3 * density_kg_m3;
        let semi_mass_kg = 0.5 * cell_kg;
        let area: Vec<f64> = stations
            .iter()
            .map(|&station| {
                let eta = station / semi_span;
                if eta < cell.span_start_fraction || eta > cell.span_end_fraction {
                    0.0
                } else {
                    tanks::box_section_area_m2(wsg, eta, front, rear)
                }
            })
            .collect();
        let integral = trapezoid(&area, stations);
        if !integral.is_finite() || integral <= 0.0 {
            continue;
        }
        capacity_kg += cell_kg;
        for (mass, &local) in running.iter_mut().zip(&area) {
            *mass += semi_mass_kg * local / integral;
        }
    }
    if capacity_kg <= 0.0 {
        return None;
    }

    let design_gross_mass_kg = requirements.mtow_kg;
    let max_zero_fuel_mass_kg = declared_max_zero_fuel_mass_kg(config);
    let design_case_kg = alas_struct::loads::design_case_wing_fuel_kg(
        capacity_kg,
        design_gross_mass_kg,
        max_zero_fuel_mass_kg,
    );
    // Scaling the whole distribution keeps the declared spanwise shape; the
    // factor is one exactly whenever the wings cannot be avoided, so a wing
    // whose case is unchanged is bit-for-bit unchanged.
    if design_case_kg < capacity_kg {
        let factor = design_case_kg / capacity_kg;
        for mass in &mut running {
            *mass *= factor;
        }
    }

    Some(DeclaredWingFuelCase {
        running_mass_kg_m: running,
        capacity_kg,
        design_case_kg,
        design_gross_mass_kg,
        max_zero_fuel_mass_kg,
    })
}

/// The maximum zero-fuel mass this configuration's aircraft is certificated to,
/// kg, from the registered preset's primary-source reference record.
///
/// `None` for a configuration that names no registered aircraft, or one whose
/// registered aircraft publishes no zero-fuel limit (the notional `AVE`). There
/// is no modelled substitute: `requirements.max_structural_payload_kg` is
/// `MZFW − OEW` and recovering `MZFW` from it would need the operating empty
/// mass this very sizing pass is an input to.
fn declared_max_zero_fuel_mass_kg(config: &AlasConfig) -> Option<f64> {
    if config.preset.is_empty() {
        return None;
    }
    alas_config::presets::get(&config.preset)
        .ok()?
        .reference
        .mzfw_kg
}

/// Trapezoidal integral of `values` over `stations`.
fn trapezoid(values: &[f64], stations: &[f64]) -> f64 {
    let mut acc = 0.0;
    for index in 0..values.len().saturating_sub(1) {
        acc += (stations[index + 1] - stations[index]) * (values[index + 1] + values[index]) / 2.0;
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::design_variables::DesignVector;

    type Case = (
        AlasConfig,
        DesignVector,
        DesignRequirements,
        WingStructureGeometry,
        Vec<f64>,
    );

    fn case(preset: &str) -> Case {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
            .unwrap_or_else(|error| panic!("{error}"));
        let registered =
            alas_config::presets::get(preset).unwrap_or_else(|error| panic!("{error}"));
        let design = registered.design_vector;
        let plane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&design), false)
            .unwrap_or_else(|error| panic!("{error}"));
        let wing = crate::wing_reconciliation::main_wing(&plane).expect("a main wing");
        let (fractions, full_span) = config.structures.resolved_spars();
        let wsg = WingStructureGeometry::new(
            &design,
            &config.geometry.wing,
            &wing.xsecs.first().expect("a root section").airfoil,
            &wing.xsecs.last().expect("a tip section").airfoil,
            &fractions,
            Some(&full_span),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let stations = alas_struct::sizing::sizing_stations(&wsg, &config.structures);
        let requirements = super::super::design_requirements(&config);
        (config, design, requirements, wsg, stations)
    }

    fn resolve(preset: &str) -> (DeclaredWingFuelCase, Vec<f64>) {
        let (config, design, requirements, wsg, stations) = case(preset);
        let (front, rear) = alas_struct::sizing::box_chord_band(&wsg);
        let resolved = declared_wing_fuel_case(
            &config,
            &design,
            &requirements,
            &wsg,
            &stations,
            front,
            rear,
        )
        .unwrap_or_else(|| panic!("{preset} declares integral wing cells"));
        (resolved, stations)
    }

    #[test]
    fn a_declared_wing_cell_resolves_to_half_its_published_volume_per_semi_wing() {
        // A340-300: EASA.A.064 III.9 inner 85,550 L and outer 7,300 L, both
        // sides together, at the preset's own published 0.8 kg/L. Its wings
        // cannot be avoided at 260 000 kg against a 178 000 kg zero-fuel limit
        // — 82 000 kg must be aboard and they hold 74 280 kg — so the declared
        // capacity is the design case unchanged.
        let (resolved, stations) = resolve("A340-300");
        let expected = (85_550.0 + 7_300.0) * 0.8;
        assert!((resolved.capacity_kg - expected).abs() / expected < 1.0e-9);
        assert_eq!(resolved.design_case_kg, resolved.capacity_kg);
        assert!(!resolved.zero_fuel_limited());
        assert_eq!(resolved.max_zero_fuel_mass_kg, Some(178_000.0));
        let semi_kg = trapezoid(&resolved.running_mass_kg_m, &stations);
        assert!(
            (semi_kg - 0.5 * expected).abs() / (0.5 * expected) < 1.0e-9,
            "{semi_kg} kg against {} kg",
            0.5 * expected
        );
        // Dry inboard of the declared band and dry at the tip.
        assert_eq!(resolved.running_mass_kg_m[0], 0.0);
        assert_eq!(
            *resolved.running_mass_kg_m.last().expect("a tip station"),
            0.0
        );
    }

    #[test]
    fn an_aircraft_that_can_reach_its_design_mass_without_full_wings_is_relieved_by_less() {
        // ATR 72-600: 23 000 kg over a 21 000 kg zero-fuel limit leaves 2 000 kg
        // of fuel guaranteed aboard against 5 065.2 kg of wing tank (6 300 L at
        // the configured 804 kg/m^3, the preset publishing no density of its
        // own), so three fifths of the declared capacity is relief the box does
        // not have.
        let (atr, stations) = resolve("ATR72-600");
        assert!(
            (atr.capacity_kg - 6_300.0 * 0.804).abs() < 1.0e-6,
            "{atr:?}"
        );
        assert!((atr.design_case_kg - 2_000.0).abs() < 1.0e-6, "{atr:?}");
        assert!(atr.zero_fuel_limited() && !atr.assumes_full_tanks());
        let semi_kg = trapezoid(&atr.running_mass_kg_m, &stations);
        assert!((semi_kg - 1_000.0).abs() < 1.0e-6, "{semi_kg} kg");

        // A380-800: 560 000 kg over 361 000 kg against 236 488 kg of wing tank.
        let (a380, stations) = resolve("A380-800");
        assert!((a380.design_case_kg - 199_000.0).abs() < 1.0e-6, "{a380:?}");
        let semi_kg = trapezoid(&a380.running_mass_kg_m, &stations);
        assert!((semi_kg - 99_500.0).abs() < 1.0e-6, "{semi_kg} kg");
        // Every cell is scaled by the one factor, so the declared spanwise
        // shape is unchanged and no fill sequence is invented.
        let (config, design, mut requirements, wsg, stations) = case("A380-800");
        let (front, rear) = alas_struct::sizing::box_chord_band(&wsg);
        // The same aircraft at a design gross mass its wings cannot be avoided
        // at: the case is then the full capacity, on the same density and the
        // same cells, so the two differ only by the scale factor.
        requirements.mtow_kg = 361_000.0 + 2.0 * a380.capacity_kg;
        let full = declared_wing_fuel_case(
            &config,
            &design,
            &requirements,
            &wsg,
            &stations,
            front,
            rear,
        )
        .expect("the same declared cells");
        assert_eq!(full.design_case_kg, full.capacity_kg);
        let factor = a380.design_case_kg / a380.capacity_kg;
        for (capped, uncapped) in a380.running_mass_kg_m.iter().zip(&full.running_mass_kg_m) {
            assert!(
                (capped - uncapped * factor).abs() <= 1.0e-12 * uncapped.abs().max(1.0),
                "{capped} against {uncapped} x {factor}"
            );
        }
    }

    #[test]
    fn every_registered_aircraft_is_relieved_only_by_fuel_its_envelope_guarantees() {
        // The bound is the bending-mass identity `DG - F = max(MZFW, DG - C)`,
        // checked on the resolved capacity of each registered aircraft rather
        // than on a recorded number.
        for preset in alas_config::presets::registry() {
            let (config, design, requirements, wsg, stations) = case(preset.name);
            let (front, rear) = alas_struct::sizing::box_chord_band(&wsg);
            let Some(resolved) = declared_wing_fuel_case(
                &config,
                &design,
                &requirements,
                &wsg,
                &stations,
                front,
                rear,
            ) else {
                continue;
            };
            let _ = stations;
            assert!(
                resolved.design_case_kg <= resolved.capacity_kg + 1.0e-9,
                "{}: {resolved:?}",
                preset.name
            );
            match resolved.max_zero_fuel_mass_kg {
                Some(mzfw_kg) => {
                    let bending = resolved.design_gross_mass_kg - resolved.design_case_kg;
                    assert!(
                        (bending
                            - mzfw_kg
                                .min(resolved.design_gross_mass_kg)
                                .max(resolved.design_gross_mass_kg - resolved.capacity_kg))
                        .abs()
                            < 1.0e-6,
                        "{}: {resolved:?}",
                        preset.name
                    );
                }
                // No declared zero-fuel limit: the case reverts to the full
                // tank and says so, which is the reportable assumption.
                None => {
                    assert!(resolved.assumes_full_tanks());
                    assert_eq!(
                        resolved.design_case_kg, resolved.capacity_kg,
                        "{}: {resolved:?}",
                        preset.name
                    );
                }
            }
        }
    }

    #[test]
    fn the_declared_fuel_replaces_the_geometric_estimate_rather_than_matching_it() {
        // The point of this seam: the two disagree, and the declaration wins.
        // On the A340-300 the geometric box under-estimates the tanks; on the
        // DC-10, whose fuel is largely in the centre and auxiliary tanks, it
        // over-estimates them by half.
        for (preset, geometric_over_declared) in [("A340-300", false), ("DC-10", true)] {
            let (resolved, stations) = resolve(preset);
            let (_, _, _, wsg, _) = case(preset);
            let (front, rear) = alas_struct::sizing::box_chord_band(&wsg);
            let geometric =
                alas_struct::tanks::integral_fuel_running_mass_kg_m(&wsg, &stations, front, rear);
            let declared_kg = trapezoid(&resolved.running_mass_kg_m, &stations);
            let geometric_kg = trapezoid(&geometric, &stations);
            assert!(
                (geometric_kg > declared_kg) == geometric_over_declared,
                "{preset}: geometric {geometric_kg} kg, declared {declared_kg} kg"
            );
        }
    }

    #[test]
    fn a_configuration_with_no_published_wing_cell_keeps_the_geometric_estimate() {
        let (mut config, design, requirements, wsg, stations) = case("A340-300");
        let (front, rear) = alas_struct::sizing::box_chord_band(&wsg);
        config.fuel_tanks.inner_wing.published_usable_volume_l = None;
        config.fuel_tanks.mid_wing.published_usable_volume_l = None;
        config.fuel_tanks.outer_wing.published_usable_volume_l = None;
        assert!(declared_wing_fuel_case(
            &config,
            &design,
            &requirements,
            &wsg,
            &stations,
            front,
            rear
        )
        .is_none());
    }
}
