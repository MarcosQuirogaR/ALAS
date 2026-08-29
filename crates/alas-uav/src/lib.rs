// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Fixed-wing electric UAV component selection and feasibility physics.
//!
//! A catalogue entry is evidence, not an engineering assumption: every
//! normalized value retains its source page, absent vendor fields remain
//! absent, and the feasibility report distinguishes a demonstrated overload
//! from missing evidence. [`catalog`] owns that boundary. [`feasibility`]
//! combines selected components with an airframe and evaluated operating
//! points, checking the coupled electrical, mass, packaging, aerodynamic,
//! propulsion, control, energy, and structural constraints without silently
//! manufacturing data the vendor did not publish.

pub mod catalog;
mod catalog_equipment;
pub mod catalog_readiness;
pub mod collector;
pub mod feasibility;
mod model;
pub mod optimizer;
mod packaging;
pub mod procurement;
pub mod production_verdict;
pub mod propulsion_electric;
pub mod shared_core;
pub mod topology;

pub use catalog::{
    apc_performance_catalog, full_catalog, multi_source_catalog, optimization_catalog,
    reviewed_catalog, seed_catalog, Catalog, CatalogError, ComponentKind, ComponentRecord,
};
pub use catalog_readiness::{analysis_evidence_gaps, is_analysis_ready, selectable_catalog};
pub use collector::{
    built_in_sources, collect, collect_sources, CollectedCatalogue, CollectedProduct,
    CollectorError, ComponentCategory, CrawlPolicy, FetchRequest, PageFetcher, ProcurementMetadata,
    ReviewStatus, Sleeper, SourceAdapter, SystemSleeper,
};
pub use feasibility::{evaluate, Finding, FindingKind, Severity, UavDesign, UavReport};
pub use optimizer::{
    optimize, optimize_for_topology, optimize_for_topology_with_control, optimize_with_control,
    OptimizationError, OptimizationProblem, OptimizationProgress, OptimizedUav,
    TopologyOptimizationError, TopologyOptimizedUav,
};
pub use procurement::{
    estimate_optimized_aircraft_cost, estimate_powertrain_procurement_cost,
    estimate_procurement_cost, has_reviewed_quote, optimized_aircraft_bom, reviewed_quote,
    AircraftBillOfMaterials, AircraftBomLine, BomCategory, Currency, CurrencySubtotal,
    ProcurementEstimate, ProcurementLine, ProcurementQuote, ProcurementSelection,
};
pub use production_verdict::{
    verify_coupled_production, CoupledAudit, CoupledFinding, CoupledOutcome,
    HorizontalTailTrimAuthority, LongitudinalStabilityAssessment, LongitudinalStabilityVerdict,
    PitchTrimAssessment, PitchTrimUnverifiedReason, ProductionVerificationInputs,
    TrimEndpointAssessment, TrimPointAssessment,
};
pub use shared_core::{
    assess_generated_geometry_with_shared_core, assess_generated_geometry_with_topology,
    assess_with_shared_core, assess_with_shared_core_for_topology, build_airplane,
    build_airplane_for_topology, verify_with_shared_core, verify_with_shared_core_for_topology,
    SharedCoreAssessment, SharedCoreFailure, SharedCoreInputs, SharedCoreLiftVerdict,
    SharedCoreVerification,
};
pub use topology::{TopologyAvailability, TopologyUnavailableReason, UavAnalysisPath, UavTopology};
