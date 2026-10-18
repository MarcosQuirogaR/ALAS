// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Cross-sections of the podded engines and of the centre-engine pod.
//!
//! The default path is the legacy polyline: circular sections of radius
//! `radius_scale_m * fraction` at the profile stations (and the pod's
//! width/height pair), unchanged. With `EngineConfig::nacelle_model = V2` (or
//! the builder override [`super::AircraftBuilder::with_nacelle_v2`])
//! an engine whose catalogue entry carries `nacelle_shape_v2` (and whose
//! profile and radius are still the catalogue's) is built from the G2
//! revolve [`NacelleLoft`] instead, sampled to a station polyline; the DC-10
//! pod is expressed in the same family through
//! `CentreEngineInstallation::nacelle_shape_v2`. Anything without V2 data
//! keeps the legacy sections.

use alas_config::{CentreEngineInstallation, EngineConfig, NacelleShapeV2};

use alas_config::{DesignVector, WingShape};

use super::{AircraftBuilder, BuildError, BuildReport, GeometryContract};
use crate::aircraft::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};
use crate::loft::{NacelleError, NacelleLoft, Section};

/// Stations per piece at density one (see [`NacelleLoft::polyline_sections`]).
const V2_DENSITY: usize = 1;

/// What the V2 request resolved to for one nacelle.
#[derive(Debug)]
pub(super) enum NacelleV2 {
    /// No V2 data applies (no catalogue entry data, an edited installation, a
    /// pod that cannot be expressed): the legacy polyline is the design.
    Unavailable,
    /// The G2 revolve.
    Built(Box<NacelleLoft>),
    /// V2 data applies but the revolve was rejected: the legacy polyline is
    /// used instead and the report says so.
    Failed(NacelleError),
}

fn revolve(shape: &NacelleShapeV2, length_m: f64, radius_m: f64) -> NacelleV2 {
    match NacelleLoft::new(shape, length_m, radius_m) {
        Ok(loft) => NacelleV2::Built(Box::new(loft)),
        Err(e) => NacelleV2::Failed(e),
    }
}

/// The V2 outcome of an engine installation: the catalogue entry has
/// `nacelle_shape_v2` and the installation still carries that entry's
/// silhouette and radius (an edited engine keeps the legacy path).
fn engine_v2(g: &EngineConfig) -> NacelleV2 {
    let Ok(spec) = alas_config::engines::get(&g.engine_name) else {
        return NacelleV2::Unavailable;
    };
    let Some(shape) = spec.nacelle_shape_v2.as_ref() else {
        return NacelleV2::Unavailable;
    };
    if g.nacelle_profile != spec.nacelle_profile() || g.radius_scale_m != spec.nacelle_max_radius_m
    {
        return NacelleV2::Unavailable;
    }
    revolve(shape, g.nacelle_length_m(), g.radius_scale_m)
}

/// The V2 nacelle of an engine installation, if it opts in and the revolve is
/// accepted; see [`AircraftBuilder::build_reported`] for the loud fallback.
///
/// [`AircraftBuilder::build_reported`]: super::AircraftBuilder::build_reported
#[must_use]
pub fn engine_nacelle_loft(g: &EngineConfig) -> Option<NacelleLoft> {
    match engine_v2(g) {
        NacelleV2::Built(loft) => Some(*loft),
        _ => None,
    }
}

fn centre_pod_v2(pod: &CentreEngineInstallation) -> NacelleV2 {
    match pod.nacelle_shape_v2() {
        Some(shape) => revolve(&shape, pod.length_m(), 0.5 * pod.width_m),
        None => NacelleV2::Unavailable,
    }
}

/// The centre-engine pod in the nacelle family, if it can be expressed.
#[must_use]
pub fn centre_pod_nacelle_loft(pod: &CentreEngineInstallation) -> Option<NacelleLoft> {
    match centre_pod_v2(pod) {
        NacelleV2::Built(loft) => Some(*loft),
        _ => None,
    }
}

