// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Puts an ALAS export's tail reference planforms back on the surfaces read from it.
//!
//! A surface whose sections are the recorded ones after a rigid translation keeps
//! its record, translated with it. Any other difference means the document was edited
//! after the export and the record describes another surface: the record is dropped
//! and the surface read as a document with no record is.

use alas_geom::aircraft::wing::{Wing, WingXSec};

use super::model::{CpacsRecordedSection, CpacsTailPlanformRecord};

/// How close, in metres and degrees, a surface must be to its record.
const RECORD_FIT_TOLERANCE: f64 = 1.0e-6;

/// The translation that takes the recorded sections onto `wing`, or the reason none
/// does.
fn record_fit(wing: &Wing, sections: &[CpacsRecordedSection]) -> Result<[f64; 3], String> {
    if sections.len() != wing.xsecs.len() {
        return Err(format!(
            "the document has {} sections and the record {}",
            wing.xsecs.len(),
            sections.len()
        ));
    }
    let (Some(first), Some(origin)) = (sections.first(), wing.xsecs.first()) else {
        return Err("the record has no sections".to_owned());
    };
    let delta = [
        origin.xyz_le[0] - first.le_m[0],
        origin.xyz_le[1] - first.le_m[1],
        origin.xyz_le[2] - first.le_m[2],
    ];
    let close = |a: f64, b: f64| (a - b).abs() <= RECORD_FIT_TOLERANCE;
    for (index, (xsec, recorded)) in wing.xsecs.iter().zip(sections).enumerate() {
        let on_translation =
            (0..3).all(|axis| close(xsec.xyz_le[axis], recorded.le_m[axis] + delta[axis]));
        if !(on_translation
            && close(xsec.chord, recorded.chord_m)
            && close(xsec.twist, recorded.twist_deg))
        {
            return Err(format!("section {index} is not the recorded one"));
        }
    }
    Ok(delta)
}

/// Restore the reference planform of every recorded surface that still fits.
pub(super) fn restore_tail_planforms(wings: &mut [Wing], records: &[CpacsTailPlanformRecord]) {
    for record in records {
        let Some(wing) = wings.iter_mut().find(|wing| wing.name == record.name) else {
            tracing::warn!(surface = %record.name, "the ALAS tail planform record names no surface of this document and was ignored");
            continue;
        };
        match record_fit(wing, &record.sections) {
            Ok(delta) => {
                let Some(airfoil) = wing.xsecs.first().map(|xsec| xsec.airfoil.clone()) else {
                    continue;
                };
                wing.reference_xsecs = Some(
                    record
                        .reference
                        .iter()
                        .map(|section| {
                            WingXSec::new(
                                [
                                    section.le_m[0] + delta[0],
                                    section.le_m[1] + delta[1],
                                    section.le_m[2] + delta[2],
                                ],
                                section.chord_m,
                                section.twist_deg,
                                airfoil.clone(),
                            )
                        })
                        .collect(),
                );
            }
            Err(reason) => {
                tracing::warn!(surface = %record.name, %reason, "the ALAS tail planform record does not fit this document and was ignored, so the sections are read as the planform");
            }
        }
    }
}
