// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/geometry/aircraft_builder.py
// Reference: alas @ rust-port-baseline.

//! Turns a `(DesignVector, GeometryConfig)` pair into an [`Airplane`] --
//! `AircraftBuilder`. This is ALAS's own assembly logic, not a translation of
//! a third-party library, which is why it lives here rather than under
//! aircraft model: every geometry decision reads from the config objects, and the
//! only literals in this module are structural (which xsec gets which
//! offset, in what order the wings and fuselages are assembled), not tunable
//! values.
//!
//! [`AircraftBuilder::build`] is where every other module in this crate
//! meets: [`crate::airfoil_library::AirfoilLibrary::get`] resolves the three
//! configured airfoil names (through all three of its branches on the
//! default aircraft -- see `docs/PORTING.md`'s Geometry section),
//! [`crate::airfoil_library::build_section`] shapes the root section from the
//! design vector, and [`crate::aircraft::wing::Wing`] /
//! [`crate::aircraft::fuselage::Fuselage`] loft the results into the returned
//! [`Airplane`].
//!
//! # Engine placement's two branches
//!
//! [`AircraftBuilder::build_engines`] has two cases: a centerline
//! tail-mounted engine (`y_pos == 0.0`, e.g. a trijet's tail engine) and a
//! wing-mounted engine, whose Z placement interpolates the wing's dihedral
//! between the root, break and tip stations. `alas-config::geometry`'s
//! default [`alas_config::EngineConfig`] carries two wing-mounted positions
//! (`9.8`, `-9.8`) and no centerline one, so this module's fixture
//! (`golden/geom/builder.json`) exercises only the wing-mounted branch; the
//! tail-mounted branch is covered by a unit test built on a synthetic
//! `y_pos == 0.0` position instead, since no default configuration reaches
//! it.
//!
//! # `sinspace`
//!
//! [`sinspace`] is duplicated here rather than shared with
//! `aircraft::spacing::linspace`/`cosspace`: that module is private to the aircraft model, and
//! `crate::airfoil_library` and `crate::wing_structure::support` already
//! establish the pattern of a small private copy per consumer rather than
//! widening the aircraft model's visibility for one helper (see either module's own
//! `linspace` for the precedent).

mod mesh;

include!("builder_parts/part_01.rs");
include!("builder_parts/part_02.rs");
