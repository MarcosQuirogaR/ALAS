// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Cap-area search: bracket the capacity root by doubling, close the floored
//! plateau with one trial at its edge, then narrow the bracket by secant steps
//! (`super::bracket`) to [`CAP_AREA_RELATIVE_TOLERANCE`], keeping the
//! feasible end.

use super::law::station_cap_dimensions;
use super::panel::CoverPanel;
use super::section::{cap_inertia, non_cap_ei};
use super::WingboxSizing;
use crate::allowables::bending_allowable_pa;
use alas_config::materials::MaterialSpec;

/// Relative width of the cap-area bracket at which the search stops; the
/// feasible end is kept.
///
/// A numerical bound, not a physical margin. 1e-9 of a cap area is far below
/// any fabrication tolerance (a 2 000 mm² cap to 2e-6 mm²) and four orders under
/// the relieved-load fixed-point tolerance (`RELIEF_TOLERANCE`, 1e-5 of the box
/// mass), so the fixed point never sees the step.
pub(super) const CAP_AREA_RELATIVE_TOLERANCE: f64 = 1.0e-9;

/// First relative step of the bracket grown around a cap-area hint; each
/// further step is four times the last.
const HINT_STEP: f64 = 2.0e-3;

/// A cap-area bracket: `(lower, its utilization, upper, its utilization)`,
/// failing at `lower` and passing at `upper`.
type Bracket = (f64, f64, f64, f64);

pub(super) struct CapSearch<'a> {
    section: &'a WingboxSizing,
    station: usize,
    moment: f64,
    minimum_gauge: f64,
    height: f64,
    /// Twice the cover's extreme-fibre distance: the contour depth where recorded
    /// ([`WingboxSizing::cover_depth_at`]), never below the deepest spar.
    cover_height: f64,
    chord: f64,
    non_cap: f64,
    panel: CoverPanel,
    cap_limit: f64,
    skin_limit: f64,
    skin_modulus: f64,
    cap_modulus: f64,
    web_modulus: f64,
    web_allowable: f64,
}

impl<'a> CapSearch<'a> {
    pub(super) fn new(
        section: &'a WingboxSizing,
        station: usize,
        moment: f64,
        minimum_gauge: f64,
        skin: &MaterialSpec,
        web: &MaterialSpec,
        cap: &MaterialSpec,
    ) -> Self {
        let height = section
            .spars
            .iter()
            .map(|spar| spar.h[station])
            .fold(0.0_f64, f64::max);
        Self {
            section,
            station,
            moment: moment.abs(),
            minimum_gauge,
            height,
            cover_height: height.max(section.cover_depth_at(station)),
            chord: section.chord[station],
            non_cap: non_cap_ei(section, station, skin, web),
            panel: CoverPanel::new(section, station, skin),
            cap_limit: bending_allowable_pa(cap),
            skin_limit: bending_allowable_pa(skin),
            skin_modulus: skin.e_pa,
            cap_modulus: cap.e_pa,
            web_modulus: web.e_pa,
            web_allowable: bending_allowable_pa(web),
        }
    }

    /// Combined utilization of the station with caps of total area `area`
    /// installed by the fabrication law.
    fn utilization(&self, area: f64) -> f64 {
        let mut stiffness = 0.0;
        let mut clear_height = 0.0_f64;
        let mut flange = 0.0;
        for spar in &self.section.spars {
            let height = spar.h[self.station];
            let requested = if self.height > 0.0 {
                area * height / self.height
            } else {
                0.0
            };
            let (width, thickness) =
                station_cap_dimensions(requested, self.chord, height, self.minimum_gauge);
            stiffness += self.cap_modulus * cap_inertia(height, width, thickness);
            if height > 0.0 {
                flange += width * thickness;
            }
            let clear = (height - 2.0 * thickness).max(0.0);
            if clear > clear_height {
                clear_height = clear;
            }
        }
        let ei = self.non_cap + stiffness;
        let web_lever = self.web_modulus * clear_height / self.web_allowable;
        // The installed flanges are part of the stiffened cover whose
        // compressive allowable they raise, so the cover lever arm shrinks as
        // caps are added and the search predicate stays monotone.
        // The caps sit at the spar height; the cover's extreme fibre follows the
        // contour between the spars, as in `section::stress_utilization`.
        let cap_cover = super::section::max_or_nan(
            self.cap_modulus
                / self
                    .panel
                    .allowable_for_pa(self.cap_modulus, self.cap_limit, flange)
                * self.height,
            self.skin_modulus
                / self
                    .panel
                    .skin_allowable_for_pa(self.skin_modulus, self.skin_limit, flange)
                * self.cover_height,
        );
        let lever = super::section::max_or_nan(cap_cover, web_lever);
        0.5 * (self.moment / ei) * lever
    }

