// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Main-gear longitudinal station resolution: the wing-mounted-fallback
//! domain gate, the resolved station positions, and the wheel-count-weighted
//! effective station across multiple struts. Split out of `landing_gear::mod`
//! to keep room for the sizing config itself (see
//! `docs/source-size-budgets.tsv`).

/// Whether the wing-mounted main-gear fallback rule applies to a layout.
///
/// Without a source station anchor the only main-gear station available is
/// `mac_le + mlg_x_fraction_mac * MAC`, which places the legs inside the wing
/// box. That is a **wing-mounted gear** rule (Raymer, *Aircraft Design*,
/// ch. 11) and it presumes the wing carry-through sits low enough on the
/// fuselage for the legs to attach to it and retract into the wing or its
/// root fairing. A wing mounted entirely above the fuselage has no wing-root
/// gear bay, so the rule has nothing to place gear in.
///
/// The boundary is the fuselage's own outer surface at the wing root, not a
/// tuned coefficient, and carries no margin term: either the root is above
/// the crown or it is not. This enum is the verdict, not the measurement -
/// the two heights are supplied by whichever crate holds the built geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WingMountedGearDomain {
    /// The wing root is on or below the fuselage crown: low-wing, mid-wing
    /// and shoulder-wing layouts alike, which is the domain the rule is
    /// stated for.
    Applicable,
    /// The wing root leading edge sits strictly above the fuselage crown at
    /// the same longitudinal station: a high-wing layout, whose main gear is
    /// carried somewhere this model does not derive.
    WingRootAboveFuselageCrown {
        /// Wing root leading-edge height in the geometry frame, m.
        wing_root_z_m: f64,
        /// Fuselage outer top surface at the wing root station, m.
        fuselage_crown_z_m: f64,
    },
}

impl WingMountedGearDomain {
    /// Decide the verdict from two modelled heights in the geometry frame
    /// (z up, m).
    ///
    /// The comparison is strict and margin-free: a root exactly on the crown
    /// is not a high-wing layout and keeps the fallback. Heights that are not
    /// both finite decide nothing, so the fallback stands and the caller's
    /// own finiteness checks report the degenerate geometry.
    #[must_use]
    pub fn from_heights(wing_root_z_m: f64, fuselage_crown_z_m: f64) -> Self {
        if wing_root_z_m.is_finite()
            && fuselage_crown_z_m.is_finite()
            && wing_root_z_m > fuselage_crown_z_m
        {
            Self::WingRootAboveFuselageCrown {
                wing_root_z_m,
                fuselage_crown_z_m,
            }
        } else {
            Self::Applicable
        }
    }

    /// Whether the wing-mounted fallback may be used on this layout.
    #[inline]
    #[must_use]
    pub fn applies(self) -> bool {
        matches!(self, Self::Applicable)
    }
}

/// The wing-mounted main-gear fallback was refused: this aircraft has no
/// main-gear longitudinal station the model can supply.
///
/// This is a missing-datum failure, not a marginal result. It is closed by
/// registering the aircraft's published gear stations
/// (`reference_station_fuselage_length_m`, `reference_nlg_x_fraction`,
/// `reference_mlg_x_fractions`), which
/// [`super::LandingGearConfig::resolved_station_positions`] then scales onto
/// the active fuselage - not by relaxing the gate.
///
/// `Eq` is deliberately not derived: the variant carries the two f64 heights
/// that decided it, and exact equality on floating-point evidence invites
/// comparisons that are not meaningful.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MainGearFallbackRefusal {
    /// Wing root leading-edge height in the geometry frame, m.
    pub wing_root_z_m: f64,
    /// Fuselage outer top surface at the wing root station, m.
    pub fuselage_crown_z_m: f64,
}

impl std::fmt::Display for MainGearFallbackRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "no main-gear longitudinal station is available: the landing-gear configuration \
             registers no reference_mlg_x_fractions anchor, and the wing-mounted fallback \
             (mlg_x_fraction_mac aft of the MAC leading edge) does not apply because the wing \
             root leading edge sits at z = {} m, above the fuselage crown at z = {} m, so no \
             wing-root gear bay exists on this layout",
            self.wing_root_z_m, self.fuselage_crown_z_m
        )
    }
}

impl std::error::Error for MainGearFallbackRefusal {}

/// Longitudinal landing-gear stations resolved in the active aircraft frame.
#[derive(Debug, Clone, PartialEq)]
pub struct LandingGearStationPositions {
    /// Nose-gear station, m.
    pub x_nlg_m: f64,
    /// Effective main-gear longitudinal station for moment balance and reactions, m.
    ///
    /// For single-station or twin gear layouts, this matches the physical axle station.
    /// For multi-bogie layouts (e.g. A380, A340), this is the wheel-count-weighted
    /// centroid across all main-gear struts if valid, or unweighted mean for missing counts.
    /// If rejected, this holds the fallback primary station. Check `resolution` for validity.
    pub x_mlg_m: f64,
    /// Main-gear station for each configured strut, in layout order.
    pub main_gear_x_m: Vec<f64>,
    /// Whether the positions came from a complete normalized source anchor.
    pub source_scaled: bool,
    /// The typed resolution outcome of resolving the effective main-gear station.
    pub resolution: Result<ValidGearStation, GearStationRejection>,
}

