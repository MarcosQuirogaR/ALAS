# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
In-memory pipeline-run registry.

Runs :meth:`DesignPipeline.run`/``analyze_baseline`` off the request-handling
thread, stream progress strings, and hand back a result -- plain
``threading.Thread`` + a queue, since the sidecar has no GUI event loop of
its own (progress is streamed to the frontend over a WebSocket instead).

``PipelineResult``/``AnalysisReport`` hold non-JSON-serialisable objects
(``asb.Airplane``, numpy arrays, ``DesignVector``/``BaselineReport``
dataclasses). They are kept here, in Python memory, keyed by run id -- never
serialised wholesale to the frontend. ``routes_figures`` (added in Phase 2)
pulls the live objects back out of this registry to render matplotlib figures
on demand; ``summarize()`` below extracts only the small set of JSON-safe
scalars a first results view needs.
"""

from __future__ import annotations

import dataclasses
import math
import queue
import threading
import traceback
import uuid
from typing import TYPE_CHECKING, Any, Dict, Optional

if TYPE_CHECKING:
    # Runtime import happens inside the worker threads instead: DesignPipeline
    # transitively imports aerosandbox/casadi/scipy (~11s cold), and pulling
    # that in at module import made the whole sidecar (routes_pipeline ->
    # here) pay it before it could even announce its port. server.py warms it
    # in the background right after startup, so the first actual run doesn't
    # pay it either.
    from ..pipeline import PipelineResult


class RunState:
    """One in-flight or completed pipeline run."""

    def __init__(self, run_id: str) -> None:
        self.id = run_id
        self.status = "running"  # running | done | error
        self.error: Optional[str] = None
        self.result: Optional[PipelineResult] = None
        self.events: "queue.Queue[Optional[str]]" = queue.Queue()

    def emit(self, message: str) -> None:
        self.events.put(message)

    def finish_ok(self, result: PipelineResult) -> None:
        self.result = result
        self.status = "done"
        self.events.put(None)  # sentinel: no more events

    def finish_error(self, error: str) -> None:
        self.error = error
        self.status = "error"
        self.events.put(None)


class RunRegistry:
    # Each completed run pins its full PipelineResult (asb.Airplane, numpy
    # polars, mission columns) in memory so routes_figures can re-render from
    # the live objects; without a cap, a long GUI session grows without
    # bound. The frontend only ever renders figures for the latest run, so a
    # handful of retained results is plenty of headroom for any in-flight
    # figure requests against a just-superseded run.
    MAX_FINISHED_RUNS = 4

    def __init__(self) -> None:
        self._runs: Dict[str, RunState] = {}  # insertion-ordered (dict semantics)
        self._lock = threading.Lock()

    def _new_id(self) -> str:
        return uuid.uuid4().hex[:12]

    def get(self, run_id: str) -> Optional[RunState]:
        with self._lock:
            return self._runs.get(run_id)

    def _register(self, state: RunState) -> None:
        with self._lock:
            self._runs[state.id] = state
            finished = [s for s in self._runs.values() if s.status != "running"]
            for stale in finished[: max(0, len(finished) - self.MAX_FINISHED_RUNS)]:
                del self._runs[stale.id]

    def start_pipeline_run(
        self,
        config,
        *,
        optimize: bool,
        compare_baseline: bool,
        parallel: bool = True,
        initial_design: Optional[Dict[str, float]] = None,
        bounds: Optional[list] = None,
    ) -> str:
        state = RunState(self._new_id())
        self._register(state)

        def _worker() -> None:
            try:
                from ..config.design_variables import DesignVector
                from ..pipeline import DesignPipeline

                design = (
                    DesignVector(**initial_design)
                    if initial_design is not None
                    else None
                )
                pipeline = DesignPipeline(config)
                result = pipeline.run(
                    optimize=optimize,
                    compare_baseline=compare_baseline,
                    output_dir=None,
                    make_plots=False,
                    show_plots=False,
                    verbose=False,
                    parallel=parallel,
                    initial_design=design,
                    bounds=bounds,
                    progress_callback=state.emit,
                )
                state.finish_ok(result)
            except Exception:
                state.finish_error(traceback.format_exc())

        threading.Thread(
            target=_worker, daemon=True, name=f"alas-run-{state.id}"
        ).start()
        return state.id

    def start_baseline_run(self, config) -> str:
        state = RunState(self._new_id())
        self._register(state)

        def _worker() -> None:
            try:
                from ..pipeline import DesignPipeline

                pipeline = DesignPipeline(config)
                result = pipeline.analyze_baseline(progress_callback=state.emit)
                state.finish_ok(result)
            except Exception:
                state.finish_error(traceback.format_exc())

        threading.Thread(
            target=_worker, daemon=True, name=f"alas-baseline-{state.id}"
        ).start()
        return state.id


REGISTRY = RunRegistry()


def summarize(result: PipelineResult) -> Dict[str, Any]:
    """Reduce a :class:`PipelineResult` to a small JSON-safe summary dict.

    Deliberately not a full ``dataclasses.asdict`` -- ``AnalysisReport``/
    ``BaselineReport`` carry an ``asb.Airplane`` and numpy polar arrays that
    aren't JSON-safe and aren't needed by a results-summary view (figures,
    added in Phase 2, read the live objects straight out of the registry
    instead of round-tripping through JSON).
    """
    out: Dict[str, Any] = {
        "preset": result.config.preset,
        "optimized_design": dataclasses.asdict(result.optimized_design),
    }

    base = result.baseline_analysis
    if base is not None:
        out["baseline"] = {
            "status": base.status,
            "error": base.error,
            "static_margin": _safe_float(base.static_margin),
            "cg_pct_mac": _safe_float(base.cg_pct_mac),
            "np_pct_mac": _safe_float(base.np_pct_mac),
            "mac": _safe_float(base.mac),
            "physical_cg": list(base.physical_cg),
        }

    opt = result.optimized_report
    if opt is not None:
        out["optimized"] = {
            "static_margin": _safe_float(opt.static_margin),
            "x_neutral_point": _safe_float(opt.x_neutral_point),
            "cg_envelope_ok": opt.cg_envelope_ok,
            "component_masses": {
                k: _safe_float(v) for k, v in opt.component_masses.items()
            },
            "design_point": _safe_asdict(opt.design_point),
        }

    baseline_cmp = result.baseline_report
    if baseline_cmp is not None:
        out["baseline_comparison"] = {
            "static_margin": _safe_float(baseline_cmp.static_margin),
            "cg_envelope_ok": baseline_cmp.cg_envelope_ok,
            "design_point": _safe_asdict(baseline_cmp.design_point),
        }

    # Surface the *reason* alongside the status, not just the bare word. A
    # "not_configured"/"error" mission carries a message naming every path that
    # was checked, which is the only thing that makes a misconfigured or
    # failed-to-extract SUAVE runtime diagnosable, so it must be forwarded to
    # the UI rather than discarded once the status string is known.
    if result.mission_result is not None:
        out["mission_status"] = result.mission_result.status
        if result.mission_result.error:
            out["mission_error"] = result.mission_result.error
    if result.mses_result is not None:
        out["mses_status"] = result.mses_result.status
        if getattr(result.mses_result, "error", None):
            out["mses_error"] = result.mses_result.error
    if result.structural_result is not None:
        out["structural_status"] = result.structural_result.status
        if getattr(result.structural_result, "error", None):
            out["structural_error"] = result.structural_result.error

    return out


def _safe_float(v: Any) -> Optional[float]:
    try:
        f = float(v)
    except (TypeError, ValueError):
        return None
    # NaN/inf -> None: JSON has literals for neither, and Python's json
    # module would happily emit non-standard NaN/Infinity tokens that the
    # frontend's strict JSON.parse then chokes on.
    return f if math.isfinite(f) else None


def _safe_asdict(obj: Any) -> Any:
    if obj is None:
        return None
    if dataclasses.is_dataclass(obj):
        return {
            f.name: _safe_asdict(getattr(obj, f.name)) for f in dataclasses.fields(obj)
        }
    if isinstance(obj, float):
        return _safe_float(obj)
    if isinstance(obj, (list, tuple)):
        return [_safe_asdict(v) for v in obj]
    if isinstance(obj, dict):
        return {k: _safe_asdict(v) for k, v in obj.items()}
    return obj