    /// The least total cap area that carries the station: zero where no
    /// caps are needed, else a bracket doubled from the floor area, or grown
    /// around `hint` (the area of a nearby trial of the same station) when it
    /// is a positive area below the ceiling, by steps growing fourfold from
    /// [`HINT_STEP`] relative. The bracket then closes by the same plateau
    /// trial and narrowing to the same tolerance, so the hint moves the
    /// returned area only within [`CAP_AREA_RELATIVE_TOLERANCE`]; a station
    /// that needs no caps still returns zero first.
    pub(super) fn area_from(&self, hint: Option<f64>) -> f64 {
        let at_zero = self.utilization(0.0);
        if at_zero <= 1.0 || at_zero.is_nan() {
            return 0.0;
        }
        let maximum = self
            .section
            .spars
            .iter()
            .map(|spar| 0.1 * self.chord * spar.h[self.station])
            .fold(0.0_f64, f64::max);
        let hinted = hint
            .filter(|&hint| hint.is_finite() && hint > 0.0 && hint < maximum)
            .and_then(|hint| self.bracket_around(hint, at_zero, maximum));
        let (mut lower, mut lower_utilization, mut upper, mut upper_utilization) =
            match hinted.unwrap_or_else(|| self.bracket_from_floor(at_zero, maximum)) {
                Ok(bracket) => bracket,
                Err(installed) => return installed,
            };
        // Below the floor edge every spar installs its floored flange, the
        // same section whatever the area, so the utilization there is flat
        // and tells the secant nothing; the bracket would have to bisect its
        // way across. One trial at the edge closes that part: by the
        // monotone predicate it is a valid lower end where it fails and a
        // valid upper end where it passes.
        if let Some(edge) = self
            .floor_edge()
            .filter(|&edge| edge > lower && edge < upper)
        {
            let at_edge = self.utilization(edge);
            if at_edge > 1.0 {
                (lower, lower_utilization) = (edge, at_edge);
            } else if at_edge <= 1.0 {
                (upper, upper_utilization) = (edge, at_edge);
            }
        }
        refine(lower, lower_utilization, upper, upper_utilization, |area| {
            self.utilization(area)
        })
    }

    /// The cold bracket, doubling from the floor area: `(lower, its
    /// utilization, upper, its utilization)`, or the area to return when the
    /// doubling budget runs out without a passing area.
    fn bracket_from_floor(&self, at_zero: f64, maximum: f64) -> Result<Bracket, f64> {
        let mut lower = 0.0;
        let mut lower_utilization = at_zero;
        let mut upper = (self.minimum_gauge * self.chord).max(1.0e-12).min(maximum);
        let mut upper_utilization = f64::NAN;
        let mut installed_upper = upper;
        for _ in 0..64 {
            installed_upper = upper;
            upper_utilization = self.utilization(upper);
            if upper_utilization <= 1.0 || upper >= maximum {
                break;
            }
            lower = upper;
            lower_utilization = upper_utilization;
            upper = (2.0 * upper).min(maximum);
        }
        if upper_utilization > 1.0 || upper_utilization.is_nan() {
            // Exhausting the doubling budget retains the last evaluated
            // section, as did the original in-place cap construction.
            return Err(installed_upper);
        }
        Ok((lower, lower_utilization, upper, upper_utilization))
    }

