# Running and checking ALAS

Written for someone who has not used Rust before. You do not need to learn the
language to check that this program is correct — the tooling is one command,
and the interesting output is in English.

---

## First, the one-time setup

Already done on this machine. If you ever move to another one:

1. Install Visual Studio Build Tools with the "Desktop development with C++"
   workload. This provides the linker; no C++ of ours is compiled.
2. Install Rust from <https://rustup.rs>.
3. Open a **new** terminal, so that `cargo` is on the path.

`cargo` is Rust's build tool, test runner and package manager in one. It is the
only command you need.

> If `cargo` is not recognised in a terminal, it is a stale path. Either open a
> new terminal, or prefix the session once with:
> ```powershell
> $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
> ```

---

## The three commands that matter

Run these from `C:\Proyectos\ALAS-native`.

```powershell
cargo test              # check that everything still agrees with Python
cargo xtask gate        # the full check: formatting, lints, tests, conventions
cargo run               # start the application (once it exists)
```

The first time each runs it will compile a lot and take a few minutes.
Afterwards it only rebuilds what changed, and takes seconds.

---

## Checking that the migration went well

This is the question the whole project is arranged to answer, and `cargo test`
is how you ask it.

Every translated module has a **parity test**: it loads numbers the Python
implementation produced, runs the Rust, and compares them. A module is not
considered done until its parity test passes.

```powershell
cargo test
```

The last line of each block is what you read:

```
test result: ok. 36 passed; 0 failed; 0 ignored
```

`0 failed` means every translated module still agrees with Python to its stated
tolerance. That is the whole claim.

### When something disagrees

A failure prints the disagreement rather than just saying "failed":

```
alas-units disagrees with the reference implementation
(2 of 32 values outside tier `closed`):
  ft: got 3.04800000000000004e-1, reference 3.04799999999999993e-1 (relative 3.6e-17)
  psi: got 6.89475729316836e3, reference 6.89475729316835e3 (relative 1.4e-15)
```

It lists **every** value that disagreed, not the first, because the pattern is
the diagnosis: one bad value is a boundary case, all of them wrong by the same
ratio is a unit error, all of them wrong by a growing amount is an accumulating
index error.

You do not have to diagnose it yourself. Copy the block into a session and ask
— that is what the `parity-diagnostician` agent exists for. The one thing not
to accept is a fix that widens the tolerance until the test passes; the tiers
are defined in `crates/alas-testkit/src/lib.rs`, each with the reason it
exists, and a module that cannot meet its tier is a finding.

### Running a part of the suite

```powershell
cargo test -p alas-units          # one crate
cargo test parity                 # every test with "parity" in its name
cargo test -- --ignored           # the tests that need MSES, Nastran or AVL installed
cargo test -- --nocapture         # show output the program printed
```

### What the tests do not cover

A parity test only checks the points its fixture was generated at. That is why
each module also has ordinary tests for properties that should hold everywhere
— an atmosphere that gets colder with altitude, a wing area that scales
correctly, a configuration that survives a save and reload. Both run under
`cargo test`.

---

## Checking the state of the port

```powershell
cargo xtask gate
```

This is what must pass before a commit. It runs, in order: the repository
conventions (file sizes, licence headers, comment rules), formatting, the
compiler's lints, and the whole test suite. It stops at the first failure and
says what it wants.

For "how much is done", read `docs/PORTING.md`. Every module of the Python
implementation has a row there with a status: `todo`, `wip`, `green`,
`native` or `dropped`. Counting the `green` rows is the honest progress
measure, and it cannot drift from reality because the gate fails if a crate
exists that no row mentions.

---

## Running the application

Not yet possible: the interface is phase 12, and the compute core comes first.
When it exists there will be two ways in.

```powershell
cargo run --release
```

starts the desktop application. `--release` builds the optimized version, which
is slower to compile and much faster to run — always use it for anything you
are timing or actually using.

```powershell
cargo run --release -- headless run --config configs/a320.yaml --out result.json
```

runs an analysis with no interface and writes the results as JSON. This is what
the acceptance check uses: run the same configuration through Python and
through Rust, and compare the two result files field by field.

The built executable lands at `target\release\alas.exe` and is standalone —
copy it anywhere, no installation, no Python.

---

## Regenerating reference data

Only needed if a fixture has to change. The generators read the Python
implementation and write into `golden/`.

```powershell
& "C:\Users\Marcos\OneDrive\Proyectos\Universidad\ALAS\.suave-venv\Scripts\python.exe" golden\generators\gen_units.py
```

Use `.suave-venv` for anything touching SUAVE and `.venv` for anything touching
AeroSandbox or ALAS itself.

A generator refuses to run if the Python repository has uncommitted changes.
That is deliberate: a fixture records which revision it came from, and a
fixture generated from a working tree that has since moved on cannot be
reproduced, so it is not evidence of anything.

---

## Reading Rust without knowing Rust

Enough to review a change:

| You will see | It means |
|---|---|
| `fn name(x: f64) -> f64` | A function taking a number and returning one |
| `pub` | Visible outside this file |
| `//!` at the top of a file | What this module is for and why it exists |
| `///` above an item | What that item does |
| `let x = ...` | A value; `let mut x` if it changes afterwards |
| `struct` / `enum` | A record; a choice between alternatives |
| `Option<T>` | Either a `T` or nothing — absence is explicit |
| `Result<T, E>` | Either a `T` or an error — failure is explicit |
| `#[test]` | A test, run by `cargo test` |
| `assert!(...)` | Something that must be true, or the test fails |

The parts worth reviewing are the module doc comment at the top, the constants
and where they are cited from, and the test names — which are written as
sentences stating the property being checked, so the test list reads as a
description of what the module guarantees.

---

## Backing up

The repository has no remote, by design. To write a full copy into OneDrive:

```powershell
cargo xtask backup
```

That produces a single file containing every branch and every commit, which a
sync client handles cleanly — unlike the build directory, which produces
thousands of files and would defeat it. Run it after any substantial session.
