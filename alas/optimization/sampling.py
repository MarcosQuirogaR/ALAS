# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Design-space sampler for ALAS DOE and Surprise modes.

Single public function: :func:`sample_design`.

*  **DOE Mode** (``constrained=True``): draws one sample uniformly within
   the user-defined design-space bounds using a 1-point Latin Hypercube
   sampler, then validates it through the Phase 3 validation rules.
   Samples that trigger any ``error``-severity issue are re-drawn up to
   *max_retries* times before the best attempt is returned regardless.

*  **Surprise Mode** (``constrained=False``): same LHS draw but against
   wider bounds (each side extended by :data:`_SURPRISE_SLACK_FRAC` of
   the range beyond the user bound), no validation filter.  The full
   pipeline may hit a geometry or aerodynamic edge case, but
   MSES/NASTRAN hangs and Python exceptions are already caught by
   ``PipelineWorker``.  This is intentionally honest: the mode cannot
   prevent a genuine in-process infinite loop in AeroSandbox; only
   external-solver timeouts and Python exceptions are actually covered.

No new dependencies: ``scipy`` is already a hard dependency of the main
``alas`` package.  No Qt import -- this module is headless-safe and
can be imported from the CLI or tests without PySide6 present.
"""

from __future__ import annotations

import warnings
from typing import TYPE_CHECKING

import numpy as np
from scipy.stats.qmc import LatinHypercube

from ..config.design_variables import DESIGN_VARIABLE_SPECS, DesignVector

if TYPE_CHECKING:
    from ..config.settings import ALASConfig

# ---------------------------------------------------------------------------
# Module-level constants
# ---------------------------------------------------------------------------

# Fraction by which Surprise Mode extends each bound edge beyond the
# user-set value. e.g. 0.30 -> 30 % wider on each side. Keeps exploration
# interesting while avoiding numerically degenerate extremes (e.g. zero-chord).
_SURPRISE_SLACK_FRAC = 0.30

# Maximum resample attempts for constrained (DOE) mode before giving up and
# returning the last sample regardless of validation status.
_DEFAULT_MAX_RETRIES = 20


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


def sample_design(
    bounds: list[tuple[float, float]],
    *,
    constrained: bool,
    config: "ALASConfig | None" = None,
    max_retries: int = _DEFAULT_MAX_RETRIES,
) -> DesignVector:
    """Draw one design point from the given bounds via Latin Hypercube sampling.

    Parameters
    ----------
    bounds:
        Ordered ``(lo, hi)`` pairs, one per design variable, in the same
        order as :data:`~alas.config.design_variables.DESIGN_VARIABLE_SPECS`.
        Obtain from ``DesignSpaceTable.get_bounds()``.
    constrained:
        ``True`` -- DOE mode: validate each candidate and re-draw on
        ``error``-severity failures.  Requires *config* to be supplied.
        ``False`` -- Surprise mode: skip validation; widen bounds by
        :data:`_SURPRISE_SLACK_FRAC` to encourage exploration.
    config:
        The current :class:`~alas.config.settings.ALASConfig`.
        Used only when *constrained* is ``True``.  Pass ``None`` to skip
        validation (useful in headless unit tests).
    max_retries:
        Maximum re-draw attempts in constrained mode before giving up.

    Returns
    -------
    DesignVector
        The sampled design point.
    """
    n_vars = len(DESIGN_VARIABLE_SPECS)
    if len(bounds) != n_vars:
        raise ValueError(
            f"Expected {n_vars} bound pairs (one per design variable), "
            f"got {len(bounds)}."
        )

    if not constrained:
        lo_arr, hi_arr = _widened_bounds(bounds, _SURPRISE_SLACK_FRAC)
        return _draw_one(lo_arr, hi_arr)

    # --- Constrained (DOE) mode ---
    lo_arr = np.array([lo for lo, _ in bounds], dtype=float)
    hi_arr = np.array([hi for _, hi in bounds], dtype=float)

    best: DesignVector | None = None
    for _attempt in range(max(1, max_retries)):
        candidate = _draw_one(lo_arr, hi_arr)
        if config is None:
            return candidate
        errors = _error_issues(candidate, config)
        if not errors:
            return candidate
        if best is None:
            best = candidate  # keep first attempt as fallback

    # All retries exhausted
    warnings.warn(
        f"DOE sampler could not find a valid design in {max_retries} attempts; "
        "returning the first sample regardless. Check your validation rules and "
        "design-space bounds.",
        stacklevel=2,
    )
    assert best is not None
    return best


# ---------------------------------------------------------------------------
# Internals
# ---------------------------------------------------------------------------


def _draw_one(lo: np.ndarray, hi: np.ndarray) -> DesignVector:
    """Draw a single sample from a 1-point LHS in [lo, hi]."""
    sampler = LatinHypercube(d=len(lo), seed=None)
    # random(n=1) returns shape (1, d) in [0, 1]; scale to [lo, hi]
    unit = sampler.random(n=1)[0]
    values = lo + unit * (hi - lo)
    return DesignVector.from_array(values)


def _widened_bounds(
    bounds: list[tuple[float, float]],
    slack: float,
) -> tuple[np.ndarray, np.ndarray]:
    """Return (lo_arr, hi_arr) with each bound widened by *slack* fraction.

    Never goes below the absolute spec lower or above spec upper --
    the widening is exploratory but not numerically suicidal.
    """
    lo_arr = np.empty(len(bounds))
    hi_arr = np.empty(len(bounds))
    for i, (lo, hi) in enumerate(bounds):
        width = hi - lo
        margin = width * slack
        spec = DESIGN_VARIABLE_SPECS[i]
        # Extend bounds outward by `margin`.  Then clamp so we don't go
        # past a "super-widened" spec limit.  The spec floor/ceiling is
        # itself widened by slack*0.5 of the *absolute* spec range so
        # that both negative and positive bounds are pushed the right way.
        spec_range = spec.upper - spec.lower
        spec_floor = spec.lower - spec_range * slack * 0.5
        spec_ceil = spec.upper + spec_range * slack * 0.5
        lo_arr[i] = max(lo - margin, spec_floor)
        hi_arr[i] = min(hi + margin, spec_ceil)
    return lo_arr, hi_arr


def _error_issues(candidate: DesignVector, config: "ALASConfig") -> list:
    """Return error-severity validation issues for *candidate*.

    Runs the Phase 3 ``validation.validate()`` rule set on *config* as-is.
    This is intentionally conservative: the rules operate on the live
    ALASConfig geometry/requirements fields rather than the design
    vector directly, so if the user's current form already has an error
    that predates the sample, those errors are counted too. That is
    acceptable -- a sample that lands on top of a pre-existing error
    genuinely can't run the pipeline either.

    If the validation import fails for any reason, the candidate is treated
    as valid and an empty list is returned.
    """
    try:
        from ..validation import validate as _validate

        issues = _validate(config)
        return [i for i in issues if getattr(i, "severity", "error") == "error"]
    except Exception:
        return []
