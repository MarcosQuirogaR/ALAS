// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Hardware roles, selections and workflow modes for the UAV page.

use alas_uav::optimizer::{NoFeasibleDesign, OptimizationProgress, OptimizedUav};
use alas_uav::{SharedCoreFailure, SharedCoreVerification};

/// One required hardware role in the mixed discrete search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentRole {
    Battery,
    Motor,
    Esc,
    Propeller,
    Servo,
    Material,
    Receiver,
    Electronics,
    LandingGear,
}

impl ComponentRole {
    /// All roles required for a complete generated aircraft.
    pub const ALL: [Self; 9] = [
        Self::Battery,
        Self::Motor,
        Self::Esc,
        Self::Propeller,
        Self::Servo,
        Self::Material,
        Self::Receiver,
        Self::Electronics,
        Self::LandingGear,
    ];

    /// Stable English label passed through the GUI translation boundary.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Battery => "Battery",
            Self::Motor => "Motor",
            Self::Esc => "Speed controller",
            Self::Propeller => "Propeller",
            Self::Servo => "Servo",
            Self::Material => "Structural material",
            Self::Receiver => "Receiver",
            Self::Electronics => "Mission electronics",
            Self::LandingGear => "Landing gear",
        }
    }
}

/// Stable catalogue identifiers selected for every required role.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UavSelections {
    pub battery: String,
    pub motor: String,
    pub esc: String,
    pub propeller: String,
    pub servo: String,
    pub material: String,
    pub receiver: String,
    pub electronics: String,
    pub landing_gear: String,
}

impl UavSelections {
    /// Read the selected stable id for a role.
    pub fn get(&self, role: ComponentRole) -> &str {
        match role {
            ComponentRole::Battery => &self.battery,
            ComponentRole::Motor => &self.motor,
            ComponentRole::Esc => &self.esc,
            ComponentRole::Propeller => &self.propeller,
            ComponentRole::Servo => &self.servo,
            ComponentRole::Material => &self.material,
            ComponentRole::Receiver => &self.receiver,
            ComponentRole::Electronics => &self.electronics,
            ComponentRole::LandingGear => &self.landing_gear,
        }
    }

    /// Replace one selected stable id.
    pub fn set(&mut self, role: ComponentRole, id: String) {
        match role {
            ComponentRole::Battery => self.battery = id,
            ComponentRole::Motor => self.motor = id,
            ComponentRole::Esc => self.esc = id,
            ComponentRole::Propeller => self.propeller = id,
            ComponentRole::Servo => self.servo = id,
            ComponentRole::Material => self.material = id,
            ComponentRole::Receiver => self.receiver = id,
            ComponentRole::Electronics => self.electronics = id,
            ComponentRole::LandingGear => self.landing_gear = id,
        }
    }
}

/// Whether propulsion data comes from selected hardware or manually entered evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PropulsionInputMode {
    /// Resolve the battery, motor, ESC, and APC table without manual points.
    #[default]
    Automatic,
    /// Retain a manual two-point map for an externally reviewed special case.
    AdvancedManual,
}

impl PropulsionInputMode {
    /// Short label suitable for a desktop selection control.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Automatic => "Automatic catalogue solver",
            Self::AdvancedManual => "Advanced manual map",
        }
    }
}

/// Whether the electrical mission is generated from the brief or entered in
/// the advanced phase editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MissionPlanMode {
    /// Generate a four-phase plan that satisfies the stated endurance and range.
    #[default]
    Standard,
    /// Use the designer's ordered steady flight phases.
    Advanced,
}

impl MissionPlanMode {
    /// Short label suitable for a desktop selection control.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Standard => "Standard four-phase mission",
            Self::Advanced => "Advanced custom phases",
        }
    }
}

/// Latest typed workflow outcome.
#[derive(Debug, Clone, PartialEq)]
pub enum UavWorkflowOutcome {
    /// Inputs have not been evaluated.
    NotRun,
    /// The mixed search and every preliminary feasibility check passed.
    /// Shared-core lift verification remains a distinct product verdict.
    PreliminaryFeasible {
        optimized: Box<OptimizedUav>,
        shared_core: Result<Box<SharedCoreVerification>, SharedCoreFailure>,
    },
    /// No candidate passed; counts retain every physical failure family.
    NoFeasibleDesign(Box<NoFeasibleDesign>),
}

/// Transient worker state kept separate from the last completed outcome.
#[derive(Debug, Clone, PartialEq)]
pub enum UavExecutionStatus {
    /// No search has been requested in this app session.
    Idle,
    /// A background worker is evaluating deterministic candidates.
    Running(OptimizationProgress),
    /// Cancellation has been requested and will be observed between candidates.
    CancelRequested(OptimizationProgress),
    /// A search completed with either an accepted design or typed rejections.
    Completed,
    /// A worker stopped cooperatively; the last completed outcome is retained.
    Cancelled {
        /// Candidates completed before the worker observed cancellation.
        evaluated_candidates: usize,
    },
    /// Input validation or worker execution failed; prior output is retained.
    Failed(String),
}

/// The current task-oriented section of the UAV sizing workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UavSection {
    /// Mission constraints and payload requirements.
    #[default]
    Mission,
    /// Reviewed component catalogue selections.
    Hardware,
    /// Measured propulsion operating-point evidence.
    Propulsion,
    /// Topology selection, geometry search, and preliminary model assumptions.
    Airframe,
    /// Production-core VLM comparison inputs.
    Verification,
    /// Latest optimization and verification result.
    Results,
}

impl UavSection {
    /// Short label suitable for the workflow tabs.
    pub const fn short_title(self) -> &'static str {
        match self {
            Self::Mission => "Mission",
            Self::Hardware => "Hardware",
            Self::Propulsion => "Propulsion",
            Self::Airframe => "Airframe",
            Self::Verification => "Verification",
            Self::Results => "Results",
        }
    }

    /// Stable title matching the desktop translation catalog.
    pub const fn title(self) -> &'static str {
        match self {
            Self::Mission => "Mission objectives",
            Self::Hardware => "Reviewed component catalogue",
            Self::Propulsion => "Propulsion map evidence",
            Self::Airframe => "Geometry search and preliminary model",
            Self::Verification => "Shared production-core verification",
            Self::Results => "UAV result",
        }
    }

    /// Explain the purpose of a section before its inputs are shown.
    pub const fn description(self) -> &'static str {
        match self {
            Self::Mission => "Set the flight objectives and payload envelope the aircraft must carry.",
            Self::Hardware => "Choose reviewed component records; missing catalogue values remain unknown.",
            Self::Propulsion => "Resolve a source-bounded electric powertrain and its multi-phase mission from the selected hardware.",
            Self::Airframe => "Choose a design convention, then set the geometry and preliminary-model bounds.",
            Self::Verification => "Choose the airfoils and operating point for the independent production-core VLM check.",
            Self::Results => "Review the selected design, physical findings, provenance, and shared-core comparison.",
        }
    }

    /// Ordered sections reflecting the evidence dependency of a run.
    pub const ALL: [Self; 6] = [
        Self::Mission,
        Self::Hardware,
        Self::Propulsion,
        Self::Airframe,
        Self::Verification,
        Self::Results,
    ];
}
