// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The struts under a centre engine pod and the fairing behind its fin.
//!
//! Frame: x metres aft of the nose tip, y to starboard, z up, in the plane of
//! symmetry.
//!
//! Both are structure in the flow on the manufacturer's drawing, so they are product
//! surfaces: built, exported and counted in the wetted area and the drag build-up.
//! Neither is in the vortex lattice ([`Wing::is_geometry_only`]). A strut is a short
//! blade between two bodies the lattice does not carry. The fairing is a fixed blade
//! behind the fin, and its side force is left out with the pod's.
//!
//! A strut stands from the fuselage crown to the pod's underside at its station, and
//! is dropped when no gap is left there. The fairing is the outline its top line
//! closes with the built fin trailing edge, the pod top and a vertical aft edge; its
//! sections are that outline cut at constant height, as for the dorsal fin.

use alas_config::{CentreEngineInstallation, CentrePodStrut, DesignVector, FinAftFairing};

use super::dorsal::{cut_strips, sections_of, FinEdge};
use super::fin_root::FinSeat;
use super::{AircraftBuilder, BuildError};
use crate::aircraft::airfoil::Airfoil;
use crate::aircraft::wing::{Wing, WingXSec, CENTRE_POD_STRUT_NAME, FIN_AFT_FAIRING_NAME};

/// Section of a strut: the thickest the lifting-surface form factor is good for. The
/// drawing gives the side chord; the drawn strut is blunter than this in front view.
const STRUT_SECTION: &str = "naca0030";

/// Section of the fairing: about 0.25 m thick on its 4 m chord in the plan view.
const FAIRING_SECTION: &str = "naca0006";

/// A gap under the pod smaller than this leaves no strut, m.
const SMALLEST_STRUT_M: f64 = 1.0e-3;

/// Points on the pod top under the fairing: finer than the step in the pod outline
/// needs on a body of tens of metres.
const POD_TOP_POINTS: usize = 96;

/// A fairing whose outline has no height or no length has no surface; this is the
/// shortest run taken as a length, m.
const SHORTEST_FAIRING_M: f64 = 1.0e-3;

impl AircraftBuilder {
    /// The struts and fin fairing of the centre pod of `dv`, on the built `fin` and
    /// the bodies `seat`; empty without a centre pod or when they find no room.
    ///
    /// # Errors
    ///
    /// When a section's airfoil cannot be resolved.
    pub(super) fn pod_surfaces(
        &self,
        dv: &DesignVector,
        fin: &Wing,
        seat: &FinSeat,
    ) -> Result<Vec<Wing>, BuildError> {
        let g = &self.geometry.engine;
        let Some(pod) = g
            .centre_engine
            .as_ref()
            .filter(|pod| pod.is_valid() && g.spanwise_positions_m.contains(&0.0))
        else {
            return Ok(Vec::new());
        };
        let x_inlet = pod.inlet_x_m(dv.fuselage_length_m);
        let mut surfaces = Vec::new();
        if !pod.struts.is_empty() {
            let section = Self::resolve(STRUT_SECTION)?;
            surfaces.extend(pod_struts(pod, x_inlet, seat, &section));
        }
        if let Some(fairing) = &pod.fin_aft_fairing {
            let section = Self::resolve(FAIRING_SECTION)?;
            let strips = cut_strips(&self.geometry.empennage);
            surfaces.extend(fin_aft_fairing(
                fairing, x_inlet, fin, seat, strips, &section,
            ));
        }
        Ok(surfaces)
    }
}

fn pod_struts(
    pod: &CentreEngineInstallation,
    x_inlet_m: f64,
    seat: &FinSeat,
    section: &Airfoil,
) -> Vec<Wing> {
    pod.struts
        .iter()
        .enumerate()
        .filter_map(|(index, strut)| {
            let name = format!("{CENTRE_POD_STRUT_NAME} {}", index + 1);
            pod_strut(name, strut, x_inlet_m, seat, section)
        })
        .collect()
}

fn pod_strut(
    name: String,
    strut: &CentrePodStrut,
    x_inlet_m: f64,
    seat: &FinSeat,
    section: &Airfoil,
) -> Option<Wing> {
    let x = x_inlet_m + strut.station_m;
    let crown = seat.fuselage_alone()?.top_at(x)?;
    let underside = seat.pod_underside_at(x)?;
    if underside - crown < SMALLEST_STRUT_M {
        return None;
    }
    let at =
        |z: f64, chord: f64| WingXSec::new([x - 0.5 * chord, 0.0, z], chord, 0.0, section.clone());
    Some(Wing::new(
        name,
        vec![
            at(crown, strut.chord_at_fuselage_m),
            at(underside, strut.chord_at_pod_m),
        ],
        false,
    ))
}

fn fin_aft_fairing(
    fairing: &FinAftFairing,
    x_inlet_m: f64,
    fin: &Wing,
    seat: &FinSeat,
    strips: usize,
    section: &Airfoil,
) -> Option<Wing> {
    let trailing_edge = FinEdge(
        fin.xsecs
            .iter()
            .map(|xsec| [xsec.xyz_le[0] + xsec.chord, xsec.xyz_le[1], xsec.xyz_le[2]])
            .collect(),
    );
    let root_z = trailing_edge.root_z();
    let top_z = fairing.top_start_z_m;
    if top_z <= root_z || top_z >= trailing_edge.tip_z() {
        return None;
    }
    let start = trailing_edge.at(top_z);
    let root = trailing_edge.at(root_z);
    let x_end = x_inlet_m + fairing.top_end_station_m;
    let bottom = seat.top_at(x_end)?;
    if x_end - start[0] < SHORTEST_FAIRING_M
        || x_end - root[0] < SHORTEST_FAIRING_M
        || fairing.top_end_z_m <= bottom
    {
        return None;
    }
    // Round the outline: top line, aft edge, pod top forward, fin edge up.
    let mut outline = vec![
        [start[0], top_z],
        [x_end, fairing.top_end_z_m],
        [x_end, bottom],
    ];
    outline.extend((1..POD_TOP_POINTS).filter_map(|index| {
        let x = x_end + (root[0] - x_end) * index as f64 / POD_TOP_POINTS as f64;
        seat.top_at(x).map(|z| [x, z])
    }));
    outline.push([root[0], root_z]);
    outline.extend(
        trailing_edge
            .corners(root_z, top_z)
            .map(|z| [trailing_edge.at(z)[0], z]),
    );
    let sections = sections_of(&outline, &trailing_edge, strips, section)?;
    Some(Wing::new(FIN_AFT_FAIRING_NAME, sections, false))
}
