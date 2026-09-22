// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::super::SurfaceError;
use super::error;

#[derive(Clone)]
pub(crate) struct Patch {
    pub(crate) count: usize,
    pub(crate) start: usize,
}
#[derive(Clone)]
pub(crate) struct ScalarField {
    internal: Scalar,
    patch: Option<Scalar>,
}
#[derive(Clone)]
pub(crate) struct VectorField {
    internal: Vector,
    patch: Option<Vector>,
}
#[derive(Clone)]
pub(crate) enum Field {
    Scalar(ScalarField),
    Vector(VectorField),
}

/// Units carried by an OpenFOAM pressure field.  The incompressible solver
/// writes kinematic pressure (`m^2/s^2`), while rhoSimpleFoam writes absolute
/// thermodynamic pressure (`Pa`).  Surface integration normalises both to the
/// kinematic representation used by the public sample type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PressureUnits {
    Kinematic,
    Absolute,
}
#[derive(Clone)]
pub(crate) enum Scalar {
    Uniform(f64),
    Values(Vec<f64>),
}
#[derive(Clone)]
pub(crate) enum Vector {
    Uniform([f64; 3]),
    Values(Vec<[f64; 3]>),
}
fn clean(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'/') {
            chars.next();
            for c in chars.by_ref() {
                if c == '\n' {
                    out.push('\n');
                    break;
                }
            }
        } else if c == '/' && chars.peek() == Some(&'*') {
            chars.next();
            let mut prev = ' ';
            for c in chars.by_ref() {
                if prev == '*' && c == '/' {
                    break;
                }
                prev = c;
            }
        } else {
            out.push(c);
        }
    }
    out
}
fn tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for c in clean(text).chars() {
        if c.is_whitespace() {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
        } else if "(){};[]".contains(c) {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            out.push(c.to_string());
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}
fn f64v(t: Option<&String>) -> Option<f64> {
    t.and_then(|v| v.parse().ok())
        .filter(|v: &f64| v.is_finite())
}
fn label(t: Option<&String>) -> Option<usize> {
    t.and_then(|v| v.parse().ok())
}
fn list_start(ts: &[String], source: &str) -> Result<(usize, usize), SurfaceError> {
    for i in 0..ts.len().saturating_sub(1) {
        if label(ts.get(i)).is_some() && ts[i + 1] == "(" {
            return Ok((i, i + 2));
        }
    }
    Err(error(
        source,
        "list count and opening parenthesis not found",
    ))
}
pub(crate) fn points(text: &str, source: &str) -> Result<Vec<[f64; 3]>, SurfaceError> {
    let ts = tokens(text);
    let (c, mut i) = list_start(&ts, source)?;
    let n = label(ts.get(c)).ok_or_else(|| error(source, "invalid point count"))?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        if ts.get(i).map(String::as_str) != Some("(") {
            return Err(error(source, "point vector missing opening parenthesis"));
        }
        let v = [
            f64v(ts.get(i + 1)),
            f64v(ts.get(i + 2)),
            f64v(ts.get(i + 3)),
        ];
        let [Some(x), Some(y), Some(z)] = v else {
            return Err(error(source, "invalid point vector"));
        };
        if ts.get(i + 4).map(String::as_str) != Some(")") {
            return Err(error(source, "point vector missing close"));
        }
        out.push([x, y, z]);
        i += 5;
    }
    Ok(out)
}
pub(crate) fn faces(text: &str, source: &str) -> Result<Vec<Vec<usize>>, SurfaceError> {
    let ts = tokens(text);
    let (c, mut i) = list_start(&ts, source)?;
    let n = label(ts.get(c)).ok_or_else(|| error(source, "invalid face count"))?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let k = label(ts.get(i)).ok_or_else(|| error(source, "invalid face vertex count"))?;
        if ts.get(i + 1).map(String::as_str) != Some("(") {
            return Err(error(source, "face missing opening parenthesis"));
        }
        let mut row = Vec::with_capacity(k);
        for j in 0..k {
            row.push(label(ts.get(i + 2 + j)).ok_or_else(|| error(source, "invalid face vertex"))?);
        }
        if ts.get(i + 2 + k).map(String::as_str) != Some(")") {
            return Err(error(source, "face missing close"));
        }
        out.push(row);
        i += k + 3;
    }
    Ok(out)
}
pub(crate) fn labels(text: &str, source: &str) -> Result<Vec<usize>, SurfaceError> {
    let ts = tokens(text);
    let (c, mut i) = list_start(&ts, source)?;
    let n = label(ts.get(c)).ok_or_else(|| error(source, "invalid label count"))?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(label(ts.get(i)).ok_or_else(|| error(source, "invalid owner label"))?);
        i += 1;
    }
    Ok(out)
}
pub(crate) fn boundary(text: &str, name: &str) -> Result<Patch, SurfaceError> {
    let ts = tokens(text);
    let Some(mut i) = ts.iter().position(|v| v == name) else {
        return Err(error("boundary", format!("patch {name} not found")));
    };
    if ts.get(i + 1).map(String::as_str) != Some("{") {
        return Err(error("boundary", "patch dictionary missing brace"));
    }
    i += 2;
    let mut depth = 1;
    let (mut count, mut start) = (None, None);
    while i < ts.len() && depth > 0 {
        match ts[i].as_str() {
            "{" => depth += 1,
            "}" => depth -= 1,
            "nFaces" if depth == 1 => count = label(ts.get(i + 1)),
            "startFace" if depth == 1 => start = label(ts.get(i + 1)),
            _ => {}
        }
        i += 1;
    }
    match (count, start) {
        (Some(count), Some(start)) if count > 0 => Ok(Patch { count, start }),
        _ => Err(error("boundary", "patch lacks positive nFaces/startFace")),
    }
}

