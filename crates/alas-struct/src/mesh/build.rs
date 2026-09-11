// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Assembling the deck: classify the ribs, place the grids, then hand each
//! element family to [`super::elements`] in the order that numbers them.
//!
//! The classification is what everything downstream is written against. A rib
//! is one of three things -- it carries skin, it was truncated by the root
//! plane and gets rivetted to the skin instead, or it reaches so little of its
//! nominal chord that it is aerodynamic surface with no structure in it at all
//! -- and which one it is decides whether it gets panels, which panels, and
//! how thick they are.

use std::collections::HashSet;

use alas_config::materials::MaterialSpec;
use alas_config::{DesignRequirements, EngineConfig, MassModelConfig, StructuresConfig};
use alas_geom::wing_structure::{RibStation, WingStructureGeometry};

use super::cards::{Deck, Mat1, Param, ParamValue, Pshell, Spc1};
use super::elements::{
    MID_CAP, MID_RIB, MID_SKIN, MID_WEB, PID_MAIN_RIB, PID_SEC_RIB, PID_SKIN, PID_TE_STRIP,
    PID_WEB_BASE,
};
use super::nodes::{NodeMap, Surface};
use super::{
    elements, health, rivets, MeshError, MeshHealthReport, MeshNodeIndex, SEC_RIB_THICKNESS_FACTOR,
};
use crate::sizing::WingboxSizing;

/// A rib whose realized cut is this fraction of its nominal chord or less
/// carries no structure -- it is aerodynamic surface only, and is left out of
/// the mesh entirely.
const AERO_ONLY_CHORD_RATIO: f64 = 0.20;