/// The revolve if there is one; a rejected revolve is recorded in `report`
/// (code `nacelle_v2_fallback`) and `None` returned so the caller builds the
/// legacy polyline.
fn accept(outcome: NacelleV2, what: &str, report: &mut BuildReport) -> Option<Box<NacelleLoft>> {
    match outcome {
        NacelleV2::Built(loft) => Some(loft),
        NacelleV2::Unavailable => None,
        NacelleV2::Failed(e) => {
            report.warn(
                "nacelle_v2_fallback",
                format!(
                    "{what}: the V2 nacelle was requested and its data apply, but the revolve \
                     was rejected ({e}); the legacy polyline nacelle was built instead"
                ),
            );
            None
        }
    }
}

fn xsecs_from_sections(
    stations: &[(f64, Section)],
    z_of_x: impl Fn(f64) -> f64,
) -> Result<Vec<FuselageXSec>, BuildError> {
    let mut out = Vec::with_capacity(stations.len());
    for &(x, s) in stations {
        out.push(FuselageXSec::new(
            [x, 0.0, z_of_x(x) + s.zc],
            None,
            Some(2.0 * s.a),
            Some(2.0 * s.e),
            DEFAULT_SHAPE,
        )?);
    }
    Ok(out)
}

/// Sections of the centre-engine pod, in the pod frame (x from the inlet
/// face, z along the inclined axis).
pub(super) fn centre_pod_xsecs(
    pod: &CentreEngineInstallation,
    v2: bool,
    report: &mut BuildReport,
) -> Result<Vec<FuselageXSec>, BuildError> {
    let outcome = if v2 {
        centre_pod_v2(pod)
    } else {
        NacelleV2::Unavailable
    };
    centre_pod_xsecs_from(pod, outcome, report)
}

fn centre_pod_xsecs_from(
    pod: &CentreEngineInstallation,
    outcome: NacelleV2,
    report: &mut BuildReport,
) -> Result<Vec<FuselageXSec>, BuildError> {
    let drop_per_metre = pod.axis_pitch_deg.to_radians().tan();
    if let Some(loft) = accept(outcome, "centre-engine pod", report) {
        return xsecs_from_sections(&loft.polyline_sections(V2_DENSITY), |x| -x * drop_per_metre);
    }
    let mut xsecs = Vec::with_capacity(pod.profile.len());
    for &(x, f) in &pod.profile {
        xsecs.push(FuselageXSec::new(
            [x, 0.0, -x * drop_per_metre],
            None,
            Some(pod.width_m * f),
            Some(pod.height_m * f),
            DEFAULT_SHAPE,
        )?);
    }
    Ok(xsecs)
}

/// Sections of a podded engine, in the nacelle frame (x from the inlet face).
pub(super) fn engine_xsecs(
    g: &EngineConfig,
    v2: bool,
    report: &mut BuildReport,
) -> Result<Vec<FuselageXSec>, BuildError> {
    let outcome = if v2 {
        engine_v2(g)
    } else {
        NacelleV2::Unavailable
    };
    engine_xsecs_from(g, outcome, report)
}

fn engine_xsecs_from(
    g: &EngineConfig,
    outcome: NacelleV2,
    report: &mut BuildReport,
) -> Result<Vec<FuselageXSec>, BuildError> {
    let what = format!("engine {}", g.engine_name);
    if let Some(loft) = accept(outcome, &what, report) {
        return xsecs_from_sections(&loft.polyline_sections(V2_DENSITY), |_| 0.0);
    }
    let mut xsecs = Vec::with_capacity(g.nacelle_profile.len());
    for &(x, r) in &g.nacelle_profile {
        xsecs.push(FuselageXSec::new(
            [x, 0.0, 0.0],
            Some(g.radius_scale_m * r),
            None,
            None,
            DEFAULT_SHAPE,
        )?);
    }
    Ok(xsecs)
}

