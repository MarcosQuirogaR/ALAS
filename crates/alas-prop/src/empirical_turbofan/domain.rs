// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Evidence domain of the empirical turbofan deck: the bypass-ratio, Mach
//! number and pressure-height ranges printed on the Jenkinson charts each
//! rating reads (Civil Jet Aircraft Design, Figs. 9.18-9.21). Inside them a
//! point is interpolated between chart readings; outside, the tables hold
//! their end values (or scale with pressure), which is an extrapolation and
//! is reported as such.

use super::*;

/// Bypass ratios of the lowest and highest chart, [min, max].
const CHART_BYPASS_RATIO: [f64; 2] = [3.0, 13.0];
/// Mach numbers of the maximum-climb chart samples, [min, max].
const CLIMB_CHART_MACH: [f64; 2] = [0.3, 0.9];
/// Highest printed maximum-climb and flight-idle curve, ft.
const CLIMB_CHART_MAXIMUM_HEIGHT_FT: f64 = 45_000.0;
/// Mach numbers of the take-off chart samples, [min, max].
const TAKEOFF_CHART_MACH: [f64; 2] = [0.0, 0.4];
/// Highest printed take-off curve, ft.
const TAKEOFF_CHART_MAXIMUM_HEIGHT_FT: f64 = 20_000.0;
/// Mach numbers of the flight-idle net-thrust chart samples, [min, max].
const IDLE_CHART_MACH: [f64; 2] = [0.2, 0.8];
/// Relative slack on the pressure-ratio bounds, so that a point exactly on
/// the sea-level or top curve is not moved outside by rounding.
const PRESSURE_RATIO_SLACK: f64 = 1.0e-9;

/// The chart family one rating is read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChartRead {
    /// Maximum-climb charts, Figs. 9.18(b)-9.21(b).
    MaximumClimb,
    /// Take-off charts, Figs. 9.18(a)-9.21(a).
    Takeoff,
    /// Descent (flight-idle) charts.
    FlightIdle,
}

impl ChartRead {
    fn label(self) -> &'static str {
        match self {
            Self::MaximumClimb => "maximum-climb",
            Self::Takeoff => "take-off",
            Self::FlightIdle => "flight-idle",
        }
    }

    fn mach_range(self) -> [f64; 2] {
        match self {
            Self::MaximumClimb => CLIMB_CHART_MACH,
            Self::Takeoff => TAKEOFF_CHART_MACH,
            Self::FlightIdle => IDLE_CHART_MACH,
        }
    }

    fn maximum_height_ft(self) -> f64 {
        match self {
            Self::MaximumClimb | Self::FlightIdle => CLIMB_CHART_MAXIMUM_HEIGHT_FT,
            Self::Takeoff => TAKEOFF_CHART_MAXIMUM_HEIGHT_FT,
        }
    }
}

/// The chart families a demand reads, in evaluation order. A maximum-
/// continuous demand has no chart of its own (it is held on the
/// maximum-climb envelope) and is reported separately.
pub(super) fn charts_read(
    demand: PropulsionDemand,
    idle_floor_active: bool,
) -> &'static [ChartRead] {
    let rating = match demand {
        PropulsionDemand::Rating(rating) | PropulsionDemand::RatedFraction { rating, .. } => rating,
        _ => {
            return if idle_floor_active {
                &[ChartRead::MaximumClimb, ChartRead::FlightIdle]
            } else {
                &[ChartRead::MaximumClimb]
            };
        }
    };
    match rating {
        PropulsionRating::TakeoffGoAround => &[ChartRead::Takeoff],
        PropulsionRating::FlightIdle => &[ChartRead::FlightIdle],
        _ => &[ChartRead::MaximumClimb],
    }
}

/// Pressure ratio of the highest printed curve of `chart` (US 1976).
fn lowest_pressure_ratio(chart: ChartRead) -> f64 {
    static RATIOS: std::sync::OnceLock<[f64; 2]> = std::sync::OnceLock::new();
    let ratios = RATIOS.get_or_init(|| {
        [
            delta_at_feet(CLIMB_CHART_MAXIMUM_HEIGHT_FT),
            delta_at_feet(TAKEOFF_CHART_MAXIMUM_HEIGHT_FT),
        ]
    });
    match chart {
        ChartRead::MaximumClimb | ChartRead::FlightIdle => ratios[0],
        ChartRead::Takeoff => ratios[1],
    }
}

impl EmpiricalTurbofanModel {
    /// Validity of one evaluated point: [ValidityStatus::Valid] when the
    /// engine bypass ratio and the flight point lie inside every chart in
    /// `charts`, otherwise [ValidityStatus::Extrapolated] naming each
    /// exceeded bound. `unsourced_rating` marks a rating without a chart of
    /// its own.
    pub(super) fn chart_validity(
        &self,
        flight: FlightCondition,
        charts: &[ChartRead],
        unsourced_rating: bool,
    ) -> ValidityStatus {
        let mut exceeded: Vec<String> = Vec::new();
        let bpr = self.deck.bypass_ratio;
        if !(CHART_BYPASS_RATIO[0]..=CHART_BYPASS_RATIO[1]).contains(&bpr) {
            exceeded.push(format!(
                "bypass ratio {bpr:.2} is outside the charts' {:.0}-{:.0} (nearest chart held)",
                CHART_BYPASS_RATIO[0], CHART_BYPASS_RATIO[1]
            ));
        }
        if unsourced_rating {
            exceeded
                .push("no maximum-continuous chart: the maximum-climb envelope is used".to_owned());
        }
        let delta = flight.pressure_pa / 101_325.0;
        for chart in charts {
            let [mach_low, mach_high] = chart.mach_range();
            if !(mach_low..=mach_high).contains(&flight.mach) {
                exceeded.push(format!(
                    "Mach {:.3} is outside the {} charts' {mach_low:.1}-{mach_high:.1}",
                    flight.mach,
                    chart.label()
                ));
            }
            let delta_low = lowest_pressure_ratio(*chart);
            if !(delta_low * (1.0 - PRESSURE_RATIO_SLACK)..=1.0 + PRESSURE_RATIO_SLACK)
                .contains(&delta)
            {
                exceeded.push(format!(
                    "pressure ratio {delta:.4} is outside the {} charts' sea level to {:.0} ft",
                    chart.label(),
                    chart.maximum_height_ft()
                ));
            }
        }
        if exceeded.is_empty() {
            ValidityStatus::Valid
        } else {
            ValidityStatus::Extrapolated {
                reason: exceeded.join("; "),
            }
        }
    }
}
