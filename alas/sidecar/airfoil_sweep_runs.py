# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
In-memory registry for airfoil-screening sweep runs.

Same threading.Thread + queue.Queue progress-streaming pattern as
``alas.sidecar.runs.RunRegistry`` (see that module's docstring for the
rationale), deliberately kept in its own small registry rather than folded
into the shared pipeline ``RunRegistry`` -- this is a purely optional,
additive feature, and keeping it structurally separate means it can never
affect the existing Run/Analyze-baseline path even by accident.
"""

from __future__ import annotations

import queue
import threading
import traceback
import uuid
from typing import TYPE_CHECKING, Dict, Optional

if TYPE_CHECKING:
    from ..analysis.airfoil_screening import AirfoilScreeningResult


class SweepState:
    """One in-flight or completed airfoil-sweep run."""

    def __init__(self, run_id: str) -> None:
        self.id = run_id
        self.status = "running"  # running | done | error
        self.error: Optional[str] = None
        self.result: Optional["AirfoilScreeningResult"] = None
        self.events: "queue.Queue[Optional[str]]" = queue.Queue()
        # Cooperative cancel flag: the screening loop polls should_cancel()
        # between candidates/stages and stops early with a partial result
        # (AirfoilScreeningResult.cancelled=True). A sweep over ~1600 airfoils
        # plus 3-D + MSES stages can run for minutes, so being able to stop it
        # is what keeps the tool interactive.
        self._cancel = threading.Event()

    def cancel(self) -> None:
        self._cancel.set()

    def should_cancel(self) -> bool:
        return self._cancel.is_set()

    def emit(self, message: str) -> None:
        self.events.put(message)

    def finish_ok(self, result: "AirfoilScreeningResult") -> None:
        self.result = result
        self.status = "done"
        self.events.put(None)  # sentinel: no more events

    def finish_error(self, error: str) -> None:
        self.error = error
        self.status = "error"
        self.events.put(None)


class SweepRegistry:
    # A full sweep result holds up to 100 candidates' worth of scalars --
    # tiny compared to a PipelineResult's live asb.Airplane/numpy polars, but
    # still capped so a long GUI session doesn't grow this unbounded.
    MAX_FINISHED = 4

    def __init__(self) -> None:
        self._runs: Dict[str, SweepState] = {}
        self._lock = threading.Lock()

    def _new_id(self) -> str:
        return uuid.uuid4().hex[:12]

    def get(self, run_id: str) -> Optional[SweepState]:
        with self._lock:
            return self._runs.get(run_id)

    def _register(self, state: SweepState) -> None:
        with self._lock:
            self._runs[state.id] = state
            finished = [s for s in self._runs.values() if s.status != "running"]
            for stale in finished[: max(0, len(finished) - self.MAX_FINISHED)]:
                del self._runs[stale.id]

    def start_sweep(
        self,
        config,
        design_vector=None,
        *,
        ld_weight: float = 0.7,
        fuel_weight: float = 0.3,
        robustness_weight: float = 0.0,
        cl_band: float = 0.05,
        top_n: int = 50,
        model_size: str = "large",
        alpha_min_deg: float = -4.0,
        alpha_max_deg: float = 14.0,
        alpha_step_deg: float = 0.5,
        min_tc: float = 0.005,
        max_tc: float = 0.25,
        name_filter: str = "",
        refine_3d: bool = True,
        refine_top_n: int = 20,
        min_static_margin: Optional[float] = None,
        verify_mses: bool = True,
        mses_top_n: int = 5,
    ) -> str:
        state = SweepState(self._new_id())
        self._register(state)

        def _worker() -> None:
            try:
                from ..analysis.airfoil_screening import run_airfoil_screening

                result = run_airfoil_screening(
                    config,
                    design_vector,
                    ld_weight=ld_weight,
                    fuel_weight=fuel_weight,
                    robustness_weight=robustness_weight,
                    cl_band=cl_band,
                    top_n=top_n,
                    model_size=model_size,
                    alpha_min_deg=alpha_min_deg,
                    alpha_max_deg=alpha_max_deg,
                    alpha_step_deg=alpha_step_deg,
                    min_tc=min_tc,
                    max_tc=max_tc,
                    name_filter=name_filter,
                    refine_3d=refine_3d,
                    refine_top_n=refine_top_n,
                    min_static_margin=min_static_margin,
                    verify_mses=verify_mses,
                    mses_top_n=mses_top_n,
                    progress_callback=state.emit,
                    should_cancel=state.should_cancel,
                )
                state.finish_ok(result)
            except Exception:
                state.finish_error(traceback.format_exc())

        threading.Thread(
            target=_worker, daemon=True, name=f"alas-airfoil-sweep-{state.id}"
        ).start()
        return state.id


REGISTRY = SweepRegistry()
