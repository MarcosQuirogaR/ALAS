// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Puts an ALAS export's bodies back as the analyses read them.
//!
//! The document holds each body's closed surface. A body whose sections are the closed
//! surface of its record after a rigid translation takes the record's stations, lip
//! scarf, loft and APU cone, translated with it. Any other difference means the document
//! was edited after the export: the record is dropped and the body read as a document
//! with no record is. Loft data that does not build, or does not span the recorded hull
//! stations, is dropped alone and the body keeps the recorded stations.
//! Two bodies can share a name (the nacelles of one side), so records go to the bodies
//! of their name in order.

use std::sync::Arc;

use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec};
use alas_geom::loft::Loft;

use super::model::CpacsBodyRecord;

/// How close, in metres, a body must be to its record.
const RECORD_FIT_TOLERANCE_M: f64 = 1.0e-6;

/// The name the builder gives the APU cone.
const APU_CONE_NAME: &str = "Fuselage APU cone";

/// The translation that takes the record's closed surface onto `body`, or the reason
/// none does.
fn record_fit(body: &Fuselage, record: &CpacsBodyRecord) -> Result<[f64; 3], String> {
    let closed = Fuselage::new(record.name.clone(), record.stations.clone()).closed_stations();
    if closed.len() != body.xsecs.len() {
        return Err(format!(
            "the document has {} sections and the record closes to {}",
            body.xsecs.len(),
            closed.len()
        ));
    }
    let (Some(first), Some(origin)) = (closed.first(), body.xsecs.first()) else {
        return Err("the record has no stations".to_owned());
    };
    let delta = [
        origin.xyz_c[0] - first.xyz_c[0],
        origin.xyz_c[1] - first.xyz_c[1],
        origin.xyz_c[2] - first.xyz_c[2],
    ];
    let close = |a: f64, b: f64| (a - b).abs() <= RECORD_FIT_TOLERANCE_M;
    for (index, (section, recorded)) in body.xsecs.iter().zip(&closed).enumerate() {
        let on_translation =
            (0..3).all(|axis| close(section.xyz_c[axis], recorded.xyz_c[axis] + delta[axis]));
        if !(on_translation
            && close(section.width, recorded.width)
            && close(section.height, recorded.height))
        {
            return Err(format!("section {index} is not the recorded one"));
        }
    }
    Ok(delta)
}

fn translated(stations: &[FuselageXSec], delta: [f64; 3]) -> Vec<FuselageXSec> {
    stations.iter().map(|xsec| xsec.translate(delta)).collect()
}

/// The recorded lofted surface, or the reason there is none to use.
fn recorded_loft(record: &CpacsBodyRecord) -> Result<Option<Loft>, String> {
    let Some(recorded) = &record.loft else {
        return Ok(None);
    };
    let loft = Loft::new(&recorded.spec, recorded.params).map_err(|error| error.to_string())?;
    let cone_stations = record.apu_cone.as_ref().map_or(0, Vec::len);
    let hull = &record.stations[..record.stations.len().saturating_sub(cone_stations)];
    let (x_nose, x_tail) = loft.x_range();
    match (hull.first(), hull.last()) {
        (Some(first), Some(last))
            if (first.xyz_c[0] - x_nose).abs() <= RECORD_FIT_TOLERANCE_M
                && (last.xyz_c[0] - x_tail).abs() <= RECORD_FIT_TOLERANCE_M =>
        {
            Ok(Some(loft))
        }
        _ => Err(format!(
            "the loft spans x = {x_nose}..{x_tail} m and is not on the recorded hull stations"
        )),
    }
}

/// Restore the stations, lip scarf, loft and APU cone of every recorded body that
/// still fits.
pub(super) fn restore_bodies(bodies: &mut [Fuselage], records: &[CpacsBodyRecord]) {
    let mut unread: Vec<&CpacsBodyRecord> = records.iter().collect();
    for body in bodies {
        let Some(index) = unread.iter().position(|record| record.name == body.name) else {
            continue;
        };
        let record = unread.remove(index);
        match record_fit(body, record) {
            Ok(delta) => {
                body.xsecs = translated(&record.stations, delta);
                body.lip_scarf = record.lip_scarf.map(|mut scarf| {
                    scarf.highlight_x_m += delta[0];
                    scarf
                });
                match recorded_loft(record) {
                    Ok(loft) => body.loft = loft.map(|loft| Arc::new(loft.translated(delta))),
                    Err(reason) => {
                        tracing::warn!(body = %record.name, %reason, "the loft of the ALAS body record was ignored, so the wetted area and volume are read on the stations");
                    }
                }
                body.apu_cone = record.apu_cone.as_ref().map(|stations| {
                    Arc::new(Fuselage::new(APU_CONE_NAME, translated(stations, delta)))
                });
            }
            Err(reason) => {
                tracing::warn!(body = %record.name, %reason, "the ALAS body record does not fit this document and was ignored, so the closed sections are read as the body");
            }
        }
    }
    for record in unread {
        tracing::warn!(body = %record.name, "the ALAS body record names no body of this document and was ignored");
    }
}
