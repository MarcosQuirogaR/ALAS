// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The fuselage built from an embedded loft (`FuselageConfig.loft = Some`).
//!
//! Precedence (design 5.2): the loft owns the outer surface. The legacy shape
//! fields (`nose_*`, `aft_*`, `tail_end_*`, `hump_*`, `belly_*`, custom and
//! generated sections) are not read; the payload-semantic fields
//! (`cabin_start_x_m`, `tailcone_length_m`, `diameter_m`, `height_m`,
//! `upper_deck_*`) stay authoritative for every consumer that reads them, and
//! the loft's widest and tallest sections are checked against `diameter_m` and
//! `height_m` (plus the upper-deck rise). The built length is
//! `dv.fuselage_length_m`: the loft stretches or trims its constant cabin
//! cylinder, as the legacy builder does (design 5.3). Frame: x metres aft of
//! the nose tip, z up, the loft's own frame (origin zero).
//!
//! The station view `Fuselage.xsecs` is a polyline within [`POLYLINE_TOLERANCE_M`]
//! of the exact crown, keel and plan half-width, so every consumer that has
//! not migrated to the loft keeps a dense, accurate body. An unknown loft id or
//! an invalid file is an error, never a fall-back to the parametric laws.

use std::sync::Arc;

use alas_config::{DesignVector, FuselageConfig, LoftRef};

use super::report::{BuildReport, LoftBuildInfo};
use super::{AircraftBuilder, BuildError};
use crate::aircraft::fuselage::Fuselage;
use crate::loft::{Loft, LoftParams};

/// Largest allowed distance of the station polyline from the exact surface, m.
pub const POLYLINE_TOLERANCE_M: f64 = 0.005;
/// Fewest and most stations of the polyline view.
const STATIONS: (usize, usize) = (41, 400);
/// Relative mismatch of the loft's extents with `diameter_m` / `height_m`
/// accepted silently, and the one above which the build is refused.
const EXTENT_OK: f64 = 0.005;
const EXTENT_ERR: f64 = 0.02;
/// Built over reference length beyond which a remap warning is recorded.
const REMAP_WARN: f64 = 0.10;
/// Axial samples used to find the widest and tallest sections.
const EXTENT_SAMPLES: usize = 2000;

impl AircraftBuilder {
    /// The fuselage of design `dv`: from the loft when the (product)
    /// configuration references one, else the parametric laws.
    pub(super) fn build_fuselage_reported(
        &self,
        dv: &DesignVector,
        report: &mut BuildReport,
    ) -> Result<Fuselage, BuildError> {
        let g = &self.geometry.fuselage;
        match (&g.loft, self.geometry_contract) {
            (Some(reference), super::GeometryContract::Product) => {
                self.build_lofted_fuselage(dv, reference, report)
            }
            _ => self.build_fuselage(dv),
        }
    }

    fn build_lofted_fuselage(
        &self,
        dv: &DesignVector,
        reference: &LoftRef,
        report: &mut BuildReport,
    ) -> Result<Fuselage, BuildError> {
        let g = &self.geometry.fuselage;
        g.validate_generated_sections()?;
        let spec = reference.resolve()?;
        let length_m = dv.fuselage_length_m;
        let loft = Loft::new(&spec, LoftParams::new(length_m))?;

        let length_ratio = length_m / spec.reference_length_m;
        if (length_ratio - 1.0).abs() > REMAP_WARN {
            report.warn(
                "loft_length_remap",
                format!(
                    "loft {}: built length {length_m:.2} m is {:.0} % of its reference length \
                     {:.2} m; only the constant cabin cylinder absorbs the change",
                    reference.id,
                    100.0 * length_ratio,
                    spec.reference_length_m
                ),
            );
        }
        warn_ignored_fields(g, &reference.id, report);
        let (max_width_m, max_height_m) = extents(&loft);
        check_extent("width", reference, max_width_m, g.diameter_m, report)?;
        check_extent(
            "height",
            reference,
            max_height_m,
            expected_height_m(g),
            report,
        )?;

        let sampled = loft.sample_xsecs_within(POLYLINE_TOLERANCE_M, STATIONS.0, STATIONS.1);
        if !sampled.within_tolerance {
            report.warn(
                "loft_polyline_tolerance",
                format!(
                    "loft {}: {} stations leave the polyline view {:.1} mm from the surface \
                     (target {:.1} mm)",
                    reference.id,
                    sampled.xsecs.len(),
                    1e3 * sampled.max_deviation_m,
                    1e3 * POLYLINE_TOLERANCE_M
                ),
            );
        }
        report.loft = Some(LoftBuildInfo {
            loft_id: reference.id.clone(),
            length_ratio,
            stations: sampled.xsecs.len(),
            polyline_deviation_m: sampled.max_deviation_m,
            max_width_m,
            max_height_m,
        });
        let mut fuselage = Fuselage::new("Fuselage", sampled.xsecs);
        fuselage.loft = Some(Arc::new(loft));
        Ok(fuselage)
    }
}

/// Height the loft must reach: the declared section height plus the full
/// upper-deck rise, if the configuration has one.
fn expected_height_m(g: &FuselageConfig) -> f64 {
    g.effective_height_m() + g.upper_deck_hump().map_or(0.0, |hump| hump.height_m)
}

/// Widest and tallest section of the loft, m.
fn extents(loft: &Loft) -> (f64, f64) {
    let (x0, x1) = loft.x_range();
    let mut xs: Vec<f64> = (0..=EXTENT_SAMPLES)
        .map(|i| x0 + (x1 - x0) * i as f64 / EXTENT_SAMPLES as f64)
        .collect();
    if let Some((a, b)) = loft.cabin_cylinder() {
        xs.extend([a, 0.5 * (a + b), b]);
    }
    xs.iter().fold((0.0_f64, 0.0_f64), |(w, h), &x| {
        let s = loft.section(x);
        (w.max(2.0 * s.a), h.max(loft.crown_z(x) - loft.keel_z(x)))
    })
}

fn check_extent(
    quantity: &'static str,
    reference: &LoftRef,
    loft_m: f64,
    config_m: f64,
    report: &mut BuildReport,
) -> Result<(), BuildError> {
    let relative = (loft_m - config_m).abs() / config_m;
    if relative > EXTENT_ERR {
        return Err(BuildError::LoftExtent {
            loft_id: reference.id.clone(),
            quantity,
            loft_m,
            config_m,
        });
    }
    if relative > EXTENT_OK {
        report.warn(
            "loft_extent_mismatch",
            format!(
                "loft {}: maximum {quantity} {loft_m:.3} m differs by {:.1} % from the \
                 configured {config_m:.3} m; the loft is used as it is",
                reference.id,
                100.0 * relative
            ),
        );
    }
    Ok(())
}

fn warn_ignored_fields(g: &FuselageConfig, id: &str, report: &mut BuildReport) {
    if !g.generated_sections.is_empty() || !g.custom_sections.is_empty() {
        report.warn(
            "loft_sections_ignored",
            format!(
                "loft {id}: custom and generated fuselage sections act on the parametric \
                 laws only and are ignored while a loft is set"
            ),
        );
    }
}
