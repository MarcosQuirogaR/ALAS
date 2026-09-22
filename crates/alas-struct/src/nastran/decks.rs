// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The four solution decks, each an executive and case-control section over a
//! shared mesh include.
//!
//! Every one of them is written by hand, card by card, rather than assembled
//! through a model object, which is what the mesh does, because case
//! control is not bulk data and there is nothing to assemble. That makes these
//! decks text, and text is what the parity test compares them as.
//!
//! The mesh is referenced rather than repeated: each deck `INCLUDE`s the one
//! `wing_mesh.bdf` written above it, so four solutions share one mesh and a
//! solver reads the same grids in all four.

use alas_config::{DesignRequirements, StructuresConfig};

use super::format::{free_field, python_str};
use super::{monitor_set, MonitorNodes};
use crate::loads::{self, LoadCase};
use crate::mesh::{Deck, MeshNodeIndex};
use crate::sizing::gradient_unit;

/// The static deck's constraint set, shared by all four solutions: the mesh
/// writes exactly one `SPC1`, and it is set 1.
const SPC_SET: i64 = 1;

/// Load-set identifier bases. The three kinds never share a range, which an
/// earlier `sid * 10` / `sid * 20` scheme did not manage: pull-up's force set
/// collided with push-down's gravity set, and the two cards' loads would have
/// merged silently under one identifier.
const GRAVITY_SID_BASE: i64 = 100;
const FORCE_SID_BASE: i64 = 200;

/// Distribute `total_force_n` over `nid_y` with the same half-ellipse
/// [`crate::loads::elliptic_distributed_load`] uses, discretized onto the
/// mesh's own front-spar node line: `_elliptic_forces_by_y`.
///
/// The node line is not evenly spaced, so each node's share is weighted by the
/// span it stands for as well as by the ellipse. A distribution that summed to
/// nothing (every node at the tip, where the ellipse is zero) falls back to
/// an equal split rather than dividing by it.
pub fn elliptic_forces_by_y(
    nid_y: &[(i64, f64)],
    total_force_n: f64,
    semi_span: f64,
) -> Vec<(i64, f64)> {
    let spans: Vec<f64> = nid_y.iter().map(|&(_, y)| y).collect();
    let ellipse: Vec<f64> = spans
        .iter()
        .map(|&y| {
            (1.0 - (y / semi_span.max(1e-9)).powi(2))
                .clamp(0.0, 1.0)
                .sqrt()
        })
        .collect();
    let widths: Vec<f64> = if spans.len() > 1 {
        gradient_unit(&spans)
            .iter()
            .map(|value| value.abs())
            .collect()
    } else {
        vec![1.0]
    };

    let mut weights: Vec<f64> = ellipse
        .iter()
        .zip(&widths)
        .map(|(&q, &width)| q * width)
        .collect();
    let mut total: f64 = weights.iter().sum();
    if total < 1e-15 {
        weights = vec![1.0; ellipse.len()];
        total = weights.iter().sum();
    }
    nid_y
        .iter()
        .zip(&weights)
        .map(|(&(nid, _), &weight)| (nid, weight / total * total_force_n))
        .collect()
}

/// SOL 101, linear static: one subcase per design load case, each combining the
/// structure's own inertial relief with the aerodynamic lift that balances it:
/// `build_sol101_bulk`.
pub fn build_sol101_bulk(
    deck: &Deck,
    node_index: &MeshNodeIndex,
    req: &DesignRequirements,
    cfg: &StructuresConfig,
    mesh_include: &str,
) -> String {
    let front_upper: &[i64] = node_index
        .spar_upper_nids
        .first()
        .map_or(&[], |nids| nids.as_slice());
    let semi_span = deck.node_y(node_index.tip_nid);
    let nid_y: Vec<(i64, f64)> = front_upper
        .iter()
        .map(|&nid| (nid, deck.node_y(nid)))
        .collect();

    let cases = loads::load_cases(req, cfg.additional_safety_factor);

    let mut out = vec![
        "SOL 101".to_string(),
        "CEND".to_string(),
        "$".to_string(),
        "TITLE = ALAS Wingbox -- Static Analysis".to_string(),
        "ECHO = NONE".to_string(),
        "$".to_string(),
    ];
    for (index, case) in cases.iter().enumerate() {
        let sid = index as i64 + 1;
        out.push(format!("SUBCASE {sid}"));
        out.push(format!(
            "  TITLE = {} (n={:+.2})",
            case.name, case.load_factor
        ));
        out.push(format!("  LOAD = {sid}"));
        out.push(format!("  SPC = {SPC_SET}"));
        out.push("  STRESS(VONMISES,CORNER) = ALL".to_string());
        out.push("  DISPLACEMENT = ALL".to_string());
        out.push("  SPCFORCE = ALL".to_string());
        out.push("  OLOAD = ALL".to_string());
        out.push("$".to_string());
    }
    out.push("BEGIN BULK".to_string());
    out.push("PARAM,COUPMASS,1".to_string());
    out.push(format!("INCLUDE '{mesh_include}'"));
    out.push("$".to_string());

    for (index, case) in cases.iter().enumerate() {
        let sid = index as i64 + 1;
        let gravity_sid = GRAVITY_SID_BASE + sid;
        let force_sid = FORCE_SID_BASE + sid;
        out.extend(load_case_cards(
            case,
            sid,
            gravity_sid,
            force_sid,
            req,
            &nid_y,
            semi_span,
        ));
    }

    out.push("ENDDATA".to_string());
    out.join("\n")
}

