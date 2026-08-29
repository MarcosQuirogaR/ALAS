// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Strict interchange for an optional FLOWUnsteady adapter.
//!
//! FLOWUnsteady is a Julia package, not code linked into ALAS.  Its public
//! API is intentionally not guessed here: a user-supplied adapter receives a
//! retained SI request and writes this versioned result file.  The explicit
//! reference/frame declarations prevent an unlabelled time history from being
//! drawn as an ALAS aircraft polar.

use std::fmt::Write as FmtWrite;

/// One whole-aircraft, time-averaged FLOWUnsteady coefficient sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlowUnsteadyPoint {
    /// Geometric angle of attack in degrees.
    pub alpha_deg: f64,
    /// Freestream Mach number.
    pub mach: f64,
    /// Sideslip angle in degrees.
    pub beta_deg: f64,
    /// Time-averaged, wind-axis lift coefficient.
    pub lift_coefficient: f64,
    /// Time-averaged body-axis pitching-moment coefficient.
    pub pitching_moment_coefficient: f64,
}

/// Declared normalization and physical scope of the adapter result.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowUnsteadyPolar {
    /// Reference planform area in square meters.
    pub area_m2: f64,
    /// Reference chord in meters.
    pub chord_m: f64,
    /// Reference span in meters.
    pub span_m: f64,
    /// Moment origin in geometry axes, meters.
    pub moment_reference_m: [f64; 3],
    /// True only for all lifting surfaces, excluding bodies and propulsion.
    pub lifting_surfaces_only: bool,
    /// True only when lift is reported in wind axes.
    pub lift_is_wind_axis: bool,
    /// True only when Cm is about the stated origin in body axes.
    pub pitch_moment_is_body_axis: bool,
    /// Adapter samples in requested order.
    pub points: Vec<FlowUnsteadyPoint>,
}

/// One physical airfoil section attached to an exported lifting surface.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowUnsteadySection {
    /// Leading-edge geometry position in ALAS geometry axes, m.
    pub leading_edge_m: [f64; 3],
    /// Local chord, m.
    pub chord_m: f64,
    /// Local geometric twist about the leading edge, degrees.
    pub twist_deg: f64,
    /// Source airfoil identity; not a solver-specific profile alias.
    pub airfoil_name: String,
    /// Normalized `(x/c, z/c)` contour in the source airfoil ordering.
    pub airfoil_coordinates: Vec<(f64, f64)>,
}

/// One lifting surface, including its physical mirror declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowUnsteadySurface {
    /// ALAS geometry name.
    pub name: String,
    /// Mirror this surface about the XZ plane when true.
    pub symmetric_about_xz: bool,
    /// Root-to-tip loft stations.
    pub sections: Vec<FlowUnsteadySection>,
}

/// A configured control-surface region, deliberately separate from the wing
/// loft because the current ALAS lifting geometry has no deflectable mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowUnsteadyControlSurface {
    /// Stable descriptive role: slat, flap, aileron, spoiler, elevator, rudder.
    pub role: &'static str,
    /// ALAS surface name receiving the region.
    pub surface_name: &'static str,
    /// Leading or trailing edge convention.
    pub edge: &'static str,
    /// Local chord fraction occupied by the control.
    pub chord_fraction: f64,
    /// Rootward normalized span coordinate.
    pub span_start_fraction: f64,
    /// Tipward normalized span coordinate.
    pub span_end_fraction: f64,
    /// Commanded deflection in degrees. The present ALAS polar is clean, so
    /// every exported control has a zero command.
    pub deflection_deg: f64,
    /// False records that the current ALAS geometry did not apply this region
    /// to the exported lifting mesh; an adapter must not silently assume it did.
    pub applied_to_geometry: bool,
}