impl LandingGearStationPositions {
    /// Primary main-gear station (e.g. wing main gear), m.
    ///
    /// Retained for direct scalar comparison against published manufacturer
    /// reference wheelbase baselines.
    pub fn primary_mlg_x_m(&self) -> f64 {
        self.main_gear_x_m.first().copied().unwrap_or(self.x_mlg_m)
    }

    /// Longitudinal wheelbase from nose gear to primary main gear (wing gear), m.
    pub fn primary_wheelbase_m(&self) -> f64 {
        self.primary_mlg_x_m() - self.x_nlg_m
    }

    /// Effective longitudinal wheelbase from nose gear to the effective main gear centroid, m.
    pub fn effective_wheelbase_m(&self) -> f64 {
        self.x_mlg_m - self.x_nlg_m
    }

    /// Whether the effective main-gear station resolution succeeded without rejection.
    #[inline]
    pub fn is_valid(&self) -> bool {
        self.resolution.is_ok()
    }

    /// Return the rejection details if the resolution was rejected, or `None` if valid.
    #[inline]
    pub fn rejection(&self) -> Option<&GearStationRejection> {
        self.resolution.as_ref().err()
    }
}

/// The validated resolution of an effective longitudinal main-gear station.
///
/// Distinguishes valid weighted centroid, missing-data unweighted mean,
/// and uniform single-station layouts. Every variant guarantees a finite,
/// physically computed longitudinal station.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValidGearStation {
    /// Explicit per-strut wheel counts provided and valid:
    /// wheel-count-weighted centroid across heterogeneous stations.
    WeightedCentroid {
        /// Effective longitudinal station in meters.
        station_m: f64,
        /// Total wheel count across all main-gear struts.
        total_wheels: i64,
    },
    /// Per-strut counts missing (`None`): declared conceptual approximation
    /// using the unweighted arithmetic mean across all struts (equal strut loading assumption).
    UnweightedMean {
        /// Effective longitudinal station in meters.
        station_m: f64,
        /// Number of main-gear struts averaged.
        strut_count: usize,
    },
    /// Single strut or identical stations across all struts:
    /// geometrically uniform station where weighting is invariant.
    UniformStation {
        /// Effective longitudinal station in meters.
        station_m: f64,
    },
}

impl ValidGearStation {
    /// Return the longitudinal main-gear station in meters.
    #[inline]
    pub fn station_m(&self) -> f64 {
        match *self {
            Self::WeightedCentroid { station_m, .. } => station_m,
            Self::UnweightedMean { station_m, .. } => station_m,
            Self::UniformStation { station_m } => station_m,
        }
    }

    /// Whether explicit wheel-count group weighting was applied.
    #[inline]
    pub fn is_weighted(&self) -> bool {
        matches!(self, Self::WeightedCentroid { .. })
    }

    /// Whether this outcome represents a missing-data declared approximation (unweighted mean).
    #[inline]
    pub fn is_unweighted_mean(&self) -> bool {
        matches!(self, Self::UnweightedMean { .. })
    }

    /// Whether this outcome represents a uniform single-station layout.
    #[inline]
    pub fn is_uniform(&self) -> bool {
        matches!(self, Self::UniformStation { .. })
    }
}

/// Description of why an explicit landing-gear configuration was rejected.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GearStationRejection {
    /// Primary main-gear station in meters, if stations were non-empty and finite.
    /// `None` when stations were empty or non-finite. Never fabricated as 0.0.
    pub primary_station_m: Option<f64>,
    /// Descriptive reason for the rejection.
    pub reason: &'static str,
}

impl GearStationRejection {
    /// Deliberate escape hatch returning the primary station fallback if available,
    /// or `f64::NAN` if stations were empty or non-finite.
    ///
    /// This method is explicitly named so callers cannot silently or ergonomically
    /// strip the rejection.
    #[inline]
    pub fn primary_station_ignoring_rejection(&self) -> f64 {
        self.primary_station_m.unwrap_or(f64::NAN)
    }

    /// Return the human-readable rejection reason.
    #[inline]
    pub fn reason(&self) -> &'static str {
        self.reason
    }
}

impl std::fmt::Display for GearStationRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GearStationRejection: {}", self.reason)
    }
}

impl std::error::Error for GearStationRejection {}

/// Typed resolution alias representing either a valid gear station or a rejection.
pub type EffectiveMainGearStation = Result<ValidGearStation, GearStationRejection>;

/// Extension trait providing audited accessors on [`EffectiveMainGearStation`].
pub trait EffectiveGearStationExt {
    /// Return the longitudinal station in meters if valid, or `None` if rejected.
    fn station_m(&self) -> Option<f64>;