/// The gravity and force cards for one load case.
///
/// The two point opposite ways by construction. Lift acts along the case's own
/// signed total force; the structure's inertial reaction always opposes the
/// load factor's sign, because a positive `n` means the airframe is
/// accelerating upward and the equivalent body force points down.
fn load_case_cards(
    case: &LoadCase,
    sid: i64,
    gravity_sid: i64,
    force_sid: i64,
    req: &DesignRequirements,
    nid_y: &[(i64, f64)],
    semi_span: f64,
) -> Vec<String> {
    let gravity_magnitude = case.load_factor.abs() * req.gravity_m_s2;
    let lift_sign = if case.total_force_n >= 0.0 { 1.0 } else { -1.0 };
    let gravity_sign: f64 = if case.load_factor >= 0.0 { -1.0 } else { 1.0 };

    let mut out = vec![
        format!("$ ---- {}: n={:+.2} ----", case.name, case.load_factor),
        format!("LOAD,{sid},1.0,1.0,{gravity_sid},1.0,{force_sid}"),
        format!(
            "GRAV,{gravity_sid},0,{},0.,0.,{}",
            free_field(gravity_magnitude),
            free_field(gravity_sign)
        ),
    ];
    for (nid, force) in elliptic_forces_by_y(nid_y, case.total_force_n.abs(), semi_span) {
        out.push(format!(
            "FORCE,{force_sid},{nid},0,{},0.,0.,1.",
            free_field(force.abs() * lift_sign)
        ));
    }
    out.push("$".to_string());
    out
}

/// SOL 103, normal modes: `build_sol103_bulk`.
pub fn build_sol103_bulk(cfg: &StructuresConfig, mesh_include: &str) -> String {
    [
        "SOL 103".to_string(),
        "CEND".to_string(),
        "$".to_string(),
        "TITLE = ALAS Wingbox -- Normal Modes".to_string(),
        "ECHO = NONE".to_string(),
        "$".to_string(),
        "SUBCASE 1".to_string(),
        "  TITLE = Normal modes extraction".to_string(),
        format!("  SPC = {SPC_SET}"),
        "  METHOD = 1".to_string(),
        "  MEFFMASS(PRINT,SUMMARY) = YES".to_string(),
        "  RESVEC = YES".to_string(),
        "  DISPLACEMENT = ALL".to_string(),
        "  SPCFORCE = ALL".to_string(),
        "$".to_string(),
        "BEGIN BULK".to_string(),
        "PARAM,COUPMASS,1".to_string(),
        format!("INCLUDE '{mesh_include}'"),
        "$".to_string(),
        format!("EIGRL,1,,,{}", cfg.n_modes),
        "$".to_string(),
        "ENDDATA".to_string(),
    ]
    .join("\n")
}

