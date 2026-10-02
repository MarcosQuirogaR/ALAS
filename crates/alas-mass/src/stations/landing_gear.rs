// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Landing-gear stations: the ground plane, and the nose/main placements
//! built on it.

use alas_config::{
    EffectiveGearStationExt, LandingGearConfig, LandingGearStationPositions, MassModelConfig,
    ValidGearStation,
};
use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec};
use alas_geom::aircraft::wing::Wing;

use super::{fuselage_datum, ComponentStation, StationError};

/// Fuselage-bottom height above the static ground line, as a fraction of the
/// fuselage diameter, used when a preset registers no clearance. Airbus
/// Aircraft Characteristics section 2-3-0 ground-clearance tables give the
/// fuselage bottom at 1.76-1.79 m on the A320-200 (0.45 D, MRW) and
/// 1.83-2.13 m on the A340-200 (0.33-0.38 D); 0.35 D sits at the low
/// (conservative for tail scrape and tip-back) end of that evidence.
pub const FALLBACK_BELLY_CLEARANCE_DIAMETER_FRACTION: f64 = 0.35;

/// The strut length and ground-contact height both gear legs share, m.
/// `landing_gear.fuselage_ground_clearance_m` if registered, else
/// [`FALLBACK_BELLY_CLEARANCE_DIAMETER_FRACTION`] of the fuselage diameter;
/// the one ground plane every caller shares (see [`ground_plane_z_m`]).
///
/// The clearance is measured from the fuselage lower surface, so the ground
/// plane hangs below the lowest point of the built lower contour
/// ([`fuselage_belly_z_m`]): the constant section of a conventional body.
/// The published clearances this reads are of that surface (Airbus A320
/// Aircraft Characteristics, Jun 01/24, Figure 2-3-0-991-004-A01 sheet 2:
/// fuselage bottom forward F1 1.786 m and aft F2 1.790 m at MRW, aft CG, a
/// level belly). Hanging it below the nose-tip centreline less half the
/// width instead puts the ground too low by the nose droop plus half the
/// height-width difference (0.305 m on the A320), and every CG height,
/// tip-back boundary and tail-scrape angle inherits that error.
fn gear_vertical_datum(
    fuselage: &Fuselage,
    geometry: &alas_config::GeometryConfig,
    landing_gear: Option<&LandingGearConfig>,
) -> (f64, f64) {
    let diameter = geometry.fuselage.diameter_m;
    let clearance = landing_gear.and_then(|config| config.fuselage_ground_clearance_m);
    let strut_length = clearance
        .filter(|v| v.is_finite() && *v > 0.0)
        .unwrap_or(FALLBACK_BELLY_CLEARANCE_DIAMETER_FRACTION * diameter);
    (strut_length, fuselage_belly_z_m(fuselage) - strut_length)
}

/// The lowest point of the fuselage's built lower contour, `z_c - height/2`
/// minimised over its cross-sections, m. A fuselage without cross-sections
/// falls back to the nose-datum centreline.
fn fuselage_belly_z_m(fuselage: &Fuselage) -> f64 {
    fuselage
        .xsecs
        .iter()
        .map(|xsec| xsec.xyz_c[2] - xsec.height / 2.0)
        .filter(|z| z.is_finite())
        .reduce(f64::min)
        .unwrap_or_else(|| fuselage_datum(fuselage).2)
}

/// The static ground-line height in geometry axes, m: the plane the gear
/// mass stations stand on and every CG-height consumer (tip-back, turnover,
/// tail scrape) must measure from.
pub fn ground_plane_z_m(
    fuselage: &Fuselage,
    geometry: &alas_config::GeometryConfig,
    landing_gear: &LandingGearConfig,
) -> f64 {
    gear_vertical_datum(fuselage, geometry, Some(landing_gear)).1
}

/// Documented-assumption fraction of strut length, above ground,
/// where a gear leg's own mass (axle/oleo/wheels) sits for the mass ledger
/// (not the tire ground-contact point z = ground).
pub(super) const GEAR_MASS_CENTROID_STRUT_FRACTION: f64 = 0.4;

fn gear_mass_centroid_z(ground_z: f64, strut_length: f64) -> f64 {
    ground_z + GEAR_MASS_CENTROID_STRUT_FRACTION * strut_length
}

