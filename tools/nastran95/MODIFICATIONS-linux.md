# Local source modification record

Base: Foadsf/NASTRAN-95 commit `060ed900db822745353fcce96bddc1565027e7db`.
The remote HEAD was independently checked with `git ls-remote origin HEAD` on
2026-09-05. No assumption about the fork's earlier NASA provenance is made.

| Files | Change | Date and originator | Verification |
|---|---|---|---|
| `bin/nastrn.f`, `CMakeLists.txt` | Parameterize open core, default 64,000,000 four-byte words (256,000,000 bytes), replacing 14,000,000; enable preprocessing and emit open-core metadata | Preexisting local changes; date and originator unknown | Source diff inspected; executable/source association not established |
| `CMakeLists.txt` | GNU static local storage via `-fno-automatic`; global `-O0` workaround | Preexisting local changes; date and originator unknown | Existing ALAS bundle document reports successful production decks at `-O0` and fatal 3011 at global `-O1` |
| `CMakeLists.txt` | Add disabled-by-default experimental selective optimization: GNU `-O3`, with `mis/ifp1c.f` kept at `-O0` | 2026-09-05, Marcos Quiroga Rodriguez (placeholder originator wording, to be confirmed) | Configure attempted; missing Ninja and GNU Fortran installation prevents build and solver regression |
| `CMakeLists.txt` | Add empty-by-default `NASTRAN95_OPTIMIZATION_FLAGS` cache string that, when set, replaces the `-O0` default and the selective `-O3` for every source file, so an optimization level can be paired with `-fno-aggressive-loop-optimizations -fno-strict-aliasing` | 2026-09-11, Marcos Quiroga Rodriguez (placeholder originator wording, to be confirmed) | Built with GNU Fortran 16.2.0 (winlibs) and Ninja 1.13.2. The selective `-O3` build (option above) fails the production SOL 101 deck with USER FATAL MESSAGE 321 in `mis/ifp.f` after 0.16 s, so `ifp1c.f` is not the only routine the optimizer breaks; the flag-set variant is the follow-up experiment. Regression status of each variant is recorded in the optimization sections of `docs/NASTRAN95-BUNDLE.md` |
| `CMakeLists.txt` | Add empty-by-default `NASTRAN95_UNOPTIMIZED_GLOBS` list: source globs kept at `-O0` while `NASTRAN95_OPTIMIZATION_FLAGS` optimizes the rest, for bisecting the routines the optimizer breaks | 2026-09-11, Marcos Quiroga Rodriguez (placeholder originator wording, to be confirmed) | `-O2 -fno-aggressive-loop-optimizations -fno-strict-aliasing` over every file also fails SOL 101 with USER FATAL MESSAGE 321 (0.34 s), so the guard flags do not explain the failure; the first bisection keeps `mis/if*.f`, `mis/x*.f`, `bin/*.f` and `bd/*.f` at `-O0` |
| `CMakeLists.txt` | Remove `EXCLUDE "bd/semdbd.f"` from the block-data source glob, so `bd/semdbd.f` is compiled and linked | Preexisting local change; date and originator unknown | Part of the Windows build that passes the production SOL 101/103 decks; Linux link unverified |
| `CMakeLists.txt` | Add empty-by-default `NASTRAN95_OPTIMIZED_GLOBS` cache string: when non-empty, every source stays at `-O0` and only the matching globs receive `NASTRAN95_OPTIMIZATION_FLAGS` (the kernel whitelist) | 2026-09-11, Marcos Quiroga Rodriguez (placeholder originator wording, to be confirmed) | Kernel-whitelist SOL 101 result in `docs/NASTRAN95-BUNDLE.md`; not used by the Linux build |

The modifications above change resource allocation and build behavior. No
numerical algorithm source change is introduced by the selective optimization
option. Optimization can nevertheless change solver behavior and requires
SOL 101/103 regression before production use. Existing solver binaries are
retained unchanged.
## Linux build (ALAS)

`tools/nastran95/build-linux.sh` applies the source differences listed above (and no others) as
`tools/nastran95/alas-local.patch` (`bin/nastrn.f` and `CMakeLists.txt`, the
`git diff` of upstream `060ed900db822745353fcce96bddc1565027e7db` to the local
sibling commit `4390bce`) and builds with the CMake defaults (`-O0`,
`-fno-automatic`). The only additions are command-line link options,
`-static-libgfortran -static-libgcc`, which change no
source file. `SOURCE-REVISION.txt` and `source/NASTRAN-95-source.zip` identify
the corresponding source of the distributed executable.

This remains an engineering inventory, **not a completed NOSA section 3C
compliance statement**: the originators and dates of the preexisting changes
are unknown, and the filled-in NOSA agreement (`LICENSE`) is supplied by the
release owner with `--license`; without it the packaging step records
NASTRAN-95 as not bundled.
