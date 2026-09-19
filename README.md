# ALAS

Aircraft Layout, Analysis and Sizing — a conceptual design tool for transport
aircraft, as a single native executable.

A design vector goes in. A trimmed, mass-balanced aircraft comes out, with a
flown mission, a drag build-up, a sized wingbox and the figures to read them
by.

This is a reimplementation in Rust of the Python version of ALAS, and it is
being built one module at a time against numbers that version produces.
Nothing here is trusted because it looks right; it is trusted because it agrees
with a reference to a stated tolerance, and because the disagreement it does
have is written down.

---

## State

Running, not finished. `cargo run --bin ALAS` launches the desktop interface,
and the full pipeline — geometry, mass/CG, mission, drag build-up, wingbox
sizing, figures — runs end to end for hand-built and CPACS-imported aircraft,
against eight reference presets and the external solvers that are installed.
None of that means it is trustworthy yet: no preset currently has a verified
design mission, and several open defects are tracked in `docs/STATUS.md`.

`docs/STATUS.md` is the authoritative answer to "does it work and what is
wrong with it". `docs/PORTING.md` answers a narrower, still-important
question — whether a given module has been checked against the Python
reference to a stated tolerance, and what licence its content carries — which
matters for the physics kernels but no longer describes the project as a
whole, since orchestration layers like `alas-pipeline` and `alas-gui` were
written natively rather than translated.

```
cargo test          # everything, including the parity tests
cargo xtask gate    # what has to pass before a commit
```

---

## Layout

| Path | What is in it |
|---|---|
| `crates/` | The workspace. One crate per discipline; `docs/ARCHITECTURE.md` explains the layering. |
| `golden/` | Reference values generated from the Python implementation, and the generators that produce them. |
| `xtask/` | Repository checks and the backup task. `cargo xtask` lists them. |
| `docs/` | Architecture, current project status, the parity/provenance ledger, and the methods the models come from. |

---

## Running the fixture generators

The generators read the Python implementation; they are only needed when a
fixture has to be regenerated, not to build or test this program.

```
$ALAS/.suave-venv/Scripts/python golden/generators/gen_units.py
```

Fixtures record the commit of the reference implementation they came from, and
the generators refuse to run against a dirty working tree — a fixture that
cannot be reproduced is not evidence of anything.

---

## Licence

AGPL-3.0-or-later. See `LICENSE`, and `NOTICE` for what that means here.

Parts of this program are translated from AeroSandbox (MIT), NeuralFoil (MIT)
and SUAVE (LGPL-2.1). A translation is a derivative work, so those licences
follow their code into this tree; every translated file names its origin in a
provenance header, and `THIRD-PARTY-NOTICES.md` lists the components one by
one.

External solvers — MSES, MSC Nastran, NASTRAN-95, AVL — are separate programs
supplied by the user. Every analysis that needs one reports itself unavailable
when it is absent, rather than substituting an approximation without saying so.
