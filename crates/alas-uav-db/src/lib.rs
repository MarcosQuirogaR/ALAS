// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Provenance-tracked UAV component database, schema v2.
//!
//! Every number is a [`Field`]: SI value, unit, raw vendor text, cited
//! [`Source`], retrieval date, method, test conditions and confidence. Unknown
//! stays `None`; prices live in a separate [`Offer`] table; compatibility is
//! computed by consumers, never stored. The crate has no dependency on the
//! physics crates so the GUI, CLI importers and tests can use it alone.
//!
//! Layout: [`inventory`] (units, ranges and mandatory-for models, one table),
//! [`parts`] and [`component`] (typed rows), [`database`] (container, merge,
//! shipping gate), [`validate`] and `sanity` (checks), [`import`] (CSV/JSON),
//! [`overlay`] (user data), [`v1`] (converter from the v1 catalogue), [`kit`]
//! (the curated, shippable parts).

pub mod component;
pub mod database;
pub mod date;
pub mod error;
pub mod field;
pub mod ids;
pub mod import;
pub mod inventory;
pub mod kit;
pub mod offer;
pub mod overlay;
pub mod parts;
mod sanity;
pub mod source;
pub mod units;
pub mod v1;
pub mod validate;

pub use component::{Body, Category, Component, Status, SCHEMA_VERSION};
pub use database::Database;
pub use date::IsoDate;
pub use error::{DbError, DbResult};
pub use field::{Curve, CurveParam, Field, Method, Num, Provenance, TestConditions};
pub use ids::{ComponentId, SourceId};
pub use offer::{Offer, PerfKind, PriceBasis, PropPerfRow, PropPerfSet};
pub use parts::PropGeometry;
pub use source::{Redistribution, Source, SourceKind};
