// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Geometry-derived component stations: where each ledger item sits and how
//! big it is.
//!
//! [`crate::statement`] builds an item-level mass ledger from a legacy
//! [`crate::breakdown::MassBreakdown`] plus loadable items; every item needs
//! a reference point and an extent before it has an inertia tensor. This
//! module is the one place those points and extents are derived from the
//! built [`Airplane`] geometry and the design configuration, so every
//! downstream consumer places the same component at the same station rather
//! than each inventing its own point.
//!
//! The placement fractions are Raymer, *Aircraft Design: A Conceptual
//! Approach*, 6th ed., ch. 15 (component centre-of-gravity guidance, table
//! 15.2 and the surrounding figures), and Torenbeek, *Synthesis of Subsonic
//! Airplane Design* (1982), ch. 8. The wing-mounted/fuselage-mounted engine
//! split and the systems/furnishings cabin fractions also match the
//! conventions [`crate::breakdown::coordinates::define_mass_coordinates`]
//! and NASA FLOPS (NASA/TM-2017-219627) already use elsewhere in this crate.
//!
//! [`component_stations`] always returns a wing station, falling back to a
//! Raymer point rather than propagating a
//! [`crate::wing_centroid::WingCentroidError`]. That is a deliberate
//! difference from [`crate::breakdown::MassCoordinateModel::StructuralWingbox`],
//! which reports the same error rather than silently substituting a point:
//! this module's contract is that a station exists for every candidate the
//! ledger is built from, structural-model failures included.
//!
//! The one placement that is *not* covered by that contract is the main
//! landing gear. Its fallback rule is stated for wing-mounted gear only, so
//! an aircraft outside that rule's domain with no registered gear stations
//! gets [`StationError::MainGearStationNotMeasured`] rather than a placement;
//! see [`main_gear_station`].

use alas_config::{
    DesignRequirements, EffectiveGearStationExt, GeometryConfig, LandingGearConfig,
    MassModelConfig, StructuresConfig, ValidGearStation,
};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec};
use alas_geom::aircraft::wing::Wing;

use crate::wing_centroid::wing_structural_centroid;

mod coordinates;
pub use coordinates::NOSE_GEAR_MASS_FRACTION;

/// Chordwise sample stations `Airfoil::max_thickness` is evaluated at to
/// recover a root section's thickness-to-chord ratio.
const THICKNESS_SAMPLE_FRACTIONS: [f64; 11] =
    [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0];

/// A component's reference point, bounding extent, and how it was placed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComponentStation {
    /// Reference point in the geometry frame (x aft, y starboard, z up), m.
    pub position_m: [f64; 3],
    /// Bounding extent `[length_x, width_y, height_z]`, m. Components not
    /// modelled with one of the three extents leave it `0.0` rather than an
    /// invented value.
    pub extent_m: [f64; 3],
    /// Stable label for which rule placed this station.
    pub method: &'static str,
}

/// One station per structural and systems group a mass ledger is built from.
#[derive(Debug, Clone, PartialEq)]
pub struct ComponentStations {
    /// Main wing structure.
    pub wing: ComponentStation,
    /// Horizontal stabilizer structure.
    pub horizontal_tail: ComponentStation,
    /// Vertical stabilizer structure.
    pub vertical_tail: ComponentStation,
    /// Fuselage structure.
    pub fuselage: ComponentStation,
    /// Nose landing gear.
    pub nose_gear: ComponentStation,
    /// Main landing gear.
    pub main_gear: ComponentStation,
    /// One station per engine nacelle, empty if the aircraft has none.
    pub propulsion_units: Vec<ComponentStation>,
    /// Avionics, electrical, hydraulics, ECS, APU and controls.
    pub systems: ComponentStation,
    /// Seats, monuments, insulation and cabin equipment.
    pub furnishings: ComponentStation,
    /// Crew, oil, catering and other operating items: the same physical
    /// station as [`Self::furnishings`], named separately so a ledger builder
    /// can place operating items without implying they are furnishings.
    pub operating_items: ComponentStation,
    /// Lumped payload centroid, used when no per-item payload layout exists.
    pub payload_fallback: ComponentStation,
}

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
    /// apply to this layout (see [`main_gear_station`]).
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

/// Derive every component's placement and extent from the built geometry.
///
/// # Errors
///
/// [`StationError::MissingMainWing`] or [`StationError::MissingFuselage`] if
/// the airplane lacks either. [`StationError::NonFiniteGeometry`] if any
/// resolved station is not finite, which only degenerate input geometry
/// (zero span, coincident sections) can produce.
/// [`StationError::MainGearStationNotMeasured`] if the aircraft has no
/// main-gear station this model can supply; see [`main_gear_station`].
pub fn component_stations(
    plane: &Airplane,
    geometry: &GeometryConfig,
    requirements: &DesignRequirements,
    mass_model: &MassModelConfig,
    structures: &StructuresConfig,
) -> Result<ComponentStations, StationError> {
    component_stations_internal(plane, geometry, requirements, mass_model, structures, None)
}

/// Derive component stations with the active landing-gear source geometry.
///
/// This additive entry point keeps the original API available to standalone
/// callers while allowing the product ledger and CG consumers to use the same
/// normalized gear stations as performance and report code. The main-gear
/// lumped station is a wheel-count-weighted centroid when a heterogeneous
/// source topology is configured. Equal weighting of unlike bogies is never
/// used; without explicit per-strut counts the primary main-gear station is
/// retained as the conservative geometry-only fallback.
///
/// # Errors
///
/// The same set [`component_stations`] reports.
pub fn component_stations_with_gear(
    plane: &Airplane,
    geometry: &GeometryConfig,
    requirements: &DesignRequirements,
    mass_model: &MassModelConfig,
    structures: &StructuresConfig,
    landing_gear: &LandingGearConfig,
) -> Result<ComponentStations, StationError> {
    component_stations_internal(
        plane,
        geometry,
        requirements,
        mass_model,
        structures,
        Some(landing_gear),
    )
}

