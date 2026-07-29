# ALAS desktop app

A **Wails** app: this Go shell embeds a React/TypeScript webview and spawns a
FastAPI sidecar (`alas/sidecar/`) that wraps the core `DesignPipeline`.
See `docs/architecture.md` §12 (repo root) for the full architecture and
`docs/architecture.md` §8 for how packaging works end to end.

## Live development

`wails dev` in this directory. This runs a Vite dev server with fast
hot-reload for frontend changes, and spawns the Python sidecar straight from
the repo's `uv`-managed venv (`uv run python -m alas.sidecar.server`) —
editing anything under `alas/` takes effect on the next app restart,
no separate build step. A dev server also runs at http://localhost:34115 if
you want to work from a browser with access to the bound Go methods via
devtools.

## Building a standalone executable

`wails build` in this directory produces a fully standalone executable — no
repo checkout, no `uv`, no Python installation needed on the machine that
runs it. This works by automatically running `scripts/build_sidecar.py`
(repo root) as a pre-build hook (`wails.json`'s `preBuildHooks`), which
freezes the Python sidecar with PyInstaller and stages the result under
`sidecar_dist/<goos>-<goarch>/`; `embed_sidecar.go`'s `//go:embed` bundles
that into the compiled Go binary, and `sidecar.go` extracts and runs it at
app startup instead of shelling out to `uv`.

You normally don't need to run the freeze step yourself — `wails build` does
it automatically. To freeze the sidecar on its own (e.g. to check it starts
correctly before doing a full Wails build):

```
uv run python ../scripts/build_sidecar.py
```

**PyInstaller cannot cross-compile.** Build on the same OS/arch you're
shipping for — a Windows `.exe` and a Linux binary each need their own native
build machine or CI runner. Running `wails build -platform linux/amd64` on a
Windows host still produces a *Windows* sidecar; there's no way around this,
so a cross-platform release needs one CI job per target OS, each running
`wails build` natively.

The frozen sidecar bundle is large (PyVista/VTK, scipy, aerosandbox, casadi,
pyNastran — roughly 600 MB one-dir), which also makes `wails build`'s own Go
compile step slower than usual (embedding that much data takes on the order
of a minute) — expected, not a hang.
