// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The metric, outcome and event types the Quick Analysis publishes.

use alas_config::{AlasConfig, DesignVector};
use serde::{Deserialize, Serialize};

use super::payload_range::QuickPayloadRange;
/// The Quick Analysis contract version carried by every event.
pub const QUICK_ANALYSIS_VERSION: u32 = 2;

/// One published estimate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QuickMetric {
    /// Expected takeoff mass from the fixed-geometry closure.
    TakeoffMass,
    /// Operating empty mass.
    OperatingEmptyMass,
    /// Maximum usable fuel capacity.
    FuelCapacity,
    /// Fuel carried for the design mission.
    CarriedFuel,
    /// Structural payload capacity.
    PayloadCapacity,
    /// Payload carried by the declared load case.
    CarriedPayload,
    /// Cruise lift-to-drag ratio.
    CruiseLiftToDrag,
    /// Cruise true airspeed, requested against attainable.
    CruiseSpeed,
    /// Cruise altitude, requested against attainable.
    CruiseAltitude,
    /// Service ceiling.
    ServiceCeiling,
    /// Still-air range of the carried fuel against the requested route.
    Range,
    /// Block fuel of the design mission: the still-air great circle the
    /// closure sizes on.
    FuelBurn,
    /// Block fuel of the planned route (airways where available), flown
    /// off-design by the full analysis' mission stage.
    RouteFuelBurn,
    /// Static margin at the closure centre of gravity.
    StaticMargin,
    /// Payload-range diagram corners.
    PayloadRange,
    /// Supported feasibility flags.
    Feasibility,
}

impl QuickMetric {
    /// Every metric, in display order.
    pub const ALL: [QuickMetric; 16] = [
        Self::TakeoffMass,
        Self::OperatingEmptyMass,
        Self::PayloadCapacity,
        Self::CarriedPayload,
        Self::FuelCapacity,
        Self::CarriedFuel,
        Self::CruiseLiftToDrag,
        Self::CruiseSpeed,
        Self::CruiseAltitude,
        Self::ServiceCeiling,
        Self::Range,
        Self::FuelBurn,
        Self::RouteFuelBurn,
        Self::StaticMargin,
        Self::PayloadRange,
        Self::Feasibility,
    ];

    /// The English display label.
    pub fn label(self) -> &'static str {
        match self {
            Self::TakeoffMass => "Expected takeoff mass",
            Self::OperatingEmptyMass => "Operating empty mass",
            Self::FuelCapacity => "Maximum usable fuel",
            Self::CarriedFuel => "Carried fuel (design mission)",
            Self::PayloadCapacity => "Maximum payload",
            Self::CarriedPayload => "Carried payload",
            Self::CruiseLiftToDrag => "Cruise L/D",
            Self::CruiseSpeed => "Cruise speed",
            Self::CruiseAltitude => "Cruise altitude",
            Self::ServiceCeiling => "Service ceiling",
            Self::Range => "Range",
            Self::FuelBurn => "Block fuel: Design mission (great circle)",
            Self::RouteFuelBurn => "Block fuel: Route",
            Self::StaticMargin => "Static margin",
            Self::PayloadRange => "Payload-range",
            Self::Feasibility => "Feasibility",
        }
    }

    /// The SI unit of the published value (empty for tables and flags).
    pub fn unit(self) -> &'static str {
        match self {
            Self::TakeoffMass
            | Self::OperatingEmptyMass
            | Self::FuelCapacity
            | Self::CarriedFuel
            | Self::PayloadCapacity
            | Self::CarriedPayload
            | Self::FuelBurn
            | Self::RouteFuelBurn => "kg",
            Self::CruiseLiftToDrag => "-",
            Self::CruiseSpeed => "m/s",
            Self::CruiseAltitude | Self::ServiceCeiling | Self::Range => "m",
            Self::StaticMargin => "% MAC",
            Self::PayloadRange | Self::Feasibility => "",
        }
    }

    /// Whether the metric belongs to the initial batch or the extended one.
    pub fn stage(self) -> QuickStage {
        match self {
            Self::CruiseLiftToDrag
            | Self::StaticMargin
            | Self::CruiseSpeed
            | Self::CruiseAltitude
            | Self::ServiceCeiling
            | Self::PayloadRange
            | Self::Feasibility
            | Self::RouteFuelBurn => QuickStage::Extended,
            _ => QuickStage::Initial,
        }
    }

    /// How the published value relates to the sandbox's Full Analysis.
    pub fn basis(self) -> QuickBasis {
        match self {
            Self::TakeoffMass | Self::CarriedFuel | Self::Range | Self::FuelBurn => {
                QuickBasis::ClosureEstimate
            }
            Self::CruiseSpeed | Self::CruiseAltitude | Self::ServiceCeiling => {
                QuickBasis::EnvelopeEstimate
            }
            Self::OperatingEmptyMass
            | Self::FuelCapacity
            | Self::PayloadCapacity
            | Self::CarriedPayload
            | Self::CruiseLiftToDrag
            | Self::StaticMargin
            | Self::RouteFuelBurn
            | Self::PayloadRange
            | Self::Feasibility => QuickBasis::FullAnalysis,
        }
    }
}

