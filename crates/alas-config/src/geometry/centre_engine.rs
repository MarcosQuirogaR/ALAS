// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The installation of a centreline (tail) engine, a trijet's No. 2 engine.
//!
//! A podded wing engine is a body of revolution scaled from the engine
//! catalogue, hung under the wing by `z_m` and `inlet_x_offset_m`. The centre
//! engine of a DC-10 or an L-1011 is a different object: a long, non-circular
//! pod that carries the fin on its upper surface, and whose inlet duct and
//! nozzle fairing are part of the pod. This group states that pod directly.
//!
//! Frame: x metres aft of the pod's inlet face, y/z as the geometry axes of
//! the fuselage (z up). The pod's aft end is stated from the end of the
//! fuselage, so its inlet station follows from
//! [`CentreEngineInstallation::inlet_x_m`].

use serde::{Deserialize, Serialize};

/// A centreline engine pod: axis height, cross-section and outline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CentreEngineInstallation {
    /// Height of the pod axis at the inlet face, m, in the fuselage-geometry
    /// axes (z up, the frame `fuselage.tail_z_m` and `vstab_z_m` are
    /// measured in).
    pub axis_z_m: f64,
    /// Pitch of the pod axis, degrees, positive when the aft end is lower
    /// than the inlet face (a straight duct inclined nose-up).
    pub axis_pitch_deg: f64,
    /// Maximum pod width, m.
    pub width_m: f64,
    /// Maximum pod height, m (the cross-section is a superellipse of
    /// exponent two, so `width_m` and `height_m` are its axes).
    pub height_m: f64,
    /// Outline as `(station aft of the inlet face in metres, fraction of the
    /// maximum width and height)` pairs; the last station is the nozzle exit.
    pub profile: Vec<(f64, f64)>,
    /// How far the nozzle exit lies aft of the end of the fuselage, m. A pod can
    /// run on past a hull that closes under it, as on the DC-10.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub nozzle_aft_of_fuselage_m: f64,
    /// The struts that carry the pod over the fuselage crown, where drawn.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub struts: Vec<CentrePodStrut>,
    /// The fixed fairing behind the fin over the pod's tail cone, where drawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fin_aft_fairing: Option<FinAftFairing>,
}

/// A strut in the gap between the fuselage crown and the pod's underside, on the
/// plane of symmetry. Its height follows from the two bodies.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CentrePodStrut {
    /// Station of the strut's mid-chord, m aft of the pod's inlet face.
    pub station_m: f64,
    /// Chord where it meets the pod, m.
    pub chord_at_pod_m: f64,
    /// Chord where it meets the fuselage, m.
    pub chord_at_fuselage_m: f64,
}

/// The fixed blade behind the fin: from the fin trailing edge to a vertical aft edge,
/// above the pod top and under a straight top line. Heights are in the frame of
/// [`CentreEngineInstallation::axis_z_m`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinAftFairing {
    /// Height where the top line leaves the fin trailing edge, m.
    pub top_start_z_m: f64,
    /// Station of the aft edge, m aft of the pod's inlet face.
    pub top_end_station_m: f64,
    /// Height of the top line at the aft edge, m.
    pub top_end_z_m: f64,
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

impl CentreEngineInstallation {
    /// Pod length, inlet face to nozzle exit, m (zero for an empty profile).
    pub fn length_m(&self) -> f64 {
        self.profile.last().map_or(0.0, |&(station, _)| station)
    }

    /// Inlet face station, m aft of the nose tip, on a fuselage of `length_m`.
    pub fn inlet_x_m(&self, fuselage_length_m: f64) -> f64 {
        fuselage_length_m + self.nozzle_aft_of_fuselage_m - self.length_m()
    }

    /// Why the struts or the fin fairing cannot be built: a chord not above zero, or a
    /// station or height that is not a number or lies off the pod. `None` when they can.
    pub fn surface_issue(&self) -> Option<String> {
        let length_m = self.length_m();
        self.struts
            .iter()
            .enumerate()
            .find_map(|(index, strut)| {
                let chords = [strut.chord_at_pod_m, strut.chord_at_fuselage_m];
                if chords.iter().any(|chord| !chord.is_finite() || *chord <= 0.0) {
                    return Some(format!(
                        "strut {index}: chords must be above zero, got {} m at the pod and {} m at the fuselage",
                        strut.chord_at_pod_m, strut.chord_at_fuselage_m
                    ));
                }
                (!(0.0..=length_m).contains(&strut.station_m)).then(|| {
                    format!(
                        "strut {index}: station {} m is off the pod, 0 to {length_m} m",
                        strut.station_m
                    )
                })
            })
            .or_else(|| {
                let fairing = self.fin_aft_fairing.as_ref()?;
                let values = [
                    fairing.top_start_z_m,
                    fairing.top_end_station_m,
                    fairing.top_end_z_m,
                ];
                (!values.iter().all(|value| value.is_finite()))
                    .then(|| format!("fin fairing: values must be finite, got {values:?}"))
            })
    }

