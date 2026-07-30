# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""MSES 2-D airfoil analysis configuration.

MSES (Mark Drela, MIT) solves the coupled viscous/inviscid Euler + integral
boundary-layer equations for a 2-D airfoil section -- a fundamentally
different model from AeroSandbox's inviscid VLM + empirical parasite/wave
drag (3-D, whole-aircraft, but viscous effects are correlation-based) and
SUAVE's mission-level aerodynamics (3-D, trajectory-integrated). Running it on
the optimized design's actual root airfoil section lets the Model Comparison
tab show what a real coupled viscous-compressible solve captures that the
other two don't (true transition location, separation, shock-induced drag) --
see :mod:`alas.physics.mses_analysis`.

Runs via AeroSandbox's own ``aerodynamics.aero_2D.MSES`` wrapper, which drives
the native ``mset``/``mses``/``mplot`` executables as subprocesses (piping
menu keystrokes via stdin) -- no isolated venv needed (unlike SUAVE): MSES is
a compiled external tool, not a Python import, so there's no numpy/scipy
version conflict with ALAS's own environment.
"""

from __future__ import annotations
from dataclasses import dataclass, field


@dataclass
class MSESConfig:
    """MSES 2-D section-analysis settings, editable in Advanced Settings -> MSES Analysis."""

    # Enable + install directory moved to Setup > External Tools (a single
    # consolidated page for every external-tool integration).
    enabled: bool = field(
        default=True,
        metadata={
            "label": "Enabled",
            "help": "Run an MSES 2-D polar sweep on the optimized design's root airfoil section as part of a normal "
            "Run, populating the Model Comparison tab. On by default. Set on Setup > External Tools.",
            "hide_in_form": True,
        },
    )
    mses_dir: str = field(
        default="external tools/MSES",
        metadata={
            "label": "MSES executables directory",
            "help": "Path (repo-root-relative or absolute) to the folder containing mset.exe/mses.exe/mplot.exe. "
            "MSES is licensed separately by MIT and is not distributed with ALAS -- obtain it "
            "yourself and point this at your own install. Without it the Model Comparison tab simply "
            "omits the MSES column. Set on Setup > External Tools.",
            "hide_in_form": True,
        },
    )
    timeout_mset_s: float = field(
        default=30.0,
        metadata={
            "label": "MSET timeout",
            "unit": "s",
            "help": "Max time allowed for one mesh-generation (mset) call before it's killed.",
        },
    )
    timeout_mses_s: float = field(
        default=60.0,
        metadata={
            "label": "MSES timeout",
            "unit": "s",
            "help": "Max time allowed for one flow-solve (mses) call before it's killed.",
        },
    )
    max_iterations: int = field(
        default=100,
        metadata={
            "label": "Max solver iterations",
            "help": "Newton iteration cap per angle of attack -- MSES reports non-convergence rather than looping "
            "forever, but a hard cap keeps a single stubborn point from stalling the whole sweep.",
        },
    )
    n_crit: float = field(
        default=9.0,
        metadata={
            "label": "Transition N-crit",
            "help": "e^N transition-prediction critical amplification factor. 9.0 is the standard sea-level-cruise "
            "default (Drela); lower values (e.g. 4-5) predict earlier transition, appropriate for a "
            "high-turbulence/rough-surface environment.",
        },
    )
    xtr_upper: float = field(
        default=1.0,
        metadata={
            "label": "Forced transition x/c, upper surface",
            "help": "Force transition at this upper-surface x/c instead of letting MSES predict it. 1.0 = free "
            "(natural) transition, the realistic default for a clean cruise wing.",
        },
    )
    xtr_lower: float = field(
        default=1.0,
        metadata={
            "label": "Forced transition x/c, lower surface",
            "help": "Force transition at this lower-surface x/c. 1.0 = free (natural) transition.",
        },
    )
    alpha_sweep_halfwidth_deg: float = field(
        default=3.0,
        metadata={
            "label": "Alpha sweep half-width around the trimmed design point",
            "unit": "deg",
            "help": "The MSES polar sweeps [trim_alpha - this, trim_alpha + this] so the comparison brackets the "
            "actual cruise operating point, not an arbitrary fixed range.",
        },
    )
    alpha_sweep_n_points: int = field(
        default=7,
        metadata={
            "label": "Alpha sweep point count",
            "help": "Number of alpha points in the MSES polar sweep. Kept small relative to AeroSandbox's own VLM "
            "sweep (analysis.sweep_n_points) since each MSES point is a real viscous-compressible solve "
            "(~1-2s) rather than a linear-algebra VLM solve.",
        },
    )
    mset_n: int = field(
        default=141,
        metadata={
            "label": "MSET panel count (n)",
            "help": "Number of surface panel nodes MSET generates for the airfoil mesh.",
        },
    )
    mset_e: float = field(
        default=0.4,
        metadata={
            "label": "MSET grid density exponent (e)",
            "help": "Streamwise grid stretching parameter for the MSET mesh -- larger values cluster more points "
            "near the airfoil.",
        },
    )
