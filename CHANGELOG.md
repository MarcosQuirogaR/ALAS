# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-07-29

First public release. The project was developed privately under the name
**AeroForge**; this release renames it, licenses it, and removes everything
that could not lawfully be redistributed.

### Changed

- **Renamed to ALAS** (Aircraft Layout, Analysis and Sizing). The Python
  package is now `alas`, the console script is `alas`, the public config class
  is `ALASConfig`, environment variables are `ALAS_*`, and release binaries are
  `ALAS-windows.exe` / `ALAS-linux`. Verified behaviour-neutral: a full run
  differs from the previous baseline only in the `metadata` block of
  `design_data.json`.
- **Licensed under AGPL-3.0-or-later.** `NOTICE` additionally records
  authorship under Spanish moral-rights law, a commercial-licensing route, a
  trademark reservation, and why combining AGPL-3.0 with LGPL-2.1 SUAVE is
  permitted. `THIRD-PARTY-NOTICES.md` lists every component and its licence.

### Removed

- **MSES binaries are no longer distributed.** MSES is licensed per seat by
  MIT's Technology Licensing Office and was never redistributable. It is now a
  user-supplied external tool, as Nastran and Patran already were. The shipped
  application is unchanged: the binaries were never frozen into it.
- **Enroute navdata is no longer bundled.** It is GPL-3.0 and would impose that
  licence on every redistribution. It is now downloaded on request, from
  Setup > External Tools or `scripts/download_navdata.py`, and routing falls
  back to a great circle until it is present. This removes ~10 MB from the
  installer.
- `requirements.txt`, which listed a provisioning script that no longer exists.
  `pyproject.toml` and `uv.lock` are the single source of truth.

### Added

- **Asset downloads in-app.** `GET /assets/status` and `POST /assets/navdata`,
  with a Setup > External Tools card that states the licence before offering
  the download.
- **Test suite and `tests/`**, starting with path resolution -- the logic whose
  failure mode only appears in a packaged build.
- `CONTRIBUTING.md` (including the code-style rules), `GOVERNANCE.md`,
  `SECURITY.md`, `CODE_OF_CONDUCT.md`, `CITATION.cff`.
- `.gitattributes` pinning line endings and marking the vendored SUAVE tree, so
  file handling no longer depends on each contributor's local git configuration.

### Fixed

- **Mission analysis failed on any fresh clone.** The vendored SUAVE tree
  carries upstream's own `.gitignore`, which excludes a generated `version.py`.
  Git honoured that nested rule, so the file was never committed and SUAVE
  failed at import with `No module named 'SUAVE.version'`.
- **An unconfigured NASTRAN path launched the repository root as a program.**
  `Path("")` normalises to `"."`, so the blank default resolved to a directory
  that passed an existence check and reached `subprocess`. Since `run_nastran`
  defaults to on, every user without a licensed NASTRAN hit it. Patran shared
  the flaw.
- **Downloaded assets could be silently truncated.** Downloads now use a
  timeout, a size floor, and an atomic rename, so an interrupted transfer
  cannot leave a partial file that the navdata parser would accept.
- Documentation no longer claims the navdata is unbundled while shipping it,
  nor references a PySide6 interface that was removed.
