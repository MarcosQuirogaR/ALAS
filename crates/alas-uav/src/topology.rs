// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Typed fixed-wing topology boundaries for the UAV workflow.
//!
//! Tail naming alone is not an engineering model. A V-tail needs a mixer and
//! coupled pitch/yaw authority, while a flying wing needs elevon, reflex, and
//! longitudinal-control geometry. This module makes those distinctions
//! inspectable before a caller starts a preliminary sizing or product-verdict
//! run. Geometry and inviscid lifting-surface assessment can represent every
//! listed topology; only paths whose control and sizing assumptions are
//! represented are made available.

/// Fixed-wing arrangement selected for a UAV study.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum UavTopology {
    /// Separate horizontal and vertical tail surfaces on the body centerline.
    #[default]
    ConventionalTail,
    /// A horizontal tail mounted at the vertical-tail tip.
    TTail,
    /// Two inclined surfaces with coupled pitch and yaw control.
    VTail,
    /// A tailless planform whose pitch and roll authority comes from elevons.
    FlyingWing,
}

impl UavTopology {
    /// Every topology exposed by the native UAV contract, in UI display order.
    pub const ALL: [Self; 4] = [
        Self::ConventionalTail,
        Self::TTail,
        Self::VTail,
        Self::FlyingWing,
    ];

    /// Short human-readable label suitable for a selector.
    pub const fn label(self) -> &'static str {
        match self {
            Self::ConventionalTail => "Conventional tail",
            Self::TTail => "T-tail",
            Self::VTail => "V-tail",
            Self::FlyingWing => "Flying wing",
        }
    }

    /// Physical description shown before a topology is selected.
    pub const fn description(self) -> &'static str {
        match self {
            Self::ConventionalTail => "Separate horizontal and vertical tail surfaces.",
            Self::TTail => "Horizontal tail mounted at the vertical-tail tip.",
            Self::VTail => "Two inclined surfaces with coupled pitch and yaw control.",
            Self::FlyingWing => "Tailless planform requiring elevons and a reflex/control model.",
        }
    }

    /// Whether a named engineering path is represented for this arrangement.
    pub const fn availability(self, path: UavAnalysisPath) -> TopologyAvailability {
        match path {
            UavAnalysisPath::GeometryExport | UavAnalysisPath::SharedCoreLift => {
                TopologyAvailability::Available
            }
            UavAnalysisPath::PreliminaryOptimization | UavAnalysisPath::CoupledStabilityAndTrim => {
                match self {
                    Self::ConventionalTail => TopologyAvailability::Available,
                    Self::TTail => TopologyAvailability::Unavailable(
                        TopologyUnavailableReason::TTailInterferenceAndMassModel,
                    ),
                    Self::VTail => TopologyAvailability::Unavailable(
                        TopologyUnavailableReason::VTailControlMixing,
                    ),
                    Self::FlyingWing => TopologyAvailability::Unavailable(
                        TopologyUnavailableReason::FlyingWingElevonAndReflexModel,
                    ),
                }
            }
        }
    }

    /// Whether geometry construction needs the generated empennage dimensions.
    pub(crate) const fn uses_generated_empennage(self) -> bool {
        !matches!(self, Self::FlyingWing)
    }
}

/// Engineering path whose topology coverage is being queried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UavAnalysisPath {
    /// Preliminary component, mass, packaging, and geometry search.
    PreliminaryOptimization,
    /// Conversion to the shared aircraft geometry primitives.
    GeometryExport,
    /// Fixed-geometry inviscid VLM lift assessment.
    SharedCoreLift,
    /// Static-stability and evidenced pitch-trim product verdict.
    CoupledStabilityAndTrim,
}

impl UavAnalysisPath {
    /// Short human-readable label for status and recovery messages.
    pub const fn label(self) -> &'static str {
        match self {
            Self::PreliminaryOptimization => "preliminary optimization",
            Self::GeometryExport => "geometry export",
            Self::SharedCoreLift => "shared-core lift assessment",
            Self::CoupledStabilityAndTrim => "coupled stability and trim",
        }
    }
}

/// Availability result for one topology/path combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TopologyAvailability {
    /// The path models the selected arrangement's required geometry and controls.
    Available,
    /// The path would need an omitted physical model and must not manufacture it.
    Unavailable(TopologyUnavailableReason),
}

impl TopologyAvailability {
    /// Whether the caller can run the requested path.
    pub const fn is_available(self) -> bool {
        matches!(self, Self::Available)
    }

    /// Typed reason when the path cannot be run.
    pub const fn unavailable_reason(self) -> Option<TopologyUnavailableReason> {
        match self {
            Self::Available => None,
            Self::Unavailable(reason) => Some(reason),
        }
    }
}

/// Missing model that prevents a topology-specific product claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TopologyUnavailableReason {
    /// The model has no T-tail junction/interference or top-mounted-tail mass model.
    TTailInterferenceAndMassModel,
    /// The model has no V-tail mixer or coupled pitch/yaw control authority.
    VTailControlMixing,
    /// The model has no elevon, reflex, or flying-wing longitudinal-control model.
    FlyingWingElevonAndReflexModel,
}

impl TopologyUnavailableReason {
    /// Actionable explanation for a selector or a typed error display.
    pub const fn description(self) -> &'static str {
        match self {
            Self::TTailInterferenceAndMassModel => {
                "T-tail sizing needs an evidenced fin-tip junction, interference, and top-mounted-tail mass model."
            }
            Self::VTailControlMixing => {
                "V-tail sizing and trim require an evidenced pitch/yaw mixer and coupled control authority."
            }
            Self::FlyingWingElevonAndReflexModel => {
                "Flying-wing sizing and trim require evidenced elevon, reflex, and longitudinal-control geometry."
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_selector_topology_has_an_explicit_boundary_for_each_path() {
        for topology in UavTopology::ALL {
            assert!(topology
                .availability(UavAnalysisPath::GeometryExport)
                .is_available());
            assert!(topology
                .availability(UavAnalysisPath::SharedCoreLift)
                .is_available());
        }
        assert!(UavTopology::ConventionalTail
            .availability(UavAnalysisPath::PreliminaryOptimization)
            .is_available());
        for topology in [
            UavTopology::TTail,
            UavTopology::VTail,
            UavTopology::FlyingWing,
        ] {
            assert!(!topology
                .availability(UavAnalysisPath::PreliminaryOptimization)
                .is_available());
        }
    }
}
