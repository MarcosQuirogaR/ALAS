# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Extra result figures the Qt app draws with dedicated *widgets* rather than
``reporting.visualization.figure_*`` factories: the Matching Chart, the
Landing & Take-Off runway/bar diagrams, and the 3D route globe.

These live here (not in ``reporting.visualization``, part of the simulation
core) because they are GUI-only presentation. The drawing code is ported from
a dedicated matching-chart and LTO widget pair, but
rewritten Qt-free: it takes a Matplotlib ``Figure`` instead of an ``MplCanvas``
and resolves the app theme via ``reporting.theme`` instead of the Qt
``style_themed_axes`` helper. No file under ``physics``/``reporting``/``config``
is modified -- their compute functions (``build_matching_chart``,
``compute_field_performance``, ``build_globe_plotter``) are only *imported*.
"""

from __future__ import annotations

from typing import Optional

import matplotlib

matplotlib.use("Agg")
import numpy as np
from matplotlib.figure import Figure

from ..reporting.theme import get_palette
from .lazy_imports import viz  # visualization.py imports aerosandbox -- deferred

_AIRPORT_COLOURS = ["#e74c3c", "#e67e22", "#f1c40f", "#2ecc71", "#1abc9c", "#9b59b6"]
_RUNWAY = "#4a4a4a"
_STRIPE = "#f0f0f0"
_GRASS = "#2d5a27"
_MS_TO_KT = 1.94384


def _style_axes(fig: Figure, ax, theme: Optional[str]):
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


def _resolve_airport(s: str):
    """Resolve a config airport string (a display 'Name (ICAO)' or a plain
    name/ICAO) to an Airport via the same database the Qt combo uses."""
    from ..config.airports import get_airport

    s = (s or "").strip()
    try:
        return get_airport(s)
    except KeyError:
        pass
    # Display form is "Name (ICAO)" -- try the ICAO in the parentheses, then
    # the bare name before it.
    if "(" in s and s.endswith(")"):
        icao = s[s.rfind("(") + 1 : -1].strip()
        name = s[: s.rfind("(")].strip()
        for cand in (icao, name):
            try:
                return get_airport(cand)
            except KeyError:
                continue
    raise KeyError(f"Airport '{s}' not found")


# ===========================================================================
# Matching chart (port of matching_chart_widget._draw + compute)
# ===========================================================================


def figure_matching_chart(result, theme: Optional[str] = None) -> Optional[Figure]:
    report = getattr(result, "optimized_report", None)
    if report is None:
        return None
    from ..physics.performance import build_matching_chart, FAR25_OEI_GRADIENT

    cfg = result.config
    perf_cfg = cfg.performance
    req = cfg.requirements
    pf = report.polar_fit
    gs = report.geometry_summary
    wing_area = gs.get("wing_area_m2", report.airplane.wings[0].area())
    n_eng = len(cfg.geometry.engine.spanwise_positions_m)

    try:
        airports = [
            _resolve_airport(cfg.departure_airport),
            _resolve_airport(cfg.arrival_airport),
        ]
    except KeyError:
        return viz.figure_status_message(
            "Matching Chart",
            "Departure/arrival airport not found in the database.",
            theme=theme,
        )

    oei_grad = FAR25_OEI_GRADIENT.get(n_eng, perf_cfg.oei_gradient)
    from ..physics.performance import static_thrust_to_weight

    tw_sl = static_thrust_to_weight(cfg)

    data = build_matching_chart(
        cd0=pf.cd0,
        k=pf.k,
        cruise_mach=req.cruise_mach,
        cruise_altitude_m=req.cruise_altitude_m,
        mtow_kg=req.mtow_kg,
        wing_area_m2=wing_area,
        n_engines=n_eng,
        airports=airports,
        cl_max_to=perf_cfg.cl_max_to,
        cl_max_land=perf_cfg.cl_max_land,
        thrust_lapse=perf_cfg.thrust_lapse,
        oei_gradient=oei_grad,
        oei_climb_cl=perf_cfg.oei_climb_cl,
        oei_climb_delta_cd=perf_cfg.oei_climb_delta_cd,
        k_land=perf_cfg.k_land,
        tw_design=tw_sl,
        ws_min_pa=perf_cfg.ws_min_pa,
        ws_max_pa=perf_cfg.ws_max_pa,
        n_ws_points=perf_cfg.matching_chart_resolution,
    )

    fig = Figure(figsize=(9, 6))
    ax = fig.add_subplot(111)
    pal = _style_axes(fig, ax, theme)
    ws_kg = data.ws_pa / 9.81

    ax.plot(
        ws_kg,
        data.tw_cruise,
        color="#3498db",
        linewidth=2.0,
        label="Cruise (T/W0 floor)",
    )
    ax.axhline(
        data.tw_oei_climb,
        color="#9b59b6",
        linewidth=1.8,
        linestyle="--",
        label=f"OEI climb >={oei_grad * 100:.1f}%  (T/W={data.tw_oei_climb:.3f})",
    )

    for i, (name, tw_curve) in enumerate(data.tw_takeoff.items()):
        colour = _AIRPORT_COLOURS[i % len(_AIRPORT_COLOURS)]
        ax.plot(
            ws_kg,
            tw_curve,
            color=colour,
            linewidth=1.6,
            linestyle="-.",
            label=f"T/O  {name}",
        )
        ws_land_kg = data.ws_land_limits[name] / 9.81
        ax.axvline(
            ws_land_kg, color=colour, linewidth=1.4, linestyle=":", label=f"Land {name}"
        )

    tw_floor = np.maximum(
        data.tw_cruise,
        np.maximum(data.tw_oei_climb, np.max(list(data.tw_takeoff.values()), axis=0)),
    )
    ax.fill_between(ws_kg, 0, tw_floor, alpha=0.12, color="#e74c3c", label="_nolegend_")

    if data.design_ws_pa is not None and data.design_tw is not None:
        ws_dp = data.design_ws_pa / 9.81
        ax.plot(
            ws_dp,
            data.design_tw,
            marker="*",
            markersize=16,
            color="#f1c40f",
            markeredgecolor="#ffffff",
            markeredgewidth=0.8,
            linewidth=0,
            zorder=10,
            label=f"Design point  ({ws_dp:.0f} kg/m2,  T/W={data.design_tw:.3f})",
        )

    ax.text(
        0.50,
        0.25,
        "FEASIBLE\nDESIGN SPACE",
        transform=ax.transAxes,
        ha="center",
        va="center",
        fontsize=13,
        fontweight="bold",
        color="#2ecc71",
        alpha=0.45,
    )
    ax.set_xlabel("Wing loading  W/S  [kg/m2]", fontsize=11)
    ax.set_ylabel("Thrust-to-weight  T0/W0  [-]", fontsize=11)
    ax.set_title("Matching Chart - Design Space", fontsize=13, fontweight="bold")
    ax.set_xlim(ws_kg[0], ws_kg[-1])
    ax.set_ylim(0, min(0.6, tw_floor.max() * 1.4))
    ax.grid(True, linestyle=":", linewidth=0.5, alpha=0.35, color="#888888")
    ax.legend(
        loc="upper left",
        fontsize=8,
        framealpha=0.25,
        facecolor=pal.panel,
        edgecolor=pal.spine,
        labelcolor=pal.tick,
    )
    fig.tight_layout()
    return fig


# ===========================================================================
# Landing & Take-Off (port of lto_widget._draw_runway + _draw_bars)
# ===========================================================================


def _draw_runway(ax, perf, theme):
    import matplotlib.patches as patches
    import matplotlib.patheffects as pe

    pal = get_palette(theme)
    dark_bg = pal.bg
    ax.set_aspect("equal", adjustable="datalim")
    ax.axis("off")
    toda = perf.toda_m
    rw_h = toda * 0.06
    margin = toda * 0.05

    ax.add_patch(
        patches.FancyBboxPatch(
            (-margin, -rw_h * 3),
            toda + 3 * margin,
            rw_h * 8,
            boxstyle="round,pad=0",
            linewidth=0,
            facecolor=_GRASS,
            zorder=0,
        )
    )
    ax.add_patch(
        patches.Rectangle(
            (0, 0),
            toda,
            rw_h,
            linewidth=1.5,
            edgecolor="#888888",
            facecolor=_RUNWAY,
            zorder=1,
        )
    )
    dash_len, dash_gap = toda * 0.04, toda * 0.04
    x = dash_gap
    while x + dash_len < toda:
        ax.plot(
            [x, x + dash_len],
            [rw_h / 2, rw_h / 2],
            color=_STRIPE,
            linewidth=1.5,
            zorder=2,
        )
        x += dash_len + dash_gap
    for xbar in [0, toda]:
        ax.add_patch(
            patches.Rectangle(
                (xbar - toda * 0.005, 0),
                toda * 0.01,
                rw_h,
                facecolor=_STRIPE,
                linewidth=0,
                zorder=2,
            )
        )

    def arrow_annot(x_end, y, colour, label, above):
        ax.annotate(
            "",
            xy=(x_end, y),
            xytext=(0, y),
            arrowprops=dict(arrowstyle="->", color=colour, lw=1.5),
        )
        y_text = y + (rw_h * 0.35 if above else -rw_h * 0.35)
        ax.text(
            x_end / 2,
            y_text,
            label,
            ha="center",
            va="bottom" if above else "top",
            color=colour,
            fontsize=8,
            fontweight="bold",
            path_effects=[pe.withStroke(linewidth=2, foreground=dark_bg)],
        )

    arrow_annot(
        perf.todr_m, rw_h * 2.6, "#3498db", f"TODR  {perf.todr_m:.0f} m", above=True
    )
    arrow_annot(
        perf.bfl_m, rw_h * 4.2, "#e67e22", f"BFL   {perf.bfl_m:.0f} m", above=True
    )
    arrow_annot(
        perf.asd_m, -rw_h * 1.8, "#e74c3c", f"ASD   {perf.asd_m:.0f} m", above=False
    )
    arrow_annot(
        perf.ldr_m, -rw_h * 3.4, "#2ecc71", f"LDR   {perf.ldr_m:.0f} m", above=False
    )

    vs = perf.v_speeds
    v2, vr, v1 = vs.v2_ms, vs.v_r_ms, vs.v1_ms

    def pos(v):
        return perf.todr_m * min((v / v2) ** 2, 1.0) * 0.75

    v_items = sorted(
        [("V1", v1, "#f1c40f"), ("VR", vr, "#e67e22"), ("V2", v2, "#3498db")],
        key=lambda t: pos(t[1]),
    )
    min_sep = toda * 0.05
    last_x = None
    row = 0
    for v_name, v_val, col in v_items:
        xp = pos(v_val)
        row = 0 if last_x is None or xp - last_x >= min_sep else 1 - row
        last_x = xp
        ax.vlines(
            xp,
            -rw_h * 0.3,
            rw_h * 2.1,
            color=col,
            linewidth=1.2,
            linestyle="--",
            alpha=0.7,
            zorder=3,
        )
        ax.text(
            xp,
            rw_h * (1.3 + 0.55 * row),
            f"{v_name} {v_val * _MS_TO_KT:.0f}kt",
            ha="center",
            va="bottom",
            color=col,
            fontsize=7,
            path_effects=[pe.withStroke(linewidth=1.5, foreground=dark_bg)],
        )

    ax.set_xlim(-margin, toda + margin)
    ax.set_ylim(-rw_h * 4.3, rw_h * 5.2)
    ax.set_title(
        f"{perf.airport.name}  |  elev {perf.airport.elevation_m:.0f} m  ISA+{perf.airport.isa_deviation_c:.0f}C\n"
        f"TODA = {toda:.0f} m   LDA = {perf.lda_m:.0f} m",
        color=pal.title,
        fontsize=9,
        pad=10,
    )


def _draw_bars(ax, perf, theme):
    import matplotlib.patches as mpatches

    pal = get_palette(theme)
    labels = ["TODR", "BFL", "ASD", "LDR"]
    values = [perf.todr_m, perf.bfl_m, perf.asd_m, perf.ldr_m]
    avail = [perf.toda_m, perf.toda_m, perf.toda_m, perf.lda_m]
    colours = ["#3498db", "#e67e22", "#e74c3c", "#2ecc71"]
    x = np.arange(len(labels))
    # A fixed, theme-neutral mid-grey rather than the old hardcoded "#3a3a3a"
    # -- that value happened to equal the "grey" theme's own axes background
    # (invisible-on-grey) and read as a stray near-black bar on light theme.
    # "#888888" keeps reasonable contrast against pal.bg in all three themes.
    ax.bar(x, avail, 0.74, color="#888888", zorder=1)
    for i, (lbl, val, col) in enumerate(zip(labels, values, colours)):
        feasible = val <= avail[i]
        ax.bar(
            x[i],
            val,
            0.56,
            color=col,
            alpha=0.85,
            edgecolor="#ff0000" if not feasible else col,
            linewidth=1.5 if not feasible else 0,
            zorder=2,
        )
        ax.text(
            x[i],
            val + avail[i] * 0.01,
            f"{val:.0f} m",
            ha="center",
            va="bottom",
            color=pal.title,
            fontsize=8,
            fontweight="bold",
        )
        if not feasible:
            ax.text(
                x[i],
                avail[i] * 0.5,
                "EXCEEDS\nRUNWAY",
                ha="center",
                va="center",
                color="#ff4444",
                fontsize=7.5,
                fontweight="bold",
                alpha=0.9,
            )
    ax.set_xticks(x)
    ax.set_xticklabels(labels, color=pal.tick)
    ax.set_ylabel("Distance [m]", fontsize=10)
    ax.set_title("Required vs Available", fontsize=11, fontweight="bold")
    ax.set_ylim(0, max(avail) * 1.20)
    ax.grid(True, axis="y", linestyle=":", alpha=0.3, color="#888888")
    ax.grid(False, axis="x")
    legend_patches = [
        mpatches.Patch(color="#888888", label="Available runway"),
        mpatches.Patch(color="#3498db", label="TODR"),
        mpatches.Patch(color="#e67e22", label="BFL"),
        mpatches.Patch(color="#e74c3c", label="ASD"),
        mpatches.Patch(color="#2ecc71", label="LDR"),
    ]
    ax.legend(
        handles=legend_patches,
        loc="upper right",
        fontsize=7.5,
        framealpha=0.25,
        facecolor=pal.panel,
        edgecolor=pal.spine,
        labelcolor=pal.tick,
    )


def _figure_lto(result, role: str, theme: Optional[str]) -> Optional[Figure]:
    report = getattr(result, "optimized_report", None)
    if report is None:
        return None
    from ..physics.performance import compute_field_performance, static_thrust_to_weight

    cfg = result.config
    which = cfg.departure_airport if role == "departure" else cfg.arrival_airport
    try:
        airport = _resolve_airport(which)
    except KeyError:
        return viz.figure_status_message(
            "Landing & Take-Off", f"Airport '{which}' not found.", theme=theme
        )

    req = cfg.requirements
    gs = report.geometry_summary
    wing_area = gs.get("wing_area_m2", report.airplane.wings[0].area())
    tw_sl = static_thrust_to_weight(cfg)
    perf = compute_field_performance(
        mtow_kg=req.mtow_kg,
        wing_area_m2=wing_area,
        airport=airport,
        cl_max_to=cfg.performance.cl_max_to,
        cl_max_land=cfg.performance.cl_max_land,
        tw_sl=tw_sl,
        k_land=cfg.performance.k_land,
        bfl_factor=cfg.performance.bfl_factor,
        perf_config=cfg.performance,
    )

    fig = Figure(figsize=(9, 8))
    pal = get_palette(theme)
    fig.patch.set_facecolor(pal.bg)
    ax_rw = fig.add_subplot(2, 1, 1)
    ax_bar = fig.add_subplot(2, 1, 2)
    _style_axes(fig, ax_rw, theme)
    _style_axes(fig, ax_bar, theme)
    _draw_runway(ax_rw, perf, theme)
    _draw_bars(ax_bar, perf, theme)
    fig.tight_layout()
    return fig


def figure_lto_departure(result, theme: Optional[str] = None):
    return _figure_lto(result, "departure", theme)


def figure_lto_arrival(result, theme: Optional[str] = None):
    return _figure_lto(result, "arrival", theme)


# ===========================================================================
# 3D route globe (pyvista off-screen screenshot; falls back to a status figure)
# ===========================================================================


def figure_route_globe(result, theme: Optional[str] = None) -> Optional[Figure]:
    route = getattr(result, "route", None)
    if route is None:
        return None
    try:
        from ..reporting.route_globe import build_globe_plotter, sync_mass_to_route

        mass_profile = altitude_profile = None
        mission = getattr(result, "mission_result", None)
        if mission is not None and getattr(mission, "status", None) == "ok":
            try:
                mass_profile, altitude_profile = sync_mass_to_route(route, mission)
            except Exception:
                mass_profile = altitude_profile = None
        plotter = build_globe_plotter(
            route, mass_profile, altitude_profile, off_screen=True
        )
        img = plotter.screenshot(return_img=True, window_size=(1000, 800))
        try:
            plotter.close()
        except Exception:
            pass
        fig = Figure(figsize=(9, 7))
        pal = get_palette(theme)
        fig.patch.set_facecolor(pal.bg)
        ax = fig.add_subplot(111)
        ax.imshow(img)
        ax.axis("off")
        return fig
    except Exception as exc:
        return viz.figure_status_message(
            "3D Globe",
            f"3D globe needs pyvista/VTK off-screen rendering.\n{exc}",
            theme=theme,
        )


EXTRA_FIGURES = {
    "matching_chart": figure_matching_chart,
    "lto_departure": figure_lto_departure,
    "lto_arrival": figure_lto_arrival,
    # "route_globe" (figure_route_globe, above) intentionally NOT registered
    # here any more -- the Mission & Route tab now renders an actual
    # interactive WebGL globe client-side (RouteGlobe.tsx, via three-globe)
    # from route_geo_data() below instead of embedding a static PyVista
    # off-screen screenshot. figure_route_globe/route_globe.py are left in
    # place (still directly importable/usable, e.g. for a future static
    # report export) rather than deleted.
}


def field_performance_data(result):
    """V-speeds + field distances for departure & arrival airports -- the
    numeric panel the Qt LTO widget shows as text (_update_vspeeds). Returns a
    JSON-safe dict or None if there's no optimized report."""
    report = getattr(result, "optimized_report", None)
    if report is None:
        return None
    from ..physics.performance import compute_field_performance, static_thrust_to_weight

    cfg = result.config
    gs = report.geometry_summary
    wing_area = gs.get("wing_area_m2", report.airplane.wings[0].area())
    tw_sl = static_thrust_to_weight(cfg)
    out = {}
    for role, apt_str in [
        ("departure", cfg.departure_airport),
        ("arrival", cfg.arrival_airport),
    ]:
        try:
            airport = _resolve_airport(apt_str)
        except KeyError:
            continue
        perf = compute_field_performance(
            mtow_kg=cfg.requirements.mtow_kg,
            wing_area_m2=wing_area,
            airport=airport,
            cl_max_to=cfg.performance.cl_max_to,
            cl_max_land=cfg.performance.cl_max_land,
            tw_sl=tw_sl,
            k_land=cfg.performance.k_land,
            bfl_factor=cfg.performance.bfl_factor,
            perf_config=cfg.performance,
        )
        vs = perf.v_speeds
        out[role] = {
            "airport": airport.name,
            "distances_m": {
                "TODR": float(perf.todr_m),
                "BFL": float(perf.bfl_m),
                "ASD": float(perf.asd_m),
                "LDR": float(perf.ldr_m),
                "TODA": float(perf.toda_m),
                "LDA": float(perf.lda_m),
            },
            "v_speeds_ms": {k: float(v) for k, v in vs.as_ms().items()},
            "v_speeds_kt": {k: float(v) for k, v in vs.as_knots().items()},
        }
    return {"tw_sl": float(tw_sl), "airports": out}


