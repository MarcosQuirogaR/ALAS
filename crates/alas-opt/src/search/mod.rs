// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The product search driver: a broad low-resolution scan that nominates
//! starting points, then mesh adaptive direct search with a progressive
//! barrier over the common candidate evaluator.

// The legacy search kernels and the product MADS driver consume one scored
// point contract.  Keep the contract in `search_methods` for the existing
// kernels and re-export it here so `search::mads` can remain an independent
// implementation without introducing a duplicate result type.
pub(crate) use crate::search_methods::{MethodOutcome, ScoredPoint};

// Normalized mesh geometry and poll directions are shared by the search
// kernel and the staged scan's sample, so they sit beside both rather than
// inside either.
pub(crate) mod directions;
pub mod mads;
pub(crate) mod staged;
