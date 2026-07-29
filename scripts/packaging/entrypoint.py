# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""PyInstaller entry point for the ALAS sidecar.

Exists only so PyInstaller's Analysis has a plain top-level script to
freeze. alas/sidecar/server.py uses package-relative imports (``from
.routes_config import ...`` etc.), which break if it's frozen directly as
the entry script: PyInstaller runs the entry script as ``__main__`` with no
package context, so those relative imports fail at startup with
"attempted relative import with no known parent package". Importing
``alas.sidecar.server`` as a real submodule here -- rather than
running it as ``__main__`` -- is what lets its relative imports resolve
normally; this file is the only thing that runs as ``__main__``.
"""

import multiprocessing

from alas.sidecar.server import main

if __name__ == "__main__":
    # Required for ANY frozen (PyInstaller) executable that uses the
    # `multiprocessing` module on Windows -- and the optimizer does, via
    # SciPy's `differential_evolution(..., workers=N)` for N>1 (every solver
    # preset except a hand-edited one sets workers=4). Windows has no fork(),
    # so multiprocessing "spawns" a worker by re-launching this very
    # executable with a special marker in sys.argv that tells it "you are a
    # worker, not the real app -- just run the pickled task and exit".
    # freeze_support() is what actually checks for that marker and short-
    # circuits into the worker bootstrap; without it, a frozen exe can't
    # tell the difference, so every spawned "worker" fell through to this
    # same `if __name__ == "__main__": main()` and re-ran the WHOLE sidecar
    # from scratch -- re-binding a new port, re-printing ALAS_PORT=...,
    # and starting a second (third, fourth...) full uvicorn server nobody
    # asked for: the visible symptom is several ALAS_PORT= lines printed
    # mid-run and an optimization that runs many times slower while never
    # printing a single "generation complete", since each spawned worker is
    # running the whole optimizer instead of the one pickled task it was
    # meant to. Must be the first call here, before `main()` or anything
    # else that could have side effects.
    multiprocessing.freeze_support()
    main()
