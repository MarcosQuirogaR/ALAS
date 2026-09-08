// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


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
