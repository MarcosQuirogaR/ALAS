// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


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
fn eigenvalue_card(
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
fn param(out: &mut String, key: &'static str, value: Field) {
    let mut tags = ContinuationTags::new();
    Card::new("PARAM", vec![Field::Text(key), value]).render(out, &mut tags);
}

/// Render one card into `out`.
fn card(out: &mut String, tags: &mut ContinuationTags, name: &'static str, fields: Vec<Field>) {
    Card::new(name, fields).render(out, tags);
}

#[cfg(test)]
mod tests {
    use super::{build_modes_deck, build_modes_deck_for_nodes, time_limit, Dialect};
    use crate::mesh::Deck;
    use alas_config::StructuresConfig;

    #[test]
    fn legacy_time_card_tracks_the_per_solution_timeout_in_whole_minutes() {
        let mut card = String::new();
        time_limit(&mut card, 121.0);
        assert_eq!(card, "TIME 3\n");
    }

    #[test]
    fn local_modes_use_a_bounded_search_with_a_sufficient_root_estimate() {
        let config = StructuresConfig {
            n_modes: 6,
            ..StructuresConfig::default()
        };
        let deck = build_modes_deck(&Deck::default(), &config, Dialect::Nastran95);
        assert!(deck.contains("EIGR    1       INV     0.      100.    64      16"));
    }

    #[test]
    fn local_modes_preserve_a_higher_explicit_request() {
        let config = StructuresConfig {
            n_modes: 30,
            ..StructuresConfig::default()
        };
        let deck = build_modes_deck(&Deck::default(), &config, Dialect::Nastran95);
        assert!(deck.contains("EIGR    1       INV     0.      100.    120     30"));
    }

    #[test]
    fn local_modes_can_limit_eigenvector_printing_to_required_nodes() {
        let nodes: Vec<i64> = (1..=40).map(|value| value * 10_000).collect();
        let deck = build_modes_deck_for_nodes(
            &Deck::default(),
            &StructuresConfig::default(),
            Dialect::Nastran95,
            &nodes,
        );
        assert!(deck.contains("  SET 9500 = "));
        assert!(deck.contains("  DISPLACEMENT = 9500\n"));
        assert!(!deck.contains("  DISPLACEMENT = ALL\n"));
        for line in deck
            .lines()
            .skip_while(|line| !line.starts_with("  SET 9500"))
        {
            if line == "  DISPLACEMENT = 9500" {
                break;
            }
            assert!(
                line.len() <= 72,
                "case-control line exceeds 72 columns: {line}"
            );
        }
    }
}