/// The longitudinal stations the landing-gear configuration resolves, with
/// the model-derived fallbacks this module would otherwise supply.
///
/// `x_nlg`/`x_mlg` reproduce the exact convention `alas-perf::landing_gear`
/// is fed under: the nose gear at a fraction of fuselage length from the
/// nose, the main gear at a fraction of the MAC aft of the MAC leading edge.
/// Whether the main-gear fallback is admissible at all is decided by
/// [`main_gear_station`], not here.
fn resolved_gear_stations(
    fuselage: &Fuselage,
    main_wing: &Wing,
    mass_model: &MassModelConfig,
    landing_gear: Option<&LandingGearConfig>,
) -> LandingGearStationPositions {
    let (start_x, length, _) = fuselage_datum(fuselage);
    let fallback_x_nlg = start_x + length * mass_model.nlg_x_fraction;
    let mac = main_wing.mean_aerodynamic_chord();
    let mac_le_x = main_wing.aerodynamic_center(0.0)[0];
    let fallback_x_mlg = mac_le_x + mass_model.mlg_x_fraction_mac * mac;
    landing_gear
        .map(|config| {
            config.resolved_station_positions(fallback_x_nlg, fallback_x_mlg, start_x, length)
        })
        .unwrap_or_else(|| LandingGearStationPositions {
            x_nlg_m: fallback_x_nlg,
            x_mlg_m: fallback_x_mlg,
            main_gear_x_m: vec![fallback_x_mlg],
            source_scaled: false,
            resolution: alas_config::effective_main_gear_station(&[fallback_x_mlg], None),
        })
}

/// The nose landing-gear station.
///
/// The fallback rule here (a fraction of fuselage length aft of the nose)
/// is a fuselage rule and carries no assumption about where the wing is, so
/// it stands for any layout the geometry admits. Only the main gear's
/// wing-mounted fallback is layout-specific; see [`main_gear_station`].
pub(super) fn nose_gear_station(
    fuselage: &Fuselage,
    main_wing: &Wing,
    geometry: &alas_config::GeometryConfig,
    mass_model: &MassModelConfig,
    landing_gear: Option<&LandingGearConfig>,
) -> ComponentStation {
    let (strut_length, ground_z) = gear_vertical_datum(fuselage, geometry, landing_gear);
    let resolved = resolved_gear_stations(fuselage, main_wing, mass_model, landing_gear);
    ComponentStation {
        position_m: [
            resolved.x_nlg_m,
            0.0,
            gear_mass_centroid_z(ground_z, strut_length),
        ],
        extent_m: [0.0, 0.0, strut_length],
        method: if resolved.source_scaled {
            "source-scaled nose-tip NLG station"
        } else {
            "nlg_x_fraction of fuselage length"
        },
    }
}

/// The main landing-gear station, or a typed missing-datum failure when this
/// aircraft has none.
///
/// # Why this can fail
///
/// With a complete normalized source anchor
/// (`landing_gear.reference_mlg_x_fractions` and its companions) the station
/// is evidence: a published drawing station scaled to the active fuselage.
/// Without one, the only station available is
/// `mac_le + mlg_x_fraction_mac * MAC`, which places the main gear inside the
/// wing box. That is a **wing-mounted gear** rule (Raymer,
/// *Aircraft Design*, ch. 11): it presumes the wing carry-through sits low
/// enough on the fuselage for the legs to attach to it and retract into the
/// wing or its root fairing.
///
/// A wing mounted entirely above the fuselage has no wing-root gear bay at
/// all. Real high-wing transports carry their main gear on fuselage
/// sponsons, at a station the wing does not determine and this model does not
/// derive from anything. Applying the wing-mounted rule there is not a
/// coarse estimate but a placement of the gear where the aircraft has none,
/// and on a short high-wing turboprop it lands the station forward of the
/// centre of gravity, which reports a *negative* static nose reaction: the
/// aeroplane sitting on its tail at every loading state.
///
/// So the rule is applied inside its stated domain and refused outside it.
/// The boundary is the fuselage's own outer surface at the wing root, not a
/// tuned coefficient: either the wing root is above the crown, in which case
/// no wing-root bay exists, or it is not.
/// [`StationError::MainGearStationNotMeasured`] then reports the two heights
/// that decided it, and is the signal to register the aircraft's published
/// gear stations rather than to relax a gate.
///
/// # Errors
///
/// [`StationError::MainGearStationNotMeasured`] when no source anchor is
/// registered and the wing root sits above the fuselage crown.
pub(super) fn main_gear_station(
    fuselage: &Fuselage,
    main_wing: &Wing,
    geometry: &alas_config::GeometryConfig,
    mass_model: &MassModelConfig,
    landing_gear: Option<&LandingGearConfig>,
) -> Result<ComponentStation, StationError> {
    let (strut_length, ground_z) = gear_vertical_datum(fuselage, geometry, landing_gear);
    let resolved = resolved_gear_stations(fuselage, main_wing, mass_model, landing_gear);
    if !resolved.source_scaled {
        if let Some((wing_root_z_m, fuselage_crown_z_m)) =
            wing_root_above_fuselage_crown(main_wing, fuselage)
        {
            return Err(StationError::MainGearStationNotMeasured {
                wing_root_z_m,
                fuselage_crown_z_m,
            });
        }
    }
    let outcome = weighted_main_gear_station(&resolved, landing_gear);
    let x_main_gear = outcome.primary_station_ignoring_rejection();
    Ok(ComponentStation {
        position_m: [
            x_main_gear,
            0.0,
            gear_mass_centroid_z(ground_z, strut_length),
        ],
        extent_m: [0.0, 0.0, strut_length],
        method: if !resolved.source_scaled {
            "mlg_x_fraction_mac aft of MAC leading edge"
        } else {
            match outcome {
                Ok(ValidGearStation::WeightedCentroid { .. }) => {
                    "source-scaled MLG stations; wheel-count-weighted group centroid"
                }
                Ok(ValidGearStation::UnweightedMean { .. }) => {
                    "source-scaled MLG stations; unweighted strut mean (missing per-strut wheel counts)"
                }
                Ok(ValidGearStation::UniformStation { .. }) => "source-scaled primary MLG station",
                Err(_) => "source-scaled primary MLG station; malformed bogie list rejected",
            }
        },
    })
}