    /// Whether the dimensions are finite and positive, the outline is a strictly
    /// increasing list of fractions in `(0, 1]` and the struts and fairing can be built.
    pub fn is_valid(&self) -> bool {
        self.surface_issue().is_none()
            && self.axis_z_m.is_finite()
            && self.axis_pitch_deg.is_finite()
            && self.axis_pitch_deg.abs() < 15.0
            && self.width_m.is_finite()
            && self.width_m > 0.0
            && self.height_m.is_finite()
            && self.height_m > 0.0
            && self.nozzle_aft_of_fuselage_m.is_finite()
            && self.profile.len() >= 2
            && self.profile.first().is_some_and(|&(x, _)| x == 0.0)
            && self
                .profile
                .windows(2)
                .all(|pair| pair[1].0 > pair[0].0 && pair[0].0.is_finite())
            && self
                .profile
                .iter()
                .all(|&(_, f)| f.is_finite() && f > 0.0 && f <= 1.0)
    }
}

/// Lip exponent of a pod expressed in the nacelle family (template, confidence C).
const POD_LIP_EXPONENT: f64 = 2.2;
/// End slope of the pod's tail cone over its mean slope (template, confidence C).
const POD_TAIL_SLOPE_FACTOR: f64 = 1.2;

impl CentreEngineInstallation {
    /// The pod in the nacelle family: a single body (no fan-nozzle step) whose
    /// section half-width is `width_m / 2` times the profile fraction and
    /// whose height is `height_m / width_m` times the width.
    ///
    /// The crest begins at the first station of fraction one and the cylinder
    /// ends at the last; the tail cone runs to the last station. The lip
    /// exponent and the tail-cone end slope are templates (the drawing shows
    /// only the outline stations); `None` when the pod or the result is not valid.
    pub fn nacelle_shape_v2(&self) -> Option<crate::engines::NacelleShapeV2> {
        use crate::engines::NacelleShapeV2;
        if !self.is_valid() {
            return None;
        }
        let r = 0.5 * self.width_m;
        // A drawn step in the outline is not the family's one boat tail.
        let steps = self
            .profile
            .windows(2)
            .any(|pair| (pair[0].1 - pair[1].1) * r > pair[1].0 - pair[0].0);
        if steps {
            return None;
        }
        let &(end_x, end_f) = self.profile.last()?;
        let crest = self.profile.iter().find(|&&(_, f)| f >= 1.0)?.0;
        let cylinder_end = self.profile.iter().rev().find(|&&(_, f)| f >= 1.0)?.0;
        let tail = end_x - cylinder_end;
        if tail <= 0.0 || cylinder_end <= crest {
            return None;
        }
        let mean_slope = (1.0 - end_f) * r / tail;
        let shape = NacelleShapeV2 {
            highlight_radius_ratio: self.profile[0].1,
            cowl_cylinder_end_ratio: cylinder_end / end_x,
            fan_cowl_length_m: end_x,
            fan_nozzle_radius_ratio: end_f,
            core_cowl_length_m: end_x,
            core_nozzle_radius_ratio: end_f,
            lip_length_ratio: crest / r,
            lip_exponent: POD_LIP_EXPONENT,
            boat_tail_angle_deg: (POD_TAIL_SLOPE_FACTOR * mean_slope).atan().to_degrees(),
            te_thickness_m: 0.0,
            plug_tip_radius_m: 0.04 * r,
            ellipticity: self.height_m / self.width_m,
            droop_m: 0.0,
            core_start_radius_ratio: None,
            plug_start_radius_ratio: None,
            axis_pitch_deg: None,
            provenance: Vec::new(),
        };
        shape.is_valid(end_x, r).then_some(shape)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pod() -> CentreEngineInstallation {
        CentreEngineInstallation {
            axis_z_m: 4.6,
            axis_pitch_deg: 4.0,
            width_m: 2.8,
            height_m: 3.3,
            profile: vec![(0.0, 0.95), (0.5, 1.0), (11.0, 1.0), (13.8, 0.3)],
            nozzle_aft_of_fuselage_m: 0.0,
            struts: Vec::new(),
            fin_aft_fairing: None,
        }
    }

    #[test]
    fn length_is_the_last_station() {
        assert_eq!(pod().length_m(), 13.8);
        assert!(pod().is_valid());
    }

    #[test]
    fn a_non_monotone_or_oversized_outline_is_rejected() {
        let mut p = pod();
        p.profile[2].0 = 0.2;
        assert!(!p.is_valid());
        let mut p = pod();
        p.profile[1].1 = 1.2;
        assert!(!p.is_valid());
    }

    #[test]
    // A failed expect here is the assertion failing.
    #[allow(clippy::expect_used)]
    fn the_pod_is_expressible_in_the_nacelle_family() {
        let v2 = pod().nacelle_shape_v2().expect("valid pod");
        assert_eq!(v2.core_cowl_length_m, v2.fan_cowl_length_m);
        assert!((v2.ellipticity - 3.3 / 2.8).abs() < 1e-12);
        assert!((v2.lip_length_ratio - 0.5 / 1.4).abs() < 1e-12);
        let mut bad = pod();
        bad.profile = vec![(0.0, 0.9), (13.8, 0.3)];
        assert!(bad.nacelle_shape_v2().is_none());
    }
}
