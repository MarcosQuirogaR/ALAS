// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Preliminary main-wing structural mass centroid.
//!
//! A wing mass is not concentrated at its aerodynamic centre. The primary
//! structure is distributed through the wingbox, and bending material is
//! strongly biased inboard. This module integrates the first moment of four
//! explicit structural families: bending caps, spar webs, wingbox skins, and
//! ribs. Cap area follows the classical fully-stressed beam relation
//! `A = M / (sigma h)`; see A. Ning, *Flight Vehicle Design*, section 8.2,
//! equations 8.3--8.5:
//! <https://flowlab.groups.et.byu.net/me415/flight.pdf>. The stationwise
//! skin/web treatment is consistent with the preliminary wingbox procedure in
//! NASA TP-1158, pp. 8--12:
//! <https://ntrs.nasa.gov/api/citations/19780017136/downloads/19780017136.pdf>.
//!
//! This is a centroid model, not a replacement for the Torenbeek total wing
//! mass correlation. Its component masses only normalize the integrated first
//! moment; [`crate::breakdown::MassBreakdown::wing`] remains the mass carried
//! into aircraft weight and balance.

include!("wing_centroid_parts/part_01.rs");
include!("wing_centroid_parts/part_02.rs");
