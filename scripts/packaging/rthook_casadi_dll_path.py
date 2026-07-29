# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""PyInstaller runtime hook.

casadi's SWIG extension module (_casadi.pyd) gets hoisted to the top level
of the frozen bundle by PyInstaller's binary classification (SWIG-wrapped
extensions expect to be importable as a bare top-level name -- casadi's own
pure-Python wrapper does a plain `import _casadi`, not `from . import
_casadi`), but its actual implementation -- libcasadi.dll and the
solver-plugin DLLs it loads -- stay nested under casadi/ alongside
casadi.py, since those aren't Python-importable modules themselves and
so aren't hoisted. Windows only searches the loading module's own
directory for a .pyd's dependency DLLs by default, so without this,
`import casadi` fails with "DLL load failed while importing _casadi: The
specified module could not be found" even though libcasadi.dll is present
one directory over.
"""

import os
import sys

if sys.platform == "win32":
    casadi_dir = os.path.join(sys._MEIPASS, "casadi")  # type: ignore[attr-defined]
    if os.path.isdir(casadi_dir):
        os.add_dll_directory(casadi_dir)
