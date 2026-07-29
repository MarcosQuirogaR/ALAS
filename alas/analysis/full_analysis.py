# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Full analysis -- the final pipeline stage.

Takes the optimizer's winning design vector and performs a complete, engines-on
AeroSandbox evaluation: rebuild geometry, autobalance, run the fine polar sweep,
locate the cruise design point, and fit clean drag-polar parameters
(CD0, k, Oswald e). The result is a structured :class:`AnalysisReport` consumed
by the reporting layer (plots, JSON export for SUAVE handoff, console summary).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Dict, List, Optional

import aerosandbox as asb
import aerosandbox.numpy as np

from ..config.design_variables import DesignVector
from ..config.settings import ALASConfig
from ..geometry.aircraft_builder import AircraftBuilder
from ..physics.aerodynamics import AeroAnalysis
from ..physics.stability import neutral_point


@dataclass
class DesignPoint:
    """Trimmed cruise design-point performance."""

    alpha_deg: float
    cl: float
    cd: float
    l_over_d: float


@dataclass
class PolarFit:
    """Parabolic drag-polar fit CD = CD0 + k * CL^2."""

    cd0: float
    k: float
    oswald_e: float
    aspect_ratio: float


@dataclass
class TrimmedDesignPoint:
    """The genuinely trimmed cruise condition -- alpha AND horizontal-
    stabilizer incidence solved jointly for CL=CL_required and Cm=0 (see
    methods.md Sec 9f). Distinct from :class:`DesignPoint`, which is
    the nearest-CL sample from the untrimmed alpha sweep (h-stab at its
    default incidence) -- kept for continuity with the existing drag-polar
    fit/figures, which intentionally sweep alpha only."""

    alpha_deg: float
    trim_ih_deg: float
    cl: float
    cd: float
    l_over_d: float
    cm_residual: float  # real (nonlinear) Cm at the trimmed point; ~0 confirms the closed-form solve


@dataclass
class AnalysisReport:
    """Everything produced by a full analysis of one design."""

    design: DesignVector
    airplane: asb.Airplane
    polar: Dict[str, np.ndarray]
    design_point: DesignPoint
    polar_fit: PolarFit
    static_margin: float
    x_neutral_point: float = 0.0
    geometry_summary: Dict[str, float] = field(default_factory=dict)
    component_masses: Dict[str, float] = field(default_factory=dict)
    mass_coordinates: Dict[str, List[float]] = field(default_factory=dict)
    physical_cg: List[float] = field(default_factory=list)
    payload_layout: object = None  # PayloadLayout (detailed cargo/passenger interior)
    trimmed_design_point: Optional[TrimmedDesignPoint] = None
    cg_envelope_ok: Optional[bool] = None


