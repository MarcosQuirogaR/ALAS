// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


// Ribs need the loft, span mapping, box layout, load, and two material
// families together; bundling them would only hide this physical dependency.
#[allow(clippy::too_many_arguments)]
fn rib_first_moment(
    wing: &Wing,
    cumulative_span: &[f64],
    semi_span: f64,
    structures: &StructuresConfig,
    layout: &SparLayout,
    supported_force_n: f64,
    skin_material: &MaterialSpec,
    rib_material: &MaterialSpec,
) -> (f64, [f64; 3]) {
    let (front, rear) = full_span_box_limits(layout);
    let root_thickness = wing.xsecs[0]
        .airfoil
        .local_thickness(&[0.5 * (front + rear)])[0]
        * wing.xsecs[0].chord;
    let root_moment = elliptic_cantilever_moment(0.0, semi_span, supported_force_n);
    let box_width = (rear - front) * wing.xsecs[0].chord;
    let membrane_force = root_moment / root_thickness.max(1e-6) / box_width.max(1e-6);
    let panel_stress = (membrane_force / structures.t_skin_min_m).max(1e6);
    let spacing = (structures.rib_radius_of_gyration_m
        * (structures.rib_buckling_coeff * std::f64::consts::PI.powi(2) * skin_material.e_pa
            / panel_stress)
            .sqrt())
    .max(0.5);
    let rib_count = structures
        .num_ribs_override
        .unwrap_or_else(|| ((semi_span / spacing).ceil() as i64 + 1).max(10))
        .max(2) as usize;
    let rib_spans = linspace(0.0, semi_span, rib_count);
    let chord_fractions = linspace(front, rear, 61);
    let thickness_by_xsec = section_thicknesses(wing, &chord_fractions);
    let mut total_mass = 0.0;
    let mut moment = [0.0; 3];
    for span in rib_spans {
        let station = sample_station(wing, cumulative_span, &thickness_by_xsec, span);
        let mut area = 0.0;
        let mut fraction_moment = 0.0;
        for index in 0..chord_fractions.len() - 1 {
            let dx_fraction = chord_fractions[index + 1] - chord_fractions[index];
            let thickness =
                0.5 * (station.thickness_to_chord[index] + station.thickness_to_chord[index + 1]);
            let area_slice = thickness * dx_fraction * station.chord_m.powi(2);
            area += area_slice;
            fraction_moment +=
                area_slice * 0.5 * (chord_fractions[index] + chord_fractions[index + 1]);
        }
        let rib_mass = area * structures.t_rib_m * rib_material.rho_kg_m3;
        let fraction_centroid = if area > 0.0 {
            fraction_moment / area
        } else {
            0.5 * (front + rear)
        };
        let xyz = [
            station.xyz_le[0] + fraction_centroid * station.chord_m,
            station.xyz_le[1],
            station.xyz_le[2],
        ];
        total_mass += rib_mass;
        for axis in 0..3 {
            moment[axis] += rib_mass * xyz[axis];
        }
    }
    (total_mass, moment)
}

fn trapezoid_property(
    stations: &[f64],
    values: &[DistributedMass],
    property: impl Fn(&DistributedMass) -> f64,
) -> f64 {
    stations
        .windows(2)
        .zip(values.windows(2))
        .map(|(span, pair)| 0.5 * (span[1] - span[0]) * (property(&pair[0]) + property(&pair[1])))
        .sum()
}

fn linspace(start: f64, stop: f64, count: usize) -> Vec<f64> {
    if count <= 1 {
        return vec![start];
    }
    (0..count)
        .map(|index| start + (stop - start) * index as f64 / (count - 1) as f64)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::wing::WingXSec;

    fn rectangular_wing() -> Wing {
        let airfoil = Airfoil::from_name("naca0012").expect("NACA 0012 is registered");
        Wing::new(
            "Main Wing",
            vec![
                WingXSec::new([2.0, 0.0, 0.0], 4.0, 0.0, airfoil.clone()),
                WingXSec::new([2.0, 10.0, 0.0], 4.0, 0.0, airfoil),
            ],
            true,
        )
    }

    #[test]
    fn spar_cap_allocation_recovers_the_applied_bending_moment_once() {
        let layout = SparLayout {
            fractions: vec![0.25, 0.70],
            full_span: vec![true, true],
        };
        let station = Station {
            xyz_le: [0.0, 0.0, 0.0],
            chord_m: 10.0,
            thickness_to_chord: vec![0.10, 0.08],
        };
        let structures = StructuresConfig::default();
        let cap_material = materials::get(&structures.spar_cap_material)
            .expect("the default cap material is registered");
        let skin_material =
            materials::get(&structures.skin_material).expect("the default skin is registered");
        let web_material = materials::get(&structures.spar_web_material)
            .expect("the default web material is registered");
        let supported_force_n = 1.0e6;
        let semi_span_m = 10.0;
        let web_thicknesses = vec![structures.t_web_min_m; 2];
        let distributed = distributed_station_mass(
            &station,
            0.0,
            semi_span_m,
            1.0,
            supported_force_n,
            &layout,
            &web_thicknesses,
            &structures,
            skin_material,
            web_material,
            cap_material,
        );

        let cap_area_each_m2 =
            distributed.caps_kg_m / (2.0 * layout.fractions.len() as f64 * cap_material.rho_kg_m3);
        let effective_height_sum_m = station.thickness_to_chord.iter().sum::<f64>()
            * station.chord_m
            * EFFECTIVE_CAP_DEPTH_FACTOR;
        let recovered_moment_nm =
            cap_area_each_m2 * cap_material.f_allow_pa * effective_height_sum_m;
        let applied_moment_nm = elliptic_cantilever_moment(0.0, semi_span_m, supported_force_n);

        assert!((recovered_moment_nm - applied_moment_nm).abs() / applied_moment_nm < 1e-12);
    }

    #[test]
    fn a_partial_span_spar_is_inactive_outboard_of_the_break() {
        let layout = SparLayout {
            fractions: vec![0.25, 0.50, 0.70],
            full_span: vec![true, false, true],
        };

        assert!(active_spar(1, 0.40, 0.40, &layout));
        assert!(!active_spar(1, 0.41, 0.40, &layout));
        assert!(active_spar(0, 1.0, 0.40, &layout));
        assert!(active_spar(2, 1.0, 0.40, &layout));
    }

    #[test]
    fn a_symmetric_wing_centroid_is_finite_and_on_the_centerline() {
        let centroid = wing_structural_centroid(
            &rectangular_wing(),
            &DesignRequirements::default(),
            &StructuresConfig::default(),
        )
        .expect("the default two-spar wingbox is valid");

        assert!(centroid.xyz_m.into_iter().all(f64::is_finite));
        assert_eq!(centroid.xyz_m[1], 0.0);
        assert!((3.0..=4.8).contains(&centroid.xyz_m[0]));
        assert!(centroid.modeled_semiwing_mass_kg() > 0.0);
    }

    #[test]
    fn an_invalid_spar_layout_is_reported_instead_of_using_a_fallback_point() {
        let structures = StructuresConfig {
            spar_chord_fractions: vec![0.25],
            ..Default::default()
        };
        let error = wing_structural_centroid(
            &rectangular_wing(),
            &DesignRequirements::default(),
            &structures,
        )
        .expect_err("one full-span spar cannot define a wingbox");

        assert_eq!(error, WingCentroidError::InvalidSparLayout);
    }
}