fn component_stations_internal(
    plane: &Airplane,
    geometry: &GeometryConfig,
    requirements: &DesignRequirements,
    mass_model: &MassModelConfig,
    structures: &StructuresConfig,
    landing_gear: Option<&LandingGearConfig>,
) -> Result<ComponentStations, StationError> {
    let main_wing = plane
        .wings
        .iter()
        .find(|wing| wing.name == "Main Wing")
        .ok_or(StationError::MissingMainWing)?;
    // The same "named, else the Nth wing, else the main wing" fallback
    // `calculate_component_masses` uses: every product build supplies all
    // three wings, so this only matters for a hand-built test fixture.
    let hstab = plane
        .wings
        .iter()
        .find(|wing| wing.name == "Horizontal Stabilizer")
        .unwrap_or_else(|| plane.wings.get(1).unwrap_or(main_wing));
    let vstab = plane
        .wings
        .iter()
        .find(|wing| wing.name == "Vertical Stabilizer")
        .unwrap_or_else(|| plane.wings.get(2).unwrap_or(main_wing));
    let fuselage = plane
        .fuselages
        .first()
        .ok_or(StationError::MissingFuselage)?;

    let stations = ComponentStations {
        wing: wing_station(main_wing, requirements, structures),
        horizontal_tail: horizontal_tail_station(hstab),
        vertical_tail: vertical_tail_station(vstab),
        fuselage: fuselage_station(fuselage, geometry),
        nose_gear: nose_gear_station(fuselage, main_wing, geometry, mass_model, landing_gear),
        main_gear: main_gear_station(fuselage, main_wing, geometry, mass_model, landing_gear)?,
        propulsion_units: propulsion_stations(plane),
        systems: cabin_station(
            fuselage,
            geometry,
            0.45,
            "cabin_start + 0.45 cabin length",
            [0.0, 0.0],
        ),
        furnishings: cabin_station(
            fuselage,
            geometry,
            0.50,
            "cabin_start + 0.50 cabin length",
            [geometry.fuselage.diameter_m, 2.0],
        ),
        operating_items: cabin_station(
            fuselage,
            geometry,
            0.50,
            "cabin_start + 0.50 cabin length",
            [geometry.fuselage.diameter_m, 2.0],
        ),
        payload_fallback: payload_fallback_station(fuselage, geometry, requirements, mass_model),
    };
    validate_finite(&stations)?;
    Ok(stations)
}

/// The nose (`x` start) and length of `fuselage`, plus its centreline `z`.
fn fuselage_datum(fuselage: &Fuselage) -> (f64, f64, f64) {
    let start_x = fuselage.xsecs.first().map_or(0.0, |xsec| xsec.xyz_c[0]);
    let end_x = fuselage.xsecs.last().map_or(start_x, |xsec| xsec.xyz_c[0]);
    let z = fuselage.xsecs.first().map_or(0.0, |xsec| xsec.xyz_c[2]);
    (start_x, end_x - start_x, z)
}

/// Root chord times root thickness-to-chord ratio, m: the wing (and, by
/// the same lifting-surface reasoning, horizontal-tail) height extent.
fn root_thickness_m(wing: &Wing) -> f64 {
    match wing.xsecs.first() {
        Some(root) => root.chord * root.airfoil.max_thickness(&THICKNESS_SAMPLE_FRACTIONS),
        None => 0.0,
    }
}

/// The point at `chord_fraction` of `wing`'s overall mean aerodynamic chord
/// aft of its leading edge, at `span_fraction` of the way from root to tip
/// along the loft: Raymer's conceptual component centre-of-gravity rule
/// (ch. 15: 40% MAC at 35% semispan for the wing, 42% MAC at 38% semispan
/// for a tail).
///
/// Distance is measured along the YZ-projected loft, the same quantity
/// [`Wing::aerodynamic_center`]'s own quadrature spans, so this works
/// whether the surface's span runs along Y (a lifting surface) or along Z (a
/// vertical tail). This is computed independently of
/// [`Wing::aerodynamic_center`] rather than by calling it at
/// `chord_fraction`, because that method's area weighting targets the
/// aerodynamic centre, not the specific span station Raymer's rule names.
fn raymer_component_point(wing: &Wing, chord_fraction: f64, span_fraction: f64) -> [f64; 3] {
    if wing.xsecs.len() < 2 {
        let (le, chord) = wing
            .xsecs
            .first()
            .map_or(([0.0; 3], 0.0), |xsec| (xsec.xyz_le, xsec.chord));
        let y = if wing.symmetric { 0.0 } else { le[1] };
        return [le[0] + chord_fraction * chord, y, le[2]];
    }

    let mac = wing.mean_aerodynamic_chord();
    let mac_le_x = wing.aerodynamic_center(0.0)[0];

    let mut cumulative = Vec::with_capacity(wing.xsecs.len());
    cumulative.push(0.0);
    for pair in wing.xsecs.windows(2) {
        let dy = pair[1].xyz_le[1] - pair[0].xyz_le[1];
        let dz = pair[1].xyz_le[2] - pair[0].xyz_le[2];
        cumulative.push(cumulative.last().copied().unwrap_or(0.0) + dy.hypot(dz));
    }
    let total = cumulative.last().copied().unwrap_or(0.0);
    let target = span_fraction.clamp(0.0, 1.0) * total;

    let mut panel = cumulative.len().saturating_sub(2);
    for index in 0..cumulative.len().saturating_sub(1) {
        if target <= cumulative[index + 1] {
            panel = index;
            break;
        }
    }
    let panel_span = (cumulative[panel + 1] - cumulative[panel]).max(1e-12);
    let blend = ((target - cumulative[panel]) / panel_span).clamp(0.0, 1.0);
    let root = wing.xsecs[panel].xyz_le;
    let tip = wing.xsecs[panel + 1].xyz_le;
    let y = root[1] + blend * (tip[1] - root[1]);
    let z = root[2] + blend * (tip[2] - root[2]);

    [
        mac_le_x + chord_fraction * mac,
        if wing.symmetric { 0.0 } else { y },
        z,
    ]
}

