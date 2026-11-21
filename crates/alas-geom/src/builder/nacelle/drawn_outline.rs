// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The part of a drawn nacelle a revolve cannot make: separate plan and side
//! outlines over a stretch of stations, and the scarfed inlet.
//!
//! The sections stay ellipses normal to the axis, each with its own width,
//! height and centre, so a stretch whose plan and side outlines differ (the
//! thrust-reverser lobes of a fan cowl, stowed buckets) gets both silhouettes.
//! An engine without [`NacelleDrawnOutline`] data is not touched at all.
//!
//! Frame: x metres aft of the inlet ring, z metres above its centre.

use alas_config::{EngineConfig, NacelleDrawnOutline};

use crate::aircraft::fuselage::{FuselageXSec, LipScarf, END_DISC_RUN_M};

/// The catalogue outline of this installation, at its rubber-engine scale; an
/// edited installation has none (as the G2 revolve, see `engine_v2`).
pub(super) fn engine_outline(g: &EngineConfig) -> Option<NacelleDrawnOutline> {
    let spec = alas_config::engines::get(&g.engine_name).ok()?;
    let outline = spec.nacelle_drawn_outline.as_ref()?;
    let scale = g.catalogue_outline_scale()?;
    Some(outline.scaled(
        scale.powf(alas_config::ENGINE_LENGTH_EXPONENT),
        scale.powf(alas_config::ENGINE_DIAMETER_EXPONENT),
    ))
}

/// Linear between the points of a list; `None` outside it.
fn interpolate(points: &[[f64; 2]], x: f64) -> Option<f64> {
    points.windows(2).find_map(|pair| {
        (x >= pair[0][0] && x <= pair[1][0]).then(|| {
            let t = (x - pair[0][0]) / (pair[1][0] - pair[0][0]);
            pair[0][1] + t * (pair[1][1] - pair[0][1])
        })
    })
}

/// The section of the plain outline at `x`, linear between its stations.
fn plain_section(sections: &[FuselageXSec], x: f64) -> Option<FuselageXSec> {
    let pair = sections
        .windows(2)
        .find(|pair| x >= pair[0].xyz_c[0] && x <= pair[1].xyz_c[0])?;
    let t = (x - pair[0].xyz_c[0]) / (pair[1].xyz_c[0] - pair[0].xyz_c[0]);
    let lerp = |a: f64, b: f64| a + t * (b - a);
    Some(FuselageXSec {
        xyz_c: [
            x,
            lerp(pair[0].xyz_c[1], pair[1].xyz_c[1]),
            lerp(pair[0].xyz_c[2], pair[1].xyz_c[2]),
        ],
        width: lerp(pair[0].width, pair[1].width),
        height: lerp(pair[0].height, pair[1].height),
        shape: pair[0].shape,
    })
}

/// `sections` (nacelle frame) with the drawn outlines applied. Every list
/// point and the station just aft of each list's end become stations, so a
/// drawn step stays a step and the plain outline resumes behind a list.
pub(super) fn apply_outline(
    sections: Vec<FuselageXSec>,
    outline: &NacelleDrawnOutline,
) -> Vec<FuselageXSec> {
    let Some(first) = sections.first().copied() else {
        return sections;
    };
    let lists = [&outline.half_width, &outline.top, &outline.bottom];
    if lists.iter().all(|list| list.is_empty()) {
        return sections;
    }
    let mut stations: Vec<f64> = sections.iter().map(|s| s.xyz_c[0]).collect();
    let end = stations[stations.len() - 1];
    for list in lists {
        stations.extend(list.iter().map(|p| p[0]));
        if let Some(last) = list.last() {
            stations.push(last[0] + END_DISC_RUN_M);
        }
    }
    stations.retain(|&x| x <= end);
    stations.sort_by(f64::total_cmp);
    stations.dedup_by(|a, b| (*a - *b).abs() < 1.0e-6);
    let axis_z = first.xyz_c[2];
    stations
        .into_iter()
        .filter_map(|x| {
            let mut section = plain_section(&sections, x)?;
            if let Some(half_width) = interpolate(&outline.half_width, x) {
                section.width = 2.0 * half_width;
            }
            let top = interpolate(&outline.top, x)
                .map_or(section.xyz_c[2] + 0.5 * section.height, |z| axis_z + z);
            let bottom = interpolate(&outline.bottom, x)
                .map_or(section.xyz_c[2] - 0.5 * section.height, |z| axis_z + z);
            section.xyz_c[2] = 0.5 * (top + bottom);
            section.height = top - bottom;
            Some(section)
        })
        .collect()
}

