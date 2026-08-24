# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""``alas-math::hybrd``: MINPACK's trust-region root finder, as SciPy drives it.

SUAVE converges every mission segment through
``SUAVE/Methods/Missions/Segments/converge_root.py``, which calls
``scipy.optimize.fsolve`` -- a wrapper over MINPACK-1's ``hybrd``, the modified
Powell hybrid method with a forward-difference Jacobian. The port has to
reproduce the *iteration path*, not merely the root: a segment that converges
to the same answer by a different sequence of trial points has a different
``nfev``, and the mission's reported fuel burn is a function of where the
iteration stopped.

**So what this fixture records is the log of every function evaluation, in
call order.** A root on its own is a weak check -- most root finders find it.
The call log pins the finite-difference probes, the trust-region trial points,
the rank-one Broyden updates that skip a Jacobian rebuild, and the
recalculation that ``ncfail == 2`` forces. A port that reproduces the whole
sequence has reproduced the algorithm; one that merely lands on the root has
not.

Every system here is closed-form algebra with no library calls, so the Rust
side evaluates bit-identical residuals and any divergence in the log is the
solver's rather than the function's.

The settings are the ones ``converge_root`` actually reaches, and each is
recorded as its *effective* value rather than the sentinel SUAVE passes:
``xtol`` is ``Numerics.tolerance_solution`` (1e-8); ``max_evaluations`` is
``0.``, which SciPy turns into ``200 * (n + 1)``; ``step_size`` is ``None``,
which SciPy turns into the machine epsilon; ``band`` is never set, so the
Jacobian is dense. ``factor`` and ``diag`` are left at SciPy's defaults,
100 and unset, because ``converge_root`` names neither.

Run under either environment -- this reaches SciPy and nothing else::

    & ".suave-venv/Scripts/python.exe" golden/generators/gen_math_hybrd.py
