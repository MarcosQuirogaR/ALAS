# Dependency policy

Run `cargo deny --locked check` with cargo-deny 0.20.2. CI runs this alongside
the workspace gate, so the local numerical gate does not require network access.
`deny.toml` includes all workspace features and target platforms. It rejects
unapproved license expressions, unknown registries and Git sources, yanked
versions, and vulnerability advisories. Local workspace path dependencies are
allowed without a registry version; the committed lockfile fixes resolution. Duplicate
transitive versions produce warnings because the GUI and SVG stacks resolve
different supported versions.

The initial graph check found two maintenance notices, explicitly recorded in
the configuration: [rustybuzz RUSTSEC-2026-0206](https://rustsec.org/advisories/RUSTSEC-2026-0206)
and [ttf-parser RUSTSEC-2026-0192](https://rustsec.org/advisories/RUSTSEC-2026-0192).
These are retained transitively by the pinned egui/resvg rendering stack.
Replacing that stack needs font, shaping, SVG and screenshot regressions.
The exceptions name those notice IDs only; other advisories still fail, and an
unused exception fails so it must be removed after an upgrade.

This verifies declared dependency metadata, not the legal status of every
embedded asset or external executable. Keep `THIRD-PARTY-NOTICES.md` and the
NASTRAN distribution checks: cargo-deny does not inspect a Fortran bundle.
Font licenses in the graph include OFL and Ubuntu Font License through egui's
default fonts. Preserve their notices in distributions.

Configuration semantics follow the [cargo-deny documentation](https://embarkstudios.github.io/cargo-deny/checks/index.html).
