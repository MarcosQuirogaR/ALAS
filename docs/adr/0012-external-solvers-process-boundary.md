# 0012. External solvers as an optional process boundary

Status: Accepted (2026-10-22)

## Context

ALAS can use MSES, MSC Nastran, NASTRAN-95, AVL, VSPAERO, OpenVSP, OpenFOAM,
Gmsh, ParaView and others. Some are proprietary, some GPL, some large. None
may be required to build or test the program, and a missing tool must not
change the meaning of a result.

## Decision

1. Every external solver is a separate, optional child process. No solver code
   is linked into ALAS.
2. `alas-exec` owns discovery (`ToolLocator`: the configured path, the parent
   chain of the application root up to a checkout marker, the per-user data folder,
   and the vendor system roots; `ALAS_TOOL_DISCOVERY=disabled` and
   `ALAS_FLOWUNSTEADY_EXE` are the environment controls), launching and process
   supervision; on Windows every child is placed in a kill-on-close Job Object.
3. Each analysis reports a typed status (result, `Error`, `NotRun`). A missing
   tool is `NotRun` and expected; a diverged run is `Error` and is not. Where
   an analytical answer exists it is computed without the solver; where it does
   not, the absence is reported. A solver result is never silently replaced by
   an approximation.
4. Tests that need a tool are `#[ignore]` and run with `--ignored`; the build
   and the default test suite need none.
5. The one-click installer is manifest-driven and fails closed: it fetches only entries
   that carry a pinned SHA-256, needs a consent receipt built from the plan the
   user was shown, checks the extracted tree for path escapes, runs a health
   check, restores the previous install on failure, installs into the per-user
   data folder, and never
   handles credentials.
6. Bundling follows ADR 0009.
7. *Accepted after the Unix watchdog tests passed on Linux (WSL 2, Ubuntu 26.04).*
   Every helper or solver the application spawns goes through `alas-exec`'s
   supervised spawn (`SupervisedSpawn`, or `captured::run_captured` for a short
   helper whose output is read: curl, tar, a script host): own process group,
   no console window, a deadline with a tree kill and kill-on-parent-exit. On
   Windows the latter is the kill-on-close job object; on Unix, where the
   workspace's ban on `unsafe` rules out `PR_SET_PDEATHSIG`, it is a per-child
   `sh` watchdog that polls once a second and kills the child's process group
   when ALAS has gone (see `supervise.rs` for the residual behaviour: up to
   about a second of lag, and no cover if the watchdog is killed too).
   Exceptions: a program started for the user to keep, such as a viewer opened
   on a result (ParaView, the OpenVSP GUI, the file manager), is not ALAS-owned
   and must outlive it (it is only reaped when it exits). Build tooling in
   `xtask`, the `alas-acceptance` binaries and the `alas-uav` examples are
   developer harnesses, not product, and are outside this rule.

## Consequences

- Cross-checks between ALAS's native methods and a solver are optional
  evidence, labelled with the solver's status.
- Tool folder names are user-visible (`external tools/`); renaming them needs
  a compatibility period for the locator (see ADR 0014).

## Evidence

`crates/alas-exec/src/tools/locator.rs`, `crates/alas-exec/src/installer.rs`,
`docs/ARCHITECTURE.md` (mechanism 4), `docs/tool-installer.md`.
