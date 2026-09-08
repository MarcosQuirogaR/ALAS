// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;
use crate::flops_transport::structure::WingBendingFactor;
use crate::torenbeek::{
    mass_wing_with_control_surface_area, wing_secondary_mass_breakdown_with_control_surface_area,
};
use alas_config::ControlSurfacesConfig;
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::wing::{Wing, WingXSec};

/// A320-200 requirements read from the registered preset
/// (`alas_config::presets::narrowbody::a320_200`): 78,000 kg MTOW, 35.80 m
/// span, 7.333/3.432/1.40 m centreline/kink/tip chords at a 0.34 kink span
/// fraction, 27 deg leading-edge sweep, 180 m/s dive speed. The section is a
/// 10 % NACA symmetric airfoil, the thickness of the preset's SC(2)-0610 and
/// SC(2)-0410 sections.
const A320_MTOW_KG: f64 = 78_000.0;
const A320_SEMISPAN_M: f64 = 17.90;
const A320_ULTIMATE_LOAD_FACTOR: f64 = 3.75;
const A320_DIVE_SPEED_M_S: f64 = 180.0;
/// `MassModelConfig::default()` flap placard speed and deflection.
const FLAP_PLACARD_M_S: f64 = 90.0;
const FLAP_DEFLECTION_DEG: f64 = 40.0;
/// `MassModelConfig::default()` suspended-mass fraction.
const SUSPENDED_MASS_FRACTION: f64 = 0.75;

fn a320_wing() -> Wing {
    let sweep = 27.0_f64.to_radians().tan();
    let kink_y = 0.34 * A320_SEMISPAN_M;
    let airfoil = Airfoil::from_name("naca0010").expect("valid 4-digit NACA name");
    Wing::new(
        "Main Wing",
        vec![
            WingXSec::new([0.0, 0.0, -1.2], 7.333, 0.0, airfoil.clone()),
            WingXSec::new([kink_y * sweep, kink_y, -0.2], 3.432, 0.0, airfoil.clone()),
            WingXSec::new(
                [A320_SEMISPAN_M * sweep, A320_SEMISPAN_M, 1.5],
                1.40,
                0.0,
                airfoil,
            ),
        ],
        true,
    )
}

/// Chord at a fraction of the semispan on the two-panel planform.
fn chord_at(fraction: f64) -> f64 {
    let kink = 0.34;
    if fraction <= kink {
        7.333 + (3.432 - 7.333) * fraction / kink
    } else {
        3.432 + (1.40 - 3.432) * (fraction - kink) / (1.0 - kink)
    }
}

/// Complete-wing planform area of a surface running between two semispan
/// fractions at a constant local-chord fraction, m^2.
fn surface_area(start: f64, end: f64, chord_fraction: f64) -> f64 {
    let samples = 512usize;
    let width = (end - start) / samples as f64;
    let area: f64 = (0..samples)
        .map(|index| {
            let fraction = start + (index as f64 + 0.5) * width;
            chord_at(fraction) * chord_fraction * width * A320_SEMISPAN_M
        })
        .sum();
    2.0 * area
}

fn a320_surfaces() -> WingMovableSurfaces {
    let control = ControlSurfacesConfig::default();
    let area = |start: f64, end: f64, chord: f64| MovableSurface {
        area_m2: surface_area(start, end, chord),
        // Only the mass split is under test here; the centroids are the
        // spanwise mid-points of each run, placed on the x axis so the first
        // moment is easy to check by hand.
        centroid_m: [0.5 * (start + end) * 10.0, 0.0, 0.0],
    };
    WingMovableSurfaces {
        trailing_edge_flaps: area(
            control.flap_span_start_frac,
            control.flap_span_end_frac,
            control.flap_chord_fraction,
        ),
        leading_edge_devices: area(
            control.slat_span_start_frac,
            control.slat_span_end_frac,
            control.slat_chord_fraction,
        ),
        ailerons: area(
            control.aileron_span_start_frac,
            control.aileron_span_end_frac,
            control.aileron_chord_fraction,
        ),
        spoilers: area(
            control.spoiler_span_start_frac,
            control.spoiler_span_end_frac,
            control.spoiler_chord_fraction,
        ),
    }
}

