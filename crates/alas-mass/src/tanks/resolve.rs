// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Building a [`FuelTankLayout`] from a configured arrangement and a built
//! aircraft.
//!
//! Each tank family gets its own builder function because each reads a
//! different piece of the built geometry -- a wing tank the main wing, the
//! centre tank the wing root and the side of body, the trim tank the
//! horizontal stabiliser, the auxiliary tank the fuselage -- and keeping
//! them apart means a change to one family's geometry cannot silently reach
//! another's.

use alas_config::{
    AuxiliaryTankConfig, CenterTankConfig, FuelPolicyConfig, FuelTankLayoutConfig, FuselageConfig,
    GeometryConfig, StructuresConfig, TrimTankConfig, WingConfig, WingTankConfig,
};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::Wing;

use super::geometry::{
    fuselage_station, horizontal_stabilizer, integrate_wing_box, section_box, semispan_bounds,
    side_of_body_y_m, spar_box_limits, SectionBox, M3_PER_LITRE,
};
use super::types::{CapacitySource, FuelTank, FuelTankLayout, TankKind, TankLayoutError, TankSide};

impl FuelTankLayout {
    /// Resolve `config`'s tank arrangement on `plane`, the aircraft
    /// `geometry` and `structures` built.
    ///
    /// `density_kg_m3` prices every tank's usable volume into a capacity;
    /// `published_total_usable_volume_l` is a registered aircraft's
    /// manufacturer-published total, read only when
    /// `config.calibrate_to_published_capacity` is set.
    ///
    /// # Errors
    ///
    /// See [`TankLayoutError`]: an invalid configuration or policy, a
    /// wingbox with fewer than two full-span spars, a tank whose configured
    /// span has no volume on the built wing, a missing stabiliser or
    /// fuselage, or a calibration that cannot be resolved to a finite,
    /// positive factor.
    pub fn resolve(
        plane: &Airplane,
        geometry: &GeometryConfig,
        structures: &StructuresConfig,
        config: &FuelTankLayoutConfig,
        policy: &FuelPolicyConfig,
        density_kg_m3: f64,
        published_total_usable_volume_l: Option<f64>,
    ) -> Result<Self, TankLayoutError> {
        config.validate().map_err(TankLayoutError::InvalidConfig)?;
        policy.validate().map_err(TankLayoutError::InvalidPolicy)?;
        if !density_kg_m3.is_finite() || density_kg_m3 <= 0.0 {
            return Err(TankLayoutError::InvalidDensity(density_kg_m3));
        }

        let wing = plane
            .wings
            .first()
            .ok_or(TankLayoutError::DegenerateWingSpan)?;
        let (front, rear) = spar_box_limits(structures)?;
        let pricing = TankPricing {
            front,
            rear,
            unusable_fuel_fraction: policy.unusable_fuel_fraction,
            expansion_space_fraction: policy.expansion_space_fraction,
            density_kg_m3,
        };

        let mut tanks = Vec::new();
        for (kind, cell) in [
            (TankKind::WingInner, &config.inner_wing),
            (TankKind::WingMid, &config.mid_wing),
            (TankKind::WingOuter, &config.outer_wing),
        ] {
            if !cell.enabled {
                continue;
            }
            tanks.extend(wing_tank_pair(wing, kind, cell, &pricing)?);
        }

        if config.center.enabled {
            tanks.push(center_tank(
                wing,
                &geometry.wing,
                &geometry.fuselage,
                &config.center,
                &pricing,
            )?);
        }

        if config.trim.enabled {
            tanks.push(trim_tank(plane, &config.trim, &pricing)?);
        }

        if config.auxiliary.enabled {
            tanks.push(auxiliary_tank(
                plane,
                &config.auxiliary,
                pricing.unusable_fuel_fraction,
                pricing.density_kg_m3,
            )?);
        }

        let geometric_calibration_factor = calibrate(
            &mut tanks,
            config.calibrate_to_published_capacity,
            published_total_usable_volume_l,
            pricing.density_kg_m3,
            pricing.unusable_fuel_fraction,
        )?;

        Ok(Self {
            tanks,
            geometric_calibration_factor,
            density_kg_m3,
        })
    }
}