/// SI freestream and rigid-body state for every requested alpha sample.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowUnsteadyFlightCondition {
    /// ISA geometric altitude, m.
    pub altitude_m: f64,
    /// Static pressure, Pa.
    pub pressure_pa: f64,
    /// Static temperature, K.
    pub temperature_k: f64,
    /// Density, kg/m^3.
    pub density_kg_m3: f64,
    /// Speed of sound, m/s.
    pub speed_of_sound_m_s: f64,
    /// True airspeed, m/s.
    pub true_airspeed_m_s: f64,
    /// Freestream Mach number.
    pub mach: f64,
    /// Sideslip, degrees.
    pub beta_deg: f64,
    /// Body angular rates `[p, q, r]`, rad/s.
    pub angular_rates_rad_s: [f64; 3],
}

/// Explicit numerical request passed to a reviewed adapter.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowUnsteadySolverRequest {
    /// Algorithm class requested from the adapter, not a claim that ALAS ran it.
    pub model: &'static str,
    /// Convective time steps per reference chord.
    pub steps_per_reference_chord: u32,
    /// Wake age retained behind the aircraft, reference chords.
    pub wake_age_reference_chords: f64,
    /// Initial settling interval discarded from time averages, reference chords.
    pub settling_reference_chords: f64,
    /// Averaging interval after settling, reference chords.
    pub averaging_reference_chords: f64,
}

/// Input sufficient for a reviewed adapter to reconstruct the optimized
/// lifting geometry and its clean cruise polar in SI units.
#[derive(Debug, Clone, PartialEq)]
pub struct FlowUnsteadyRequest {
    /// Shared aircraft references, SI.
    pub area_m2: f64,
    /// Shared reference chord, SI.
    pub chord_m: f64,
    /// Shared reference span, SI.
    pub span_m: f64,
    /// Shared moment origin, SI.
    pub moment_reference_m: [f64; 3],
    /// Complete lifting geometry; bodies and propulsion are intentionally absent.
    pub lifting_surfaces: Vec<FlowUnsteadySurface>,
    /// Configured controls and their zero-command, not-applied provenance.
    pub controls: Vec<FlowUnsteadyControlSurface>,
    /// Freestream state shared by every requested alpha.
    pub flight_condition: FlowUnsteadyFlightCondition,
    /// Adapter numerical settings, expressed in convective reference-chord units.
    pub solver: FlowUnsteadySolverRequest,
    /// Requested angle schedule in degrees, at the flight condition above.
    pub alpha_deg: Vec<f64>,
}

/// A malformed adapter request or result is never accepted as solver data.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum FlowUnsteadyError {
    /// A required line or key was absent.
    #[error("FLOWUnsteady adapter output is missing {0}")]
    Missing(&'static str),
    /// A token is not a finite number.
    #[error("invalid FLOWUnsteady {field}: {token}")]
    InvalidNumber {
        /// Name of the rejected physical field.
        field: &'static str,
        /// Raw adapter token rejected as non-finite or non-numeric.
        token: String,
    },
    /// A finite-value field used an unsupported literal such as a malformed
    /// boolean flag.
    #[error("invalid FLOWUnsteady {field}: {token}")]
    InvalidValue {
        /// Name of the rejected field.
        field: &'static str,
        /// Raw literal rejected by the protocol.
        token: String,
    },
    /// The adapter file is a different protocol revision.
    #[error("unsupported FLOWUnsteady adapter protocol")]
    Protocol,
}

