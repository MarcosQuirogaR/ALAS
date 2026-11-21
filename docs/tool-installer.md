# One-click external tool installer

ALAS drives several external programs (OpenVSP/VSPAERO, Gmsh, ParaView,
OpenFOAM, NASTRAN-95, MSES, MSC Nastran/Patran, FLOWUnsteady). The installer on
**Setup > External Tools > Install external tools** fetches the ones that may be
fetched, and tells you the steps for the rest.

## What it does

1. **Install all tools...** builds a plan and shows it. Nothing is downloaded yet.
   Each row names the tool, its licence, the publisher page, the exact source URL
   and the approximate size.
2. **Download and install** is the single consent. The installer runs only the
   plan that was shown; if the manifest changed in between, it refuses.
3. For each tool: resumable download (curl, HTTPS only), SHA-256 check against a
   pinned value, extraction with the system `tar` into a staging folder (entry
   names and links are checked first), move into
   `<ALAS data folder>/external tools/<tool>`, then a **health check** that runs
   the tool on a tiny case. A failure removes the new files and restores the
   previous install.
4. Installed tools are discovered automatically: ALAS already searches
   `<ALAS data folder>/external tools`. Nothing in your preferences is rewritten
   (Gmsh and ParaView paths are filled in only when they are empty).
5. **Uninstall** removes the folder and its record (`installed-tools.json`).
   A cancelled or failed download keeps its partial file so the next attempt
   resumes.

ALAS data folder: `%LOCALAPPDATA%\ALAS` on Windows, `$XDG_DATA_HOME/ALAS` or
`~/.local/share/ALAS` on Linux.

## Command line (Windows and Linux)

```
alas --install-tools            # print the plan, download nothing
alas --install-tools --yes      # consent to that plan and install
alas --uninstall-tool openvsp   # remove one installed tool
```

## Which tools are downloaded

| Tool | Licence | Handling |
|---|---|---|
| OpenVSP / VSPAERO 3.51.2 (Windows) | NASA Open Source Agreement 1.3 | Automatic, SHA-256 pinned (same pin as `tools/setup_openvsp_preview.ps1`) |
| OpenVSP on Linux | NASA Open Source Agreement 1.3 | Manual: the publisher ships .deb packages only, which need administrator rights |
| Gmsh 4.15.2 (Windows, Linux) | GPL-2.0-or-later with linking exception | Automatic, SHA-256 pinned |
| ParaView 6.2.0 (Windows, Linux; about 1 GB) | BSD-3-Clause | Automatic, SHA-256 pinned |
| OpenFOAM | GPL-3.0 | Guided: system installer / package manager needs administrator rights |
| NASTRAN-95 | NASA Open Source Agreement 1.3 | Built on your computer from the pinned source (Foadsf/NASTRAN-95 @ 060ed900), see "Tools built on your computer"; the manual build in `NASTRAN95-BUNDLE.md` remains available |
| MSES | MIT licence on request | Guided |
| MSC Nastran / Patran | Proprietary | Guided |
| FLOWUnsteady / Julia | User-chosen release | Guided |

ALAS downloads from the publisher at your request and does not redistribute these
programs, so their licences do not change ALAS's own. An entry with no pinned
SHA-256 is **never** downloaded (the installer fails closed).

## Maintainers: pinning a new tool

Edit `crates/alas-exec/data/tool-manifest.json`.

1. Review the publisher's licence and download page; record the URL of the exact
   archive (a versioned file, not "latest").
2. Download it once on a trusted machine and record `Get-FileHash -Algorithm SHA256`
   (or `sha256sum`) in lowercase.
3. Fill `artifacts.<platform>` (`windows-x86_64` or `linux-x86_64`): `version`,
   `url`, `sha256`, `min_bytes`, `size_bytes`, `format`, `directory`,
   `strip_single_root`, `executable`, and a `health` tiny case; change `kind`
   from `pending_review` to `automatic`.
4. `cargo test -p alas-exec installer` checks the manifest (HTTPS, hash present,
   a tiny case for every automatic tool, no unreviewed URL on pending tools).

## Tests

The engine tests serve archives from a mock HTTP server on `127.0.0.1` (plain
HTTP is accepted only for loopback and only in tests). They cover install and
registration, resume with a `Range` request, wrong hash, path traversal, health
failure rollback, consent mismatch, HTTPS-only policy, uninstall, and ZIP
extraction through Windows `tar.exe`. They need no network.

## Tools built on your computer (`build_from_source`)

NASTRAN-95 is published only as source, and its licence (NASA Open Source
Agreement 1.3) is not accepted by ALAS on your behalf, so ALAS never ships a
NASTRAN-95 binary. The installer instead builds it for you, from the source
you consent to download:

1. The plan row says **Will Download and Compile** and lists **each** archive on its
   own line with its source URL, licence and size: the source archive and, on
   Windows, a pinned GNU Fortran toolchain from a different publisher (a winlibs
   GCC archive that brings gfortran, CMake and Ninja; about 280 MB, unpacked once
   below `<ALAS data folder>/toolchains` and reused; a toolchain that is already
   there is listed as "already on this computer" and not counted). The row also
   shows the disk the build needs while compiling. On Linux the installer needs
   `gfortran`, `cmake` and `ninja` from your package manager (for example
   `sudo apt install gfortran cmake ninja-build`); if they are missing the row
   says **Install System Tools First** and lists what to run.
2. Every downloaded archive is SHA-256 pinned and verified; the source is the exact
   upstream revision recorded in the manifest. A toolchain that was extracted
   earlier is reused after checking its marker (the pinned hash it was extracted
   from) and its compiler; its files are not re-hashed. Changing the patch, the
   toolchain or the build commands under the same revision counts as a new build.
3. ALAS applies its own patch (open-core size and the build options listed in
   `tools/nastran95/MODIFICATIONS-linux.md`), compiles with a few minutes of
   CPU, keeps only the solver and its rigid-format tree and writes
   `ALAS-BUILD.txt` (source, hashes, toolchain, check result).
4. The **health check is a numerical one**: the reference cantilever box is
   solved through ALAS's own launcher and must match beam theory within 5 %
   (deflection and flange stress). A build that fails it is discarded and the
   previous install is restored.

A cancelled build stops the compiler. Nothing built here may be passed on to
others: the built folder carries no licence notice.

Uninstalling NASTRAN-95 removes the built tool; the unpacked toolchain stays in
`<ALAS data folder>/toolchains/` (about 1 GB) so that the next build reuses it.
Delete that folder to reclaim the space.

Each solve of the installed NASTRAN-95 runs in its own randomly named work folder,
removed afterwards (stale ones older than a day are swept). The solver is given
paths relative to that folder, so it can live in a private place: on Windows below
`%LOCALAPPDATA%\ALAS\run`, with inherited permissions removed and full control
granted only to you (by account SID) and SYSTEM (the folder is not created if that fails); on Linux
below `$XDG_RUNTIME_DIR` (or `/tmp`) with mode 0700. Other accounts cannot read or
change the files a solve uses. No drive-root folder is used.

The Windows build links statically (`-static`), so the built `nastran.exe` runs
without the toolchain or its DLLs; this was confirmed by the end-to-end build
(`cargo test -p alas-struct --test nastran95_install_e2e -- --ignored`, about
100 s, closed-form check passing).
