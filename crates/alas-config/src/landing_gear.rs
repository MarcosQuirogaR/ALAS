// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/landing_gear_config.py
// Reference: alas @ rust-port-baseline.

//! Wheel, tire and strut sizing assumptions.
//!
//! These drive a real wheel-and-tire sizing pass rather than a fraction of
//! takeoff weight: the number of wheels and their rated load are what produce
//! the gear load limits, which in turn produce the strength boundaries of the
//! centre-of-gravity envelope. The distinction matters because the envelope
//! then reflects gear the aircraft could actually be built with, rather than
//! an assumption about gear nobody sized.
//!
//! Every count here accepts zero, meaning "size it": a wheel count is a
//! discrete choice made from the load, and asking a user to pick one before
//! the load is known has the causality backwards. A non-zero value overrides
//! the sizing, which is what makes an existing aircraft's gear reproducible.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// Tunable landing-gear sizing assumptions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct LandingGearConfig {
    /// Margin left in the rated tire load after the static reaction.
    #[config(
        label = "Tire load safety factor",
        help = "Margin applied to the static reaction load when selecting/verifying tire count: real gear is sized so the rated tire load is never fully consumed by static load alone, leaving margin for dynamic (braking, turning, rough-field) loads. Raymer: ~1.07 typical for a preliminary sizing pass."
    )]
    pub tire_safety_factor: f64,

    /// Wheels on the nose gear, or zero to size it.
    #[config(
        label = "Nose-gear wheel count (0 = auto)",
        help = "Wheels on the nose gear strut. 0 = auto: 1 for light aircraft, 2 (the near-universal choice for CS-25/FAR-25 transports) once MTOW exceeds nlg_dual_wheel_mtow_kg."
    )]
    pub n_nlg_wheels: i64,

    /// Where auto-sizing switches to a twin nose wheel.
    #[config(
        label = "MTOW threshold for dual nose wheels",
        unit = "kg",
        help = "Auto-sizing switches from a single to a dual (twin) nose wheel above this MTOW: below it, transport-category aircraft still commonly fly single nose wheels."
    )]
    pub nlg_dual_wheel_mtow_kg: f64,

    /// Main-gear legs, left and right combined, or zero to size them.
    #[config(
        label = "Main-gear strut count (0 = auto)",
        help = "Number of main-gear legs (each with its own wheel bogie), left+right combined. 0 = auto: 2 (one per side) below mlg_body_gear_mtow_kg, 4 (adds centreline body gear, e.g. A380/747-class) above it: real widebodies above roughly 300 t add body gear because a two-leg bogie would need an impractically large tire count/track width to carry the load within tire-pressure limits."
    )]
    pub n_mlg_struts: i64,

    /// Where auto-sizing adds centreline body gear.
    #[config(
        label = "MTOW threshold for body (centreline) main gear",
        unit = "kg",
        help = "Auto-sizing adds two centreline body-gear legs (4 main legs total) above this MTOW."
    )]
    pub mlg_body_gear_mtow_kg: f64,

    /// Wheels on each main-gear leg, or zero to size them.
    #[config(
        label = "Wheels per main-gear strut (0 = auto)",
        help = "0 = auto: the smallest of {2, 4, 6} standard bogie sizes whose rated capacity (tire_safety_factor-derated) covers this strut's static reaction load at the aft CG limit."
    )]
    pub wheels_per_mlg_strut: i64,

    /// Main-gear track as a multiple of fuselage diameter.
    #[config(
        label = "Main-gear track / fuselage-diameter factor",
        help = "Main-gear lateral track width, as a multiple of fuselage diameter. Real transports with wing-root-mounted main gear run track/diameter ~1.75-2.0 (777-300ER 2.03, 787-9 1.90, A340-300 1.91, A380-800 2.00, A320-200 1.92, DC-10-30 1.77): 1.85 is the fleet-average calibration. An earlier default (1.15) understated real track width by roughly a factor of 1.6, which fed directly into the lateral-turnover check (physics.landing_gear) reading artificially safe."
    )]
    pub track_diameter_factor: f64,

    /// Published primary-group wheelbase used as a comparison datum.
    ///
    /// This remains a source value for the parity contract.  When a complete
    /// normalized station anchor set is registered below, the active model
    /// scales those source fractions with its fuselage geometry; this scalar
    /// alone never moves stations or changes reaction loads.
    #[config(
        label = "Reference landing-gear wheelbase",
        unit = "m",
        help = "Published nose-gear to primary main-gear wheelbase retained for reference comparison. It does not define a station by itself; a complete normalized source anchor set is required to scale source stations with active geometry. Leave unset for a clean-sheet or optimized aircraft."
    )]
    #[serde(default)]
    pub reference_wheelbase_m: Option<f64>,

    /// Source drawing frame used by the normalized longitudinal station
    /// anchors below.  The current Airbus references measure from the
    /// geometric nose-tip extension on an aircraft-characteristics drawing;
    /// this is deliberately not presented as a certified WBM/AFM datum.
    #[serde(default)]
    #[config(skip)]
    pub reference_station_frame: Option<String>,

    /// Fuselage length used to normalize the source drawing stations, m.
    ///
    /// The value is provenance for the fractions, not a frozen absolute
    /// station.  When the active fuselage changes during a clean-sheet or
    /// shrink run, the fractions are re-applied to that active length.
    #[serde(default)]
    #[config(skip)]
    pub reference_station_fuselage_length_m: Option<f64>,

    /// Nose-gear station as a fraction of the source drawing fuselage length.
    #[serde(default)]
    #[config(skip)]
    pub reference_nlg_x_fraction: Option<f64>,

    /// Main-gear stations as fractions of the source drawing fuselage length.
    ///
    /// Order is left wing, right wing, then centreline/body units.  The first
    /// entry is the primary main-gear station used for the scalar
    /// `x_mlg`/wheelbase compatibility fields.
    #[serde(default)]
    #[config(skip)]
    pub reference_mlg_x_fractions: Option<Vec<f64>>,

    /// Published nose-gear to body-main-gear wheelbase, when the source has a
    /// distinct body-gear group (for example the A380 BLG).
    #[config(
        label = "Reference body-gear wheelbase",
        unit = "m",
        help = "Published nose-gear to body-main-gear wheelbase retained for a distinct source comparison. It is metadata; the active model uses normalized group stations when those are registered."
    )]
    #[serde(default)]
    pub reference_body_wheelbase_m: Option<f64>,

    /// Published main-gear track, measured according to the source definition.
    ///
    /// Unlike wheelbase, track supplies a source-backed lateral reference when
    /// its definition is known (for the Airbus references below this is the
    /// wing-main-gear centreline spacing). It remains a baseline datum: the
    /// design track is scaled with the active fuselage diameter through
    /// `track_diameter_factor`, so optimization does not freeze a source
    /// aircraft's absolute span.
    #[config(
        label = "Reference main-gear track",
        unit = "m",
        help = "Published main-gear track used as a source baseline when its definition matches the layout, such as wing-gear centreline spacing. The active design scales it with fuselage diameter and track_diameter_factor; leave unset to retain automatic sizing."
    )]
    #[serde(default)]
    pub reference_track_m: Option<f64>,

    /// Explicit wheel count for each main-gear strut, in layout order.
    ///
    /// The order is left wing, right wing, then centreline/body units. This
    /// permits real heterogeneous arrangements such as the A340's 4 + 4 + 2
    /// wheels while keeping the scalar `wheels_per_mlg_strut` compatibility
    /// control for uniform and automatic designs.
    #[config(
        label = "Main-gear wheels by strut",
        help = "Optional per-strut bogie counts in layout order: left wing, right wing, then centreline/body gear. Use standard even counts (2, 4 or 6) and provide one entry per configured main-gear strut. Leave unset for automatic or uniform sizing."
    )]
    #[serde(default)]
    pub mlg_strut_bogie_wheels: Option<Vec<i64>>,

    /// Which reference tire the sizing works from.
    #[config(
        options = TireClass,
        label = "Tire class",
        help = "Which reference tire (see physics.landing_gear.TIRE_DATABASE) to size with: 'auto' picks the smallest class whose rated load, combined with a realistic wheel count (<=6/strut), covers the aircraft's static gear loads. Options: auto, light, narrowbody, widebody, heavy."
    )]
    pub tire_class: String,

    /// What the strut is made of.
    #[config(
        options = StrutMaterial,
        label = "Strut material",
        help = "Landing-gear strut/piston material, shown on the planform diagram and in the design report. 'auto' selects by MTOW class (see physics.landing_gear.STRUT_MATERIALS): high-strength steel (300M-class) for larger transports, an aluminium/steel combination for light aircraft. Informational/labelling only; this preliminary-design tool does not run a structural (FEA) stress analysis of the strut itself."
    )]
    pub strut_material: String,

    /// The lateral tip-over criterion.
    #[config(
        label = "Max lateral turnover angle",
        unit = "deg",
        help = "Lateral tip-over (overturn) criterion, Raymer Ch.11 / Currey convention: the angle from the vertical whose tangent is CG height over the CG's perpendicular distance to the nose-gear-to-main-gear ground line must not exceed this (evaluated at the forward CG limit, the worst case), or the aircraft risks tipping over in a tight turn. 63 deg is the standard transport-category limit; a higher CG, narrower track, or more forward CG all push the angle up toward it."
    )]
    pub turnover_angle_limit_deg: f64,
}