/// The main-wing station: the integrated wingbox centroid when it resolves,
/// else the Raymer 40%-MAC/35%-semispan point.
fn wing_station(
    wing: &Wing,
    requirements: &DesignRequirements,
    structures: &StructuresConfig,
) -> ComponentStation {
    let extent_m = [
        wing.mean_aerodynamic_chord(),
        wing.reference_span(),
        root_thickness_m(wing),
    ];
    match wing_structural_centroid(wing, requirements, structures) {
        Ok(centroid) => ComponentStation {
            position_m: centroid.xyz_m,
            extent_m,
            method: "integrated wingbox",
        },
        Err(_) => ComponentStation {
            position_m: raymer_component_point(wing, 0.40, 0.35),
            extent_m,
            method: "Raymer 40% MAC at 35% semispan",
        },
    }
}

/// The horizontal-tail station: Raymer's 42%-MAC/38%-semispan point, with
/// the same lifting-surface extent convention as the main wing.
fn horizontal_tail_station(hstab: &Wing) -> ComponentStation {
    ComponentStation {
        position_m: raymer_component_point(hstab, 0.42, 0.38),
        extent_m: [
            hstab.mean_aerodynamic_chord(),
            hstab.reference_span(),
            root_thickness_m(hstab),
        ],
        method: "Raymer 42% MAC at 38% semispan",
    }
}

/// The vertical-tail station. A fin is an XZ surface, so its "semispan" is
/// height: [`raymer_component_point`]'s YZ-projected distance already
/// measures that, and the extent's height comes from the xsecs' own z-range
/// rather than a spanwise dimension.
fn vertical_tail_station(vstab: &Wing) -> ComponentStation {
    let (min_z, max_z) = vstab.xsecs.iter().fold(
        (f64::INFINITY, f64::NEG_INFINITY),
        |(min_z, max_z), xsec| (min_z.min(xsec.xyz_le[2]), max_z.max(xsec.xyz_le[2])),
    );
    let height = if max_z > min_z { max_z - min_z } else { 0.0 };
    ComponentStation {
        position_m: raymer_component_point(vstab, 0.42, 0.38),
        extent_m: [vstab.mean_aerodynamic_chord(), 0.0, height],
        method: "Raymer 42% MAC at 38% fin height",
    }
}

/// Whether every configured engine sits close enough to the centreline to
/// be fuselage- or tail-mounted rather than wing-mounted.
///
/// An aircraft with no configured engines is treated as wing-mounted, the
/// more common transport layout, rather than vacuously fuselage-mounted.
fn engines_are_fuselage_mounted(geometry: &GeometryConfig) -> bool {
    let half_width = geometry.fuselage.diameter_m / 2.0;
    !geometry.engine.spanwise_positions_m.is_empty()
        && geometry
            .engine
            .spanwise_positions_m
            .iter()
            .all(|y| y.abs() <= half_width)
}

/// The fuselage station: 0.45 of fuselage length from the nose for
/// wing-mounted engines, 0.50 for fuselage/tail-mounted engines (Raymer).
fn fuselage_station(fuselage: &Fuselage, geometry: &GeometryConfig) -> ComponentStation {
    let (start_x, length, z) = fuselage_datum(fuselage);
    let fuselage_mounted = engines_are_fuselage_mounted(geometry);
    let fraction = if fuselage_mounted { 0.50 } else { 0.45 };
    ComponentStation {
        position_m: [start_x + length * fraction, 0.0, z],
        extent_m: [
            length,
            geometry.fuselage.diameter_m,
            geometry.fuselage.effective_height_m(),
        ],
        method: if fuselage_mounted {
            "0.50 fuselage length (fuselage/tail-mounted engines)"
        } else {
            "0.45 fuselage length (wing-mounted engines)"
        },
    }
}

