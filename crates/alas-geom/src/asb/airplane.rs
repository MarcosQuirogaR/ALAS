// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from aerosandbox/geometry/airplane.py
// Upstream: AeroSandbox 4.2.8, MIT.
// Reference: alas @ rust-port-baseline.

//! AeroSandbox's `Airplane`, scoped to the one call site that constructs it:
//! `alas-geom::builder`'s `AircraftBuilder::build`, which always supplies
//! `name`, `xyz_ref`, `wings`, `fuselages`, `s_ref`, `c_ref` and `b_ref`
//! explicitly.
//!
//! Left untranslated: the `propulsors` field (this program's engines are
//! `Fuselage`-shaped nacelles appended to `fuselages`, never a `Propulsor`),
//! `analysis_specific_options`, and the constructor's "derive `s_ref`/
//! `c_ref`/`b_ref` from `wings[0]`" fallback -- `AircraftBuilder::build`
//! never omits any of the three, so that branch is never reached from this
//! program's inputs. A future caller that needs either should extend this
//! module deliberately rather than assume the fallback is present.

use super::fuselage::Fuselage;
use super::wing::Wing;

/// An airplane: named geometry (wings and fuselages) plus the reference
/// quantities moments and stability derivatives are taken about --
/// `Airplane`, scoped to the fields [`crate::builder::AircraftBuilder`]
/// always supplies (see the module doc).
#[derive(Debug, Clone, PartialEq)]
pub struct Airplane {
    /// The airplane's name.
    pub name: String,
    /// The `(x, y, z)` reference point moments and stability derivatives are
    /// computed about -- normally the center of gravity.
    pub xyz_ref: [f64; 3],
    /// Every wing on the airplane.
    pub wings: Vec<Wing>,
    /// Every fuselage-shaped body on the airplane, including podded engine
    /// nacelles.
    pub fuselages: Vec<Fuselage>,
    /// Reference area.
    pub s_ref: f64,
    /// Reference chord.
    pub c_ref: f64,
    /// Reference span.
    pub b_ref: f64,
}