"""

from __future__ import annotations

import math

import numpy as np
import scipy.optimize

import _framework

# SciPy's own defaults for the two arguments ``converge_root`` does not name.
_FACTOR = 100.0
_MACHINE_EPS = float(np.finfo(np.float64).eps)

# ``Numerics.__defaults__.tolerance_solution``.
_XTOL = 1e-8


# ----------------------------------------------------------------------
#  The systems
# ----------------------------------------------------------------------
#
# Each is closed-form algebra over the unknown vector, so that the Rust
# parity test can evaluate the identical residual without a fixture lookup.


def _rosenbrock(x):
    return [10.0 * (x[1] - x[0] * x[0]), 1.0 - x[0]]


def _powell_singular(x):
    return [
        x[0] + 10.0 * x[1],
        math.sqrt(5.0) * (x[2] - x[3]),
        (x[1] - 2.0 * x[2]) ** 2,
        math.sqrt(10.0) * (x[0] - x[3]) ** 2,
    ]


def _helical_valley(x):
    if x[0] > 0.0:
        theta = math.atan(x[1] / x[0]) / (2.0 * math.pi)
    elif x[0] < 0.0:
        theta = math.atan(x[1] / x[0]) / (2.0 * math.pi) + 0.5
    else:
        theta = 0.25 if x[1] >= 0.0 else -0.25
    r = math.sqrt(x[0] * x[0] + x[1] * x[1])
    return [
        10.0 * (x[2] - 10.0 * theta),
        10.0 * (r - 1.0),
        x[2],
    ]


def _broyden_tridiagonal(x):
    n = len(x)
    out = []
    for i in range(n):
        left = x[i - 1] if i > 0 else 0.0
        right = x[i + 1] if i < n - 1 else 0.0
        out.append((3.0 - 2.0 * x[i]) * x[i] - left - 2.0 * right + 1.0)
    return out


def _trigonometric(x):
    n = len(x)
    total = sum(math.cos(v) for v in x)
    return [
        n - total + (i + 1) * (1.0 - math.cos(x[i])) - math.sin(x[i])
        for i in range(n)
    ]


def _force_balance(x):
    """A system shaped like the one a cruise segment actually solves.

    SUAVE's ``Constant_Speed_Constant_Altitude`` carries two unknown vectors
    over its sixteen control points -- ``throttle`` and ``body_angle`` -- and
    two residual rows per point, the horizontal and vertical force balance.
    That is a 32-unknown system whose blocks couple only through the shared
    trigonometry, which is a different sparsity pattern from any of the
    classical test problems above and therefore a different dogleg path.

    The thrust, lift and drag laws here are algebraic stand-ins, not the real
    propulsion and aerodynamic models: what is under test is the solver, and a
    residual the Rust side can evaluate exactly is what makes the call log a
    check on the solver alone.
    """
    n_points = len(x) // 2
    throttle = x[:n_points]
    alpha = x[n_points:]
    out = []
    for i in range(n_points):
        weight = 1.0 + 0.05 * i
        thrust = 0.6 * throttle[i] + 0.15 * throttle[i] * throttle[i]
        lift = 4.5 * alpha[i] + 0.25
        drag = 0.02 + 0.05 * lift * lift
        out.append(thrust * math.cos(alpha[i]) - drag)
        out.append(thrust * math.sin(alpha[i]) + lift - weight)
    return out


_SYSTEMS = {
    "rosenbrock": _rosenbrock,
    "powell_singular": _powell_singular,
    "helical_valley": _helical_valley,
    "broyden_tridiagonal": _broyden_tridiagonal,
    "trigonometric": _trigonometric,
    "force_balance": _force_balance,
}


# ----------------------------------------------------------------------
#  Cases
# ----------------------------------------------------------------------

_CASES = [
    {
        "name": "rosenbrock",
        "system": "rosenbrock",
        "x0": [-1.2, 1.0],
        "note": "The classic two-dimensional valley; converges in few iterations.",
    },
    {
        "name": "powell_singular",
        "system": "powell_singular",
        "x0": [3.0, -1.0, 0.0, 1.0],
        "note": (
            "Jacobian is singular at the root, so the dogleg spends its time on "
            "the scaled-gradient leg rather than the Gauss-Newton one."
        ),
    },
    {
        "name": "helical_valley",
        "system": "helical_valley",
        "x0": [-1.0, 0.0, 0.0],
        "note": "Reaches the branch-cut arm of the atan; a long curved path.",
    },
    {
        "name": "broyden_tridiagonal_10",
        "system": "broyden_tridiagonal",
        "x0": [-1.0] * 10,
        "note": "Banded Jacobian solved densely, as `band=None` forces.",
    },
    {
        "name": "trigonometric_8",
        "system": "trigonometric",
        "x0": [1.0 / 8.0] * 8,
        "note": "Dense Jacobian with every unknown in every row.",
    },
    {
        "name": "force_balance_32",
        "system": "force_balance",
        "x0": [0.5] * 16 + [1.0 * math.pi / 180.0] * 16,
        "note": (
            "The mission segment's own shape and starting guess: throttle 0.5 "
            "and body angle 1 degree over sixteen control points."
        ),
    },
    {
        "name": "rosenbrock_at_root",
        "system": "rosenbrock",
        "x0": [1.0, 1.0],
        "note": (
            "Starts exactly at the root. `fnorm == 0` on the first evaluation, "
            "so hybrd reports success from inside the first inner loop rather "
            "than through the `delta <= xtol*xnorm` test."
        ),
    },
    {
        "name": "powell_singular_maxfev",
        "system": "powell_singular",
        "x0": [3.0, -1.0, 0.0, 1.0],
        "maxfev": 12,
        "note": (
            "Evaluation budget exhausted before convergence: pins the `info = 2` "
            "exit and the state the solver returns from it, which a port that "
            "only ever tested converged cases would get wrong."
        ),
    },
]


def _run_case(case: dict) -> dict:
    system = _SYSTEMS[case["system"]]
    x0 = list(case["x0"])
    n = len(x0)

    maxfev = case.get("maxfev", 0)
    effective_maxfev = 200 * (n + 1) if maxfev == 0 else maxfev

    log: list[dict] = []

    def logged(x):
        residual = system(list(np.asarray(x, dtype=float)))
        log.append(
            {
                "x": [float(v) for v in x],
                "f": [float(v) for v in residual],
            }
        )
        return residual

    x, infodict, ier, msg = scipy.optimize.fsolve(
        logged,
        x0,
        xtol=_XTOL,
        maxfev=maxfev,
        epsfcn=None,
        factor=_FACTOR,
        full_output=1,
    )

    # SciPy calls the residual twice at `x0` before MINPACK ever runs -- once in
    # `_check_func`, to learn the output shape, and once inside the `_hybrd` C
    # wrapper, to size `fvec` before handing it to the Fortran. Neither is
    # counted in `nfev` and neither is part of the algorithm, so they are
    # dropped here rather than recorded: what the port has to reproduce is
    # MINPACK's call sequence, and a Rust caller has no shape to discover.
    # The equality below is what stops that offset being assumed rather than
    # checked -- if a future SciPy drops or adds a sizing call, this fails
    # instead of silently shifting every logged point by one.
    if len(log) != int(infodict["nfev"]) + 2:
        raise SystemExit(
            f"{case['name']}: logged {len(log)} evaluations but MINPACK reported "
            f"nfev={infodict['nfev']}; expected exactly two wrapper sizing calls"
        )
    for extra in log[:2]:
        if extra["x"] != [float(v) for v in x0]:
            raise SystemExit(
                f"{case['name']}: a leading wrapper call was not at x0, so the "
                "two dropped evaluations are not the sizing calls"
            )
    log = log[2:]

    return {
        "name": case["name"],
        "system": case["system"],
        "note": case["note"],
        "n": n,
        "x0": [float(v) for v in x0],
        "xtol": _XTOL,
        "maxfev": effective_maxfev,
        "epsfcn": _MACHINE_EPS,
        "factor": _FACTOR,
        "ier": int(ier),
        "message": msg.strip(),
        "nfev": int(infodict["nfev"]),
        "x": [float(v) for v in x],
        "fvec": [float(v) for v in infodict["fvec"]],
        "evaluations": log,
    }


def main() -> None:
    cases = [_run_case(case) for case in _CASES]

    # --- Sanity checks: a fixture that reached none of these branches would
    # --- agree with a port that implemented none of them.
    if not any(c["ier"] == 1 for c in cases):
        raise SystemExit("no case converged; the fixture proves nothing")
    if not any(c["ier"] != 1 for c in cases):
        raise SystemExit(
            "every case converged, so the non-convergence exit is unchecked; "
            "the `maxfev` case is supposed to fail"
        )
    if not any(c["n"] >= 30 for c in cases):
        raise SystemExit(
            "no case is the size of a real segment solve; the rank-one update "
            "path is only meaningfully exercised on a larger system"
        )

    # A case that converges without ever rebuilding the Jacobian would never
    # reach `r1updt`/`r1mpyq`. The dense Jacobian costs `n` evaluations, so a
    # run whose evaluation count exceeds what a pure Newton path would spend
    # is one that took at least one rank-one update.
    updated = [c for c in cases if c["ier"] == 1 and c["nfev"] > 2 * (c["n"] + 1)]
    if not updated:
        raise SystemExit(
            "no converged case took a rank-one Broyden update; `r1updt` and "
            "`r1mpyq` would be unchecked"
        )

    total_evaluations = sum(len(c["evaluations"]) for c in cases)
    for case in cases:
        print(
            f"  {case['name']:<24} n={case['n']:<3} ier={case['ier']} "
            f"nfev={case['nfev']:<4} |f|={max(abs(v) for v in case['fvec']):.3e}"
        )
    print(f"  {total_evaluations} logged evaluations in total")

    _framework.write(
        "math",
        "hybrd",
        {"cases": cases},
        description=(
            "MINPACK hybrd through scipy.optimize.fsolve at the settings "
            "SUAVE's converge_root reaches it with: eight systems from "
            "Rosenbrock to a 32-unknown segment-shaped force balance, each "
            "recording the full log of function evaluations in call order so "
            "the iteration path is checked and not only the root."
        ),
    )


if __name__ == "__main__":
    main()