/// Build the semi-wing wingbox deck -- `build_wing_mesh_bdf`.
///
/// Ribs are generated fresh at the mesh's own resolution (`sizing.num_ribs`
/// stations, `cfg.mesh_chordwise_points` chordwise points), which is generally
/// a finer grid than the sizing pass integrated on. Cap dimensions are
/// therefore re-derived here from the root value and the taper law rather than
/// sampled off the sizing arrays, which are indexed on the other grid.
///
/// # Errors
///
/// [`MeshError`] when a health check finds the mesh corrupt: a triangle with no
/// area, or a grid inboard of the root plane.
#[allow(clippy::too_many_arguments)] // Mirrors the reference's own signature.
pub fn build_wing_mesh_bdf(
    wsg: &WingStructureGeometry,
    sizing: &WingboxSizing,
    cfg: &StructuresConfig,
    engine_cfg: &EngineConfig,
    mass_cfg: &MassModelConfig,
    req: &DesignRequirements,
    skin_mat: &MaterialSpec,
    web_mat: &MaterialSpec,
    cap_mat: &MaterialSpec,
    rib_mat: &MaterialSpec,
) -> Result<(Deck, MeshHealthReport, MeshNodeIndex), MeshError> {
    let mut stations = wsg.get_rib_stations(
        sizing.num_ribs.max(0) as usize,
        cfg.mesh_chordwise_points.max(0) as usize,
    );
    let n_spars = wsg.spar_fracs.len();

    let regions = classify_ribs(wsg, &mut stations);
    let mut warnings = regions.warnings.clone();

    let mut deck = Deck::new();
    for (mid, material) in [
        (MID_SKIN, skin_mat),
        (MID_WEB, web_mat),
        (MID_CAP, cap_mat),
        (MID_RIB, rib_mat),
    ] {
        deck.materials.push(Mat1 {
            mid,
            e: material.e_pa,
            g: material.g_pa(),
            nu: material.nu,
            rho: material.rho_kg_m3,
        });
    }
    for (pid, mid, t) in [
        (PID_SKIN, MID_SKIN, sizing.t_skin),
        (PID_MAIN_RIB, MID_RIB, cfg.t_rib_m),
        (PID_SEC_RIB, MID_RIB, cfg.t_rib_m * SEC_RIB_THICKNESS_FACTOR),
        (PID_TE_STRIP, MID_RIB, cfg.t_te_strip_m),
    ] {
        deck.shell_properties.push(Pshell {
            pid,
            mid1: mid,
            t,
            mid2: mid,
        });
    }
    for (index, spar) in sizing.spars.iter().enumerate().take(n_spars) {
        deck.shell_properties.push(Pshell {
            pid: PID_WEB_BASE + index as i64,
            mid1: MID_WEB,
            t: spar.t_web,
            mid2: MID_WEB,
        });
    }

    let mut nodes = NodeMap::new();
    for (rib, station) in stations.iter().enumerate() {
        for (point, xyz) in station.extrados.iter().enumerate() {
            nodes.place(&mut deck, (rib, point, Surface::Ext), *xyz);
        }
        for (point, xyz) in station.intrados.iter().enumerate() {
            nodes.place(&mut deck, (rib, point, Surface::Int), *xyz);
        }
    }

    // The spar runs through every structural rib, including the truncated ones
    // the skin does not reach: it is the continuous load path, and the skin is
    // rivetted to it later rather than starting where it starts.
    let mut spar_upper: Vec<Vec<i64>> = Vec::with_capacity(n_spars);
    let mut spar_lower: Vec<Vec<i64>> = Vec::with_capacity(n_spars);
    for spar in 0..n_spars {
        let mut upper = Vec::new();
        let mut lower = Vec::new();
        for &rib in &regions.all_structural {
            let Some(&point) = stations[rib].j_spars.get(spar) else {
                continue;
            };
            if point >= 0 {
                upper.push(nodes.at(rib, point as usize, Surface::Ext));
                lower.push(nodes.at(rib, point as usize, Surface::Int));
            }
        }
        spar_upper.push(upper);
        spar_lower.push(lower);
    }

    let mut eid: i64 = 1;
    for pair in regions.skin_ribs.windows(2) {
        for surface in [Surface::Ext, Surface::Int] {
            elements::zipper_skin_strip(&mut deck, &mut eid, &stations, &nodes, pair, surface);
        }
    }
    for (spar, (upper, lower)) in spar_upper.iter().zip(&spar_lower).enumerate() {
        for index in 0..upper.len().saturating_sub(1) {
            elements::add_quad(
                &mut deck,
                &mut eid,
                [
                    upper[index],
                    upper[index + 1],
                    lower[index + 1],
                    lower[index],
                ],
                PID_WEB_BASE + spar as i64,
            );
        }
    }
    elements::add_rib_panels(&mut deck, &mut eid, &stations, &nodes, &regions);
    elements::add_trailing_edge(&mut deck, &mut eid, &stations, &nodes, &regions, cfg, wsg);
    elements::add_spar_caps(&mut deck, &mut eid, &spar_upper, &spar_lower, sizing, wsg);

    let root_nodes = elements::root_constraint_nodes(&stations, &nodes);
    deck.constraints.push(Spc1 {
        sid: 1,
        components: "123456",
        nodes: root_nodes,
    });

    let engine_nids = elements::add_engine_masses(
        &mut deck, &mut eid, &stations, &nodes, &regions, wsg, engine_cfg, mass_cfg, req,
    );

    for (key, value) in [
        ("GRDPNT", ParamValue::Int(0)),
        ("AUTOSPC", ParamValue::Name("YES")),
        ("POST", ParamValue::Int(-1)),
    ] {
        deck.params.push(Param { key, value });
    }

    let rbe3_count = rivets::add_transition_rbe3(
        &mut deck,
        &stations,
        &nodes,
        &regions.skin_ribs_set,
        &regions.transition_ribs,
        wsg.semi_span,
        sizing.num_ribs,
        eid,
    );

    let mut report = health::check_mesh_health(&deck, &stations, &regions, &spar_upper, wsg)?;
    warnings.append(&mut report.warnings);
    report.warnings = warnings;
    report.rbe3_count = rbe3_count;
    report.n_nodes = deck.grids().len();
    report.n_elements = deck.element_count();

    let front_upper = spar_upper.first().cloned().unwrap_or_default();
    let node_index = MeshNodeIndex {
        root_nid: front_upper.first().copied().unwrap_or(0),
        tip_nid: front_upper.last().copied().unwrap_or(0),
        kink_nid: front_upper
            .iter()
            .copied()
            .min_by(|&a, &b| {
                (deck.node_y(a) - wsg.y_break)
                    .abs()
                    .total_cmp(&(deck.node_y(b) - wsg.y_break).abs())
            })
            .unwrap_or(0),
        spar_upper_nids: spar_upper,
        spar_lower_nids: spar_lower,
        engine_nids,
    };

    Ok((deck, report, node_index))
}

