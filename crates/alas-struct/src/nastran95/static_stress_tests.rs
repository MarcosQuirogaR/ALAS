// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Strict stress identity/coverage tests and elementary plane-stress invariants.
// Assertions intentionally stop immediately on fixture errors.
#![allow(clippy::unwrap_used)]

use super::*;
use crate::mesh::{Mat1, MeshNodeIndex, Pshell, Shell};
use alas_config::{DesignRequirements, StructuresConfig};

fn fixture() -> (Deck, Vec<LoadCase>, String) {
    let mut deck = Deck::new();
    deck.materials.push(Mat1 {
        mid: 7,
        e: 70e9,
        g: 27e9,
        nu: 0.3,
        rho: 2700.0,
    });
    deck.shell_properties.push(Pshell {
        pid: 9,
        mid1: 7,
        mid2: 7,
        t: 0.006,
    });
    deck.quads.push(Shell {
        eid: 11,
        pid: 9,
        nodes: vec![1, 2, 3, 4],
    });
    deck.trias.push(Shell {
        eid: 12,
        pid: 9,
        nodes: vec![1, 2, 3],
    });
    let cases = crate::loads::load_cases(&DesignRequirements::default(), 1.0).to_vec();
    let mut print = String::new();
    for sid in [3, 1, 2] {
        for (id, kind) in [(11, "C Q U A D 4"), (12, "C T R I A 3")] {
            print.push_str(&format!(
                "0 LABEL SUBCASE {sid}\n S T R E S S E S  ({kind})\n"
            ));
            print.push_str(&format!(
                "0 {id} -3.000000E-03 3.000000E+08 0.0 0.0 0.0 3e8 0.0 1.5e8\n"
            ));
            print.push_str(" 3.000000E-03 0.0 0.0 1.000000E+08 45.0 1e8 -1e8 1e8\n");
        }
    }
    (deck, cases, print)
}

#[test]
fn printed_components_produce_plane_stress_invariants_and_real_material_identity() {
    assert_eq!(von_mises(3e8, 0.0, 0.0), 3e8);
    assert_eq!(von_mises(3e8, 3e8, 0.0), 3e8);
    assert!((von_mises(0.0, 0.0, 1e8) - 3.0_f64.sqrt() * 1e8).abs() < 1e-7);
    let (deck, cases, print) = fixture();
    let result = read_static_shell_stress_print(&print, &deck, &cases);
    assert_eq!(result.error, None);
    assert_eq!(result.case_identity, StaticCaseIdentity::ExplicitSubcaseIds);
    assert_eq!(result.cases.len(), 3);
    assert_eq!(result.cases[0].subcase_id, 1);
    assert_eq!(result.cases[0].samples.len(), 4);
    let sample = &result.cases[0].samples[0];
    assert_eq!(
        (sample.element_id, sample.property_id, sample.material_id),
        (11, 9, 7)
    );
    assert_eq!(sample.von_mises_pa, 3e8); // Not printed maximum shear 1.5e8.
}

#[test]
fn incomplete_duplicate_unknown_nonfinite_or_ambiguous_stresses_fail_closed() {
    let (deck, cases, print) = fixture();
    for broken in [
        print.replace("SUBCASE 3", "SUBCASE 9"),
        print.replace("SUBCASE 1", "NO ID"),
        print.replace("0 11 -3.000000E-03", "0 99 -3.000000E-03"),
        print.replace("3.000000E+08", "NaN"),
        print.replace(" 3.000000E-03 0.0 0.0 1.000000E+08 45.0 1e8 -1e8 1e8\n", ""),
        format!("{print}{print}"),
    ] {
        let result = read_static_shell_stress_print(&broken, &deck, &cases);
        assert!(result.error.is_some(), "{broken}");
        assert!(result.cases.is_empty());
    }
    let mut missing_shell = deck.clone();
    missing_shell.quads.push(Shell {
        eid: 13,
        pid: 9,
        nodes: vec![1, 2, 3, 4],
    });
    assert!(
        read_static_shell_stress_print(&print, &missing_shell, &cases)
            .error
            .is_some()
    );
    let mut unsupported = deck;
    unsupported.shell_properties[0].mid2 = 8;
    assert!(read_static_shell_stress_print(&print, &unsupported, &cases)
        .error
        .is_some());
}

#[test]
fn product_stress_request_changes_only_case_control_output() {
    let (mut deck, _, _) = fixture();
    for id in 1..=4 {
        deck.add_grid(id, [0.0, id as f64, 0.0]);
    }
    let nodes = MeshNodeIndex {
        root_nid: 1,
        tip_nid: 4,
        kink_nid: 2,
        spar_upper_nids: vec![vec![1, 4]],
        spar_lower_nids: vec![],
        engine_nids: vec![],
    };
    let req = DesignRequirements::default();
    let cfg = StructuresConfig::default();
    for dialect in [
        super::super::Dialect::Nastran95,
        super::super::Dialect::Modern,
    ] {
        let reference = super::super::build_static_deck(&deck, &nodes, &req, &cfg, dialect);
        let product = super::super::build_static_deck_product(&deck, &nodes, &req, &cfg, dialect);
        assert_eq!(product.matches("  STRESS = ALL\n").count(), 1);
        assert_eq!(product.replace("  STRESS = ALL\n", ""), reference);
    }
}