/// The spar-box chord bounds and pricing terms every tank builder needs, so
/// the per-family functions below stay under the arity lint without each
/// carrying a hidden dependency on `policy` or `structures` directly.
struct TankPricing {
    front: f64,
    rear: f64,
    unusable_fuel_fraction: f64,
    expansion_space_fraction: f64,
    density_kg_m3: f64,
}

/// The usable volume and its [`CapacitySource`] for one geometric estimate,
/// honouring a published override.
fn usable_volume(
    geometric_volume_m3: f64,
    usable_fraction: f64,
    expansion_space_fraction: f64,
    published_usable_volume_l: Option<f64>,
) -> (f64, CapacitySource) {
    match published_usable_volume_l {
        Some(volume_l) => (volume_l * M3_PER_LITRE, CapacitySource::Published),
        None => (
            geometric_volume_m3 * usable_fraction * (1.0 - expansion_space_fraction),
            CapacitySource::Geometric,
        ),
    }
}

/// One or two [`FuelTank`]s (mirrored, for a symmetric wing) for an integral
/// wing cell.
fn wing_tank_pair(
    wing: &Wing,
    kind: TankKind,
    cell: &WingTankConfig,
    pricing: &TankPricing,
) -> Result<Vec<FuelTank>, TankLayoutError> {
    let (front, rear) = (pricing.front, pricing.rear);
    let (root_y, _tip_y, semispan) =
        semispan_bounds(wing).ok_or(TankLayoutError::DegenerateWingSpan)?;
    let start_y = root_y + cell.span_start_fraction * semispan;
    let end_y = root_y + cell.span_end_fraction * semispan;
    let integral = integrate_wing_box(wing, front, rear, start_y, end_y)
        .ok_or(TankLayoutError::DegenerateWingSpan)?;
    if integral.is_degenerate() {
        return Err(TankLayoutError::DegenerateWingSpan);
    }

    let extent_m = [
        integral.mean_width_m(),
        end_y - start_y,
        integral.mean_depth_m(),
    ];
    let x_centroid = integral.x_centroid_m();
    let y_centroid = integral.y_centroid_m();
    let z_centroid = integral.z_centroid_m();

    // A published wing-tank volume is both sides together; each side gets
    // half, since the two are geometrically mirrored and fed independently.
    let published_each_side_l = cell
        .published_usable_volume_l
        .map(|volume_l| volume_l / 2.0);
    let (usable_volume_each_m3, capacity_source) = usable_volume(
        integral.volume_m3,
        cell.usable_fraction,
        pricing.expansion_space_fraction,
        published_each_side_l,
    );
    let usable_capacity_kg = usable_volume_each_m3 * pricing.density_kg_m3;

    let sides: &[(TankSide, f64)] = if wing.symmetric {
        &[(TankSide::Right, 1.0), (TankSide::Left, -1.0)]
    } else {
        &[(TankSide::Right, 1.0)]
    };
    Ok(sides
        .iter()
        .map(|&(side, sign)| FuelTank {
            id: format!("{}_{}", kind.id_prefix(), side.suffix()),
            kind,
            side,
            geometric_volume_m3: integral.volume_m3,
            usable_volume_m3: usable_volume_each_m3,
            usable_capacity_kg,
            unusable_kg: pricing.unusable_fuel_fraction * usable_capacity_kg,
            centroid_m: [x_centroid, sign * y_centroid, z_centroid],
            extent_m,
            burn_priority: cell.burn_priority,
            capacity_source,
        })
        .collect())
}