/// The strut length and ground-contact height both gear legs share, m.
///
/// Vertical placement (fuselage bottom less a strut length of 0.25 fuselage
/// diameters) is a conceptual-design assumption stated here because neither
/// gear leg's real strut geometry is modelled.
fn gear_vertical_datum(fuselage: &Fuselage, geometry: &GeometryConfig) -> (f64, f64) {
    let (_, _, z) = fuselage_datum(fuselage);
    let diameter = geometry.fuselage.diameter_m;
    let strut_length = 0.25 * diameter;
    (strut_length, z - diameter / 2.0 - strut_length)
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
) -> alas_config::LandingGearStationPositions {
    let (start_x, length, _) = fuselage_datum(fuselage);
    let fallback_x_nlg = start_x + length * mass_model.nlg_x_fraction;
    let mac = main_wing.mean_aerodynamic_chord();
    let mac_le_x = main_wing.aerodynamic_center(0.0)[0];
    let fallback_x_mlg = mac_le_x + mass_model.mlg_x_fraction_mac * mac;
    landing_gear
        .map(|config| {
            config.resolved_station_positions(fallback_x_nlg, fallback_x_mlg, start_x, length)
        })
        .unwrap_or_else(|| alas_config::LandingGearStationPositions {
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
fn nose_gear_station(
    fuselage: &Fuselage,
    main_wing: &Wing,
    geometry: &GeometryConfig,
    mass_model: &MassModelConfig,
    landing_gear: Option<&LandingGearConfig>,
) -> ComponentStation {
    let (strut_length, ground_z) = gear_vertical_datum(fuselage, geometry);
    let resolved = resolved_gear_stations(fuselage, main_wing, mass_model, landing_gear);
    ComponentStation {
        position_m: [resolved.x_nlg_m, 0.0, ground_z],
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
fn main_gear_station(
    fuselage: &Fuselage,
    main_wing: &Wing,
    geometry: &GeometryConfig,
    mass_model: &MassModelConfig,
    landing_gear: Option<&LandingGearConfig>,
) -> Result<ComponentStation, StationError> {
    let (strut_length, ground_z) = gear_vertical_datum(fuselage, geometry);
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
        position_m: [x_main_gear, 0.0, ground_z],
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
fn fuselage_crown_z_m(fuselage: &Fuselage, x_m: f64) -> Option<f64> {
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
fn wing_root_above_fuselage_crown(main_wing: &Wing, fuselage: &Fuselage) -> Option<(f64, f64)> {
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
/// outcome in the reported station `method` (gear-integration-review.md F5).
///
/// Without per-strut bogie wheel counts on a multi-strut aircraft, both this
/// crate and `alas_config::LandingGearConfig::resolved_station_positions` use
/// the unweighted strut mean (`sum(x_i) / N`, equal strut load); taking the
/// primary station instead would put the mass model 1.635 m forward of the
/// performance model on a multi-strut layout (A380/A340). Single-strut and
/// uniform twin-gear layouts sit at their axle station either way.
///
/// When explicit counts are provided:
/// - Valid standard counts in `{2, 4, 6}` yield the wheel-count-weighted centroid.
/// - Malformed counts are rejected by `effective_main_gear_station` with a typed
///   rejection and fall back to the primary station.
fn weighted_main_gear_station(
    resolved: &alas_config::LandingGearStationPositions,
    landing_gear: Option<&LandingGearConfig>,
) -> alas_config::EffectiveMainGearStation {
    alas_config::effective_main_gear_station(
        &resolved.main_gear_x_m,
        landing_gear.and_then(|config| config.mlg_strut_bogie_wheels.as_deref()),
    )
}

/// One station per engine nacelle, at each nacelle's mid-length point.
fn propulsion_stations(plane: &Airplane) -> Vec<ComponentStation> {
    plane
        .fuselages
        .iter()
        .filter(|fuselage| fuselage.name.contains("Nacelle"))
        .map(|nacelle| {
            let start = nacelle.xsecs.first().map_or([0.0; 3], |xsec| xsec.xyz_c);
            let end_x = nacelle.xsecs.last().map_or(start[0], |xsec| xsec.xyz_c[0]);
            let length = end_x - start[0];
            let radius = nacelle
                .xsecs
                .iter()
                .map(|xsec| xsec.width.max(xsec.height) / 2.0)
                .fold(0.0, f64::max);
            ComponentStation {
                position_m: [start[0] + length * 0.5, start[1], start[2]],
                extent_m: [length, 2.0 * radius, 2.0 * radius],
                method: "nacelle mid-length",
            }
        })
        .collect()
}

/// The systems, furnishings and operating-items station: `fraction` of the
/// installed cabin length aft of its start, with `extent_yz` as the
/// width/height pair (`[0.0, 0.0]` for the systems rod, which only extends
/// along x).
fn cabin_station(
    fuselage: &Fuselage,
    geometry: &GeometryConfig,
    fraction: f64,
    method: &'static str,
    extent_yz: [f64; 2],
) -> ComponentStation {
    let (_, length, z) = fuselage_datum(fuselage);
    let cabin_start = geometry.fuselage.cabin_start_x_m;
    let cabin_len = (length - cabin_start - geometry.fuselage.tailcone_length_m).max(1.0);
    ComponentStation {
        position_m: [cabin_start + fraction * cabin_len, 0.0, z],
        extent_m: [cabin_len, extent_yz[0], extent_yz[1]],
        method,
    }
}

/// The payload fallback station: a lumped planning payload distributed about
/// the cabin's own centroid, occupying `occupied_len` of it.
///
/// # Why this is the cabin centre and not the forward bulkhead
///
/// This station is only reached when no per-item payload layout exists
/// ([`crate::statement`]), so what it has to represent is a *planning* payload:
/// a mass stated for the aircraft with no loading instruction attached. A
/// planning payload is distributed over the usable cabin floor and an operator
/// trims it into the certified envelope; it is not a forward-limit load.
///
/// The previous form placed it at `cabin_start + 0.50 * occupied_len`, the
/// centre of a block that always begins at the **forward bulkhead**. Whenever
/// the planning payload does not fill the cabin that is the aircraft's forward
/// loading extreme, applied as if it were the neutral case, and it is
/// asymmetric for no stated reason: the same module places `systems` and
/// `furnishings` as fractions of the *cabin*, not of an occupied block.
///
/// Measured across the registered aircraft
/// (`alas-mass/examples/payload_station_matrix.rs`), the forward-bulkhead form
/// puts the centroid this far forward of the cabin centre, and moves the centre
/// of gravity at maximum take-off mass by:
///
/// | preset | cabin fill | forward error | CG at MTOW |
/// |---|---:|---:|---:|
/// | ATR72-600 | 0.495 | 4.583 m | **1.435 m** |
/// | A220-300 | 0.570 | 6.125 m | 1.178 m |
/// | A320-200 | 0.706 | 3.910 m | 0.752 m |
/// | AVE | 0.771 | 6.485 m | 0.633 m |
/// | A340-300 | 0.785 | 4.955 m | 0.553 m |
/// | B787-9 | 0.800 | 4.530 m | 0.516 m |
/// | DC-10 | 0.811 | 3.650 m | 0.352 m |
/// | A380-800 | **1.000** | **0.000 m** | **0.000 m** |
///
/// On the ATR 72-600 that 1.435 m is **57.4 %** of its 2.499 m mean
/// aerodynamic chord, which is the scale of that aircraft's open static-margin
/// and nose-gear-load findings.
///
/// The correction consults no target and introduces no coefficient: it is the
/// one symmetric placement available, and where the cabin is full it is
/// **exactly** the previous value, which the A380-800 row shows and a test
/// pins. The occupied length is kept as the station's `extent_m`, which is what
/// that field describes.
///
/// The previous comment's stated intent was that "a stretched fuselage does not
/// move an unchanged payload's centroid aft for free". That is an optimizer
/// stability concern rather than a physical one (a longer cabin carrying the
/// same payload over a uniformly loaded floor *does* move its centroid aft),
/// and an implausible stretch belongs to the geometry plausibility windows,
/// which own it. Recorded here rather than preserved by placing mass where it
/// is not.
fn payload_fallback_station(
    fuselage: &Fuselage,
    geometry: &GeometryConfig,
    requirements: &DesignRequirements,
    mass_model: &MassModelConfig,
) -> ComponentStation {
    let (_, length, z) = fuselage_datum(fuselage);
    let cabin_start = geometry.fuselage.cabin_start_x_m;
    let cabin_len = (length - cabin_start - geometry.fuselage.tailcone_length_m).max(1.0);
    let occupied_len =
        cabin_len.min(requirements.payload_kg() / mass_model.cabin_payload_density_kg_m.max(1e-6));
    ComponentStation {
        position_m: [cabin_start + 0.50 * cabin_len, 0.0, z],
        extent_m: [occupied_len, geometry.fuselage.diameter_m, 2.0],
        method: "cabin centre, planning payload distributed about it",
    }
}

/// Reject a result with a non-finite position or extent anywhere in it.
fn validate_finite(stations: &ComponentStations) -> Result<(), StationError> {
    let named: [(&'static str, &ComponentStation); 9] = [
        ("wing", &stations.wing),
        ("horizontal_tail", &stations.horizontal_tail),
        ("vertical_tail", &stations.vertical_tail),
        ("fuselage", &stations.fuselage),
        ("nose_gear", &stations.nose_gear),
        ("main_gear", &stations.main_gear),
        ("systems", &stations.systems),
        ("furnishings", &stations.furnishings),
        ("payload_fallback", &stations.payload_fallback),
    ];
    let is_finite = |station: &ComponentStation| {
        station
            .position_m
            .iter()
            .chain(station.extent_m.iter())
            .all(|value| value.is_finite())
    };
    for (name, station) in named {
        if !is_finite(station) {
            return Err(StationError::NonFiniteGeometry(name));
        }
    }
    for unit in &stations.propulsion_units {
        if !is_finite(unit) {
            return Err(StationError::NonFiniteGeometry("propulsion_units"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::{AlasConfig, DesignVector};
    use alas_geom::builder::AircraftBuilder;

    // Building the default product aircraft is an assertion that the
    // default configuration is valid, so a failed expect here is that
    // assertion failing, not a library invariant being broken.
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    fn default_stations() -> ComponentStations {
        let config = AlasConfig::default();
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&DesignVector::default()), true)
            .expect("the default geometry configuration builds");
        component_stations(
            &plane,
            &config.geometry,
            &config.requirements,
            &config.mass_model,
            &config.structures,
        )
        .expect("the default aircraft resolves every station")
    }

    #[test]
    fn every_default_station_is_finite() {
        let stations = default_stations();
        let all_finite = |station: &ComponentStation| {
            station
                .position_m
                .iter()
                .chain(station.extent_m.iter())
                .all(|value| value.is_finite())
        };
        assert!(all_finite(&stations.wing));
        assert!(all_finite(&stations.horizontal_tail));
        assert!(all_finite(&stations.vertical_tail));
        assert!(all_finite(&stations.fuselage));
        assert!(all_finite(&stations.nose_gear));
        assert!(all_finite(&stations.main_gear));
        assert!(all_finite(&stations.systems));
        assert!(all_finite(&stations.furnishings));
        assert!(all_finite(&stations.operating_items));
        assert!(all_finite(&stations.payload_fallback));
        for unit in &stations.propulsion_units {
            assert!(all_finite(unit));
        }
    }

    #[test]
    fn the_main_gear_sits_aft_of_the_nose_gear() {
        let stations = default_stations();
        assert!(stations.main_gear.position_m[0] > stations.nose_gear.position_m[0]);
    }

    #[test]
    fn heterogeneous_main_gear_centroid_uses_explicit_wheel_count_weights() {
        let config = LandingGearConfig {
            n_mlg_struts: 4,
            mlg_strut_bogie_wheels: Some(vec![4, 4, 6, 6]),
            reference_station_fuselage_length_m: Some(72.73),
            reference_nlg_x_fraction: Some(4.97 / 72.73),
            reference_mlg_x_fractions: Some(vec![
                33.58 / 72.73,
                33.58 / 72.73,
                36.85 / 72.73,
                36.85 / 72.73,
            ]),
            ..Default::default()
        };
        let resolved = config.resolved_station_positions(4.97, 33.58, 0.0, 72.73);
        let outcome = weighted_main_gear_station(&resolved, Some(&config));
        let expected = (4.0 * 33.58 + 4.0 * 33.58 + 6.0 * 36.85 + 6.0 * 36.85) / 20.0;
        let unweighted = (33.58 + 33.58 + 36.85 + 36.85) / 4.0;
        match outcome {
            Ok(ValidGearStation::WeightedCentroid { station_m, .. }) => {
                assert!((station_m - expected).abs() < 1.0e-12);
                assert!((station_m - unweighted).abs() > 1.0e-3);
            }
            other => panic!("expected a valid WeightedCentroid outcome, got {other:?}"),
        }
    }

    #[test]
    fn multi_strut_with_none_counts_yields_identical_station_across_crates() {
        // Negative-control test required by Packet 4:
        // When per-strut counts are None on a multi-strut aircraft with distinct stations
        // (e.g. A380 geometry: wing gear at 33.58 m, body gear at 36.85 m),
        // both alas_config (resolved.x_mlg_m) and alas_mass (weighted_main_gear_station)
        // must yield the exact SAME station (the declared unweighted mean 35.215 m).
        //
        // BEFORE THIS FIX:
        // - alas_config resolved.x_mlg_m was 35.215 m (unweighted mean)
        // - alas_mass weighted_main_gear_station returned 33.58 m (primary_mlg_x_m)
        // creating a 1.635 m behavioral divergence across crates.
        let config = LandingGearConfig {
            n_mlg_struts: 4,
            mlg_strut_bogie_wheels: None,
            reference_station_fuselage_length_m: Some(72.73),
            reference_nlg_x_fraction: Some(4.97 / 72.73),
            reference_mlg_x_fractions: Some(vec![
                33.58 / 72.73,
                33.58 / 72.73,
                36.85 / 72.73,
                36.85 / 72.73,
            ]),
            ..Default::default()
        };
        let resolved = config.resolved_station_positions(4.97, 33.58, 0.0, 72.73);
        let outcome = weighted_main_gear_station(&resolved, Some(&config));

        let expected_unweighted_mean = (33.58 * 2.0 + 36.85 * 2.0) / 4.0; // 35.215 m

        let mass_station = match outcome {
            Ok(ValidGearStation::UnweightedMean { station_m, .. }) => station_m,
            other => panic!("missing counts must yield UnweightedMean, got {other:?}"),
        };
        assert!(
            (resolved.x_mlg_m - expected_unweighted_mean).abs() < 1.0e-12,
            "alas_config resolved.x_mlg_m was {}, expected {}",
            resolved.x_mlg_m,
            expected_unweighted_mean
        );
        assert!(
            (mass_station - expected_unweighted_mean).abs() < 1.0e-12,
            "alas_mass station was {}, expected {}",
            mass_station,
            expected_unweighted_mean
        );
        // CRITICAL CROSS-CRATE AGREEMENT ASSERTION:
        assert_eq!(
            mass_station, resolved.x_mlg_m,
            "Cross-crate divergence! alas_mass ({mass_station}) != alas_config ({})",
            resolved.x_mlg_m
        );
    }

    #[test]
    fn malformed_bogie_weights_fall_back_unweighted_to_primary_station() {
        let config = LandingGearConfig {
            n_mlg_struts: 4,
            mlg_strut_bogie_wheels: Some(vec![4, 3, 6, 6]), // 3 is non-standard
            reference_station_fuselage_length_m: Some(72.73),
            reference_nlg_x_fraction: Some(4.97 / 72.73),
            reference_mlg_x_fractions: Some(vec![
                33.58 / 72.73,
                33.58 / 72.73,
                36.85 / 72.73,
                36.85 / 72.73,
            ]),
            ..Default::default()
        };
        let resolved = config.resolved_station_positions(4.97, 33.58, 0.0, 72.73);
        let outcome = weighted_main_gear_station(&resolved, Some(&config));
        match outcome {
            Err(rejection) => {
                assert!(
                    rejection.reason().contains("non-standard"),
                    "unexpected rejection reason: {}",
                    rejection.reason()
                );
                assert_eq!(
                    rejection.primary_station_ignoring_rejection(),
                    resolved.primary_mlg_x_m()
                );
            }
            other => panic!("malformed bogie counts must be rejected, got {other:?}"),
        }
    }

    #[test]
    fn both_tails_sit_aft_of_the_wing() {
        let stations = default_stations();
        assert!(stations.horizontal_tail.position_m[0] > stations.wing.position_m[0]);
        assert!(stations.vertical_tail.position_m[0] > stations.wing.position_m[0]);
    }

    #[test]
    fn the_default_wing_resolves_the_integrated_wingbox() {
        // The default structures configuration is a valid two-spar wingbox,
        // so the fallback point should never be exercised for it.
        let stations = default_stations();
        assert_eq!(stations.wing.method, "integrated wingbox");
    }

    #[test]
    fn a_missing_main_wing_is_a_typed_error_not_a_panic() {
        let config = AlasConfig::default();
        let plane = Airplane {
            name: "No Main Wing".to_owned(),
            xyz_ref: [0.0; 3],
            wings: Vec::new(),
            fuselages: Vec::new(),
            s_ref: 1.0,
            c_ref: 1.0,
            b_ref: 1.0,
        };
        let result = component_stations(
            &plane,
            &config.geometry,
            &config.requirements,
            &config.mass_model,
            &config.structures,
        );
        assert_eq!(result, Err(StationError::MissingMainWing));
    }

    #[test]
    fn engines_within_the_fuselage_half_width_are_fuselage_mounted() {
        let mut geometry = GeometryConfig::default();
        geometry.fuselage.diameter_m = 4.0;
        geometry.engine.spanwise_positions_m = vec![0.5, -0.5];
        assert!(engines_are_fuselage_mounted(&geometry));
        geometry.engine.spanwise_positions_m = vec![9.0, -9.0];
        assert!(!engines_are_fuselage_mounted(&geometry));
    }

    #[test]
    fn a_planning_payload_sits_on_the_cabin_centre_and_a_full_cabin_is_unchanged() {
        use alas_config::presets;
        // A cabin the planning payload fills exactly puts the two placements
        // in the same place, which is what makes the correction symmetric
        // rather than a shift: the A380-800 is that case.
        // The unfilled case was the ATR 72-600 (cabin fill 0.495) until that
        // preset stopped resolving a main-gear station; the A220-300 (0.570)
        // is the next-least-filled cabin and tests the same asymmetry.
        for (preset, fills_the_cabin) in [("A380-800", true), ("A220-300", false)] {
            let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
                .unwrap_or_else(|error| panic!("{error}"));
            let registered = presets::get(preset).unwrap_or_else(|error| panic!("{error}"));
            let plane = AircraftBuilder::new(Some(config.geometry.clone()))
                .build(Some(&registered.design_vector), true)
                .expect("a registered aircraft builds");
            let stations = component_stations(
                &plane,
                &config.geometry,
                &config.requirements,
                &config.mass_model,
                &config.structures,
            )
            .expect("a registered aircraft resolves its stations");

            let fuselage = plane.fuselages.first().expect("a fuselage");
            let (_, length, _) = fuselage_datum(fuselage);
            let cabin_start = config.geometry.fuselage.cabin_start_x_m;
            let cabin_len =
                (length - cabin_start - config.geometry.fuselage.tailcone_length_m).max(1.0);
            let cabin_centre = cabin_start + 0.5 * cabin_len;
            let payload = &stations.payload_fallback;

            assert!(
                (payload.position_m[0] - cabin_centre).abs() < 1.0e-9,
                "{preset}: payload at {} against a cabin centre of {cabin_centre}",
                payload.position_m[0]
            );
            // The occupied length is what the extent describes, and it never
            // exceeds the cabin.
            assert!(payload.extent_m[0] <= cabin_len + 1.0e-9);
            assert!(payload.extent_m[0] > 0.0);

            // The forward-bulkhead placement, for the comparison.
            let nose_first = cabin_start + 0.5 * payload.extent_m[0];
            if fills_the_cabin {
                assert!(
                    (nose_first - cabin_centre).abs() < 1.0e-9,
                    "{preset} fills its cabin, so the two placements must coincide"
                );
            } else {
                assert!(
                    cabin_centre - nose_first > 1.0,
                    "{preset} does not fill its cabin, so the two must differ"
                );
            }
        }
    }

    /// Build a registered preset's airplane and resolve its stations through
    /// the gear-aware entry point, the way every product consumer does.
    #[allow(clippy::expect_used)]
    fn preset_stations(preset: &str) -> Result<ComponentStations, StationError> {
        use alas_config::presets;
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
            .unwrap_or_else(|error| panic!("{error}"));
        let registered = presets::get(preset).unwrap_or_else(|error| panic!("{error}"));
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&registered.design_vector), true)
            .expect("a registered aircraft builds");
        component_stations_with_gear(
            &plane,
            &config.geometry,
            &config.requirements,
            &config.mass_model,
            &config.structures,
            &config.landing_gear,
        )
    }

    /// Recreate the explicit missing-datum fixture without depending on the
    /// station status of a registered aircraft.  ATR's published anchors are
    /// intentionally cleared here so this test continues to exercise the
    /// fail-closed high-wing path after the real ATR datum is registered.
    #[allow(clippy::expect_used)]
    fn unmeasured_preset_stations(preset: &str) -> Result<ComponentStations, StationError> {
        use alas_config::presets;
        let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
            .unwrap_or_else(|error| panic!("{error}"));
        config.landing_gear.reference_station_fuselage_length_m = None;
        config.landing_gear.reference_nlg_x_fraction = None;
        config.landing_gear.reference_mlg_x_fractions = None;
        let registered = presets::get(preset).unwrap_or_else(|error| panic!("{error}"));
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&registered.design_vector), true)
            .expect("a registered aircraft builds");
        component_stations_with_gear(
            &plane,
            &config.geometry,
            &config.requirements,
            &config.mass_model,
            &config.structures,
            &config.landing_gear,
        )
    }

    #[test]
    fn a_high_wing_aircraft_without_registered_stations_gets_no_main_gear_station() {
        // The ATR 72-600 is the registered high-wing, sponson-gear aircraft
        // and it registers no station anchor, so the wing-mounted fallback is
        // outside its stated domain. The failure must be the typed
        // missing-datum one, carrying the two heights that decided it: a
        // silent fallback here is what produced a main-gear station forward
        // of the centre of gravity, and with it a negative static nose
        // reaction at every loading state.
        match unmeasured_preset_stations("ATR72-600") {
            Err(StationError::MainGearStationNotMeasured {
                wing_root_z_m,
                fuselage_crown_z_m,
            }) => {
                assert!(
                    wing_root_z_m > fuselage_crown_z_m,
                    "the reported wing root ({wing_root_z_m} m) must be above the reported crown \
                     ({fuselage_crown_z_m} m)"
                );
            }
            other => panic!(
                "a high-wing aircraft with no registered gear stations must report \
                 MainGearStationNotMeasured, got {other:?}"
            ),
        }
    }

    #[test]
    fn low_wing_aircraft_keep_the_station_they_already_had() {
        // The refusal is scoped to the layout the fallback rule excludes, not
        // to the absence of a registered anchor. The B787-9 and the DC-10
        // register no anchor either and must still resolve through the same
        // wing-mounted fallback, by the same method string, as before; the
        // A320-200 registers one and must still be source-scaled.
        for preset in ["B787-9", "DC-10"] {
            let stations = preset_stations(preset).unwrap_or_else(|error| {
                panic!("{preset} must still resolve its stations: {error}")
            });
            assert_eq!(
                stations.main_gear.method, "mlg_x_fraction_mac aft of MAC leading edge",
                "{preset} must keep the wing-mounted fallback it already used"
            );
            assert!(stations.main_gear.position_m[0] > stations.nose_gear.position_m[0]);
        }
        let a320 = preset_stations("A320-200")
            .unwrap_or_else(|error| panic!("the A320-200 must resolve its stations: {error}"));
        assert!(
            a320.main_gear.method.starts_with("source-scaled"),
            "the A320-200 registers a station anchor, got method {:?}",
            a320.main_gear.method
        );
    }

    #[test]
    fn a_registered_station_anchor_is_honoured_on_a_high_wing_layout() {
        // The refusal is about a missing datum, not about the layout itself:
        // give the same high-wing geometry a complete source anchor and the
        // station resolves from it. This is the path that closes the ATR, and
        // it is exercised here so the refusal cannot be mistaken for a rule
        // that high-wing aircraft are unsupported.
        use alas_config::presets;
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": "ATR72-600" }))
            .unwrap_or_else(|error| panic!("{error}"));
        let registered = presets::get("ATR72-600").unwrap_or_else(|error| panic!("{error}"));
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&registered.design_vector), true)
            .expect("the ATR builds");
        // Illustrative fractions of the active fuselage length, not ATR data:
        // this test asserts the plumbing, and asserts nothing about where the
        // ATR's gear actually is.
        let landing_gear = LandingGearConfig {
            reference_station_fuselage_length_m: Some(27.166),
            reference_nlg_x_fraction: Some(0.1),
            reference_mlg_x_fractions: Some(vec![0.45, 0.45]),
            ..config.landing_gear.clone()
        };
        let stations = component_stations_with_gear(
            &plane,
            &config.geometry,
            &config.requirements,
            &config.mass_model,
            &config.structures,
            &landing_gear,
        )
        .expect("a registered anchor resolves the main-gear station");
        assert!(stations.main_gear.method.starts_with("source-scaled"));
        assert!(stations.main_gear.position_m[0] > stations.nose_gear.position_m[0]);
    }

    #[test]
    fn a_wing_root_on_the_fuselage_crown_is_not_a_high_wing_layout() {
        // The boundary is a strict comparison with no margin: a root exactly
        // on the crown still admits a wing-root gear bay and keeps the
        // fallback. Pinned so no tolerance can be introduced later.
        let fuselage = Fuselage::new(
            "Fuselage",
            vec![
                FuselageXSec {
                    xyz_c: [0.0, 0.0, 0.0],
                    width: 2.0,
                    height: 2.0,
                    shape: 2.0,
                },
                FuselageXSec {
                    xyz_c: [10.0, 0.0, 0.0],
                    width: 2.0,
                    height: 2.0,
                    shape: 2.0,
                },
            ],
        );
        assert_eq!(fuselage_crown_z_m(&fuselage, 5.0), Some(1.0));
        assert_eq!(fuselage_crown_z_m(&fuselage, -3.0), Some(1.0));
        assert_eq!(fuselage_crown_z_m(&fuselage, 99.0), Some(1.0));

        let airfoil = alas_geom::aircraft::airfoil::Airfoil::from_name("naca2412")
            .unwrap_or_else(|| panic!("valid NACA name"));
        let wing_at = |z: f64| Wing {
            name: "Main Wing".to_owned(),
            xsecs: vec![alas_geom::aircraft::wing::WingXSec::new(
                [4.0, 0.0, z],
                3.0,
                0.0,
                airfoil.clone(),
            )],
            symmetric: true,
        };
        assert_eq!(
            wing_root_above_fuselage_crown(&wing_at(1.0), &fuselage),
            None
        );
        assert_eq!(
            wing_root_above_fuselage_crown(&wing_at(1.000_001), &fuselage),
            Some((1.000_001, 1.0))
        );
    }

    #[test]
    fn propulsion_stations_is_empty_without_nacelles() {
        let config = AlasConfig::default();
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&DesignVector::default()), false)
            .expect("building without engines succeeds");
        assert!(propulsion_stations(&plane).is_empty());
    }
}