    /// Deliberate escape hatch returning the computed station if valid,
    /// or the primary fallback station if rejected (or `NAN` if empty/non-finite).
    fn primary_station_ignoring_rejection(&self) -> f64;
}

impl EffectiveGearStationExt for Result<ValidGearStation, GearStationRejection> {
    #[inline]
    fn station_m(&self) -> Option<f64> {
        self.as_ref().ok().map(|v| v.station_m())
    }

    #[inline]
    fn primary_station_ignoring_rejection(&self) -> f64 {
        match self {
            Ok(v) => v.station_m(),
            Err(rej) => rej.primary_station_ignoring_rejection(),
        }
    }
}

/// Compute the effective longitudinal main-gear station across multiple struts,
/// returning a typed resolution outcome [`EffectiveMainGearStation`].
///
/// For single-strut or identical-station layouts (e.g. twin gear), this
/// returns [`ValidGearStation::UniformStation`] with the physical axle station unchanged,
/// provided that any explicit `bogie_wheels` specification is valid.
///
/// For multi-strut layouts with heterogeneous longitudinal stations:
/// - When explicit per-strut wheel counts are provided in `bogie_wheels`,
///   they are validated against the domain rule: counts must match `stations.len()`,
///   must not be empty, and every entry must be a standard even count in `{2, 4, 6}`.
///   If valid, the effective station is the wheel-count-weighted centroid:
///
///     x_eff = sum(x_i * w_i) / sum(w_i)
///
///   returning [`ValidGearStation::WeightedCentroid`].
///
///   Malformed explicit values (length mismatch, or counts outside `{2, 4, 6}` like
///   3, 5, 0, or negative) are rejected and must NOT silently become valid weights;
///   they return `Err(`[`GearStationRejection`]`)` retaining `stations[0]`
///   as an emergency fallback datum with an explicit rejection flag and descriptive reason.
///
/// - When `bogie_wheels` is `None` (missing data):
///   Keeps the honest missing-data conceptual assumption distinct by returning
///   [`ValidGearStation::UnweightedMean`] with the unweighted arithmetic mean across
///   all struts: `sum(x_i) / N`.
///
/// Physical caveat: This weighting represents a declared 2D static equivalent
/// approximation consistent with manufacturer pavement reference conditions
/// (e.g. Airbus AC 7-3-0 static gear loading). It does NOT resolve the real 3D
/// multi-contact static indeterminacy across varying oleo inflation, pitch attitude,
/// pavement unevenness, or dynamic braking conditions, and must not be presented
/// as a certified weight-and-balance calculation.
pub fn effective_main_gear_station(
    stations: &[f64],
    bogie_wheels: Option<&[i64]>,
) -> EffectiveMainGearStation {
    if stations.is_empty() {
        return Err(GearStationRejection {
            primary_station_m: None,
            reason: "stations slice is empty",
        });
    }

    if stations.iter().any(|s| !s.is_finite()) {
        return Err(GearStationRejection {
            primary_station_m: None,
            reason: "stations slice contains non-finite values",
        });
    }

    let first = stations[0];

    // F4: Validate bogie_wheels shape and domain unconditionally and FIRST.
    // Geometric degeneracy (uniform stations) must NOT excuse malformed explicit declarations.
    if let Some(counts) = bogie_wheels {
        if counts.is_empty() {
            return Err(GearStationRejection {
                primary_station_m: Some(first),
                reason: "bogie_wheels count list is empty",
            });
        }
        if counts.len() != stations.len() {
            return Err(GearStationRejection {
                primary_station_m: Some(first),
                reason: "strut count mismatch between stations and bogie_wheels",
            });
        }
        if !counts.iter().all(|&c| matches!(c, 2 | 4 | 6)) {
            return Err(GearStationRejection {
                primary_station_m: Some(first),
                reason:
                    "non-standard bogie wheel count: all counts must be even numbers in {2, 4, 6}",
            });
        }
        let total_wheels: i64 = counts.iter().sum();
        if total_wheels <= 0 {
            return Err(GearStationRejection {
                primary_station_m: Some(first),
                reason: "total wheel count must be positive",
            });
        }
    }

    if stations.len() == 1 {
        return Ok(ValidGearStation::UniformStation { station_m: first });
    }

    if stations.iter().all(|&s| (s - first).abs() < 1e-9) {
        return Ok(ValidGearStation::UniformStation { station_m: first });
    }

    match bogie_wheels {
        Some(counts) => {
            let total_wheels: i64 = counts.iter().sum();
            let weighted_moment: f64 = stations
                .iter()
                .zip(counts.iter())
                .map(|(&s, &w)| s * w as f64)
                .sum();
            Ok(ValidGearStation::WeightedCentroid {
                station_m: weighted_moment / total_wheels as f64,
                total_wheels,
            })
        }
        None => {
            let unweighted_mean = stations.iter().sum::<f64>() / stations.len() as f64;
            Ok(ValidGearStation::UnweightedMean {
                station_m: unweighted_mean,
                strut_count: stations.len(),
            })
        }
    }
}
