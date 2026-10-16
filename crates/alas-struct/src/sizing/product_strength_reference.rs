// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Cap construction using the same compatible-strain section as the response.
//!
//! Primary-box mass integrates explicit material volume, following the
//! analytical inventory distinction in Torenbeek, *Development and application
//! of a comprehensive, design-sensitive weight prediction method for wing
//! structures of transport category aircraft*, TU Delft LR-693, 1992.
//! Secondary wing items are resolved separately by the mass model.

use super::super::law::station_cap_dimensions;
use super::super::section::{cap_ei, non_cap_ei};
use super::super::WingboxSizing;
use crate::allowables::bending_allowable_pa;
use alas_config::{materials::MaterialSpec, StructuresConfig};

/// Flange width follows available spar depth in the existing fabrication law.
/// A common added flange gauge therefore gives A_i proportional to h_i.
/// This assigns more material to the larger bending lever arm while preserving
/// a manufacturable section, rather than an arbitrary equal-area allocation.
/// Moment shares still follow compatible EI, not an imposed force split.
fn install(sizing: &mut WingboxSizing, station: usize, area: f64, minimum_gauge: f64) {
    let chord = sizing.chord[station];
    let maximum_height = sizing
        .spars
        .iter()
        .map(|spar| spar.h[station])
        .fold(0.0_f64, f64::max);
    for spar in &mut sizing.spars {
        let area = if maximum_height > 0.0 {
            area * spar.h[station] / maximum_height
        } else {
            0.0
        };
        let (width, thickness) =
            station_cap_dimensions(area, chord, spar.h[station], minimum_gauge);
        spar.w_cap[station] = width;
        spar.t_cap[station] = thickness;
        spar.a_cap[station] = width * thickness;
    }
}

/// Size the caps of one station. Every quantity it reads or writes belongs to
/// that station, so stations are independent at a fixed web gauge.
pub(super) fn size_station_caps(
    sizing: &mut WingboxSizing,
    station: usize,
    moment: f64,
    cfg: &StructuresConfig,
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
) {
    let panel = super::super::panel::CoverPanel::new(sizing, station);
    let web_allowable = bending_allowable_pa(web);
    // The cap search changes only the caps: spar depths, web gauge and
    // covers, hence the non-cap stiffness and the cover lever arm, are
    // fixed for this station and evaluated once. `station_ei` is the same
    // non-cap term plus the same cap sum, so every trial is unchanged.
    let non_cap = non_cap_ei(sizing, station, skin, web);
    let height = sizing
        .spars
        .iter()
        .map(|spar| spar.h[station])
        .fold(0.0_f64, f64::max);
    let utilization = |section: &WingboxSizing| {
        let ei = non_cap
            + section
                .spars
                .iter()
                .map(|spar| cap_ei(spar, station, cap))
                .sum::<f64>();
        let curvature = moment.abs() / ei;
        let clear_height = section
            .spars
            .iter()
            .map(|spar| super::super::web::clear_height(spar, station))
            .fold(0.0_f64, f64::max);
        let flange = super::super::panel::flange_area_m2(section, station);
        let cap_cover = (cap.e_pa / panel.allowable_pa(cap, flange))
            .max(skin.e_pa / panel.allowable_pa(skin, flange))
            * height;
        0.5 * curvature * cap_cover.max(web.e_pa * clear_height / web_allowable)
    };
    install(sizing, station, 0.0, cfg.t_skin_min_m);
    if utilization(sizing) > 1.0 {
        let mut lower = 0.0;
        let mut upper = cfg.t_skin_min_m * sizing.chord[station];
        // At half-chord width and h/5 thickness every flange is at the
        // existing geometric maximum. The finite search cannot buy a
        // section that exceeds its packaging or occupies the whole web.
        let maximum = sizing
            .spars
            .iter()
            .map(|spar| 0.1 * sizing.chord[station] * spar.h[station])
            .fold(0.0_f64, f64::max);
        upper = upper.max(1.0e-12).min(maximum);
        for _ in 0..64 {
            install(sizing, station, upper, cfg.t_skin_min_m);
            if utilization(sizing) <= 1.0 || upper >= maximum {
                break;
            }
            lower = upper;
            upper = (2.0 * upper).min(maximum);
        }
        if utilization(sizing) <= 1.0 {
            for _ in 0..56 {
                let middle = 0.5 * (lower + upper);
                install(sizing, station, middle, cfg.t_skin_min_m);
                if utilization(sizing) > 1.0 {
                    lower = middle;
                } else {
                    upper = middle;
                }
            }
            // Return the feasible side of the bounded section search.
            install(sizing, station, upper, cfg.t_skin_min_m);
        }
    }
}

/// The whole-box sizing with the plain cap bisection of this oracle.
#[allow(clippy::too_many_arguments)]
pub(super) fn size(
    sizing: &mut WingboxSizing,
    moments: &[f64],
    shears: &[f64],
    cfg: &StructuresConfig,
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
) {
    super::size_with(
        sizing,
        moments,
        shears,
        cfg,
        skin,
        web,
        cap,
        None,
        size_station_caps,
    );
}
