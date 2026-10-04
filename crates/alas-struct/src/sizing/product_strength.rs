// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Cap construction using the same compatible-strain section as the response.
//!
//! Primary-box mass integrates explicit material volume, following the
//! analytical inventory distinction in Torenbeek, *Development and application
//! of a comprehensive, design-sensitive weight prediction method for wing
//! structures of transport category aircraft*, TU Delft LR-693, 1992.
//! Secondary wing items are resolved separately by the mass model.

use super::law::{station_cap_dimensions, trapezoid};
use super::section::{stress_utilization, update_moment_fractions};
use super::WingboxSizing;

use alas_config::{materials::MaterialSpec, StructuresConfig};

#[cfg(test)]
#[path = "product_strength_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "product_strength_reference.rs"]
mod reference;

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

fn size_caps(
    sizing: &mut WingboxSizing,
    moments: &[f64],
    cfg: &StructuresConfig,
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
) {
    for (station, &moment) in moments.iter().enumerate() {
        size_station_caps(sizing, station, moment, cfg, skin, web, cap);
    }
}

/// Size the caps of one station. Every quantity it reads or writes belongs to
/// that station, so stations are independent at a fixed web gauge.
fn size_station_caps(
    sizing: &mut WingboxSizing,
    station: usize,
    moment: f64,
    cfg: &StructuresConfig,
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
) {
    let area = super::cap_search::CapSearch::new(
        sizing,
        station,
        moment,
        cfg.t_skin_min_m,
        skin,
        web,
        cap,
    )
    .area();
    install(sizing, station, area, cfg.t_skin_min_m);
}

fn install_web(sizing: &mut WingboxSizing, thickness: f64) {
    for spar in &mut sizing.spars {
        spar.t_web = thickness;
    }
}

pub(super) fn size(
    sizing: &mut WingboxSizing,
    moments: &[f64],
    shears: &[f64],
    cfg: &StructuresConfig,
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
) {
    let utilization = |section: &WingboxSizing| {
        moments
            .iter()
            .enumerate()
            .flat_map(|(station, moment)| {
                (0..section.spars.len()).map(move |index| {
                    stress_utilization(
                        section,
                        station,
                        index,
                        *moment,
                        shears[station],
                        skin,
                        web,
                        cap,
                    )
                    .1
                })
            })
            .fold(0.0_f64, f64::max)
    };
    size_caps(sizing, moments, cfg, skin, web, cap);
    let feasible = 1.0 + super::MARGIN_NUMERICAL_ZERO;
    if utilization(sizing) > feasible {
        let mut lower = cfg.t_web_min_m;
        let mut upper = 2.0 * lower.max(1.0e-12);
        // Reconstruct caps at every trial web gauge. Joint feasibility is
        // bracketed directly: plain fixed-point iteration can converge slowly
        // when caps and webs share a limiting axial strain. The final tested
        // feasible side satisfies combined strength and shear buckling with
        // the same installed section. This is a feasible construction, not a
        // global cap/web minimum-weight optimization.
        for _ in 0..64 {
            install_web(sizing, upper);
            size_caps(sizing, moments, cfg, skin, web, cap);
            if utilization(sizing) <= feasible || !upper.is_finite() {
                break;
            }
            lower = upper;
            upper *= 2.0;
        }
        if utilization(sizing) <= feasible {
            for _ in 0..52 {
                // This bounds gauge search error only. The retained upper
                // section is physically feasible at the unchanged strength
                // tolerance, rather than a nearly feasible root estimate.
                if upper - lower <= 1.0e-8 * upper {
                    break;
                }
                let middle = 0.5 * (lower + upper);
                install_web(sizing, middle);
                // A trial section is infeasible as soon as one station is:
                // stations are independent at a fixed web gauge, so the rest
                // need not be built. Its partial caps are discarded, because
                // the section retained after the search is rebuilt in full.
                let infeasible = moments.iter().enumerate().any(|(station, &moment)| {
                    size_station_caps(sizing, station, moment, cfg, skin, web, cap);
                    (0..sizing.spars.len()).any(|index| {
                        stress_utilization(
                            sizing,
                            station,
                            index,
                            moment,
                            shears[station],
                            skin,
                            web,
                            cap,
                        )
                        .1 > feasible
                    })
                });
                if infeasible {
                    lower = middle;
                } else {
                    upper = middle;
                }
            }
            install_web(sizing, upper);
            size_caps(sizing, moments, cfg, skin, web, cap);
        }
    }
    for (station, &moment) in moments.iter().enumerate() {
        for index in 0..sizing.spars.len() {
            let ratio = stress_utilization(
                sizing,
                station,
                index,
                moment,
                shears[station],
                skin,
                web,
                cap,
            )
            .1;
            sizing.spars[index].margin_of_safety[station] = if ratio > 0.0 {
                1.0 / ratio - 1.0
            } else {
                f64::INFINITY
            };
        }
    }
    update_moment_fractions(sizing, skin, web, cap);
    // Product quadrature conserves the running-mass inventory exactly. The
    // historical unit-gradient rule gave each endpoint a full station cell.
    sizing.mass_breakdown_kg.spar_caps = sizing
        .spars
        .iter()
        .map(|spar| {
            trapezoid(
                &spar
                    .a_cap
                    .iter()
                    .map(|area| 2.0 * area * cap.rho_kg_m3)
                    .collect::<Vec<_>>(),
                &sizing.y_stations,
            )
        })
        .sum();
    sizing.mass_breakdown_kg.spar_webs = sizing
        .spars
        .iter()
        .map(|spar| {
            trapezoid(
                &spar
                    .h
                    .iter()
                    .map(|height| spar.t_web * height * web.rho_kg_m3)
                    .collect::<Vec<_>>(),
                &sizing.y_stations,
            )
        })
        .sum();
    let (front, rear) = match (sizing.spar_fracs.first(), sizing.spar_fracs.last()) {
        (Some(front), Some(rear)) => (*front, *rear),
        _ => (0.0, 0.0),
    };
    sizing.mass_breakdown_kg.skin = trapezoid(
        &sizing
            .chord
            .iter()
            .map(|chord| 2.0 * (rear - front) * chord * sizing.t_skin * skin.rho_kg_m3)
            .collect::<Vec<_>>(),
        &sizing.y_stations,
    );
    sizing.total_mass_kg = sizing.mass_breakdown_kg.spar_caps
        + sizing.mass_breakdown_kg.spar_webs
        + sizing.mass_breakdown_kg.skin
        + sizing.mass_breakdown_kg.ribs;
}