fn vec_at(ts: &[String], i: &mut usize, source: &str) -> Result<[f64; 3], SurfaceError> {
    if ts.get(*i).map(String::as_str) != Some("(") {
        return Err(error(source, "vector missing opening parenthesis"));
    }
    let v = [
        f64v(ts.get(*i + 1)),
        f64v(ts.get(*i + 2)),
        f64v(ts.get(*i + 3)),
    ];
    let [Some(x), Some(y), Some(z)] = v else {
        return Err(error(source, "invalid vector"));
    };
    if ts.get(*i + 4).map(String::as_str) != Some(")") {
        return Err(error(source, "vector missing close"));
    }
    *i += 5;
    Ok([x, y, z])
}
fn scalar_at(ts: &[String], i: &mut usize, source: &str) -> Result<Scalar, SurfaceError> {
    match ts.get(*i).map(String::as_str) {
        Some("uniform") => {
            *i += 1;
            Ok(Scalar::Uniform(
                f64v(ts.get(*i)).ok_or_else(|| error(source, "invalid scalar"))?,
            ))
        }
        Some("nonuniform") => {
            *i += 1;
            *i += 1;
            let n = label(ts.get(*i)).ok_or_else(|| error(source, "invalid scalar list count"))?;
            *i += 1;
            if ts.get(*i).map(String::as_str) != Some("(") {
                return Err(error(source, "scalar list missing opening parenthesis"));
            }
            *i += 1;
            let mut v = Vec::with_capacity(n);
            for _ in 0..n {
                v.push(f64v(ts.get(*i)).ok_or_else(|| error(source, "invalid scalar list value"))?);
                *i += 1;
            }
            if ts.get(*i).map(String::as_str) != Some(")") {
                return Err(error(source, "scalar list missing close"));
            }
            *i += 1;
            Ok(Scalar::Values(v))
        }
        _ => Err(error(source, "field value must be uniform or nonuniform")),
    }
}
fn vector_at(ts: &[String], i: &mut usize, source: &str) -> Result<Vector, SurfaceError> {
    match ts.get(*i).map(String::as_str) {
        Some("uniform") => {
            *i += 1;
            Ok(Vector::Uniform(vec_at(ts, i, source)?))
        }
        Some("nonuniform") => {
            *i += 1;
            *i += 1;
            let n = label(ts.get(*i)).ok_or_else(|| error(source, "invalid vector list count"))?;
            *i += 1;
            if ts.get(*i).map(String::as_str) != Some("(") {
                return Err(error(source, "vector list missing opening parenthesis"));
            }
            *i += 1;
            let mut v = Vec::with_capacity(n);
            for _ in 0..n {
                v.push(vec_at(ts, i, source)?);
            }
            if ts.get(*i).map(String::as_str) != Some(")") {
                return Err(error(source, "vector list missing close"));
            }
            Ok(Vector::Values(v))
        }
        _ => Err(error(source, "field value must be uniform or nonuniform")),
    }
}
pub(crate) fn validate_header(
    ts: &[String],
    source: &str,
    allow_absolute_pressure: bool,
) -> Result<PressureUnits, SurfaceError> {
    let format = ts
        .iter()
        .position(|v| v == "format")
        .and_then(|i| ts.get(i + 1))
        .map(String::as_str);
    if format != Some("ascii") {
        return Err(error(source, "field format must be ASCII"));
    }
    let i = ts
        .iter()
        .position(|v| v == "dimensions")
        .ok_or_else(|| error(source, "field dimensions missing"))?;
    if ts.get(i + 1).map(String::as_str) != Some("[") {
        return Err(error(
            source,
            "field dimensions must use a seven-component bracket list",
        ));
    }
    let mut values = [0_i32; 7];
    for (j, value) in values.iter_mut().enumerate() {
        *value = ts
            .get(i + 2 + j)
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| error(source, "field dimensions contain a non-integer component"))?;
    }
    if ts.get(i + 9).map(String::as_str) != Some("]") {
        return Err(error(
            source,
            "field dimensions must be kinematic pressure/stress [0 2 -2 0 0 0 0]",
        ));
    }
    if values == [0, 2, -2, 0, 0, 0, 0] {
        Ok(PressureUnits::Kinematic)
    } else if allow_absolute_pressure && values == [1, -1, -2, 0, 0, 0, 0] {
        Ok(PressureUnits::Absolute)
    } else {
        Err(error(
            source,
            "field dimensions must be kinematic pressure/stress [0 2 -2 0 0 0 0] or absolute pressure [1 -1 -2 0 0 0 0]",
        ))
    }
}

