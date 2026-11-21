// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The failure of component-station resolution.

/// Why component stations could not be resolved.
///
/// `Eq` is deliberately not derived: [`Self::MainGearStationNotMeasured`]
/// carries the two SI heights that decided it, and an exact-equality trait on
/// floating-point evidence would invite comparisons that are not meaningful.
#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum StationError {
    /// The built airplane has no wing named `"Main Wing"`.
    #[error("the built airplane has no wing named \"Main Wing\"")]
    MissingMainWing,
    /// The built airplane has no fuselage.
    #[error("the built airplane has no fuselage")]
    MissingFuselage,
    /// A resolved station's position or extent is not finite.
    #[error("component station \"{0}\" has a non-finite position or extent")]
    NonFiniteGeometry(&'static str),
    /// No longitudinal main-gear station is available for this aircraft: the
    /// configuration registers no source-backed gear stations, and the
    /// wing-mounted fallback rule that would otherwise stand in does not
    /// apply to this layout (see `main_gear_station`).
    ///
    /// This is a missing-datum failure, not a marginal result. It is raised
    /// rather than returning a station so that no consumer can accept a
    /// balance, reaction load or centre-of-gravity envelope computed from a
    /// main-gear station this model never measured or derived.
    #[error(
        "no main-gear longitudinal station is available: the landing-gear configuration registers \
         no reference_mlg_x_fractions anchor, and the wing-mounted fallback \
         (mlg_x_fraction_mac aft of the MAC leading edge) does not apply because the wing root \
         leading edge sits at z = {wing_root_z_m} m, above the fuselage crown at \
         z = {fuselage_crown_z_m} m, so no wing-root gear bay exists on this layout"
    )]
    MainGearStationNotMeasured {
        /// Wing root leading-edge height in the geometry frame, m.
        wing_root_z_m: f64,
        /// Fuselage outer top surface at the wing root station, m.
        fuselage_crown_z_m: f64,
    },
}
