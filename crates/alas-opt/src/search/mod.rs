// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The product search driver: mesh adaptive direct search with a progressive
//! barrier over the common candidate evaluator.

// The legacy search kernels and the product MADS driver consume one scored
// point contract.  Keep the contract in `search_methods` for the existing
// kernels and re-export it here so `search::mads` can remain an independent
// implementation without introducing a duplicate result type.
pub(crate) use crate::search_methods::{MethodOutcome, ScoredPoint};

pub mod mads;
