# Running and checking ALAS

Written for someone who has not used Rust before. You do not need to learn the
language to check that this program is correct; the tooling is one command,
and the interesting output is in English.

---

## First, the one-time setup

On a new machine:

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

Run these from the repository root.

```powershell
cargo test              # check that everything still agrees with Python
cargo xtask gate        # the full check: formatting, lints, tests, conventions
cargo run --release --bin ALAS   # start the application
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

Include the whole block when reporting a mismatch. The one thing not
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
each module also has ordinary tests for properties that should hold everywhere:
an atmosphere that gets colder with altitude, a wing area that scales
correctly, a configuration that survives a save and reload. Both run under
`cargo test`.

---

## Checking the state of the port

```powershell
cargo xtask gate
```

This is what must pass before a commit. It runs, in order: the repository
conventions (file sizes, licence headers, comment rules), formatting, the
compiler's lints, and the whole test suite. It reports every failed step and
says what it wants.

The gate has three tiers. The test split needs
[cargo-nextest](https://nexte.st) (a prebuilt Windows binary from its GitHub
releases, or `cargo install cargo-nextest --locked`); without it the gate falls
back to `cargo test` and cannot separate the heavy tests.

```powershell
cargo xtask gate --quick           # every change: checks, fmt, lints and fast tests of affected packages
cargo xtask gate                   # phase integration and CI: everything
cargo xtask gate --integration     # only the heavy tier (alias: --slow)
cargo nextest run                  # the fast dev-loop tier alone
cargo nextest run -p alas-pipeline # the fast tier of one crate
cargo nextest run --profile integration   # the heavy tier: all-preset replays, acceptance, staged searches
cargo nextest run --profile full   # every test
cargo xtask gate --medium          # tests of 5 s or more outside the heavy tier
cargo xtask gate --nightly         # the *_nightly tests (16-worker staged-search replay)
cargo xtask test [nextest args]    # nextest with the shared test-result store (below)
cargo xtask smoke                  # every preset: native baseline + 24+24 evaluation search, in parallel
cargo xtask tiers target/nextest/full/junit.xml --also <older.xml> --write   # regenerate the tier lists (slowest time per test)
cargo xtask repin <suite>          # rewrite tests/golden/<suite>.json from its producer
```

`cargo xtask test` sets `ALAS_MEMO_KEY` (a hash of the sources), which switches
on `alas_testkit::memo`: tests that need the same expensive deterministic result
(for example `assess_product_candidate` on a preset nominal) compute it once, in
whichever test process asks first, and the others read it from
`target/alas-memo/<key>/`. A plain `cargo test` or `cargo nextest run` leaves it
off and computes everything, as before. Pins that move with the model live in
`tests/golden/<suite>.json` (`alas_testkit::pins::assert_pinned`); after an
intended change, `cargo xtask repin <suite>` rewrites their values and the diff
is what gets reviewed.

The tiers are defined in `.config/nextest.toml`. The `integration` tier is the
set of tests that each take tens of seconds or more (about 3 % of the tests and
70 % of the summed test time at the time it was generated); nothing in it is
skipped by the full gate, it only stays out of the edit-compile-test loop.
Doctests are not used: the gate does not run them, and the repository checks
reject a doc example that rustdoc would run.

### Fast iteration

Optional setup that shortens the edit-build-test loop on Windows. None of it
changes results.

* **cargo-nextest** runs each test in its own process, in parallel across
  binaries, and reports per-test times. Download the prebuilt
  `cargo-nextest-<version>-x86_64-pc-windows-msvc.zip` from
  <https://github.com/nextest-rs/nextest/releases>, check it against the
  `.sha256` published beside it, and put `cargo-nextest.exe` in
  `%USERPROFILE%\.cargo\bin`.
* **rust-lld** (shipped with the toolchain) links test binaries about twice as
  fast as `link.exe`. In a development shell:
  `$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = "rust-lld"`. It is not
  set in the repository so that release packaging keeps the reviewed linker.
* **sccache** caches compiled dependencies between target directories, which
  helps most when a fresh checkout or worktree would otherwise rebuild every
  third-party crate. Download `sccache-<version>-x86_64-pc-windows-msvc.zip`
  from <https://github.com/mozilla/sccache/releases>, verify its `.sha256`, put
  `sccache.exe` on `PATH`, then set `RUSTC_WRAPPER=sccache` and
  `CARGO_INCREMENTAL=0` for the shell (incremental compilation and sccache
  exclude each other, so keep the plain incremental setup for the crate you are
  editing).
* **`harness` profile** builds study harnesses and examples with release
  optimization but thin LTO and 16 codegen units, so they relink in a fraction
  of the shipped profile's time:
  `cargo build --profile harness -p alas-pipeline --example full_matrix`.
  The shipped `release` profile is unchanged.
* **`cargo xtask dev-env`** prints the settings above for the shell to
  evaluate, so a fresh shell or worktree needs one line:
  `cargo xtask dev-env --target-dir <target-root>/<lane> | Invoke-Expression`
  (PowerShell; `--shell sh` prints `export` lines instead). It sets
  `CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER=rust-lld` on Windows, optionally
  `CARGO_TARGET_DIR` (use one target directory per lane, reused across that
  lane's commits) and, with `--sccache`, `RUSTC_WRAPPER=sccache` plus
  `CARGO_INCREMENTAL=0`. `cargo xtask dev-env --run -- cargo build --profile
  harness -p alas-acceptance --bins` applies it to one command only. Pass
  `-j N` so the total thread count of all concurrent lanes stays within the
  host's budget.

### Native preset matrix

`matrix_native` (alas-acceptance) runs every registered preset in the three
user-facing MTOW modes, each cell one optimization run plus its native baseline,
under evaluation budgets only (`--stop-on-evaluations`), so a cell's result does
not depend on host load, worker count or cell parallelism. Run it on an idle
host; `P x W` (cells in parallel x search workers per cell) is capped at 15.

```powershell
cargo build --profile harness -p alas-acceptance --bins -j 15
target/harness/matrix_native run --output-dir out/matrix --presets all --modes all --cells-parallel 3 --workers 5 --budget matrix
```

* `--budget matrix` is the embedded table `crates/alas-acceptance/data/matrix_budgets.json`
  (per-preset screening and refinement evaluation counts, seed, provenance and a
  `status` flag; the current table is provisional, derived from the
  2026-10-07 matrix histories). `--budget FILE` loads another table of the same shape.
* Resume: a cell whose `cells/<preset>@<mode>/measurement_result.json` has
  status `success` or `infeasible` is skipped; `--force` reruns everything. `--cell-timeout-s`
  bounds each cell (the child's cooperative guard, then a hard kill 120 s later).
* `--dry-run` lists the cells and budgets without running; `--summary-only`
  rebuilds the summary from existing cell directories.
* The embedded table scales the two iterating modes (`mtow_band`,
  `payload_adjusted`) by 0.3 (`mode_scale`): their evaluations cost 3-6 times
  more than `fixed_requirement` in the 2026-10-07 data, and the 1 h target does
  not hold at full budget in all three modes. Since 2026-10-15 `mtow_band` is a
  single-pass fixed-requirement evaluation plus one search coordinate, so its
  scale is a pre-relaxation figure awaiting re-measurement. A cell whose search finds no
  feasible design is reported as `infeasible` with a `baseline_only` block in
  its `measurement_result.json`, so every cell carries a native baseline.
* Outputs: `matrix_summary.json` and `matrix_summary.csv` (status, wall times
  per stage, evaluations, lane utilization, termination, baseline and optimized
  fuel and L/D, feasibility findings, a determinism digest), rebuilt after every
  cell. `matrix_native compare A.json B.json` lists cells whose digest differs
  between two summaries (same budgets must give equal digests at any `P x W`).

### Throughput options for external tools

The pipeline runs VSPAERO, AVL, MSES, NASTRAN-95 and MSC Nastran when they are
installed. These environment variables change how fast that happens, not what
is computed, unless the reduced profile is chosen. All default to off.

| Variable | Effect |
|----------|--------|
| `ALAS_VSPAERO_THREADS` | OpenMP threads for each VSPAERO process (`-omp N`). Default: logical cores divided by `ALAS_CONCURRENT_JOBS`. VSPAERO's polar was bit-identical at 1, 2, 4, 8 and 16 threads on the case checked. |
| `ALAS_CONCURRENT_JOBS` | How many analyses share this machine (default 1); only used to split cores. |
| `ALAS_TOOL_TIMEOUT_SCALE` | Multiplier on every external-tool wall-clock deadline (default 1). Raise it when several analyses share a machine: a healthy solver starved of CPU can otherwise hit its deadline and change a result into a `timed_out` status. |
| `ALAS_TOOL_CACHE_DIR` | Enables an input-hash result cache for AVL, VSPAERO, MSES, NASTRAN-95 and MSC Nastran. A run is keyed by the SHA-256 of its input files, the solver executable, its options and, for MSES, its working files; only successful runs are stored; a hit restores the exact output files, so results with and without the cache are identical. Delete the directory to clear it. The desktop application never sets it. |
| `ALAS_FIDELITY_PROFILE=matrix` | Reduced external-tool sweeps for validation matrices: 5 VSPAERO alpha points instead of the full sweep, 3 MSES polar points (trim -3, 0, +3 degrees) instead of 7, an 8-chord AVL mesh instead of the fine mesh, with the same wake iterations and convergence gates. VSPAERO values at the retained alpha points are bit-identical to the full sweep; the AVL mesh moves coefficients by up to a few percent; MSES converges point by point along its sweep, so a 3-point sweep can converge fewer points than the 7-point one (a coarse screening indicator only). Native ALAS results are unaffected. Every output records the profile (`execution.tool_profile`, `run_manifest.json`, a `REDUCED_FIDELITY.txt` marker and a run-log line); a reduced run is not a full-fidelity external-tool result. |

Set `PipelineOptions.parallel` (the harnesses expose `--parallel`) to run the
independent downstream stages concurrently; each tool writes into its own
subdirectory of the run, and results are identical to the sequential order.

For numerical parity against the Python reference and licence provenance,
read `docs/PORTING.md`; every translated module has a row there. It is not
a project-completion tracker any more, since orchestration layers such as
`alas-pipeline` and `alas-gui` were written natively rather than translated.

---

### Optional Earth texture

Globe and map figures use the plain vector globe and flat map by default. ALAS
does not ship an Earth raster. To add a texture, set `ALAS_EARTH_TEXTURE` to the
path of a local equirectangular PNG (2:1, for example a copy of the NASA Blue
Marble, which is public domain). It is read once per process; if it is unset or
unreadable, figures render without the texture and a warning is printed.

## Running the application

There are two ways in, both through the `alas` binary built from `alas-app`.
The workspace also builds `alas-bench` and `external_preset_audit`
(`alas-acceptance`), so `cargo run` alone is ambiguous; name the binary.

```powershell
cargo run --release --bin ALAS
```

starts the desktop application (equivalent to `... --bin ALAS -- --gui`).
`--release` builds the optimized version, which is slower to compile and much
faster to run; always use it for anything you are timing or actually using.

```powershell
cargo run --release --bin ALAS -- --config path\to\your.yaml --output outputs
```

runs an analysis with no interface: `--config` overlays a YAML/JSON
configuration, `--output` selects the directory the reports and figures are
written into (default `outputs`). Run `cargo run --bin ALAS -- --help` for
the full flag list; it includes `--no-optimize`, `--no-mission`,
`--aero-solver`, `--cpacs-input` and others.

The built executable lands at `target\release\ALAS.exe` and is standalone:
copy it anywhere, no installation, no Python.

On Windows, every solver ALAS launches (AVL, VSPAERO, MSES, OpenVSP, NASTRAN,
Patran, FLOWUnsteady) is placed in a kill-on-close Job Object owned by the
ALAS process, so ending ALAS from Task Manager, or a crash, ends those solvers
and anything they started. Task Manager lists them under their own executable
names; the Run Log records each launch with its PID and the ALAS task behind
it. Viewers opened on results, such as ParaView, are not owned and stay open.

---

## Regenerating reference data

Only needed if a fixture has to change. The generators read the Python
implementation and write into `golden/`.

```powershell
& "<path to the Python reference>\.suave-venv\Scripts\python.exe" golden\generators\gen_units.py
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
| `Option<T>` | Either a `T` or nothing; absence is explicit |
| `Result<T, E>` | Either a `T` or an error; failure is explicit |
| `#[test]` | A test, run by `cargo test` |
| `assert!(...)` | Something that must be true, or the test fails |

