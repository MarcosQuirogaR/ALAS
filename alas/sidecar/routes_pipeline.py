# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Pipeline execution endpoints: start a run, stream its progress, fetch its
summary.

Mirrors ``DesignPipeline.run()``/``analyze_baseline()`` -- the exact same
calls ``cli.py`` and ``gui/worker.py::PipelineWorker`` already make -- via a
start/poll/stream split instead of a single blocking call, since a run can
take from milliseconds (``optimize=False``) to minutes (a full differential-
evolution search + SUAVE mission analysis).
"""

from __future__ import annotations

import asyncio
import queue
from typing import List, Optional, Tuple

from fastapi import APIRouter, HTTPException, WebSocket, WebSocketDisconnect
from pydantic import BaseModel

from ..config.settings import ALASConfig
from . import runs

router = APIRouter()

# Distinct from None (the queue's own end-of-run sentinel): "the 1s poll
# timed out with nothing to deliver" -- see stream_events.
_POLL_AGAIN = object()


class RunRequest(BaseModel):
    config: dict = {}
    optimize: bool = True
    compare_baseline: bool = True
    parallel: bool = True
    # DesignVector-shaped dict (see DesignSpaceTable.tsx's get_initial_design
    # equivalent) and (lower, upper) pairs in DESIGN_VARIABLE_SPECS order
    # (its get_bounds equivalent). Both optional -- omitting them falls back
    # to DesignPipeline.run()'s own preset/default nominal design and the
    # specs' own default bounds, exactly like the CLI/Qt app's un-edited path.
    initial_design: Optional[dict] = None
    bounds: Optional[List[Tuple[float, float]]] = None


class BaselineRequest(BaseModel):
    config: dict = {}


def _build_config(data: dict) -> ALASConfig:
    # TypeError/ValueError cover malformed values (e.g. a string mid-edit
    # where a number belongs), not just KeyError's unknown-key case -- all
    # deserve a 422, not an unhandled 500.
    try:
        return ALASConfig.from_dict(data)
    except (KeyError, TypeError, ValueError) as exc:
        raise HTTPException(status_code=422, detail=str(exc))


@router.post("/pipeline/run")
def start_run(req: RunRequest) -> dict:
    config = _build_config(req.config)
    run_id = runs.REGISTRY.start_pipeline_run(
        config,
        optimize=req.optimize,
        compare_baseline=req.compare_baseline,
        parallel=req.parallel,
        initial_design=req.initial_design,
        bounds=req.bounds,
    )
    return {"run_id": run_id}


@router.post("/pipeline/baseline")
def start_baseline(req: BaselineRequest) -> dict:
    config = _build_config(req.config)
    run_id = runs.REGISTRY.start_baseline_run(config)
    return {"run_id": run_id}


@router.get("/pipeline/{run_id}/result")
def get_result(run_id: str) -> dict:
    state = runs.REGISTRY.get(run_id)
    if state is None:
        raise HTTPException(status_code=404, detail=f"Unknown run id '{run_id}'")
    if state.status == "running":
        return {"status": "running"}
    if state.status == "error":
        return {"status": "error", "error": state.error}
    return {"status": "done", "result": runs.summarize(state.result)}


@router.websocket("/pipeline/{run_id}/events")
async def stream_events(websocket: WebSocket, run_id: str) -> None:
    """Streams the same human-readable progress strings
    ``DesignPipeline``'s ``progress_callback`` already emits, one JSON text
    frame per message, ending with ``{"done": true, ...}``.

    The queue is polled with a short timeout rather than blocked on
    indefinitely: an unbounded ``events.get`` parks an executor thread until
    the *next* event even after this coroutine is cancelled (client
    disconnect), and that orphaned get also swallows the end-of-run sentinel
    -- so a client that reconnected mid-run would wait forever for a "done"
    that was already consumed. Checking ``state.status`` on each timeout
    makes run completion observable without depending on exactly-once
    sentinel delivery.
    """
    await websocket.accept()
    state = runs.REGISTRY.get(run_id)
    if state is None:
        await websocket.send_json({"error": f"Unknown run id '{run_id}'"})
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
                message = (
                    None  # finished while quiet; sentinel already consumed or pending
                )
            if message is None:  # run finished
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
            pass  # already closed