def route_geo_data(result):
    """Plain lat/lon/altitude (+ mass, if a mission ran) route data for the
    frontend's interactive WebGL globe (RouteGlobe.tsx) -- the client-side
    replacement for the old server-rendered-screenshot ``figure_route_globe``
    above. No matplotlib/PyVista involved; this is just JSON. Returns None if
    there's no route for this run (same "route is None" gate
    ``figure_route_globe`` used)."""
    route = getattr(result, "route", None)
    if route is None:
        return None

    from ..reporting.route_globe import sync_mass_to_route

    mass_kg = None
    altitude_m = [float(wp.alt_m) for wp in route.waypoints]
    mission = getattr(result, "mission_result", None)
    if mission is not None and getattr(mission, "status", None) == "ok":
        try:
            mass_profile, altitude_profile = sync_mass_to_route(route, mission)
            mass_kg = [float(v) for v in mass_profile]
            altitude_m = [float(v) for v in altitude_profile]
        except Exception:
            pass

    waypoints = [
        {
            "lat": float(wp.lat),
            "lon": float(wp.lon),
            "alt_m": altitude_m[i],
            "ident": wp.ident,
        }
        for i, wp in enumerate(route.waypoints)
    ]
    return {
        "source": route.source,
        "origin_ident": route.waypoints[0].ident or "DEP",
        "dest_ident": route.waypoints[-1].ident or "ARR",
        "waypoints": waypoints,
        "mass_kg": mass_kg,
        "total_distance_m": float(route.total_distance_m),
    }