/// Render a stable, SI-only input file for an external adapter.
pub fn render_request(request: &FlowUnsteadyRequest) -> Result<String, FlowUnsteadyError> {
    for (field, value) in [
        ("area_m2", request.area_m2),
        ("chord_m", request.chord_m),
        ("span_m", request.span_m),
        ("mach", request.flight_condition.mach),
        ("altitude_m", request.flight_condition.altitude_m),
        ("pressure_pa", request.flight_condition.pressure_pa),
        ("temperature_k", request.flight_condition.temperature_k),
        ("density_kg_m3", request.flight_condition.density_kg_m3),
        (
            "speed_of_sound_m_s",
            request.flight_condition.speed_of_sound_m_s,
        ),
        (
            "true_airspeed_m_s",
            request.flight_condition.true_airspeed_m_s,
        ),
        (
            "wake_age_reference_chords",
            request.solver.wake_age_reference_chords,
        ),
        (
            "settling_reference_chords",
            request.solver.settling_reference_chords,
        ),
        (
            "averaging_reference_chords",
            request.solver.averaging_reference_chords,
        ),
    ] {
        if !value.is_finite() || value <= 0.0 {
            return Err(FlowUnsteadyError::InvalidNumber {
                field,
                token: value.to_string(),
            });
        }
    }
    for (field, value) in [
        ("moment_reference_x_m", request.moment_reference_m[0]),
        ("moment_reference_y_m", request.moment_reference_m[1]),
        ("moment_reference_z_m", request.moment_reference_m[2]),
        ("beta_deg", request.flight_condition.beta_deg),
        (
            "angular_rate_p_rad_s",
            request.flight_condition.angular_rates_rad_s[0],
        ),
        (
            "angular_rate_q_rad_s",
            request.flight_condition.angular_rates_rad_s[1],
        ),
        (
            "angular_rate_r_rad_s",
            request.flight_condition.angular_rates_rad_s[2],
        ),
    ] {
        if !value.is_finite() {
            return Err(FlowUnsteadyError::InvalidNumber {
                field,
                token: value.to_string(),
            });
        }
    }
    if request.lifting_surfaces.is_empty() || request.alpha_deg.is_empty() {
        return Err(FlowUnsteadyError::Missing(
            "lifting surfaces and alpha schedule",
        ));
    }
    if request.solver.steps_per_reference_chord == 0
        || request.solver.wake_age_reference_chords <= 0.0
        || request.solver.settling_reference_chords < 0.0
        || request.solver.averaging_reference_chords <= 0.0
    {
        return Err(FlowUnsteadyError::Missing(
            "positive solver time and wake settings",
        ));
    }
    let mut text = String::from("ALAS_FLOWUNSTEADY_REQUEST_V2\nunits=SI\ngeometry_axes=x_aft_y_right_z_up\nbody_axes=x_forward_y_right_z_down\n");
    writeln!(text, "area_m2={:.12}", request.area_m2).ok();
    writeln!(text, "chord_m={:.12}", request.chord_m).ok();
    writeln!(text, "span_m={:.12}", request.span_m).ok();
    writeln!(
        text,
        "moment_reference_m={:.12},{:.12},{:.12}",
        request.moment_reference_m[0], request.moment_reference_m[1], request.moment_reference_m[2]
    )
    .ok();
    writeln!(
        text,
        "altitude_m={:.12}",
        request.flight_condition.altitude_m
    )
    .ok();
    writeln!(
        text,
        "pressure_pa={:.12}",
        request.flight_condition.pressure_pa
    )
    .ok();
    writeln!(
        text,
        "temperature_k={:.12}",
        request.flight_condition.temperature_k
    )
    .ok();
    writeln!(
        text,
        "density_kg_m3={:.12}",
        request.flight_condition.density_kg_m3
    )
    .ok();
    writeln!(
        text,
        "speed_of_sound_m_s={:.12}",
        request.flight_condition.speed_of_sound_m_s
    )
    .ok();
    writeln!(
        text,
        "true_airspeed_m_s={:.12}",
        request.flight_condition.true_airspeed_m_s
    )
    .ok();
    writeln!(text, "mach={:.12}", request.flight_condition.mach).ok();
    writeln!(text, "beta_deg={:.12}", request.flight_condition.beta_deg).ok();
    writeln!(
        text,
        "angular_rates_rad_s={:.12},{:.12},{:.12}",
        request.flight_condition.angular_rates_rad_s[0],
        request.flight_condition.angular_rates_rad_s[1],
        request.flight_condition.angular_rates_rad_s[2]
    )
    .ok();
    writeln!(text, "solver_model={}", request.solver.model).ok();
    writeln!(
        text,
        "steps_per_reference_chord={}",
        request.solver.steps_per_reference_chord
    )
    .ok();
    writeln!(
        text,
        "wake_age_reference_chords={:.12}",
        request.solver.wake_age_reference_chords
    )
    .ok();
    writeln!(
        text,
        "settling_reference_chords={:.12}",
        request.solver.settling_reference_chords
    )
    .ok();
    writeln!(
        text,
        "averaging_reference_chords={:.12}",
        request.solver.averaging_reference_chords
    )
    .ok();
    for (surface_index, surface) in request.lifting_surfaces.iter().enumerate() {
        if surface.name.contains([',', '\n', '\r', '=']) || surface.sections.is_empty() {
            return Err(FlowUnsteadyError::Missing("valid named surface sections"));
        }
        writeln!(
            text,
            "surface={surface_index},{},{},{}",
            surface.name,
            surface.symmetric_about_xz,
            surface.sections.len()
        )
        .ok();
        for (section_index, section) in surface.sections.iter().enumerate() {
            for (field, value) in [
                ("section_chord_m", section.chord_m),
                ("section_twist_deg", section.twist_deg),
            ] {
                if !value.is_finite() || (field == "section_chord_m" && value <= 0.0) {
                    return Err(FlowUnsteadyError::InvalidNumber {
                        field,
                        token: value.to_string(),
                    });
                }
            }
            if section
                .leading_edge_m
                .iter()
                .any(|value| !value.is_finite())
            {
                return Err(FlowUnsteadyError::InvalidNumber {
                    field: "section_leading_edge_m",
                    token: format!(
                        "{},{},{}",
                        section.leading_edge_m[0],
                        section.leading_edge_m[1],
                        section.leading_edge_m[2]
                    ),
                });
            }
            if section.airfoil_name.contains([',', '\n', '\r', '='])
                || section.airfoil_coordinates.len() < 3
            {
                return Err(FlowUnsteadyError::Missing("named airfoil coordinates"));
            }
            writeln!(
                text,
                "section={surface_index},{section_index},{:.12},{:.12},{:.12},{:.12},{:.12},{}",
                section.leading_edge_m[0],
                section.leading_edge_m[1],
                section.leading_edge_m[2],
                section.chord_m,
                section.twist_deg,
                section.airfoil_name
            )
            .ok();
            for &(x_over_c, z_over_c) in &section.airfoil_coordinates {
                if !x_over_c.is_finite() || !z_over_c.is_finite() {
                    return Err(FlowUnsteadyError::InvalidNumber {
                        field: "airfoil_coordinate",
                        token: format!("{x_over_c},{z_over_c}"),
                    });
                }
                writeln!(
                    text,
                    "airfoil_point={surface_index},{section_index},{x_over_c:.12},{z_over_c:.12}"
                )
                .ok();
            }
        }
    }
    for control in &request.controls {
        for (field, value) in [
            ("control_chord_fraction", control.chord_fraction),
            ("control_span_start_fraction", control.span_start_fraction),
            ("control_span_end_fraction", control.span_end_fraction),
            ("control_deflection_deg", control.deflection_deg),
        ] {
            if !value.is_finite() {
                return Err(FlowUnsteadyError::InvalidNumber {
                    field,
                    token: value.to_string(),
                });
            }
        }
        if !(0.0..=1.0).contains(&control.chord_fraction)
            || !(0.0..=1.0).contains(&control.span_start_fraction)
            || !(0.0..=1.0).contains(&control.span_end_fraction)
            || control.span_start_fraction >= control.span_end_fraction
        {
            return Err(FlowUnsteadyError::Missing(
                "ordered normalized control region",
            ));
        }
        writeln!(
            text,
            "control={},{},{},{:.12},{:.12},{:.12},{:.12},{}",
            control.role,
            control.surface_name,
            control.edge,
            control.chord_fraction,
            control.span_start_fraction,
            control.span_end_fraction,
            control.deflection_deg,
            control.applied_to_geometry
        )
        .ok();
    }
    text.push_str("alpha_deg=");
    for (index, alpha) in request.alpha_deg.iter().enumerate() {
        if !alpha.is_finite() {
            return Err(FlowUnsteadyError::InvalidNumber {
                field: "alpha_deg",
                token: alpha.to_string(),
            });
        }
        if index != 0 {
            text.push(',');
        }
        write!(text, "{alpha:.12}").ok();
    }
    text.push('\n');
    Ok(text)
}