The parts worth reviewing are the module doc comment at the top, the constants
and where they are cited from, and the test names, which are written as
sentences stating the property being checked, so the test list reads as a
description of what the module guarantees.

---

## Files in the desktop GUI

Every path field has a text box (paste a path) and a **Browse...** button that
opens the system file or folder dialog (Windows common dialogs; the XDG desktop
portal, or `zenity`/`kdialog`, on Linux).

All saves default to one per-user data folder, the same one that holds
`tool-preferences.json` and `custom-airports.json`:

| System | Folder |
| --- | --- |
| Windows | `%LOCALAPPDATA%\ALAS` |
| macOS | `~/Library/Application Support/ALAS` |
| Linux | `$XDG_DATA_HOME/ALAS`, else `~/.local/share/ALAS` |

The File menu shows the folder with an **Open folder** button. Relative paths
typed in a save field resolve there; any absolute path or dialog choice is
accepted instead.

**My aircraft.** *Save as my aircraft...* (Inputs page, under the aircraft
selector) stores the current aircraft and mission configuration in
`my-aircraft/` of the data folder under a name you choose. Saving under another
name keeps another variant of the same airframe (for example one per mission).
Saved aircraft appear in the aircraft list under *My aircraft*; *Manage my
aircraft...* renames, deletes, imports and exports them to any path. Loading one
keeps this machine's tool locations. The aircraft, engine and airport lists have
a search box (case-insensitive substring over name and ICAO code) that is
focused when the list opens.

## Backing up

The repository has no remote, by design. To write a full copy into OneDrive:

```powershell
cargo xtask backup
```

That produces a single file containing every branch and every commit, which a
sync client handles cleanly, unlike the build directory, which produces
thousands of files and would defeat it. Run it after any substantial working session.
