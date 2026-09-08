// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

/// Items a conventional transport wing must carry for the inventory to be
/// presented as complete. Leading-edge devices are deliberately absent: an
/// unslatted wing is a valid transport configuration.
const REQUIRED_ITEMS: [&str; 4] = [
    "trailing_edge_high_lift",
    "ailerons",
    "spoilers_and_speedbrakes",
    "fixed_non_box_structure",
];

/// Build the enumerated non-box inventory for a clean-sheet transport wing.
///
/// The five item masses depend on geometry, design gross mass and ultimate
/// load factor only. The sized box enters the result solely through the
/// additive total and the plausibility gates.
///
/// Returns `Err` only for inputs that are not physically evaluable. An
/// inventory that is evaluable but not defensible is returned with an
/// [`WingInventoryStatus::Incomplete`] status carrying the numbers; call
/// [`WingNonBoxInventory::require_complete`] for the typed error.
pub fn build_wing_inventory(
    inputs: &WingInventoryInputs,
) -> Result<WingNonBoxInventory, WingInventoryError> {
    validate(inputs)?;
    let sized_box_mass_kg = match inputs.sized_box.extent {
        WingExtent::FullWing => inputs.sized_box.mass_kg,
        WingExtent::SymmetricSemiWing => inputs.sized_box.mass_kg * 2.0,
    };

    let surfaces = &inputs.surfaces;
    let flaps = surfaces.trailing_edge_flaps.area_m2;
    let spoilers = surfaces.spoilers.area_m2;
    let slats = surfaces.leading_edge_devices.area_m2;
    let ailerons = surfaces.ailerons.area_m2;

    // The Torenbeek-covered movable area is the reference point of the FLOPS
    // Eq. 35 increments, so the two sources never claim the same surface.
    let covered_m2 = flaps + spoilers;
    let leading_edge_kg =
        shear_and_control_increment(&inputs.flops, covered_m2, covered_m2 + slats);
    let aileron_kg = shear_and_control_increment(
        &inputs.flops,
        covered_m2 + slats,
        covered_m2 + slats + ailerons,
    );

    let group = wing_group(&inputs.flops, covered_m2 + slats + ailerons);
    let fixed_structure_kg =
        group.miscellaneous_kg * inputs.fixed_structure.chord_fraction_outside_box;

    let items = [
        WingInventoryEntry {
            name: "trailing_edge_high_lift",
            mass_kg: inputs.torenbeek.high_lift_devices_kg,
            centroid_m: surfaces.trailing_edge_flaps.centroid_m,
            source: "Torenbeek, Synthesis of Subsonic Airplane Design, 1982, App. C Eq. C-10, with the 1.2 installation multiplier of mass_wing",
            applicability: "trailing-edge flap panels, tracks and carriages; not modelled by the sized box; flap actuation stays in the FLOPS surface-controls systems group",
            relative_uncertainty: 0.20,
        },
        WingInventoryEntry {
            name: "leading_edge_high_lift",
            mass_kg: leading_edge_kg,
            centroid_m: surfaces.leading_edge_devices.centroid_m,
            source: "NASA/TM-2017-219627 Vol. I Eq. 35 (W2), evaluated as the movable-area increment over the Torenbeek-covered area",
            applicability: "slats or Krueger flaps with their tracks; zero for an unslatted wing; attributing an increment of a lumped shear-plus-control term to one surface group is an assumption",
            relative_uncertainty: 0.30,
        },
        WingInventoryEntry {
            name: "ailerons",
            mass_kg: aileron_kg,
            centroid_m: surfaces.ailerons.centroid_m,
            source: "NASA/TM-2017-219627 Vol. I Eq. 35 (W2), evaluated as the movable-area increment over flaps, spoilers and leading-edge devices",
            applicability: "aileron structure and hinges; actuation stays in the FLOPS surface-controls systems group",
            relative_uncertainty: 0.30,
        },
        WingInventoryEntry {
            name: "spoilers_and_speedbrakes",
            mass_kg: inputs.torenbeek.spoilers_and_speedbrakes_kg,
            centroid_m: surfaces.spoilers.centroid_m,
            source: "Torenbeek, Synthesis of Subsonic Airplane Design, 1982, App. C spoiler allowance, with the 1.2 installation multiplier of mass_wing",
            applicability: "spoiler and speedbrake panels and hinges; scales with the Torenbeek basic structure, hence with ultimate load factor and gross mass",
            relative_uncertainty: 0.30,
        },
        WingInventoryEntry {
            name: "fixed_non_box_structure",
            mass_kg: fixed_structure_kg,
            centroid_m: inputs.fixed_structure.centroid_m,
            source: "NASA/TM-2017-219627 Vol. I Eq. 37 (W3, miscellaneous), prorated to the local-chord fraction outside the spars",
            applicability: "fixed leading- and trailing-edge structure, wing/body fairing and joint fittings, tips, access panels and non-optimum allowance; the chordwise proration keeps the rib and box content of W3 with the sized box and is an assumption",
            relative_uncertainty: 0.40,
        },
    ];

    let total_kg: f64 = items.iter().map(|item| item.mass_kg).sum();
    let first_moment_kg_m = items.iter().fold([0.0; 3], |mut moment, item| {
        let item_moment = item.first_moment_kg_m();
        for axis in 0..3 {
            moment[axis] += item_moment[axis];
        }
        moment
    });
    if !total_kg.is_finite() || !first_moment_kg_m.iter().all(|value| value.is_finite()) {
        return Err(WingInventoryError::NonFinite {
            field: "non-box wing inventory total",
        });
    }
    if total_kg <= 0.0 {
        return Err(WingInventoryError::NonPositive {
            field: "non-box wing inventory total",
        });
    }
    let centroid_m = std::array::from_fn(|axis| first_moment_kg_m[axis] / total_kg);
    let complete_wing_mass_kg = sized_box_mass_kg + total_kg;
    let torenbeek_basic_structure_kg = inputs.torenbeek.basic_structure_kg();
    let non_box_fraction = total_kg / complete_wing_mass_kg;
    // Lower bound: the wing must at least carry its movable surfaces, whose
    // share of the wing group Torenbeek gives directly. Upper bound: the box
    // must at least carry the bending material, so the non-box share cannot
    // exceed the FLOPS non-bending share. Both bounds are published
    // correlations evaluated on this same candidate, not fitted constants.
    let lower = inputs.torenbeek.movable_items_kg() / inputs.torenbeek.group_total_kg;
    let upper = (group.shear_and_control_kg + group.miscellaneous_kg) / group.total_kg;
    let diagnostics = WingInventoryDiagnostics {
        torenbeek_group_total_kg: inputs.torenbeek.group_total_kg,
        torenbeek_basic_structure_kg,
        flops_group_total_kg: group.total_kg,
        flops_bending_material_kg: group.bending_material_kg,
        flops_shear_and_control_kg: group.shear_and_control_kg,
        flops_miscellaneous_kg: group.miscellaneous_kg,
        non_box_fraction,
        non_box_fraction_band: [lower, upper],
        box_to_torenbeek_basic_ratio: sized_box_mass_kg / torenbeek_basic_structure_kg,
        box_plus_fixed_to_torenbeek_basic_ratio: (sized_box_mass_kg + fixed_structure_kg)
            / torenbeek_basic_structure_kg,
        total_to_torenbeek_group_ratio: complete_wing_mass_kg / inputs.torenbeek.group_total_kg,
        total_to_flops_group_ratio: complete_wing_mass_kg / group.total_kg,
    };

    let mut findings = [None; MAX_FINDINGS];
    let mut count = 0usize;
    for name in REQUIRED_ITEMS {
        let present = items
            .iter()
            .any(|item| item.name == name && item.mass_kg > 0.0);
        if !present && count < MAX_FINDINGS {
            findings[count] = Some(WingInventoryFinding::MissingItem { name });
            count += 1;
        }
    }
    if sized_box_mass_kg >= inputs.torenbeek.group_total_kg && count < MAX_FINDINGS {
        findings[count] = Some(WingInventoryFinding::SizedBoxExceedsEmpiricalGroup {
            sized_box_kg: sized_box_mass_kg,
            empirical_group_kg: inputs.torenbeek.group_total_kg,
        });
        count += 1;
    }
    if !(lower..=upper).contains(&non_box_fraction) && count < MAX_FINDINGS {
        findings[count] = Some(WingInventoryFinding::NonBoxFractionOutOfBand {
            fraction: non_box_fraction,
            lower,
            upper,
        });
        count += 1;
    }
    let status = if count == 0 {
        WingInventoryStatus::Complete
    } else {
        WingInventoryStatus::Incomplete { findings }
    };

    Ok(WingNonBoxInventory {
        items,
        total_kg,
        first_moment_kg_m,
        centroid_m,
        sized_box_mass_kg,
        complete_wing_mass_kg,
        diagnostics,
        status,
    })
}

