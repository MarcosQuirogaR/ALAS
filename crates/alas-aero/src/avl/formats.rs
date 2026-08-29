// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native AVL output-format parsers and renderers.
//!
//! This child module keeps the format-specific machinery separate from the
//! geometry-deck and legacy total-force code in the parent module.

use std::fmt::Write as FmtWrite;

use super::{
    optional_value, parse_reference, required_value, AvlDerivativeCoefficient, AvlDerivativeRow,
    AvlDerivatives, AvlError, AvlFrame, AvlOutputKind, AvlReference, AvlSpanLoading,
    AvlStripForces, AvlStripLoading, AvlSurfaceStripForces, AvlTrefftzPlane, AvlTrimCase,
    AvlTrimConstraint, AvlTrimParameter,
};

/// Parse the far-field Trefftz coefficients from one native `FT` or `TOT` file.
pub fn parse_trefftz_plane(text: &str) -> Result<AvlTrefftzPlane, AvlError> {
    Ok(AvlTrefftzPlane {
        lift_coefficient: required_value(text, "CLff")?,
        induced_drag_coefficient: required_value(text, "CDff")?,
        side_force_coefficient: required_value(text, "CYff")?,
        span_efficiency: optional_value(text, "e")?.filter(|value| value.is_finite()),
    })
}

/// Render a full-precision AVL `OPER` output command.
///
/// AVL's `MRF` switch applies to all following output commands. The returned
/// three-line fragment is therefore safe to append after `OPER` and a solved
/// case; the caller still owns the session lifecycle and the `QUIT` command.
pub fn render_output_command(kind: AvlOutputKind, file_name: &str) -> Result<String, AvlError> {
    if file_name.trim().is_empty()
        || file_name.contains(['\0', '\r', '\n'])
        || file_name.trim() != file_name
    {
        return Err(AvlError::InvalidInput(
            "AVL output filename must be non-empty, trimmed, and single-line".to_owned(),
        ));
    }
    let command = match kind {
        AvlOutputKind::TotalForces => "FT",
        AvlOutputKind::StripForces => "FS",
        AvlOutputKind::BodyStripForces => "FSB",
        AvlOutputKind::ShearAndBending => "VM",
        AvlOutputKind::SpanLoading => "CN",
        AvlOutputKind::StabilityDerivatives => "ST",
        AvlOutputKind::StabilityForceBodyMomentDerivatives => "SM",
        AvlOutputKind::BodyDerivatives => "SB",
    };
    Ok(format!("MRF\n{command}\n{file_name}\n"))
}

/// Parse AVL's native `CNC` strip span-loading file.
#[rustfmt::skip]
pub fn parse_span_loading(text: &str, reference: AvlReference) -> Result<Vec<AvlSpanLoading>, AvlError> {
    if !text.lines().any(|line| line.trim() == "CNC") { return Err(AvlError::InvalidOutput("span-loading output is not a CNC record".to_owned())); }
    let count = text.lines().find_map(|line| line.contains("| # strips").then(|| parse_prefix(line, 1))).transpose()?.and_then(|values| values.first().copied()).map(|value| count_value(value, "# strips")).transpose()?.ok_or(AvlError::MissingValue("# strips"))?;
    validate_reference(reference)?; let mut rows = Vec::with_capacity(count);
    for line in text.lines().filter(|line| !line.contains('|')) { let Some(values) = numeric_row(line, 8, "CNC")? else { continue }; rows.push(AvlSpanLoading { index: rows.len() + 1, midpoint_m: [values[0], values[1], values[2]], normal_loading: values[3], lift_coefficient: values[4], chord_m: values[5], width_m: values[6], area_m2: values[7] }); }
    if rows.len() != count { return Err(AvlError::InvalidOutput(format!("CNC declares {count} strips but contains {} rows", rows.len()))); } Ok(rows)
}

