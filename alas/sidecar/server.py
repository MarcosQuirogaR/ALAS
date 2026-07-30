# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Sidecar entrypoint.

Run directly (``python -m alas.sidecar.server``) or via the frozen
``alas-core.exe`` build (Phase 6). Binds an OS-assigned loopback port,
announces it on stdout as a single ``ALAS_PORT=<n>`` line, then serves
the FastAPI app. The Go/Wails shell (``desktop/``) spawns this process and
reads that one line to learn where to connect -- a standard sidecar
handshake (the same idea Electron/Tauri sidecar processes use), simpler and
more robust than a fixed port (which could already be in use) or a shared
lockfile.

Binding happens on the *same* socket uvicorn then serves on (``sockets=[sock]``)
rather than closing a probe socket and re-binding the discovered port number,
which would leave a race window for another process to grab it first.
"""

from __future__ import annotations

import re
import socket
import sys
import threading

import uvicorn
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse

from .routes_airfoil_sweep import router as airfoil_sweep_router
from .routes_assets import router as assets_router
from .routes_config import router as config_router
from .routes_figures import router as figures_router
from .routes_maintenance import router as maintenance_router
from .routes_pipeline import router as pipeline_router
from .routes_validate import router as validate_router

app = FastAPI(title="ALAS Sidecar")

# Origins the embedded webview legitimately uses. Wails v2 serves the frontend
# from a custom scheme (wails://) or wails.localhost depending on platform, and
# `wails dev` runs a Vite server on localhost.
_ALLOWED_ORIGIN_RE = r"^(wails://.*|https?://wails\.localhost(:\d+)?|https?://(localhost|127\.0\.0\.1)(:\d+)?)$"


@app.middleware("http")
async def _apply_language(request, call_next):
    """Set the request's UI language from ``?lang=`` (or the X-ALAS-Lang
    header), so schema labels/help and figure text come back translated.

    Set per request rather than as app state: the frontend can switch language
    live, and a stale global would leave half the UI in the previous one. See
    alas/i18n.py for why this is a ContextVar.
    """
    from ..i18n import set_language

    set_language(request.query_params.get("lang") or request.headers.get("x-alas-lang"))
    return await call_next(request)


@app.middleware("http")
async def _block_foreign_origins(request, call_next):
    """Reject browser requests coming from an ordinary web page.

    Binding to 127.0.0.1 keeps this process off the network, but it does NOT
    protect it from the user's own browser: any site they visit can
    ``fetch("http://127.0.0.1:<port>/...")``, scan for this sidecar and drive
    it. That matters a lot here, because the pipeline config includes
    ``structures.nastran_exe_path``/``patran_exe_path`` plus their ``run_*``
    flags -- i.e. a POST to /pipeline/run can make this process spawn an
    arbitrary executable. A drive-by page could therefore have achieved code
    execution (and read back the user's designs) purely because CORS was
    ``allow_origins=["*"]``, which additionally let it READ every response.

    Browsers always attach an ``Origin`` header to such cross-origin requests,
    so anything presenting an origin that isn't the app's own webview is
    refused outright. Requests with no ``Origin`` (the webview's own
    same-origin XHRs, curl, tests) are unaffected.
    """
    origin = request.headers.get("origin")
    if origin and not re.match(_ALLOWED_ORIGIN_RE, origin):
        return JSONResponse(
            status_code=403,
            content={
                "detail": "Cross-origin requests to the ALAS sidecar are not allowed."
            },
        )
    return await call_next(request)


app.add_middleware(
    CORSMiddleware,
    # Regex, not "*": with a wildcard the browser would hand a malicious page
    # the *response body* of anything it managed to call.
    allow_origin_regex=_ALLOWED_ORIGIN_RE,
    allow_methods=["*"],
    allow_headers=["*"],
)

app.include_router(maintenance_router)
app.include_router(assets_router)
app.include_router(airfoil_sweep_router)
app.include_router(config_router)
app.include_router(figures_router)
app.include_router(pipeline_router)
app.include_router(validate_router)


@app.get("/healthz")
def healthz() -> dict:
    return {"status": "ok"}


def _bind_loopback_socket() -> socket.socket:
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.bind(("127.0.0.1", 0))
    sock.listen(128)
    return sock


def _warm_heavy_imports() -> None:
    """Import the analysis stack in the background once the server is up.

    aerosandbox/casadi/scipy (via ``alas.pipeline``) and the
    matplotlib-based figure builders take ~10s+ cold. All of them are now
    imported lazily (see ``lazy_imports.py`` / ``runs.py`` / the package
    ``__init__``), which is what lets this process bind its port and answer
    /healthz in a couple of seconds -- this thread then pre-pays the heavy
    imports so the first run/preview doesn't stall on them either. Python's
    import lock makes this safe: a request needing one of these modules
    mid-warm-up simply blocks until that import finishes.
    """
    try:
        import alas.pipeline  # noqa: F401
        from ..geometry import airfoils  # noqa: F401  (AirfoilLibrary for /schema)
        from ..reporting import visualization  # noqa: F401
    except Exception:
        # Warm-up is best-effort; a failure here surfaces later (with real
        # context) on the request that actually needs the module.
        pass


def main() -> None:
    # The sidecar runs headless under a windowed GUI parent, so every console
    # tool it shells out to (MSES/MSET/MPLOT, SUAVE, NASTRAN, Patran) would
    # otherwise allocate its own console window. AeroSandbox's MSES wrapper
    # alone issues 5+ shell=True spawns per airfoil, so an Airfoil Screening
    # MSES stage popped hundreds of terminals and slowed to a crawl partway
    # through. Installed here -- a process-wide default belongs at an entry
    # point, never in library code. See alas/proc.py.
    from ..proc import install_no_console_default

    install_no_console_default()

    sock = _bind_loopback_socket()
    port = sock.getsockname()[1]
    # Flush immediately and use stdout (not logging) -- the parent Go process
    # reads exactly one line matching this prefix to learn the port.
    print(f"ALAS_PORT={port}", flush=True)
    sys.stdout.flush()

    threading.Thread(
        target=_warm_heavy_imports, daemon=True, name="alas-warmup"
    ).start()

    config = uvicorn.Config(app, log_level="warning")
    server = uvicorn.Server(config)
    server.run(sockets=[sock])


if __name__ == "__main__":
    main()
