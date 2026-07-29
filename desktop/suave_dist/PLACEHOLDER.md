# suave_dist

`//go:embed` (see `embed_suave.go`) requires at least one file to exist under
this directory at Go-compile time, even when no bundled SUAVE runtime has been
built yet -- so this placeholder always stays checked in and the rest of this
directory's contents (the actual `<goos>-<goarch>/` bundles) are gitignored
build output.

`wails dev` never populates this directory: SUAVE mission analysis in dev mode
resolves its venv/runner the normal way (`alas/paths.py`'s
`resolve_tool_dir`, pointed at the repo's own `.suave-venv`/
`external tools/suave_runner`). Only `wails build`'s `*/*` `preBuildHooks`
entry (`wails.json`, via `scripts/prebuild.py`) does, by running
`scripts/build_suave_env.py`. At startup, `sidecar.go` checks whether a real
bundle for the current `GOOS`/`GOARCH` exists under here; if not (this
placeholder is all there is), the sidecar falls back to its normal
`alas/paths.py`-based resolution instead of failing.