fn field_with_pressure_units(
    text: &str,
    want_vector: bool,
    source: &str,
    allow_absolute_pressure: bool,
) -> Result<(Field, PressureUnits), SurfaceError> {
    let ts = tokens(text);
    let units = validate_header(&ts, source, allow_absolute_pressure)?;
    let internal = ts
        .iter()
        .position(|v| v == "internalField")
        .ok_or_else(|| error(source, "internalField missing"))?
        + 1;
    let mut i = internal;
    let f = if want_vector {
        Field::Vector(VectorField {
            internal: vector_at(&ts, &mut i, source)?,
            patch: None,
        })
    } else {
        Field::Scalar(ScalarField {
            internal: scalar_at(&ts, &mut i, source)?,
            patch: None,
        })
    };
    let Some(mut b) = ts.iter().position(|v| v == "boundaryField") else {
        return Ok((f, units));
    };
    b += 1;
    if ts.get(b).map(String::as_str) != Some("{") {
        return Err(error(source, "boundaryField missing brace"));
    }
    let mut depth = 1;
    b += 1;
    while b < ts.len() && depth > 0 {
        if depth == 1 && ts[b] == "airfoil" && ts.get(b + 1).map(String::as_str) == Some("{") {
            let mut j = b + 2;
            let mut d = 1;
            while j < ts.len() && d > 0 {
                if ts[j] == "{" {
                    d += 1;
                } else if ts[j] == "}" {
                    d -= 1;
                } else if d == 1 && ts[j] == "value" {
                    j += 1;
                    let mut value = j;
                    if want_vector {
                        let v = vector_at(&ts, &mut value, source)?;
                        return Ok((
                            match f {
                                Field::Vector(mut x) => {
                                    x.patch = Some(v);
                                    Field::Vector(x)
                                }
                                _ => f,
                            },
                            units,
                        ));
                    }
                    let v = scalar_at(&ts, &mut value, source)?;
                    return Ok((
                        match f {
                            Field::Scalar(mut x) => {
                                x.patch = Some(v);
                                Field::Scalar(x)
                            }
                            _ => f,
                        },
                        units,
                    ));
                }
                j += 1;
            }
        } else if ts[b] == "{" {
            depth += 1;
        } else if ts[b] == "}" {
            depth -= 1;
        }
        b += 1;
    }
    Ok((f, units))
}

