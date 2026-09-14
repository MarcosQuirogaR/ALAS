# NASTRAN-95 bundle policy and validated performance

NASTRAN-95 remains a separate NOSA 1.3 program launched by ALAS. It is not
linked into, translated into, or relicensed as part of the AGPL application.
The Windows package may aggregate a reviewed build beside `alas.exe` at
`external tools/NASTRAN-95`.

This is an engineering and release-control policy, not legal advice. Before a
public release, counsel should confirm the provenance of the selected upstream
fork and the application of NOSA 1.3 to the intended distribution territory.

## Fail-closed release inputs

The release task will not create an archive unless the staging directory has:

- `build/bin/nastran.exe` and `nastran95-build.txt`;
- the `rf` tree, including `NASINFO`;
- the required GNU Fortran runtime files and their licence notices in `runtime`;
- the filled-in NOSA licence as `LICENSE`;
- `SOURCE-REVISION.txt`, identifying the immutable upstream revision and build;
- `MODIFICATIONS.md`, recording every local change, date, originator, and the
  NOSA 1.3 section 3C characterization statement;
- `source/NASTRAN-95-source.zip`, the exact corresponding modified source and
  build instructions for the distributed executable.

The packaged executable is discovered automatically only from this adjacent
directory. Explicit user configuration and `ALAS_NASTRAN95_*` variables take
priority.

## Production-model benchmark (2026-08-30)

The benchmark used the ALAS full wingbox: 4,632 `GRID`, 6,394 `CQUAD4`, 92
`CBAR`, and 48 `CTRIA3` entries, with the production SOL 101 and SOL 103 decks.
The compiler was GNU Fortran 16.2.0 on Windows, with `-fno-automatic`, a
64-million-word open-core build, and global `-O0`.

| Case | Wall time | F06 size | Result |
|---|---:|---:|---|
| SOL 101, baseline | 16.47 s | 2.85 MB | Passed |
| SOL 103, all-grid displacement print | about 457 s | 16.91 MB | Passed |
| SOL 103, required front-spar nodes only | about 349 s | 1.47 MB | Passed |
| Global `-O1`, SOL 101 | 0.67 s to failure | — | Fatal 3011 |
| Global `-O1`, reduced-output SOL 103 | 0.12 s to failure | — | Fatal 3011 |

Restricting the modal displacement request improved modal wall time by about
24% and reduced retained output by about 91%. All 30 unique printed
eigenvalues matched the all-grid baseline exactly. The change does not reduce
the stiffness/mass system or alter eigensolver mathematics; it only avoids
formatting and parsing thousands of unused eigenvector rows.

The solver remains single-threaded. Independent analyses may run concurrently
only after their scratch and rigid-format staging directories are isolated;
SOL 101 is too short relative to SOL 103 for that concurrency alone to produce
a large improvement. Global optimization is rejected. Any selective
module-level optimization remains experimental until it passes SOL 101 and SOL
103 regression, eigenvalue correlation, displacement/stress correlation, and
repeatability checks on the production decks.

## Local remediation status

The sibling solver now records upstream fork commit
`060ed900db822745353fcce96bddc1565027e7db` in `SOURCE-REVISION.txt`, verified
against the fork remote, and inventories the local diff in `MODIFICATIONS.md`.
Preexisting local change dates and authors are unknown, so this inventory is
not a completed NOSA characterization or authorization to distribute. Required
license and exact source/archive inputs remain release blockers.

An opt-in `NASTRAN95_EXPERIMENTAL_SELECTIVE_OPTIMIZATION` CMake option applies
GNU `-O3` except `mis/ifp1c.f` (`-O0`). It defaults off to retain the measured
working build. Configuration on the audit-remediation host could not find
Ninja or the GNU Fortran installation referenced by the old CMake cache;
there is no rebuilt executable or new performance claim. Production adoption
still requires the regression and correlation checks above.

## Kernel-whitelist optimization (2026-09-11)

The selective `-O3` option was built and run for the first time, with GNU
Fortran 16.2.0 (winlibs) and Ninja 1.13.2 unpacked under the ALAS
`.agent/tools/` directory because the MSYS2 installation the old CMake cache
referenced no longer exists. It fails the production SOL 101 deck with USER
FATAL MESSAGE 321 (`mis/ifp.f`) after 0.16 s, so `ifp1c.f` is not the only
routine the optimizer breaks. `-O2 -fno-aggressive-loop-optimizations
-fno-strict-aliasing` over every file fails the same way; excluding the
`mis/if*.f` and `mis/x*.f` families moves the failure to XGPI (SYSTEM FATAL
14, array IORDNL overflowed); optimizing the kernel and assembly families
together fails in EMGOUT (SYSTEM FATAL 3111). Several unrelated legacy
routines miscompile, so an exclusion list is the wrong tool.

The working policy is a whitelist. The CMake project now has two empty-by-
default cache strings, `NASTRAN95_OPTIMIZATION_FLAGS` and
`NASTRAN95_OPTIMIZED_GLOBS` (recorded in the solver's `MODIFICATIONS.md`): every
source stays at `-O0` and only the listed files receive the flags. With
`-O2 -fno-aggressive-loop-optimizations -fno-strict-aliasing` applied to the
decomposition, forward-backward substitution, multiply-add, transpose and
eigensolver families only (`mis/fbs*.f`, `mis/sdcomp*.f`, `mis/sdcmps.f`,
`mis/decomp.f`, `mis/cdcomp.f`, `mis/cdcmp*.f`, `mis/cfbsor.f`, `mis/gfbs.f`,
`mis/invfbs.f`, `mis/mpyad*.f`, `mis/mpy3*.f`, `mis/mpy4t.f`, `mis/mpydri.f`,
`mis/mpyq.f`, `mis/invp*.f`, `mis/feer*.f`, `mis/solve*.f`, `mis/trnsp*.f`), the
production SOL 101 deck passes with an F06 of identical size, all 12,822
displacement rows identical to the `-O0` baseline to every printed digit,
identical epsilon values, and 13.6 s wall time against 23.6 s for the `-O0`
executable on the same host (both under the ALAS launch contract: DBMEM 1,
OCMEM 64,000,000 words, patched NASINFO timing constants). The SOL 103 result
for the same build is recorded in `docs/STATUS.md` together with its modal
correlation against the baseline.

Configure and build:

```sh
cmake -S . -B build/o2-kernels2 -G Ninja \
  -DNASTRAN95_OPTIMIZATION_FLAGS="-O2 -fno-aggressive-loop-optimizations -fno-strict-aliasing" \
  -DNASTRAN95_OPTIMIZED_GLOBS="mis/fbs*.f;mis/sdcomp*.f;mis/sdcmps.f;mis/decomp.f;mis/cdcomp.f;mis/cdcmp*.f;mis/cfbsor.f;mis/gfbs.f;mis/invfbs.f;mis/mpyad*.f;mis/mpy3*.f;mis/mpy4t.f;mis/mpydri.f;mis/mpyq.f;mis/invp*.f;mis/feer*.f;mis/solve*.f;mis/trnsp*.f"
cmake --build build/o2-kernels2
```

The default `-O0` build and the packaged executable are unchanged. Adopting the
whitelist build for the bundle still requires the SOL 101/103 regression on
every production deck, the eigenvalue correlation, the repeatability check,
and the NOSA source-archive obligations described above.
