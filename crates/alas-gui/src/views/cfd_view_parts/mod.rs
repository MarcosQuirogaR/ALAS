// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Implementation modules for the detached Airfoil CFD window.

pub(crate) mod advanced;
pub(crate) mod drawing;
#[cfg(test)]
mod evidence;
#[cfg(test)]
mod evidence_raster;
pub(crate) mod layout;
#[cfg(test)]
mod layout_tests;
pub(crate) mod log;
pub(crate) mod results;
pub(crate) mod study;
pub(crate) mod widgets;