/// FLOPS Eqs. 33-38 with an explicit total movable-surface area.
fn wing_group(base: &FlopsWingInputs, movable_surface_area_m2: f64) -> FlopsWingBreakdown {
    let mut inputs = *base;
    inputs.movable_surface_area_m2 = movable_surface_area_m2.max(0.0);
    wing_mass(&inputs)
}

/// The FLOPS Eq. 35 shear-and-control mass added by growing the movable area
/// from `from_m2` to `to_m2`, kg. Nonnegative because Eq. 35 is monotone in
/// `SFLAP`.
fn shear_and_control_increment(base: &FlopsWingInputs, from_m2: f64, to_m2: f64) -> f64 {
    let lower = wing_group(base, from_m2).shear_and_control_kg;
    let upper = wing_group(base, to_m2).shear_and_control_kg;
    (upper - lower).max(0.0)
}

fn validate(inputs: &WingInventoryInputs) -> Result<(), WingInventoryError> {
    let positive: [(&'static str, f64); 6] = [
        ("design gross mass", inputs.flops.design_gross_mass_kg),
        ("wing area", inputs.flops.wing_area_m2),
        ("wing span", inputs.flops.wing_span_m),
        ("ultimate load factor", inputs.flops.ultimate_load_factor),
        ("thickness-to-chord", inputs.flops.thickness_to_chord),
        ("Torenbeek wing group", inputs.torenbeek.group_total_kg),
    ];
    for (field, value) in positive {
        if !value.is_finite() {
            return Err(WingInventoryError::NonFinite { field });
        }
        if value <= 0.0 {
            return Err(WingInventoryError::NonPositive { field });
        }
    }
    let nonnegative: [(&'static str, f64); 7] = [
        (
            "Torenbeek high-lift devices",
            inputs.torenbeek.high_lift_devices_kg,
        ),
        (
            "Torenbeek spoilers and speedbrakes",
            inputs.torenbeek.spoilers_and_speedbrakes_kg,
        ),
        (
            "trailing-edge flap area",
            inputs.surfaces.trailing_edge_flaps.area_m2,
        ),
        (
            "leading-edge device area",
            inputs.surfaces.leading_edge_devices.area_m2,
        ),
        ("aileron area", inputs.surfaces.ailerons.area_m2),
        ("spoiler area", inputs.surfaces.spoilers.area_m2),
        (
            "non-box chord fraction",
            inputs.fixed_structure.chord_fraction_outside_box,
        ),
    ];
    for (field, value) in nonnegative {
        if !value.is_finite() {
            return Err(WingInventoryError::NonFinite { field });
        }
        if value < 0.0 {
            return Err(WingInventoryError::Negative { field });
        }
    }
    if inputs.torenbeek.basic_structure_kg() <= 0.0 {
        return Err(WingInventoryError::NonPositive {
            field: "Torenbeek basic structure",
        });
    }
    if !inputs.sized_box.mass_kg.is_finite() {
        return Err(WingInventoryError::NonFinite {
            field: "sized wingbox mass",
        });
    }
    if inputs.sized_box.mass_kg <= 0.0 {
        return Err(WingInventoryError::NonPositive {
            field: "sized wingbox mass",
        });
    }
    let centroids = [
        inputs.surfaces.trailing_edge_flaps.centroid_m,
        inputs.surfaces.leading_edge_devices.centroid_m,
        inputs.surfaces.ailerons.centroid_m,
        inputs.surfaces.spoilers.centroid_m,
        inputs.fixed_structure.centroid_m,
        inputs.sized_box.centroid_m,
    ];
    if !centroids
        .iter()
        .all(|centroid| centroid.iter().all(|value| value.is_finite()))
    {
        return Err(WingInventoryError::NonFinite {
            field: "wing inventory centroid",
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "../wing_inventory_tests.rs"]
mod tests;
