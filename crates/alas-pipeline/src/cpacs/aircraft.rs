// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Conversion from the CPACS aircraft boundary to the physics geometry type.
//!
//! CPACS is the authoritative aircraft-data representation at this boundary,
//! while the existing aerodynamic and mass crates still operate on
//! [`Airplane`]. This adapter reconstructs the geometry written by the CPACS
//! exporter, including its parent-relative translations, section scales and
//! profile references. It rejects transforms that the current physics geometry
//! cannot represent instead of silently changing the aircraft.

include!("aircraft_parts/part_01.rs");
include!("aircraft_parts/part_02.rs");
include!("aircraft_parts/part_03.rs");