impl AircraftBuilder {
    /// The podded engines, one small [`Fuselage`] per spanwise position:
    /// `_build_engines`. See the module doc for the two placement branches.
    pub(super) fn build_engines(
        &self,
        dv: &DesignVector,
        shape: WingShape,
    ) -> Result<Vec<Fuselage>, BuildError> {
        self.build_engines_reported(dv, shape, &mut BuildReport::default())
    }

    pub(super) fn build_engines_reported(
        &self,
        dv: &DesignVector,
        shape: WingShape,
        report: &mut BuildReport,
    ) -> Result<Vec<Fuselage>, BuildError> {
        let g = &self.geometry.engine;
        let x_wing_global = self.geometry.wing.root_datum_x_m + dv.wing_x_shift_m;
        let wing = &self.geometry.wing;
        let planform = wing.transport_planform(dv)?;
        // Drawings and clearance tables show the nacelles at their static height; a wing
        // that declares so keeps them there in the flight shape.
        let heights = wing.heights(
            if wing.nacelles_hold_ground_height {
                WingShape::Ground
            } else {
                shape
            },
            &planform,
        )?;

        let mut nacelles = Vec::with_capacity(g.spanwise_positions_m.len());
        for (engine_index, &y_pos) in g.spanwise_positions_m.iter().enumerate() {
            if let Some(pod) = g
                .centre_engine
                .as_ref()
                .filter(|pod| y_pos == 0.0 && pod.is_valid())
            {
                // The drawn tail-engine pod: its own axis height, a
                // width/height pair and an outline ending at the tail end.
                let xsecs = centre_pod_xsecs(pod, self.nacelle_v2_active(), report)?;
                let x_inlet = dv.fuselage_length_m - pod.length_m();
                nacelles.push(Fuselage::new("Nacelle Center", xsecs).translate([
                    x_inlet,
                    0.0,
                    pod.axis_z_m,
                ]));
                continue;
            }
            let (name, x_inlet, z_nacelle) = if y_pos == 0.0 {
                // Centerline tail-mounted engine (e.g. a trijet's tail
                // engine), placed on top of the tailcone at the base of the
                // vstab.
                let x_inlet = dv.fuselage_length_m - g.nacelle_length_m();
                let z_nacelle = self.geometry.fuselage.tail_z_m + g.radius_scale_m + 0.2;
                ("Nacelle Center".to_owned(), x_inlet, z_nacelle)
            } else {
                let name = if y_pos > 0.0 {
                    "Nacelle R"
                } else {
                    "Nacelle L"
                }
                .to_owned();
                let leading_edge_offset = match self.geometry_contract {
                    GeometryContract::Product => planform.leading_edge_x_at(y_pos.abs())?,
                    GeometryContract::ReferenceCompatibility => {
                        y_pos.abs() * dv.sweep_deg.to_radians().tan()
                    }
                };
                let x_inlet = x_wing_global + leading_edge_offset - g.inlet_x_offset_m;

                // The local leading-edge height of the wing shape being
                // built, so the nacelle hangs under the wing its pylon is
                // on; then the local Z offset relative to that leading edge.
                let z_wing = heights.at(&planform, y_pos);
                (name, x_inlet, z_wing + g.z_for_engine_m(engine_index))
            };

            let xsecs = engine_xsecs(g, self.nacelle_v2_active(), report)?;
            nacelles.push(Fuselage::new(name, xsecs).translate([x_inlet, y_pos, z_nacelle]));
        }
        Ok(nacelles)
    }
}

