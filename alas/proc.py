# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Subprocess helpers for running console tools inside a *windowed* GUI app.

ALAS shells out to several console executables (MSES/MSET/MPLOT, the SUAVE
runner, NASTRAN, Patran). On Windows, a GUI process has no console of its own,
so every one of those spawns allocates a **new console window** that flashes on
screen, steals focus, and costs real time to create.

That is not a cosmetic problem at ALAS's call volumes: AeroSandbox's own
MSES wrapper issues 5+ ``subprocess.run(..., shell=True)`` calls per airfoil
evaluation (and ``shell=True`` adds a ``cmd.exe`` per call on top), so an
Airfoil Screening Stage-3 pass over ~20 candidates -- each with alpha-retry
attempts -- creates *hundreds* of console windows. Users see a storm of popping
terminals and a severe slowdown partway through the MSES stage.

Because most of those calls happen inside third-party code we don't own,
per-call fixes can't cover them. :func:`install_no_console_default` patches the
default **once**, process-wide, so any subprocess started without an explicit
console preference runs windowless. Our own call sites additionally pass
:func:`no_window_kwargs` so they're correct even without the global patch.

No-ops on non-Windows platforms (the flags simply don't exist there).
"""

from __future__ import annotations

import subprocess
import sys

# Only defined on Windows; guarded so this module imports cleanly everywhere.
_CREATE_NO_WINDOW = getattr(subprocess, "CREATE_NO_WINDOW", 0)

_installed = False


def is_windows() -> bool:
    return sys.platform == "win32"


def no_window_kwargs() -> dict:
    """``subprocess`` kwargs that suppress a console window for one call.

    Spread into a ``subprocess.run``/``Popen`` call:
    ``subprocess.run(cmd, **no_window_kwargs())``. Empty dict off Windows.
    """
    if not is_windows():
        return {}
    return {"creationflags": _CREATE_NO_WINDOW}


def install_no_console_default() -> bool:
    """Make ``CREATE_NO_WINDOW`` the process-wide default for new subprocesses.

    Wraps :meth:`subprocess.Popen.__init__` so any spawn that did **not**
    explicitly ask for console behaviour gets the windowless flag. Deliberately
    conservative -- it leaves a call alone when the caller already expressed an
    intent, so nothing that genuinely wants a console (or its own
    ``STARTUPINFO``) is overridden:

    * an explicit non-zero ``creationflags`` is respected as-is;
    * an explicit ``startupinfo`` is respected as-is;
    * flags that are mutually exclusive with ``CREATE_NO_WINDOW``
      (``CREATE_NEW_CONSOLE``/``DETACHED_PROCESS``) are never combined with it.

    Idempotent, and a no-op off Windows. Returns True if the patch is now
    active. Call once, early, from a GUI/sidecar entry point -- never from
    library code, since it changes global behaviour for the whole process.
    """
    global _installed
    if _installed or not is_windows():
        return _installed
    if not _CREATE_NO_WINDOW:
        return False

    create_new_console = getattr(subprocess, "CREATE_NEW_CONSOLE", 0)
    detached_process = getattr(subprocess, "DETACHED_PROCESS", 0)
    conflicting = create_new_console | detached_process
    original_init = subprocess.Popen.__init__

    def patched_init(self, *args, **kwargs):  # type: ignore[no-untyped-def]
        flags = kwargs.get("creationflags", 0)
        # `creationflags` can also arrive positionally; Popen's signature puts
        # it far down the list, so only the (overwhelmingly common) keyword
        # form is adjusted. A positional caller has clearly opted in anyway.
        if (
            not flags
            and kwargs.get("startupinfo") is None
            and not (flags & conflicting)
        ):
            kwargs["creationflags"] = flags | _CREATE_NO_WINDOW
        return original_init(self, *args, **kwargs)

    subprocess.Popen.__init__ = patched_init  # type: ignore[method-assign]
    _installed = True
    return True
