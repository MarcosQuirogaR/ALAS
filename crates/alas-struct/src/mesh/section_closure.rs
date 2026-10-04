// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Product shell topology on the section's positive-thickness material domain.
//! A crossed trailing-edge gap is closed at the first genuine intersection of
//! the existing linear surface interpolants, without changing those surfaces.

use alas_geom::wing_structure::{RibStation, WingStructureGeometry};

use super::MeshError;

pub(super) fn trim_aft_intersections(
    geometry: &WingStructureGeometry,
    stations: &mut [RibStation],
) -> Result<usize, MeshError> {
    let knots = geometry.airfoil_chordwise_knots();
    let mut trimmed = 0;
    for station in stations {
        let error = MeshError::SectionMaterialDomain { rib: station.index };
        let closure = first_intersection(&knots, |x| {
            let (upper, lower) = geometry.airfoil_zu_zl(station.eta, x);
            upper - lower
        })
        .map_err(|()| error)?;
        let Some(closure) = closure.filter(|&x| x < station.frac_actual) else {
            continue;
        };
        let x_le = geometry.x_le(station.eta);
        let z_le = geometry.z_le(station.eta);
        let (aft_x, aft_y) = station.rib_dir_xy;
        let (nominal, _) = geometry.get_rib_lengths(station.y_station, x_le, aft_x, aft_y);
        let kept = station
            .extrados
            .iter()
            .take_while(|point| {
                let fraction =
                    ((point[0] - x_le) * aft_x + (point[1] - station.y_station) * aft_y) / nominal;
                fraction < closure
            })
            .count();
        // A closure cutting a credited spar cannot retain that box's strength.
        if kept == 0
            || station
                .j_spars
                .iter()
                .any(|&index| usize::try_from(index).is_ok_and(|index| index >= kept))
        {
            return Err(error);
        }
        let (upper, lower) = geometry.airfoil_zu_zl(station.eta, closure);
        let point = [
            x_le + closure * nominal * aft_x,
            station.y_station + closure * nominal * aft_y,
            z_le + 0.5 * (upper + lower) * geometry.local_chord(station.eta),
        ];
        station.extrados.truncate(kept);
        station.intrados.truncate(kept);
        station.extrados.push(point);
        station.intrados.push(point);
        station.frac_actual = closure;
        trimmed += 1;
    }
    Ok(trimmed)
}

/// Exact root on the first linear interval where positive thickness closes.
fn first_intersection(knots: &[f64], thickness: impl Fn(f64) -> f64) -> Result<Option<f64>, ()> {
    let mut positive = false;
    for pair in knots.windows(2) {
        let (left, right) = (thickness(pair[0]), thickness(pair[1]));
        if !left.is_finite() || !right.is_finite() || (!positive && left < 0.0) {
            return Err(());
        }
        positive |= left > 0.0;
        if positive && left > 0.0 && right <= 0.0 {
            return Ok(Some(pair[0] + (pair[1] - pair[0]) * left / (left - right)));
        }
        if !positive && right < 0.0 {
            return Err(());
        }
    }
    Ok(None)
}

#[cfg(test)]
#[path = "section_closure_tests.rs"]
mod tests;
