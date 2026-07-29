# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Compatibility shim that must be imported before SUAVE.

SUAVE 2.5.2's vendored ``pint`` plugin (``SUAVE/Plugins/pint``) imports ABC
classes straight out of ``collections`` (``from collections import
MutableMapping``), a path Python removed in 3.10 (it lived in
``collections.abc`` since 3.3 and the deprecated alias was dropped in 3.10).
Re-aliasing the names here -- before SUAVE is imported -- avoids patching the
vendored third-party source.
"""

import collections
import collections.abc
import sys
import warnings
from pathlib import Path

for _name in ("MutableMapping", "Mapping", "Iterable", "Sequence", "Callable", "Hashable", "Set"):
    if not hasattr(collections, _name):
        setattr(collections, _name, getattr(collections.abc, _name))

# `external tools/suave_runner/_compat.py` -> `external tools/SUAVE-2.5.2/trunk`
_SUAVE_TRUNK = Path(__file__).resolve().parents[1] / "SUAVE-2.5.2" / "trunk"
if str(_SUAVE_TRUNK) not in sys.path:
    sys.path.insert(0, str(_SUAVE_TRUNK))

# The vendored pint plugin also imports pkg_resources, which emits a
# deprecation warning under modern setuptools; silence it so it doesn't
# pollute the JSON-over-stdio channel with noise.
warnings.filterwarnings("ignore", category=UserWarning, module="pkg_resources")
