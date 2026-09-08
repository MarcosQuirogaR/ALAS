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

use alas_config::{DesignRequirements, GeometryConfig, MassModelConfig, StructuresConfig};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::Fuselage;
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
    /// Crew, oil, catering and other operating items -- the same physical
    /// station as [`Self::furnishings`], named separately so a ledger builder
    /// can place operating items without implying they are furnishings.
    pub operating_items: ComponentStation,
    /// Lumped payload centroid, used when no per-item payload layout exists.
    pub payload_fallback: ComponentStation,
}

/// Why component stations could not be resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
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
}

/// Derive every component's placement and extent from the built geometry.
///
/// # Errors
///
/// [`StationError::MissingMainWing`] or [`StationError::MissingFuselage`] if
/// the airplane lacks either. [`StationError::NonFiniteGeometry`] if any
/// resolved station is not finite, which only degenerate input geometry
/// (zero span, coincident sections) can produce.
pub fn component_stations(
    plane: &Airplane,
    geometry: &GeometryConfig,
    requirements: &DesignRequirements,
    mass_model: &MassModelConfig,
    structures: &StructuresConfig,
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
        nose_gear: gear_station(fuselage, main_wing, geometry, mass_model, true),
        main_gear: gear_station(fuselage, main_wing, geometry, mass_model, false),
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

/// Root chord times root thickness-to-chord ratio, m -- the wing (and, by
/// the same lifting-surface reasoning, horizontal-tail) height extent.
fn root_thickness_m(wing: &Wing) -> f64 {
    match wing.xsecs.first() {
        Some(root) => root.chord * root.airfoil.max_thickness(&THICKNESS_SAMPLE_FRACTIONS),
        None => 0.0,
    }
}

/// The point at `chord_fraction` of `wing`'s overall mean aerodynamic chord
/// aft of its leading edge, at `span_fraction` of the way from root to tip
/// along the loft -- Raymer's conceptual component centre-of-gravity rule
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

/// The nose (`is_nose == true`) or main landing-gear station.
///
/// `x_nlg`/`x_mlg` reproduce the exact convention `alas-perf::landing_gear`
/// is fed under: the nose gear at a fraction of fuselage length from the
/// nose, the main gear at a fraction of the MAC aft of the MAC leading
/// edge. Vertical placement -- fuselage bottom less a strut length of 0.25
/// fuselage diameters -- is a conceptual-design assumption stated here
/// because neither gear leg's real strut geometry is modelled.
fn gear_station(
    fuselage: &Fuselage,
    main_wing: &Wing,
    geometry: &GeometryConfig,
    mass_model: &MassModelConfig,
    is_nose: bool,
) -> ComponentStation {
    let (start_x, length, z) = fuselage_datum(fuselage);
    let diameter = geometry.fuselage.diameter_m;
    let strut_length = 0.25 * diameter;
    let ground_z = z - diameter / 2.0 - strut_length;
    if is_nose {
        ComponentStation {
            position_m: [start_x + length * mass_model.nlg_x_fraction, 0.0, ground_z],
            extent_m: [0.0, 0.0, strut_length],
            method: "nlg_x_fraction of fuselage length",
        }
    } else {
        let mac = main_wing.mean_aerodynamic_chord();
        let mac_le_x = main_wing.aerodynamic_center(0.0)[0];
        ComponentStation {
            position_m: [
                mac_le_x + mass_model.mlg_x_fraction_mac * mac,
                0.0,
                ground_z,
            ],
            extent_m: [0.0, 0.0, strut_length],
            method: "mlg_x_fraction_mac aft of MAC leading edge",
        }
    }
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

/// The payload fallback station: the centre of the *occupied* cabin length,
/// reproducing `define_mass_coordinates`'s `occupied_len` exactly so a
/// stretched fuselage does not move an unchanged payload's centroid aft for
/// free.
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
        position_m: [cabin_start + 0.50 * occupied_len, 0.0, z],
        extent_m: [occupied_len, geometry.fuselage.diameter_m, 2.0],
        method: "occupied-cabin centre",
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
    fn propulsion_stations_is_empty_without_nacelles() {
        let config = AlasConfig::default();
        let plane = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&DesignVector::default()), false)
            .expect("building without engines succeeds");
        assert!(propulsion_stations(&plane).is_empty());
    }
}
