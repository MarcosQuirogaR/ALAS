// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Cap construction using the same compatible-strain section as the response.
//!
//! Primary-box mass integrates explicit material volume, following the
//! analytical inventory distinction in Torenbeek, *Development and application
//! of a comprehensive, design-sensitive weight prediction method for wing
//! structures of transport category aircraft*, TU Delft LR-693, 1992.
//! Secondary wing items are resolved separately by the mass model.

use super::bracket::End;
use super::fatigue::FatigueDemand;
use super::law::{station_cap_dimensions, trapezoid};
use super::section::{max_or_nan, station_ei, stress_utilization, update_moment_fractions};
use super::WingboxSizing;

use alas_config::{materials::MaterialSpec, StructuresConfig};

/// Relative resolution of the web-gauge bisection.
///
/// A numerical bound on the gauge search, not a physical margin: the retained
/// section is the feasible end of the bracket. On a web of at most 20 mm it
/// resolves 2e-6 mm, far below any sheet or machining tolerance (0.01 mm and
/// coarser). The relieved-load fixed point of [`super::scoped`] iterates this
/// search, whose result moves in steps of this size when the loads move, so
/// the step is kept two orders under the fixed-point tolerance
/// (`RELIEF_TOLERANCE`, 1e-5 of the box mass) and the iteration cannot cycle
/// between two adjacent gauges.
pub(super) const WEB_GAUGE_RELATIVE_TOLERANCE: f64 = 1.0e-7;

/// Width, in natural-log units of the gauge, at which the skin-gauge search
/// stops: a relative gauge resolution of 1e-4, 2 µm on a 20 mm cover and
/// 0.2 µm on a 2 mm one, at least five times under a 0.01 mm gauge step.
///
/// A numerical bound, not a physical margin. The search minimises the running
/// mass of the station, which is stationary at an interior optimum, so the
/// mass error is second order in this width (about 1e-8 of the station mass),
/// far below the relieved-load fixed-point tolerance (1e-5 of the box mass).
pub(super) const SKIN_GAUGE_LOG_TOLERANCE: f64 = 1.0e-4;

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
///
/// `hint`, the total cap area of a nearby trial of the station, brackets the
/// search around it ([`super::cap_search::CapSearch::area_from`]). Returns
/// the total cap area installed.
#[allow(clippy::too_many_arguments)]
fn size_station_caps(
    sizing: &mut WingboxSizing,
    station: usize,
    moment: f64,
    cfg: &StructuresConfig,
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
    hint: Option<f64>,
) -> Option<f64> {
    let area = super::cap_search::CapSearch::new(
        sizing,
        station,
        moment,
        cfg.t_skin_min_m,
        skin,
        web,
        cap,
    )
    .area_from(hint);
    install(sizing, station, area, cfg.t_skin_min_m);
    Some(area)
}

fn install_web(sizing: &mut WingboxSizing, station: usize, thickness: f64) {
    for spar in &mut sizing.spars {
        spar.t_web[station] = thickness;
    }
}

/// Everything a station trial installs: the station's cover gauge, when the
/// section has one per station, and each spar's web gauge and cap
/// dimensions there. A trial reads none of it before writing it, so
/// restoring the record of a trial leaves the section exactly as repeating
/// that trial would.
#[derive(Default)]
struct StationRecord {
    gauge: f64,
    values: Vec<f64>,
}

impl StationRecord {
    fn capture(&mut self, sizing: &WingboxSizing, station: usize, gauge: f64) {
        self.gauge = gauge;
        self.values.clear();
        self.values.push(
            sizing
                .skin_gauge_m
                .get(station)
                .copied()
                .unwrap_or(f64::NAN),
        );
        for spar in &sizing.spars {
            self.values.extend([
                spar.t_web[station],
                spar.w_cap[station],
                spar.t_cap[station],
                spar.a_cap[station],
            ]);
        }
    }

