// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The long-range cruise speed of a flown state.
//!
//! Long-range cruise (LRC) is the speed above the maximum-specific-range
//! speed at which the specific range has fallen to 99 % of its maximum: the
//! definition Airbus gives in "Getting to grips with aircraft performance"
//! (Airbus Customer Services, Flight Operations Support, 2002, cruise
//! chapter) and Boeing in its fuel-conservation articles (Boeing AERO
//! magazine QTR_04.07, "Fuel Conservation Strategies: Cruise Flight"). The
//! 1 % of range given up buys a speed well above the maximum-range speed
//! where the specific-range curve is flat. A published range quoted "at long-range
//! cruise" is flown at this speed, not at the configured cruise speed.
//!
//! The specific air range is the mission model's own (`TAS / fuel flow` on
//! its trimmed polar and propulsion deck), so the speed found is the one the
//! flown mission would see as LRC, and it is searched only among speeds that
//! qualify as cruise (residual climb, thrust and buffet ceilings).

use super::SegmentMissionModel;

/// Share of the maximum specific range that defines long-range cruise.
pub const LONG_RANGE_CRUISE_SR_FRACTION: f64 = 0.99;

/// Scan resolution of the speed search (engineering choice; the maximum is
/// then refined, so the scan only has to bracket it).
const SCAN_POINTS: usize = 240;

/// Search window around the reference speed.
const WINDOW: (f64, f64) = (0.45, 1.6);

/// Bisection steps of the refinements (speed resolution well below 1e-6 of
/// the window).
const REFINEMENTS: usize = 60;

/// The maximum-range and long-range cruise speeds of one state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LongRangeCruise {
    /// True airspeed of maximum specific range, m/s.
    pub max_range_tas_m_s: f64,
    /// Maximum specific air range, m/kg.
    pub max_specific_range_m_kg: f64,
    /// Long-range cruise true airspeed (99 % of the maximum, high-speed
    /// side), m/s.
    pub tas_m_s: f64,
    /// Specific air range at `tas_m_s`, m/kg.
    pub specific_range_m_kg: f64,
}

impl SegmentMissionModel {
    /// The long-range cruise speed at `mass_kg` and `altitude_m`, searched
    /// within `0.45 .. 1.6` of `reference_tas_m_s` among speeds that qualify
    /// as level cruise.
    ///
    /// # Errors
    ///
    /// The reason, as text, when no speed in the window can be priced and
    /// qualifies.
    pub fn long_range_cruise(
        &self,
        mass_kg: f64,
        altitude_m: f64,
        reference_tas_m_s: f64,
    ) -> Result<LongRangeCruise, String> {
        if !(reference_tas_m_s.is_finite() && reference_tas_m_s > 0.0) {
            return Err(format!(
                "reference speed {reference_tas_m_s} m/s is unusable"
            ));
        }
        let sr = |tas: f64| -> Option<f64> {
            if !self.level_qualifies(mass_kg, altitude_m, tas) {
                return None;
            }
            self.specific_air_range_m_kg(mass_kg, altitude_m, tas)
                .ok()
                .filter(|v| v.is_finite() && *v > 0.0)
        };
        let (lo, hi) = (WINDOW.0 * reference_tas_m_s, WINDOW.1 * reference_tas_m_s);
        let step = (hi - lo) / SCAN_POINTS as f64;
        let grid: Vec<(f64, Option<f64>)> = (0..=SCAN_POINTS)
            .map(|i| {
                let tas = lo + step * i as f64;
                (tas, sr(tas))
            })
            .collect();
        let (peak_index, _) = grid
            .iter()
            .enumerate()
            .filter_map(|(i, (_, v))| v.map(|v| (i, v)))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .ok_or_else(|| {
                format!(
                    "no cruise speed between {lo:.0} and {hi:.0} m/s qualifies at {altitude_m:.0} m and {mass_kg:.0} kg"
                )
            })?;
        // Golden-section refinement of the maximum between the neighbours of
        // the best grid point (unpriced neighbours bound the bracket at the
        // best point itself).
        let bracket_lo = if peak_index > 0 && grid[peak_index - 1].1.is_some() {
            grid[peak_index - 1].0
        } else {
            grid[peak_index].0
        };
        let bracket_hi = if peak_index < SCAN_POINTS && grid[peak_index + 1].1.is_some() {
            grid[peak_index + 1].0
        } else {
            grid[peak_index].0
        };
        let (mut a, mut b) = (bracket_lo, bracket_hi);
        let phi = 0.5 * (5.0_f64.sqrt() - 1.0);
        for _ in 0..REFINEMENTS {
            if b - a < 1.0e-9 {
                break;
            }
            let (c, d) = (b - phi * (b - a), a + phi * (b - a));
            if sr(c).unwrap_or(0.0) > sr(d).unwrap_or(0.0) {
                b = d;
            } else {
                a = c;
            }
        }
        let refined = 0.5 * (a + b);
        let grid_peak = (grid[peak_index].0, grid[peak_index].1.unwrap_or(0.0));
        let (max_tas, max_sr) = match sr(refined) {
            Some(v) if v >= grid_peak.1 => (refined, v),
            _ => grid_peak,
        };
        let target = LONG_RANGE_CRUISE_SR_FRACTION * max_sr;
        // First grid speed above the maximum whose range is below the
        // target (an unpriced or non-qualifying speed ends the search too:
        // the aircraft cannot fly it).
        let mut inside = max_tas;
        let mut outside = None;
        for &(tas, value) in grid.iter().filter(|(tas, _)| *tas > max_tas) {
            match value {
                Some(v) if v >= target => inside = tas,
                _ => {
                    outside = Some(tas);
                    break;
                }
            }
        }
        let lrc = match outside {
            Some(mut out) => {
                let mut inn = inside;
                for _ in 0..REFINEMENTS {
                    let mid = 0.5 * (inn + out);
                    if sr(mid).is_some_and(|v| v >= target) {
                        inn = mid;
                    } else {
                        out = mid;
                    }
                }
                inn
            }
            // The window ends first: the fastest speed of the window.
            None => inside,
        };
        let lrc_sr =
            sr(lrc).ok_or_else(|| "long-range cruise speed cannot be priced".to_owned())?;
        Ok(LongRangeCruise {
            max_range_tas_m_s: max_tas,
            max_specific_range_m_kg: max_sr,
            tas_m_s: lrc,
            specific_range_m_kg: lrc_sr,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_definition_is_a_share_of_the_maximum() {
        assert!((LONG_RANGE_CRUISE_SR_FRACTION - 0.99).abs() < f64::EPSILON);
    }
}