/// The bulk cards both SOL 111 decks share: the modal basis, the frequency
/// sweep, the damping table, and the unit harmonic force: `_dynamic_bulk`.
fn dynamic_bulk(
    cfg: &StructuresConfig,
    excitation_nid: i64,
    include_random: bool,
    msc_product: bool,
) -> Vec<String> {
    let step_hz = cfg.freq_step_hz.max(1e-6);
    // FREQ1's NDF is the number of increments after F1, not the total number
    // of points. The frozen reference used max/step and therefore overshoots
    // the requested upper limit by one increment. Keep that text for parity,
    // but correct the installed product deck.
    let frequency_increments = if msc_product {
        (((cfg.freq_sweep_max_hz - step_hz) / step_hz).floor() as i64).max(0)
    } else {
        ((cfg.freq_sweep_max_hz / step_hz) as i64).max(1)
    };
    let frequency_points = frequency_increments + 1;
    let damping = free_field(cfg.modal_damping_ratio);
    let sweep_max = free_field(cfg.freq_sweep_max_hz);
    // Constant damping has no physical cutoff. Cover all aircraft modes rather
    // than asking MSC to extrapolate beyond the response sweep and issuing
    // warning 7697 for the retained high-frequency modal basis.
    let damping_table_max = if msc_product {
        free_field(1.0e9)
    } else {
        sweep_max.clone()
    };

    let mut out = vec![
        "$ ---- Modal extraction ----".to_string(),
        format!("EIGRL,1,,,{}", cfg.n_modes),
        "$".to_string(),
        if msc_product {
            format!(
                "$ ---- Frequency range: {} to {} Hz ({frequency_points} points) ----",
                python_str(cfg.freq_step_hz),
                python_str(cfg.freq_sweep_max_hz)
            )
        } else {
            format!(
                "$ ---- Frequency range: {} to {} Hz ({frequency_increments} steps) ----",
                python_str(cfg.freq_step_hz),
                python_str(cfg.freq_sweep_max_hz)
            )
        },
        if frequency_increments == 0 {
            format!("FREQ,3,{}", free_field(step_hz))
        } else {
            format!(
                "FREQ1,3,{},{},{frequency_increments}",
                free_field(step_hz),
                free_field(step_hz)
            )
        },
        "$".to_string(),
        format!(
            "$ ---- Damping: {:.0}% critical ----",
            cfg.modal_damping_ratio * 100.0
        ),
        "TABDMP1,4,CRIT".to_string(),
        format!(
            "+,{},{damping},{damping_table_max},{damping},ENDT",
            free_field(0.0)
        ),
        "$".to_string(),
        "$ ---- Unit-amplitude TABLED1 ----".to_string(),
        "TABLED1,200,LINEAR,LINEAR".to_string(),
        format!(
            "+,{},{},{sweep_max},{},ENDT",
            free_field(0.0),
            free_field(1.0),
            free_field(1.0)
        ),
        "$".to_string(),
        format!("$ ---- DAREA: unit force (1 N) in Z at excitation node {excitation_nid} ----"),
        format!("DAREA,1000,{excitation_nid},3,1."),
        "$".to_string(),
        "RLOAD1,100,1000,0.,0.,200,0.,LOAD".to_string(),
        "$".to_string(),
        "DLOAD,300,1.,1.,100".to_string(),
        "$".to_string(),
    ];

    if include_random {
        let psd = cfg.psd_base_g2_per_hz * GRAVITY_G.powi(2);
        out.push(format!(
            "$ ---- Random PSD: {} g^2/Hz = {psd:.4} (m/s^2)^2/Hz ----",
            python_str(cfg.psd_base_g2_per_hz)
        ));
        out.push("TABRND1,500,LOG,LOG".to_string());
        out.push(format!(
            "+,{},{},{sweep_max},{},ENDT",
            free_field(0.0),
            free_field(psd),
            free_field(psd)
        ));
        out.push("$".to_string());
        out.push("RANDPS,600,100,100,1.,0.,500".to_string());
        out.push("$".to_string());
    }
    out
}

/// The standard gravity the excitation spectrum is quoted against.
///
/// The reference writes `9.81` here rather than reading `DesignRequirements`,
/// so this is the same literal and not `req.gravity_m_s2`: a configuration that
/// changed gravity would not move this deck's spectrum upstream either.
const GRAVITY_G: f64 = 9.81;

/// SOL 111 driven by a swept unit harmonic force: `build_sol111_sine_bulk`.
pub fn build_sol111_sine_bulk(
    cfg: &StructuresConfig,
    node_index: &MeshNodeIndex,
    mesh_include: &str,
) -> String {
    let monitors = monitor_set(node_index);
    let mut out = case_control(
        "ALAS Wingbox -- Sine Sweep",
        "Modal frequency response -- unit harmonic force at excitation node",
        &monitors,
        mesh_include,
        &[
            "  DISPLACEMENT(PHASE,SORT2) = 9000",
            "  ACCELERATION(PHASE,SORT2) = 9000",
            "  STRESS(SORT2,PHASE) = ALL",
        ],
        None,
    );
    out.extend(dynamic_bulk(cfg, monitors.engine, false, false));
    out.push("ENDDATA".to_string());
    out.join("\n")
}

/// MSC-compatible SOL 111 harmonic-response deck.
///
/// This is deliberately separate from [`build_sol111_sine_bulk`]: the latter
/// is frozen Python parity, while this product path applies corrections proven
/// against MSC's F06 diagnostics.
pub fn build_sol111_sine_bulk_msc(
    cfg: &StructuresConfig,
    node_index: &MeshNodeIndex,
    mesh_include: &str,
) -> String {
    let monitors = monitor_set(node_index);
    let mut out = case_control(
        "ALAS Wingbox -- Sine Sweep",
        "Modal frequency response -- unit harmonic force at excitation node",
        &monitors,
        mesh_include,
        // The postprocessor reads only these four monitor displacements to
        // form the force-PSD RMS response. Full stress and acceleration
        // output at every frequency creates large F06/OP2/scratch files but
        // contributes nothing to this calculation.
        &["  DISPLACEMENT(PHASE,SORT2) = 9000"],
        None,
    );
    out.extend(dynamic_bulk(cfg, monitors.engine, false, true));
    out.push("ENDDATA".to_string());
    out.join("\n")
}