    /// Restore the recorded trial if it was at `gauge` (bit for bit);
    /// whether it was.
    fn restore(&self, sizing: &mut WingboxSizing, station: usize, gauge: f64) -> bool {
        if self.values.is_empty() || self.gauge.to_bits() != gauge.to_bits() {
            return false;
        }
        if let Some(skin) = sizing.skin_gauge_m.get_mut(station) {
            *skin = self.values[0];
        }
        for (spar, values) in sizing
            .spars
            .iter_mut()
            .zip(self.values[1..].chunks_exact(4))
        {
            spar.t_web[station] = values[0];
            spar.w_cap[station] = values[1];
            spar.t_cap[station] = values[2];
            spar.a_cap[station] = values[3];
        }
        true
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
        .fold(0.0_f64, max_or_nan);
    let curvature = moment.abs() / station_ei(sizing, station, skin, web, cap);
    let web_only = (0..sizing.spars.len())
        .map(|index| super::web::utilization(sizing, station, index, curvature, shear, web))
        .fold(0.0_f64, max_or_nan);
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
/// gauge by doubling, then refines it by secant steps on the log web
/// utilization against the log gauge (the utilization falls about as a power
/// of the gauge; `bracket::narrow`). The retained section is the feasible end of the bracket, at the
/// unchanged strength tolerance; this bounds the gauge search error only and is
/// not a global cap/web minimum-weight optimisation.
///
/// A station whose utilization is undefined (`NaN` load, stiffness or
/// allowable) is left at the section of the trial that found it; its margin
/// is then published as `NaN`. The gauge search stops at the gauge at which
/// the webs of all spars, side by side, would fill the local chord: a station
/// that still fails there keeps that last section and its margin is negative.
///
/// `hint`, the web gauge the same station took at a nearby cover gauge,
/// replaces the doubling from the minimum by a bracket grown geometrically
/// around it (first step [`super::web_bracket::HINT_STEP`] relative). Either way the bracket
/// holds a failing and a passing gauge and is narrowed by the same steps to
/// the same tolerance, so the hint moves the retained gauge only within that
/// tolerance; a passing minimum is found and kept as without it. Returns the
/// web gauge the search ended on.
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
    hint: Option<f64>,
) -> f64 {
    let feasible = 1.0 + super::MARGIN_NUMERICAL_ZERO;
    // The latest trial that carried the station, so that the search need not
    // repeat it to leave its section installed.
    let passed = std::cell::RefCell::new(StationRecord::default());
    // The cap area of the previous web trial, close to the next one's.
    let cap_hint = std::cell::Cell::new(None);
    let trial = |sizing: &mut WingboxSizing, gauge: f64| {
        install_web(sizing, station, gauge);
        let area = size_caps(sizing, station, moment, cfg, skin, web, cap, cap_hint.get());
        cap_hint.set(area);
        let (peak, web_only) = station_utilizations(sizing, station, moment, shear, skin, web, cap);
        if peak <= feasible {
            passed.borrow_mut().capture(sizing, station, gauge);
        }
        (peak, web_only)
    };
    let minimum = cfg.t_web_min_m;
    // Geometric ceiling of the gauge, never below the first trial above the
    // manufacturing minimum.
    let ceiling =
        (sizing.chord[station] / sizing.spars.len().max(1) as f64).max(2.0 * minimum.max(1.0e-12));
    let hint = hint.filter(|&h| h.is_finite() && h > minimum && h < ceiling);
    let bracket = match hint {
        Some(hint) => {
            super::web_bracket::from_hint(sizing, hint, minimum, ceiling, feasible, &trial)
        }
        None => super::web_bracket::from_minimum(sizing, minimum, ceiling, feasible, &trial),
    };
    let (lower, lower_web, upper, upper_web) = match bracket {
        Ok(bracket) => bracket,
        // The minimum carries the station, no gauge up to the ceiling does,
        // or the utilization is undefined: the last evaluated section stays,
        // and the margin published for it (positive, negative or NaN)
        // reports which.
        Err(installed) => return installed,
    };
    // Secant steps on the log web utilization against the log gauge (the
    // utilization falls about as a power of the gauge, so the two logs are
    // nearly linear; `bracket::narrow`). Feasibility is the station's peak
    // utilization, as for a bisection; the web's share only orders the trials.
    let (_, upper_end) = super::bracket::narrow(
        End {
            x: lower.ln(),
            residual: -lower_web.ln(),
        },
        End {
            x: upper.ln(),
            residual: -upper_web.ln(),
        },
        |_| WEB_GAUGE_RELATIVE_TOLERANCE.ln_1p(),
        |lower, upper| {
            let (lower, upper, upper_web) = (lower.x.exp(), upper.x.exp(), (-upper.residual).exp());
            upper - lower <= WEB_GAUGE_RELATIVE_TOLERANCE * upper
                || (upper_web >= 1.0 - WEB_GAUGE_RELATIVE_TOLERANCE && upper_web <= feasible)
        },
        |log_gauge| {
            let (peak, web_only) = trial(sizing, log_gauge.exp());
            (peak <= feasible, -web_only.ln())
        },
        100,
    );
    let upper = upper_end.x.exp();
    if !passed.borrow().restore(sizing, station, upper) {
        trial(sizing, upper);
    }
    upper
}

/// Primary-box running mass of one station, kg/m: both cover skins between
/// the outer spars (the width the box mass integrates), the caps of both
/// covers and the webs. The lower-cover fatigue area is added after strength
/// sizing and is not part of it.
fn station_running_mass_kg_m(
    sizing: &WingboxSizing,
    station: usize,
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
) -> f64 {
    let (front, rear) = match (sizing.spar_fracs.first(), sizing.spar_fracs.last()) {
        (Some(front), Some(rear)) => (*front, *rear),
        _ => (0.0, 0.0),
    };
    let covers =
        2.0 * (rear - front) * sizing.chord[station] * sizing.skin_at(station) * skin.rho_kg_m3;
    sizing.spars.iter().fold(covers, |mass, spar| {
        mass + 2.0 * spar.a_cap[station] * cap.rho_kg_m3
            + spar.t_web[station] * spar.h[station] * web.rho_kg_m3
    })
}

/// Size one station's cover skin gauge, with its webs and caps at every trial
/// gauge.
///
/// The gauge is the one of least station running mass, at or above the
/// manufacturing minimum ([`WingboxSizing::t_skin`]), among those at which
/// caps and webs carry the station. Skin and caps share the cover's bending,
/// and the skin alone carries the torsional shear flow: a thicker skin lowers
/// its shear stress, which the skin's combined direct and shear limit
/// ([`super::panel`]) takes out of the direct stress the cover may carry, and
/// adds bending material, while near the thinnest gauge that carries the
/// station the caps grow without bound. The minimum gauge is one of the
/// candidates, so no station is heavier than on the minimum. The search
/// doubles the gauge from the minimum until the mass rises, then refines the
/// bracket by golden section in the log of the gauge; an infeasible trial
/// counts as infinite mass. A metal skin's allowables step down where its
/// gauge leaves the sheet column for the plate column (Niu 1988, Figs. 4.3.3
/// and 4.3.5), so the running mass may jump there; the lightest gauge
/// evaluated is the one kept. The gauge is bounded by half the
/// deepest spar (both covers filling the box). A station that no gauge up to
/// that bound carries keeps the minimum-gauge section, whose negative margin
/// reports it.
#[allow(clippy::too_many_arguments)]
fn size_station_skin(
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
    if station >= sizing.skin_gauge_m.len() {
        size_station(
            sizing, station, moment, shear, cfg, skin, web, cap, size_caps, None,
        );
        return;
    }
    let feasible = 1.0 + super::MARGIN_NUMERICAL_ZERO;
    // The lightest trial so far, so that the search need not repeat it to
    // leave its section installed.
    let lightest = std::cell::RefCell::new((f64::INFINITY, StationRecord::default()));
    // The web gauge of the previous cover-gauge trial: successive trials of
    // the search are close, and so are their web gauges.
    let web_hint = std::cell::Cell::new(None);
    let trial = |sizing: &mut WingboxSizing, gauge: f64| {
        sizing.skin_gauge_m[station] = gauge;
        web_hint.set(Some(size_station(
            sizing,
            station,
            moment,
            shear,
            cfg,
            skin,
            web,
            cap,
            size_caps,
            web_hint.get(),
        )));
        let peak = station_utilizations(sizing, station, moment, shear, skin, web, cap).0;
        if peak <= feasible {
            let mass = station_running_mass_kg_m(sizing, station, skin, web, cap);
            let mut lightest = lightest.borrow_mut();
            if mass < lightest.0 {
                lightest.0 = mass;
                lightest.1.capture(sizing, station, gauge);
            }
            mass
        } else {
            f64::INFINITY
        }
    };
    let minimum = sizing.t_skin;
    let at_minimum = trial(sizing, minimum);
    let ceiling = 0.5
        * sizing
            .spars
            .iter()
            .map(|spar| spar.h[station])
            .fold(0.0_f64, f64::max);
    if !(minimum > 0.0 && ceiling > minimum) {
        trial(sizing, minimum);
        return;
    }
    // Bracket: double until a feasible gauge is followed by a heavier one.
    let mut points = vec![(minimum, at_minimum)];
    let mut gauge = minimum;
    while gauge < ceiling {
        gauge = (2.0 * gauge).min(ceiling);
        let mass = trial(sizing, gauge);
        let rising = points
            .last()
            .is_some_and(|&(_, last)| last.is_finite() && mass >= last);
        points.push((gauge, mass));
        if rising {
            break;
        }
    }
    let Some(best) = (0..points.len())
        .filter(|&index| points[index].1.is_finite())
        .min_by(|&a, &b| points[a].1.total_cmp(&points[b].1))
    else {
        trial(sizing, minimum);
        return;
    };
    let mut retained = points[best];
    let mut lower = points[best.saturating_sub(1)].0.ln();
    let mut upper = points[(best + 1).min(points.len() - 1)].0.ln();
    // Golden section on the log gauge.
    let ratio = 0.5 * (5.0_f64.sqrt() - 1.0);
    let mut inner = upper - ratio * (upper - lower);
    let mut outer = lower + ratio * (upper - lower);
    let mut inner_mass = trial(sizing, inner.exp());
    let mut outer_mass = trial(sizing, outer.exp());
    for (point, mass) in [(inner, inner_mass), (outer, outer_mass)] {
        if mass < retained.1 {
            retained = (point.exp(), mass);
        }
    }
    for _ in 0..200 {
        if upper - lower <= SKIN_GAUGE_LOG_TOLERANCE {
            break;
        }
        if inner_mass < outer_mass {
            upper = outer;
            outer = inner;
            outer_mass = inner_mass;
            inner = upper - ratio * (upper - lower);
            inner_mass = trial(sizing, inner.exp());
            if inner_mass < retained.1 {
                retained = (inner.exp(), inner_mass);
            }
        } else {
            lower = inner;
            inner = outer;
            inner_mass = outer_mass;
            outer = lower + ratio * (upper - lower);
            outer_mass = trial(sizing, outer.exp());
            if outer_mass < retained.1 {
                retained = (outer.exp(), outer_mass);
            }
        }
    }
    if !lightest.borrow().1.restore(sizing, station, retained.0) {
        trial(sizing, retained.0);
    }
}

/// Stations sized by one task of the helper pool. Four keep about fifty tasks
/// on the 200-station default, enough to balance a 32-thread pool, while each
/// task copies the section once.
const STATIONS_PER_TASK: usize = 4;

/// The pool the product law sizes its stations on: `RAYON_NUM_THREADS`
/// threads when set, else one per logical processor; `None` with one thread
/// or when the pool cannot be built, and the stations are then sized in turn.
///
/// The optimizer evaluates every candidate on a one-thread lane; a lane that
/// reaches here waits while the pool sizes its stations, so the pool's threads
/// are the ones doing the work and the lanes the batch leaves idle are used.
fn station_pool() -> Option<&'static rayon::ThreadPool> {
    static POOL: std::sync::OnceLock<Option<rayon::ThreadPool>> = std::sync::OnceLock::new();
    POOL.get_or_init(|| {
        let threads = std::env::var("RAYON_NUM_THREADS")
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok())
            .filter(|&threads| threads > 0)
            .or_else(|| std::thread::available_parallelism().ok().map(usize::from))
            .unwrap_or(1);
        (threads > 1)
            .then(|| {
                rayon::ThreadPoolBuilder::new()
                    .num_threads(threads)
                    .thread_name(|index| format!("alas-station-{index}"))
                    .build()
                    .ok()
            })
            .flatten()
    })
    .as_ref()
}

