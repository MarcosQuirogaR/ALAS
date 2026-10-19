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

[unsafe-libyaml RUSTSEC-2024-0449](https://rustsec.org/advisories/RUSTSEC-2024-0449)
is ignored as an unmaintained notice, not a vulnerability: it is reached only
through `serde_yaml` (also archived), which parses locally authored configuration
and preset files. The exception is removed when the YAML layer moves to a maintained
parser.

Configuration semantics follow the [cargo-deny documentation](https://embarkstudios.github.io/cargo-deny/checks/index.html).

## Native file dialogs (`rfd`, 2026-10-18)

The desktop GUI opens system file/folder dialogs through
[`rfd`](https://crates.io/crates/rfd) 0.17 (MIT), declared once in the workspace
`Cargo.toml` with default features off and only `xdg-portal` on. That choice
needs no system development package on Linux (the default `gtk3` feature needs
`libgtk-3-dev`): the Linux backend talks to the XDG desktop portal and falls
back to `zenity`/`kdialog` when present, and it builds on a bare `ubuntu-22.04`
runner. On Windows the Win32 common dialogs are used and the feature selection
is inert. The crate adds `rfd`, `pollster` and `block2` to the lockfile;
`cargo deny check` (advisories, bans, licenses, sources) passes with no new
exception. rfd 0.15 was rejected: it pulled `zbus`/`zvariant`, which carry
unmaintained advisories. When no dialog backend is available the Browse button
does nothing and the path text boxes remain usable.
## Add-on dependencies (2026-10-18)

`alas-addons` adds two direct dependencies. [`rhai`](https://rhai.rs) 1.26 is the
embedded scripting engine for sandboxed script add-ons (MIT OR Apache-2.0, pure
Rust, `unsafe` only inside the crate; built with `no_module` and `no_time`, with
no file, network or process function registered). [`toml`](https://crates.io/crates/toml)
0.9 parses the `addon.toml` manifest (MIT OR Apache-2.0, serde support only). Both
pass `cargo deny` with the existing allow-list. See `docs/ADDONS.md`.

[smartstring RUSTSEC-2026-0249](https://rustsec.org/advisories/RUSTSEC-2026-0249)
is ignored as an unmaintained notice, not a vulnerability: the crate is archived
and reached only through `rhai` 1.x (it backs Rhai's immutable strings). The
exception is removed when Rhai drops it.
