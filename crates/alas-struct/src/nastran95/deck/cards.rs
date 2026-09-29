// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Fixed-field bulk data: the mesh cards, parameters, loads and the eigenvalue card.

use super::beam_cards::bars;
use super::*;

/// Reformulate every mesh card into fixed-field bulk data.
pub(super) fn bulk(out: &mut String, tags: &mut ContinuationTags, deck: &Deck, dialect: Dialect) {
    params(out, deck, dialect);
    for grid in deck.grids() {
        card(
            out,
            tags,
            "GRID",
            vec![
                Field::Int(grid.nid),
                Field::Blank,
                Field::Real(grid.xyz[0]),
                Field::Real(grid.xyz[1]),
                Field::Real(grid.xyz[2]),
            ],
        );
    }
    for shell in deck.quads().iter().chain(deck.trias()) {
        let name = if shell.nodes.len() == 4 {
            "CQUAD4"
        } else {
            "CTRIA3"
        };
        let mut fields = vec![Field::Int(shell.eid), Field::Int(shell.pid)];
        fields.extend(shell.nodes.iter().map(|&nid| Field::Int(nid)));
        card(out, tags, name, fields);
    }
    bars(out, tags, deck);
    for mass in &deck.masses {
        card(
            out,
            tags,
            "CONM2",
            vec![
                Field::Int(mass.eid),
                Field::Int(mass.nid),
                Field::Int(mass.cid),
                Field::Real(mass.mass),
                Field::Real(mass.offset[0]),
                Field::Real(mass.offset[1]),
                Field::Real(mass.offset[2]),
            ],
        );
    }
    let rbe3_name = match dialect {
        Dialect::Nastran95 => "CRBE3",
        Dialect::Modern => "RBE3",
    };
    for rigid in &deck.rigid_elements {
        // The same layout in both dialects (field 2 blank, then the reference
        // grid and components, the one weight and component group, and the
        // independent grids) emitted small-field so MSC's input processor
        // accepts the continuation the mesh's large field does not survive.
        let mut fields = vec![
            Field::Int(rigid.eid),
            Field::Blank,
            Field::Int(rigid.refgrid),
            Field::Text(rigid.refc),
            Field::Real(rigid.weight),
            Field::Text(rigid.comp),
        ];
        fields.extend(rigid.gijs.iter().map(|&nid| Field::Int(nid)));
        card(out, tags, rbe3_name, fields);
    }
    for property in &deck.shell_properties {
        card(
            out,
            tags,
            "PSHELL",
            vec![
                Field::Int(property.pid),
                Field::Int(property.mid1),
                Field::Real(property.t),
                Field::Int(property.mid2),
            ],
        );
    }
    for material in &deck.materials {
        card(
            out,
            tags,
            "MAT1",
            vec![
                Field::Int(material.mid),
                Field::Real(material.e),
                Field::Real(material.g),
                Field::Real(material.nu),
                Field::Real(material.rho),
            ],
        );
    }
    for constraint in &deck.constraints {
        let mut fields = vec![
            Field::Int(constraint.sid),
            Field::Text(constraint.components),
        ];
        fields.extend(constraint.nodes.iter().map(|&nid| Field::Int(nid)));
        card(out, tags, "SPC1", fields);
    }
}

/// The `PARAM`s the mesh carried, translated to `dialect`.
///
/// `AUTOSPC` is the load-bearing one and its value is where the two dialects
/// disagree: an integer here, `YES` there. `GRDPNT` passes through; `POST`
/// selects an output file neither of these runs needs and is dropped.
fn params(out: &mut String, deck: &Deck, dialect: Dialect) {
    let autospc = match dialect {
        Dialect::Nastran95 => Field::Int(1),
        Dialect::Modern => Field::Text("YES"),
    };
    for card in &deck.params {
        match card.key {
            "AUTOSPC" => param(out, "AUTOSPC", autospc),
            "POST" => {}
            other => param(
                out,
                other,
                match card.value {
                    ParamValue::Int(number) => Field::Int(number),
                    ParamValue::Name(text) => Field::Text(text),
                },
            ),
        }
    }
}