/// The scarfed inlet of `outline` on a nacelle of `sections` (nacelle frame):
/// the setback over the highlight height, faded out at the crest of the lip,
/// the first section of full width.
pub(super) fn lip_scarf(
    sections: &[FuselageXSec],
    outline: &NacelleDrawnOutline,
) -> Option<LipScarf> {
    let setback_m = outline.lip_setback_m.filter(|&s| s > 0.0)?;
    let highlight = sections.first()?;
    let widest = sections.iter().map(|s| s.width).fold(0.0, f64::max);
    let crest = sections.iter().find(|s| s.width >= 0.999 * widest)?;
    let blend_m = crest.xyz_c[0] - highlight.xyz_c[0];
    (blend_m > 0.0 && highlight.height > 0.0).then(|| LipScarf {
        highlight_x_m: highlight.xyz_c[0],
        blend_m,
        slope: setback_m / highlight.height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aircraft::fuselage::Fuselage;

    fn ring(x: f64, r: f64) -> FuselageXSec {
        FuselageXSec {
            xyz_c: [x, 0.0, 0.0],
            width: 2.0 * r,
            height: 2.0 * r,
            shape: 2.0,
        }
    }

    fn outline() -> NacelleDrawnOutline {
        NacelleDrawnOutline {
            half_width: vec![],
            top: vec![[2.0, 1.0], [3.0, 0.8]],
            bottom: vec![[2.5, -1.0], [3.0, -0.7]],
            lip_setback_m: Some(0.1),
            provenance: "test".to_owned(),
        }
    }

    #[test]
    fn plan_and_side_outlines_are_separate_and_the_plain_outline_resumes() {
        let plain = vec![
            ring(0.0, 0.9),
            ring(1.0, 1.0),
            ring(2.0, 1.0),
            ring(4.0, 0.2),
        ];
        let built = apply_outline(plain.clone(), &outline());
        let at = |x: f64| {
            *built
                .iter()
                .find(|s| (s.xyz_c[0] - x).abs() < 1.0e-9)
                .unwrap()
        };
        // Untouched ahead of the lists, bit for bit.
        assert_eq!(at(1.0), plain[1]);
        // Width stays the revolve's while the top and bottom are the drawn lines.
        let mid = at(2.5);
        assert!((mid.width - 2.0 * 0.8).abs() < 1.0e-12);
        assert!((mid.xyz_c[2] + 0.5 * mid.height - 0.9).abs() < 1.0e-12);
        assert!((mid.xyz_c[2] - 0.5 * mid.height + 1.0).abs() < 1.0e-12);
        // A step behind the lists, then the revolve again.
        let behind = at(3.0 + END_DISC_RUN_M);
        assert!(behind.xyz_c[2].abs() < 1.0e-12);
        assert!((behind.height - behind.width).abs() < 1.0e-12);
        assert!(built.windows(2).all(|p| p[0].xyz_c[0] < p[1].xyz_c[0]));
    }

    #[test]
    fn the_scarf_sets_the_lower_lip_back_and_fades_at_the_crest() {
        let plain = vec![ring(0.0, 0.8), ring(0.5, 1.0), ring(3.0, 1.0)];
        let mut nacelle = Fuselage::new("Nacelle R", plain.clone());
        nacelle.lip_scarf = lip_scarf(&plain, &outline());
        let nacelle = nacelle.translate([10.0, 2.0, -1.0]);
        let lip = nacelle.xsecs[0];
        let top = lip.xyz_c[2] + 0.5 * lip.height;
        let bottom = lip.xyz_c[2] - 0.5 * lip.height;
        assert_eq!(nacelle.ring_x_shift_m(&lip, top), 0.0);
        assert!((nacelle.ring_x_shift_m(&lip, bottom) - 0.1).abs() < 1.0e-12);
        let crest = nacelle.xsecs[1];
        assert_eq!(nacelle.ring_x_shift_m(&crest, crest.xyz_c[2]), 0.0);
    }

    #[test]
    fn an_engine_without_outline_data_is_not_touched() {
        let mut g = alas_config::GeometryConfig::default().engine;
        g.engine_name = "GE9X".to_owned();
        g.apply_engine_spec();
        assert!(engine_outline(&g).is_none());
    }
}