/// The single carry-through centre tank between `-y_sob` and `+y_sob`.
fn center_tank(
    wing: &Wing,
    wing_config: &WingConfig,
    fuselage_config: &FuselageConfig,
    cell: &CenterTankConfig,
    pricing: &TankPricing,
) -> Result<FuelTank, TankLayoutError> {
    let (front, rear) = (pricing.front, pricing.rear);
    let (root_y, _tip_y, semispan) =
        semispan_bounds(wing).ok_or(TankLayoutError::DegenerateWingSpan)?;
    let y_sob = side_of_body_y_m(wing_config, fuselage_config, root_y, semispan);
    if !(y_sob.is_finite() && y_sob > 0.0) {
        return Err(TankLayoutError::DegenerateWingSpan);
    }
    let root_section = wing
        .xsecs
        .first()
        .ok_or(TankLayoutError::DegenerateWingSpan)?;
    let root_box =
        section_box(root_section, front, rear).ok_or(TankLayoutError::DegenerateWingSpan)?;
    // A wing whose loft places a distinct station at the side of body tapers
    // in chord from the centreline root to it, so the representative
    // section is the mean of the two; a wing with no such station (the side
    // of body coincides with the root, or the loft runs straight to the
    // kink) falls back to the root section alone.
    let side_box = wing
        .xsecs
        .iter()
        .find(|xsec| (xsec.xyz_le[1] - y_sob).abs() <= 1.0e-6 * semispan.max(1.0))
        .and_then(|xsec| section_box(xsec, front, rear));
    let representative = match side_box {
        Some(side) => blend_sections(root_box, side),
        None => root_box,
    };

    let width_y_m = 2.0 * y_sob;
    let geometric_volume_m3 = representative.area_m2 * width_y_m;
    if !(geometric_volume_m3.is_finite() && geometric_volume_m3 > 0.0) {
        return Err(TankLayoutError::DegenerateWingSpan);
    }

    let (usable_volume_m3, capacity_source) = usable_volume(
        geometric_volume_m3,
        cell.usable_fraction,
        pricing.expansion_space_fraction,
        cell.published_usable_volume_l,
    );
    let usable_capacity_kg = usable_volume_m3 * pricing.density_kg_m3;

    Ok(FuelTank {
        id: TankKind::Center.id_prefix().to_owned(),
        kind: TankKind::Center,
        side: TankSide::Centerline,
        geometric_volume_m3,
        usable_volume_m3,
        usable_capacity_kg,
        unusable_kg: pricing.unusable_fuel_fraction * usable_capacity_kg,
        centroid_m: [representative.x_mid_m, 0.0, representative.z_mid_m],
        extent_m: [representative.width_m, width_y_m, representative.depth_m],
        burn_priority: cell.burn_priority,
        capacity_source,
    })
}

/// The arithmetic mean of two spar-box sections.
fn blend_sections(a: SectionBox, b: SectionBox) -> SectionBox {
    SectionBox {
        area_m2: (a.area_m2 + b.area_m2) / 2.0,
        x_mid_m: (a.x_mid_m + b.x_mid_m) / 2.0,
        z_mid_m: (a.z_mid_m + b.z_mid_m) / 2.0,
        width_m: (a.width_m + b.width_m) / 2.0,
        depth_m: (a.depth_m + b.depth_m) / 2.0,
    }
}

/// The single trim tank in the horizontal stabiliser, both sides combined.
fn trim_tank(
    plane: &Airplane,
    cell: &TrimTankConfig,
    pricing: &TankPricing,
) -> Result<FuelTank, TankLayoutError> {
    let (front, rear) = (pricing.front, pricing.rear);
    let stabilizer =
        horizontal_stabilizer(plane).ok_or(TankLayoutError::MissingHorizontalStabilizer)?;
    let (root_y, _tip_y, semispan) =
        semispan_bounds(stabilizer).ok_or(TankLayoutError::DegenerateWingSpan)?;
    let start_y = root_y + cell.span_start_fraction * semispan;
    let end_y = root_y + cell.span_end_fraction * semispan;
    let integral = integrate_wing_box(stabilizer, front, rear, start_y, end_y)
        .ok_or(TankLayoutError::DegenerateWingSpan)?;
    if integral.is_degenerate() {
        return Err(TankLayoutError::DegenerateWingSpan);
    }

    let side_count = if stabilizer.symmetric { 2.0 } else { 1.0 };
    let geometric_volume_m3 = side_count * integral.volume_m3;
    let extent_m = [
        integral.mean_width_m(),
        side_count * (end_y - start_y),
        integral.mean_depth_m(),
    ];

    let (usable_volume_m3, capacity_source) = usable_volume(
        geometric_volume_m3,
        cell.usable_fraction,
        pricing.expansion_space_fraction,
        cell.published_usable_volume_l,
    );
    let usable_capacity_kg = usable_volume_m3 * pricing.density_kg_m3;

    Ok(FuelTank {
        id: TankKind::Trim.id_prefix().to_owned(),
        kind: TankKind::Trim,
        side: TankSide::Centerline,
        geometric_volume_m3,
        usable_volume_m3,
        usable_capacity_kg,
        unusable_kg: pricing.unusable_fuel_fraction * usable_capacity_kg,
        centroid_m: [integral.x_centroid_m(), 0.0, integral.z_centroid_m()],
        extent_m,
        burn_priority: cell.burn_priority,
        capacity_source,
    })
}