/// The gravity, force and combination cards for every subcase, identical in both
/// dialects.
pub(super) fn static_loads(
    out: &mut String,
    tags: &mut ContinuationTags,
    deck: &Deck,
    node_index: &MeshNodeIndex,
    req: &DesignRequirements,
    cases: &[LoadCase],
) {
    let front_upper: &[i64] = node_index
        .spar_upper_nids
        .first()
        .map_or(&[], |nids| nids.as_slice());
    let semi_span = deck.node_y(node_index.tip_nid);
    let nid_y: Vec<(i64, f64)> = front_upper
        .iter()
        .map(|&nid| (nid, deck.node_y(nid)))
        .collect();

    for (index, case) in cases.iter().enumerate() {
        let sid = index as i64 + 1;
        let gravity_sid = GRAVITY_SID_BASE + sid;
        let force_sid = FORCE_SID_BASE + sid;
        let gravity_magnitude = case.load_factor.abs() * req.gravity_m_s2;
        let lift_sign = if case.total_force_n >= 0.0 { 1.0 } else { -1.0 };
        let gravity_sign: f64 = if case.load_factor >= 0.0 { -1.0 } else { 1.0 };

        card(
            out,
            tags,
            "LOAD",
            vec![
                Field::Int(sid),
                Field::Real(1.0),
                Field::Real(1.0),
                Field::Int(gravity_sid),
                Field::Real(1.0),
                Field::Int(force_sid),
            ],
        );
        card(
            out,
            tags,
            "GRAV",
            vec![
                Field::Int(gravity_sid),
                Field::Int(0),
                Field::Real(gravity_magnitude),
                Field::Real(0.0),
                Field::Real(0.0),
                Field::Real(gravity_sign),
            ],
        );
        for (nid, force) in elliptic_forces_by_y(&nid_y, case.total_force_n.abs(), semi_span) {
            card(
                out,
                tags,
                "FORCE",
                vec![
                    Field::Int(force_sid),
                    Field::Int(nid),
                    Field::Int(0),
                    Field::Real(force.abs() * lift_sign),
                    Field::Real(0.0),
                    Field::Real(0.0),
                    Field::Real(1.0),
                ],
            );
        }
    }
}

/// The eigenvalue-extraction card each dialect uses to ask for the lowest
/// `n_modes`.
///
/// `EIGRL,,,,N` asks the modern solver for the lowest `N` directly. NASTRAN-95's
/// Givens methods need a positive-definite mass matrix, which a shell-and-mass
/// model does not have, so it uses `EIGR,,INV` over a bounded low-frequency
/// interval.  `NE` is an estimate of the roots in that interval, not the
/// requested output count: supplying `n_modes` for both while retaining the
/// 500 Hz SOL 111 response ceiling made NASTRAN-95 skip the elastic roots.
/// Extracting at least sixteen modes with a four-to-one root estimate gives the
/// historic solver enough shifts to return the requested band. Its broad
/// inverse-power search can still print message 3307 for an intermediate shift;
/// callers must use the final Sturm `ROOTS BELOW` count to establish that no
/// lower emitted root was omitted. The continuation card is required by the
/// `EIGR` format.
pub(super) fn eigenvalue_card(
    out: &mut String,
    tags: &mut ContinuationTags,
    cfg: &StructuresConfig,
    dialect: Dialect,
) {
    let n_modes = cfg.n_modes.max(1);
    let local_extract_count = n_modes.max(NASTRAN95_MINIMUM_EXTRACTED_MODES);
    let local_root_estimate =
        local_extract_count.saturating_mul(NASTRAN95_MODAL_ROOT_ESTIMATE_FACTOR);
    let local_upper_hz = NASTRAN95_MODAL_SEARCH_UPPER_HZ;
    match dialect {
        Dialect::Modern => card(
            out,
            tags,
            "EIGRL",
            vec![
                Field::Int(METHOD_SET),
                Field::Blank,
                Field::Blank,
                Field::Int(n_modes),
            ],
        ),
        Dialect::Nastran95 => card(
            out,
            tags,
            "EIGR",
            vec![
                Field::Int(METHOD_SET),
                Field::Text("INV"),
                Field::Real(0.0),
                Field::Real(local_upper_hz),
                Field::Int(local_root_estimate),
                Field::Int(local_extract_count),
                Field::Blank,
                Field::Blank,
                Field::Text("MASS"),
            ],
        ),
    }
}

/// One `PARAM` card in fixed field.
pub(super) fn param(out: &mut String, key: &'static str, value: Field) {
    let mut tags = ContinuationTags::new();
    Card::new("PARAM", vec![Field::Text(key), value]).render(out, &mut tags);
}

/// Render one card into `out`.
pub(super) fn card(
    out: &mut String,
    tags: &mut ContinuationTags,
    name: &'static str,
    fields: Vec<Field>,
) {
    Card::new(name, fields).render(out, tags);
}