/// Parse native `FS` or `FSB` surface and strip forces.
#[rustfmt::skip]
pub fn parse_strip_forces(text: &str, expected_frame: AvlFrame) -> Result<AvlStripForces, AvlError> {
    let lines = text.lines().collect::<Vec<_>>(); if lines.first().is_none_or(|line| line.trim() != "STRP") { return Err(AvlError::InvalidOutput("surface-force output is not STRP".to_owned())); }
    let frame = strip_frame(&lines)?; if frame != expected_frame { return Err(AvlError::IncompatibleFrame { output: "STRP", actual: frame, expected: expected_frame }); }
    let reference = parse_reference(text)?; validate_reference(reference)?; let count = marker_count(&lines, "| surfaces", "surfaces")?; let mut cursor = lines.iter().position(|line| line.trim() == "SURFACE").ok_or(AvlError::MissingValue("SURFACE"))?; let mut surfaces = Vec::with_capacity(count);
    for _ in 0..count {
        if lines.get(cursor).is_none_or(|line| line.trim() != "SURFACE") { return Err(AvlError::InvalidOutput("STRP surface blocks are incomplete".to_owned())); }
        let name = line_at(&lines, cursor + 1, "surface name")?.trim().to_owned(); let meta = parse_prefix(line_at(&lines, cursor + 2, "surface metadata")?, 4)?; let area = parse_prefix(line_at(&lines, cursor + 3, "surface area")?, 2)?;
        let reference_coefficients: [f64; 8] = parse_prefix(line_at(&lines, cursor + 4, "surface coefficients")?, 8)?.try_into().map_err(|_| AvlError::InvalidOutput("STRP coefficient count changed".to_owned()))?;
        let local_coefficients: [f64; 2] = parse_prefix(line_at(&lines, cursor + 5, "local coefficients")?, 2)?.try_into().map_err(|_| AvlError::InvalidOutput("STRP local coefficient count changed".to_owned()))?;
        let spanwise_strips = count_value(meta[2], "spanwise strips")?; cursor += 8; let mut strips = Vec::with_capacity(spanwise_strips);
        while cursor < lines.len() && strips.len() < spanwise_strips { if let Some(values) = numeric_row(lines[cursor], 15, "STRP")? { strips.push(strip_loading(&values)?); } cursor += 1; }
        if strips.len() != spanwise_strips { return Err(AvlError::InvalidOutput(format!("STRP surface declares {spanwise_strips} strips but contains {}", strips.len()))); }
        surfaces.push(AvlSurfaceStripForces { index: count_value(meta[0], "surface index")?, name, chordwise_panels: count_value(meta[1], "chordwise panels")?, spanwise_strips, first_strip: count_value(meta[3], "first strip")?, area_m2: area[0], average_chord_m: area[1], reference_coefficients, local_coefficients, strips });
        while cursor < lines.len() && lines[cursor].trim() != "SURFACE" { cursor += 1; }
    }
    Ok(AvlStripForces { reference, frame, surfaces })
}

/// Parse native `ST`, `SM`, or `SB` derivative matrices.
pub fn parse_derivatives(text: &str, expected_frame: AvlFrame) -> Result<AvlDerivatives, AvlError> {
    let lines = text.lines().collect::<Vec<_>>();
    let (actual_frame, first_columns, rate_columns, first_marker) = derivative_header(&lines)?;
    if actual_frame != expected_frame {
        return Err(AvlError::IncompatibleFrame {
            output: "DERMAT*",
            actual: actual_frame,
            expected: expected_frame,
        });
    }
    let reference = parse_reference(text)?;
    validate_reference(reference)?;
    let rate_marker = lines
        .iter()
        .enumerate()
        .skip(first_marker + 1)
        .find(|(_, line)| line.to_ascii_lowercase().contains("roll rate"))
        .map(|(index, _)| index)
        .ok_or(AvlError::MissingValue("roll rate"))?;
    let first_order = derivative_rows(&lines, first_marker + 1, rate_marker)?;
    let control_marker = lines
        .iter()
        .enumerate()
        .skip(rate_marker + 1)
        .find(|(_, line)| line.contains("| # control vars"))
        .map(|(index, _)| index)
        .ok_or(AvlError::MissingValue("# control vars"))?;
    let rates = derivative_rows(&lines, rate_marker + 1, control_marker)?;
    let design_marker = lines
        .iter()
        .enumerate()
        .skip(control_marker + 1)
        .find(|(_, line)| line.contains("| # design vars"))
        .ok_or(AvlError::MissingValue("# design vars"))
        .map(|(index, _)| index)?;
    let control_count = marker_count(&lines, "| # control vars", "control vars")?;
    let mut cursor = control_marker + 1;
    let control_names = read_names(&lines, &mut cursor, control_count)?;
    let controls = derivative_rows(&lines, cursor, design_marker)?;
    let design_count = count_value(parse_prefix(lines[design_marker], 1)?[0], "design vars")?;
    cursor = design_marker + 1;
    let design_names = read_names(&lines, &mut cursor, design_count)?;
    let neutral_marker = lines
        .iter()
        .enumerate()
        .skip(cursor)
        .find(|(_, line)| line.contains("| Neutral point"))
        .map(|(index, _)| index)
        .ok_or(AvlError::MissingValue("Neutral point Xnp"))?;
    let design = derivative_rows(&lines, cursor, neutral_marker)?;
    let neutral_point_m = marker_value(lines[neutral_marker], "Neutral point Xnp")?;
    let spiral_marker = lines
        .iter()
        .enumerate()
        .skip(neutral_marker + 1)
        .find(|(_, line)| line.contains("| Clb Cnr / Clr Cnb"))
        .ok_or(AvlError::MissingValue("Clb Cnr / Clr Cnb"))
        .map(|(index, _)| index)?;
    let spiral = marker_value(lines[spiral_marker], "Clb Cnr / Clr Cnb")?;
    if !(5..=6).contains(&first_order.len())
        || !(5..=6).contains(&rates.len())
        || (!control_names.is_empty() && controls.is_empty())
        || (!design_names.is_empty() && design.is_empty())
    {
        return Err(AvlError::InvalidOutput(
            "DERMAT* matrix row count is inconsistent".to_owned(),
        ));
    }
    Ok(AvlDerivatives {
        reference,
        frame: actual_frame,
        first_order_columns: first_columns,
        first_order,
        rate_columns,
        rates,
        control_names,
        controls,
        design_names,
        design,
        neutral_point_m,
        spiral_stability_parameter: spiral,
    })
}

