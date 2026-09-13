// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The metric, outcome and event types the reduced analysis publishes.

use alas_config::{AlasConfig, DesignVector};
use serde::{Deserialize, Serialize};

use super::breguet::QuickPayloadRange;
/// The reduced-model contract version carried by every event.
pub const QUICK_ANALYSIS_VERSION: u32 = 1;

/// One published estimate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QuickMetric {
    /// Expected takeoff mass from the fixed-geometry closure.
    TakeoffMass,
    /// Operating empty mass.
    OperatingEmptyMass,
    /// Maximum usable fuel capacity.
    FuelCapacity,
    /// Fuel carried for the declared route.
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
    /// Range, requested route against Breguet estimate.
    Range,
    /// Block fuel for the declared route.
    FuelBurn,
    /// Static margin at the closure centre of gravity.
    StaticMargin,
    /// Payload-range diagram corners.
    PayloadRange,
    /// Supported feasibility flags.
    Feasibility,
}

impl QuickMetric {
    /// Every metric, in display order.
    pub const ALL: [QuickMetric; 15] = [
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
            Self::CarriedFuel => "Carried fuel (route)",
            Self::PayloadCapacity => "Maximum payload",
            Self::CarriedPayload => "Carried payload",
            Self::CruiseLiftToDrag => "Cruise L/D",
            Self::CruiseSpeed => "Cruise speed",
            Self::CruiseAltitude => "Cruise altitude",
            Self::ServiceCeiling => "Service ceiling",
            Self::Range => "Range",
            Self::FuelBurn => "Block fuel",
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
            | Self::FuelBurn => "kg",
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
            Self::CruiseSpeed
            | Self::CruiseAltitude
            | Self::ServiceCeiling
            | Self::PayloadRange
            | Self::Feasibility => QuickStage::Extended,
            _ => QuickStage::Initial,
        }
    }
}

/// Which batch a metric is published in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuickStage {
    /// Published from the sized closure, targeted inside the initial-result
    /// latency budget.
    Initial,
    /// Published after the reduced full analysis.
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

/// The terminal state of one metric.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum QuickOutcome {
    /// A scalar estimate.
    Value(QuickValue),
    /// The payload-range corners.
    PayloadRange(QuickPayloadRange),
    /// Feasibility flags.
    Feasibility(QuickFeasibility),
    /// The reduced model could not produce this metric.
    Failed(String),
    /// The reduced model does not cover this configuration.
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
