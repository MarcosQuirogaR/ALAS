// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Cap construction using the same compatible-strain section as the response.
//!
//! Primary-box mass integrates explicit material volume, following the
//! analytical inventory distinction in Torenbeek, *Development and application
//! of a comprehensive, design-sensitive weight prediction method for wing
//! structures of transport category aircraft*, TU Delft LR-693, 1992.
//! Secondary wing items are resolved separately by the mass model.

use super::fatigue::FatigueDemand;
use super::law::{station_cap_dimensions, trapezoid};
use super::section::{station_ei, stress_utilization, update_moment_fractions};
use super::WingboxSizing;

use alas_config::{materials::MaterialSpec, StructuresConfig};

/// Relative resolution of the web-gauge bisection.
///
/// A numerical bound on the gauge search, not a physical margin: the retained
/// section is the feasible end of the bracket. The relieved-load fixed point
/// of [`super::scoped`] iterates this search, whose result moves in steps of
/// this size when the loads move; the step has to stay well under the
/// fixed-point tolerance (1e-9 of the box mass) or the iteration can cycle
/// between two adjacent gauges and never settle. 1e-8 left a noise floor of
/// 1.3e-9 once the torque case made the web gauge a bisected quantity; 1e-11
/// is two orders under the tolerance and costs about ten more bisection steps.
pub(super) const WEB_GAUGE_RELATIVE_TOLERANCE: f64 = 1.0e-11;

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

fn install_web(sizing: &mut WingboxSizing, station: usize, thickness: f64) {
    for spar in &mut sizing.spars {
        spar.t_web[station] = thickness;
    }
}

/// Highest utilization (cap, web, cover) over the spars of one station, and the
/// web's own share of it (clear-panel strength and shear buckling).
fn station_utilizations(
    sizing: &WingboxSizing,
    station: usize,
    moment: f64,
    shear: f64,
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
) -> (f64, f64) {
    let peak = (0..sizing.spars.len())
        .map(|index| stress_utilization(sizing, station, index, moment, shear, skin, web, cap).1)
        .fold(0.0_f64, f64::max);
    let curvature = moment.abs() / station_ei(sizing, station, skin, web, cap);
    let web_only = (0..sizing.spars.len())
        .map(|index| super::web::utilization(sizing, station, index, curvature, shear, web))
        .fold(0.0_f64, f64::max);
    (peak, web_only)
}

/// Size one station: the thinnest web gauge, at or above the manufacturing
/// minimum, for which caps exist that carry the station's combined strength and
/// clear-panel shear buckling at the installed rib pitch.
///
/// Stations are independent at a fixed web gauge, so each gets its own gauge.
/// Caps are rebuilt at every trial gauge (caps and webs share the axial
/// strain), so the caps' own utilization stays at its limit and only the web's
/// share tells how far the gauge is from the boundary. The search brackets the
/// gauge by doubling, then refines it with a log-linear secant on the web
/// utilization (which falls as a power of the gauge), every third step plain
/// bisection. The retained section is the feasible end of the bracket, at the
/// unchanged strength tolerance; this bounds the gauge search error only and is
/// not a global cap/web minimum-weight optimisation.
#[allow(clippy::too_many_arguments)]
fn size_station(
    sizing: &mut WingboxSizing,
    station: usize,
    moment: f64,
    shear: f64,
    cfg: &StructuresConfig,
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
    size_caps: CapSizer,
) {
    let feasible = 1.0 + super::MARGIN_NUMERICAL_ZERO;
    let trial = |sizing: &mut WingboxSizing, gauge: f64| {
        install_web(sizing, station, gauge);
        size_caps(sizing, station, moment, cfg, skin, web, cap);
        station_utilizations(sizing, station, moment, shear, skin, web, cap)
    };
    let minimum = cfg.t_web_min_m;
    let (peak, mut lower_web) = trial(sizing, minimum);
    if peak <= feasible {
        return;
    }
    let mut lower = minimum;
    let mut upper = 2.0 * lower.max(1.0e-12);
    let mut upper_web = f64::INFINITY;
    let mut upper_peak = f64::INFINITY;
    for _ in 0..64 {
        (upper_peak, upper_web) = trial(sizing, upper);
        if upper_peak <= feasible || !upper.is_finite() {
            break;
        }
        lower = upper;
        lower_web = upper_web;
        upper *= 2.0;
    }
    if upper_peak > feasible {
        // The doubling budget is exhausted: the last evaluated section stays,
        // as did the original in-place construction.
        return;
    }
    for step in 0..100 {
        if upper - lower <= WEB_GAUGE_RELATIVE_TOLERANCE * upper
            || (upper_web >= 1.0 - WEB_GAUGE_RELATIVE_TOLERANCE && upper_web <= feasible)
        {
            break;
        }
        // The web utilization is near a power law of the gauge: interpolate
        // ln u against ln t to the target u = 1.
        let secant = if lower_web > 1.0 && upper_web > 0.0 && upper_web < lower_web {
            (lower_web.ln() / (lower_web.ln() - upper_web.ln())).clamp(0.0, 1.0)
        } else {
            0.5
        };
        let fraction = if step % 3 == 2 || !secant.is_finite() {
            0.5
        } else {
            secant.clamp(0.05, 0.95)
        };
        let middle = (lower.ln() + fraction * (upper.ln() - lower.ln()))
            .exp()
            .clamp(lower, upper);
        let (peak, web_only) = trial(sizing, middle);
        if peak <= feasible {
            upper = middle;
            upper_web = web_only;
        } else {
            lower = middle;
            lower_web = web_only;
        }
    }
    trial(sizing, upper);
}

/// Cap sizing of one station at a fixed web gauge: the production search, or
/// the plain bisection the tests replay it against.
type CapSizer = fn(
    &mut WingboxSizing,
    usize,
    f64,
    &StructuresConfig,
    &MaterialSpec,
    &MaterialSpec,
    &MaterialSpec,
);

// The entry point mirrors the sizing inputs: loads, config and the three materials.
#[allow(clippy::too_many_arguments)]
pub(super) fn size(
    sizing: &mut WingboxSizing,
    moments: &[f64],
    shears: &[f64],
    cfg: &StructuresConfig,
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
    fatigue: Option<&FatigueDemand<'_>>,
) {
    size_with(
        sizing,
        moments,
        shears,
        cfg,
        skin,
        web,
        cap,
        fatigue,
        size_station_caps,
    );
}

// Same inputs as `size`, plus the cap sizer the tests replace.
#[allow(clippy::too_many_arguments)]
fn size_with(
    sizing: &mut WingboxSizing,
    moments: &[f64],
    shears: &[f64],
    cfg: &StructuresConfig,
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
    fatigue: Option<&FatigueDemand<'_>>,
    size_caps: CapSizer,
) {
    for (station, (&moment, &shear)) in moments.iter().zip(shears).enumerate() {
        size_station(
            sizing, station, moment, shear, cfg, skin, web, cap, size_caps,
        );
    }
    if let Some(demand) = fatigue {
        super::fatigue::size_lower_cover(sizing, demand, skin, web, cap);
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
                    .zip(&spar.lower_extra_area)
                    .map(|(area, extra)| (2.0 * area + extra) * cap.rho_kg_m3)
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
                    .zip(&spar.t_web)
                    .map(|(height, gauge)| gauge * height * web.rho_kg_m3)
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
