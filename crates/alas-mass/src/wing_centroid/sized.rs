// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! First moments of the actual sized product box, with conserved family masses.

use alas_geom::wing_structure::WingStructureGeometry;
use alas_struct::sizing::WingboxSizing;

use super::{WingCentroidError, WingStructuralCentroid};

#[derive(Default)]
struct Integral {
    mass: f64,
    moment: [f64; 3],
}

impl Integral {
    fn add(&mut self, mass: f64, point: [f64; 3]) {
        self.mass += mass;
        for (moment, coordinate) in self.moment.iter_mut().zip(point) {
            *moment += mass * coordinate;
        }
    }
}

/// Integrate the geometry of the sized caps, webs, covers and ribs. Each
/// family's geometric distribution is normalized to its declared sizing mass;
/// this conserves inventory and cannot alter the empirical complete-wing mass.
/// Cap and web material follow physical spar arc lengths, as in the product
/// beam's inertial relief. Covers follow local chord and ribs are distributed
/// uniformly, matching the beam's smeared rib density. SI aircraft axes.
pub fn sized_wingbox_centroid(
    geometry: &WingStructureGeometry,
    sizing: &WingboxSizing,
    root_le_x_m: f64,
) -> Result<WingStructuralCentroid, WingCentroidError> {
    let count = sizing.y_stations.len();
    if count < 2
        || !geometry.semi_span.is_finite()
        || geometry.semi_span <= 0.0
        || !root_le_x_m.is_finite()
        || sizing.y_stations.iter().any(|y| !y.is_finite())
        || sizing.y_stations.windows(2).any(|pair| pair[1] <= pair[0])
        || sizing.chord.len() != count
        || sizing
            .chord
            .iter()
            .any(|chord| !chord.is_finite() || *chord <= 0.0)
        || sizing.spars.is_empty()
    {
        return Err(WingCentroidError::DegenerateMass);
    }
    let mut caps = Integral::default();
    let mut webs = Integral::default();
    let mut skin = Integral::default();
    let mut ribs = Integral::default();
    for spar in &sizing.spars {
        if [&spar.h, &spar.a_cap, &spar.t_cap].iter().any(|values| {
            values.len() != count || values.iter().any(|v| !v.is_finite() || *v < 0.0)
        }) || !spar.t_web.is_finite()
            || spar.t_web <= 0.0
        {
            return Err(WingCentroidError::DegenerateMass);
        }
        for j in 1..count {
            let endpoints: Vec<_> = [j - 1, j]
                .into_iter()
                .map(|i| {
                    let eta = sizing.y_stations[i] / geometry.semi_span;
                    let chord = geometry.local_chord(eta);
                    let (upper, lower) = geometry.airfoil_zu_zl(eta, spar.chord_fraction);
                    let x = geometry.x_le(eta) + spar.chord_fraction * chord;
                    let z = geometry.z_le(eta);
                    (
                        [
                            x,
                            sizing.y_stations[i],
                            z + upper * chord - 0.5 * spar.t_cap[i],
                        ],
                        [
                            x,
                            sizing.y_stations[i],
                            z + lower * chord + 0.5 * spar.t_cap[i],
                        ],
                        [x, sizing.y_stations[i], z + 0.5 * (upper + lower) * chord],
                    )
                })
                .collect();
            for (a, b) in [
                (endpoints[0].0, endpoints[1].0),
                (endpoints[0].1, endpoints[1].1),
            ] {
                let ds = distance(a, b);
                caps.add(0.5 * ds * spar.a_cap[j - 1], a);
                caps.add(0.5 * ds * spar.a_cap[j], b);
            }
            let ds = distance(endpoints[0].2, endpoints[1].2);
            webs.add(0.5 * ds * spar.t_web * spar.h[j - 1], endpoints[0].2);
            webs.add(0.5 * ds * spar.t_web * spar.h[j], endpoints[1].2);
        }
    }
    let (front, rear) = alas_struct::sizing::box_chord_band(geometry);
    for j in 1..count {
        let dy = sizing.y_stations[j] - sizing.y_stations[j - 1];
        for i in [j - 1, j] {
            let eta = sizing.y_stations[i] / geometry.semi_span;
            let chord = sizing.chord[i];
            let point = [
                geometry.x_le(eta) + 0.5 * (front + rear) * chord,
                sizing.y_stations[i],
                geometry.z_le(eta),
            ];
            skin.add(0.5 * dy * chord, point);
            ribs.add(0.5 * dy, point);
        }
    }
    let inventory = sizing.mass_breakdown_kg;
    let mut total = Integral::default();
    for (family, mass) in [
        (&caps, inventory.spar_caps),
        (&webs, inventory.spar_webs),
        (&skin, inventory.skin),
        (&ribs, inventory.ribs),
    ] {
        if !mass.is_finite() || mass < 0.0 || (mass > 0.0 && family.mass <= 0.0) {
            return Err(WingCentroidError::DegenerateMass);
        }
        if mass > 0.0 {
            total.add(mass, family.moment.map(|moment| moment / family.mass));
        }
    }
    if !total.mass.is_finite() || total.mass <= 0.0 {
        return Err(WingCentroidError::DegenerateMass);
    }
    let mut xyz = total.moment.map(|moment| moment / total.mass);
    // The structural geometry's X is relative to the root leading edge;
    // its Z already uses the aircraft datum. Restore only the X translation.
    xyz[0] += root_le_x_m;
    xyz[1] = 0.0;
    if !xyz.iter().all(|value| value.is_finite()) {
        return Err(WingCentroidError::DegenerateMass);
    }
    Ok(WingStructuralCentroid {
        xyz_m: xyz,
        cap_mass_kg: inventory.spar_caps,
        web_mass_kg: inventory.spar_webs,
        skin_mass_kg: inventory.skin,
        rib_mass_kg: inventory.ribs,
    })
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (b[0] - a[0]).hypot(b[1] - a[1]).hypot(b[2] - a[2])
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::{materials, DesignRequirements, DesignVector, StructuresConfig, WingConfig};
    use alas_geom::aircraft::airfoil::Airfoil;

    fn uniform_box() -> (WingStructureGeometry, WingboxSizing) {
        let foil = Airfoil::from_name("naca0012").unwrap();
        let design = DesignVector::default();
        let config = StructuresConfig::default();
        let mut geometry = WingStructureGeometry::new(
            &design,
            &WingConfig::default(),
            &foil,
            &foil,
            &[0.25, 0.70],
            None,
        )
        .unwrap();
        geometry.c_root = 4.0;
        geometry.c_break = 4.0;
        geometry.c_tip = 4.0;
        geometry.dx_break = 0.0;
        geometry.dx_tip = 0.0;
        geometry.z_root = 0.0;
        geometry.z_break = 0.0;
        geometry.z_tip = 0.0;
        let metal = materials::get("Al 7075-T6").unwrap();
        let mut sizing = alas_struct::sizing::size_wingbox(
            &geometry,
            &config,
            &DesignRequirements::default(),
            metal,
            metal,
            metal,
            metal,
        );
        for spar in &mut sizing.spars {
            spar.a_cap.fill(0.01);
            spar.t_cap.fill(0.01);
            spar.h.fill(0.4);
            spar.t_web = 0.002;
        }
        (geometry, sizing)
    }

    #[test]
    fn uniform_box_has_mid_box_centroid_and_conserves_inventory() {
        let (geometry, sizing) = uniform_box();
        let centroid = sized_wingbox_centroid(&geometry, &sizing, 2.0).unwrap();
        assert!((centroid.xyz_m[0] - 3.9).abs() < 1.0e-12);
        assert_eq!(centroid.xyz_m[1], 0.0);
        assert!(centroid.xyz_m[2].abs() < 1.0e-12);
        assert_eq!(centroid.cap_mass_kg, sizing.mass_breakdown_kg.spar_caps);
        assert_eq!(centroid.web_mass_kg, sizing.mass_breakdown_kg.spar_webs);
        let translated = sized_wingbox_centroid(&geometry, &sizing, 12.0).unwrap();
        assert!((translated.xyz_m[0] - centroid.xyz_m[0] - 10.0).abs() < 1.0e-12);
    }

    #[test]
    fn actual_rear_cap_material_moves_the_centroid_aft() {
        let (geometry, mut sizing) = uniform_box();
        let original = sized_wingbox_centroid(&geometry, &sizing, 2.0).unwrap();
        sizing.spars[1].a_cap.fill(0.04);
        let aft = sized_wingbox_centroid(&geometry, &sizing, 2.0).unwrap();
        assert!(aft.xyz_m[0] > original.xyz_m[0]);
        assert_eq!(aft.cap_mass_kg, original.cap_mass_kg);
        sizing.spars[1].a_cap[0] = -1.0;
        assert!(sized_wingbox_centroid(&geometry, &sizing, 2.0).is_err());
    }
}