#[cfg(test)]
thread_local! {
    /// Tests size the stations in turn on this thread to compare the two.
    static SERIAL_STATIONS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Size every station (`size_station_skin`). Each station reads and writes
/// only its own entries of the section, so groups of stations are sized on
/// copies of the section in parallel and their own entries copied back: the
/// result is bit-identical to sizing them in turn.
#[allow(clippy::too_many_arguments)]
fn size_stations(
    sizing: &mut WingboxSizing,
    moments: &[f64],
    shears: &[f64],
    cfg: &StructuresConfig,
    skin: &MaterialSpec,
    web: &MaterialSpec,
    cap: &MaterialSpec,
    size_caps: CapSizer,
) {
    use rayon::prelude::*;
    let count = moments.len().min(shears.len());
    #[cfg(test)]
    let serial = SERIAL_STATIONS.with(std::cell::Cell::get);
    #[cfg(not(test))]
    let serial = false;
    let pool = station_pool().filter(|_| !serial && count > STATIONS_PER_TASK);
    let Some(pool) = pool else {
        for station in 0..count {
            size_station_skin(
                sizing,
                station,
                moments[station],
                shears[station],
                cfg,
                skin,
                web,
                cap,
                size_caps,
            );
        }
        return;
    };
    let snapshot: &WingboxSizing = sizing;
    let sized: Vec<(usize, WingboxSizing)> = pool.install(|| {
        (0..count)
            .step_by(STATIONS_PER_TASK)
            .collect::<Vec<_>>()
            .into_par_iter()
            .map(|start| {
                let mut local = snapshot.clone();
                for station in start..(start + STATIONS_PER_TASK).min(count) {
                    size_station_skin(
                        &mut local,
                        station,
                        moments[station],
                        shears[station],
                        cfg,
                        skin,
                        web,
                        cap,
                        size_caps,
                    );
                }
                (start, local)
            })
            .collect()
    });
    for (start, local) in sized {
        for station in start..(start + STATIONS_PER_TASK).min(count) {
            copy_station(sizing, &local, station);
        }
    }
}

/// Copy what `size_station_skin` writes at `station`: the skin gauge and every
/// spar's cap dimensions and web gauge.
fn copy_station(to: &mut WingboxSizing, from: &WingboxSizing, station: usize) {
    if let (Some(target), Some(&gauge)) = (
        to.skin_gauge_m.get_mut(station),
        from.skin_gauge_m.get(station),
    ) {
        *target = gauge;
    }
    for (target, source) in to.spars.iter_mut().zip(&from.spars) {
        target.w_cap[station] = source.w_cap[station];
        target.t_cap[station] = source.t_cap[station];
        target.a_cap[station] = source.a_cap[station];
        target.t_web[station] = source.t_web[station];
    }
}

/// Cap sizing of one station at a fixed web gauge, with an area hint: the
/// production search (which returns the area it installed), or the plain
/// bisection the tests replay it against (which ignores the hint).
type CapSizer = fn(
    &mut WingboxSizing,
    usize,
    f64,
    &StructuresConfig,
    &MaterialSpec,
    &MaterialSpec,
    &MaterialSpec,
    Option<f64>,
) -> Option<f64>;

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
    size_stations(sizing, moments, shears, cfg, skin, web, cap, size_caps);
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
            // An undefined utilization, or a fatigue area the lower-cover
            // search could not determine, is an undefined margin, never a
            // safe one.
            let undefined =
                ratio.is_nan() || sizing.spars[index].lower_extra_area[station].is_nan();
            sizing.spars[index].margin_of_safety[station] = if undefined {
                f64::NAN
            } else if ratio > 0.0 {
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
            .enumerate()
            .map(|(station, chord)| {
                2.0 * (rear - front) * chord * sizing.skin_at(station) * skin.rho_kg_m3
            })
            .collect::<Vec<_>>(),
        &sizing.y_stations,
    );
    sizing.total_mass_kg = sizing.mass_breakdown_kg.spar_caps
        + sizing.mass_breakdown_kg.spar_webs
        + sizing.mass_breakdown_kg.skin
        + sizing.mass_breakdown_kg.ribs;
}