/// Parse the adapter's explicit, line-oriented result contract.
pub fn parse_result(text: &str) -> Result<FlowUnsteadyPolar, FlowUnsteadyError> {
    if text.lines().next() != Some("ALAS_FLOWUNSTEADY_RESULT_V1") {
        return Err(FlowUnsteadyError::Protocol);
    }
    let get = |name: &'static str| {
        text.lines()
            .find_map(|line| line.strip_prefix(&format!("{name}=")))
            .ok_or(FlowUnsteadyError::Missing(name))
    };
    let number = |name| get(name).and_then(|token| finite(name, token));
    let point_flags = (
        parse_bool(get("lifting_surfaces_only")?, "lifting_surfaces_only")?,
        parse_bool(get("lift_is_wind_axis")?, "lift_is_wind_axis")?,
        parse_bool(
            get("pitch_moment_is_body_axis")?,
            "pitch_moment_is_body_axis",
        )?,
    );
    let origin_tokens = get("moment_reference_m")?.split(',').collect::<Vec<_>>();
    if origin_tokens.len() != 3 {
        return Err(FlowUnsteadyError::Missing(
            "three moment_reference_m coordinates",
        ));
    }
    let origin = origin_tokens
        .iter()
        .enumerate()
        .map(|(index, value)| {
            finite(
                [
                    "moment_reference_x",
                    "moment_reference_y",
                    "moment_reference_z",
                ][index],
                value,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut points = Vec::new();
    for line in text.lines().filter(|line| line.starts_with("point=")) {
        let tokens = line[6..].split(',').collect::<Vec<_>>();
        if tokens.len() != 5 {
            return Err(FlowUnsteadyError::Missing("five point columns"));
        }
        let values = tokens
            .iter()
            .enumerate()
            .map(|(index, value)| {
                finite(["alpha_deg", "mach", "beta_deg", "CL", "Cm"][index], value)
            })
            .collect::<Result<Vec<_>, _>>()?;
        points.push(FlowUnsteadyPoint {
            alpha_deg: values[0],
            mach: values[1],
            beta_deg: values[2],
            lift_coefficient: values[3],
            pitching_moment_coefficient: values[4],
        });
    }
    if points.is_empty() {
        return Err(FlowUnsteadyError::Missing("point"));
    }
    Ok(FlowUnsteadyPolar {
        area_m2: number("area_m2")?,
        chord_m: number("chord_m")?,
        span_m: number("span_m")?,
        moment_reference_m: [origin[0], origin[1], origin[2]],
        lifting_surfaces_only: point_flags.0,
        lift_is_wind_axis: point_flags.1,
        pitch_moment_is_body_axis: point_flags.2,
        points,
    })
}

fn parse_bool(token: &str, field: &'static str) -> Result<bool, FlowUnsteadyError> {
    match token {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(FlowUnsteadyError::InvalidValue {
            field,
            token: token.to_owned(),
        }),
    }
}

fn finite(field: &'static str, token: &str) -> Result<f64, FlowUnsteadyError> {
    let value = token
        .parse::<f64>()
        .map_err(|_| FlowUnsteadyError::InvalidNumber {
            field,
            token: token.to_owned(),
        })?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(FlowUnsteadyError::InvalidNumber {
            field,
            token: token.to_owned(),
        })
    }
}

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
