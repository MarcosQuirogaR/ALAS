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

Two further entries date from 2026-09-11. `CC0-1.0` is allowed: the desktop
shell's move from the Glow to the wgpu painter brings in naga, wgpu's shader
compiler, whose `hexf-parse` dependency is a public-domain dedication; CC0 is
compatible with the program's AGPL-3.0-or-later licence and carries no notice
obligation. [paste RUSTSEC-2024-0436](https://rustsec.org/advisories/RUSTSEC-2024-0436)
is ignored: `paste` is an archived proc-macro crate reached through `faer`'s
`gemm` kernels, runs only at compile time, and ships no code in the binary; the
exception is removed when faer drops it.

Configuration semantics follow the [cargo-deny documentation](https://embarkstudios.github.io/cargo-deny/checks/index.html).