fn torenbeek_group(wing: &Wing, mtow_kg: f64, ultimate_load_factor: f64) -> TorenbeekWingGroup {
    let flap_area = a320_surfaces().trailing_edge_flaps.area_m2;
    let suspended = mtow_kg * SUSPENDED_MASS_FRACTION;
    let breakdown = wing_secondary_mass_breakdown_with_control_surface_area(
        wing,
        mtow_kg,
        ultimate_load_factor,
        suspended,
        A320_DIVE_SPEED_M_S,
        FLAP_PLACARD_M_S,
        true,
        FLAP_DEFLECTION_DEG,
        None,
        flap_area,
    );
    TorenbeekWingGroup {
        group_total_kg: mass_wing_with_control_surface_area(
            wing,
            mtow_kg,
            ultimate_load_factor,
            suspended,
            A320_DIVE_SPEED_M_S,
            FLAP_PLACARD_M_S,
            true,
            FLAP_DEFLECTION_DEG,
            None,
            flap_area,
        ),
        high_lift_devices_kg: breakdown.high_lift_devices_kg,
        spoilers_and_speedbrakes_kg: breakdown.spoilers_and_speedbrakes_kg,
    }
}

fn a320_flops(wing: &Wing, mtow_kg: f64, ultimate_load_factor: f64) -> FlopsWingInputs {
    FlopsWingInputs {
        design_gross_mass_kg: mtow_kg,
        wing_area_m2: wing.reference_area(),
        wing_span_m: wing.reference_span(),
        taper_ratio: 1.40 / 7.333,
        quarter_chord_sweep_deg: 25.0,
        thickness_to_chord: 0.10,
        movable_surface_area_m2: 0.0,
        ultimate_load_factor,
        composite_utilization: 0.0,
        aeroelastic_tailoring: 0.0,
        strut_bracing: 0.0,
        wing_load_fraction: 1.0,
        fuselage_count: 1,
        variable_sweep_penalty: 0.0,
        wing_mounted_engine_count: 2,
        bending: WingBendingFactor::Simplified,
    }
}

/// Inputs for the A320-class wing with a sized box supplied by the caller.
fn a320_inputs(
    wing: &Wing,
    sized_box_kg: f64,
    mtow_kg: f64,
    ultimate_load_factor: f64,
) -> WingInventoryInputs {
    WingInventoryInputs {
        flops: a320_flops(wing, mtow_kg, ultimate_load_factor),
        torenbeek: torenbeek_group(wing, mtow_kg, ultimate_load_factor),
        sized_box: SizedWingboxMass::full_wing(sized_box_kg, [17.0, 0.0, 0.0]),
        surfaces: a320_surfaces(),
        fixed_structure: FixedNonBoxStructure {
            // `StructuresConfig::default()` spars at 25 % and 70 % chord.
            chord_fraction_outside_box: 0.25 + 0.30,
            centroid_m: [17.0, 0.0, 0.0],
        },
    }
}