/// Which ribs carry skin, which are truncated, and which carry no structure --
/// the classification every element loop below is written against.
pub(super) struct RibRegions {
    /// Ribs the skin panels span, root first.
    pub(super) skin_ribs: Vec<usize>,
    /// The same set, for membership tests.
    pub(super) skin_ribs_set: HashSet<usize>,
    /// Skin ribs from the first full-length one outboard: the root rib is a
    /// skin rib but is not one of these, since it is the constrained cut.
    pub(super) skin_ribs_main: Vec<usize>,
    /// Root-adjacent ribs the root plane truncated, which the rivets tie in.
    pub(super) transition_ribs: Vec<usize>,
    /// Every rib carrying structure, ascending.
    pub(super) all_structural: Vec<usize>,
    /// The aero-only exclusions, phrased as the reference phrases them.
    warnings: Vec<String>,
}

/// Split the stations into skin, transition and aero-only ribs, and set each
/// station's `is_full` flag from the result.
fn classify_ribs(wsg: &WingStructureGeometry, stations: &mut [RibStation]) -> RibRegions {
    let n_ribs = stations.len();
    let te_x_root = stations
        .first()
        .and_then(|station| station.extrados.last())
        .map_or(f64::NAN, |point| point[0]);

    let skin_start = (1..n_ribs)
        .find(|&i| {
            stations[i]
                .extrados
                .last()
                .is_some_and(|point| point[0] >= te_x_root * 0.99)
        })
        .unwrap_or(1);

    let mut aero_only = HashSet::new();
    let mut warnings = Vec::new();
    for (i, station) in stations.iter().enumerate() {
        let reach = match (station.extrados.last(), station.extrados.first()) {
            (Some(last), Some(first)) => {
                let d = [last[0] - first[0], last[1] - first[1], last[2] - first[2]];
                (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
            }
            _ => 0.0,
        };
        let ratio = reach / wsg.local_chord(station.eta).max(1e-9);
        if ratio <= AERO_ONLY_CHORD_RATIO {
            aero_only.insert(i);
            warnings.push(format!(
                "Rib {i} (y={:.2} m) aero-only: {:.1}% nominal chord -- excluded.",
                station.y_station,
                ratio * 100.0
            ));
        }
    }

    let skin_ribs: Vec<usize> = (0..n_ribs)
        .filter(|i| !aero_only.contains(i) && (*i == 0 || *i >= skin_start))
        .collect();
    let transition_ribs: Vec<usize> = (1..skin_start).filter(|i| !aero_only.contains(i)).collect();
    let skin_ribs_set: HashSet<usize> = skin_ribs.iter().copied().collect();

    let mut all_structural: Vec<usize> = skin_ribs
        .iter()
        .chain(&transition_ribs)
        .copied()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    all_structural.sort_unstable();

    for (i, station) in stations.iter_mut().enumerate() {
        station.is_full = !transition_ribs.contains(&i);
    }

    let skin_ribs_main: Vec<usize> = skin_ribs
        .iter()
        .copied()
        .filter(|&i| i >= skin_start)
        .collect();

    RibRegions {
        skin_ribs,
        skin_ribs_set,
        skin_ribs_main,
        transition_ribs,
        all_structural,
        warnings,
    }
}
