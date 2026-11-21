# 0014. Repository layout

Status: Proposed (2026-10-22). Not implemented: no file has been moved.

## Context

The repository root holds source-level directories, but also two folders with
spaces in their names (`external tools/`, `reproducible tools/`), three brand
PNG files and an icon, and a small `tests/` folder holding one fixture
(`tests/golden/preset_dimension_corrections.json`). `tools/` holds a flat mix of
scripts for unrelated purposes. `docs/` mixes user guides, developer notes,
architecture and method records in one flat folder. Packages, the installer and
several tests refer to `external tools/` by name, so the folder name is also a
user-visible layout, and saved data refers to it too (see "Persisted data").

## Decision (target layout)

```
assets/brand/            icon and logos (compile-time assets)
assets/mses, textures    unchanged
tools/external/          tracked third-party payloads (AVL, schemas)
tools/reproducible/      scripts that reproduce published numbers
tools/cfd, parity, analysis, windows, openvsp   grouped scripts
tools/cabin_renderer/    unchanged
docs/user, developer, architecture, methods     sections
docs/adr/                this directory
crates/<name>/examples/  stay where Cargo discovers them, catalogued in
                         docs/developer/examples.md
tests/golden/            unchanged (read by tests)
```

### Moves, with the references each one touches

Counts are `git grep` file counts at the time of writing.

| Move | From -> to | References to update |
|---|---|---|
| M1 | `external tools/` -> `tools/external/` (5 tracked files) | 44 crate files (tool locator, installer, MSES default, tests, osmap), `xtask` dist, AVL packaging and smoke test, `.gitignore`, golden fixtures and their generators, docs and site pages, `THIRD-PARTY-NOTICES.md` |
| M2 | `reproducible tools/` -> `tools/reproducible/` (1 script) | none |
| M3 | `app_icon.ico`, `app_logo.png`, `app_text_logo.png`, `app_branding.png` -> `assets/brand/` | `alas-app` build script and resource test, `alas-gui` branding module and a view test, the dist allowlist constants |
| M4 | loose scripts in `tools/` into `cfd/`, `openvsp/`, `parity/`, `analysis/`, `windows/` | dist allowlist, the parity and correlation docs, the site build, CI |
| M5 | `docs/*.md` into `user/`, `developer/`, `architecture/`, `methods/` as listed in `docs/README.md` | comments and docs that cite a path (most-cited: `PORTING.md` 56 files, which therefore stays) |

### Rules for carrying it out

1. One move per commit, no behaviour change in the same commit, each followed by
   the repository checks, the capability-map check, `cargo xtask dist` and the
   fast tier, and each audited independently.
2. Repository and archive paths are separate from user-data paths. The archive
   and checkout may adopt `tools/external/` while keeping a lookup of
   `external tools/` for one release. The per-user data folder
   (`<user data>/external tools`) is a live location that the installer writes
   and that saved settings point to; it is not renamed by M1.
3. Persisted data. Several persisted values carry the old folder name: the
   configuration default `mses_dir = "external tools/MSES"` and any saved copy of
   it, the installer's `installed-tools.json` entries
   (`directory: "external tools/<dir>"`), absolute paths in the saved tool
   preferences, and Spanish catalogue strings. Renaming the user-data folder
   requires, first, a load-time conversion of persisted `external tools/...`
   values in the style of ADR 0004 and a migration of `installed-tools.json`,
   both tested with old files. Until both exist, the old name has no removal
   date.
4. `docs/PORTING.md`, `docs/source-size-budgets.tsv`, `docs/schemas/`,
   `docs/data/` and `docs/benchmarks/` stay where code and tests read them.
5. Source-snapshot allowlist. `tools/` is admitted by file extension in
   `xtask/src/dist.rs` (`SOURCE_TOOL_EXTENSIONS`, including `txt`), whereas
   `external tools/` is not a source root. After M1,
   `tools/external/AVL-GPL-2.0.txt` would enter the snapshot while the other
   files would not. The move must add an explicit exclusion or allowance and
   update the license-boundary tests and snapshot tests to the chosen
   behaviour.
6. Examples are not moved: Cargo discovers only `examples/<name>.rs` and
   `examples/<name>/main.rs`, so an `examples/dev/` subdirectory would need an
   `[[example]]` entry per file.

## Consequences

- The moves touch code in about 50 files, so they are scheduled when no open
  change is editing them.
- Decisions still open: the archive layout (`tools/external/` primary with the
  old name read for one release is recommended), the user-data folder (keep
  until the migration exists), and whether `PORTING.md` stays at the docs root
  (recommended).

## Evidence

`.gitignore`, `xtask/src/dist.rs`, `crates/alas-exec/src/tools/locator.rs`,
`crates/alas-exec/src/installer/engine.rs`, `crates/alas-config/src/mses.rs`.
