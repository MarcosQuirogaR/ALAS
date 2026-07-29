# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""numpy 2.x compatibility shim for pyNastran, imported first by any module
that touches ``pyNastran`` (:mod:`alas.geometry.wing_mesh_bdf`,
:mod:`alas.integration.nastran_runner`).

Same idea as ``external tools/suave_runner/_compat.py`` for SUAVE: a
third-party library written against an older numpy still imports fine at
runtime with a newer one except for the handful of renamed/removed APIs it
actually calls. pyNastran 1.4.x's own package metadata pins ``numpy<2``, but
only a small, growing set of calls actually break against numpy>=2:

* ``np.in1d`` (removed in numpy 2.0, renamed to ``np.isin`` -- same values,
  same signature for our usage).
* ``np.chararray`` (its top-level alias was dropped in this environment's
  numpy 2.5.1, confirmed directly: reading a SOL 111 vibration ``.op2`` hits
  ``table_object.py``'s ``self.gridtype_str = np.chararray((nnodes),
  unicode=True)`` and raises ``AttributeError: module 'numpy' has no
  attribute 'chararray'``. The class itself isn't gone -- it moved to
  ``np.char.chararray`` -- so re-exposing it at the old name is a same-
  behavior fix, not a reimplementation).

Patching these names here is far less invasive than downgrading numpy for
the whole app (which could break AeroSandbox) or standing up a second
isolated venv (SUAVE's much deeper incompatibility genuinely needed that;
pyNastran's does not).

No-ops harmlessly if numpy is already <2 (both names still exist there) or
if a future pyNastran release drops the calls -- the ``hasattr`` guards mean
this shim never overwrites a real ``np.in1d``/``np.chararray``.
"""

import numpy as np

if not hasattr(np, "in1d"):
    np.in1d = np.isin

if not hasattr(np, "chararray"):
    np.chararray = np.char.chararray
