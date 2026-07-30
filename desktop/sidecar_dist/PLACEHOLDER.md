# sidecar_dist

`//go:embed` (see `embed_sidecar.go`) requires at least one file to exist
under this directory at Go-compile time, even when no frozen sidecar has
been built yet -- so this placeholder always stays checked in and the rest
of this directory's contents (the actual `<goos>-<goarch>/` frozen bundles)
are gitignored build output.

`wails dev` never populates this directory: it always runs the sidecar the
normal dev way (`uv run python -m alas.sidecar.server`, see
`sidecar.go`'s `runDev`). Only `wails build`'s `*/*` `preBuildHooks` entry
(`wails.json`) does, by running `scripts/build_sidecar.py`. At startup,
`sidecar.go` checks whether a real binary for the current `GOOS`/`GOARCH`
exists under here; if not (this placeholder is all there is), it falls back
to the same `uv run` dev path instead of failing.