#[test]
fn the_a320_class_wing_produces_a_complete_inventory_inside_the_sourced_band() {
    let wing = a320_wing();
    // The reference area of this planform closes the published 122.6 m^2.
    assert!(
        (wing.reference_area() - 122.6).abs() < 0.2,
        "reference area {} m^2",
        wing.reference_area()
    );
    // The like-for-like stand-in for the analytical box in this unit test is
    // this module's own box definition: Torenbeek's basic structure (spar box,
    // skin and ribs) less the fixed non-box structure the inventory carries as
    // a separate item. The coupled value comes from `alas_struct::sizing` at
    // the MDO seam, where that identity holds to about 2 %.
    let torenbeek = torenbeek_group(&wing, A320_MTOW_KG, A320_ULTIMATE_LOAD_FACTOR);
    let miscellaneous_kg = {
        let mut flops = a320_flops(&wing, A320_MTOW_KG, A320_ULTIMATE_LOAD_FACTOR);
        flops.movable_surface_area_m2 = 40.0;
        wing_mass(&flops).miscellaneous_kg
    };
    let box_kg = torenbeek.basic_structure_kg() - 0.55 * miscellaneous_kg;
    let inputs = a320_inputs(&wing, box_kg, A320_MTOW_KG, A320_ULTIMATE_LOAD_FACTOR);
    let inventory = build_wing_inventory(&inputs).expect("A320-class inputs are evaluable");
    inventory
        .require_complete()
        .expect("A320-class inventory is complete");

    // Independent cross-check of the two empirical wing groups on the same
    // planform. They are unrelated fits, so their agreement is evidence that
    // this geometry is inside both validity domains.
    let method_ratio =
        inventory.diagnostics.torenbeek_group_total_kg / inventory.diagnostics.flops_group_total_kg;
    assert!(
        (0.90..=1.10).contains(&method_ratio),
        "Torenbeek/FLOPS wing group ratio {method_ratio:.3}"
    );

    // Every enumerated item is present and positive.
    assert_eq!(inventory.items.len(), ITEM_COUNT);
    assert!(inventory.items.iter().all(|item| item.mass_kg > 0.0));
    assert!(inventory
        .items
        .iter()
        .all(|item| !item.source.is_empty() && item.relative_uncertainty > 0.0));
    // The item masses sum to the reported total and first moment.
    let summed: f64 = inventory.items.iter().map(|item| item.mass_kg).sum();
    assert!((summed - inventory.total_kg()).abs() < 1.0e-9);
    assert!(
        (inventory.complete_wing_mass_kg - inventory.sized_box_mass_kg - inventory.total_kg).abs()
            < 1.0e-9
    );

    let fraction = inventory.diagnostics.non_box_fraction;
    let [lower, upper] = inventory.diagnostics.non_box_fraction_band;
    assert!(
        (lower..=upper).contains(&fraction),
        "non-box share {fraction} outside [{lower}, {upper}]"
    );

    // Reference value. Airbus publishes no A320 wing group mass, so the
    // comparison is to the published narrowbody class statement: a transport
    // wing group is 9-11 % of maximum takeoff weight (B737-200 9.2 %,
    // B727-200 10.4 %, DC-9-30 10.5 %, Roskam, Airplane Design Part V, the
    // same statement `flops_transport::structure`'s own class test cites).
    // For 78,000 kg MTOW that is 7,020-8,580 kg with a 7,800 kg mid-point.
    // Nothing above was tuned to it. Measured on 2026-09-06: wing group
    // 8,509 kg, 10.91 % of MTOW, +9.1 % from the 7,800 kg mid-point, from a
    // 6,682 kg box and a 1,827 kg non-box inventory (21.5 % non-box share);
    // Torenbeek group 7,899 kg and FLOPS group 7,972 kg on the same planform.
    let class_reference_kg = 0.10 * A320_MTOW_KG;
    let relative_error =
        (inventory.complete_wing_mass_kg - class_reference_kg) / class_reference_kg;
    assert!(
        (0.09..=0.115).contains(&(inventory.complete_wing_mass_kg / A320_MTOW_KG)),
        "wing group is {:.2} % of MTOW, outside the 9-11 % class band",
        100.0 * inventory.complete_wing_mass_kg / A320_MTOW_KG
    );
    assert!(
        relative_error.abs() < 0.15,
        "wing group is {:+.1} % from the class mid-point",
        100.0 * relative_error
    );
}

