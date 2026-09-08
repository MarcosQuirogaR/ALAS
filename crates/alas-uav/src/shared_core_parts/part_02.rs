// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


fn tail_wings(topology: UavTopology, geometry: GeneratedGeometry, airfoil: Airfoil) -> Vec<Wing> {
    if !topology.uses_generated_empennage() {
        return Vec::new();
    }
    let tail = geometry.empennage;
    let tail_x_m =
        geometry.wing.leading_edge_x_m + 0.25 * geometry.wing.mean_chord_m + tail.tail_arm_m;
    let horizontal_chord_m = tail.horizontal_area_m2 / tail.horizontal_span_m;
    let vertical_chord_m = tail.vertical_area_m2 / tail.vertical_span_m;
    match topology {
        UavTopology::ConventionalTail | UavTopology::TTail => {
            let horizontal_z_m = if topology == UavTopology::TTail {
                tail.vertical_span_m
            } else {
                0.0
            };
            vec![
                Wing::new(
                    "UAV Horizontal Tail",
                    vec![
                        WingXSec::new(
                            [tail_x_m, 0.0, horizontal_z_m],
                            horizontal_chord_m,
                            0.0,
                            airfoil.clone(),
                        ),
                        WingXSec::new(
                            [tail_x_m, tail.horizontal_span_m / 2.0, horizontal_z_m],
                            horizontal_chord_m,
                            0.0,
                            airfoil.clone(),
                        ),
                    ],
                    true,
                ),
                Wing::new(
                    "UAV Vertical Tail",
                    vec![
                        WingXSec::new([tail_x_m, 0.0, 0.0], vertical_chord_m, 0.0, airfoil.clone()),
                        WingXSec::new(
                            [tail_x_m, 0.0, tail.vertical_span_m],
                            vertical_chord_m,
                            0.0,
                            airfoil,
                        ),
                    ],
                    false,
                ),
            ]
        }
        UavTopology::VTail => {
            let horizontal_projection_m = tail.horizontal_span_m / 2.0;
            let inclination_rad = tail
                .vertical_area_m2
                .sqrt()
                .atan2(tail.horizontal_area_m2.sqrt());
            let cosine = inclination_rad.cos();
            let span_m = horizontal_projection_m / cosine;
            let surface_area_m2 = 0.5 * (tail.horizontal_area_m2 + tail.vertical_area_m2);
            let chord_m = surface_area_m2 / span_m;
            let tip_z_m = span_m * inclination_rad.sin();
            vec![
                inclined_tail(
                    "UAV V-Tail Port",
                    tail_x_m,
                    -horizontal_projection_m,
                    tip_z_m,
                    chord_m,
                    airfoil.clone(),
                ),
                inclined_tail(
                    "UAV V-Tail Starboard",
                    tail_x_m,
                    horizontal_projection_m,
                    tip_z_m,
                    chord_m,
                    airfoil,
                ),
            ]
        }
        UavTopology::FlyingWing => Vec::new(),
    }
}

fn inclined_tail(
    name: &str,
    x_m: f64,
    tip_y_m: f64,
    tip_z_m: f64,
    chord_m: f64,
    airfoil: Airfoil,
) -> Wing {
    Wing::new(
        name,
        vec![
            WingXSec::new([x_m, 0.0, 0.0], chord_m, 0.0, airfoil.clone()),
            WingXSec::new([x_m, tip_y_m, tip_z_m], chord_m, 0.0, airfoil),
        ],
        false,
    )
}

fn airplane_name(topology: UavTopology) -> String {
    match topology {
        UavTopology::ConventionalTail => "Generated Fixed-Wing UAV".to_owned(),
        _ => format!("UAV {}", topology.label()),
    }
}

fn fuselage(geometry: GeneratedGeometry) -> Result<Fuselage, SharedCoreFailure> {
    let length_m = geometry.fuselage.length_m;
    let radius_m = 0.5 * geometry.fuselage.diameter_m;
    let sections = [
        (0.0, 0.0),
        (0.15 * length_m, radius_m),
        (0.85 * length_m, radius_m),
        (length_m, 0.0),
    ]
    .into_iter()
    .map(|(x_m, radius_m)| {
        FuselageXSec::new([x_m, 0.0, 0.0], Some(radius_m), None, None, DEFAULT_SHAPE)
    })
    .collect::<Result<Vec<_>, _>>()?;
    Ok(Fuselage::new("UAV Fuselage", sections))
}

fn validate_inputs(inputs: &SharedCoreInputs) -> Result<(), SharedCoreFailure> {
    if !inputs.altitude_m.is_finite()
        || !inputs.speed_m_s.is_finite()
        || inputs.speed_m_s <= 0.0
        || !inputs.angle_of_attack_deg.is_finite()
        || inputs.spanwise_resolution == 0
        || inputs.chordwise_resolution == 0
    {
        return Err(SharedCoreFailure::InvalidInput(
            "shared-core altitude, speed, angle, and mesh must be finite and positive where applicable"
                .to_owned(),
        ));
    }
    Ok(())
}

fn validate_geometry(
    topology: UavTopology,
    geometry: GeneratedGeometry,
    center_of_gravity_x_m: f64,
) -> Result<(), SharedCoreFailure> {
    let values = [
        geometry.wing.area_m2,
        geometry.wing.span_m,
        geometry.wing.mean_chord_m,
        geometry.fuselage.length_m,
        geometry.fuselage.diameter_m,
    ];
    let invalid_base = values
        .into_iter()
        .any(|value| !value.is_finite() || value <= 0.0);
    let invalid_empennage = topology.uses_generated_empennage()
        && [
            geometry.empennage.horizontal_area_m2,
            geometry.empennage.horizontal_span_m,
            geometry.empennage.vertical_area_m2,
            geometry.empennage.vertical_span_m,
            geometry.empennage.tail_arm_m,
        ]
        .into_iter()
        .any(|value| !value.is_finite() || value <= 0.0);
    if invalid_base
        || invalid_empennage
        || !geometry.wing.leading_edge_x_m.is_finite()
        || !center_of_gravity_x_m.is_finite()
    {
        return Err(SharedCoreFailure::InvalidInput(
            "generated geometry and loaded CG must be finite and positive where applicable"
                .to_owned(),
        ));
    }
    Ok(())
}

