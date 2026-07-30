# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Hybrid aerodynamics engine.

Combines an inviscid Vortex-Lattice Method (VLM, from AeroSandbox) for lift,
induced drag and pitching moment with semi-empirical estimates for the physics
the VLM cannot see:

* parasite (viscous) drag -- Raymer flat-plate component buildup,
* transonic wave drag -- Korn equation.

The result is a corrected drag polar suitable for a transonic transport design
point. All empirical coefficients come from :class:`DragModelConfig`; the wing
sweep and section thickness used by the buildup come from the actual geometry,
not hardcoded constants.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Dict

import aerosandbox as asb
import aerosandbox.numpy as np

from ..config.analysis_config import AnalysisConfig
from ..config.geometry_config import GeometryConfig
from ..config.physics_config import DragModelConfig


@dataclass
class DragComponents:
    """Breakdown of the drag estimate at a single operating point."""

    cd_parasite: float  # viscous/parasite (Raymer buildup, incl. viscous margin)
    cd_induced: float  # lift-induced (from VLM)
    cd_wave: float  # transonic wave drag (Korn)

    @property
    def cd_total(self) -> float:
        return self.cd_parasite + self.cd_induced + self.cd_wave


def swept_pg_beta(mach: float, sweep_deg: float) -> float:
    """Prandtl-Glauert compressibility factor ``beta = sqrt(1 - (M cosΛ)^2)``.

    Uses the Mach component normal to the sweep line (``M cosΛ``), the standard
    swept-wing compressibility parameter. Clamped just below the ``M cosΛ -> 1``
    singularity (near drag divergence the linear correction breaks down anyway;
    the Korn wave-drag model governs drag there).
    """
    mn = min(abs(float(mach)) * abs(float(np.cos(np.radians(sweep_deg)))), 0.95)
    return float(np.sqrt(max(1e-3, 1.0 - mn * mn)))


def compressible_report_alpha(
    alpha_inc_deg: float, alpha_0L_deg: float, mach: float, sweep_deg: float
) -> float:
    """Compressibility-correct a geometric angle of attack for reporting.

    AeroSandbox's VLM is **incompressible**, so at transonic cruise it
    under-predicts the lift-curve slope and therefore *over*-predicts the
    geometric alpha for a given CL -- a supercritical widebody reads ~4-6 deg
    where SUAVE and the real aircraft show ~1-4 deg. Prandtl-Glauert steepens
    the compressible lift curve by ``1/beta`` about the zero-lift alpha, so the
    alpha needed for a given CL compresses toward ``alpha_0L`` by ``beta``:

        alpha_compressible = alpha_0L + beta * (alpha_incompressible - alpha_0L)

    Only the *reported/trim alpha* is corrected. The induced-drag polar is left
    on the physical cruise CL (standard conceptual-design practice: CDi at a
    fixed CL is ~Mach-independent), and the VLM is still evaluated at the
    incompressible alpha where its CL equals the target -- so L/D is unchanged,
    only the angle reads correctly. See docs/methods.md.
    """
    beta = swept_pg_beta(mach, sweep_deg)
    return float(alpha_0L_deg + beta * (alpha_inc_deg - alpha_0L_deg))