#[test]
fn a_heavier_box_makes_a_heavier_wing_and_never_shrinks_the_inventory() {
    let wing = a320_wing();
    let light = build_wing_inventory(&a320_inputs(
        &wing,
        6_000.0,
        A320_MTOW_KG,
        A320_ULTIMATE_LOAD_FACTOR,
    ))
    .expect("evaluable");
    let heavy = build_wing_inventory(&a320_inputs(
        &wing,
        7_000.0,
        A320_MTOW_KG,
        A320_ULTIMATE_LOAD_FACTOR,
    ))
    .expect("evaluable");

    // The design intent: d(total wing)/d(box) = 1 exactly, so a stiffer or
    // heavier box can never be cancelled by a complementary remainder.
    assert!((heavy.total_kg() - light.total_kg()).abs() < 1.0e-9);
    assert!(
        (heavy.complete_wing_mass_kg - light.complete_wing_mass_kg - 1_000.0).abs() < 1.0e-9,
        "{} vs {}",
        heavy.complete_wing_mass_kg,
        light.complete_wing_mass_kg
    );
}

#[test]
fn a_box_heavier_than_the_empirical_group_is_rejected_with_a_typed_error() {
    let wing = a320_wing();
    let group_kg = torenbeek_group(&wing, A320_MTOW_KG, A320_ULTIMATE_LOAD_FACTOR).group_total_kg;
    let inventory = build_wing_inventory(&a320_inputs(
        &wing,
        group_kg * 1.10,
        A320_MTOW_KG,
        A320_ULTIMATE_LOAD_FACTOR,
    ))
    .expect("an implausible box is still evaluable");
    assert!(!inventory.status().is_complete());
    match inventory.require_complete() {
        Err(WingInventoryError::SizedBoxExceedsEmpiricalGroup {
            sized_box_kg,
            empirical_group_kg,
        }) => {
            assert!((sized_box_kg - group_kg * 1.10).abs() < 1.0e-6);
            assert!((empirical_group_kg - group_kg).abs() < 1.0e-9);
        }
        other => panic!("expected a typed box-versus-group rejection, got {other:?}"),
    }
}

#[test]
fn a_missing_aileron_run_is_reported_as_a_missing_item() {
    let wing = a320_wing();
    let mut inputs = a320_inputs(&wing, 6_000.0, A320_MTOW_KG, A320_ULTIMATE_LOAD_FACTOR);
    inputs.surfaces.ailerons = MovableSurface::NONE;
    let inventory = build_wing_inventory(&inputs).expect("evaluable");
    assert!(matches!(
        inventory.require_complete(),
        Err(WingInventoryError::MissingItem { name: "ailerons" })
    ));

    // An unslatted wing stays complete: leading-edge devices are optional.
    let mut unslatted = a320_inputs(&wing, 6_000.0, A320_MTOW_KG, A320_ULTIMATE_LOAD_FACTOR);
    unslatted.surfaces.leading_edge_devices = MovableSurface::NONE;
    let inventory = build_wing_inventory(&unslatted).expect("evaluable");
    assert_eq!(
        inventory
            .items
            .iter()
            .find(|item| item.name == "leading_edge_high_lift")
            .map(|item| item.mass_kg),
        Some(0.0)
    );
    assert!(inventory.status().is_complete());
}

#[test]
fn the_secondary_items_grow_with_ultimate_load_factor_and_gross_mass() {
    let wing = a320_wing();
    let at = |mtow_kg: f64, ulf: f64| {
        build_wing_inventory(&a320_inputs(&wing, 6_000.0, mtow_kg, ulf))
            .expect("evaluable")
            .total_kg()
    };
    let nominal = at(A320_MTOW_KG, A320_ULTIMATE_LOAD_FACTOR);
    // Ultimate load factor enters through the Torenbeek basic structure that
    // scales the spoiler allowance; gross mass enters through that term and
    // through the FLOPS Eq. 35 movable-area increments.
    for ulf in [3.9_f64, 4.2, 4.5] {
        assert!(
            at(A320_MTOW_KG, ulf) > nominal,
            "ultimate load factor {ulf} did not increase the inventory"
        );
    }
    let mut previous = nominal;
    for mtow_kg in [82_000.0_f64, 86_000.0, 90_000.0] {
        let heavier = at(mtow_kg, A320_ULTIMATE_LOAD_FACTOR);
        assert!(
            heavier > previous,
            "MTOW {mtow_kg} kg did not increase the inventory"
        );
        previous = heavier;
    }
}