// Retained as the documented single parser entry point for a field; the mesh
// reader currently reaches the typed helpers below it directly.
#[allow(dead_code)]
pub(crate) fn field(text: &str, want_vector: bool, source: &str) -> Result<Field, SurfaceError> {
    field_with_pressure_units(text, want_vector, source, false).map(|(field, _)| field)
}

pub(crate) fn pressure_field(
    text: &str,
    source: &str,
) -> Result<(Field, PressureUnits), SurfaceError> {
    field_with_pressure_units(text, false, source, true)
}

pub(crate) fn wall_shear_field(
    text: &str,
    source: &str,
) -> Result<(Field, PressureUnits), SurfaceError> {
    // Incompressible wallShearStress is kinematic (`m^2/s^2`), while the
    // compressible function object writes physical stress (`Pa`).
    field_with_pressure_units(text, true, source, true)
}

pub(crate) fn scalar_patch(
    field: &Field,
    owners: &[usize],
    start: usize,
    count: usize,
) -> Result<Vec<f64>, SurfaceError> {
    let Field::Scalar(f) = field else {
        return Err(SurfaceError::Mismatch("p field is not scalar".to_owned()));
    };
    if let Some(v) = &f.patch {
        return scalar_values(v, count, "p boundary");
    }
    match &f.internal {
        Scalar::Uniform(v) => Ok(vec![*v; count]),
        Scalar::Values(v) => {
            let mut out = Vec::with_capacity(count);
            for i in start..start + count {
                let owner = *owners.get(i).ok_or_else(|| {
                    SurfaceError::Mismatch("owner index missing for p".to_owned())
                })?;
                out.push(*v.get(owner).ok_or_else(|| {
                    SurfaceError::Mismatch("p owner exceeds internalField".to_owned())
                })?);
            }
            Ok(out)
        }
    }
}
pub(crate) fn vector_patch(
    field: &Field,
    owners: &[usize],
    start: usize,
    count: usize,
) -> Result<Vec<[f64; 3]>, SurfaceError> {
    let Field::Vector(f) = field else {
        return Err(SurfaceError::Mismatch(
            "wallShearStress field is not vector".to_owned(),
        ));
    };
    if let Some(v) = &f.patch {
        return vector_values(v, count, "wall-shear boundary");
    }
    match &f.internal {
        Vector::Uniform(_v) => Err(SurfaceError::Mismatch(
            "wallShearStress has no airfoil boundary values".to_owned(),
        )),
        Vector::Values(v) => {
            let mut out = Vec::with_capacity(count);
            for i in start..start + count {
                let owner = *owners.get(i).ok_or_else(|| {
                    SurfaceError::Mismatch("owner index missing for wall shear".to_owned())
                })?;
                out.push(*v.get(owner).ok_or_else(|| {
                    SurfaceError::Mismatch("wall shear owner exceeds internalField".to_owned())
                })?);
            }
            Ok(out)
        }
    }
}
fn scalar_values(v: &Scalar, count: usize, source: &str) -> Result<Vec<f64>, SurfaceError> {
    match v {
        Scalar::Uniform(v) => Ok(vec![*v; count]),
        Scalar::Values(v) if v.len() == count => Ok(v.clone()),
        Scalar::Values(v) => Err(SurfaceError::Mismatch(format!(
            "{source} has {} rows, expected {count}",
            v.len()
        ))),
    }
}
fn vector_values(v: &Vector, count: usize, source: &str) -> Result<Vec<[f64; 3]>, SurfaceError> {
    match v {
        Vector::Uniform(v) => Ok(vec![*v; count]),
        Vector::Values(v) if v.len() == count => Ok(v.clone()),
        Vector::Values(v) => Err(SurfaceError::Mismatch(format!(
            "{source} has {} rows, expected {count}",
            v.len()
        ))),
    }
}
