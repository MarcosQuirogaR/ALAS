// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The reference-wing convention at build time: which wing the airplane's
//! `s_ref`, `b_ref`, `c_ref` and `%MAC` datum describe.
//!
//! The construction itself (the trapezoid of the outboard panel extended to
//! the centreline) is `alas_config::TransportPlanform::reference_trapezoid`.
//! This module places it in the aircraft frame, attaches it to the main wing
//! and checks it against an optional published area. The drawn lofted wing,
//! its wetted area and its lattice panels are untouched.

use alas_config::{DesignVector, ReferenceWingMode, TransportPlanform};

use super::{AircraftBuilder, BuildReport, GeometryContract};
use crate::aircraft::wing::{Wing, WingReference};

impl AircraftBuilder {
    /// Attach the reference wing (default: none, the drawn planform) to the
    /// main wing, `wings[0]`.
    pub(super) fn attach_reference_wing(
        &self,
        wings: &mut [Wing],
        planform: &TransportPlanform,
        dv: &DesignVector,
        report: &mut BuildReport,
    ) {
        let x_wing_global = self.geometry.wing.root_datum_x_m + dv.wing_x_shift_m;
        let drawn = DrawnWing {
            area_m2: wings[0].reference_area(),
            span_m: wings[0].projected_span(),
            mac_m: wings[0].mean_aerodynamic_chord(),
            mac_le_x_m: wings[0].mac_station().x_le_m,
        };
        wings[0].reference = self.reference_wing(planform, &drawn, x_wing_global, report);
    }

    /// The reference wing of the main wing built at fuselage station
    /// `x_wing_global`, or `None` when the drawn planform is the reference
    /// (the default, and always under the reference-compatibility contract).
    pub(super) fn reference_wing(
        &self,
        planform: &TransportPlanform,
        drawn: &DrawnWing,
        x_wing_global: f64,
        report: &mut BuildReport,
    ) -> Option<WingReference> {
        if self.geometry_contract != GeometryContract::Product {
            return None;
        }
        let config = &self.geometry.wing.reference_wing;
        if let Some(message) = config.validation_error() {
            report.warn("reference_wing_published_area_invalid", message);
        }
        let valid_published = config.validation_error().is_none();
        let trapezoid = match config.mode {
            ReferenceWingMode::Drawn => None,
            ReferenceWingMode::Published => planform.reference_trapezoid(),
            ReferenceWingMode::Trapezoid => {
                let trapezoid = planform.reference_trapezoid();
                if trapezoid.is_none() {
                    report.warn(
                        "reference_wing_unphysical",
                        "the outboard panel extended to the centreline has a non-positive \
                         chord; the drawn planform is the reference wing"
                            .to_owned(),
                    );
                }
                trapezoid
            }
        };
        if config.mode == ReferenceWingMode::Published && valid_published {
            return self.published_reference(config, planform, trapezoid, drawn, x_wing_global);
        }
        let used_area_m2 = trapezoid.map_or(drawn.area_m2, |t| t.area_m2);
        if valid_published {
            if let Some(relative) = config.published_area_disagreement(used_area_m2) {
                report.warn(
                    "reference_wing_published_area_mismatch",
                    format!(
                        "the {} reference area is {used_area_m2:.2} m^2, {:+.2} % from the \
                         published {:.2} m^2 (tolerance {:.0} %)",
                        if trapezoid.is_some() {
                            "reference-trapezoid"
                        } else {
                            "drawn-planform"
                        },
                        100.0 * relative,
                        config.published_area_m2.unwrap_or(f64::NAN),
                        100.0 * alas_config::PUBLISHED_AREA_TOLERANCE,
                    ),
                );
            }
        }
        trapezoid.map(|t| WingReference {
            area_m2: t.area_m2,
            span_m: t.span_m,
            mac_m: t.mac_m,
            mac_le_x_m: x_wing_global + t.mac_leading_edge_x_m,
        })
    }
}

impl AircraftBuilder {
    /// The `Published` reference mapped onto the candidate planform (see
    /// `alas_config::ReferenceWingConfig::published_reference`): the area,
    /// MAC and MAC leading edge are the published ones at the registered
    /// geometry and follow the built trapezoid and `wing_x_shift_m`
    /// elsewhere. The span is the drawn span. A MAC or MAC station that is not
    /// published comes from the reference trapezoid of the candidate planform,
    /// or, when that is not constructible, from the drawn wing itself.
    fn published_reference(
        &self,
        config: &alas_config::ReferenceWingConfig,
        planform: &TransportPlanform,
        trapezoid: Option<alas_config::ReferenceTrapezoid>,
        drawn: &DrawnWing,
        x_wing_global: f64,
    ) -> Option<WingReference> {
        let mapped = config.published_reference(planform, x_wing_global)?;
        let mac_m = mapped
            .mac_m
            .or_else(|| trapezoid.map(|t| t.mac_m))
            .unwrap_or(drawn.mac_m);
        let mac_le_x_m = mapped
            .mac_le_x_m
            .or_else(|| trapezoid.map(|t| x_wing_global + t.mac_leading_edge_x_m))
            .unwrap_or(drawn.mac_le_x_m);
        Some(WingReference {
            area_m2: mapped.area_m2,
            span_m: drawn.span_m,
            mac_m,
            mac_le_x_m,
        })
    }
}

/// The drawn main wing's own reference quantities (fallbacks of the
/// `Published` mode).
pub(super) struct DrawnWing {
    pub area_m2: f64,
    pub span_m: f64,
    pub mac_m: f64,
    pub mac_le_x_m: f64,
}
