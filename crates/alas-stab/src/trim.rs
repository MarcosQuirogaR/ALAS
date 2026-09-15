// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/stability.py
// Reference: alas @ rust-port-baseline.

//! Longitudinal stability and balance: the static margin, the neutral point,
//! the cruise-condition trim solve, the `autobalance` CG shift, and the
//! closed-form tail-volume and Munk apparent-mass correlations they build on.
//!
//! # Two kinds of computation in one module
//!
//! [`static_margin`], [`neutral_point`] and [`stability_and_trim`] each run
//! two or three [`alas_aero::vlm`] solves on the airplane and read
//! stability derivatives off finite differences of `CL` and `Cm`;
//! [`autobalance`] shifts the CG by the static margin those produce.
//! [`munk_apparent_mass_factor`], [`fuselage_cm_alpha`] and
//! [`tail_volume_coefficients`] are closed-form `f64` arithmetic over the
//! geometry and a published table, no factorization anywhere in them, though
//! two of the three are reached *from inside* the VLM-fed functions and so see
//! a VLM-derived slope as an input.
//!
//! # The stabilizer perturbation is a copy, not a mutation
//!
//! [`stability_and_trim`] measures the incidence derivatives by rigidly
//! deflecting the horizontal stabilizer and re-solving. Upstream overwrites
//! every stabilizer section's twist in place and restores it in a `finally`;
//! this port solves on a [`Airplane::clone`]d, perturbed copy
//! ([`with_hstab_twist`]) instead. The result is identical and the aircraft a
//! caller holds comes back unchanged either way, and it is also why
//! upstream's restore quirk (it writes `xsecs[0]`'s twist back to *all* the
//! sections, losing any spanwise variation) has nothing to reproduce here: no
//! section of the caller's airplane is ever written to. `alas-aero::analysis`'s
//! `trimmed_performance` already made the same decision, for the same reason.
//! [`autobalance`] *is* an in-place mutation, matching upstream: its whole
//! contract is to shift the caller's `xyz_ref[0]`.
//!
//! # `_xsec_width`/`_xsec_height` are field reads here
//!
//! Every function `alas/physics/stability.py` defines is translated here;
//! `_xsec_width`/`_xsec_height` are the only two that change shape. Upstream's
//! two accessors answer an explicit `width`/`height` or fall back to
//! `2 * radius`; `alas-geom::aircraft::fuselage`'s `FuselageXSec` normalizes
//! `radius` into `width`/`height` at construction, so both reduce to a field
//! read here. (`alas-payload`'s `CabinGeometry` reimplements them locally for
//! the same reason its row records.)

include!("trim_parts/part_01.rs");
include!("trim_parts/part_02.rs");
