# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Figure registries for the sidecar.

Two maps power the desktop app's (``desktop/``, Go/Wails + React) charts:

* ``RESULT_FIGURES`` -- one entry per ``reporting.visualization.figure_*``
  shown across the Results screen's tabs (Baseline / Optimization /
  Aerodynamics / Weight & Balance / Model Comparison / Propulsion /
  Structures / Mission). Each is a ``fn(PipelineResult, theme) -> Figure |
  None``; ``None`` means "this run has no data for that figure" (e.g. a
  mission figure on a run with mission analysis disabled) and the HTTP layer
  turns it into a 404 the frontend renders as an empty slot -- graceful
  per-section degradation.

* ``PREVIEW_FIGURES`` -- the *live* previews recomputed on every debounced
  form edit: the Inputs screen's 3D exterior + cabin views and each Advanced
  Settings tab's side-panel chart. These build straight from a config +
  design vector, with **no** pipeline run, so the frontend can show them the
  moment the user changes an input.

This module (like the rest of ``alas.sidecar``) must load headless, with
no GUI toolkit present. The hand-drawn previews' axis theming is implemented
here as ``_style_axes`` against ``reporting.theme``'s palette.
"""

from __future__ import annotations

from typing import Any, Callable, Dict, Optional

import matplotlib

matplotlib.use("Agg")  # headless: no display, no Qt backend
import matplotlib as mpl
from matplotlib.figure import Figure

from . import figures_extra
from .lazy_imports import viz  # visualization.py imports aerosandbox -- deferred
from ..reporting.theme import get_palette

# ---------------------------------------------------------------------------
# Small helpers
# ---------------------------------------------------------------------------


def _style_axes(fig: Figure, ax, theme: Optional[str]):
    """Qt-free equivalent of ``gui.widgets.mpl_canvas.style_themed_axes``."""
    pal = get_palette(theme)
    fig.patch.set_facecolor(pal.bg)
    ax.set_facecolor(pal.bg)
    for spine in ax.spines.values():
        spine.set_edgecolor(pal.spine)
    ax.tick_params(colors=pal.tick)
    ax.xaxis.label.set_color(pal.tick)
    ax.yaxis.label.set_color(pal.tick)
    ax.title.set_color(pal.title)
    return pal


def _plane(result) -> Optional[Any]:
    report = result.optimized_report or result.baseline_report
    return getattr(report, "airplane", None) if report is not None else None


def _mass_report(result) -> Optional[Any]:
    """An object with ``.component_masses`` -- optimized report or baseline."""
    return result.optimized_report or result.baseline_analysis


# ===========================================================================
# RESULT FIGURES (post-run)
# ===========================================================================


def _opt_history(result):
    optr = getattr(result, "optimization_result", None)
    return getattr(optr, "history", None) if optr is not None else None


def _fig_design_evolution(result, theme):
    hist = _opt_history(result)
    if hist is None:
        return None
    from ..geometry.aircraft_builder import AircraftBuilder

    builder = AircraftBuilder(result.config.geometry)
    return viz.figure_design_evolution(hist, builder, theme=theme)


def _fig_cabin_payload(result, theme):
    rep = _mass_report(result)
    layout = getattr(rep, "payload_layout", None) if rep is not None else None
    plane = getattr(rep, "airplane", None) if rep is not None else None
    if layout is None or plane is None:
        return None
    return viz.figure_cabin_payload(layout, plane, result.config, theme=theme)


def _fig_airfoil_reynolds(result, theme):
    plane = _plane(result)
    if plane is None:
        return None
    try:
        airfoil = plane.wings[0].xsecs[0].airfoil
    except (AttributeError, IndexError):
        return None
    return viz.figure_airfoil_reynolds(airfoil, theme=theme)


def _fig_route_2d(result, theme):
    route = getattr(result, "route", None)
    if route is None:
        return None
    mass_profile = altitude_profile = None
    mission_result = getattr(result, "mission_result", None)
    if mission_result is not None and getattr(mission_result, "status", None) == "ok":
        try:
            from ..reporting.route_globe import sync_mass_to_route

            mass_profile, altitude_profile = sync_mass_to_route(route, mission_result)
        except Exception:
            mass_profile = altitude_profile = None
    return viz.figure_mission_route_2d(
        route, mass_profile, altitude_profile, theme=theme
    )


def _mission(result):
    return getattr(result, "mission_result", None)


# The registry. Every lambda is guarded by the HTTP layer's try/except, so a
# raising factory (bad/missing intermediate data) becomes a 404 rather than a
# 500 -- same "each tab degrades on its own" contract as results_view.py.
RESULT_FIGURES: Dict[str, Callable[[Any, Optional[str]], Optional[Figure]]] = {
    # --- Baseline / Optimization ------------------------------------------
    "optimization_history": lambda r, t: (
        viz.figure_optimization_history(_opt_history(r), theme=t)
        if _opt_history(r)
        else None
    ),
    "design_evolution": _fig_design_evolution,
    "airfoil_comparison": lambda r, t: viz.figure_airfoil_comparison(
        r.config.geometry.wing.root_airfoil, r.optimized_design, theme=t
    ),
    "airfoil_evolution": lambda r, t: (
        viz.figure_airfoil_evolution(r.optimized_report.airplane, theme=t)
        if r.optimized_report
        else None
    ),
    "wireframe_wing": lambda r, t: (
        viz.figure_wireframe_wing(_plane(r), theme=t) if _plane(r) else None
    ),
    "wireframe_fuselage": lambda r, t: (
        viz.figure_wireframe_fuselage(_plane(r), theme=t) if _plane(r) else None
    ),
    "wireframe_empennage": lambda r, t: (
        viz.figure_wireframe_empennage(_plane(r), theme=t) if _plane(r) else None
    ),
    "polar_comparison": lambda r, t: (
        viz.figure_polar_comparison(r.baseline_report, r.optimized_report, theme=t)
        if (r.baseline_report and r.optimized_report)
        else None
    ),
    "planform_comparison": lambda r, t: (
        viz.figure_planform_comparison(
            r.baseline_report.airplane, r.optimized_report.airplane, theme=t
        )
        if (r.baseline_report and r.optimized_report)
        else None
    ),
    "asb_threeview": lambda r, t: (
        viz.figure_asb_threeview(_plane(r), theme=t) if _plane(r) else None
    ),
    # --- Aerodynamics -----------------------------------------------------
    "aero_panel": lambda r, t: (
        viz.figure_aero_panel(r.optimized_report, theme=t)
        if r.optimized_report
        else None
    ),
    "drag_breakdown": lambda r, t: (
        viz.figure_drag_breakdown(r.optimized_report, theme=t)
        if r.optimized_report
        else None
    ),
    "span_loading": lambda r, t: (
        viz.figure_span_loading(r.optimized_report, r.config, theme=t)
        if r.optimized_report
        else None
    ),
    "vn_diagram": lambda r, t: (
        viz.figure_vn_diagram(r.optimized_report, r.config, theme=t)
        if r.optimized_report
        else None
    ),
    "vlm_flow": lambda r, t: (
        viz.figure_vlm_flow(r.optimized_report, r.config, theme=t)
        if r.optimized_report
        else None
    ),
    "dynamic_modes": lambda r, t: (
        viz.figure_dynamic_modes(r.optimized_report, r.config, theme=t)
        if r.optimized_report
        else None
    ),
    "control_surfaces": lambda r, t: (
        viz.figure_control_surfaces(r.optimized_report, r.config, theme=t)
        if r.optimized_report
        else None
    ),
    "airfoil_reynolds": _fig_airfoil_reynolds,
    # Gate on status == "ok", not mere truthy-object presence: a
    # run_mses_pressure_distribution() failure (non-convergence, missing BL
    # dump, ...) still returns a real MSESPressureResult instance -- just one
    # whose surface/field arrays are all empty defaults -- so a bare
    # `getattr(...) else None` check let a FAILED pressure solve through as
    # if it had data, rendering a blank/empty-axes figure instead of the
    # standard "Not available for this run" slot every other figure gets on
    # a genuine failure.
    "mses_pressure": lambda r, t: (
        viz.figure_mses_pressure_distribution(r.mses_pressure, theme=t)
        if getattr(r, "mses_pressure", None) and r.mses_pressure.status == "ok"
        else None
    ),
    "mses_mach_contours": lambda r, t: (
        viz.figure_mses_mach_contours(r.mses_pressure, theme=t)
        if getattr(r, "mses_pressure", None) and r.mses_pressure.status == "ok"
        else None
    ),
    # --- Weight & Balance -------------------------------------------------
    "mass_breakdown": lambda r, t: (
        viz.figure_mass_breakdown(_mass_report(r), theme=t) if _mass_report(r) else None
    ),
    "fuel_volume_check": lambda r, t: (
        viz.figure_fuel_volume_check(r.optimized_report, r.config, theme=t)
        if r.optimized_report
        else None
    ),
    "mass_distribution": lambda r, t: (
        viz.figure_mass_distribution(_mass_report(r), theme=t)
        if _mass_report(r)
        else None
    ),
    "cg_envelope": lambda r, t: (
        viz.figure_cg_envelope(_mass_report(r), config=r.config, theme=t)
        if _mass_report(r)
        else None
    ),
    "landing_gear_planform": lambda r, t: (
        viz.figure_landing_gear_planform(_mass_report(r), config=r.config, theme=t)
        if _mass_report(r)
        else None
    ),
    "stability_side_view": lambda r, t: (
        viz.figure_stability_side_view(r.optimized_report, config=r.config, theme=t)
        if r.optimized_report
        else None
    ),
    "stability_metrics": lambda r, t: (
        viz.figure_stability_metrics(r.optimized_report, config=r.config, theme=t)
        if r.optimized_report
        else None
    ),
    "cabin_payload": _fig_cabin_payload,
    # --- Model Comparison -------------------------------------------------
    "model_comparison": lambda r, t: (
        viz.figure_model_comparison(
            r.optimized_report,
            mission_result=_mission(r),
            mses_result=getattr(r, "mses_result", None),
            theme=t,
        )
        if r.optimized_report
        else None
    ),
    # --- Propulsion -------------------------------------------------------
    "propulsion_cycle_summary": lambda r, t: (
        viz.figure_propulsion_cycle_summary(r.optimized_report, r.config, theme=t)
        if r.optimized_report
        else None
    ),
    "propulsion_carpet_plot": lambda r, t: (
        viz.figure_propulsion_carpet_plot(r.optimized_report, r.config, theme=t)
        if r.optimized_report
        else None
    ),
    "propulsion_efficiency_decomposition": lambda r, t: (
        viz.figure_propulsion_efficiency_decomposition(
            r.optimized_report, r.config, theme=t
        )
        if r.optimized_report
        else None
    ),
    "propulsion_bpr_sensitivity": lambda r, t: (
        viz.figure_propulsion_bpr_sensitivity(r.optimized_report, r.config, theme=t)
        if r.optimized_report
        else None
    ),
    "propulsion_altitude_sweep": lambda r, t: (
        viz.figure_propulsion_altitude_sweep(r.optimized_report, r.config, theme=t)
        if r.optimized_report
        else None
    ),
    # --- Structures (each factory self-degrades to a status figure) -------
    "structures_sizing": lambda r, t: viz.figure_structures_sizing(
        getattr(r, "structural_result", None), theme=t
    ),
    "structures_loads": lambda r, t: viz.figure_structures_loads(
        getattr(r, "structural_result", None), theme=t
    ),
    "structures_stress": lambda r, t: viz.figure_structures_stress(
        getattr(r, "structural_result", None), theme=t
    ),
    "structures_modes": lambda r, t: viz.figure_structures_modes(
        getattr(r, "structural_result", None), theme=t
    ),
    "structures_vibration": lambda r, t: viz.figure_structures_vibration(
        getattr(r, "structural_result", None), theme=t
    ),
    "structures_patran": lambda r, t: viz.figure_structures_patran(
        getattr(r, "structural_result", None), theme=t
    ),
    # --- Mission & Route --------------------------------------------------
    "mission_route_2d": _fig_route_2d,
    "payload_range": lambda r, t: (
        viz.figure_payload_range(r.optimized_report, r.config, theme=t)
        if r.optimized_report
        else None
    ),
    "mission_profile": lambda r, t: (
        viz.figure_mission_profile(_mission(r), theme=t) if _mission(r) else None
    ),
    "mission_velocities": lambda r, t: (
        viz.figure_mission_velocities(_mission(r), theme=t) if _mission(r) else None
    ),
    "mission_flight_path": lambda r, t: (
        viz.figure_mission_flight_path(_mission(r), theme=t) if _mission(r) else None
    ),
    "mission_aero_coefficients": lambda r, t: (
        viz.figure_mission_aero_coefficients(_mission(r), theme=t)
        if _mission(r)
        else None
    ),
    "mission_aero_forces": lambda r, t: (
        viz.figure_mission_aero_forces(_mission(r), theme=t) if _mission(r) else None
    ),
    "mission_drag_components": lambda r, t: (
        viz.figure_mission_drag_components(_mission(r), theme=t)
        if _mission(r)
        else None
    ),
}

# Matching Chart / Landing & Take-Off / 3D globe (GUI-only widget figures,
# ported Qt-free in figures_extra.py).
RESULT_FIGURES.update(figures_extra.EXTRA_FIGURES)


# ===========================================================================
# LIVE PREVIEW FIGURES (no run needed) -- ports of MainWindow._update_preview
# ===========================================================================


def _build_plane(config, dv, include_engines: bool = True):
    from ..geometry.aircraft_builder import AircraftBuilder

    return AircraftBuilder(config.geometry).build(dv, include_engines=include_engines)


def _preview_exterior(config, dv, theme, view=None):
    """Live 3D exterior wireframe -- port of MainWindow._update_preview's
    exterior-canvas drawing (wings cyan, fuselage in the theme's title color,
    engines orange), rendered from just config + design vector."""
    plane = _build_plane(config, dv)
    pal = get_palette(theme)
    v = view or {}
    fig = Figure(figsize=(6, 5))
    fig.patch.set_facecolor(pal.bg)
    ax = fig.add_subplot(111, projection="3d")
    ax.set_facecolor(pal.bg)
    ax.view_init(elev=float(v.get("elev", 22)), azim=float(v.get("azim", -125)))
    # draw_wireframe imports AeroSandbox pretty_plots, which runs a global
    # seaborn.set_theme() the first time -- isolate it so it can't reset
    # rcParams for later 2-D figures rendered by this same process.
    with mpl.rc_context():
        for wing in plane.wings:
            color = "#00d8ff" if "wing" in wing.name.lower() else "#a0a0a0"
            wing.draw_wireframe(
                ax=ax, show=False, color=color, thin_linewidth=0.8, thick_linewidth=0.8
            )
        for i, fus in enumerate(plane.fuselages):
            color = pal.title if i == 0 else "#ff9900"
            fus.draw_wireframe(
                ax=ax, show=False, color=color, thin_linewidth=0.8, thick_linewidth=0.8
            )
    ax.set_axis_off()
    # equal aspect via bounding box
    limits = [axfn() for axfn in (ax.get_xlim, ax.get_ylim, ax.get_zlim)]
    max_range = (max(hi - lo for lo, hi in limits) / 2.0) * float(v.get("zoom", 1.0))
    for i, (lo, hi) in enumerate(limits):
        mid = (lo + hi) / 2.0
        [ax.set_xlim, ax.set_ylim, ax.set_zlim][i](mid - max_range, mid + max_range)
    return fig


def _preview_cabin(config, dv, theme, view=None):
    from ..physics.payload import build_payload_layout

    plane = _build_plane(config, dv)
    layout = build_payload_layout(plane, config)
    fig = viz.figure_cabin_payload_3d(layout, plane, config, theme=theme)
    v = view or {}
    if "elev" in v or "azim" in v:
        for ax in fig.axes:
            if hasattr(ax, "view_init"):
                ax.view_init(
                    elev=float(v.get("elev", 22)), azim=float(v.get("azim", -125))
                )
    return fig


def _preview_geometry(config, dv, theme, view=None):
    plane = _build_plane(config, dv)
    with mpl.rc_context():
        fig = viz.figure_asb_threeview(plane, theme=theme)
    if fig is not None:
        return fig
    return viz.figure_geometry(plane, theme=theme)


def _preview_drag(config, dv, theme, view=None):
    """Live CD0 / wave-drag vs Mach chart -- port of _update_drag_chart."""
    import numpy as np
    from ..physics.aerodynamics import AeroAnalysis

    plane = _build_plane(config, dv)
    aero = AeroAnalysis(
        plane,
        sweep_deg=dv.sweep_deg,
        geometry=config.geometry,
        drag_model=config.drag_model,
        analysis=config.analysis,
    )
    cl_ref = 0.5
    altitude = config.requirements.cruise_altitude_m
    machs = np.linspace(0.3, 0.92, 30)
    cd_parasite = [aero.parasite_drag(float(m), altitude, cl_ref) for m in machs]
    cd_wave = [aero.wave_drag(float(m), cl_ref) for m in machs]

    fig = Figure(figsize=(6, 4))
    ax = fig.add_subplot(111)
    pal = _style_axes(fig, ax, theme)
    ax.plot(machs, cd_parasite, color="#00d8ff", lw=1.8, label="CD0 (parasite)")
    ax.plot(machs, cd_wave, color="#ff9900", lw=1.8, label="CD wave")
    ax.axvline(
        config.requirements.cruise_mach, color=pal.title, ls="--", lw=1, alpha=0.5
    )
    ax.set_xlabel("Mach")
    ax.set_ylabel("CD")
    ax.set_title(f"Drag vs Mach (illustrative, CL={cl_ref:.2f})", fontsize=10)
    ax.legend(
        loc="upper left",
        fontsize=8,
        facecolor=pal.bg,
        edgecolor=pal.spine,
        labelcolor=pal.tick,
    )
    return fig


def _quick_mass_report(config, dv):
    """The lumped-mass AnalysisReport the CG-envelope / landing-gear previews
    build (no VLM/aero solve) -- port of _update_cg_envelope_chart's setup."""
    from ..physics.mass import run_mass_analysis
    from ..analysis.full_analysis import AnalysisReport, DesignPoint, PolarFit

    plane = _build_plane(config, dv)
    masses, coords, cg = run_mass_analysis(
        plane, config.requirements, config.geometry, config.mass_model
    )
    return AnalysisReport(
        design=dv,
        airplane=plane,
        polar={},
        design_point=DesignPoint(0.0, 0.0, 0.0, 0.0),
        polar_fit=PolarFit(0.0, 0.0, 0.0, 0.0),
        static_margin=float("nan"),
        component_masses=masses,
        mass_coordinates=coords,
        physical_cg=cg,
    )