/// The declared-volume auxiliary tank, at a fraction of the fuselage length.
fn auxiliary_tank(
    plane: &Airplane,
    cell: &AuxiliaryTankConfig,
    unusable_fuel_fraction: f64,
    density_kg_m3: f64,
) -> Result<FuelTank, TankLayoutError> {
    let fuselage = plane
        .fuselages
        .first()
        .ok_or(TankLayoutError::MissingFuselage)?;
    let centroid_m = fuselage_station(fuselage, cell.x_position_fraction)
        .ok_or(TankLayoutError::MissingFuselage)?;
    let usable_volume_m3 = cell.usable_volume_l * M3_PER_LITRE;
    let usable_capacity_kg = usable_volume_m3 * density_kg_m3;
    // No spar box or hull section sizes a declared fuselage tank, so its
    // inertia is modelled as a cube of the same volume: the least-biased
    // shape when nothing about its actual proportions is known.
    let side_m = usable_volume_m3.max(0.0).cbrt();

    Ok(FuelTank {
        id: TankKind::Auxiliary.id_prefix().to_owned(),
        kind: TankKind::Auxiliary,
        side: TankSide::Centerline,
        geometric_volume_m3: usable_volume_m3,
        usable_volume_m3,
        usable_capacity_kg,
        unusable_kg: unusable_fuel_fraction * usable_capacity_kg,
        centroid_m,
        extent_m: [side_m, side_m, side_m],
        burn_priority: cell.burn_priority,
        capacity_source: CapacitySource::Declared,
    })
}

/// Scale every geometric tank so the layout's total usable volume reproduces
/// `published_total_usable_volume_l`, and return the factor applied.
///
/// Tanks with their own published or declared volume are left untouched and
/// subtracted from the published total first, so the calibration absorbs
/// only the volume that estimate is actually responsible for.
fn calibrate(
    tanks: &mut [FuelTank],
    calibrate_to_published_capacity: bool,
    published_total_usable_volume_l: Option<f64>,
    density_kg_m3: f64,
    unusable_fuel_fraction: f64,
) -> Result<f64, TankLayoutError> {
    if !calibrate_to_published_capacity {
        return Ok(1.0);
    }
    let Some(published_total_l) = published_total_usable_volume_l else {
        return Ok(1.0);
    };
    let published_total_m3 = published_total_l * M3_PER_LITRE;
    let already_accounted_m3: f64 = tanks
        .iter()
        .filter(|tank| !matches!(tank.capacity_source, CapacitySource::Geometric))
        .map(|tank| tank.usable_volume_m3)
        .sum();
    let geometric: Vec<f64> = tanks
        .iter()
        .filter(|tank| matches!(tank.capacity_source, CapacitySource::Geometric))
        .map(|tank| tank.usable_volume_m3)
        .collect();
    // A layout whose every cell carries its own published volume has nothing
    // for the calibration to absorb: the published cells stand as declared
    // (their sum is checked against the registered total where the layout is
    // registered), so the factor is the identity rather than an error.
    if geometric.is_empty() {
        return Ok(1.0);
    }
    let geometric_sum_m3: f64 = geometric.iter().sum();
    if !(geometric_sum_m3.is_finite() && geometric_sum_m3 > 0.0) {
        return Err(TankLayoutError::NothingToCalibrate);
    }
    let factor = (published_total_m3 - already_accounted_m3) / geometric_sum_m3;
    if !(factor.is_finite() && factor > 0.0) {
        return Err(TankLayoutError::InvalidCalibrationFactor { factor });
    }
    for tank in tanks
        .iter_mut()
        .filter(|tank| matches!(tank.capacity_source, CapacitySource::Geometric))
    {
        tank.usable_volume_m3 *= factor;
        tank.usable_capacity_kg = tank.usable_volume_m3 * density_kg_m3;
        tank.unusable_kg = unusable_fuel_fraction * tank.usable_capacity_kg;
        tank.capacity_source = CapacitySource::GeometricCalibrated;
    }
    Ok(factor)
}