impl Default for LandingGearConfig {
    fn default() -> Self {
        Self {
            tire_safety_factor: 1.07,
            n_nlg_wheels: 0,
            nlg_dual_wheel_mtow_kg: 15_000.0,
            n_mlg_struts: 0,
            mlg_body_gear_mtow_kg: 300_000.0,
            wheels_per_mlg_strut: 0,
            track_diameter_factor: 1.85,
            reference_wheelbase_m: None,
            reference_station_frame: None,
            reference_station_fuselage_length_m: None,
            reference_nlg_x_fraction: None,
            reference_mlg_x_fractions: None,
            reference_body_wheelbase_m: None,
            reference_track_m: None,
            mlg_strut_bogie_wheels: None,
            tire_class: "auto".to_owned(),
            strut_material: "auto".to_owned(),
            turnover_angle_limit_deg: 63.0,
        }
    }
}

impl LandingGearConfig {
    /// Return configuration errors that concern source-backed gear geometry.
    ///
    /// The global configuration validator turns these `(field, message)`
    /// pairs into blocking `ValidationIssue`s. Keeping the shape checks here
    /// makes the typed list contract available to callers that only own a
    /// landing-gear configuration, while the central validator still catches
    /// malformed values before a run starts.
    pub fn validation_errors(&self) -> Vec<(String, String)> {
        let mut errors = Vec::new();

        if let Some(value) = self.reference_wheelbase_m {
            if !value.is_finite() || value <= 0.0 {
                errors.push((
                    "landing_gear.reference_wheelbase_m".to_owned(),
                    format!(
                        "Reference wheelbase ({value:?} m) must be finite and greater than zero; it is a comparison datum only and cannot define absolute gear stations."
                    ),
                ));
            }
        }

        if let Some(value) = self.reference_body_wheelbase_m {
            if !value.is_finite() || value <= 0.0 {
                errors.push((
                    "landing_gear.reference_body_wheelbase_m".to_owned(),
                    format!(
                        "Reference body-gear wheelbase ({value:?} m) must be finite and greater than zero."
                    ),
                ));
            }
        }

        let station_fields_present = self.reference_station_fuselage_length_m.is_some()
            || self.reference_nlg_x_fraction.is_some()
            || self.reference_mlg_x_fractions.is_some();
        let station_fields_complete = self.reference_station_fuselage_length_m.is_some()
            && self.reference_nlg_x_fraction.is_some()
            && self.reference_mlg_x_fractions.is_some();
        if station_fields_present && !station_fields_complete {
            errors.push((
                "landing_gear.reference_station".to_owned(),
                "Normalized source stations require reference_station_fuselage_length_m, reference_nlg_x_fraction, and reference_mlg_x_fractions together.".to_owned(),
            ));
        }
        if let Some(value) = self.reference_station_fuselage_length_m {
            if !value.is_finite() || value <= 0.0 {
                errors.push((
                    "landing_gear.reference_station_fuselage_length_m".to_owned(),
                    format!(
                        "Reference station fuselage length ({value:?} m) must be finite and greater than zero."
                    ),
                ));
            }
        }
        if let Some(value) = self.reference_nlg_x_fraction {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                errors.push((
                    "landing_gear.reference_nlg_x_fraction".to_owned(),
                    format!(
                        "Reference nose-gear station fraction ({value:?}) must be finite and lie between zero and one."
                    ),
                ));
            }
        }
        if let Some(fractions) = &self.reference_mlg_x_fractions {
            let expected = if self.n_mlg_struts > 0 {
                Some(self.n_mlg_struts as usize)
            } else {
                None
            };
            let length_is_valid = expected.map_or(matches!(fractions.len(), 2..=4), |expected| {
                fractions.len() == expected
            });
            if !length_is_valid {
                let expected_text = expected.map_or_else(
                    || "2, 3, or 4 for automatic strut count".to_owned(),
                    |value| value.to_string(),
                );
                errors.push((
                    "landing_gear.reference_mlg_x_fractions".to_owned(),
                    format!(
                        "Normalized source main-gear station list has {} entries; expected {expected_text}.",
                        fractions.len()
                    ),
                ));
            }
            for (index, &value) in fractions.iter().enumerate() {
                if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                    errors.push((
                        format!("landing_gear.reference_mlg_x_fractions[{index}]"),
                        format!(
                            "Reference main-gear station fraction ({value:?}) must be finite and lie between zero and one."
                        ),
                    ));
                }
            }
        }

        if let Some(value) = self.reference_track_m {
            if !value.is_finite() || value <= 0.0 {
                errors.push((
                    "landing_gear.reference_track_m".to_owned(),
                    format!(
                        "Reference main-gear track ({value:?} m) must be finite and greater than zero."
                    ),
                ));
            }
        }

        if let Some(counts) = &self.mlg_strut_bogie_wheels {
            let expected = if self.n_mlg_struts > 0 {
                Some(self.n_mlg_struts as usize)
            } else {
                None
            };
            let length_is_valid = expected.map_or(matches!(counts.len(), 2 | 4), |expected| {
                counts.len() == expected
            });
            if !length_is_valid {
                let expected_text = expected.map_or_else(
                    || "2 or 4 for automatic strut count".to_owned(),
                    |value| value.to_string(),
                );
                errors.push((
                    "landing_gear.mlg_strut_bogie_wheels".to_owned(),
                    format!(
                        "Per-strut main-gear wheel list has {} entries; expected {expected_text}.",
                        counts.len()
                    ),
                ));
            }

            for (index, &count) in counts.iter().enumerate() {
                if !matches!(count, 2 | 4 | 6) {
                    errors.push((
                        format!("landing_gear.mlg_strut_bogie_wheels[{index}]"),
                        format!(
                            "Main-gear bogie wheel count {count} is unsupported; use one of the standard even counts 2, 4 or 6."
                        ),
                    ));
                }
            }
        }

        errors
    }

    /// Resolve the longitudinal landing-gear stations in the active geometry
    /// frame.
    ///
    /// A complete source anchor set is expressed as nose-tip drawing
    /// fractions and scales with the active fuselage length.  This preserves
    /// the geometry design space during shrink/optimization while making a
    /// source-backed preset reproducible.  If the source anchor is absent or
    /// malformed, the caller's existing model-derived fallback stations are
    /// retained.  Source stations are geometric evidence; they do not use
    /// mass, CG, or reaction loads to calibrate a position.
    pub fn resolved_station_positions(
        &self,
        fallback_x_nlg_m: f64,
        fallback_x_mlg_m: f64,
        fuselage_start_x_m: f64,
        fuselage_length_m: f64,
    ) -> LandingGearStationPositions {
        let source = self
            .reference_station_fuselage_length_m
            .filter(|length| length.is_finite() && *length > 0.0)
            .zip(self.reference_nlg_x_fraction)
            .zip(self.reference_mlg_x_fractions.as_deref())
            .filter(|((_, nlg), mlg)| {
                fuselage_start_x_m.is_finite()
                    && nlg.is_finite()
                    && (0.0..=1.0).contains(nlg)
                    && !mlg.is_empty()
                    && mlg
                        .iter()
                        .all(|fraction| fraction.is_finite() && (0.0..=1.0).contains(fraction))
                    && fuselage_length_m.is_finite()
                    && fuselage_length_m > 0.0
            });
        if let Some(((_, nlg_fraction), mlg_fractions)) = source {
            let x_nlg_m = fuselage_start_x_m + fuselage_length_m * nlg_fraction;
            let main_gear_x_m: Vec<f64> = mlg_fractions
                .iter()
                .map(|fraction| fuselage_start_x_m + fuselage_length_m * fraction)
                .collect();
            let effective =
                effective_main_gear_station(&main_gear_x_m, self.mlg_strut_bogie_wheels.as_deref());
            return LandingGearStationPositions {
                x_nlg_m,
                x_mlg_m: effective.primary_station_ignoring_rejection(),
                main_gear_x_m,
                source_scaled: true,
                resolution: effective,
            };
        }

        LandingGearStationPositions {
            x_nlg_m: fallback_x_nlg_m,
            x_mlg_m: fallback_x_mlg_m,
            main_gear_x_m: vec![fallback_x_mlg_m],
            source_scaled: false,
            resolution: effective_main_gear_station(&[fallback_x_mlg_m], None),
        }
    }
}

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

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Entry, OptionSource};

    #[test]
    fn every_count_defaults_to_being_sized_rather_than_asserted() {
        // A wheel count follows from the load, so the shipped default has to
        // be the one that lets the sizing decide.
        let config = LandingGearConfig::default();
        assert_eq!(config.n_nlg_wheels, 0);
        assert_eq!(config.n_mlg_struts, 0);
        assert_eq!(config.wheels_per_mlg_strut, 0);
        assert_eq!(config.reference_wheelbase_m, None);
        assert_eq!(config.reference_station_frame, None);
        assert_eq!(config.reference_station_fuselage_length_m, None);
        assert_eq!(config.reference_nlg_x_fraction, None);
        assert_eq!(config.reference_mlg_x_fractions, None);
        assert_eq!(config.reference_body_wheelbase_m, None);
        assert_eq!(config.reference_track_m, None);
        assert_eq!(config.mlg_strut_bogie_wheels, None);
        assert_eq!(config.tire_class, "auto");
        assert_eq!(config.strut_material, "auto");
    }

    #[test]
    fn body_gear_is_added_well_above_the_twin_nose_wheel_threshold() {
        let config = LandingGearConfig::default();
        assert!(config.mlg_body_gear_mtow_kg > config.nlg_dual_wheel_mtow_kg);
    }

    #[test]
    fn the_strut_material_has_its_own_option_list_and_not_the_general_one() {
        // Its accepted values are the strut materials, which are a different
        // set from the structural material database, and offering the wrong
        // list would let a value through that nothing downstream can resolve.
        let schema = LandingGearConfig::default().schema();
        match &schema.field("strut_material").unwrap().entry {
            Entry::Leaf(leaf) => {
                assert_eq!(leaf.options, Some(OptionSource::StrutMaterial));
                assert!(!OptionSource::StrutMaterial.editable());
            }
            Entry::Node(_) => panic!("a material name is not a group"),
        }
    }

    #[test]
    fn heterogeneous_bogie_lists_require_matching_standard_counts() {
        let mut config = LandingGearConfig {
            n_mlg_struts: 3,
            mlg_strut_bogie_wheels: Some(vec![4, 4, 2]),
            ..Default::default()
        };
        assert!(config.validation_errors().is_empty());

        config.mlg_strut_bogie_wheels = Some(vec![4, 4]);
        assert!(config
            .validation_errors()
            .iter()
            .any(|(path, _)| path == "landing_gear.mlg_strut_bogie_wheels"));

        config.mlg_strut_bogie_wheels = Some(vec![4, 3, 2]);
        assert!(config
            .validation_errors()
            .iter()
            .any(|(path, _)| path.ends_with("[1]")));
    }

    #[test]
    fn reference_dimensions_must_be_positive_and_finite() {
        let mut config = LandingGearConfig {
            reference_wheelbase_m: Some(0.0),
            reference_track_m: Some(f64::NAN),
            ..Default::default()
        };
        assert_eq!(config.validation_errors().len(), 2);
        config.reference_wheelbase_m = Some(12.64);
        config.reference_track_m = Some(7.59);
        assert!(config.validation_errors().is_empty());
    }

    #[test]
    fn normalized_source_stations_scale_with_active_fuselage_length() {
        let config = LandingGearConfig {
            reference_station_frame: Some("nose_tip_drawing_reference".to_owned()),
            reference_station_fuselage_length_m: Some(72.73),
            reference_nlg_x_fraction: Some(4.97 / 72.73),
            reference_mlg_x_fractions: Some(vec![
                33.58 / 72.73,
                33.58 / 72.73,
                36.85 / 72.73,
                36.85 / 72.73,
            ]),
            n_mlg_struts: 4,
            mlg_strut_bogie_wheels: Some(vec![4, 4, 6, 6]),
            ..Default::default()
        };
        assert!(config.validation_errors().is_empty());
        let resolved = config.resolved_station_positions(1.0, 2.0, 10.0, 72.73);
        assert!(resolved.source_scaled);
        assert!((resolved.x_nlg_m - 14.97).abs() < 1.0e-12);
        assert!((resolved.x_mlg_m - 45.542).abs() < 1.0e-12);
        assert!((resolved.primary_mlg_x_m() - 43.58).abs() < 1.0e-12);
        assert!((resolved.primary_wheelbase_m() - 28.61).abs() < 1.0e-12);
        assert!((resolved.effective_wheelbase_m() - 30.572).abs() < 1.0e-12);
        assert_eq!(resolved.main_gear_x_m.len(), 4);

        let shrunk = config.resolved_station_positions(1.0, 2.0, 10.0, 60.0);
        assert!((shrunk.x_nlg_m - (10.0 + 60.0 * 4.97 / 72.73)).abs() < 1.0e-12);
        assert!(shrunk.x_mlg_m < resolved.x_mlg_m);
    }

    #[test]
    fn incomplete_or_invalid_source_stations_keep_model_fallback() {
        let config = LandingGearConfig {
            reference_station_fuselage_length_m: Some(60.0),
            reference_nlg_x_fraction: Some(0.1),
            reference_mlg_x_fractions: Some(vec![f64::NAN, 0.5]),
            n_mlg_struts: 2,
            ..Default::default()
        };
        let resolved = config.resolved_station_positions(3.0, 8.0, 0.0, 60.0);
        assert!(!resolved.source_scaled);
        assert_eq!(resolved.x_nlg_m, 3.0);
        assert_eq!(resolved.x_mlg_m, 8.0);
        assert_eq!(resolved.main_gear_x_m, vec![8.0]);
    }

    #[test]
    fn effective_main_gear_station_a380_unequal_bogies() {
        // A380 layout: 2 wing struts (4 wheels each at 33.58 m) +
        // 2 body struts (6 wheels each at 36.85 m). Total = 20 wheels.
        let stations = [33.58, 33.58, 36.85, 36.85];
        let bogie_wheels = [4, 4, 6, 6];
        let eff = effective_main_gear_station(&stations, Some(&bogie_wheels));
        // (4*33.58 + 4*33.58 + 6*36.85 + 6*36.85) / 20 = 710.84 / 20 = 35.542 m
        assert!(eff.is_ok());
        let valid = eff.unwrap();
        assert!((valid.station_m() - 35.542).abs() < 1.0e-12);
        assert!(valid.is_weighted());
        assert!(!valid.is_unweighted_mean());
        assert!(!valid.is_uniform());
        match valid {
            ValidGearStation::WeightedCentroid {
                station_m,
                total_wheels,
            } => {
                assert!((station_m - 35.542).abs() < 1.0e-12);
                assert_eq!(total_wheels, 20);
            }
            _ => panic!("expected WeightedCentroid, got {valid:?}"),
        }
    }

    #[test]
    fn effective_main_gear_station_twin_gear_invariant() {
        // Twin-gear aircraft (A220, A320, B787) with identical longitudinal stations
        let stations = [18.633948, 18.633948];
        let eff_none = effective_main_gear_station(&stations, None);
        let eff_some = effective_main_gear_station(&stations, Some(&[2, 2]));
        assert!(eff_none.is_ok());
        assert!(eff_some.is_ok());
        let valid_none = eff_none.unwrap();
        let valid_some = eff_some.unwrap();
        assert!((valid_none.station_m() - 18.633948).abs() < 1.0e-12);
        assert!((valid_some.station_m() - 18.633948).abs() < 1.0e-12);
        assert!(valid_none.is_uniform());
        assert!(valid_some.is_uniform());
        assert!(!valid_none.is_weighted());
        assert!(!valid_some.is_weighted());
    }

    #[test]
    fn effective_main_gear_station_permutation_invariance() {
        // Strut ordering in arrays must not alter the resulting centroid
        let stations_layout = [33.58, 33.58, 36.85, 36.85];
        let wheels_layout = [4, 4, 6, 6];
        let eff_layout = effective_main_gear_station(&stations_layout, Some(&wheels_layout))
            .expect("layout should be valid");

        let stations_perm = [36.85, 33.58, 36.85, 33.58];
        let wheels_perm = [6, 4, 6, 4];
        let eff_perm = effective_main_gear_station(&stations_perm, Some(&wheels_perm))
            .expect("permuted layout should be valid");

        // Permutation invariance of the effective gear station holds to floating-point
        // summation round-off (~6e-15 m), not bit-exactly, because IEEE 754 addition is
        // non-associative under strut reordering. An absolute SI bound of 1.0e-12 m
        // (1 picometer, well below any physical manufacturing or sub-atomic scale)
        // rigorously bounds the numerical precision of the centroid summation.
        assert!(
            (eff_layout.station_m() - eff_perm.station_m()).abs() < 1.0e-12,
            "permutation invariant to summation round-off: layout={}, perm={}, diff={:e}",
            eff_layout.station_m(),
            eff_perm.station_m(),
            (eff_layout.station_m() - eff_perm.station_m()).abs()
        );
        assert!((eff_perm.station_m() - 35.542).abs() < 1.0e-12);
        assert!(eff_layout.is_weighted());
        assert!(eff_perm.is_weighted());
    }

    #[test]
    fn effective_main_gear_station_missing_data_conceptual_fallback() {
        // When per-strut counts are missing (None), the declared conceptual
        // assumption of equal strut loading applies an unweighted arithmetic mean.
        let stations = [33.58, 33.58, 36.85, 36.85];
        let unweighted_mean = (33.58 * 2.0 + 36.85 * 2.0) / 4.0; // 35.215 m
        let eff = effective_main_gear_station(&stations, None);
        assert!(eff.is_ok());
        let valid = eff.unwrap();
        assert!((valid.station_m() - unweighted_mean).abs() < 1.0e-12);
        assert!(valid.is_unweighted_mean());
        assert!(!valid.is_weighted());
        assert!(!valid.is_uniform());
        match valid {
            ValidGearStation::UnweightedMean {
                station_m,
                strut_count,
            } => {
                assert!((station_m - unweighted_mean).abs() < 1.0e-12);
                assert_eq!(strut_count, 4);
            }
            _ => panic!("expected UnweightedMean, got {valid:?}"),
        }
    }

    #[test]
    fn uniform_stations_with_malformed_counts_reject() {
        // F4 table: geometric degeneracy must NOT bypass count validation
        // Row 1: Non-standard count (3 and 999 not in {2, 4, 6})
        let row1 = effective_main_gear_station(&[18.633948, 18.633948], Some(&[3, 999]));
        assert!(
            row1.is_err(),
            "non-standard count on uniform stations must reject"
        );
        let rej1 = row1.unwrap_err();
        assert_eq!(rej1.primary_station_ignoring_rejection(), 18.633948);
        assert_eq!(
            rej1.reason,
            "non-standard bogie wheel count: all counts must be even numbers in {2, 4, 6}"
        );

        // Row 2: Length mismatch (3 counts for 2 struts)
        let row2 = effective_main_gear_station(&[18.6, 18.6], Some(&[4, 4, 6]));
        assert!(
            row2.is_err(),
            "length mismatch on uniform stations must reject"
        );
        let rej2 = row2.unwrap_err();
        assert_eq!(rej2.primary_station_ignoring_rejection(), 18.6);
        assert_eq!(
            rej2.reason,
            "strut count mismatch between stations and bogie_wheels"
        );

        // Row 3: Empty counts
        let row3 = effective_main_gear_station(&[18.6, 18.6], Some(&[]));
        assert!(
            row3.is_err(),
            "empty counts on uniform stations must reject"
        );
        let rej3 = row3.unwrap_err();
        assert_eq!(rej3.primary_station_ignoring_rejection(), 18.6);
        assert_eq!(rej3.reason, "bogie_wheels count list is empty");

        // Row 4: Negative count on single strut
        let row4 = effective_main_gear_station(&[18.6], Some(&[-2]));
        assert!(row4.is_err(), "negative count on single strut must reject");
        let rej4 = row4.unwrap_err();
        assert_eq!(rej4.primary_station_ignoring_rejection(), 18.6);
        assert_eq!(
            rej4.reason,
            "non-standard bogie wheel count: all counts must be even numbers in {2, 4, 6}"
        );
    }

    #[test]
    fn non_finite_stations_reject() {
        // F7 cases: NaN or Infinity must reject and never pass is_ok()
        let nan_none = effective_main_gear_station(&[f64::NAN, 33.58], None);
        assert!(nan_none.is_err());
        let rej_nan_none = nan_none.unwrap_err();
        assert!(rej_nan_none.primary_station_m.is_none());
        assert!(rej_nan_none.primary_station_ignoring_rejection().is_nan());
        assert_eq!(
            rej_nan_none.reason,
            "stations slice contains non-finite values"
        );

        let nan_some = effective_main_gear_station(&[f64::NAN, 33.58], Some(&[4, 4]));
        assert!(nan_some.is_err());
        let rej_nan_some = nan_some.unwrap_err();
        assert!(rej_nan_some.primary_station_m.is_none());
        assert!(rej_nan_some.primary_station_ignoring_rejection().is_nan());

        let inf_res = effective_main_gear_station(&[f64::INFINITY, 33.58], Some(&[4, 4]));
        assert!(inf_res.is_err());
        assert_eq!(
            inf_res.unwrap_err().reason,
            "stations slice contains non-finite values"
        );

        let neg_inf_res = effective_main_gear_station(&[33.58, f64::NEG_INFINITY], None);
        assert!(neg_inf_res.is_err());
        assert_eq!(
            neg_inf_res.unwrap_err().reason,
            "stations slice contains non-finite values"
        );
    }

    #[test]
    fn non_finite_fuselage_start_x_falls_back_to_model() {
        let config = LandingGearConfig {
            reference_station_fuselage_length_m: Some(60.0),
            reference_nlg_x_fraction: Some(0.1),
            reference_mlg_x_fractions: Some(vec![0.5]),
            n_mlg_struts: 2,
            ..Default::default()
        };
        let resolved = config.resolved_station_positions(3.0, 8.0, f64::NAN, 60.0);
        assert!(!resolved.source_scaled);
        assert_eq!(resolved.x_nlg_m, 3.0);
        assert_eq!(resolved.x_mlg_m, 8.0);
    }

    #[test]
    fn effective_main_gear_station_malformed_explicit_weights_rejected() {
        let stations = [33.58, 33.58, 36.85, 36.85];
        let primary_station = 33.58;

        // Empty stations: rejected, carries None / NAN, never 0.0 (F8)
        let empty_res = effective_main_gear_station(&[], Some(&[4, 4]));
        assert!(empty_res.is_err());
        let rej_empty = empty_res.unwrap_err();
        assert!(rej_empty.primary_station_m.is_none());
        assert!(rej_empty.primary_station_ignoring_rejection().is_nan());
        assert_eq!(rej_empty.reason, "stations slice is empty");

        // Length mismatch (too few): rejected with explicit flag and reason
        let too_few = effective_main_gear_station(&stations, Some(&[4, 4]));
        assert!(too_few.is_err());
        let rej_few = too_few.unwrap_err();
        assert_eq!(
            rej_few.primary_station_ignoring_rejection(),
            primary_station
        );
        assert_eq!(
            rej_few.reason,
            "strut count mismatch between stations and bogie_wheels"
        );

        // Length mismatch (too many): rejected -> primary station
        let too_many = effective_main_gear_station(&stations, Some(&[4, 4, 6, 6, 2]));
        assert!(too_many.is_err());
        let rej_many = too_many.unwrap_err();
        assert_eq!(
            rej_many.primary_station_ignoring_rejection(),
            primary_station
        );
        assert_eq!(
            rej_many.reason,
            "strut count mismatch between stations and bogie_wheels"
        );

        // Non-standard count (e.g. 3 or 5): rejected -> primary station
        let non_std_3 = effective_main_gear_station(&stations, Some(&[4, 3, 6, 6]));
        assert!(non_std_3.is_err());
        let rej_3 = non_std_3.unwrap_err();
        assert_eq!(rej_3.primary_station_ignoring_rejection(), primary_station);
        assert_eq!(
            rej_3.reason,
            "non-standard bogie wheel count: all counts must be even numbers in {2, 4, 6}"
        );

        let non_std_5 = effective_main_gear_station(&stations, Some(&[4, 5, 6, 6]));
        assert!(non_std_5.is_err());
        let rej_5 = non_std_5.unwrap_err();
        assert_eq!(rej_5.primary_station_ignoring_rejection(), primary_station);

        // Zero count: rejected -> primary station
        let zero_cnt = effective_main_gear_station(&stations, Some(&[4, 4, 0, 6]));
        assert!(zero_cnt.is_err());
        let rej_0 = zero_cnt.unwrap_err();
        assert_eq!(rej_0.primary_station_ignoring_rejection(), primary_station);

        // Negative count: rejected -> primary station
        let neg_cnt = effective_main_gear_station(&stations, Some(&[4, 4, -2, 6]));
        assert!(neg_cnt.is_err());
        let rej_neg = neg_cnt.unwrap_err();
        assert_eq!(
            rej_neg.primary_station_ignoring_rejection(),
            primary_station
        );
    }

    #[test]
    fn effective_main_gear_station_force_moment_equilibrium_closure() {
        // Airbus A380 AC 7-3-0 reference loading condition:
        // WLG: 106,920 kg per strut (4 wheels -> 26,730 kg/wheel)
        // BLG: 160,380 kg per strut (6 wheels -> 26,730 kg/wheel)
        // Total main gear load = 2 * 106,920 + 2 * 160,380 = 534,600 kg (20 wheels * 26,730 kg)
        let stations = [33.58, 33.58, 36.85, 36.85];
        let wheels = [4, 4, 6, 6];
        let eff = effective_main_gear_station(&stations, Some(&wheels))
            .expect("A380 closure stations should be valid");
        let eff_x = eff.station_m();

        let x_nlg = 4.97;
        let g = 9.80665;
        let load_per_wheel = 26_730.0 * g; // N

        // Discrete sum of moments about NLG
        let discrete_moment: f64 = stations
            .iter()
            .zip(wheels.iter())
            .map(|(&x, &w)| (w as f64 * load_per_wheel) * (x - x_nlg))
            .sum();

        // Effective aggregate reaction moment about NLG
        let total_main_load = 20.0 * load_per_wheel;
        let aggregate_moment = total_main_load * (eff_x - x_nlg);

        let moment_residual = (discrete_moment - aggregate_moment).abs();
        assert!(
            moment_residual < 1.0e-6,
            "Moment equilibrium residual was {moment_residual} N*m"
        );
    }
}