/// Parse one or more native AVL `.run` trim cases.
#[rustfmt::skip]
pub fn parse_trim_cases(text: &str) -> Result<Vec<AvlTrimCase>, AvlError> {
    let mut out = Vec::new(); let mut current = None;
    for line in text.lines().map(str::trim) {
        if line.starts_with("Run case") {
            if let Some(case) = current.take() { out.push(case); }
            let (head, title) = line.split_once(':').ok_or_else(|| AvlError::InvalidOutput("trim case heading lacks ':'".to_owned()))?;
            let index = head.split_whitespace().nth(2).ok_or_else(|| AvlError::InvalidOutput("trim case heading lacks an index".to_owned()))?.parse().map_err(|_| AvlError::InvalidOutput("trim case index is not an integer".to_owned()))?;
            current = Some(AvlTrimCase { index, title: title.trim().to_owned(), constraints: Vec::new(), parameters: Vec::new() }); continue;
        }
        let Some(case) = current.as_mut() else { continue };
        if let Some((variable, right)) = line.split_once("->") {
            let (constraint, value) = right.split_once('=').ok_or_else(|| AvlError::InvalidOutput("trim constraint lacks '='".to_owned()))?;
            case.constraints.push(AvlTrimConstraint { variable: variable.trim().to_owned(), constraint: constraint.trim().to_owned(), value: first_value(value, "trim constraint")? });
        } else if let Some((name, value)) = line.split_once('=') {
            let mut tokens = value.split_whitespace(); let number = tokens.next().ok_or_else(|| AvlError::InvalidOutput("trim parameter lacks a value".to_owned()))?;
            case.parameters.push(AvlTrimParameter { name: name.trim().to_owned(), value: parse_float(number, "trim parameter")?, units: tokens.next().map(str::to_owned) });
        }
    }
    if let Some(case) = current { out.push(case); }
    if out.is_empty() { Err(AvlError::InvalidOutput("trim file contains no run cases".to_owned())) } else { Ok(out) }
}

/// Render one native AVL `.run` trim case.
#[rustfmt::skip]
pub fn render_trim_case(case: &AvlTrimCase) -> Result<String, AvlError> {
    if case.index == 0 || case.title.contains(['\r', '\n']) { return Err(AvlError::InvalidInput("trim case index/title is invalid".to_owned())); }
    let mut output = String::new(); writeln!(output, " ---------------------------------------------").ok(); writeln!(output, " Run case {:2}:  {}", case.index, case.title).ok(); writeln!(output).ok();
    for item in &case.constraints { validate_trim_label(&item.variable)?; validate_trim_label(&item.constraint)?; if !item.value.is_finite() { return Err(AvlError::InvalidInput("trim constraint value is not finite".to_owned())); } writeln!(output, " {:<12} -> {:<12} = {:.8E}", item.variable, item.constraint, item.value).ok(); }
    writeln!(output).ok();
    for item in &case.parameters { validate_trim_label(&item.name)?; if !item.value.is_finite() { return Err(AvlError::InvalidInput("trim parameter value is not finite".to_owned())); } match &item.units { Some(units) if units.contains(['\r', '\n']) => return Err(AvlError::InvalidInput("trim parameter units contain a newline".to_owned())), Some(units) => writeln!(output, " {:<10} = {:.8E}     {}", item.name, item.value, units).ok(), None => writeln!(output, " {:<10} = {:.8E}", item.name, item.value).ok() }; }
    Ok(output)
}