/// SOL 111 driven by a white-noise spectrum: `build_sol111_random_bulk`.
pub fn build_sol111_random_bulk(
    cfg: &StructuresConfig,
    node_index: &MeshNodeIndex,
    mesh_include: &str,
) -> String {
    let monitors = monitor_set(node_index);
    let mut out = case_control(
        "ALAS Wingbox -- Random Vibration",
        "Random response -- white noise excitation",
        &monitors,
        mesh_include,
        &[
            "  DISPLACEMENT(SORT2,PSDF,CRMS) = 9000",
            "  ACCELERATION(SORT2,PSDF,CRMS) = 9000",
            "  STRESS(SORT2,CRMS) = ALL",
        ],
        Some("  RANDOM = 600"),
    );
    out.extend(dynamic_bulk(cfg, monitors.engine, true, false));
    out.push("ENDDATA".to_string());
    out.join("\n")
}

/// The executive and case-control section the two SOL 111 decks share.
fn case_control(
    title: &str,
    subcase_title: &str,
    monitors: &MonitorNodes,
    mesh_include: &str,
    outputs: &[&str],
    random: Option<&str>,
) -> Vec<String> {
    let mut ids: Vec<i64> = monitors.all();
    ids.sort_unstable();
    ids.dedup();
    let monitor_ids: Vec<String> = ids.iter().map(i64::to_string).collect();

    let mut out = vec![
        "SOL 111".to_string(),
        "CEND".to_string(),
        "$".to_string(),
        format!("TITLE = {title}"),
        "ECHO = NONE".to_string(),
        "$".to_string(),
        format!("SET 9000 = {}", monitor_ids.join(", ")),
        "$".to_string(),
        "SUBCASE 1".to_string(),
        format!("  TITLE = {subcase_title}"),
        format!("  SPC = {SPC_SET}"),
        "  METHOD = 1".to_string(),
        "  MEFFMASS(PRINT,SUMMARY) = YES".to_string(),
        "  RESVEC = YES".to_string(),
        "  FREQUENCY = 3".to_string(),
        "  SDAMP = 4".to_string(),
        "  DLOAD = 300".to_string(),
    ];
    if let Some(card) = random {
        out.push(card.to_string());
    }
    out.extend(outputs.iter().map(|line| (*line).to_string()));
    out.push("$".to_string());
    out.push("BEGIN BULK".to_string());
    out.push("PARAM,COUPMASS,1".to_string());
    out.push(format!("INCLUDE '{mesh_include}'"));
    out.push("$".to_string());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node_index() -> MeshNodeIndex {
        MeshNodeIndex {
            root_nid: 6,
            tip_nid: 536,
            kink_nid: 136,
            spar_upper_nids: Vec::new(),
            spar_lower_nids: Vec::new(),
            engine_nids: vec![194],
        }
    }

    #[test]
    fn the_msc_sweep_stops_at_the_configured_upper_frequency() {
        let config = StructuresConfig {
            freq_step_hz: 2.0,
            freq_sweep_max_hz: 55.0,
            ..StructuresConfig::default()
        };
        let frozen = build_sol111_sine_bulk(&config, &node_index(), "../wing_mesh.bdf");
        let product = build_sol111_sine_bulk_msc(&config, &node_index(), "../wing_mesh.bdf");

        assert!(frozen.contains("FREQ1,3,2.,2.,27"));
        assert!(frozen.contains("Frequency range: 2.0 to 55.0 Hz (27 steps)"));
        assert!(product.contains("FREQ1,3,2.,2.,26"));
        assert!(product.contains("Frequency range: 2.0 to 55.0 Hz (27 points)"));
        assert!(product.contains(&format!("+,0.,0.02,{},0.02,ENDT", free_field(1.0e9))));
        assert!(product.contains("DISPLACEMENT(PHASE,SORT2) = 9000"));
        assert!(!product.contains("ACCELERATION(PHASE,SORT2)"));
        assert!(!product.contains("STRESS(SORT2,PHASE)"));
    }

    #[test]
    fn a_one_point_product_sweep_uses_a_frequency_card() {
        let config = StructuresConfig {
            freq_step_hz: 10.0,
            freq_sweep_max_hz: 10.0,
            ..StructuresConfig::default()
        };
        let product = build_sol111_sine_bulk_msc(&config, &node_index(), "../wing_mesh.bdf");

        assert!(product.contains("FREQ,3,10."));
        assert!(!product.contains("FREQ1,3"));
    }
}