    /// The warm bracket around `hint`; `None` (the cold bracket decides)
    /// where a trial is undefined or the steps run out. Zero, which fails,
    /// bounds the downward steps; the ceiling bounds the upward ones, and a
    /// failing ceiling is returned as the cold doubling returns it.
    fn bracket_around(
        &self,
        hint: f64,
        at_zero: f64,
        maximum: f64,
    ) -> Option<Result<Bracket, f64>> {
        let at_hint = self.utilization(hint);
        if at_hint.is_nan() {
            return None;
        }
        let mut step = HINT_STEP;
        if at_hint <= 1.0 {
            let (mut upper, mut upper_utilization) = (hint, at_hint);
            for _ in 0..32 {
                let lower = upper / (1.0 + step);
                let at_lower = self.utilization(lower);
                if at_lower.is_nan() {
                    return None;
                }
                if at_lower > 1.0 {
                    return Some(Ok((lower, at_lower, upper, upper_utilization)));
                }
                (upper, upper_utilization) = (lower, at_lower);
                step *= 4.0;
            }
            return Some(Ok((0.0, at_zero, upper, upper_utilization)));
        }
        let (mut lower, mut lower_utilization) = (hint, at_hint);
        for _ in 0..32 {
            let upper = (lower * (1.0 + step)).min(maximum);
            let at_upper = self.utilization(upper);
            if at_upper.is_nan() {
                return None;
            }
            if at_upper <= 1.0 {
                return Some(Ok((lower, lower_utilization, upper, at_upper)));
            }
            if upper >= maximum {
                return Some(Err(upper));
            }
            (lower, lower_utilization) = (upper, at_upper);
            step *= 4.0;
        }
        None
    }

    /// The least total cap area at which some spar's flange reaches its
    /// floor thickness ([`station_cap_dimensions`]): below it every spar's
    /// flange is floored. `None` without a spar with depth and a positive
    /// floor. This mirrors the fabrication law's flange width at the floor
    /// (`law::root_cap_dimensions` while the flange is thinner than its
    /// thickness limit); the search only places a trial there, so an edge
    /// that misses the true one costs a trial, never the result's validity.
    fn floor_edge(&self) -> Option<f64> {
        if self.height <= 0.0 || self.height.is_nan() {
            return None;
        }
        self.section
            .spars
            .iter()
            .filter_map(|spar| {
                let height = spar.h[self.station];
                let floor = self.minimum_gauge.min(height / 3.0).max(0.0);
                let width = (0.5 * self.chord).max(1e-6).min(0.6 * height).max(1e-6);
                (height > 0.0 && floor > 0.0).then(|| width * floor * self.height / height)
            })
            .filter(|edge| edge.is_finite() && *edge > 0.0)
            .min_by(f64::total_cmp)
    }
}

/// Narrow the cap-area bracket `[lower, upper]` (utilization above 1 at
/// `lower`, at most 1 at `upper`) to [`CAP_AREA_RELATIVE_TOLERANCE`] of
/// `upper` and return its feasible end.
///
/// The residual is the reciprocal utilization `1/u - 1`: the utilization is
/// the moment over the section stiffness times a lever arm, and the stiffness
/// is affine in the cap area, so the residual is nearly linear in the area
/// and secant steps ([`super::bracket::narrow`]) reach the capacity root in a
/// few trials. Feasibility is the same `utilization <= 1` predicate a
/// bisection would use, and the feasible end is returned.
fn refine(
    lower: f64,
    lower_utilization: f64,
    upper: f64,
    upper_utilization: f64,
    utilization: impl Fn(f64) -> f64,
) -> f64 {
    use super::bracket::{narrow, End};
    let residual = |utilization: f64| 1.0 / utilization - 1.0;
    let (_, upper) = narrow(
        End {
            x: lower,
            residual: residual(lower_utilization),
        },
        End {
            x: upper,
            residual: residual(upper_utilization),
        },
        |upper| CAP_AREA_RELATIVE_TOLERANCE * upper,
        |lower, upper| upper.x - lower.x <= CAP_AREA_RELATIVE_TOLERANCE * upper.x,
        |area| {
            let value = utilization(area);
            (value <= 1.0 || value.is_nan(), residual(value))
        },
        200,
    );
    upper.x
}
