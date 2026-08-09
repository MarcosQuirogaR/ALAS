# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Chebyshev pseudospectral differentiation and integration matrices.

``chebyshev_data`` builds the cosine-spaced node vector ``x`` and the dense
``D``/``I`` operators SUAVE's mission segment solver uses to turn a segment's
differential equations into an algebraic system. ``N = 16`` is what the
segment solver actually asks for; ``N = 4`` and ``N = 8`` are included so the
Rust port is checked at more than one matrix size, and one run with
``integration=False`` confirms that branch leaves ``I`` unset.

Layout: ``x`` is a flat list of length ``N``. ``D`` and ``I`` are nested lists,
row-major -- ``D[i][j]`` is row ``i``, column ``j``, matching
``numpy.ndarray.tolist()`` -- so ``numpy.dot(D, f)[i]`` reads as row ``i``
dotted with ``f``. ``I`` is ``null`` for the ``integration=False`` case.
"""

from __future__ import annotations

import _framework
import numpy as np


def main() -> None:
    _framework.add_suave_to_path()
    from SUAVE.Methods.Utilities.Chebyshev import chebyshev_data  # noqa: PLC0415

    cases = []
    for n in (4, 8, 16):
        x, d, i = chebyshev_data(n, integration=True)
        cases.append(
            {
                "n": n,
                "integration": True,
                "x": x.tolist(),
                "D": d.tolist(),
                "I": i.tolist(),
            }
        )

    # Confirms the integration=False branch leaves I unset, rather than
    # trusting that reading the source once is enough.
    x, d, i = chebyshev_data(8, integration=False)
    if i is not None:
        raise SystemExit("chebyshev_data(integration=False) returned an I matrix")
    cases.append(
        {
            "n": 8,
            "integration": False,
            "x": x.tolist(),
            "D": d.tolist(),
            "I": None,
        }
    )

    # Sanity check on the construction itself: differentiating a constant
    # must give (approximately) zero, for every N generated. A mistake in the
    # fixture generator should fail here rather than surface as a mysterious
    # Rust-side disagreement.
    for case in cases:
        d = np.array(case["D"])
        residual = np.abs(d @ np.ones(case["n"])).max()
        if residual > 1e-10:
            raise SystemExit(
                f"N={case['n']}: D applied to a constant vector left a residual of {residual!r}"
            )

    _framework.write(
        "math",
        "chebyshev",
        {"cases": cases},
        description="SUAVE chebyshev_data: cosine-spaced nodes and the D/I pseudospectral operators",
    )


if __name__ == "__main__":
    main()
