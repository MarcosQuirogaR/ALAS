// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The screening stage of the product search and the diverse elite it hands
//! to the refinement kernel (`search_methods::lshade_de`).

mod anchored_sampling;
mod coupled_geometry;
pub(crate) mod elite;
pub(crate) mod fidelity_pairs;
pub(crate) mod planform_projection;
pub(crate) mod screening;
pub(crate) mod work_cap;
