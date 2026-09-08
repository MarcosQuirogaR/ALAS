// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> FlowUnsteadyRequest {
        FlowUnsteadyRequest {
            area_m2: 100.0,
            chord_m: 4.0,
            span_m: 30.0,
            moment_reference_m: [1.0, 0.0, 0.0],
            lifting_surfaces: vec![FlowUnsteadySurface {
                name: "Main Wing".to_owned(),
                symmetric_about_xz: true,
                sections: vec![FlowUnsteadySection {
                    leading_edge_m: [2.0, 0.0, 0.0],
                    chord_m: 5.0,
                    twist_deg: 1.0,
                    airfoil_name: "naca2412".to_owned(),
                    airfoil_coordinates: vec![(1.0, 0.0), (0.0, 0.0), (1.0, 0.0)],
                }],
            }],
            controls: vec![FlowUnsteadyControlSurface {
                role: "flap",
                surface_name: "Main Wing",
                edge: "trailing",
                chord_fraction: 0.25,
                span_start_fraction: 0.1,
                span_end_fraction: 0.6,
                deflection_deg: 0.0,
                applied_to_geometry: false,
            }],
            flight_condition: FlowUnsteadyFlightCondition {
                altitude_m: 11_000.0,
                pressure_pa: 22_632.0,
                temperature_k: 216.65,
                density_kg_m3: 0.364,
                speed_of_sound_m_s: 295.0,
                true_airspeed_m_s: 236.0,
                mach: 0.8,
                beta_deg: 0.0,
                angular_rates_rad_s: [0.0, 0.0, 0.0],
            },
            solver: FlowUnsteadySolverRequest {
                model: "unsteady_vortex_lattice",
                steps_per_reference_chord: 20,
                wake_age_reference_chords: 20.0,
                settling_reference_chords: 10.0,
                averaging_reference_chords: 5.0,
            },
            alpha_deg: vec![-2.0, 2.0],
        }
    }

    #[test]
    fn v2_request_retains_geometry_airfoil_control_and_si_solver_contract() {
        let text = render_request(&request()).unwrap_or_else(|error| panic!("render: {error}"));
        for required in [
            "ALAS_FLOWUNSTEADY_REQUEST_V2",
            "units=SI",
            "geometry_axes=x_aft_y_right_z_up",
            "surface=0,Main Wing,true,1",
            "section=0,0,2.000000000000,0.000000000000,0.000000000000,5.000000000000,1.000000000000,naca2412",
            "airfoil_point=0,0,1.000000000000,0.000000000000",
            "control=flap,Main Wing,trailing,0.250000000000,0.100000000000,0.600000000000,0.000000000000,false",
            "solver_model=unsteady_vortex_lattice",
            "true_airspeed_m_s=236.000000000000",
        ] {
            assert!(text.contains(required), "request omitted {required}");
        }
    }

    #[test]
    fn adapter_result_requires_explicit_frames_and_native_samples() {
        let parsed = parse_result("ALAS_FLOWUNSTEADY_RESULT_V1\narea_m2=100\nchord_m=4\nspan_m=30\nmoment_reference_m=1,0,0\nlifting_surfaces_only=true\nlift_is_wind_axis=true\npitch_moment_is_body_axis=true\npoint=2,0.2,0,0.4,-0.03\n").unwrap_or_else(|error| panic!("parse: {error}"));
        assert_eq!(parsed.points.len(), 1);
        assert!(parse_result("ALAS_FLOWUNSTEADY_RESULT_V1\n").is_err());
    }

    #[test]
    fn malformed_result_field_counts_are_rejected_without_indexing_panics() {
        let base = "ALAS_FLOWUNSTEADY_RESULT_V1\narea_m2=100\nchord_m=4\nspan_m=30\nmoment_reference_m=1,0,0\nlifting_surfaces_only=true\nlift_is_wind_axis=true\npitch_moment_is_body_axis=true\n";
        let extra_point = format!("{base}point=2,0.2,0,0.4,-0.03,unexpected\n");
        assert_eq!(
            parse_result(&extra_point),
            Err(FlowUnsteadyError::Missing("five point columns"))
        );
        let extra_origin = "ALAS_FLOWUNSTEADY_RESULT_V1\narea_m2=100\nchord_m=4\nspan_m=30\nmoment_reference_m=1,0,0,unexpected\nlifting_surfaces_only=true\nlift_is_wind_axis=true\npitch_moment_is_body_axis=true\npoint=2,0.2,0,0.4,-0.03\n".to_owned();
        assert_eq!(
            parse_result(&extra_origin),
            Err(FlowUnsteadyError::Missing(
                "three moment_reference_m coordinates"
            ))
        );
        let bad_flag = "ALAS_FLOWUNSTEADY_RESULT_V1\narea_m2=100\nchord_m=4\nspan_m=30\nmoment_reference_m=1,0,0\nlifting_surfaces_only=maybe\nlift_is_wind_axis=true\npitch_moment_is_body_axis=true\npoint=2,0.2,0,0.4,-0.03\n";
        assert!(matches!(
            parse_result(bad_flag),
            Err(FlowUnsteadyError::InvalidValue {
                field: "lifting_surfaces_only",
                ..
            })
        ));
    }
}