impl AircraftBuilder {
    /// Whether nacelles are built from the V2 revolve: the explicit
    /// [`Self::with_nacelle_v2`] override if given, otherwise the
    /// configuration's `EngineConfig::nacelle_model`. Never under the
    /// reference-compatibility contract.
    fn nacelle_v2_active(&self) -> bool {
        self.geometry_contract == GeometryContract::Product
            && self
                .nacelle_v2
                .unwrap_or(self.geometry.engine.nacelle_model == alas_config::NacelleModel::V2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::GeometryConfig;

    fn a320_engine() -> EngineConfig {
        let mut g = GeometryConfig::default().engine;
        g.engine_name = "CFM56-5B4/3".to_owned();
        g.apply_engine_spec();
        g
    }

    #[test]
    fn the_legacy_path_is_the_polyline_bit_for_bit() {
        let g = a320_engine();
        let xsecs = engine_xsecs(&g, false, &mut BuildReport::default()).unwrap();
        assert_eq!(xsecs.len(), g.nacelle_profile.len());
        for (xsec, &(x, r)) in xsecs.iter().zip(&g.nacelle_profile) {
            assert_eq!(xsec.xyz_c, [x, 0.0, 0.0]);
            assert_eq!(
                xsec.width.to_bits(),
                (2.0 * (g.radius_scale_m * r)).to_bits()
            );
            assert_eq!(xsec.height.to_bits(), xsec.width.to_bits());
        }
    }

    #[test]
    fn an_engine_without_v2_data_keeps_the_legacy_sections_when_v2_is_on() {
        let mut g = GeometryConfig::default().engine;
        g.engine_name = "GE9X".to_owned();
        g.apply_engine_spec();
        assert!(engine_nacelle_loft(&g).is_none());
        assert_eq!(
            engine_xsecs(&g, true, &mut BuildReport::default()).unwrap(),
            engine_xsecs(&g, false, &mut BuildReport::default()).unwrap()
        );
    }

    #[test]
    fn an_edited_installation_keeps_the_legacy_path() {
        let mut g = a320_engine();
        g.radius_scale_m += 0.01;
        assert!(engine_nacelle_loft(&g).is_none());
    }

    #[test]
    fn opting_in_builds_the_g2_revolve_of_the_same_size() {
        let g = a320_engine();
        let loft = engine_nacelle_loft(&g).expect("A320 engine has V2 data");
        let v2 = engine_xsecs(&g, true, &mut BuildReport::default()).unwrap();
        assert_ne!(
            v2,
            engine_xsecs(&g, false, &mut BuildReport::default()).unwrap()
        );
        assert!(v2.windows(2).all(|p| p[0].xyz_c[0] < p[1].xyz_c[0]));
        let widest = v2.iter().map(|x| x.width).fold(0.0, f64::max);
        assert!((widest - 2.0 * g.radius_scale_m).abs() < 1e-9);
        let last = v2.last().unwrap().xyz_c[0];
        assert!((last - g.nacelle_length_m()).abs() < 1e-9);
        assert_eq!(loft.body_count(), 2);
    }

    #[test]
    fn a_rejected_revolve_falls_back_loudly_to_the_legacy_polyline() {
        let g = a320_engine();
        let mut report = BuildReport::default();
        let outcome = NacelleV2::Failed(NacelleError::NotMonotone("test: lip overshoot"));
        let xsecs = engine_xsecs_from(&g, outcome, &mut report).unwrap();
        assert_eq!(
            xsecs,
            engine_xsecs(&g, false, &mut BuildReport::default()).unwrap()
        );
        assert!(report.has_warning("nacelle_v2_fallback"));
        assert!(report.warnings[0].message.contains("lip overshoot"));
        assert!(report.warnings[0].message.contains(&g.engine_name));
    }

    #[test]
    fn a_working_revolve_and_a_missing_one_add_no_warning() {
        let g = a320_engine();
        let mut report = BuildReport::default();
        engine_xsecs(&g, true, &mut report).unwrap();
        engine_xsecs_from(&g, NacelleV2::Unavailable, &mut report).unwrap();
        assert!(report.warnings.is_empty());
    }
}
