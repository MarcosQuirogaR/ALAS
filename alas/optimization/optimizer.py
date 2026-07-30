# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Design optimizer.

Drives SciPy's ``differential_evolution`` (a global, gradient-free evolutionary
solver) over the named design space to minimise the :class:`DesignObjective`.
Returns a structured result holding the winning design as a
:class:`DesignVector`, plus the full evaluation history for diagnostics.
"""

from __future__ import annotations

import time
from dataclasses import dataclass
from typing import Optional

import numpy as np
from scipy.optimize import differential_evolution

from ..config.design_variables import DesignVector
from ..config.settings import ALASConfig
from .objective import DesignObjective, OptimizationHistory


@dataclass
class OptimizationResult:
    """Outcome of a design optimization run."""

    best_design: DesignVector
    best_cost: float
    history: OptimizationHistory
    wall_time_s: float
    raw_result: object  # the SciPy OptimizeResult, for advanced inspection


class DesignOptimizer:
    """Searches the design space for the configuration that best meets the
    user's requirements."""

    def __init__(self, config: ALASConfig):
        self.config = config

    def run(
        self,
        verbose: bool = True,
        bounds=None,
        progress_callback=None,
        initial_design: Optional[DesignVector] = None,
    ) -> OptimizationResult:
        solver = self.config.optimizer.solver
        objective = DesignObjective(self.config)
        # Bounds default to the named design-space specs but may be overridden
        # per-run (e.g. a GUI design-space editor) as long as the length matches.
        bounds = bounds if bounds is not None else DesignVector.bounds()
        n_expected = len(DesignVector.bounds())
        if len(bounds) != n_expected:
            raise ValueError(f"Expected {n_expected} bound pairs, got {len(bounds)}.")

        # Seed the initial population as a tight cluster of small perturbations
        # around the initial/preset design, rather than SciPy's default
        # full-space latin-hypercube coverage. Guarantees a known-valid,
        # physically-balanced design is in generation 0 -- per SciPy's own
        # documentation, an array `init` "could be used ... to create a tight
        # bunch of initial guesses in a location where the solution is known
        # to exist, thereby reducing time for convergence." Without this, a
        # uniform-random population routinely stretches the fuselage (or
        # otherwise moves the CG) far more than the wing/tail-shift DOFs
        # compensate for in the same candidate, so most/all of generation 0
        # (and often the whole run) can end up physically invalid even though
        # a compliant, high-L/D region is reachable.
        init = "latinhypercube"
        if solver.seed_near_initial_design and initial_design is not None:
            n_dof = len(bounds)
            lo = np.array([b[0] for b in bounds])
            hi = np.array([b[1] for b in bounds])
            x0_vec = initial_design.to_array()
            # Guard: if the initial design falls outside the given bounds (e.g.
            # a preset's own dimensions evaluated against unrelated bounds --
            # this happens easily since DESIGN_VARIABLE_SPECS' bounds are one
            # global default, not per-preset; the GUI narrows bounds to +/-10%
            # around whichever preset is loaded, but any other caller might
            # not), silently clipping x0 into range would distort it into a
            # different, unverified design and defeat the entire point of
            # seeding near a *known-valid* point. Fall back to ordinary
            # full-space sampling instead of seeding a corrupted point.
            if np.any(x0_vec < lo) or np.any(x0_vec > hi):
                if verbose:
                    print(
                        "  ! initial_design falls outside the optimizer bounds -- "
                        "skipping seed_near_initial_design (falling back to latinhypercube)."
                    )
            else:
                pop_size = solver.population_size * n_dof
                rng = np.random.default_rng(solver.seed)
                span = hi - lo
                jitter = (
                    rng.uniform(-1.0, 1.0, size=(pop_size, n_dof))
                    * span
                    * solver.seed_perturbation_fraction
                )
                init = np.clip(x0_vec + jitter, lo, hi)
                init[0] = (
                    x0_vec  # guarantee the unperturbed initial design is in generation 0
                )

        last_valid_count = [0]

        def callback(intermediate_result):
            h = objective.history
            last_valid_count[0] = h.n_valid
            msg = (
                f"generation complete | "
                f"valid: {h.n_valid}/{h.n_evaluations} total | "
                f"best L/D so far: {max(h.l_over_d, default=0.0):.2f}"
            )
            if verbose:
                print(f"  > {msg}")
            if progress_callback is not None:
                progress_callback(msg)

        if verbose:
            req = self.config.requirements
            print(
                f"--- ALAS optimization "
                f"(M{req.cruise_mach}, MTOW {req.mtow_kg / 1e3:.0f} t) ---"
            )

        t0 = time.time()
        result = differential_evolution(
            objective,
            bounds,
            strategy=solver.strategy,
            maxiter=solver.max_iterations,
            popsize=solver.population_size,  # ignored by SciPy when `init` is a custom array
            tol=solver.tolerance,
            seed=solver.seed,
            workers=solver.workers,
            callback=callback,
            disp=solver.display_progress,
            polish=False,  # skip L-BFGS-B polish; each eval is 2 VLM solves
            init=init,
        )
        wall_time = time.time() - t0

        if verbose:
            print(
                f"--- optimization done in {wall_time:.1f}s "
                f"({objective.history.n_valid} valid evaluations) ---"
            )

        # Apply the cabin preset one last time on the best design to ensure
        # the config reflects the optimized design's actual capacity.
        from ..physics.payload import apply_cabin_preset

        apply_cabin_preset(self.config, DesignVector.from_array(result.x))

        return OptimizationResult(
            best_design=DesignVector.from_array(result.x),
            best_cost=float(result.fun),
            history=objective.history,
            wall_time_s=wall_time,
            raw_result=result,
        )