class AeroAnalysis:
    """Hybrid VLM + empirical-correction aerodynamic analysis of one aircraft."""

    def __init__(
        self,
        airplane: asb.Airplane,
        sweep_deg: float,
        geometry: GeometryConfig | None = None,
        drag_model: DragModelConfig | None = None,
        analysis: AnalysisConfig | None = None,
    ):
        self.plane = airplane
        self.sweep_deg = sweep_deg
        self.geometry = geometry or GeometryConfig()
        self.drag = drag_model or DragModelConfig()
        self.analysis = analysis or AnalysisConfig()

    # -- empirical drag ------------------------------------------------------
    @staticmethod
    def _turbulent_cf(reynolds: float, mach: float) -> float:
        """Compressible turbulent flat-plate skin friction (Prandtl-Schlichting)."""
        return 0.455 / (np.log10(reynolds) ** 2.58 * (1 + 0.144 * mach**2) ** 0.65)

    def _section_thickness(self) -> float:
        """Real max thickness-to-chord of the morphed root section."""
        try:
            return float(self.plane.wings[0].xsecs[0].airfoil.max_thickness())
        except Exception:
            return 0.12

    def parasite_drag(
        self,
        mach: float,
        altitude: float,
        cl: float = 0.0,
        *,
        atmo: asb.Atmosphere | None = None,
        section_thickness: float | None = None,
    ) -> float:
        """Raymer component buildup for total parasite drag coefficient.

        ``atmo``/``section_thickness`` let :meth:`drag_components` (called
        once per candidate evaluation, or once per alpha in a sweep) pass in
        values it already computed instead of each of ``parasite_drag`` /
        ``wave_drag`` rebuilding the same ``Atmosphere`` / section thickness
        independently. Direct callers (e.g. the optimizer objective, which
        calls this standalone) can omit both and get the previous behaviour.
        """
        atmo = atmo or asb.Atmosphere(altitude=altitude)
        u = mach * atmo.speed_of_sound()
        rho, mu = atmo.density(), atmo.dynamic_viscosity()
        s_ref = self.plane.s_ref
        tc = (
            section_thickness
            if section_thickness is not None
            else self._section_thickness()
        )
        xc = self.drag.max_thickness_chordwise_loc
        sweep = np.radians(self.sweep_deg)

        cd0 = 0.0
        # Lifting surfaces
        for wing in self.plane.wings:
            mac = wing.mean_aerodynamic_chord()
            re = rho * u * mac / mu
            cf = self._turbulent_cf(re, mach)
            ff = (1 + 0.6 / xc * tc + 100 * tc**4) * (
                1.34 * mach**0.18 * np.cos(sweep) ** 0.28
            )
            swet = wing.area() * self.geometry.wing_wetted_area_factor
            cd0 += cf * ff * self.drag.interference_factor_wing * (swet / s_ref)

        # Fuselage (uses the primary body length and diameter)
        fus = self.plane.fuselages[0]
        fus_len = fus.xsecs[-1].xyz_c[0] - fus.xsecs[0].xyz_c[0]
        diameter = self.geometry.fuselage.diameter_m
        swet_fus = np.pi * diameter * fus_len * self.geometry.fuselage_wetted_factor
        re_fus = rho * u * fus_len / mu
        cf_fus = self._turbulent_cf(re_fus, mach)
        cd0 += cf_fus * self.drag.interference_factor_fuselage * (swet_fus / s_ref)

        # Engine nacelles (if present)
        for eng in self.plane.fuselages[1:]:
            eng_len = eng.xsecs[-1].xyz_c[0] - eng.xsecs[0].xyz_c[0]
            eng_diam = 2.0 * self.geometry.engine.radius_scale_m
            swet_eng = np.pi * eng_diam * eng_len  # cylinder approximation
            re_eng = rho * u * eng_len / mu
            cf_eng = self._turbulent_cf(re_eng, mach)
            cd0 += cf_eng * self.drag.interference_factor_nacelle * (swet_eng / s_ref)

        return cd0 * self.drag.viscous_margin

    def wave_drag(
        self, mach: float, cl: float, *, section_thickness: float | None = None
    ) -> float:
        """Korn-equation transonic wave-drag estimate."""
        if mach < self.drag.wave_drag_onset_mach:
            return 0.0
        tc = (
            section_thickness
            if section_thickness is not None
            else self._section_thickness()
        )
        sweep = np.radians(self.sweep_deg)
        kappa = self.drag.korn_technology_factor
        m_dd = (
            kappa / np.cos(sweep)
            - tc / np.cos(sweep) ** 2
            - cl / (10 * np.cos(sweep) ** 3)
        )
        if mach > m_dd:
            return self.drag.wave_drag_coefficient * (mach - m_dd) ** 4
        return 0.0

    def drag_components(
        self,
        mach: float,
        altitude: float,
        cl: float,
        cd_induced: float,
        *,
        atmo: asb.Atmosphere | None = None,
    ) -> DragComponents:
        atmo = atmo or asb.Atmosphere(altitude=altitude)
        tc = self._section_thickness()
        return DragComponents(
            cd_parasite=self.parasite_drag(
                mach, altitude, cl, atmo=atmo, section_thickness=tc
            ),
            cd_induced=cd_induced,
            cd_wave=self.wave_drag(mach, cl, section_thickness=tc),
        )

    # -- VLM helpers ---------------------------------------------------------
    def _run_vlm(self, op_point: asb.OperatingPoint) -> Dict[str, float]:
        return asb.VortexLatticeMethod(
            airplane=self.plane,
            op_point=op_point,
            spanwise_resolution=self.analysis.spanwise_resolution,
            chordwise_resolution=self.analysis.chordwise_resolution,
            verbose=False,
        ).run()

    # -- performance estimates ----------------------------------------------
    def quick_performance(
        self, cl_target: float, mach: float, altitude: float
    ) -> Dict[str, float]:
        """Fast (2-point) cruise estimate used inside the optimization loop.

        Linearises lift vs alpha from two probe points to find the trim alpha for
        ``cl_target``, scales induced drag, then adds the empirical components.
        """
        atmo = asb.Atmosphere(altitude=altitude)
        v = mach * atmo.speed_of_sound()
        a_lo = self.analysis.probe_alpha_low_deg
        a_hi = self.analysis.probe_alpha_high_deg

        r_lo = self._run_vlm(
            asb.OperatingPoint(atmosphere=atmo, velocity=v, alpha=a_lo)
        )
        r_hi = self._run_vlm(
            asb.OperatingPoint(atmosphere=atmo, velocity=v, alpha=a_hi)
        )

        cl_alpha = (r_hi["CL"] - r_lo["CL"]) / (a_hi - a_lo)
        alpha_req = a_lo + (cl_target - r_lo["CL"]) / cl_alpha
        # Report the compressibility-corrected alpha (VLM is incompressible;
        # see compressible_report_alpha). Induced drag below stays on the
        # physical cl_target, so this changes only the reported angle.
        alpha_0L = a_lo - r_lo["CL"] / cl_alpha if abs(cl_alpha) > 1e-9 else alpha_req
        alpha_report = compressible_report_alpha(
            alpha_req, alpha_0L, mach, self.sweep_deg
        )

        k_induced = r_lo["CD"] / (r_lo["CL"] ** 2 + 1e-9)
        cd_induced = k_induced * cl_target**2
        comps = self.drag_components(mach, altitude, cl_target, cd_induced, atmo=atmo)
        return {
            "L/D": cl_target / comps.cd_total,
            "alpha": alpha_report,
            "CD": comps.cd_total,
            "CL": cl_target,
        }

    def trimmed_performance(
        self, trim, mach: float, altitude: float
    ) -> Dict[str, float]:
        """Evaluate the genuinely trimmed cruise condition from a :class:`~alas.physics.stability.StabilityTrimResult`.

        Runs the ONE extra VLM point at the closed-form trimmed
        ``(alpha, i_h)`` (see ``stability.stability_and_trim`` /
        methods.md Sec 9f). The VLM's own induced-drag response to
        whatever tail lift/download the trim solve requires IS the physical
        trim drag -- no separate ad-hoc trim-drag formula is added on top.
        ``Cm_residual`` (the real, nonlinear Cm at this point -- should be
        close to zero) is returned for diagnostics, not penalised.
        """
        atmo = asb.Atmosphere(altitude=altitude)
        v = mach * atmo.speed_of_sound()

        hstab = next(
            (w for w in self.plane.wings if w.name == "Horizontal Stabilizer"), None
        )
        perturb = hstab is not None and trim.trim_ih_deg == trim.trim_ih_deg  # not NaN
        if perturb:
            i_h0 = float(hstab.xsecs[0].twist)
            for xs in hstab.xsecs:
                xs.twist = trim.trim_ih_deg
        try:
            out = self._run_vlm(
                asb.OperatingPoint(
                    atmosphere=atmo, velocity=v, alpha=trim.trim_alpha_deg
                )
            )
        finally:
            if perturb:
                for xs in hstab.xsecs:
                    xs.twist = i_h0

        cl_trim = float(out["CL"])
        cd_induced_trim = float(out["CD"])
        comps = self.drag_components(
            mach, altitude, cl_trim, cd_induced_trim, atmo=atmo
        )
        # The VLM ran at the incompressible trim alpha, so CL/CD (hence L/D and
        # trim drag) are correct; report the compressibility-corrected angle.
        cla_pd = float(trim.cl_alpha)  # per degree
        if abs(cla_pd) > 1e-9:
            alpha_0L = trim.trim_alpha_deg - cl_trim / cla_pd
            alpha_report = compressible_report_alpha(
                trim.trim_alpha_deg, alpha_0L, mach, self.sweep_deg
            )
        else:
            alpha_report = trim.trim_alpha_deg
        return {
            "L/D": cl_trim / comps.cd_total,
            "alpha": alpha_report,
            "i_h": trim.trim_ih_deg,
            "CD": comps.cd_total,
            "CL": cl_trim,
            "Cm_residual": float(out["Cm"]),
        }

    def run_sweep(self, mach: float, altitude: float) -> Dict[str, np.ndarray]:
        """Full alpha sweep producing the corrected drag polar and stability data."""
        atmo = asb.Atmosphere(altitude=altitude)
        v = mach * atmo.speed_of_sound()
        alphas = np.linspace(
            self.analysis.sweep_alpha_min_deg,
            self.analysis.sweep_alpha_max_deg,
            self.analysis.sweep_n_points,
        )
        res = {
            k: []
            for k in (
                "alpha",
                "CL",
                "CD",
                "CD_induced",
                "CD_wave",
                "CD_parasite",
                "Cm",
                "L/D",
            )
        }
        for a in alphas:
            out = self._run_vlm(
                asb.OperatingPoint(atmosphere=atmo, velocity=v, alpha=a)
            )
            cl, cd_i, cm = float(out["CL"]), float(out["CD"]), float(out["Cm"])
            comps = self.drag_components(mach, altitude, cl, cd_i, atmo=atmo)
            res["alpha"].append(a)
            res["CL"].append(cl)
            res["Cm"].append(cm)
            res["CD"].append(comps.cd_total)
            res["CD_induced"].append(comps.cd_induced)
            res["CD_wave"].append(comps.cd_wave)
            res["CD_parasite"].append(comps.cd_parasite)
            res["L/D"].append(cl / comps.cd_total)
        out = {k: np.array(v) for k, v in res.items()}
        # Compressibility-correct the reported alpha axis so the CL/Cm-vs-alpha
        # slopes reflect compressible flow and the extracted cruise alpha matches
        # SUAVE/real (~1-4 deg), not the inflated incompressible value. Only the
        # alpha labels move (toward alpha_0L by beta); CL/CD/Cm are unchanged, so
        # the drag polar and stability curve are untouched. See compressible_report_alpha.
        cl_arr, al_arr = out["CL"], out["alpha"]
        if cl_arr.size >= 2 and float(cl_arr[-1] - cl_arr[0]) > 1e-6:
            alpha_0L = float(np.interp(0.0, cl_arr, al_arr))
            beta = swept_pg_beta(mach, self.sweep_deg)
            out["alpha"] = alpha_0L + beta * (al_arr - alpha_0L)
        return out
