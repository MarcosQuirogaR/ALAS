// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The mission network's edge with the rest of the program.
//!
//! Historically this crate sat behind a subprocess boundary: two integration
//! modules assembled a JSON request, handed it to a separate interpreter, and
//! read a result back. The translation dissolves that boundary; there is no
//! second process and no serialization round trip,
//! but keeps the two halves of the request as the seam the rest of the program
//! meets the mission through, because each half is built from a different part
//! of the configuration and each is worth checking against the reference on its
//! own.
//!
//! [`profile`] is the mission half: cruise targets, field elevations and the
//! flown speed/rate/altitude schedule. [`vehicle`] is the vehicle half: the
//! design, its geometry and mass breakdown, its engine cycle and the
//! certification requirements. The segment network the two feed lands later.
//!
//! [`numerics`] is the pseudospectral scaffolding a mission segment is solved
//! on: the `Numerics` conditions container and the two functions that fill
//! and time-scale its Chebyshev differentiation and integration operators.
//!
//! [`segments`] is the network itself, one leg of a mission, its conditions
//! and the process chain that turns two unknowns per control point into two
//! force residuals, and [`solve`] is what drives it: MINPACK on each
//! segment, and each segment in turn.
//!
//! This is the crate where the phase's edges land. The segment chain reaches
//! `alas-atmo` for the atmosphere, `alas-aero` for the lift surrogate and the
//! drag polar, `alas-prop` for the engine and `alas-math` for the root finder
//! and the discretization, all green rows, and every one of them reached
//! because a mission segment is where the analyses are actually *evaluated*
//! rather than merely constructed.

pub mod numerics;
pub mod operating;
pub mod profile;
pub mod segments;
pub mod solve;
pub mod vehicle;

pub use numerics::Numerics;
pub use profile::{
    build_mission_request, check_profile_for_route, propose_profile_for_route,
    route_cruise_altitude_m, MissionProfileProposal, MissionProfileRouteCheck, MissionRequest,
};
pub use segments::{
    Conditions, Initials, MissionAnalyses, Segment, SegmentError, SegmentKind, SegmentSpec,
};
pub use solve::{
    converge_root, CompletedMissionSummary, FuelExhaustion, Mission, MissionError, MissionResult,
    SegmentSolution,
};
pub use vehicle::{
    build_vehicle_request, build_vehicle_request_reference_compatibility, ReportView,
    VehicleRequest, VehicleRequestError,
};
