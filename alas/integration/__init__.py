# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
SUAVE mission-analysis integration.

This package runs in ALAS's main process (Python 3.10+, numpy 2.x) and
must never import SUAVE or anything from the isolated ``.suave-venv``
directly -- SUAVE 2.5.2 needs an old numpy/scipy/matplotlib stack that's
incompatible with ALAS's own. Instead, :mod:`suave_bridge` shells out to
that environment as a subprocess (see ``external tools/suave_runner``).
"""