#[rustfmt::skip]
fn parse_float(token: &str, label: &'static str) -> Result<f64, AvlError> {
    let value = token.replace(['D', 'd'], "E").parse::<f64>().map_err(|_| AvlError::InvalidNumber { label, token: token.to_owned() })?;
    value.is_finite().then_some(value).ok_or_else(|| AvlError::InvalidNumber { label, token: token.to_owned() })
}

#[rustfmt::skip]
fn parse_prefix(line: &str, count: usize) -> Result<Vec<f64>, AvlError> {
    let tokens = line.split('|').next().unwrap_or(line).split_whitespace().collect::<Vec<_>>();
    if tokens.len() < count { return Err(AvlError::InvalidOutput(format!("expected {count} numeric values before '|', got {}", tokens.len()))); }
    tokens[..count].iter().map(|token| parse_float(token, "AVL output")).collect()
}

#[rustfmt::skip]
fn numeric_row(line: &str, count: usize, label: &'static str) -> Result<Option<Vec<f64>>, AvlError> {
    let tokens = line.split('|').next().unwrap_or(line).split_whitespace().collect::<Vec<_>>();
    (tokens.len() == count).then(|| tokens.into_iter().map(|token| parse_float(token, label)).collect::<Result<Vec<_>, _>>().map(Some)).transpose()?.flatten().map_or(Ok(None), |row| Ok(Some(row)))
}

#[rustfmt::skip]
fn count_value(value: f64, label: &'static str) -> Result<usize, AvlError> {
    if !value.is_finite() || value < 0.0 || (value - value.round()).abs() > 1.0e-9 { Err(AvlError::InvalidOutput(format!("{label} is not a non-negative integer"))) } else { Ok(value as usize) }
}

#[rustfmt::skip]
fn marker_count(lines: &[&str], marker: &str, label: &'static str) -> Result<usize, AvlError> {
    count_value(parse_prefix(lines.iter().find(|line| line.contains(marker)).ok_or(AvlError::MissingValue(label))?, 1)?[0], label)
}

#[rustfmt::skip]
fn line_at<'a>(lines: &'a [&str], index: usize, label: &'static str) -> Result<&'a str, AvlError> { lines.get(index).copied().ok_or(AvlError::MissingValue(label)) }

#[rustfmt::skip]
fn validate_reference(reference: AvlReference) -> Result<(), AvlError> {
    if [reference.area_m2, reference.chord_m, reference.span_m].iter().any(|value| !value.is_finite() || *value <= 0.0) || reference.moment_reference_m.iter().any(|value| !value.is_finite()) { Err(AvlError::InvalidInput("AVL output reference must be finite with positive Sref, Cref, and Bref".to_owned())) } else { Ok(()) }
}

#[rustfmt::skip]
fn strip_frame(lines: &[&str]) -> Result<AvlFrame, AvlError> {
    let orientation = lines.iter().find(|line| line.to_ascii_lowercase().contains("axis orientation")).ok_or_else(|| AvlError::InvalidOutput("STRP output has no axis orientation".to_owned()))?.to_ascii_lowercase();
    if orientation.contains("standard axis") { Ok(AvlFrame::StabilityAxes) } else if orientation.contains("body axis") { Ok(AvlFrame::BodyAxes) } else { Err(AvlError::InvalidOutput("STRP output uses an unsupported axis orientation".to_owned())) }
}

#[rustfmt::skip]
fn strip_loading(values: &[f64]) -> Result<AvlStripLoading, AvlError> {
    if values.len() != 15 { return Err(AvlError::InvalidOutput("STRP strip row has the wrong width".to_owned())); }
    Ok(AvlStripLoading { index: count_value(values[0], "strip index")?, leading_edge_m: [values[1], values[2], values[3]], chord_m: values[4], area_m2: values[5], chord_loading: values[6], induced_angle_rad: values[7], perpendicular_lift_coefficient: values[8], lift_coefficient: values[9], drag_coefficient: values[10], viscous_drag_coefficient: values[11], quarter_chord_moment_coefficient: values[12], leading_edge_moment_coefficient: values[13], center_of_pressure_x_over_c: values[14] })
}

