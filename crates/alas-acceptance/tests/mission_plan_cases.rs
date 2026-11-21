// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Acceptance cases of the v1.5 programme (the programme's acceptance cases C1 to C6)
//! flown end to end through the public pipeline with a user-defined mission
//! plan: the stages, the mass events, the fuel closed to the plan, and the
//! mass and centre-of-gravity history through each event. Each case also passes
//! the plausibility-tier mission rules and a bound on the fuel flow of its level
//! stages.
//!
//! C1 the competition mission, C3 a humanitarian airdrop, C4 a drone mothership,
//! C5 a tanker and a receiver, C6 a long-haul twin with a trim tank; and the
//! engine's own checks (Raymer's profiles, the field check, agreement with the
//! standard mission).

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

#[path = "mission_plan_cases/airdrop_drone.rs"]
mod airdrop_drone;
#[path = "mission_plan_cases/competition.rs"]
mod competition;
#[path = "mission_plan_cases/engine.rs"]
mod engine;
#[path = "mission_plan_cases/long_haul.rs"]
mod long_haul;
#[path = "mission_plan_cases/support.rs"]
mod support;
#[path = "mission_plan_cases/tanker.rs"]
mod tanker;