class FullAnalysis:
    """Runs the complete high-fidelity evaluation of a final design."""

    def __init__(self, config: ALASConfig):
        self.config = config
        self.builder = AircraftBuilder(config.geometry)

    def run(
        self, design: DesignVector, include_engines: bool = True, verbose: bool = True
    ) -> AnalysisReport:
        req = self.config.requirements
        if verbose:
            print("\n--- Full analysis of optimized design ---")

        plane = self.builder.build(design, include_engines=include_engines)

        # Weight and balance FIRST: the physical CG anchors the stability
        # reference, rather than an invented aerodynamic balance CG. First pass
        # with the lumped payload to establish OEW, then rebuild with the detailed
        # cargo/passenger layout so the reported payload CG is physically realistic.
        from ..physics.mass import run_mass_analysis

        masses, coords, cg = run_mass_analysis(
            plane, req, self.config.geometry, self.config.mass_model
        )
        payload_layout = None
        try:
            from ..physics.payload import build_payload_layout, oew_and_cg

            oew, x_oew = oew_and_cg(masses, coords)
            payload_layout = build_payload_layout(plane, self.config, oew, x_oew)
            masses, coords, cg = run_mass_analysis(
                plane,
                req,
                self.config.geometry,
                self.config.mass_model,
                payload_layout=payload_layout,
            )
        except Exception:
            payload_layout = None  # non-fatal: fall back to the lumped payload

        # Anchor the aerodynamic moment reference to the actual mass CG.
        plane.xyz_ref[0] = float(cg[0])

        # Fine VLM resolution for the once-per-run reported analysis. The
        # in-loop config keeps a coarse chordwise mesh for speed, but that
        # under-resolves a supercritical camber line -- inflating the reported
        # cruise alpha by several degrees and under-predicting L/D by ~7%. The
        # final polar/trim/neutral-point pass uses the fine resolution so the
        # REPORTED numbers are physically accurate (see AnalysisConfig).
        import dataclasses as _dc

        fine_analysis = _dc.replace(
            self.config.analysis,
            spanwise_resolution=self.config.analysis.fine_spanwise_resolution,
            chordwise_resolution=self.config.analysis.fine_chordwise_resolution,
        )

        aero = AeroAnalysis(
            plane,
            sweep_deg=design.sweep_deg,
            geometry=self.config.geometry,
            drag_model=self.config.drag_model,
            analysis=fine_analysis,
        )
        if verbose:
            print(
                f"  > running polar sweep ({self.config.analysis.sweep_n_points} points)..."
            )
        polar = aero.run_sweep(req.cruise_mach, req.cruise_altitude_m)

        design_point = self._design_point(plane, polar)
        polar_fit = self._fit_polar(plane, polar)
        # Neutral point (wing+tail VLM + tail efficiency + fuselage), about the CG.
        x_np, sm, _cl_alpha = neutral_point(plane, fine_analysis)

        # Same CG-envelope check the optimizer enforces (objective.py's
        # _check_cg_envelope), run here purely for reporting: lets the
        # comparison plots explain why a raw-L/D-better baseline can still
        # lose to a lower-L/D optimized design -- the optimizer trades L/D
        # for CG-envelope/static-margin compliance, which a bare polar
        # overlay doesn't show.
        cg_envelope_ok = None
        try:
            from ..optimization.objective import _check_cg_envelope

            violation, _ = _check_cg_envelope(
                plane,
                masses,
                coords,
                float(cg[0]),
                x_np,
                plane.c_ref,
                self.config,
            )
            cg_envelope_ok = not violation
        except Exception:
            cg_envelope_ok = None

        # Genuinely trimmed cruise design point (alpha + h-stab incidence
        # solved jointly for CL=CL_required and Cm=0 -- the same trim solve
        # the optimizer uses, see methods.md Sec 9f). Additive
        # and non-fatal: a trim-solve failure leaves the untrimmed
        # design_point/polar/polar_fit above unaffected.
        trimmed_design_point = None
        try:
            from ..physics.stability import stability_and_trim

            cl_target = self._cruise_cl(plane)
            trim = stability_and_trim(
                plane,
                fine_analysis,
                cl_target,
                req.cruise_mach,
                req.cruise_altitude_m,
            )
            trim_perf = aero.trimmed_performance(
                trim, req.cruise_mach, req.cruise_altitude_m
            )
            trimmed_design_point = TrimmedDesignPoint(
                alpha_deg=trim_perf["alpha"],
                trim_ih_deg=trim_perf["i_h"],
                cl=trim_perf["CL"],
                cd=trim_perf["CD"],
                l_over_d=trim_perf["L/D"],
                cm_residual=trim_perf["Cm_residual"],
            )
        except Exception:
            trimmed_design_point = None

        return AnalysisReport(
            design=design,
            airplane=plane,
            polar=polar,
            design_point=design_point,
            polar_fit=polar_fit,
            static_margin=sm,
            x_neutral_point=x_np,
            geometry_summary=self._geometry_summary(plane, design),
            component_masses=masses,
            mass_coordinates=coords,
            physical_cg=cg,
            payload_layout=payload_layout,
            trimmed_design_point=trimmed_design_point,
            cg_envelope_ok=cg_envelope_ok,
        )

    # -- helpers -------------------------------------------------------------
    def _cruise_cl(self, plane: asb.Airplane) -> float:
        req = self.config.requirements
        atmo = asb.Atmosphere(altitude=req.cruise_altitude_m)
        q = 0.5 * atmo.density() * (req.cruise_mach * atmo.speed_of_sound()) ** 2
        return req.required_cruise_cl(q, plane.s_ref)

    def _design_point(self, plane, polar) -> DesignPoint:
        cl_target = self._cruise_cl(plane)
        idx = int(np.argmin(np.abs(polar["CL"] - cl_target)))
        return DesignPoint(
            alpha_deg=float(polar["alpha"][idx]),
            cl=float(polar["CL"][idx]),
            cd=float(polar["CD"][idx]),
            l_over_d=float(polar["L/D"][idx]),
        )

    def _fit_polar(self, plane, polar) -> PolarFit:
        """Least-squares fit of CD = CD0 + k*CL^2 over the configured mid-CL range."""
        cfg = self.config.analysis
        cl, cd = polar["CL"], polar["CD"]
        mask = (cl > cfg.polar_fit_cl_min) & (cl < cfg.polar_fit_cl_max)
        if np.sum(mask) < 3:
            mask = (cl > cfg.polar_fit_cl_min_fallback) & (
                cl < cfg.polar_fit_cl_max_fallback
            )
        x = cl[mask] ** 2
        y = cd[mask]
        a_mat = np.vstack([np.ones(len(x)), x]).T
        cd0_fit, k_fit = np.linalg.lstsq(a_mat, y, rcond=None)[0]
        ar = plane.wings[0].aspect_ratio()
        e = 1.0 / (np.pi * ar * k_fit) if k_fit > 0 else float("nan")
        return PolarFit(
            cd0=float(cd0_fit),
            k=float(k_fit),
            oswald_e=float(e),
            aspect_ratio=float(ar),
        )

    def _geometry_summary(self, plane, design) -> Dict[str, float]:
        wing = plane.wings[0]
        summary = {
            "span_m": float(wing.span()),
            "wing_area_m2": float(wing.area()),
            "aspect_ratio": float(wing.aspect_ratio()),
            "mean_aerodynamic_chord_m": float(wing.mean_aerodynamic_chord()),
            "taper_ratio": float(design.tip_chord_m / design.root_chord_m),
            "sweep_deg": float(design.sweep_deg),
            "fuselage_length_m": float(design.fuselage_length_m),
        }
        if len(plane.wings) > 1:
            summary["h_stab_area_m2"] = float(plane.wings[1].area())
        if len(plane.wings) > 2:
            summary["v_stab_area_m2"] = float(plane.wings[2].area())
        return summary
