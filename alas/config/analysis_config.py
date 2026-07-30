# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Analysis configuration -- settings for the polar sweeps run during
optimization (fast) and for the final full analysis (fine).
"""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class AnalysisConfig:
    """Settings for AeroSandbox Vortex-Lattice polar sweeps."""

    # Final, high-fidelity sweep performed on the optimized aircraft.
    sweep_alpha_min_deg: float = field(
        default=-4.0,
        metadata={
            "label": "Polar sweep: alpha minimum",
            "unit": "deg",
            "help": "Lowest angle of attack evaluated in the final high-fidelity polar sweep.",
        },
    )
    sweep_alpha_max_deg: float = field(
        default=10.0,
        metadata={
            "label": "Polar sweep: alpha maximum",
            "unit": "deg",
            "help": "Highest angle of attack evaluated in the final high-fidelity polar sweep.",
        },
    )
    sweep_n_points: int = field(
        default=15,
        metadata={
            "label": "Polar sweep: number of points",
            "help": "How many angle-of-attack points to evaluate between the min/max alpha. "
            "More points = smoother drag polar, slower analysis. Part of the Fidelity preset.",
        },
    )

    spanwise_resolution: int = field(
        default=1,
        metadata={
            "label": "VLM spanwise panel resolution",
            "help": "Multiplier on each surface's built-in spanwise panel subdivision for the vortex-lattice solver. "
            "Higher = finer mesh, slower. Part of the Fidelity preset.",
        },
    )
    chordwise_resolution: int = field(
        default=1,
        metadata={
            "label": "VLM chordwise panel resolution",
            "help": "Multiplier on each surface's built-in chordwise panel subdivision for the vortex-lattice solver. "
            "Higher = finer mesh, slower. Part of the Fidelity preset. Used by the fast in-loop estimate; the "
            "final reported analysis uses fine_chordwise_resolution instead.",
        },
    )

    fine_spanwise_resolution: int = field(
        default=2,
        metadata={
            "label": "Fine VLM spanwise resolution (final analysis)",
            "help": "Spanwise panel resolution used ONLY for the once-per-run final/reported analysis (drag polar, "
            "trimmed cruise point, neutral point) -- not the optimizer loop. Higher fidelity where speed doesn't matter.",
        },
    )
    fine_chordwise_resolution: int = field(
        default=8,
        metadata={
            "label": "Fine VLM chordwise resolution (final analysis)",
            "help": "Chordwise panel resolution for the once-per-run final/reported analysis. A supercritical/cambered "
            "section needs ~8 chordwise panels for the VLM to resolve its camber line; at the coarse in-loop "
            "resolution the camber (and hence the zero-lift alpha) is under-captured, which inflates the reported "
            "cruise alpha by several degrees and under-predicts L/D by ~7%. Kept high here so the REPORTED "
            "cruise alpha (~1-4 deg, matching SUAVE) and L/D are physically accurate.",
        },
    )

    probe_alpha_low_deg: float = field(
        default=2.0,
        metadata={
            "label": "Fast-probe alpha (low)",
            "unit": "deg",
            "help": "Lower of a two-point alpha pair used for a fast in-loop lift-slope/stability estimate (not the full sweep).",
        },
    )
    probe_alpha_high_deg: float = field(
        default=3.0,
        metadata={
            "label": "Fast-probe alpha (high)",
            "unit": "deg",
            "help": "Higher of the two-point fast-probe alpha pair.",
        },
    )

    trim_incidence_probe_delta_deg: float = field(
        default=1.0,
        metadata={
            "label": "Trim-solve h-stab incidence probe delta",
            "unit": "deg",
            "help": "Small horizontal-stabilizer incidence perturbation used to estimate dCL/di_h "
            "and dCm/di_h for the closed-form longitudinal trim solve (alpha, tail incidence) "
            "run inside the optimizer loop and the final full analysis. Smaller values are "
            "more locally linear but noisier; 0.5-2 deg is typical for a small-perturbation "
            "VLM probe. See methods.md Sec 9f.",
        },
    )

    autobalance_velocity_m_s: float = field(
        default=250.0,
        metadata={
            "label": "Autobalance probe airspeed",
            "unit": "m/s",
            "help": "Airspeed used for the autobalance neutral-point probe (finds the CG that gives the target static margin).",
        },
    )
    autobalance_alpha_low_deg: float = field(
        default=0.0,
        metadata={
            "label": "Autobalance probe alpha (low)",
            "unit": "deg",
            "help": "Lower alpha of the two-point pair used to estimate the Cm-Cl slope during autobalance.",
        },
    )
    autobalance_alpha_high_deg: float = field(
        default=2.0,
        metadata={
            "label": "Autobalance probe alpha (high)",
            "unit": "deg",
            "help": "Higher alpha of the two-point pair used to estimate the Cm-Cl slope during autobalance.",
        },
    )

    # -- Neutral-point model (physically-anchored stability) -----------------
    tail_efficiency: float = field(
        default=0.90,
        metadata={
            "label": "Tail dynamic-pressure efficiency (eta_t)",
            "help": "Ratio of the tail's local dynamic pressure to freestream (the tail sits in the wing wake / fuselage "
            "boundary layer). Standard range 0.85-0.95; lower it to make the tail less stabilising (NP moves forward).",
        },
    )
    include_fuselage_stability: bool = field(
        default=True,
        metadata={
            "label": "Include fuselage destabilising effect",
            "help": "Whether to include the geometry-driven fuselage (Munk/Multhopp) destabilising contribution, "
            "which moves the neutral point forward.",
        },
    )

    # CL window for fitting the parabolic drag polar CD = CD0 + k*CL^2.
    polar_fit_cl_min: float = field(
        default=0.3,
        metadata={
            "label": "Drag-polar fit window: CL minimum",
            "help": "Lower CL bound of the window used to fit the parabolic drag polar (CD = CD0 + k*CL^2). "
            "Points outside the window are excluded so stall/pre-stall regions don't bias the fit.",
        },
    )
    polar_fit_cl_max: float = field(
        default=0.6,
        metadata={
            "label": "Drag-polar fit window: CL maximum",
            "help": "Upper CL bound of the primary drag-polar fit window.",
        },
    )
    polar_fit_cl_min_fallback: float = field(
        default=0.1,
        metadata={
            "label": "Drag-polar fit fallback window: CL minimum",
            "help": "Wider fallback lower CL bound used when the primary fit window captures fewer than 3 points.",
        },
    )
    polar_fit_cl_max_fallback: float = field(
        default=0.8,
        metadata={
            "label": "Drag-polar fit fallback window: CL maximum",
            "help": "Wider fallback upper CL bound used when the primary fit window captures fewer than 3 points.",
        },
    )
