// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Print the plausibility rules document: the header and the rule table.

#![allow(clippy::print_stdout)]

const HEADER: &str = r#"# Plausibility rules (task 02)

Generic engineering sanity limits enforced by the automated tier
(`crates/alas-plausibility/src/`, test `plausibility_tier`). None is keyed to an
aircraft or fitted to a preset. Severity: HIGH = physically impossible or clearly absurd for the
class; MEDIUM = outside the usual range, needs a physical explanation; LOW = unusual.
Bands in a statement are "usual / MEDIUM outside / HIGH outside".
Regenerate this table with
`cargo run -p alas-acceptance --example plausibility_rules_md`.

Gate: `plausibility_gate` fails while any HIGH finding is open. Ratchet:
`registered_findings_do_not_worsen` fails on an open finding not listed in
`crates/alas-acceptance/data/plausibility_known.tsv`, on a listed finding further past its
rule's limit than the bound registered there, and on a listed finding that is no longer
reproduced (a fix must delete its line). `no_rule_group_is_skipped` fails when a case could not
be judged by a group of rules.
Knobs: `ALAS_PLAUS_THREADS` (default 4), `ALAS_PLAUS_ONLY=<substring>`, `ALAS_PLAUS_OUT=<jsonl>`.
"#;

fn main() {
    print!(
        "{HEADER}
{}",
        alas_acceptance::plausibility::rules_markdown()
    );
}