def _preview_cg(config, dv, theme, view=None):
    return viz.figure_cg_envelope(
        _quick_mass_report(config, dv), config=config, theme=theme
    )


def _preview_landing_gear(config, dv, theme, view=None):
    return viz.figure_landing_gear_planform(
        _quick_mass_report(config, dv), config=config, theme=theme
    )


def _preview_control_surfaces(config, dv, theme, view=None):
    from ..analysis.full_analysis import AnalysisReport, DesignPoint, PolarFit

    plane = _build_plane(config, dv)
    report = AnalysisReport(
        design=dv,
        airplane=plane,
        polar={},
        design_point=DesignPoint(0.0, 0.0, 0.0, 0.0),
        polar_fit=PolarFit(0.0, 0.0, 0.0, 0.0),
        static_margin=float("nan"),
    )
    return viz.figure_control_surfaces(report, config=config, theme=theme)


def _preview_structures(config, dv, theme, view=None):
    from ..config.materials import get_material
    from ..config.structures_config import resolve_spar_geometry
    from ..geometry.airfoils import AirfoilLibrary, build_section
    from ..geometry.wing_structure import WingStructureGeometry
    from ..physics.structural_sizing import size_wingbox

    scfg = config.structures
    root_section = build_section(
        dv, AirfoilLibrary.get(config.geometry.wing.root_airfoil).coordinates
    )
    tip_airfoil = AirfoilLibrary.get(config.geometry.wing.tip_airfoil)
    spar_fracs, spar_full_span = resolve_spar_geometry(scfg)
    wsg = WingStructureGeometry(
        dv, config.geometry.wing, root_section, tip_airfoil, spar_fracs, spar_full_span
    )
    sizing = size_wingbox(
        wsg,
        scfg,
        config.requirements,
        get_material(scfg.skin_material),
        get_material(scfg.spar_web_material),
        get_material(scfg.spar_cap_material),
        get_material(scfg.rib_material),
    )
    return viz.figure_structures_designer_preview(wsg, sizing, theme=theme)


def _preview_engine(config, dv, theme, view=None):
    return viz.figure_engine_designer_preview(config, theme=theme)


PREVIEW_FIGURES: Dict[str, Callable[[Any, Any, Optional[str]], Optional[Figure]]] = {
    "exterior_3d": _preview_exterior,
    "cabin_3d": _preview_cabin,
    "geometry": _preview_geometry,
    "drag": _preview_drag,
    "mass_cg": _preview_cg,
    "landing_gear": _preview_landing_gear,
    "control_surfaces": _preview_control_surfaces,
    "structures": _preview_structures,
    "engine": _preview_engine,
}