/// The fuselage's outer top surface at longitudinal station `x_m`, m.
///
/// The loft is linear between adjacent cross-sections
/// ([`alas_geom::aircraft::fuselage::Fuselage`]), so the crown between two of
/// them is the linear interpolation of their own crowns. A station forward of
/// the nose section or aft of the tail section takes that end section's
/// crown. `None` only for a fuselage with no cross-sections.
pub(super) fn fuselage_crown_z_m(fuselage: &Fuselage, x_m: f64) -> Option<f64> {
    let crown = |xsec: &FuselageXSec| xsec.xyz_c[2] + xsec.height / 2.0;
    let first = fuselage.xsecs.first()?;
    let last = fuselage.xsecs.last()?;
    if !x_m.is_finite() || x_m <= first.xyz_c[0] {
        return Some(crown(first));
    }
    if x_m >= last.xyz_c[0] {
        return Some(crown(last));
    }
    for pair in fuselage.xsecs.windows(2) {
        let (fwd, aft) = (&pair[0], &pair[1]);
        if x_m >= fwd.xyz_c[0] && x_m <= aft.xyz_c[0] {
            let span = aft.xyz_c[0] - fwd.xyz_c[0];
            if span <= 0.0 {
                return Some(crown(fwd).max(crown(aft)));
            }
            let blend = (x_m - fwd.xyz_c[0]) / span;
            return Some(crown(fwd) + blend * (crown(aft) - crown(fwd)));
        }
    }
    Some(crown(last))
}

/// `Some((wing_root_z_m, fuselage_crown_z_m))` when the main wing's root
/// leading edge sits strictly above the fuselage's outer surface at the same
/// longitudinal station: a high-wing layout, whose main gear cannot be
/// carried in the wing root.
///
/// `None` for every layout whose root is on or below the crown (low-wing,
/// mid-wing and shoulder-wing alike), which is the domain the wing-mounted
/// gear rule is stated for. The comparison is between two modelled heights
/// with no margin term, so it cannot drift with calibration.
pub(super) fn wing_root_above_fuselage_crown(
    main_wing: &Wing,
    fuselage: &Fuselage,
) -> Option<(f64, f64)> {
    let root = main_wing.xsecs.first()?;
    let root_z_m = root.xyz_le[2];
    let crown_z_m = fuselage_crown_z_m(fuselage, root.xyz_le[0])?;
    if root_z_m.is_finite() && crown_z_m.is_finite() && root_z_m > crown_z_m {
        Some((root_z_m, crown_z_m))
    } else {
        None
    }
}

/// Return the typed main-gear mass station resolution.
///
/// Delegates directly to [`alas_config::effective_main_gear_station`] to ensure
/// a single, unified domain validity gate across crates, eliminating duplicated logic.
/// The caller must match on the returned outcome (see [`ValidGearStation`] and
/// [`alas_config::GearStationRejection`]) rather than collapsing it to a scalar,
/// so a rejected explicit declaration stays distinguishable from every valid
/// outcome in the reported station `method`.
///
/// Physical agreement on missing counts (`None`):
/// When per-strut bogie wheel counts are not explicitly provided on a multi-strut
/// aircraft with distinct stations, both `alas_config` (in `resolved_station_positions`)
/// and `alas_mass` (here) agree on the declared conceptual approximation: an unweighted
/// arithmetic mean across all installed struts (`sum(x_i) / N`, equal strut load and mass split).
/// Using the same mean keeps the mass model and the config/performance models
/// on one station for multi-strut layouts (e.g. A380/A340; taking the primary
/// station alone would differ from the mean by 1.635 m there), while keeping
/// single-strut and uniform twin-gear layouts invariant at their physical
/// axle station.
///
/// When explicit counts are provided:
/// - Valid standard counts in `{2, 4, 6}` yield the wheel-count-weighted centroid.
/// - Malformed counts are rejected by `effective_main_gear_station` with a typed
///   rejection and fall back to the primary station.
pub(super) fn weighted_main_gear_station(
    resolved: &LandingGearStationPositions,
    landing_gear: Option<&LandingGearConfig>,
) -> alas_config::EffectiveMainGearStation {
    alas_config::effective_main_gear_station(
        &resolved.main_gear_x_m,
        landing_gear.and_then(|config| config.mlg_strut_bogie_wheels.as_deref()),
    )
}