#[rustfmt::skip]
fn derivative_header(lines: &[&str]) -> Result<(AvlFrame, Vec<String>, Vec<String>, usize), AvlError> {
    let id = lines.iter().map(|line| line.trim()).find(|line| line.starts_with("DERMAT")).ok_or_else(|| AvlError::InvalidOutput("derivative output has no DERMAT* id".to_owned()))?;
    let (frame, first, rates) = match id {
        "DERMATS" => (AvlFrame::StabilityAxes, vec!["alpha".into(), "beta".into()], vec!["p'".into(), "q'".into(), "r'".into()]),
        "DERMATM" => (AvlFrame::StabilityForcesBodyMoments, vec!["alpha".into(), "beta".into()], vec!["p".into(), "q".into(), "r".into()]),
        "DERMATB" => (AvlFrame::GeometryAxes, vec!["u".into(), "v".into(), "w".into()], vec!["p".into(), "q".into(), "r".into()]),
        _ => return Err(AvlError::InvalidOutput("unsupported DERMAT* id".to_owned())),
    };
    let marker = lines.iter().position(|line| { let lower = line.to_ascii_lowercase(); (frame == AvlFrame::GeometryAxes && lower.contains("axial") && lower.contains("normal")) || (frame != AvlFrame::GeometryAxes && lower.trim() == "alpha, beta") }).ok_or(AvlError::MissingValue("derivative variable header"))?;
    Ok((frame, first, rates, marker))
}

#[rustfmt::skip]
fn derivative_rows(lines: &[&str], start: usize, end: usize) -> Result<Vec<AvlDerivativeRow>, AvlError> {
    lines[start..end].iter().filter_map(|line| line.split_once('|')).map(|(numbers, labels)| {
        let values = numbers.split_whitespace().map(|token| parse_float(token, "derivative")).collect::<Result<Vec<_>, _>>()?;
        if values.is_empty() { return Err(AvlError::InvalidOutput("derivative row has no values".to_owned())); }
        Ok(AvlDerivativeRow { coefficient: derivative_coefficient(labels)?, values })
    }).collect()
}

#[rustfmt::skip]
fn derivative_coefficient(labels: &str) -> Result<AvlDerivativeCoefficient, AvlError> {
    let l = labels.to_ascii_lowercase();
    let kind = if l.contains("span eff") { AvlDerivativeCoefficient::SpanEfficiency } else if l.contains("trefftz drag") { AvlDerivativeCoefficient::TrefftzDrag } else if l.contains("force cl") { AvlDerivativeCoefficient::Lift } else if l.contains("force cy") { AvlDerivativeCoefficient::SideForce } else if l.contains("force cd") || l.contains("force cx") { AvlDerivativeCoefficient::Drag } else if l.contains("force cz") { AvlDerivativeCoefficient::NormalForce } else if l.contains("x") && (l.contains("mom") || l.contains("roll")) { AvlDerivativeCoefficient::RollingMoment } else if l.contains("y") && (l.contains("mom") || l.contains("pitch")) { AvlDerivativeCoefficient::PitchingMoment } else if l.contains("z") && (l.contains("mom") || l.contains("yaw")) { AvlDerivativeCoefficient::YawingMoment } else { return Err(AvlError::InvalidOutput(format!("unknown derivative row label: {}", labels.trim()))); };
    Ok(kind)
}

#[rustfmt::skip]
fn read_names(lines: &[&str], cursor: &mut usize, count: usize) -> Result<Vec<String>, AvlError> {
    let mut names = Vec::with_capacity(count);
    while names.len() < count {
        let name = line_at(lines, *cursor, "derivative variable name")?.trim();
        *cursor += 1;
        if name.is_empty() {
            continue;
        }
        if name.contains('|') {
            return Err(AvlError::InvalidOutput(
                "derivative variable name is not a plain line".to_owned(),
            ));
        }
        names.push(name.to_owned());
    }
    Ok(names)
}

#[rustfmt::skip]
fn marker_value(line: &str, _label: &'static str) -> Result<Option<f64>, AvlError> { let value = parse_prefix(line, 1)?[0]; Ok((value > -1.0e29).then_some(value)) }

#[rustfmt::skip]
fn first_value(text: &str, label: &'static str) -> Result<f64, AvlError> { parse_float(text.split_whitespace().next().ok_or_else(|| AvlError::InvalidOutput(format!("{label} has no value")) )?, label) }

#[rustfmt::skip]
fn validate_trim_label(label: &str) -> Result<(), AvlError> { if label.trim().is_empty() || label.contains(['\r', '\n']) { Err(AvlError::InvalidInput("AVL trim labels must be non-empty and single-line".to_owned())) } else { Ok(()) } }
