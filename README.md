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

Early. The foundations are in place and the process that governs the rest of
the port is working end to end; the physics has not been translated yet.

`docs/PORTING.md` is the authoritative answer to "what is done" — one row per
module of the Python implementation, with where it goes, what licence its
content carries, and whether it has been shown to agree.

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
| `docs/` | Architecture, the porting ledger, and the methods the models come from. |

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
