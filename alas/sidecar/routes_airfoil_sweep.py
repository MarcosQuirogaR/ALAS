# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Airfoil-database screening endpoints (optional tool, see
``alas.analysis.airfoil_screening``).

* ``POST /airfoil-sweep/run`` -- kick off a screening sweep over the whole
  airfoil database against the posted config + design vector, returns
  ``{"run_id": ...}``.
* ``GET  /airfoil-sweep/{run_id}/result`` -- poll: running/error/done + the
  ``AirfoilScreeningResult`` (plain ``dataclasses.asdict``, already JSON-safe).
* ``WS   /airfoil-sweep/{run_id}/events`` -- progress strings, same poll-with-
  timeout streaming pattern as ``routes_pipeline.py``'s ``stream_events``.

Structurally identical to ``routes_pipeline.py`` but against its own
``airfoil_sweep_runs.REGISTRY`` -- entirely separate from the main pipeline
run registry, so this optional feature can't interact with normal Run/
Analyze-baseline state.
"""

from __future__ import annotations

import asyncio
import dataclasses
import queue
from typing import Optional

from fastapi import (
    APIRouter,
    HTTPException,
    Query,
    Response,
    WebSocket,
    WebSocketDisconnect,
)
from pydantic import BaseModel

from ..config.design_variables import DesignVector
from ..config.settings import ALASConfig
from ..reporting.airfoil_sweep_figures import SWEEP_FIGURES
from . import airfoil_sweep_runs, lazy_imports

# Reuse the single process-wide Matplotlib render lock + SVG renderer so a
# sweep figure and a Results figure can never drive Matplotlib's global state
# concurrently (see routes_figures.py's module docstring).
from .routes_figures import _RENDER_LOCK, _close_all_figures, _render_svg

# Per-candidate MSES figures (Stage 3): the SAME factories the main Run's
# Model Comparison tab uses for the optimized design's own root section
# (alas.reporting.visualization), reused here against a screened
# candidate's own MSESPressureResult (populated by
# airfoil_screening._verify_candidate_mses) so each MSES-verified candidate
# gets its own real Cp/Mach-distribution and shock-contour figures, not just
# a scalar L/D. Named distinctly from SWEEP_FIGURES (which take the whole
# AirfoilScreeningResult) since these take one candidate's MSESPressureResult.
_CANDIDATE_FIGURES = {
    "mses_pressure": lambda mp, t: lazy_imports.viz.figure_mses_pressure_distribution(
        mp, theme=t
    ),
    "mses_mach_contours": lambda mp, t: lazy_imports.viz.figure_mses_mach_contours(
        mp, theme=t
    ),
}

router = APIRouter()

_POLL_AGAIN = object()


class AirfoilSweepRequest(BaseModel):
    config: dict = {}
    design: Optional[dict] = None
    ld_weight: float = 0.7
    fuel_weight: float = 0.3
    robustness_weight: float = 0.0
    cl_band: float = 0.05
    top_n: int = 50
    model_size: str = "large"
    alpha_min_deg: float = -4.0
    alpha_max_deg: float = 14.0
    alpha_step_deg: float = 0.5
    min_tc: float = 0.005
    max_tc: float = 0.25
    name_filter: str = ""
    refine_3d: bool = True
    refine_top_n: int = 20
    min_static_margin: Optional[float] = None
    verify_mses: bool = True
    mses_top_n: int = 5


def _build_config(data: dict) -> ALASConfig:
    try:
        return ALASConfig.from_dict(data)
    except (KeyError, TypeError, ValueError) as exc:
        raise HTTPException(status_code=422, detail=str(exc))


@router.post("/airfoil-sweep/run")
def start_sweep(req: AirfoilSweepRequest) -> dict:
    config = _build_config(req.config)
    try:
        dv = DesignVector(**req.design) if req.design else None
    except TypeError as exc:
        raise HTTPException(status_code=422, detail=f"Bad design vector: {exc}")
    run_id = airfoil_sweep_runs.REGISTRY.start_sweep(
        config,
        dv,
        ld_weight=req.ld_weight,
        fuel_weight=req.fuel_weight,
        robustness_weight=req.robustness_weight,
        cl_band=req.cl_band,
        top_n=req.top_n,
        model_size=req.model_size,
        alpha_min_deg=req.alpha_min_deg,
        alpha_max_deg=req.alpha_max_deg,
        alpha_step_deg=req.alpha_step_deg,
        min_tc=req.min_tc,
        max_tc=req.max_tc,
        name_filter=req.name_filter,
        refine_3d=req.refine_3d,
        refine_top_n=req.refine_top_n,
        min_static_margin=req.min_static_margin,
        verify_mses=req.verify_mses,
        mses_top_n=req.mses_top_n,
    )
    return {"run_id": run_id}


@router.post("/airfoil-sweep/{run_id}/cancel")
def cancel_sweep(run_id: str) -> dict:
    """Request cooperative cancellation of an in-flight sweep. The screening
    loop polls this between candidates/stages and stops early, returning the
    partial ranking it had so far (``AirfoilScreeningResult.cancelled=True``).
    Idempotent and safe on an already-finished run."""
    state = airfoil_sweep_runs.REGISTRY.get(run_id)
    if state is None:
        raise HTTPException(status_code=404, detail=f"Unknown sweep id '{run_id}'")
    state.cancel()
    return {"status": "cancelling"}


@router.get("/airfoil-sweep/{run_id}/result")
def get_sweep_result(run_id: str) -> dict:
    state = airfoil_sweep_runs.REGISTRY.get(run_id)
    if state is None:
        raise HTTPException(status_code=404, detail=f"Unknown sweep id '{run_id}'")
    if state.status == "running":
        return {"status": "running"}
    if state.status == "error":
        return {"status": "error", "error": state.error}
    return {"status": "done", "result": dataclasses.asdict(state.result)}


@router.get("/airfoil-sweep/figures")
def list_sweep_figures() -> dict:
    return {"figures": list(SWEEP_FIGURES.keys())}


@router.get("/airfoil-sweep/{run_id}/figures/{name}")
def get_sweep_figure(
    run_id: str, name: str, theme: Optional[str] = Query(default=None)
) -> Response:
    """Render one Airfoil Screening figure as themed SVG (same contract as
    routes_figures.get_figure): 404 for an unknown id/figure, 409 while the
    sweep is still running or if it errored, 404 (empty slot) when the figure
    has no data for this sweep (e.g. the 2-D->3-D chart with refinement off)."""
    state = airfoil_sweep_runs.REGISTRY.get(run_id)
    if state is None:
        raise HTTPException(status_code=404, detail=f"Unknown sweep id '{run_id}'")
    if state.status == "running":
        raise HTTPException(
            status_code=409, detail=f"Sweep '{run_id}' is still running"
        )
    if state.status == "error":
        raise HTTPException(
            status_code=409, detail=f"Sweep '{run_id}' failed: {state.error}"
        )

    builder = SWEEP_FIGURES.get(name)
    if builder is None:
        raise HTTPException(status_code=404, detail=f"Unknown figure '{name}'")

    with _RENDER_LOCK:
        try:
            fig = builder(state.result, theme)
        except Exception as exc:
            _close_all_figures()
            raise HTTPException(
                status_code=404, detail=f"Figure '{name}' unavailable: {exc}"
            )
        if fig is None:
            raise HTTPException(
                status_code=404, detail=f"Figure '{name}' has no data for this sweep"
            )
        svg = _render_svg(fig)

    return Response(content=svg, media_type="image/svg+xml")


@router.get("/airfoil-sweep/{run_id}/candidates/{name}/figures/{fig_name}")
def get_candidate_mses_figure(
    run_id: str, name: str, fig_name: str, theme: Optional[str] = Query(default=None)
) -> Response:
    """Per-candidate MSES figure (Cp/Mach distribution, shock contours) --
    same real surface-pressure/flowfield data the main Run's Model Comparison
    tab shows for the optimized design's own root section, but for THIS
    screened candidate (populated by
    ``airfoil_screening._verify_candidate_mses`` only when it was MSES-
    verified). 404 if the candidate wasn't MSES-verified or has no data for
    this specific figure (e.g. no Mach-field dump for a converged-but-
    field-less solve)."""
    state = airfoil_sweep_runs.REGISTRY.get(run_id)
    if state is None:
        raise HTTPException(status_code=404, detail=f"Unknown sweep id '{run_id}'")
    if state.status == "running":
        raise HTTPException(
            status_code=409, detail=f"Sweep '{run_id}' is still running"
        )
    if state.status == "error":
        raise HTTPException(
            status_code=409, detail=f"Sweep '{run_id}' failed: {state.error}"
        )

    builder = _CANDIDATE_FIGURES.get(fig_name)
    if builder is None:
        raise HTTPException(status_code=404, detail=f"Unknown figure '{fig_name}'")

    candidate = next((c for c in state.result.candidates if c.name == name), None)
    if candidate is None:
        raise HTTPException(
            status_code=404, detail=f"Unknown candidate '{name}' in this sweep"
        )
    mses_pressure = candidate.mses_pressure
    if mses_pressure is None or mses_pressure.status != "ok":
        raise HTTPException(
            status_code=404, detail=f"No MSES pressure data for candidate '{name}'"
        )

    with _RENDER_LOCK:
        try:
            fig = builder(mses_pressure, theme)
        except Exception as exc:
            _close_all_figures()
            raise HTTPException(
                status_code=404, detail=f"Figure '{fig_name}' unavailable: {exc}"
            )
        svg = _render_svg(fig)

    return Response(content=svg, media_type="image/svg+xml")


@router.websocket("/airfoil-sweep/{run_id}/events")
async def stream_sweep_events(websocket: WebSocket, run_id: str) -> None:
    await websocket.accept()
    state = airfoil_sweep_runs.REGISTRY.get(run_id)
    if state is None:
        await websocket.send_json({"error": f"Unknown sweep id '{run_id}'"})
        await websocket.close()
        return

    def _next_event() -> object:
        try:
            return state.events.get(timeout=1.0)
        except queue.Empty:
            return _POLL_AGAIN

    loop = asyncio.get_running_loop()
    try:
        while True:
            message = await loop.run_in_executor(None, _next_event)
            if message is _POLL_AGAIN:
                if state.status == "running":
                    continue
                message = None  # finished while quiet
            if message is None:
                if state.status == "error":
                    await websocket.send_json(
                        {"done": True, "status": "error", "error": state.error}
                    )
                else:
                    await websocket.send_json({"done": True, "status": "done"})
                break
            await websocket.send_json({"done": False, "message": message})
    except WebSocketDisconnect:
        pass
    finally:
        try:
            await websocket.close()
        except RuntimeError:
            pass