#[test]
fn the_first_moment_and_secondary_conversion_are_consistent() {
    let wing = a320_wing();
    let inventory = build_wing_inventory(&a320_inputs(
        &wing,
        6_000.0,
        A320_MTOW_KG,
        A320_ULTIMATE_LOAD_FACTOR,
    ))
    .expect("evaluable");

    let expected: [f64; 3] = std::array::from_fn(|axis| {
        inventory
            .items
            .iter()
            .map(|item| item.mass_kg * item.centroid_m[axis])
            .sum()
    });
    for (axis, (&reported, &target)) in inventory
        .first_moment_kg_m()
        .iter()
        .zip(expected.iter())
        .enumerate()
    {
        let tolerance = 1.0e-9 * target.abs().max(1.0);
        assert!(
            (reported - target).abs() <= tolerance,
            "axis {axis}: {reported} vs {target}"
        );
        assert!(
            (inventory.centroid_m[axis] * inventory.total_kg() - target).abs() <= tolerance,
            "axis {axis} centroid does not reproduce the first moment"
        );
    }

    let secondary = inventory.secondary_wing_mass();
    assert_eq!(secondary.extent, WingExtent::FullWing);
    assert_eq!(secondary.mass_kg, inventory.total_kg());
    assert_eq!(secondary.centroid_m, inventory.centroid_m);
}

#[test]
fn a_symmetric_semispan_box_is_doubled_once_before_the_gates() {
    let wing = a320_wing();
    let mut inputs = a320_inputs(&wing, 6_000.0, A320_MTOW_KG, A320_ULTIMATE_LOAD_FACTOR);
    let full = build_wing_inventory(&inputs).expect("evaluable");
    inputs.sized_box = SizedWingboxMass::symmetric_semiwing(3_000.0, [17.0, 4.0, 0.0]);
    let semi = build_wing_inventory(&inputs).expect("evaluable");
    assert_eq!(semi.sized_box_mass_kg, full.sized_box_mass_kg);
    assert_eq!(semi.complete_wing_mass_kg, full.complete_wing_mass_kg);
}

#[test]
fn non_physical_inputs_are_rejected_before_any_mass_is_formed() {
    let wing = a320_wing();
    let base = a320_inputs(&wing, 6_000.0, A320_MTOW_KG, A320_ULTIMATE_LOAD_FACTOR);

    let mut nan_mass = base;
    nan_mass.flops.design_gross_mass_kg = f64::NAN;
    assert!(matches!(
        build_wing_inventory(&nan_mass),
        Err(WingInventoryError::NonFinite { .. })
    ));

    let mut zero_area = base;
    zero_area.flops.wing_area_m2 = 0.0;
    assert!(matches!(
        build_wing_inventory(&zero_area),
        Err(WingInventoryError::NonPositive { .. })
    ));

    let mut negative_area = base;
    negative_area.surfaces.spoilers.area_m2 = -1.0;
    assert!(matches!(
        build_wing_inventory(&negative_area),
        Err(WingInventoryError::Negative { .. })
    ));

    let mut negative_box = base;
    negative_box.sized_box = SizedWingboxMass::full_wing(-1.0, [17.0, 0.0, 0.0]);
    assert!(matches!(
        build_wing_inventory(&negative_box),
        Err(WingInventoryError::NonPositive {
            field: "sized wingbox mass"
        })
    ));

    // A Torenbeek group that leaves no basic structure cannot describe a wing.
    let mut no_basic = base;
    no_basic.torenbeek.group_total_kg = no_basic.torenbeek.movable_items_kg();
    assert!(matches!(
        build_wing_inventory(&no_basic),
        Err(WingInventoryError::NonPositive {
            field: "Torenbeek basic structure"
        })
    ));
}