/// How a published value relates to the sandbox's Full Analysis, which runs
/// the full baseline analysis of the drawn aircraft at its declared design
/// weights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuickBasis {
    /// The value the Full Analysis computes, by the same function on the same
    /// inputs (equal to the bit on the registered presets).
    FullAnalysis,
    /// A mission-sized closure value. The Full Analysis' fuel model carries
    /// that closure's drag table, trimmed at its converged takeoff mass and
    /// centre of gravity, and its frozen plan
    /// (`alas_opt::mdo::baseline_fuel_artifacts`), so its dispatch over the
    /// same still-air distance lands on the same fixed point: within the
    /// dispatch settling tolerance (`sizing_tolerance_kg`, 1 kg by default)
    /// in takeoff mass, takeoff fuel and block fuel; measured at most 0.5 kg
    /// on the registered presets. The planned route, longer along airways,
    /// is [`QuickMetric::RouteFuelBurn`].
    ClosureEstimate,
    /// A thrust-limited envelope value the Full Analysis does not report:
    /// maximum-climb thrust against the Full Analysis' trimmed drag table at
    /// the closure takeoff mass, one representative mass for the whole
    /// cruise.
    EnvelopeEstimate,
}

/// Which batch a metric is published in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuickStage {
    /// Published from the sized closure, targeted inside the initial-result
    /// latency budget.
    Initial,
    /// Published from the full baseline analysis.
    Extended,
}

/// A scalar estimate with its requested counterpart where one exists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickValue {
    /// The attainable or expected value in SI units.
    pub achieved: f64,
    /// The requested or declared value, when the metric has one.
    pub requested: Option<f64>,
    /// Unit of both numbers.
    pub unit: String,
    /// The assumption or limit that qualifies this number.
    pub note: String,
}

/// One feasibility flag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickFlag {
    /// Stable identifier of the check.
    pub code: String,
    /// Whether the flag fails the design.
    pub blocking: bool,
    /// Human-readable explanation.
    pub message: String,
}

/// The supported feasibility flags.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct QuickFeasibility {
    /// True when no blocking flag was raised.
    pub feasible: bool,
    /// Every raised flag.
    pub flags: Vec<QuickFlag>,
}

/// The planned route flown off-design at the aircraft the closure sized on
/// the design mission, priced by the full analysis' mission stage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickRouteFuel {
    /// Block fuel (taxi-out, trip and taxi-in) of the route's plan, kg.
    pub block_fuel_kg: f64,
    /// Fuel at brake release (trip, reserves and taxi-in) of the route's
    /// plan, kg.
    pub takeoff_fuel_kg: f64,
    /// Takeoff mass the route is flown at, kg.
    pub takeoff_mass_kg: f64,
    /// Fuel the route needs beyond the takeoff-mass and tank limits, kg.
    pub shortfall_kg: f64,
    /// Still-air distance of the planned route, m.
    pub route_distance_m: f64,
    /// Great-circle distance between the route's airports, m.
    pub great_circle_m: f64,
    /// The planner tier that supplied the route (`alas_route` source name).
    pub source: String,
    /// The assumption or limit that qualifies these numbers.
    pub note: String,
}

impl QuickRouteFuel {
    /// Route distance in excess of the great circle, as a fraction (0.19 is
    /// 19 % longer).
    pub fn excess_over_great_circle(&self) -> f64 {
        self.route_distance_m / self.great_circle_m - 1.0
    }
}

/// The terminal state of one metric.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum QuickOutcome {
    /// A scalar estimate.
    Value(QuickValue),
    /// The payload-range corners.
    PayloadRange(QuickPayloadRange),
    /// Feasibility flags.
    Feasibility(QuickFeasibility),
    /// The planned route's fuel.
    Route(QuickRouteFuel),
    /// The model could not produce this metric.
    Failed(String),
    /// The model does not cover this configuration.
    Unsupported(String),
}

/// One streamed result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickEvent {
    /// The configuration revision the request carried.
    pub revision: u64,
    /// Which metric terminated.
    pub metric: QuickMetric,
    /// Its outcome.
    pub outcome: QuickOutcome,
    /// Milliseconds since the request started.
    pub elapsed_ms: u64,
}

/// What a quick analysis runs on.
#[derive(Debug, Clone, PartialEq)]
pub struct QuickAnalysisRequest {
    /// The complete configuration of the drawn aircraft.
    pub config: AlasConfig,
    /// The design vector that positions and sizes the lifting surfaces.
    pub design: DesignVector,
    /// Monotonic identity of the configuration state.
    pub revision: u64,
}

/// Timing and completion summary of one run.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct QuickAnalysisSummary {
    /// Metrics that reached a terminal state.
    pub published: usize,
    /// Milliseconds when the last initial-stage metric terminated.
    pub initial_stage_ms: u64,
    /// Milliseconds when the last metric terminated.
    pub final_ms: u64,
    /// Whether the run stopped early on request.
    pub cancelled: bool,
}
