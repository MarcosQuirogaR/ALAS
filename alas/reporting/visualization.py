# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Visualization for ALAS.

Figure-factory pattern: every function *builds and returns* a matplotlib
``Figure`` and performs no physics. Functions optionally draw into a provided
``Figure`` (so a GUI can embed them in its own canvas) or create a standalone
one (so the CLI can save/show them). This keeps plotting embeddable, headless-
safe, and decoupled from any front-end.

The plots are deliberately **generic**: axis labels are physical quantities
(CL, CD, alpha, ...) and titles describe the physics, never a specific aircraft.
Any airplane the pipeline produces can be represented through them.
"""

from __future__ import annotations

from pathlib import Path
from typing import Dict, List, Optional

import aerosandbox as asb
import numpy as np
from matplotlib.figure import Figure

from ..analysis.full_analysis import AnalysisReport
from ..config.design_variables import DesignVector
from ..optimization.objective import OptimizationHistory
from ..physics.mass import OEW_KEYS
from .theme import get_palette, BASELINE_COLOR, OPTIMIZED_COLOR, GHOST_COLOR


# --- figure helpers ---------------------------------------------------------
def _ensure_fig(fig: Optional[Figure], figsize) -> Figure:
    """Return a cleared drawing surface: the supplied figure, or a fresh one."""
    if fig is None:
        fig = Figure(figsize=figsize, tight_layout=True)
    else:
        fig.clear()
    return fig


def _theme_figure(fig: Figure, pal) -> None:
    """Apply ``pal`` to every themeable element of ``fig``.

    Beyond the obvious per-axes facecolor/spine/tick/axis-label/title, this
    also covers two Text objects the naive "just color ax.title" approach
    misses -- both defaulting to matplotlib's black regardless of theme:
    ``fig.suptitle()`` (a Figure-level Text, not an Axes one) and the
    separate ``_left_title``/``_right_title`` Text objects created by
    ``ax.set_title(..., loc="left"/"right")`` (distinct from ``ax.title``,
    which is really `loc="center"`). 3-D axes keep getting their title
    colored too -- only spine/tick/pane theming is skipped for them, since
    mplot3d panes aren't part of this theme system.
    """
    fig.patch.set_facecolor(pal.bg)
    if fig._suptitle is not None:
        fig._suptitle.set_color(pal.title)
    for _theme_ax in fig.axes:
        _theme_ax.set_facecolor(pal.bg)
        _theme_ax.title.set_color(pal.title)
        if _theme_ax._left_title is not None:
            _theme_ax._left_title.set_color(pal.title)
        if _theme_ax._right_title is not None:
            _theme_ax._right_title.set_color(pal.title)
        if _theme_ax.name == "3d":
            # mplot3d draws its own axis "panes" -- separate fills, not
            # ax.get_facecolor() -- defaulting to a light grey regardless of
            # theme, and its tick/axis-label text was never touched by the
            # 2-D-only styling below either. Most of this codebase's own 3-D
            # axes call ax.set_axis_off() (nothing to theme here), but
            # AeroSandbox's own draw_three_view() (figure_asb_threeview)
            # keeps default axis chrome, which read as a jarring light
            # rectangle with unstyled text on a dark/grey theme. Leave the
            # light theme's own near-white default untouched (already
            # correct against a white page, and keeps headless report PNGs,
            # which always render at the "light" palette, unchanged).
            _theme_ax.tick_params(colors=pal.tick)
            for pane_axis in (_theme_ax.xaxis, _theme_ax.yaxis, _theme_ax.zaxis):
                pane_axis.label.set_color(pal.tick)
                if pal.bg != "#ffffff":
                    pane_axis.pane.set_facecolor(pal.bg)
                    pane_axis.pane.set_edgecolor(pal.spine)
        else:
            for spine in _theme_ax.spines.values():
                spine.set_edgecolor(pal.spine)
            _theme_ax.tick_params(colors=pal.tick)
            _theme_ax.xaxis.label.set_color(pal.tick)
            _theme_ax.yaxis.label.set_color(pal.tick)


def save_figure(fig: Figure, path: str | Path, dpi: int = 150) -> None:
    """Save a figure to disk (works on standalone figures, no pyplot needed)."""
    fig.savefig(Path(path), dpi=dpi, bbox_inches="tight", transparent=True)
    print(f"  [figure] saved: {path}")


def new_managed_figure(figsize=(10, 6)) -> Figure:
    """Create a pyplot-managed figure (CLI interactive path).

    Pass the result as the ``fig=`` argument to a factory function so the drawn
    figure has a window manager and can be displayed by :func:`show_all`.
    """
    import matplotlib.pyplot as plt

    return plt.figure(figsize=figsize)


def show_all() -> None:
    """Display all pyplot-managed figures (CLI use; needs a GUI backend)."""
    import matplotlib.pyplot as plt

    plt.show()


def _draw_planform(
    ax, plane, *, color, style="-", lw=2.0, label=None, fill=False, alpha=1.0
):
    """Draw the top-view outline of every wing of an airplane onto ``ax``."""
    first = True
    for w in plane.wings:
        le = [s.xyz_le for s in w.xsecs]
        te = [s.xyz_le + np.array([s.chord, 0, 0]) for s in w.xsecs]
        y = [p[1] for p in le] + [p[1] for p in reversed(te)]
        x = [p[0] for p in le] + [p[0] for p in reversed(te)]
        sides = [1, -1] if w.symmetric else [1]
        for side in sides:
            ys = [yi * side for yi in y]
            lbl = label if (first and side == 1) else None
            if fill:
                ax.fill(ys, x, color=color, alpha=alpha, edgecolor=None)
            else:
                ax.plot(
                    ys + [ys[0]],
                    x + [x[0]],
                    color=color,
                    linestyle=style,
                    linewidth=lw,
                    alpha=alpha,
                    label=lbl,
                )
        first = False


# --- optimization-run figures ----------------------------------------------
def figure_optimization_history(
    history: OptimizationHistory, fig: Optional[Figure] = None, theme: str | None = None
) -> Optional[Figure]:
    """L/D per valid evaluation, coloured by span, with running-best overlaid."""
    lds = np.array(history.l_over_d)
    if len(lds) == 0:
        return None
    spans = np.array(history.span_m)
    sim_nums = np.arange(len(lds)) + 1
    best_so_far = np.maximum.accumulate(lds)

    fig = _ensure_fig(fig, (10, 6))
    ax = fig.add_subplot(111)
    pal = get_palette(theme)
    _theme_figure(fig, pal)
    sc = ax.scatter(
        sim_nums, lds, c=spans, cmap="viridis", alpha=0.7, s=22, label="evaluation"
    )
    ax.plot(sim_nums, best_so_far, "r-", linewidth=2, label="best so far")
    ax.set_title(f"Optimization convergence ({len(lds)} valid evaluations)")
    ax.set_xlabel("valid evaluation #")
    ax.set_ylabel("L/D")
    ax.grid(True, alpha=0.3)
    ax.legend(loc="lower right")
    fig.colorbar(sc, ax=ax, label="span [m]")
    # A colorbar is a whole new Axes, added after _theme_figure ran above --
    # re-running it (idempotent) is the simplest way to theme it too.
    _theme_figure(fig, pal)
    return fig


def figure_design_evolution(
    history: OptimizationHistory,
    builder,
    max_samples: int = 60,
    fig: Optional[Figure] = None,
    theme: str | None = None,
) -> Optional[Figure]:
    """Morphological evolution: planforms of sampled valid designs, coloured
    from first (cool) to last (warm) evaluation. Rebuilds geometry per sample
    via the supplied ``AircraftBuilder``; fully generic to any configuration."""
    valid_dnas = [v for v, ok in zip(history.design_vectors, history.valid) if ok]
    if not valid_dnas:
        return None

    import matplotlib as mpl

    idxs = np.linspace(
        0, len(valid_dnas) - 1, num=min(max_samples, len(valid_dnas)), dtype=int
    )
    cmap = mpl.colormaps["turbo"]

    fig = _ensure_fig(fig, (12, 8))
    ax = fig.add_subplot(111)
    pal = get_palette(theme)
    _theme_figure(fig, pal)
    for i, idx in enumerate(idxs):
        progress = i / max(1, len(idxs) - 1)
        try:
            plane = builder.build(
                DesignVector.from_array(valid_dnas[idx]), include_engines=False
            )
        except Exception:
            continue
        _draw_planform(
            ax, plane, color=cmap(progress), fill=True, alpha=0.05 + 0.18 * progress
        )
    ax.set_aspect("equal")
    ax.invert_yaxis()
    ax.grid(True, alpha=0.3)
    ax.set_title("Design evolution (planform)")
    ax.set_xlabel("span Y [m]")
    ax.set_ylabel("longitudinal X [m]")
    return fig


# --- analysis figures -------------------------------------------------------
def figure_aero_panel(
    report: AnalysisReport, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Four-panel polar set: lift curve, drag polar, efficiency, stability."""
    p = report.polar
    fig = _ensure_fig(fig, (12, 9))
    ax = fig.subplots(2, 2)
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    ax[0, 0].plot(p["alpha"], p["CL"], "-o", color="tab:blue", markersize=4)
    ax[0, 0].set(xlabel="alpha [deg]", ylabel="CL", title="Lift curve")

    ax[0, 1].plot(
        p["CD"], p["CL"], "-o", color="tab:red", markersize=4, label="total (corrected)"
    )
    ax[0, 1].plot(
        p["CD_induced"], p["CL"], "--.", color="tab:blue", label="induced (VLM)"
    )
    ax[0, 1].set(xlabel="CD", ylabel="CL", title="Drag polar")
    ax[0, 1].legend()

    ax[1, 0].plot(p["CL"], p["L/D"], color="tab:purple", linewidth=2)
    ax[1, 0].axvline(report.design_point.cl, color="tab:red", ls=":", label="design CL")
    ax[1, 0].set(xlabel="CL", ylabel="L/D", title="Efficiency")
    ax[1, 0].legend()

    ax[1, 1].plot(p["alpha"], p["Cm"], color="tab:blue")
    ax[1, 1].axhline(0, color="r", ls="--")
    ax[1, 1].set(xlabel="alpha [deg]", ylabel="Cm", title="Longitudinal stability")

    for a in ax.ravel():
        a.grid(True, alpha=0.3)
    return fig


def _compliance_suffix(report: AnalysisReport) -> str:
    """Short ' [SM=x.x%, CG env. OK/VIOLATED]' tag for a comparison-plot legend.

    A raw L/D overlay alone can make an optimized design look "worse" than a
    baseline that is aerodynamically cleaner but physically non-compliant --
    the optimizer trades L/D for CG-envelope/static-margin compliance (see
    docs/architecture.md Sec 5), which the curves alone don't
    show. Surfacing each design's compliance status here is what turns "the
    optimized polar looks worse, is that a bug?" into a visible, explainable
    trade-off instead of a mystery.
    """
    parts = []
    sm = getattr(report, "static_margin", float("nan"))
    if sm == sm:
        parts.append(f"SM={sm * 100:.1f}%")
    ok = getattr(report, "cg_envelope_ok", None)
    if ok is True:
        parts.append("CG env. OK")
    elif ok is False:
        parts.append("CG env. VIOLATED")
    return f"  [{', '.join(parts)}]" if parts else ""


def figure_polar_comparison(
    baseline: AnalysisReport,
    optimized: AnalysisReport,
    labels=("baseline", "optimized"),
    fig: Optional[Figure] = None,
    theme: str | None = None,
    ghost_reports: list | None = None,
) -> Figure:
    """Overlay two designs' drag polar and efficiency curves.

    Legend labels are annotated with each design's static margin and CG-
    envelope compliance (see :func:`_compliance_suffix`): if the baseline
    shows a higher L/D but is CG/SM non-compliant, that's the optimizer
    correctly trading raw efficiency for a flyable design, not a regression.

    ``ghost_reports`` is an optional list of prior :class:`PipelineResult`
    objects (completed results from earlier runs) whose optimized polars are
    overlaid as dashed traces in ``GHOST_COLOR`` at low alpha for visual
    comparison. Existing callers that omit this kwarg see no change.
    """
    fig = _ensure_fig(fig, (13, 6))
    ax = fig.subplots(1, 2)
    pal = get_palette(theme)
    _theme_figure(fig, pal)
    b, o = baseline.polar, optimized.polar
    label_b = labels[0] + _compliance_suffix(baseline)
    label_o = labels[1] + _compliance_suffix(optimized)

    # Ghost traces (prior runs) drawn first so they sit behind the current ones.
    # Each ghost entry is a PipelineResult; we pull its optimized_report's polar.
    if ghost_reports:
        for i, ghost in enumerate(ghost_reports):
            try:
                g_report = (
                    ghost.optimized_report
                    if hasattr(ghost, "optimized_report")
                    and ghost.optimized_report is not None
                    else ghost
                )
                g = g_report.polar
                lbl = f"Prior run {i + 1}"
                ax[0].plot(
                    g["CD"],
                    g["CL"],
                    "--",
                    color=GHOST_COLOR,
                    alpha=0.45,
                    linewidth=1.2,
                    label=lbl,
                )
                ax[1].plot(
                    g["CL"],
                    g["L/D"],
                    "--",
                    color=GHOST_COLOR,
                    alpha=0.45,
                    linewidth=1.2,
                    label=lbl,
                )
            except Exception:
                pass  # non-critical: skip a ghost that can't be plotted

    ax[0].plot(
        b["CD"], b["CL"], "--o", color=BASELINE_COLOR, markersize=4, label=label_b
    )
    ax[0].plot(
        o["CD"], o["CL"], "-o", color=OPTIMIZED_COLOR, markersize=4, label=label_o
    )
    ax[0].set(xlabel="CD", ylabel="CL", title="Drag polar")

    ax[1].plot(b["CL"], b["L/D"], "--", color=BASELINE_COLOR, label=label_b)
    ax[1].plot(
        o["CL"], o["L/D"], "-", color=OPTIMIZED_COLOR, linewidth=2, label=label_o
    )
    ax[1].set(xlabel="CL", ylabel="L/D", title="Efficiency")

    for a in ax:
        a.grid(True, alpha=0.3)
        a.legend(fontsize=8)
    return fig


def figure_model_comparison(
    report: "AnalysisReport",
    mission_result=None,
    mses_result=None,
    fig: Optional[Figure] = None,
    theme: str | None = None,
) -> Figure:
    """Overlay AeroSandbox / SUAVE / MSES on shared CL-CD and CL-alpha axes.

    Each model captures different physics, and this is deliberately NOT an
    apples-to-apples reconciliation -- the point is to show what each one
    sees that the others don't:

    * **AeroSandbox** (this app's own polar, ``report.polar``): inviscid VLM
      lift/induced drag (3-D, whole aircraft) + empirical parasite/wave drag
      correlations. Fast, used for the optimizer search and the primary report.
    * **SUAVE** (``mission_result``, cruise-segment points only -- filtered by
      ``Segment`` starting with "cruise"): a full mission-trajectory
      aerodynamic model (3-D, whole aircraft, its own empirical buildup),
      evaluated at whatever CL the trajectory actually flew, not a clean sweep.
    * **MSES** (``mses_result``): a real coupled viscous/inviscid Euler +
      integral-boundary-layer solve on the optimized design's ROOT AIRFOIL
      SECTION alone (2-D) at the sweep-corrected effective section Mach --
      captures true transition/separation physics, but excludes 3-D induced
      drag and every other component's parasite drag, so its CD is expected
      to read lower and its apparent L/D higher than the other two 3-D,
      whole-aircraft models. See physics.mses_analysis's module docstring.

    Any model that's unavailable (mission/MSES disabled, or a non-"ok"
    status) is simply omitted from the legend, not shown as an error.
    """
    fig = _ensure_fig(fig, (13, 10))
    axs = fig.subplots(2, 2)
    ax_drag, ax_lift = axs[0, 0], axs[0, 1]
    ax_cm, ax_eff = axs[1, 0], axs[1, 1]
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    b = report.polar

    # --- AeroSandbox (Baseline) ---
    l_b = "AeroSandbox (3-D, whole aircraft)"
    c_b = "tab:blue"
    ax_drag.plot(b["CD"], b["CL"], "-o", color=c_b, markersize=4, label=l_b)
    ax_lift.plot(b["alpha"], b["CL"], "-o", color=c_b, markersize=4, label=l_b)
    if "Cm" in b:
        ax_cm.plot(b["alpha"], b["Cm"], "-o", color=c_b, markersize=4, label=l_b)
    if "L/D" in b:
        ax_eff.plot(b["alpha"], b["L/D"], "-o", color=c_b, markersize=4, label=l_b)

    # --- SUAVE (Mission trajectory) ---
    if mission_result is not None and getattr(mission_result, "status", None) == "ok":
        cols = mission_result.columns
        segs = cols.get("Segment", [])
        cl_all = cols.get("CL", [])
        cd_all = cols.get("CD", [])
        aoa_all = cols.get("AoA_deg", [])

        idx = [i for i, s in enumerate(segs) if str(s).lower().startswith("cruise")]
        if idx and cl_all and cd_all:
            cl = [cl_all[i] for i in idx]
            cd = [cd_all[i] for i in idx]
            l_s = "SUAVE (3-D, whole aircraft, mission-trajectory)"
            c_s = "tab:green"
            ax_drag.plot(cd, cl, "s", color=c_s, markersize=6, label=l_s)

            if aoa_all:
                aoa = [aoa_all[i] for i in idx]
                ax_lift.plot(aoa, cl, "s", color=c_s, markersize=6, label=l_s)
                # SUAVE efficiency
                eff = [cl_ / cd_ for cl_, cd_ in zip(cl, cd)]
                ax_eff.plot(aoa, eff, "s", color=c_s, markersize=6, label=l_s)

                # If SUAVE provides Cm
                cm_all = cols.get("CM", [])
                if cm_all:
                    cm = [cm_all[i] for i in idx]
                    ax_cm.plot(aoa, cm, "s", color=c_s, markersize=6, label=l_s)

    # --- MSES (2-D Section) ---
    if (
        mses_result is not None
        and getattr(mses_result, "status", None) == "ok"
        and mses_result.CL
    ):
        l_m = "MSES (2-D root section, viscous-compressible)"
        c_m = "tab:orange"
        ax_drag.plot(
            mses_result.CD, mses_result.CL, "^-", color=c_m, markersize=6, label=l_m
        )
        ax_lift.plot(
            mses_result.alpha_deg,
            mses_result.CL,
            "^-",
            color=c_m,
            markersize=6,
            label=l_m,
        )
        if getattr(mses_result, "CM", None):
            ax_cm.plot(
                mses_result.alpha_deg,
                mses_result.CM,
                "^-",
                color=c_m,
                markersize=6,
                label=l_m,
            )
        if getattr(mses_result, "l_over_d", None):
            ax_eff.plot(
                mses_result.alpha_deg,
                mses_result.l_over_d,
                "^-",
                color=c_m,
                markersize=6,
                label=l_m,
            )

    ax_drag.set(xlabel="CD", ylabel="CL", title="Drag polar")
    ax_lift.set(xlabel="Alpha [deg]", ylabel="CL", title="Lift curve")
    ax_cm.set(xlabel="Alpha [deg]", ylabel="Cm", title="Pitching Moment")
    ax_eff.set(xlabel="Alpha [deg]", ylabel="L/D", title="Efficiency (L/D)")

    for a in axs.flatten():
        a.grid(True, alpha=0.3)
        if a.get_legend_handles_labels()[1]:
            a.legend(fontsize=7.5, loc="best")

    fig.suptitle(
        "Model Comparison -- shared variables across AeroSandbox / SUAVE / MSES",
        fontsize=10,
        fontweight="bold",
        color=pal.title,
    )
    fig.set_layout_engine("tight")
    return fig


def figure_mses_pressure_distribution(
    mses_pressure, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Surface Cp and local-Mach distribution from an MSES solve, at the
    trimmed cruise operating point on the optimized design's root section.

    This is spatial information neither AeroSandbox's VLM nor SUAVE's
    mission-level aerodynamics expose (both report only integrated
    coefficients, not a surface pressure/Mach distribution) -- MSES's real
    coupled viscous/inviscid solve is what makes this plot possible. The
    Cp axis follows the standard aerodynamic convention (inverted, so
    suction/negative Cp plots upward); the Mach panel marks M=1 to show
    where a supersonic pocket (and therefore wave drag) exists.
    """
    fig = _ensure_fig(fig, (11, 5))
    ax = fig.subplots(1, 2)
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    ax[0].plot(
        mses_pressure.x_upper,
        mses_pressure.cp_upper,
        "-o",
        color="tab:blue",
        markersize=3,
        label="Upper surface",
    )
    ax[0].plot(
        mses_pressure.x_lower,
        mses_pressure.cp_lower,
        "-o",
        color="tab:red",
        markersize=3,
        label="Lower surface",
    )
    ax[0].invert_yaxis()
    ax[0].set(xlabel="x/c", ylabel="Cp", title="Surface pressure distribution")
    ax[0].axhline(0, color="gray", lw=0.7, alpha=0.5)
    ax[0].grid(True, alpha=0.3)
    ax[0].legend(fontsize=8)

    ax[1].plot(
        mses_pressure.x_upper,
        mses_pressure.mach_upper,
        "-o",
        color="tab:blue",
        markersize=3,
        label="Upper surface",
    )
    ax[1].plot(
        mses_pressure.x_lower,
        mses_pressure.mach_lower,
        "-o",
        color="tab:red",
        markersize=3,
        label="Lower surface",
    )
    ax[1].axhline(1.0, color="k", ls="--", lw=1, alpha=0.6, label="M = 1 (sonic)")
    ax[1].set(xlabel="x/c", ylabel="Local Mach", title="Surface Mach distribution")
    ax[1].grid(True, alpha=0.3)
    ax[1].legend(fontsize=8)

    fig.suptitle(
        f"MSES Root-Section Pressure Field  (alpha = {mses_pressure.alpha_deg:.2f} deg)",
        fontsize=10,
        fontweight="bold",
        color=pal.title,
    )
    fig.set_layout_engine(None)
    fig.subplots_adjust(top=0.86, bottom=0.13, wspace=0.28)
    return fig


def figure_mses_mach_contours(
    mses_pressure, airfoil=None, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Filled contour plot of the Mach field from MSES.

    Draws the outline of the EXACT panelled geometry MSES solved
    (``mses_pressure.airfoil_x/airfoil_y``, populated in
    ``run_mses_pressure_distribution``) rather than any airfoil object a
    caller might pass in -- the section MSES actually analyzes is the
    optimized/morphed root section (post-repanel), which is generally NOT
    the same shape as a named preset like ``config.geometry.wing.root_airfoil``
    (passing that in as ``airfoil`` would silently draw nothing: it is a
    plain string, so ``hasattr(airfoil, "coordinates")`` is always False).
    The ``airfoil`` parameter is kept only as a fallback for results that
    predate the stored coordinates.
    """
    fig = _ensure_fig(fig, (11, 5))
    ax = fig.subplots()
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    if not getattr(mses_pressure, "field_x", None):
        ax.text(
            0.5,
            0.5,
            "No Mach field data available",
            ha="center",
            va="center",
            color=pal.title,
        )
        return fig

    import numpy as np

    x = np.array(mses_pressure.field_x)
    y = np.array(mses_pressure.field_y)
    m = np.array(mses_pressure.field_mach)

    # Filter out extreme far-field to focus on the airfoil
    mask = (x > -0.5) & (x < 1.5) & (y > -1.0) & (y < 1.0)
    x, y, m = x[mask], y[mask], m[mask]

    if len(x) > 3:
        levels = np.linspace(max(0, m.min()), m.max(), 40)
        cnt = ax.tricontourf(x, y, m, levels=levels, cmap="turbo", extend="both")
        fig.colorbar(cnt, ax=ax, label="Mach")

        # Plot airfoil surface: prefer the coordinates MSES actually solved.
        outline_x = getattr(mses_pressure, "airfoil_x", None)
        outline_y = getattr(mses_pressure, "airfoil_y", None)
        if outline_x:
            ax.fill(outline_x, outline_y, color="lightgray", zorder=3, ec="black")
        elif airfoil is not None and hasattr(airfoil, "coordinates"):
            ax.fill(
                airfoil.coordinates[:, 0],
                airfoil.coordinates[:, 1],
                color="lightgray",
                zorder=3,
                ec="black",
            )

        ax.set_aspect("equal")
        ax.set(
            xlabel="x/c",
            ylabel="y/c",
            title=f"Mach Contours (alpha = {mses_pressure.alpha_deg:.2f} deg)",
        )
        # The colorbar above is a whole new Axes, added after _theme_figure
        # ran; re-running it (idempotent) is the simplest way to theme it too.
        _theme_figure(fig, pal)

    fig.set_layout_engine("tight")
    return fig


def figure_status_message(
    title: str,
    message: str,
    ok: bool = False,
    fig: Optional[Figure] = None,
    theme: str | None = None,
) -> Figure:
    """A deliberately minimal, chart-sized status note -- for surfacing WHY
    an optional analysis (MSES, SUAVE mission) is missing from a results tab
    instead of silently omitting it with no explanation. Red/left-aligned
    text for a failure, green for an informational success note.
    """
    fig = _ensure_fig(fig, (10, 2.4))
    fig.set_layout_engine(None)
    fig.subplots_adjust(top=0.85, bottom=0.1, left=0.04, right=0.98)
    ax = fig.add_subplot(111)
    pal = get_palette(theme)
    _theme_figure(fig, pal)
    ax.axis("off")
    color = "#27ae60" if ok else "#c0392b"
    ax.text(
        0.01,
        0.85,
        title,
        transform=ax.transAxes,
        ha="left",
        va="top",
        fontsize=12,
        fontweight="bold",
        color=color,
    )
    ax.text(
        0.01,
        0.45,
        message,
        transform=ax.transAxes,
        ha="left",
        va="top",
        fontsize=10,
        color=pal.tick,
        wrap=True,
    )
    return fig


def figure_planform_comparison(
    baseline_plane,
    optimized_plane,
    labels=("baseline", "optimized"),
    fig: Optional[Figure] = None,
    theme: str | None = None,
) -> Figure:
    """Top-view planform overlay of two airplanes."""
    fig = _ensure_fig(fig, (11, 8))
    ax = fig.add_subplot(111)
    pal = get_palette(theme)
    _theme_figure(fig, pal)
    _draw_planform(
        ax, baseline_plane, color=BASELINE_COLOR, style="--", label=labels[0]
    )
    _draw_planform(
        ax, optimized_plane, color=OPTIMIZED_COLOR, style="-", label=labels[1]
    )
    ax.set_aspect("equal")
    ax.invert_yaxis()
    ax.grid(True, alpha=0.3)
    ax.legend()
    ax.set(xlabel="span Y [m]", ylabel="longitudinal X [m]", title="Planform")
    return fig


def figure_asb_threeview(plane, theme: str | None = None) -> Optional[Figure]:
    """Return ASB's native four-panel 3D three-view (top/front/side/isometric).

    Calls ``Airplane.draw_three_view(show=False)`` and captures the pyplot figure
    it creates internally.  The figure contains four 3-D ``Axes3D`` subplots and
    is ready to be embedded via ``MplCanvas.set_figure()``.

    Must be called from the main (GUI) thread -- matplotlib 3-D axes are not
    thread-safe.

    ``Airplane.draw_three_view`` lazily imports AeroSandbox's own
    ``tools.pretty_plots`` the first time it's called, which runs
    ``seaborn.set_theme()`` as an import-time side effect -- a *global*,
    process-wide ``matplotlib.rcParams`` change (font family falls back to
    Arial before DejaVu Sans, among other things) that otherwise silently
    persists for every figure drawn afterward, in this function or anywhere
    else in the app. Concretely: Arial doesn't have the U+221D ("proportional
    to", "∝") glyph DejaVu Sans does, so any *later* chart using it would
    render a missing-glyph tofu box instead of the symbol -- and everything
    drawn after this call quietly shifts to a different visual style besides.
    Isolate the side effect to just this call with ``rc_context``, which
    snapshots and restores rcParams around it.
    """
    import matplotlib as mpl
    import matplotlib.pyplot as plt

    plt.close("all")
    try:
        with mpl.rc_context():
            plane.draw_three_view(show=False)
            fig = plt.gcf()
        pal = get_palette(theme)
        _theme_figure(fig, pal)
        return fig
    except Exception:
        return None


def figure_drag_breakdown(
    report: AnalysisReport, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Two-panel drag breakdown at the cruise design point.

    Left  -- stacked bar: CD parasite / induced / wave at the design CL.
    Right -- efficiency curve with design point marked and CD0 / CDi labelled.
    """
    fig = _ensure_fig(fig, (11, 5))
    ax_bar, ax_eff = fig.subplots(1, 2)
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    p = report.polar
    dp = report.design_point
    fit = report.polar_fit

    # Find index closest to design CL
    idx = int(np.argmin(np.abs(p["CL"] - dp.cl)))
    cd_p = float(p.get("CD_parasite", np.full_like(p["CL"], fit.cd0))[idx])
    cd_i = float(p.get("CD_induced", np.full_like(p["CL"], fit.k * dp.cl**2))[idx])
    cd_w = float(p.get("CD_wave", np.zeros_like(p["CL"]))[idx])

    cd_total = max(cd_p + cd_i + cd_w, 1e-9)

    # Stacked bar
    bar_x = ["Design point"]
    ax_bar.bar(bar_x, [cd_p], label="Parasite (CD0)", color="tab:blue", width=0.3)
    ax_bar.bar(
        bar_x,
        [cd_i],
        bottom=[cd_p],
        label="Induced (CDi)",
        color="tab:orange",
        width=0.3,
    )
    ax_bar.bar(
        bar_x,
        [cd_w],
        bottom=[cd_p + cd_i],
        label="Wave (CDwave)",
        color="tab:red",
        width=0.3,
    )

    # The wave-drag segment is frequently a fraction of a percent of the
    # total (cruising comfortably below drag divergence is the whole point
    # of a supercritical section) -- too thin a sliver to see at all in a
    # linear stacked bar next to CD0/CDi. Label every segment's actual value
    # so "is wave drag even there?" has a legible answer regardless of how
    # tall its sliver renders, with a leader line for the wave segment since
    # its bar height alone can't anchor readable text.
    for val, y0, name in [(cd_p, cd_p / 2, "CD0"), (cd_i, cd_p + cd_i / 2, "CDi")]:
        if val / cd_total > 0.03:
            ax_bar.text(
                0,
                y0,
                f"{name}\n{val:.4f}",
                ha="center",
                va="center",
                fontsize=8,
                color="white",
                fontweight="bold",
            )

    y_wave = cd_p + cd_i + cd_w
    ax_bar.annotate(
        f"Wave = {cd_w:.2e}  ({cd_w / cd_total * 100:.3f}% of CD)",
        xy=(0.15, y_wave),
        xytext=(0.55, cd_p + cd_i + 0.12 * (cd_p + cd_i)),
        fontsize=8,
        color="tab:red",
        fontweight="bold",
        ha="left",
        va="center",
        arrowprops=dict(arrowstyle="->", color="tab:red", lw=1.2),
    )

    # The "Wave = ..." annotation above intentionally sits above the bar
    # top (at 1.12x the CD0+CDi height) so its leader line has room to
    # point down at the sliver-thin wave segment. Text extents aren't
    # included in matplotlib's autoscale, so without an explicit ylim the
    # axes only grow to fit the *bars* -- leaving the annotation clipped
    # against (and overlapping) the title above it. Give it headroom.
    ax_bar.set_xlim(-1, 1)
    ax_bar.set_ylim(0, cd_total * 1.3)
    ax_bar.set(ylabel="CD", title="Drag breakdown at design CL")
    ax_bar.legend()
    ax_bar.grid(True, axis="y", alpha=0.3)
    ax_bar.set_xticks([])

    # Efficiency curve
    ax_eff.plot(p["CL"], p["L/D"], color="tab:purple", linewidth=2)
    ax_eff.axvline(dp.cl, color="tab:red", ls=":", label=f"Design CL = {dp.cl:.3f}")
    ax_eff.axhline(
        dp.l_over_d, color="#e6c200", ls="--", label=f"L/D = {dp.l_over_d:.2f}"
    )
    ax_eff.scatter([dp.cl], [dp.l_over_d], color="red", zorder=5)
    ax_eff.set(xlabel="CL", ylabel="L/D", title="Efficiency curve")
    ax_eff.legend(fontsize=8)
    ax_eff.grid(True, alpha=0.3)

    return fig


def figure_vn_diagram(
    report: AnalysisReport,
    config,
    fig: Optional[Figure] = None,
    theme: str | None = None,
) -> Figure:
    """CS-25-style V-n (flight envelope) diagram.

    Every speed/load-factor boundary is derived from ``requirements``/
    ``performance`` config and the analyzed design's own wing area/MTOW
    (:func:`physics.performance.build_vn_diagram`) -- label positions and
    axis limits are computed from those actual values, not fixed offsets
    tuned for one aircraft, so the diagram stays correctly proportioned and
    legible for any design. Zone colors follow the usual convention: green
    (normal ops, <= VC), yellow (caution, VC..VD), orange (limit..ultimate
    structural margin), red (cannot be exceeded -- stall boundary or
    beyond VD).
    """
    import matplotlib.patheffects as pe
    import matplotlib.patches as mpatches
    from ..physics.performance import build_vn_diagram

    req = config.requirements
    data = build_vn_diagram(
        report.airplane, req, config.performance, req.cruise_altitude_m
    )

    fig = _ensure_fig(fig, (11, 7.5))
    ax = fig.add_subplot(111)
    pal = get_palette(theme)
    _theme_figure(fig, pal)
    # Label fill/outline must contrast with whatever's underneath (the
    # colored envelope bands, or bare page background outside them) --
    # a fixed white outline collapses into the label fill itself when the
    # theme's default text color (set process-wide via rcParams by the GUI
    # theme switcher) is also white on a dark/grey theme, rendering as an
    # unreadable white-on-white blob instead of outlined text.
    label_fg = pal.title
    label_outline = "black" if label_fg == "#ffffff" else "white"
    txt_style = [pe.withStroke(linewidth=2.5, foreground=label_outline)]

    v = data.v_kt
    n_lim_pos, n_lim_neg = data.n_lim_pos, data.n_lim_neg
    n_ult_pos, n_ult_neg = data.n_ult_pos, data.n_ult_neg
    v_a, v_c, v_d = data.v_a_kt, data.v_c_kt, data.v_d_kt

    mask = v <= v_d
    v_env = v[mask]
    stall_pos = data.n_stall_pos[mask]
    stall_neg = data.n_stall_neg[mask]

    # Full stall/overspeed envelope (outermost -- cannot be exceeded).
    ax.fill_between(v_env, stall_neg, stall_pos, color="#ff4d4d", alpha=0.35, zorder=1)
    ax.axvspan(v_d, v[-1], color="#ff4d4d", alpha=0.35, zorder=1)

    # Structural margin band: limit load -> ultimate load, wherever the
    # stall boundary allows it (pinches to zero width inside the
    # stall-limited region below Va, where the aircraft physically cannot
    # reach even the limit load). Both edges get a boundary line -- the
    # reference version this is adapted from only outlined the limit-load
    # edge, leaving the ultimate-load edge to fade into blank space.
    org_up_pos = np.minimum(stall_pos, n_ult_pos)
    org_lo_pos = np.minimum(stall_pos, n_lim_pos)
    org_up_neg = np.maximum(stall_neg, n_ult_neg)
    org_lo_neg = np.maximum(stall_neg, n_lim_neg)
    ax.fill_between(v_env, org_lo_pos, org_up_pos, color="#ff9933", zorder=2)
    ax.fill_between(v_env, org_up_neg, org_lo_neg, color="#ff9933", zorder=2)
    ax.plot(v_env, org_up_pos, color="#994c00", linewidth=1.1, zorder=6)
    ax.plot(v_env, org_up_neg, color="#994c00", linewidth=1.1, zorder=6)

    # Caution band (VC..VD) and safe band (<=VC), both bounded at limit load.
    lim_up = np.minimum(stall_pos, n_lim_pos)
    lim_dw = np.maximum(stall_neg, n_lim_neg)
    caution = v_env >= v_c
    ax.fill_between(v_env, lim_dw, lim_up, where=caution, color="#ffeb3b", zorder=3)
    ax.fill_between(v_env, lim_dw, lim_up, where=~caution, color="#66cc66", zorder=4)
    ax.plot(v_env, lim_up, "k-", linewidth=1.6, zorder=6)
    ax.plot(v_env, lim_dw, "k-", linewidth=1.6, zorder=6)
    ax.vlines(v_d, n_ult_neg, n_ult_pos, colors="red", linewidth=3, zorder=6)

    # Stall-limited region: no load factor is sustainable below Vs.
    ax.axvspan(
        0,
        data.v_s_kt,
        facecolor="none",
        edgecolor=(0, 0, 0, 0.15),
        hatch="///",
        zorder=5,
    )

    # Markers + labels. Offsets are in "offset points" (matplotlib's
    # resolution-independent unit, ~pixels at render time) with the
    # side/alignment chosen from each point's actual position relative to
    # the others -- not fixed absolute-speed nudges tuned for one aircraft.
    def label(x, y, text, dx=0, dy=10, **kw):
        kw.setdefault("color", label_fg)
        ax.annotate(
            text,
            (x, y),
            xytext=(dx, dy),
            textcoords="offset points",
            fontweight="bold",
            zorder=7,
            path_effects=txt_style,
            **kw,
        )

    ax.plot(v_a, n_lim_pos, "ko", zorder=7)
    label(v_a, n_lim_pos, f"$V_A$\n{v_a:.0f} kt", ha="center")

    ax.plot(data.v_s_kt, 1.0, "ko", zorder=7)
    label(
        data.v_s_kt,
        1.0,
        f"$V_S$\n{data.v_s_kt:.0f} kt",
        dx=-10,
        dy=0,
        ha="right",
        va="center",
    )

    label(
        v_d, n_ult_pos, f"$V_D$\n{v_d:.0f} kt", dx=-6, dy=6, ha="right", color="#c0392b"
    )

    ax.plot(data.v_cruise_op_kt, 1.0, "D", color="tab:blue", markersize=8, zorder=7)
    label(
        data.v_cruise_op_kt,
        1.0,
        f"Cruise\n{data.v_cruise_op_kt:.0f} kt",
        dx=10,
        dy=10,
        ha="left",
        color="tab:blue",
    )

    legend_patches = [
        mpatches.Patch(color="#66cc66", label="Normal (<= VC)"),
        mpatches.Patch(color="#ffeb3b", label="Caution (VC..VD)"),
        mpatches.Patch(color="#ff9933", label="Structural margin (limit..ultimate)"),
        mpatches.Patch(color="#ff4d4d", alpha=0.6, label="Never exceed"),
    ]
    ax.legend(handles=legend_patches, loc="lower left", fontsize=8, framealpha=0.9)

    ax.set_xlim(0, v[-1])
    y_pad = 0.15 * (n_ult_pos - n_ult_neg)
    ax.set_ylim(n_ult_neg - y_pad, n_ult_pos + y_pad)
    ax.set_xlabel("Equivalent airspeed (kt)")
    ax.set_ylabel("Load factor n")
    ax.set_title(f"V-n Diagram (MTOW {req.mtow_kg / 1000:.0f} t)", fontweight="bold")
    ax.grid(True, linestyle="--", alpha=0.3)
    return fig


def figure_geometry(
    plane, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Generic three-projection geometry view (top / side / front) of any
    airplane, built from its wing and fuselage primitives."""
    fig = _ensure_fig(fig, (12, 9))
    ax_top = fig.add_subplot(2, 2, 1)
    ax_side = fig.add_subplot(2, 2, 3)
    ax_front = fig.add_subplot(2, 2, 4)
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    def wing_loops(w):
        le = [s.xyz_le for s in w.xsecs]
        te = [s.xyz_le + np.array([s.chord, 0, 0]) for s in w.xsecs]
        x = [p[0] for p in le] + [p[0] for p in reversed(te)]
        y = [p[1] for p in le] + [p[1] for p in reversed(te)]
        z = [p[2] for p in le] + [p[2] for p in reversed(te)]
        return x, y, z

    for w in plane.wings:
        x, y, z = wing_loops(w)
        for side in [1, -1] if w.symmetric else [1]:
            ys = [yi * side for yi in y]
            ax_top.fill(
                ys, x, color="tab:blue", alpha=0.35, edgecolor="k", linewidth=0.5
            )
            ax_side.fill(
                x, z, color="tab:blue", alpha=0.35, edgecolor="k", linewidth=0.5
            )
            ax_front.fill(
                ys, z, color="tab:blue", alpha=0.35, edgecolor="k", linewidth=0.5
            )

    for f in plane.fuselages:
        xc = [s.xyz_c[0] for s in f.xsecs]
        zc = [s.xyz_c[2] for s in f.xsecs]
        # FuselageXSec never actually keeps a .radius attribute (a circular
        # section built via radius= gets converted to width/height at
        # construction), so this always silently fell back to a hardcoded
        # 0.5 m -- every aircraft's fuselage rendered at the same fixed
        # ~1 m diameter here regardless of its real size.
        r = [float(s.width) / 2 for s in f.xsecs]
        x_loop = xc + xc[::-1]
        z_loop = [z + ri for z, ri in zip(zc, r)] + [z - ri for z, ri in zip(zc, r)][
            ::-1
        ]
        y_loop = r + [-ri for ri in reversed(r)]
        ax_side.fill(x_loop, z_loop, color="tab:gray", alpha=0.4)
        ax_top.fill(y_loop, x_loop, color="tab:gray", alpha=0.4)
        ax_front.fill(y_loop, z_loop, color="tab:gray", alpha=0.2)

    ax_top.set(title="top", xlabel="Y [m]", ylabel="X [m]")
    ax_top.invert_yaxis()
    ax_side.set(title="side", xlabel="X [m]", ylabel="Z [m]")
    ax_front.set(title="front", xlabel="Y [m]", ylabel="Z [m]")
    for a in (ax_top, ax_side, ax_front):
        a.set_aspect("equal", adjustable="datalim")
        a.grid(True, alpha=0.3)
    return fig


def figure_airfoil_comparison(
    base_airfoil_name: str,
    optimized_design: DesignVector,
    fig: Optional[Figure] = None,
    theme: str | None = None,
) -> Figure:
    """Compare the baseline library airfoil with the optimized (bumped & morphed) airfoil."""
    from ..geometry.airfoils import AirfoilLibrary, build_section

    base_airfoil = AirfoilLibrary.get(base_airfoil_name)
    base_coords = base_airfoil.coordinates
    opt_airfoil = build_section(optimized_design, base_coords)

    fig = _ensure_fig(fig, (10, 5))
    ax = fig.add_subplot(111)
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    ax.plot(
        base_coords[:, 0],
        base_coords[:, 1],
        color=BASELINE_COLOR,
        linestyle="--",
        label=f"Original ({base_airfoil_name})",
        alpha=0.7,
    )
    ax.plot(
        opt_airfoil.coordinates[:, 0],
        opt_airfoil.coordinates[:, 1],
        color=OPTIMIZED_COLOR,
        linestyle="-",
        label="Optimized",
        linewidth=2,
    )

    ax.set_aspect("equal")
    ax.set_xlabel("x/c [-]")
    ax.set_ylabel("y/c [-]")
    ax.set_title("Airfoil Comparison: Original vs Optimized")
    ax.grid(True, alpha=0.3)
    ax.legend()
    return fig


def figure_airfoil_evolution(
    plane, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Plot the airfoil coordinates for each cross-section of the main wing,
    colored by spanwise position.
    """
    import matplotlib as mpl

    main_wing = next((w for w in plane.wings if w.name == "Main Wing"), plane.wings[0])
    max_y = max(abs(xsec.xyz_le[1]) for xsec in main_wing.xsecs)

    fig = _ensure_fig(fig, (10, 5))
    ax = fig.add_subplot(111)
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    cmap = mpl.colormaps["plasma"]
    norm = mpl.colors.Normalize(vmin=0, vmax=max_y)

    for xsec in main_wing.xsecs:
        y_val = abs(xsec.xyz_le[1])
        coords = xsec.airfoil.coordinates
        ax.plot(coords[:, 0], coords[:, 1], color=cmap(norm(y_val)), alpha=0.6)

    ax.set_aspect("equal")
    ax.set_xlabel("x/c [-]")
    ax.set_ylabel("y/c [-]")
    ax.set_title(f"Wing cross-sections — {len(main_wing.xsecs)} stations (root → tip)")
    ax.grid(True, alpha=0.3)

    sm = mpl.cm.ScalarMappable(cmap=cmap, norm=norm)
    sm.set_array([])
    fig.colorbar(sm, ax=ax, label="Spanwise station y [m]")
    # A colorbar is a whole new Axes, added after _theme_figure ran above --
    # re-running it (idempotent) is the simplest way to theme it too, same
    # fix as figure_optimization_history/figure_mission_route_2d/etc.
    _theme_figure(fig, pal)
    return fig


def figure_wireframe_wing(
    plane, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Isolated 3D wireframe representation of the Main Wing."""
    import matplotlib as mpl

    fig = _ensure_fig(fig, (10, 6))
    ax = fig.add_subplot(111, projection="3d")
    pal = get_palette(theme)
    _theme_figure(fig, pal)
    main_wing = next((w for w in plane.wings if w.name == "Main Wing"), plane.wings[0])
    # draw_wireframe() unconditionally imports AeroSandbox's pretty_plots
    # even when an existing `ax` is passed in -- same global rcParams-
    # polluting seaborn.set_theme() side effect as draw_three_view (see
    # figure_asb_threeview), just triggered from a different entry point.
    # Left unguarded, whichever wireframe tab a user opens first would
    # silently reset chart styling (and, e.g., the x-axis grid visibility)
    # for every 2-D chart drawn afterward anywhere in the app.
    with mpl.rc_context():
        main_wing.draw_wireframe(ax=ax, show=False, color=pal.title)
    ax.set_axis_off()
    ax.set_title("Wing Wireframe")
    return fig


def figure_wireframe_fuselage(
    plane, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Isolated 3D wireframe representation of the Fuselage."""
    import matplotlib as mpl

    fig = _ensure_fig(fig, (10, 6))
    ax = fig.add_subplot(111, projection="3d")
    pal = get_palette(theme)
    _theme_figure(fig, pal)
    fus = next((f for f in plane.fuselages if f.name == "Fuselage"), plane.fuselages[0])
    with mpl.rc_context():  # see figure_wireframe_wing
        fus.draw_wireframe(ax=ax, show=False, color=pal.title)
    ax.set_axis_off()
    ax.set_title("Fuselage Wireframe")
    return fig


def figure_wireframe_empennage(
    plane, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Isolated 3D wireframe representation of the Empennage."""
    import matplotlib as mpl

    fig = _ensure_fig(fig, (10, 6))
    ax = fig.add_subplot(111, projection="3d")
    pal = get_palette(theme)
    _theme_figure(fig, pal)
    hstab = next((w for w in plane.wings if w.name == "Horizontal Stabilizer"), None)
    vstab = next((w for w in plane.wings if w.name == "Vertical Stabilizer"), None)

    with mpl.rc_context():  # see figure_wireframe_wing
        if hstab is not None:
            hstab.draw_wireframe(ax=ax, show=False, color=pal.title)
        elif len(plane.wings) > 1:
            plane.wings[1].draw_wireframe(ax=ax, show=False, color=pal.title)

        if vstab is not None:
            vstab.draw_wireframe(ax=ax, show=False, color=pal.title)
        elif len(plane.wings) > 2:
            plane.wings[2].draw_wireframe(ax=ax, show=False, color=pal.title)

    ax.set_axis_off()
    ax.set_title("Empennage Wireframe (H-Stab & V-Stab)")
    return fig


def figure_span_loading(
    report: AnalysisReport,
    config,
    fig: Optional[Figure] = None,
    theme: str | None = None,
) -> Figure:
    """Plot the spanwise lift distribution of the main wing and compare with ideal elliptical distribution."""
    import aerosandbox as asb

    plane = report.airplane
    req = config.requirements
    atmo = asb.Atmosphere(altitude=req.cruise_altitude_m)
    v = req.cruise_mach * atmo.speed_of_sound()
    alpha = report.design_point.alpha_deg

    op_point = asb.OperatingPoint(atmosphere=atmo, velocity=v, alpha=alpha)

    vlm = asb.VortexLatticeMethod(
        airplane=plane,
        op_point=op_point,
        spanwise_resolution=config.analysis.spanwise_resolution,
        chordwise_resolution=config.analysis.chordwise_resolution,
        verbose=False,
    )
    vlm.run()

    # Find the main wing panels
    wing = vlm.airplane.wings[0]
    w_sub = wing
    if vlm.spanwise_resolution > 1:
        w_sub = wing.subdivide_sections(
            ratio=vlm.spanwise_resolution,
            spacing_function=vlm.spanwise_spacing_function,
        )
    points, faces = w_sub.mesh_thin_surface(
        method="quad",
        chordwise_resolution=vlm.chordwise_resolution,
        chordwise_spacing_function=vlm.chordwise_spacing_function,
        add_camber=True,
    )
    n_wing_panels = len(faces)
    c_res = vlm.chordwise_resolution
    n_cols = n_wing_panels // c_res

    y_centers = vlm.vortex_centers[:n_wing_panels, 1]
    forces_wind = vlm.op_point.convert_axes(
        vlm.forces_geometry[:n_wing_panels, 0],
        vlm.forces_geometry[:n_wing_panels, 1],
        vlm.forces_geometry[:n_wing_panels, 2],
        from_axes="geometry",
        to_axes="wind",
    )
    lift_forces = -forces_wind[2]

    y_left = vlm.front_left_vertices[:n_wing_panels, 1]
    y_right = vlm.front_right_vertices[:n_wing_panels, 1]
    panel_dy = np.abs(y_right - y_left)

    col_y = []
    col_lift_per_span = []

    for j in range(n_cols):
        start_idx = j * c_res
        end_idx = start_idx + c_res
        avg_y = np.mean(y_centers[start_idx:end_idx])
        avg_dy = np.mean(panel_dy[start_idx:end_idx])
        total_lift = np.sum(lift_forces[start_idx:end_idx])
        lift_per_span = total_lift / avg_dy
        col_y.append(avg_y)
        col_lift_per_span.append(lift_per_span)

    col_y = np.array(col_y)
    col_lift_per_span = np.array(col_lift_per_span)

    # Filter for the right wing (positive Y)
    pos_mask = col_y >= 0
    col_y_pos = col_y[pos_mask]
    col_lift_pos = col_lift_per_span[pos_mask]

    # Sort by Y
    sort_idx = np.argsort(col_y_pos)
    col_y_pos = col_y_pos[sort_idx]
    col_lift_pos = col_lift_pos[sort_idx]

    # Elliptical distribution matching the total lift of this wing
    total_wing_lift = np.sum(lift_forces)
    semi_span = wing.span() / 2
    l_root = 4.0 * (total_wing_lift / 2.0) / (np.pi * semi_span)
    elliptical_lift = l_root * np.sqrt(1.0 - (col_y_pos / semi_span) ** 2)

    fig = _ensure_fig(fig, (10, 6))
    ax = fig.add_subplot(111)
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    ax.plot(col_y_pos, col_lift_pos, "b-", label="Calculated Span Loading", linewidth=2)
    ax.plot(
        col_y_pos, elliptical_lift, "r--", label="Ideal Elliptical Loading", alpha=0.8
    )

    ax.set_xlabel("Spanwise position Y [m]")
    ax.set_ylabel("Lift per unit span L' [N/m]")
    ax.set_title("Span Loading (Lift Distribution) comparison")
    ax.grid(True, alpha=0.3)
    ax.legend()

    return fig


def _sparse_streamline_seeds(vlm, n_target: int) -> np.ndarray:
    """One streamline seed per trailing-edge panel, thinned to at most ``n_target``.

    AeroSandbox's own auto-seeding (used when ``seed_points`` is omitted)
    aims for ~200 streamlines but can overshoot far past that on geometries
    with many trailing-edge panels (multiple wings/xsecs at high spanwise
    resolution), since its per-panel count only ever rounds *up* to a
    minimum of 1. Each streamline step costs an induced-velocity evaluation
    against every vortex filament, so streamline count is the dominant
    driver of how long the wake render takes -- capping it here (rather than
    leaving it to scale with whatever geometry gets built) keeps the
    Aerodynamics tab responsive.
    """
    left_te = vlm.back_left_vertices[vlm.is_trailing_edge.astype(bool)]
    right_te = vlm.back_right_vertices[vlm.is_trailing_edge.astype(bool)]
    seeds = 0.5 * (left_te + right_te)
    if len(seeds) > n_target:
        idx = np.linspace(0, len(seeds) - 1, n_target).round().astype(int)
        seeds = seeds[idx]
    return seeds


def figure_vlm_flow(
    report: AnalysisReport,
    config,
    fig: Optional[Figure] = None,
    theme: str | None = None,
) -> Figure:
    """VLM wake visualization -- streamlines coloured by spanwise vortex roll-up.

    Each streamline is coloured by its spanwise Y-origin so the wing-tip
    vortex roll-up is immediately visible in the 3D view. Streamline count is
    deliberately kept low (see :func:`_sparse_streamline_seeds`) and traced
    further per line -- fewer, longer streamlines make the wake's roll-up and
    turbulence easier to read and are far cheaper to compute than a dense mat
    of short ones.
    """
    import aerosandbox as asb
    import matplotlib as mpl
    import matplotlib.cm as cm
    import matplotlib.colors as mcolors

    plane = report.airplane
    req = config.requirements
    atmo = asb.Atmosphere(altitude=req.cruise_altitude_m)
    v = req.cruise_mach * atmo.speed_of_sound()
    alpha = report.design_point.alpha_deg

    op_point = asb.OperatingPoint(atmosphere=atmo, velocity=v, alpha=alpha)

    # Moderately elevated resolution -> resolves individual trailing vortices
    # without the panel count (and its cubic effect on the VLM solve time)
    # exploding; streamline sparsity is controlled separately below.
    vlm = asb.VortexLatticeMethod(
        airplane=plane,
        op_point=op_point,
        spanwise_resolution=6,
        chordwise_resolution=3,
        verbose=False,
    )
    vlm.run()

    # Few, long streamlines: sparse seeding keeps per-frame tracing cost down
    # while a longer length/step count still resolves the tip-vortex roll-up
    # and wake turbulence over a good distance behind the aircraft.
    seed_points = _sparse_streamline_seeds(vlm, n_target=40)
    streamlines = vlm.calculate_streamlines(
        seed_points=seed_points,
        n_steps=220,
        length=plane.wings[0].span() * 4.5,
    )

    fig = _ensure_fig(fig, (14, 10))
    ax = fig.add_subplot(111, projection="3d")
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    # Aircraft wireframe -- muted so streamlines stand out.
    # draw_wireframe() unconditionally imports AeroSandbox's pretty_plots,
    # which runs seaborn.set_theme() as a global rcParams-polluting,
    # process-wide side effect the first time it's imported (see
    # figure_asb_threeview / figure_wireframe_wing for the same issue) --
    # isolate it here too so this call doesn't silently reset chart styling
    # for every 2-D chart drawn afterward.
    with mpl.rc_context():
        for wing in plane.wings:
            wing.draw_wireframe(
                ax=ax,
                show=False,
                color="#555555",
                thin_linewidth=0.6,
                thick_linewidth=0.6,
            )
        for i, fus in enumerate(plane.fuselages):
            col = "#444444" if i == 0 else "#3a3a3a"
            fus.draw_wireframe(
                ax=ax, show=False, color=col, thin_linewidth=0.5, thick_linewidth=0.5
            )

    # Streamlines coloured by absolute spanwise origin |Y| -- highlights tip vortex
    n_lines = streamlines.shape[0]
    y_origins = np.abs(streamlines[:, 1, 0])  # |Y| at t=0
    y_max = y_origins.max() if y_origins.max() > 0 else 1.0
    cmap = mpl.colormaps["plasma"]

    for i in range(n_lines):
        colour = cmap(y_origins[i] / y_max)
        alpha = 0.55 + 0.35 * (y_origins[i] / y_max)  # tip vortices more opaque
        ax.plot(
            streamlines[i, 0, :],
            streamlines[i, 1, :],
            streamlines[i, 2, :],
            color=colour,
            alpha=float(alpha),
            linewidth=1.2,
        )

    ax.set_axis_off()
    ax.set_title(
        f"VLM Wake — α = {alpha:.1f}°   CL = {report.design_point.cl:.3f}"
        f"   L/D = {report.design_point.l_over_d:.1f}",
        color=pal.title,
        fontsize=11,
        fontweight="bold",
        pad=8,
    )
    ax.view_init(elev=20, azim=-120)

    # Equal aspect
    X, Y, Z = streamlines[:, 0, :], streamlines[:, 1, :], streamlines[:, 2, :]
    pts = np.array([X.min(), X.max(), Y.min(), Y.max(), Z.min(), Z.max()])
    max_r = (pts[1::2] - pts[0::2]).max() / 2.0
    mid_x = (X.max() + X.min()) / 2.0
    mid_y = (Y.max() + Y.min()) / 2.0
    mid_z = (Z.max() + Z.min()) / 2.0
    ax.set_xlim(mid_x - max_r, mid_x + max_r)
    ax.set_ylim(mid_y - max_r, mid_y + max_r)
    ax.set_zlim(mid_z - max_r, mid_z + max_r)

    # Colourbar legend
    sm = cm.ScalarMappable(cmap=cmap, norm=mcolors.Normalize(vmin=0, vmax=y_max))
    sm.set_array([])
    cbar = fig.colorbar(sm, ax=ax, shrink=0.45, pad=0.02, location="right")
    cbar.set_label("|Y| spanwise origin [m]", color=pal.tick, fontsize=9)
    cbar.ax.yaxis.set_tick_params(color=pal.tick)
    for lbl in cbar.ax.yaxis.get_ticklabels():
        lbl.set_color(pal.tick)
        lbl.set_fontsize(8)

    return fig


def figure_dynamic_modes(
    report: AnalysisReport,
    config,
    fig: Optional[Figure] = None,
    theme: str | None = None,
) -> Figure:
    """Longitudinal/lateral-directional dynamic-mode analysis at trimmed cruise.

    Left panel: an s-plane pole plot -- real part (damping) on x, imaginary
    part (frequency) on y, shaded left-half-plane = stable. This is the
    standard, immediately-legible way to see both stability *and*
    oscillation character for every mode at once, rather than reading five
    separate period/damping numbers. Axes use a symmetric-log scale, since
    the phugoid/spiral modes are typically 1-2 orders of magnitude slower
    than short-period/dutch-roll -- a linear scale would collapse the slow
    modes onto the origin.
    Right panel: the same data as a compact numeric readout.
    """
    from ..physics.dynamics import estimate_inertia, compute_dynamic_modes

    plane = report.airplane
    req = config.requirements
    atmo = asb.Atmosphere(altitude=req.cruise_altitude_m)
    v = req.cruise_mach * atmo.speed_of_sound()
    dp = report.trimmed_design_point or report.design_point
    alpha = dp.alpha_deg

    op_point = asb.OperatingPoint(atmosphere=atmo, velocity=v, alpha=alpha)

    mass_kg = sum(max(0.0, m) for m in report.component_masses.values())
    x_cg = report.physical_cg[0] if report.physical_cg else float(plane.xyz_ref[0])
    ixx, iyy, izz = estimate_inertia(plane, mass_kg)
    mass_props = asb.MassProperties(mass=mass_kg, x_cg=x_cg, Ixx=ixx, Iyy=iyy, Izz=izz)

    modes = compute_dynamic_modes(plane, op_point, mass_props)

    fig = _ensure_fig(fig, (11.5, 6))
    # Disable the auto layout engine so the legend's reserved bottom margin
    # (below the axes, not overlapping any pole) actually sticks once this
    # figure is embedded at a GUI-constrained canvas size (same fix as
    # figure_mission_profile).
    fig.set_layout_engine(None)
    fig.subplots_adjust(top=0.86, bottom=0.20, left=0.08, right=0.96, wspace=0.3)
    ax_pole, ax_text = fig.subplots(1, 2, gridspec_kw={"width_ratios": [1.3, 1]})
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    display_names = {
        "phugoid": "Phugoid",
        "short_period": "Short period",
        "dutch_roll": "Dutch roll",
        "roll_subsidence": "Roll subsidence",
        "spiral": "Spiral",
    }
    colors = {
        "phugoid": "tab:blue",
        "short_period": "tab:red",
        "dutch_roll": "tab:green",
        "roll_subsidence": "tab:orange",
        "spiral": "tab:purple",
    }

    all_vals = [
        v for m in modes.values() for v in (m.eigenvalue_real, m.eigenvalue_imag)
    ]
    nonzero = [abs(x) for x in all_vals if abs(x) > 1e-9]
    span = max(all_vals and max(abs(x) for x in all_vals), 0.05) * 1.8
    # Floored at a fraction of the span (not just the smallest eigenvalue):
    # a linthresh set purely from the smallest nonzero magnitude (e.g. a
    # lightly-damped phugoid ~1e-3) leaves too narrow a linear region, and
    # symlog's automatic tick locator then crowds/overlaps its innermost
    # major labels right at the axis crossing.
    linthresh = max((min(nonzero) * 0.5) if nonzero else 1e-3, span * 0.01)

    ax_pole.axvspan(-span, 0, color="#66cc66", alpha=0.15, zorder=0)
    ax_pole.axvspan(0, span, color="#ff4d4d", alpha=0.15, zorder=0)
    ax_pole.axvline(0, color="black", linewidth=1, zorder=1)
    ax_pole.axhline(0, color="gray", linewidth=0.7, alpha=0.5, zorder=1)

    for key, m in modes.items():
        c = colors.get(key, "gray")
        ax_pole.scatter(
            [m.eigenvalue_real],
            [m.eigenvalue_imag],
            color=c,
            s=80,
            zorder=5,
            label=display_names.get(key, key),
            edgecolor="black",
            linewidth=0.5,
        )
        if abs(m.eigenvalue_imag) > 1e-9:
            ax_pole.scatter(
                [m.eigenvalue_real],
                [-m.eigenvalue_imag],
                color=c,
                s=80,
                zorder=5,
                edgecolor="black",
                linewidth=0.5,
            )

    ax_pole.set_xscale("symlog", linthresh=linthresh)
    ax_pole.set_yscale("symlog", linthresh=linthresh)
    # Symlog's automatic minor-tick labels can crowd/overlap right at the
    # linear-to-log transition near zero when linthresh is small (common
    # here, since a near-neutral spiral/phugoid mode is the norm, not an
    # edge case) -- keep the minor tick marks for visual scale but drop
    # their text labels, leaving only the clean major (power-of-ten) labels.
    from matplotlib.ticker import NullFormatter

    ax_pole.xaxis.set_minor_formatter(NullFormatter())
    ax_pole.yaxis.set_minor_formatter(NullFormatter())
    ax_pole.set_xlim(-span, span)
    ax_pole.set_ylim(-span, span)
    ax_pole.set_xlabel("Real part (1/s) — damping")
    ax_pole.set_ylabel("Imaginary part (1/s) — frequency")
    ax_pole.set_title("Dynamic modes — s-plane", fontweight="bold")
    # Placed below the axes (outside the data area) rather than in a corner
    # of the plot itself: an in-plot legend (e.g. "upper right") can and does
    # sit on top of a real pole -- the whole point of this chart is to show
    # whether a mode (spiral instability especially) has drifted into the
    # unstable right-half-plane, which is exactly the marker an opaque
    # legend box must never be allowed to hide.
    ax_pole.legend(
        loc="upper center",
        bbox_to_anchor=(0.5, -0.14),
        ncol=3,
        fontsize=8,
        framealpha=0.9,
    )
    ax_pole.grid(True, alpha=0.3)

    ax_text.axis("off")
    # Column widths trimmed to the longest actual value in each ("Roll
    # subsidence" is the longest mode name at 15 chars, so Mode gets 16 for
    # a 1-char gap) and the font shrunk a point below the pole-plot labels':
    # this panel is the narrower half of a 1.3:1 gridspec split, where a
    # wider table would get clipped against the figure edge on any canvas
    # not at least ~11in wide.
    header = f"{'Mode':<16}{'Period':>8}{'Damping':>9}{'Status':>10}"
    lines = [header, "-" * len(header)]
    for key, m in modes.items():
        period_str = f"{m.period_s:.1f} s" if m.period_s > 0 else "n/a"
        status = "stable" if m.stable else "UNSTABLE"
        lines.append(
            f"{display_names.get(key, key):<16}{period_str:>8}{m.damping_ratio:>9.3f}{status:>10}"
        )
    ax_text.text(
        0.0,
        0.85,
        "\n".join(lines),
        transform=ax_text.transAxes,
        va="top",
        fontsize=8.5,
        fontfamily="monospace",
        color=pal.title,
    )

    fig.suptitle(
        f"Dynamic Stability — trimmed cruise (α = {alpha:.1f}°)",
        fontweight="bold",
        color=pal.title,
    )
    return fig


def _le_chord_at_span(xsecs, span_val: float, span_idx: int):
    """Interpolate (leading-edge x, chord) at ``span_val`` along the xsecs'
    spanwise axis (``span_idx``: 1 for a wing/h-stab's Y, 2 for a v-stab's Z)."""
    vals = [xs.xyz_le[span_idx] for xs in xsecs]
    for i in range(len(xsecs) - 1):
        v0, v1 = vals[i], vals[i + 1]
        lo, hi = min(v0, v1), max(v0, v1)
        if lo - 1e-9 <= span_val <= hi + 1e-9:
            f = (span_val - v0) / (v1 - v0) if v1 != v0 else 0.0
            x_le = xsecs[i].xyz_le[0] + f * (
                xsecs[i + 1].xyz_le[0] - xsecs[i].xyz_le[0]
            )
            chord = xsecs[i].chord + f * (xsecs[i + 1].chord - xsecs[i].chord)
            return x_le, chord
    edge = xsecs[-1] if abs(span_val - vals[-1]) < abs(span_val - vals[0]) else xsecs[0]
    return edge.xyz_le[0], edge.chord


def _span_stations(xsecs, span_idx: int, s0: float, s1: float) -> List[float]:
    """Span values to sample between ``s0`` and ``s1``, including any xsec
    break station strictly in between so a surface that straddles a wing
    kink (e.g. a flap spanning the yehudi break) is sampled on both sides of
    the bend, not just at its own two span endpoints."""
    lo, hi = min(s0, s1), max(s0, s1)
    inner = sorted(
        float(xs.xyz_le[span_idx])
        for xs in xsecs
        if lo + 1e-9 < float(xs.xyz_le[span_idx]) < hi - 1e-9
    )
    stations = [lo] + inner + [hi]
    return stations if s0 <= s1 else list(reversed(stations))


def _cs_surface_patch(
    xsecs, span_idx: int, s0: float, s1: float, frac_lo: float, frac_hi: float
):
    """Polygon (list of [span, chordwise_x] points) between chordwise
    fractions ``[frac_lo, frac_hi]`` (0=LE, 1=TE) and span positions
    ``s0..s1``. The caller decides plot-axis order (span-then-x for a top
    view, x-then-span for a side view).

    Samples every xsec break station between ``s0``/``s1`` (see
    :func:`_span_stations`), not just the two endpoints -- a straight line
    between endpoints alone would cut across a wing kink instead of
    following it, visibly drifting off the real planform outline for any
    control surface that spans a break station (e.g. a flap crossing the
    yehudi break).
    """
    stations = _span_stations(xsecs, span_idx, s0, s1)
    le_chord = [_le_chord_at_span(xsecs, s, span_idx) for s in stations]
    front = [[s, x + c * frac_lo] for s, (x, c) in zip(stations, le_chord)]
    back = [[s, x + c * frac_hi] for s, (x, c) in zip(stations, le_chord)]
    return front + list(reversed(back))


def _cs_surface_area(
    xsecs,
    span_idx: int,
    s0: float,
    s1: float,
    frac_lo: float,
    frac_hi: float,
    mirror: bool,
) -> float:
    x0, c0 = _le_chord_at_span(xsecs, s0, span_idx)
    x1, c1 = _le_chord_at_span(xsecs, s1, span_idx)
    chord_frac = frac_hi - frac_lo
    area = 0.5 * (c0 * chord_frac + c1 * chord_frac) * abs(s1 - s0)
    return area * (2.0 if mirror else 1.0)


def figure_control_surfaces(
    report: AnalysisReport,
    config,
    fig: Optional[Figure] = None,
    theme: str | None = None,
) -> Figure:
    """Control-surface layout + tail-volume sizing check.

    Redesigned from a common arrow-callout convention (each label connected
    to its surface by a hand-placed leader line, with the offset tuned by
    eye for one aircraft) to plain shaded regions plus a single legend
    outside the geometry -- callouts collide the moment their tuned offsets
    are reused on a differently-sized or -shaped aircraft; a legend has
    nothing to collide with.

    Chord-fraction/span-fraction bounds come from ``config.control_surfaces``
    -- a pure *representation* input (ALAS models the wing/tail as
    plain lifting surfaces without deflectable sub-geometry, so these
    fractions don't feed back into the aero/mass models, only this
    diagram). Vh/Vv reuse ``physics.stability.tail_volume_coefficients`` --
    the exact number the optimizer's own penalty term already checks --
    against the same configured ``optimizer.weights`` bounds, so this
    directly shows whether the tail is sized the way the optimizer thinks
    it is, not a second, independently-computed estimate.
    """
    import matplotlib.patches as patches
    from ..physics.stability import tail_volume_coefficients

    plane = report.airplane
    cs = config.control_surfaces
    w_opt = config.optimizer.weights

    wing = plane.wings[0]
    hstab = plane.wings[1] if len(plane.wings) > 1 else None
    vstab = plane.wings[2] if len(plane.wings) > 2 else None

    fig = _ensure_fig(fig, (13.5, 7.5))
    # MplCanvas figures are created with tight_layout=True, which recomputes
    # spacing on every draw and doesn't know about the suptitle + Vh/Vv info
    # box added below -- at the short embedded GUI canvas heights this figure
    # gets squeezed into, that leaves them fighting for the same top strip
    # (the title ends up drawn on top of the info box). Disable the auto
    # layout engine and reserve a fixed top margin instead, same fix as
    # figure_mission_profile.
    fig.set_layout_engine(None)
    fig.subplots_adjust(top=0.80, bottom=0.14, left=0.06, right=0.97, wspace=0.25)
    if vstab is not None:
        ax_top, ax_side = fig.subplots(1, 2, gridspec_kw={"width_ratios": [2.4, 1]})
    else:
        ax_top, ax_side = fig.add_subplot(111), None

    pal = get_palette(theme)
    _theme_figure(fig, pal)

    _draw_planform(ax_top, plane, color="#b0bec5", fill=True, alpha=0.5)
    ax_top.set_aspect("equal")
    ax_top.invert_yaxis()
    ax_top.set_xlabel("Span Y (m)")
    ax_top.set_ylabel("Longitudinal X (m)")
    ax_top.set_title("Top view", fontweight="bold", loc="left")
    ax_top.grid(True, alpha=0.2)

    legend_handles: Dict[str, "patches.Patch"] = {}
    rows = []  # (name, chord %, span range m, area m^2)

    def add_top_patch(xsecs, span_lo, span_hi, frac_lo, frac_hi, color, name, mirror):
        poly = _cs_surface_patch(xsecs, 1, span_lo, span_hi, frac_lo, frac_hi)
        for sign in [1, -1] if mirror else [1]:
            pts = [[p[0] * sign, p[1]] for p in poly]
            ax_top.add_patch(
                patches.Polygon(
                    pts,
                    closed=True,
                    facecolor=color,
                    edgecolor="black",
                    linewidth=0.6,
                    alpha=0.85,
                    zorder=4,
                )
            )
        legend_handles.setdefault(
            name,
            patches.Patch(
                facecolor=color, edgecolor="black", linewidth=0.6, label=name
            ),
        )
        area = _cs_surface_area(xsecs, 1, span_lo, span_hi, frac_lo, frac_hi, mirror)
        rows.append((name, (frac_hi - frac_lo) * 100.0, span_lo, span_hi, area))

    wing_semi = wing.span() / 2.0
    add_top_patch(
        wing.xsecs,
        cs.slat_span_start_frac * wing_semi,
        cs.slat_span_end_frac * wing_semi,
        0.0,
        cs.slat_chord_fraction,
        "#e74c3c",
        "Slat",
        wing.symmetric,
    )
    add_top_patch(
        wing.xsecs,
        cs.flap_span_start_frac * wing_semi,
        cs.flap_span_end_frac * wing_semi,
        1.0 - cs.flap_chord_fraction,
        1.0,
        "#27ae60",
        "Flap",
        wing.symmetric,
    )
    add_top_patch(
        wing.xsecs,
        cs.aileron_span_start_frac * wing_semi,
        cs.aileron_span_end_frac * wing_semi,
        1.0 - cs.aileron_chord_fraction,
        1.0,
        "#2980b9",
        "Aileron",
        wing.symmetric,
    )
    spoiler_hi = (
        1.0 - cs.flap_chord_fraction
    )  # immediately ahead of the flap hinge line
    add_top_patch(
        wing.xsecs,
        cs.spoiler_span_start_frac * wing_semi,
        cs.spoiler_span_end_frac * wing_semi,
        spoiler_hi - cs.spoiler_chord_fraction,
        spoiler_hi,
        "#9b59b6",
        "Spoiler",
        wing.symmetric,
    )

    if hstab is not None:
        hstab_semi = hstab.span() / 2.0
        add_top_patch(
            hstab.xsecs,
            cs.elevator_span_start_frac * hstab_semi,
            cs.elevator_span_end_frac * hstab_semi,
            1.0 - cs.elevator_chord_fraction,
            1.0,
            "#f39c12",
            "Elevator",
            hstab.symmetric,
        )

    if vstab is not None and ax_side is not None:
        z_vals = [xs.xyz_le[2] for xs in vstab.xsecs]
        le = [xs.xyz_le[0] for xs in vstab.xsecs]
        te = [xs.xyz_le[0] + xs.chord for xs in vstab.xsecs]
        ax_side.fill(
            le + list(reversed(te)),
            z_vals + list(reversed(z_vals)),
            color="#b0bec5",
            alpha=0.5,
            edgecolor="black",
            linewidth=0.8,
            zorder=1,
        )

        z_lo, z_hi = min(z_vals), max(z_vals)
        rz0 = z_lo + cs.rudder_span_start_frac * (z_hi - z_lo)
        rz1 = z_lo + cs.rudder_span_end_frac * (z_hi - z_lo)
        poly = _cs_surface_patch(
            vstab.xsecs, 2, rz0, rz1, 1.0 - cs.rudder_chord_fraction, 1.0
        )
        pts = [[p[1], p[0]] for p in poly]  # swap to (x, z) for the side view
        ax_side.add_patch(
            patches.Polygon(
                pts,
                closed=True,
                facecolor="#e67e22",
                edgecolor="black",
                linewidth=0.6,
                alpha=0.85,
                zorder=4,
            )
        )
        legend_handles.setdefault(
            "Rudder",
            patches.Patch(
                facecolor="#e67e22", edgecolor="black", linewidth=0.6, label="Rudder"
            ),
        )
        area = _cs_surface_area(
            vstab.xsecs,
            2,
            rz0,
            rz1,
            1.0 - cs.rudder_chord_fraction,
            1.0,
            vstab.symmetric,
        )
        rows.append(("Rudder", cs.rudder_chord_fraction * 100.0, rz0, rz1, area))

        ax_side.set_aspect("equal")
        ax_side.set_xlabel("Longitudinal X (m)")
        ax_side.set_ylabel("Height Z (m)")
        ax_side.set_title("V-stab side view", fontweight="bold", loc="left")
        ax_side.grid(True, alpha=0.2)

    fig.legend(
        handles=list(legend_handles.values()),
        loc="lower center",
        ncol=len(legend_handles),
        fontsize=9,
        framealpha=0.9,
        bbox_to_anchor=(0.5, -0.02),
    )

    # -- Tail-volume sizing check: the same Vh/Vv the optimizer enforces --
    vh, vv = tail_volume_coefficients(plane)

    def _fmt(name, val, lo, hi):
        if val is None:
            return f"{name}: n/a"
        ok = lo <= val <= hi
        mark = "OK" if ok else "OUT OF RANGE"
        return f"{name} = {val:.3f}  (target {lo:.2f}-{hi:.2f})  [{mark}]"

    info = "\n".join(
        [
            _fmt("Vh", vh, w_opt.min_hstab_volume_coef, w_opt.max_hstab_volume_coef),
            _fmt("Vv", vv, w_opt.min_vstab_volume_coef, w_opt.max_vstab_volume_coef),
        ]
    )
    # The reserved top margin (top=0.80 above) gives the title and this info
    # box a shared band to live in, but that alone doesn't stop them from
    # colliding *with each other* inside it: a left-anchored box at y=0.99
    # sits at essentially the same height as the (centered) suptitle, so on
    # any canvas narrow enough for the title's span to reach the left third
    # of the figure, the two draw on top of one another. Stack the box
    # clearly below the title instead of sharing its row.
    # Explicit black text: this box's face is a fixed white regardless of
    # theme, so it must not inherit the GUI theme's process-wide
    # ``rcParams["text.color"]`` default (white on dark/grey themes) --
    # doing so renders invisible white-on-white text.
    fig.text(
        0.01,
        0.93,
        info,
        transform=fig.transFigure,
        va="top",
        ha="left",
        fontsize=9,
        fontfamily="monospace",
        color="black",
        bbox=dict(boxstyle="round", fc="white", ec="#999999", alpha=0.9),
    )

    fig.suptitle(
        "Control Surfaces & Tail Sizing", fontweight="bold", y=0.99, color=pal.title
    )
    return fig


def figure_airfoil_reynolds(
    airfoil: asb.Airfoil, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Plot 5-panel airfoil aerodynamic contours (CL, CD, L/D, CM) vs Re and alpha using NeuralFoil."""
    import neuralfoil
    import matplotlib.ticker as mticker

    # Setup grid (30x30 is fast for responsive GUI loads)
    alphas = np.linspace(-5, 12, 30)
    res_list = np.logspace(4, 7, 30)

    # Meshgrid for contour plotting
    RE, ALPHA = np.meshgrid(res_list, alphas)
    CL = np.zeros_like(RE)
    CD = np.zeros_like(RE)
    CM = np.zeros_like(RE)

    # Run NeuralFoil on the grid
    for i in range(len(alphas)):
        for j in range(len(res_list)):
            try:
                res = neuralfoil.get_aero_from_coordinates(
                    coordinates=airfoil.coordinates, alpha=alphas[i], Re=res_list[j]
                )
                CL[i, j] = float(res["CL"][0])
                CD[i, j] = float(res["CD"][0])
                CM[i, j] = float(res["CM"][0])
            except Exception:
                CL[i, j] = np.nan
                CD[i, j] = np.nan
                CM[i, j] = np.nan

    LD = np.where(CD > 0, CL / CD, np.nan)

    # Tall aspect ratio suitable for 5 panels
    fig = _ensure_fig(fig, (11, 9))
    fig.clf()  # Rebuild clean layout
    gs = fig.add_gridspec(3, 2, height_ratios=[1, 2, 2])

    # Subplot 0: Airfoil Profile shape
    ax_af = fig.add_subplot(gs[0, :])
    coords = airfoil.coordinates
    ax_af.plot(coords[:, 0], coords[:, 1], "b-", linewidth=2)
    ax_af.fill(coords[:, 0], coords[:, 1], "b", alpha=0.1)
    ax_af.set_title(f"Airfoil Profile: {airfoil.name}")
    ax_af.set_xlabel("x/c")
    ax_af.set_ylabel("y/c")
    ax_af.set_aspect("equal")
    ax_af.grid(True, alpha=0.3)

    # Helper function to plot contour
    def plot_contour(ax, X, Y, Z, title, label, cmap="plasma", log_x=True):
        cnt = ax.contourf(X, Y, Z, levels=20, cmap=cmap)
        # A contourf with 20 fill levels defaults to a colorbar tick at
        # every one of those 20+ level boundaries -- at this panel's short
        # height that's far more labels than can fit without overlapping
        # each other into an unreadable stack. Cap it at a handful of
        # evenly-spaced ticks regardless of how many fill levels there are.
        cb = fig.colorbar(cnt, ax=ax, label=label, ticks=mticker.MaxNLocator(nbins=6))
        cb.ax.tick_params(labelsize=8)
        # Fewer line levels (4, was 6, was 10): the steep-gradient corner of
        # this data (high alpha, low Re, near stall) packs many contour
        # lines close together, so their inline clabel numbers still piled
        # up into an unreadable jumble even at 6 -- thin the cluster out
        # further and space the inline labels out along each line.
        lines = ax.contour(X, Y, Z, levels=4, colors="black", linewidths=0.5, alpha=0.5)
        ax.clabel(lines, inline=True, fontsize=8, fmt="%.2f", inline_spacing=12)
        ax.set_title(title)
        ax.set_ylabel(r"$\alpha$ [deg]")
        ax.set_xlabel("Re")
        if log_x:
            ax.set_xscale("log")
            # A log axis over 3 decades otherwise gets matplotlib's default
            # LogLocator, which also draws unlabelled minor ticks at
            # 2x/3x/.../9x each decade -- fine on their own, but their tick
            # *marks* plus the major decade labels read as a cramped axis at
            # this panel's size. Keep exactly the 4 clean decade labels
            # (1e4..1e7) and drop minor tick marks entirely.
            ax.xaxis.set_major_locator(mticker.LogLocator(base=10.0, numticks=4))
            ax.xaxis.set_minor_locator(mticker.NullLocator())
        ax.grid(True, alpha=0.3)

    # Subplot 1: CL
    ax_cl = fig.add_subplot(gs[1, 0])
    plot_contour(
        ax_cl, RE, ALPHA, CL, r"$C_l$ from $Re, \alpha$", "$C_l$", cmap="plasma"
    )

    # Subplot 2: CD
    ax_cd = fig.add_subplot(gs[1, 1])
    plot_contour(
        ax_cd, RE, ALPHA, CD, r"$C_d$ from $Re, \alpha$", "$C_d$", cmap="viridis"
    )

    # Subplot 3: L/D
    ax_ld = fig.add_subplot(gs[2, 0])
    plot_contour(
        ax_ld, RE, ALPHA, LD, r"$L/D$ from $Re, \alpha$", "$L/D$", cmap="inferno"
    )

    # Subplot 4: CM
    ax_cm = fig.add_subplot(gs[2, 1])
    plot_contour(
        ax_cm, RE, ALPHA, CM, r"$C_m$ from $Re, \alpha$", "$C_m$", cmap="magma"
    )

    pal = get_palette(theme)
    _theme_figure(fig, pal)

    fig.tight_layout()
    return fig


def figure_mass_breakdown(
    report: "AnalysisReport", fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Horizontal stacked weight bar showing the mass buildup from OEW to MTOW.

    Handles all use cases:
    - Normal: OEW + Payload + Fuel = MTOW (fuel bar is positive, green)
    - Overweight: OEW + Payload > MTOW (fuel bar is negative, shown as red warning)
    - Cargo vs passenger: both are valid since component_masses handles it
    """
    fig = _ensure_fig(fig, (11, 6))
    fig.clf()
    pal = get_palette(theme)
    fig.patch.set_facecolor(pal.bg)

    def _apply_theme() -> None:
        _theme_figure(fig, pal)

    masses = report.component_masses
    if not masses:
        ax = fig.add_subplot(111)
        _apply_theme()
        ax.text(
            0.5,
            0.5,
            "No mass data available",
            ha="center",
            va="center",
            color=pal.title,
        )
        return fig

    # --- Define component groups and colors ---
    STRUCTURE_KEYS = ["Wing", "H-Stab", "V-Stab", "Fuselage", "Gear"]
    STRUCTURE_COLORS = ["#2980b9", "#3498db", "#5dade2", "#7f8c8d", "#34495e"]
    OTHER_KEYS = ["Propulsion", "Systems", "Furnishings"]
    OTHER_COLORS = ["#c0392b", "#8e44ad", "#a569bd"]
    PAYLOAD_COLOR = "#27ae60"
    FUEL_COLOR = "#e67e22"
    FUEL_NEG_COLOR = "#e74c3c"

    structure_vals = [masses.get(k, 0.0) for k in STRUCTURE_KEYS]
    other_vals = [masses.get(k, 0.0) for k in OTHER_KEYS]
    payload = masses.get("Payload", 0.0)
    fuel = masses.get("Fuel", 0.0)

    oew = sum(structure_vals) + sum(other_vals)
    mzfw = oew + payload
    actual_mtow = sum(max(0.0, v) for v in masses.values())

    ax = fig.add_subplot(111)
    _apply_theme()
    bar_height = 0.5

    # --- Row 0: Structure breakdown ---
    # Segments here are fractions of the *structure* row only, which itself
    # typically fills well under half of the full MTOW-scaled axis -- so
    # even a "large" component (Wing, Systems) is a fairly narrow sliver of
    # the actual drawn width, and several (Gear especially, the smallest)
    # are too narrow for a centered two-line label without it bleeding into
    # its neighbours' text. Segments below ``inline_frac`` get an external,
    # collision-staggered label above the row instead, matching the
    # leader-line convention used for the thin wave-drag slice in
    # figure_drag_breakdown.
    inline_frac = 0.08
    segments = [
        (v, c, k)
        for v, c, k in zip(structure_vals, STRUCTURE_COLORS, STRUCTURE_KEYS)
        if v > 0
    ]
    segments += [
        (v, c, k) for v, c, k in zip(other_vals, OTHER_COLORS, OTHER_KEYS) if v > 0
    ]

    left = 0.0
    external: list[
        tuple[float, str]
    ] = []  # (center_x_t, label) awaiting staggered placement
    for val, color, key in segments:
        ax.barh(
            2,
            val / 1000,
            left=left / 1000,
            height=bar_height,
            color=color,
            edgecolor="white",
            lw=0.6,
        )
        cx = left / 1000 + val / 2000
        if val / actual_mtow > inline_frac:
            ax.text(
                cx,
                2,
                f"{key}\n{val / 1000:.1f}t",
                ha="center",
                va="center",
                fontsize=7.5,
                fontweight="bold",
                color="white",
            )
        elif val / actual_mtow > 0.015:
            external.append((cx, f"{key} {val / 1000:.1f}t"))
        left += val

    y_top_limit = 2 + bar_height / 2 + 0.2  # default autoscale headroom above row 2
    if external:
        xs = [cx for cx, _ in external]
        rows = _assign_label_rows(xs, min_sep=actual_mtow / 1000 * 0.09)
        y_top = 2 + bar_height / 2
        for (cx, label), row in zip(external, rows):
            y_text = y_top + 0.16 + row * 0.30
            ax.annotate(
                label,
                xy=(cx, y_top),
                xytext=(cx, y_text),
                ha="center",
                va="bottom",
                fontsize=7.5,
                fontweight="bold",
                color=pal.title,
                arrowprops=dict(arrowstyle="-", color="#999999", lw=0.8),
            )
        # Text extents aren't included in autoscale, so the staggered rows
        # of external labels above need their headroom reserved explicitly
        # or the topmost row gets clipped against the axes/figure edge.
        y_top_limit = y_top + 0.16 + max(rows) * 0.30 + 0.20

    # --- Row 1: OEW -> MZFW ---
    ax.barh(
        1,
        oew / 1000,
        left=0,
        height=bar_height,
        color="#2c3e50",
        edgecolor="white",
        lw=0.6,
        label=f"OEW: {oew / 1000:.1f} t",
    )
    ax.barh(
        1,
        payload / 1000,
        left=oew / 1000,
        height=bar_height,
        color=PAYLOAD_COLOR,
        edgecolor="white",
        lw=0.6,
        label=f"Payload: {payload / 1000:.1f} t",
    )
    ax.text(
        oew / 2000,
        1,
        f"OEW\n{oew / 1000:.1f} t",
        ha="center",
        va="center",
        fontsize=8.5,
        fontweight="bold",
        color="white",
    )
    ax.text(
        (oew + payload / 2) / 1000,
        1,
        f"Payload\n{payload / 1000:.1f} t",
        ha="center",
        va="center",
        fontsize=8.5,
        fontweight="bold",
        color="white",
    )

    # --- Row 2: MZFW -> MTOW (with fuel or deficit) ---
    ax.barh(
        0,
        mzfw / 1000,
        left=0,
        height=bar_height,
        color="#2c3e50",
        edgecolor="white",
        lw=0.6,
        label=f"MZFW: {mzfw / 1000:.1f} t",
    )
    if fuel >= 0.0:
        ax.barh(
            0,
            fuel / 1000,
            left=mzfw / 1000,
            height=bar_height,
            color=FUEL_COLOR,
            edgecolor="white",
            lw=0.6,
            label=f"Fuel: {fuel / 1000:.1f} t",
        )
        ax.text(
            (mzfw + fuel / 2) / 1000,
            0,
            f"Fuel\n{fuel / 1000:.1f} t",
            ha="center",
            va="center",
            fontsize=8.5,
            fontweight="bold",
            color="white",
        )
    else:
        # Negative fuel: MTOW budget overrun -- draw a red warning bar
        deficit = -fuel
        ax.barh(
            0,
            deficit / 1000,
            left=mzfw / 1000,
            height=bar_height,
            color=FUEL_NEG_COLOR,
            edgecolor="white",
            lw=0.6,
            label=f"⚠ Fuel deficit: {deficit / 1000:.1f} t",
        )
        ax.text(
            (mzfw + deficit / 2) / 1000,
            0,
            f"⚠ DEFICIT\n{deficit / 1000:.1f} t",
            ha="center",
            va="center",
            fontsize=8.5,
            fontweight="bold",
            color="white",
        )

    ax.text(
        mzfw / 2000,
        0,
        f"MZFW\n{mzfw / 1000:.1f} t",
        ha="center",
        va="center",
        fontsize=8.5,
        fontweight="bold",
        color="white",
    )

    # --- Reference MTOW line ---
    ax.axvline(
        actual_mtow / 1000,
        color="red",
        lw=2,
        linestyle="--",
        label=f"MTOW = {actual_mtow / 1000:.1f} t",
        zorder=10,
    )
    ax.set_ylim(-bar_height / 2 - 0.15, y_top_limit)
    return fig


def figure_cg_envelope(
    report: "AnalysisReport",
    config=None,
    fig: Optional[Figure] = None,
    theme: str | None = None,
) -> Figure:
    """Standard CG envelope (loading chart) diagram from cg_envelope_Example.py.

    Shows how the CG location changes as payload and fuel are loaded, bounded by
    landing gear load limits (NLG strength, MLG strength, steering nose load),
    tip-over, and aerodynamic stability limits.
    """
    fig = _ensure_fig(fig, (10, 8))
    fig.clf()
    # An in-plot "upper left" legend collides with whatever's actually
    # drawn there -- the MTOW threshold label (anchored at the left edge)
    # and the NLG Max Strength curve both live in exactly that corner, and
    # every other corner has its own curve/label (MLG Max Strength and the
    # envelope's own upper-right sweep, TIP-OVER/NP near the right edge, the
    # loading trajectories near the bottom). Rather than chase whichever
    # corner is least bad for a given aircraft's geometry, park the legend
    # below the axes entirely, in its own reserved strip -- same fix
    # pattern as figure_dynamic_modes/figure_mission_profile.
    fig.set_layout_engine(None)
    fig.subplots_adjust(top=0.91, bottom=0.15, left=0.09, right=0.97)
    ax = fig.add_subplot(111)
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    masses = report.component_masses
    coords = report.mass_coordinates
    plane = report.airplane

    if not masses or not coords:
        ax.text(
            0.5,
            0.5,
            "No mass / coordinate data available",
            ha="center",
            va="center",
            color=pal.title,
        )
        return fig

    mac = plane.c_ref
    wing = next((w for w in plane.wings if w.name == "Main Wing"), plane.wings[0])
    x_wing_ac = float(wing.aerodynamic_center()[0])
    x_mac_le = x_wing_ac - 0.25 * mac  # leading edge of the MAC

    def to_pct(x_m: float) -> float:
        return ((x_m - x_mac_le) / max(mac, 0.001)) * 100.0

    # --- Load advanced gear/limit settings from config ---
    mm = getattr(config, "mass_model", None) if config is not None else None
    if mm is None:
        from ..config.mass_config import MassModelConfig

        mm = MassModelConfig()

    nlg_x_frac = mm.nlg_x_fraction
    mlg_x_frac_mac = mm.mlg_x_fraction_mac
    pct_nlg_min = mm.pct_load_nlg_min
    mlw_frac = mm.mlw_fraction_mtow

    # --- Extract fuselage for NLG/MLG wheel positioning ---
    fus = plane.fuselages[0]
    fus_start_x = fus.xsecs[0].xyz_c[0]
    fus_end_x = fus.xsecs[-1].xyz_c[0]
    fus_len = fus_end_x - fus_start_x

    x_nlg = fus_start_x + fus_len * nlg_x_frac
    x_mlg = x_mac_le + mlg_x_frac_mac * mac
    wheelbase = x_mlg - x_nlg

    # --- Build component groups ---
    oew_mass = sum(masses.get(k, 0.0) for k in OEW_KEYS)
    payload = masses.get("Payload", 0.0)
    fuel = masses.get("Fuel", 0.0)
    mtow_mass = oew_mass + payload + max(0, fuel)
    mlw_mass = mtow_mass * mlw_frac
    mzfw_mass = oew_mass + payload

    # OEW CG
    def cg_of_subset(keys):
        m_tot = sum(max(0.0, masses.get(k, 0.0)) for k in keys)
        if m_tot <= 0:
            return to_pct(plane.xyz_ref[0])
        x_mom = sum(
            max(0.0, masses.get(k, 0.0)) * coords.get(k, [plane.xyz_ref[0], 0, 0])[0]
            for k in keys
        )
        return to_pct(x_mom / m_tot)

    oew_cg_mac = cg_of_subset(OEW_KEYS)
    payload_cg_x = coords.get("Payload", [plane.xyz_ref[0], 0, 0])[0]
    fuel_cg_x = coords.get("Fuel", [plane.xyz_ref[0], 0, 0])[0]

    # Reconstruct OEW X position
    oew_cg_x = x_mac_le + oew_cg_mac / 100.0 * mac

    # --- Loading sequence: OEW -> load payload -> load fuel ---
    def composite_cg(m_oew, x_oew, m_payload, x_payload, m_fuel, x_fuel):
        m_tot = max(m_oew + m_payload + m_fuel, 1.0)
        x_cg = (m_oew * x_oew + m_payload * x_payload + m_fuel * x_fuel) / m_tot
        return m_tot, to_pct(x_cg)

    # Sequence A: Load payload progressively (0 -> 100%), then fuel (0 -> 100%)
    pts_weight_A, pts_cg_A = [], []
    for frac in np.linspace(0, 1, 15):
        w, cg = composite_cg(
            oew_mass, oew_cg_x, frac * payload, payload_cg_x, 0.0, fuel_cg_x
        )
        pts_weight_A.append(w)
        pts_cg_A.append(cg)
    for frac in np.linspace(0, 1, 15):
        w, cg = composite_cg(
            oew_mass, oew_cg_x, payload, payload_cg_x, frac * max(0, fuel), fuel_cg_x
        )
        pts_weight_A.append(w)
        pts_cg_A.append(cg)

    # --- Aerodynamic limits ---
    sm_val = (
        float(report.static_margin)
        if (
            hasattr(report, "static_margin")
            and report.static_margin == report.static_margin
        )
        else 0.10
    )
    x_np = plane.xyz_ref[0] + sm_val * mac
    np_pct = to_pct(x_np)

    if config is not None and hasattr(config, "requirements"):
        target_sm = config.requirements.target_static_margin
        cg_range = config.requirements.cg_range_pct_mac
    else:
        target_sm = 0.10
        cg_range = 15.0

    aft_limit_mac = np_pct - target_sm * 100.0
    fwd_limit_mac = aft_limit_mac - cg_range
    tip_over_pct = to_pct(x_mlg)

    # Gear strength limits: the same wheel/tire-derived values the optimizer's
    # CG check enforces (physics.landing_gear), computed from the aerodynamic
    # limits just above -- so this plot shows exactly the boundary a design
    # was actually held to, not a fixed guess. Falls back to the legacy fixed
    # MassModelConfig fractions if sizing fails for any reason.
    try:
        from ..physics.landing_gear import size_landing_gear

        gear_cfg = getattr(config, "landing_gear", None) if config is not None else None
        if gear_cfg is None:
            from ..config.landing_gear_config import LandingGearConfig

            gear_cfg = LandingGearConfig()
        fus_diam = getattr(getattr(config, "geometry", None), "fuselage", None)
        fus_diam = getattr(fus_diam, "diameter_m", None) or 4.0
        aero_fwd_lim_x = x_mac_le + fwd_limit_mac / 100.0 * mac
        aero_aft_lim_x = x_mac_le + aft_limit_mac / 100.0 * mac
        gear = size_landing_gear(
            mtow_mass,
            x_nlg,
            x_mlg,
            aero_fwd_lim_x,
            aero_aft_lim_x,
            fuselage_diameter_m=fus_diam,
            cg_height_estimate_m=fus_diam * 1.1,
            gear_config=gear_cfg,
        )
        pct_nlg_max = gear.pct_load_nlg_max
        pct_mlg_max = gear.pct_load_mlg_max
    except Exception:
        pct_nlg_max = mm.pct_load_nlg_max
        pct_mlg_max = mm.pct_load_mlg_max

    # --- Calculate curves over weight range ---
    w_calc = np.linspace(oew_mass * 0.5, mtow_mass * 1.3, 500)
    load_nlg_max = mtow_mass * pct_nlg_max
    load_mlg_max = mtow_mass * pct_mlg_max
    load_nlg_min = mtow_mass * pct_nlg_min

    c_nlg_str = to_pct(x_mlg - (load_nlg_max * wheelbase / w_calc))
    c_mlg_str = to_pct(x_nlg + (load_mlg_max * wheelbase / w_calc))
    c_nlg_min = to_pct(x_mlg - (load_nlg_min * wheelbase / w_calc))

    # --- Operational envelope bounds ---
    w_ops = np.linspace(oew_mass, mtow_mass, 300)
    op_nlg_str = to_pct(x_mlg - (load_nlg_max * wheelbase / w_ops))
    op_mlg_str = to_pct(x_nlg + (load_mlg_max * wheelbase / w_ops))
    op_nlg_min = to_pct(x_mlg - (load_nlg_min * wheelbase / w_ops))

    poly_fwd = np.maximum(fwd_limit_mac, op_nlg_str)
    poly_aft = np.minimum(aft_limit_mac, np.minimum(op_mlg_str, op_nlg_min))

    # --- Calculate dynamic viewport bounds based on the operational envelope ---
    x_min_poly = np.min(poly_fwd)
    x_max_poly = np.max(poly_aft)
    width = x_max_poly - x_min_poly
    view_min = min(x_min_poly, fwd_limit_mac) - width * 0.4
    view_max = max(x_max_poly, tip_over_pct, np_pct) + width * 0.4

    ax.set_xlim(view_min, view_max)
    ax.set_ylim(oew_mass * 0.7 / 1000.0, mtow_mass * 1.25 / 1000.0)

    # --- Draw background limits ---
    box_style = dict(boxstyle="round,pad=0.3", fc="white", ec="none", alpha=0.85)

    # Weight thresholds (placed cleanly at the left boundary of the plot)
    x_pos_weight_labels = view_min + (view_max - view_min) * 0.02
    for w, lab, col in [
        (mtow_mass, "MTOW", "#c0392b"),
        (mlw_mass, "MLW", "#8e44ad"),
        (mzfw_mass, "MZFW", "#2980b9"),
        (oew_mass, "OEW", "navy"),
    ]:
        ax.axhline(w / 1000.0, color=col, ls=":", lw=1, alpha=0.4)
        ax.text(
            x_pos_weight_labels,
            w / 1000.0,
            lab,
            color=col,
            va="bottom",
            ha="left",
            fontweight="bold",
            fontsize=9,
            bbox=dict(boxstyle="square,pad=0.1", fc="white", ec="none", alpha=0.7),
        )

    # NLG Max Strength curve
    ax.plot(c_nlg_str, w_calc / 1000.0, color="#e74c3c", ls="-.", lw=1.5)
    y_target = mtow_mass * 1.1
    x_target = np.interp(y_target, w_calc, c_nlg_str)
    ax.annotate(
        "NLG Max Strength",
        xy=(x_target, y_target / 1000.0),
        xytext=(-10, 0),
        textcoords="offset points",
        color="#c0392b",
        fontsize=9,
        ha="right",
        va="center",
        bbox=box_style,
        rotation=60,
    )

    # MLG Max Strength curve
    ax.plot(c_mlg_str, w_calc / 1000.0, color="#2980b9", ls="-.", lw=1.5)
    y_target = mtow_mass * 0.95
    x_target = np.interp(y_target, w_calc, c_mlg_str)
    ax.annotate(
        "MLG Max Strength",
        xy=(x_target, y_target / 1000.0),
        xytext=(10, 0),
        textcoords="offset points",
        color="#2980b9",
        fontsize=9,
        ha="left",
        va="center",
        bbox=box_style,
        rotation=-60,
    )

    # Aerodynamic vertical limits (staggered vertical placement of labels) --
    # computed here, ahead of the Min Nose Load label below, so that label
    # can steer clear of the tall rotated TIP-OVER/NP text bands instead of
    # guessing a single fixed offset that only happens to clear them for one
    # aircraft's geometry.
    y_top = mtow_mass * 1.22 / 1000.0
    y_mid = (mtow_mass + oew_mass) / 2.0 / 1000.0
    y_bot = oew_mass * 0.75 / 1000.0

    # Min Nose Load curve
    ax.plot(c_nlg_min, w_calc / 1000.0, color="#d35400", ls="--", lw=1.5)
    y_target = oew_mass * 1.1
    x_target = np.interp(y_target, w_calc, c_nlg_min)
    # This label's x lands wherever the steering-limit curve crosses this
    # weight, which is frequently close to the aft-cg vertical limit lines
    # (Neutral Point / Tip-over) whose rotated labels occupy a tall vertical
    # band right around the OEW height -- collide there often enough (seen
    # both here and reported by real usage) that a fixed offset isn't
    # reliable. Move up to the MTOW-height label band instead when close.
    near_aft_vline = any(
        abs(x_target - vx) < (view_max - view_min) * 0.08
        for vx in (tip_over_pct, np_pct)
    )
    if near_aft_vline:
        nose_load_xytext = (x_target, y_top)
        nose_load_textcoords = "data"
        nose_load_va = "bottom"
    else:
        nose_load_xytext = (15, -10)
        nose_load_textcoords = "offset points"
        nose_load_va = "center"
    ax.annotate(
        "Min Nose Load (Steering)",
        xy=(x_target, y_target / 1000.0),
        xytext=nose_load_xytext,
        textcoords=nose_load_textcoords,
        arrowprops=dict(
            arrowstyle="->", color="#d35400", connectionstyle="arc3,rad=0.2"
        ),
        color="#d35400",
        fontsize=9,
        ha="left",
        va=nose_load_va,
        bbox=box_style,
    )

    # These labels sit directly on the plot background (dark on the
    # grey/dark themes), not a curve like the NLG/MLG/Min-Nose labels above
    # -- give every one of them the same white contrast box those use
    # instead of leaving some boxed and some bare (which also left the bare
    # ones unreadable against a dark theme's page background). The vline
    # strokes use the theme's own title color (``pal.title``) rather than a
    # hardcoded 'k', which would be invisible-ish against a dark/grey
    # theme's near-black page background.
    ax.axvline(fwd_limit_mac, color=pal.title, ls=":", lw=1.5, alpha=0.7)
    ax.text(
        fwd_limit_mac,
        y_top,
        "  Fwd Aero Limit",
        rotation=90,
        va="top",
        ha="left",
        fontsize=9,
        color="black",
        fontweight="bold",
        bbox=box_style,
    )

    ax.axvline(aft_limit_mac, color=pal.title, ls="--", lw=2, alpha=0.7)
    ax.text(
        aft_limit_mac,
        y_mid,
        f"  Stability Limit (NP-{int(target_sm * 100)}%)",
        rotation=90,
        va="center",
        ha="left",
        fontsize=9,
        color="black",
        fontweight="bold",
        bbox=box_style,
    )

    ax.axvline(np_pct, color="gray", ls="-.", lw=1.5, alpha=0.5)
    ax.text(
        np_pct,
        y_top,
        "  Neutral Point (NP)",
        rotation=90,
        va="top",
        ha="left",
        fontsize=9,
        color="gray",
        bbox=box_style,
    )

    ax.axvline(tip_over_pct, color="#c0392b", lw=2, alpha=0.3)
    ax.text(
        tip_over_pct,
        y_bot,
        "  TIP-OVER (MLG)",
        color="#c0392b",
        rotation=90,
        va="bottom",
        ha="left",
        fontsize=9,
        fontweight="bold",
        bbox=box_style,
    )

    # --- Operational envelope filling & outlines ---
    ax.fill_betweenx(
        w_ops / 1000.0, poly_fwd, poly_aft, color="#2ecc71", alpha=0.2, zorder=5
    )
    ax.plot(
        poly_fwd,
        w_ops / 1000.0,
        color="#27ae60",
        lw=3,
        zorder=6,
        label="Operational Limits",
    )
    ax.plot(poly_aft, w_ops / 1000.0, color="#27ae60", lw=3, zorder=6)
    ax.plot(
        [poly_fwd[0], poly_aft[0]],
        [oew_mass / 1000.0, oew_mass / 1000.0],
        color="#27ae60",
        lw=3,
        zorder=6,
    )
    ax.plot(
        [poly_fwd[-1], poly_aft[-1]],
        [mtow_mass / 1000.0, mtow_mass / 1000.0],
        color="#27ae60",
        lw=3,
        zorder=6,
    )

    # --- Plot loading trajectories ---
    # Payload path
    _, cg_mzfw_mac = composite_cg(
        oew_mass, oew_cg_x, payload, payload_cg_x, 0.0, fuel_cg_x
    )
    ax.plot(
        pts_cg_A[:15],
        np.array(pts_weight_A[:15]) / 1000.0,
        "b-o",
        lw=2,
        label="Payload loading",
        zorder=10,
    )

    # Fuel path
    _, cg_mtow_mac = composite_cg(
        oew_mass, oew_cg_x, payload, payload_cg_x, fuel, fuel_cg_x
    )
    ax.plot(
        pts_cg_A[14:],
        np.array(pts_weight_A[14:]) / 1000.0,
        color="orange",
        lw=2,
        marker="o",
        label="Fuel loading",
        zorder=10,
    )

    # Annotate key points
    ax.annotate(
        "OEW",
        (oew_cg_mac, oew_mass / 1000.0),
        xytext=(-20, -10),
        textcoords="offset points",
        fontsize=8,
        fontweight="bold",
        color="navy",
    )
    ax.annotate(
        "MZFW",
        (cg_mzfw_mac, mzfw_mass / 1000.0),
        xytext=(10, 0),
        textcoords="offset points",
        fontsize=8,
        fontweight="bold",
        color="#2980b9",
    )
    ax.annotate(
        "MTOW",
        (cg_mtow_mac, mtow_mass / 1000.0),
        xytext=(10, 10),
        textcoords="offset points",
        fontsize=8,
        fontweight="bold",
        color="#c0392b",
    )

    # Formatting limits, grids and labels
    ax.set_title(
        "Weight & Balance / CG Operational Envelope",
        fontsize=14,
        fontweight="bold",
        pad=15,
    )
    ax.set_xlabel("Aircraft Center of Gravity (% MAC)")
    ax.set_ylabel("Weight (tonnes)")
    ax.grid(True, which="major", alpha=0.3)
    ax.grid(True, which="minor", alpha=0.1, linestyle=":")
    ax.minorticks_on()
    ax.legend(
        loc="upper center",
        bbox_to_anchor=(0.5, -0.11),
        ncol=3,
        framealpha=1,
        shadow=True,
        fontsize=9,
    )

    return fig


def figure_landing_gear_planform(
    report: "AnalysisReport",
    config=None,
    fig: Optional[Figure] = None,
    theme: str | None = None,
) -> Figure:
    """Top-down planform view of the aircraft with the sized landing gear.

    Every wheel is drawn individually (NLG vs. MLG-L/R/Body distinguished by
    colour) at its real longitudinal/lateral position and true tire diameter,
    over the fuselage/wing outline -- the same top-view convention (X-axis =
    spanwise Y, Y-axis = fuselage-station X, nose at the top) as
    :func:`figure_geometry`, so this reads as a natural companion view.
    """
    from ..physics.landing_gear import size_landing_gear
    from ..config.landing_gear_config import LandingGearConfig
    from ..physics.mass import OEW_KEYS
    import matplotlib.patches as mpatches

    fig = _ensure_fig(fig, (9, 11))
    ax = fig.add_subplot(111)
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    plane = report.airplane
    masses = report.component_masses
    mac = plane.c_ref
    wing = next((w for w in plane.wings if w.name == "Main Wing"), plane.wings[0])
    x_wing_ac = float(wing.aerodynamic_center()[0])
    x_mac_le = x_wing_ac - 0.25 * mac

    mm = getattr(config, "mass_model", None) if config is not None else None
    if mm is None:
        from ..config.mass_config import MassModelConfig

        mm = MassModelConfig()
    gear_cfg = getattr(config, "landing_gear", None) if config is not None else None
    if gear_cfg is None:
        gear_cfg = LandingGearConfig()

    fus = plane.fuselages[0]
    fus_start_x = fus.xsecs[0].xyz_c[0]
    fus_end_x = fus.xsecs[-1].xyz_c[0]
    fus_len = fus_end_x - fus_start_x
    x_nlg = fus_start_x + fus_len * mm.nlg_x_fraction
    x_mlg = x_mac_le + mm.mlg_x_fraction_mac * mac
    fus_diam = (
        getattr(getattr(config, "geometry", None), "fuselage", None)
        if config is not None
        else None
    )
    fus_diam = getattr(fus_diam, "diameter_m", None) or (
        max(float(s.width) for s in fus.xsecs) if fus.xsecs else 4.0
    )

    # Aerodynamic (gear-independent) CG limits -- the same worst-case loads
    # the optimizer's CG check sizes the gear against (see
    # optimization.objective._check_cg_envelope).
    sm_val = (
        float(report.static_margin)
        if (
            hasattr(report, "static_margin")
            and report.static_margin == report.static_margin
        )
        else 0.10
    )
    x_np = plane.xyz_ref[0] + sm_val * mac
    if config is not None and hasattr(config, "requirements"):
        target_sm = config.requirements.target_static_margin
        cg_range = config.requirements.cg_range_pct_mac
    else:
        target_sm = 0.10
        cg_range = 15.0
    np_pct = ((x_np - x_mac_le) / max(mac, 0.001)) * 100.0
    aft_limit_mac = np_pct - target_sm * 100.0
    fwd_limit_mac = aft_limit_mac - cg_range
    aero_fwd_lim_x = x_mac_le + fwd_limit_mac / 100.0 * mac
    aero_aft_lim_x = x_mac_le + aft_limit_mac / 100.0 * mac

    oew_mass = sum(masses.get(k, 0.0) for k in OEW_KEYS) if masses else 0.0
    mtow_mass = (
        oew_mass + masses.get("Payload", 0.0) + max(0.0, masses.get("Fuel", 0.0))
        if masses
        else 0.0
    )

    gear = size_landing_gear(
        mtow_mass,
        x_nlg,
        x_mlg,
        aero_fwd_lim_x,
        aero_aft_lim_x,
        fuselage_diameter_m=fus_diam,
        cg_height_estimate_m=fus_diam * 1.1,
        gear_config=gear_cfg,
    )

    # --- Fuselage + wing outline (same convention as figure_geometry's top view) ---
    for w in plane.wings:
        le = [s.xyz_le for s in w.xsecs]
        te = [s.xyz_le + np.array([s.chord, 0, 0]) for s in w.xsecs]
        x = [p[0] for p in le] + [p[0] for p in reversed(te)]
        y = [p[1] for p in le] + [p[1] for p in reversed(te)]
        for side in [1, -1] if w.symmetric else [1]:
            ax.fill(
                [yi * side for yi in y],
                x,
                color="tab:blue",
                alpha=0.25,
                edgecolor="k",
                linewidth=0.5,
                zorder=1,
            )

    xc = [s.xyz_c[0] for s in fus.xsecs]
    r = [float(s.width) / 2 for s in fus.xsecs]
    x_loop = xc + xc[::-1]
    y_loop = r + [-ri for ri in reversed(r)]
    ax.fill(y_loop, x_loop, color="tab:gray", alpha=0.35, zorder=1)

    # --- Wheels: each drawn individually, NLG vs MLG-* colour-coded ---
    group_colors = {
        "NLG": "#2ecc71",
        "MLG-L": "#e74c3c",
        "MLG-R": "#e74c3c",
        "MLG-Body-L": "#f39c12",
        "MLG-Body-R": "#f39c12",
    }
    seen_labels = set()
    for wheel in gear.wheels:
        color = group_colors.get(wheel.strut_label, "#9b59b6")
        legend_label = (
            wheel.strut_label if wheel.strut_label not in seen_labels else None
        )
        seen_labels.add(wheel.strut_label)
        circ = mpatches.Ellipse(
            (wheel.y, wheel.x),
            width=wheel.width_m,
            height=wheel.diameter_m,
            facecolor=color,
            edgecolor="black",
            linewidth=1.0,
            alpha=0.9,
            zorder=5,
            label=legend_label,
        )
        ax.add_patch(circ)

    fig.suptitle(
        f"Landing Gear Planform  --  NLG: {gear.n_nlg_wheels}x{gear.nlg_tire.name.split(' (')[0]}   "
        f"MLG: {gear.n_mlg_struts} strut(s) x {gear.wheels_per_mlg_strut}w {gear.mlg_tire.name.split(' (')[0]}",
        fontsize=10,
        fontweight="bold",
        y=0.98,
        color=pal.title,
    )
    ax.set_title(
        f"Strut: {gear.strut_material}   |   Track: {gear.track_width_m:.2f} m   |   "
        f"Wheelbase: {gear.wheelbase_m:.2f} m   |   "
        f"Turnover angle: {gear.turnover_angle_deg:.0f} deg "
        f"({'OK' if gear.turnover_ok else 'EXCEEDS LIMIT'})",
        fontsize=8.5,
        pad=8,
    )
    ax.set_xlabel("Y [m]")
    ax.set_ylabel("X [m] (fuselage station)")
    ax.invert_yaxis()  # nose at top, matching figure_geometry's top view
    ax.set_aspect("equal", adjustable="datalim")
    ax.grid(True, alpha=0.3)
    n_legend = len(seen_labels)
    ax.legend(
        loc="upper center",
        bbox_to_anchor=(0.5, -0.05),
        ncol=max(1, n_legend),
        fontsize=8,
        framealpha=1,
    )
    fig.set_layout_engine(None)
    fig.subplots_adjust(top=0.90, bottom=0.11)
    return fig


def figure_fuel_volume_check(
    report: "AnalysisReport",
    config,
    fig: Optional[Figure] = None,
    theme: str | None = None,
) -> Figure:
    """Single-bar check: does the wing physically have room for the fuel the
    design requires?

    Deliberately minimal -- this is a yes/no engineering check
    (:func:`physics.performance.fuel_volume_check`), not a multi-panel
    figure. A green bar means the wing's usable tank volume (Torenbeek
    estimate, converted to mass at the configured fuel density) covers the
    required fuel mass with margin to spare; red means it doesn't.
    """
    from ..physics.performance import fuel_volume_check

    fig = _ensure_fig(fig, (10, 3.2))
    # This figure is deliberately short (one bar), which leaves little room
    # for a title -- MplCanvas figures are created with tight_layout=True,
    # which recomputes spacing on every draw and can leave the title
    # partially clipped against the canvas edge at the small embedded GUI
    # heights this chart is squeezed into. Disable the auto layout engine and
    # reserve the top margin explicitly (same fix as figure_mission_profile).
    fig.set_layout_engine(None)
    fig.subplots_adjust(top=0.78, bottom=0.22, left=0.08, right=0.97)
    ax = fig.add_subplot(111)
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    check = fuel_volume_check(report, config)
    cap_t = check.tank_capacity_kg / 1000.0
    req_t = check.required_fuel_kg / 1000.0
    bar_max = max(cap_t, req_t) * 1.25 if max(cap_t, req_t) > 0 else 1.0

    color = "#27ae60" if check.sufficient else "#e74c3c"
    ax.barh(
        0,
        cap_t,
        height=0.5,
        color=color,
        alpha=0.35,
        edgecolor=color,
        linewidth=1.5,
        label=f"Tank capacity: {cap_t:,.1f} t ({check.tank_volume_m3:,.0f} m³)",
    )
    ax.barh(0, req_t, height=0.22, color=color, label=f"Required fuel: {req_t:,.1f} t")
    ax.axvline(cap_t, color=color, linewidth=1.5, linestyle="--")

    status = "sufficient" if check.sufficient else "INSUFFICIENT"
    margin_t = check.margin_kg / 1000.0
    ax.text(
        0.99,
        0.92,
        f"{status}  (margin: {margin_t:+,.1f} t)",
        transform=ax.transAxes,
        ha="right",
        va="top",
        fontsize=10,
        fontweight="bold",
        color=color,
    )

    ax.set_xlim(0, bar_max)
    ax.set_yticks([])
    ax.set_xlabel("Fuel mass (t)")
    ax.set_title("Wing Fuel-Volume Check", fontweight="bold", loc="left")
    ax.legend(loc="lower right", fontsize=8, framealpha=0.9)
    ax.grid(True, axis="x", alpha=0.3)
    return fig


def _stability_scalars(report: "AnalysisReport"):
    """Shared side-view/metrics scalar quantities, computed once from the
    AnalysisReport (no new VLM runs) so the two split stability figures stay
    numerically consistent with each other."""
    plane = report.airplane
    wing = next((w for w in plane.wings if w.name == "Main Wing"), plane.wings[0])
    hstab = next(
        (w for w in plane.wings if w.name == "Horizontal Stabilizer"),
        plane.wings[1] if len(plane.wings) > 1 else None,
    )

    c_ref = float(plane.c_ref)
    x_cg_aero = float(plane.xyz_ref[0])
    x_cg_phys = float(report.physical_cg[0]) if report.physical_cg else x_cg_aero
    x_wing_ac = float(wing.aerodynamic_center()[0])
    sm = (
        float(report.static_margin)
        if report.static_margin == report.static_margin
        else 0.10
    )
    x_np = x_cg_aero + sm * c_ref

    S_wing = float(wing.area())
    S_tail = float(hstab.area()) if hstab else 0.0
    x_hstab_ac = float(hstab.aerodynamic_center()[0]) if hstab else (x_np + 2.0)
    l_t = x_hstab_ac - x_wing_ac
    V_H = (l_t * S_tail) / (c_ref * S_wing) if c_ref * S_wing > 0 else 0.0

    x_lemac = x_wing_ac - 0.25 * c_ref  # wing AC sits at 25% chord of the MAC

    return dict(
        plane=plane,
        wing=wing,
        hstab=hstab,
        c_ref=c_ref,
        x_cg_aero=x_cg_aero,
        x_cg_phys=x_cg_phys,
        x_wing_ac=x_wing_ac,
        sm=sm,
        x_np=x_np,
        S_wing=S_wing,
        S_tail=S_tail,
        x_hstab_ac=x_hstab_ac,
        l_t=l_t,
        V_H=V_H,
        x_lemac=x_lemac,
    )


def _assign_label_rows(xs: List[float], min_sep: float) -> List[int]:
    """Greedily assign a row index to each (already x-sorted) label so any
    two labels sharing a row are at least ``min_sep`` apart in x.

    Replaces a fixed 3-row cycle (``i % 3``): with only 3 rows, a 4th or 5th
    marker clustered tightly in x (the common case -- Wing AC/Phys CG/Aero
    CG/NP are frequently within a couple of metres of each other) wraps back
    onto a row still occupied by a nearby label, so their annotation boxes
    overlap. Growing the row count on demand guarantees no collision
    regardless of how many markers land close together.
    """
    last_x_per_row: List[float] = []
    rows: List[int] = []
    for x in xs:
        placed = False
        for r, last_x in enumerate(last_x_per_row):
            if x - last_x >= min_sep:
                last_x_per_row[r] = x
                rows.append(r)
                placed = True
                break
        if not placed:
            last_x_per_row.append(x)
            rows.append(len(last_x_per_row) - 1)
    return rows


def figure_stability_side_view(
    report: "AnalysisReport",
    config=None,
    fig: Optional[Figure] = None,
    theme: str | None = None,
) -> Figure:
    """Longitudinal stability diagram -- side-profile view.

    Fuselage side silhouette (the real per-station width envelope, not a
    schematic box) with Wing AC, physical CG, aerodynamic CG, neutral point,
    and H-Stab AC marked by coloured vertical dashed lines; moment-arm
    arrows annotate SM, l_t, and any CG mismatch below the fuselage outline.
    Companion to :func:`figure_stability_metrics` (ruler + Cm-vs-CL +
    numeric table) -- split into two figures so each renders legibly at the
    smaller heights a GUI results tab embeds them at, instead of one cramped
    three-panel figure. All quantities come directly from the
    AnalysisReport -- no new VLM runs.
    """
    s = _stability_scalars(report)
    plane, wing, hstab = s["plane"], s["wing"], s["hstab"]
    c_ref, x_cg_aero, x_cg_phys = s["c_ref"], s["x_cg_aero"], s["x_cg_phys"]
    x_wing_ac, sm, x_np = s["x_wing_ac"], s["sm"], s["x_np"]
    x_hstab_ac, l_t, V_H, x_lemac = s["x_hstab_ac"], s["l_t"], s["V_H"], s["x_lemac"]
    fus = plane.fuselages[0]

    def pct(x: float) -> float:
        return (x - x_lemac) / c_ref * 100.0

    fig = _ensure_fig(fig, (13, 7))
    fig.clf()
    # Layout engine disabled + margins reserved explicitly: the label rows
    # below are sized to the figure *before* any layout engine gets a chance
    # to recompute (and shrink) the axes, so the reserved top/bottom margins
    # actually hold at the small embedded GUI canvas heights this figure is
    # squeezed into (same fix as figure_mission_profile).
    fig.set_layout_engine(None)
    ax_side = fig.add_subplot(111)
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    # --- fuselage silhouette (the actual per-station width envelope) ---
    fus_x = np.array([xs.xyz_c[0] for xs in fus.xsecs])
    fus_z = np.array([xs.xyz_c[2] for xs in fus.xsecs])
    fus_r = np.array([max(0.1, float(xs.width) / 2) for xs in fus.xsecs])

    top_x = np.concatenate([fus_x, fus_x[::-1]])
    top_z = np.concatenate([fus_z + fus_r, fus_z[::-1] - fus_r[::-1]])
    ax_side.fill(
        top_x,
        top_z,
        color="#95a5a6",
        alpha=0.45,
        edgecolor="#616a6e",
        lw=1.2,
        zorder=0,
        label="Fuselage",
    )

    # Wing root cross-section (tapered wedge profile)
    wr = wing.xsecs[0]
    x_wle, z_wr, chord_w = float(wr.xyz_le[0]), float(wr.xyz_le[2]), float(wr.chord)
    th_w = max(0.25, chord_w * 0.12)
    ax_side.fill(
        [x_wle, x_wle + chord_w, x_wle + chord_w, x_wle],
        [z_wr - th_w / 2, z_wr - th_w / 4, z_wr + th_w / 4, z_wr + th_w / 2],
        color="#2980b9",
        alpha=0.6,
        ec="#1a5276",
        lw=1.2,
        label="Wing (root)",
        zorder=3,
    )

    if hstab:
        hr = hstab.xsecs[0]
        x_hle, z_hr, chord_h = float(hr.xyz_le[0]), float(hr.xyz_le[2]), float(hr.chord)
        th_h = max(0.12, chord_h * 0.10)
        ax_side.fill(
            [x_hle, x_hle + chord_h, x_hle + chord_h, x_hle],
            [z_hr - th_h / 2, z_hr - th_h / 4, z_hr + th_h / 4, z_hr + th_h / 2],
            color="#1abc9c",
            alpha=0.6,
            ec="#0e6655",
            lw=1.2,
            label="H-Stab (root)",
            zorder=3,
        )

    z_top = float((fus_z + fus_r).max())
    z_bot = float((fus_z - fus_r).min())

    # Stability markers: name, x-position, colour. Phys CG and Aero CG
    # collapse into one label when autobalance has left them essentially
    # coincident (the common case) -- showing two identical-looking labels
    # on top of each other is pure clutter; the CG-mismatch arrow below
    # still appears whenever they genuinely differ.
    if abs(x_cg_phys - x_cg_aero) < 0.15:
        markers = [
            ("Wing AC", x_wing_ac, "#c0392b"),
            ("Phys/Aero CG", x_cg_aero, "#2980b9"),
            ("Neutral Pt", x_np, "#8e44ad"),
        ]
    else:
        markers = [
            ("Wing AC", x_wing_ac, "#c0392b"),
            ("Phys CG", x_cg_phys, "#e67e22"),
            ("Aero CG", x_cg_aero, "#2980b9"),
            ("Neutral Pt", x_np, "#8e44ad"),
        ]
    if hstab:
        markers.append(("H-Stab AC", x_hstab_ac, "#27ae60"))

    # Dynamically row-stagger labels so any that land close together in x
    # (the common near-CG cluster) never share a row (see _assign_label_rows).
    markers_sorted = sorted(markers, key=lambda t: t[1])
    min_sep = max(1.5, 0.05 * (float(fus_x.max()) - float(fus_x.min())))
    rows_idx = _assign_label_rows([m[1] for m in markers_sorted], min_sep)
    n_rows = max(rows_idx) + 1
    row_height = 0.95

    for (name, x_m, color), row in zip(markers_sorted, rows_idx):
        ax_side.axvline(x_m, color=color, lw=1.5, ls="--", alpha=0.75, zorder=7)
        z_lbl = z_top + 0.35 + row * row_height
        p = pct(x_m)
        # A marker far outside the wing's own MAC (the tail, typically
        # 200-400% MAC aft of LEMAC) makes a "% MAC" figure read as noise
        # rather than information -- show it only in the range where it's
        # actually a meaningful stability-margin quantity. Kept on one line
        # with the distance (rather than its own line) so each label is only
        # two lines tall, leaving more row-to-row clearance for the same
        # vertical budget.
        pct_txt = f"  ({p:.0f}% MAC)" if -60.0 <= p <= 160.0 else ""
        ax_side.annotate(
            f"{name}\n{x_m:.1f} m{pct_txt}",
            xy=(x_m, z_top + 0.05),
            xytext=(x_m, z_lbl),
            ha="center",
            va="bottom",
            fontsize=7.5,
            color=color,
            fontweight="bold",
            arrowprops=dict(arrowstyle="-", color=color, lw=0.8, alpha=0.7),
            bbox=dict(
                boxstyle="round,pad=0.15", fc="white", alpha=0.95, ec=color, lw=0.8
            ),
            zorder=15,
        )

    # --- moment-arm annotations (below fuselage) ---
    # Row height and text offset both grown (0.42->0.9, 0.20->0.45): the SM
    # arrow spans aero CG -> NP, frequently under 2m on a fuselage tens of
    # metres long, so its "<->" arrowheads collapse into a single blob at
    # that x -- with only 0.20 units of clearance, the label sitting right
    # underneath had that blob rendering on top of its own text. The larger
    # row gap is needed too, once the label sits further from its own row:
    # otherwise it instead lands on top of the *next* row's arrow/text.
    gap = 0.9
    below_rows = [z_bot - gap * (k + 1) for k in range(3)]
    label_dy = 0.45

    # Static margin: aero CG -> neutral point
    ax_side.annotate(
        "",
        xy=(x_np, below_rows[0]),
        xytext=(x_cg_aero, below_rows[0]),
        arrowprops=dict(arrowstyle="<->", color="#8e44ad", lw=2.0),
    )
    ax_side.text(
        (x_cg_aero + x_np) / 2,
        below_rows[0] - label_dy,
        f"SM = {sm * 100:.1f}% MAC  =  {sm * c_ref:.2f} m",
        ha="center",
        va="top",
        fontsize=8,
        color="#8e44ad",
        fontweight="bold",
    )

    # Tail moment arm: wing AC -> H-Stab AC
    if hstab:
        ax_side.annotate(
            "",
            xy=(x_hstab_ac, below_rows[1]),
            xytext=(x_wing_ac, below_rows[1]),
            arrowprops=dict(arrowstyle="<->", color="#27ae60", lw=2.0),
        )
        ax_side.text(
            (x_wing_ac + x_hstab_ac) / 2,
            below_rows[1] - label_dy,
            f"l_t = {l_t:.1f} m    ->    V_H = {V_H:.3f}",
            ha="center",
            va="top",
            fontsize=8,
            color="#27ae60",
            fontweight="bold",
        )

    # CG mismatch: physical vs aerodynamic CG
    delta = x_cg_phys - x_cg_aero
    if abs(delta) > 0.15:
        ax_side.annotate(
            "",
            xy=(x_cg_phys, below_rows[2]),
            xytext=(x_cg_aero, below_rows[2]),
            arrowprops=dict(arrowstyle="<->", color="#e74c3c", lw=2.0),
        )
        ax_side.text(
            (x_cg_phys + x_cg_aero) / 2,
            below_rows[2] - label_dy,
            f"CG mismatch  {delta:+.2f} m  ({pct(x_cg_phys) - pct(x_cg_aero):+.1f}% MAC)",
            ha="center",
            va="top",
            fontsize=8,
            color="#e74c3c",
            fontweight="bold",
        )
        n_below_rows = 3
    else:
        n_below_rows = 2

    ax_side.set_xlabel("x — longitudinal position [m]", fontsize=9)
    ax_side.set_ylabel("z — vertical [m]", fontsize=9)
    ax_side.set_title(
        f"Longitudinal Stability Diagram   |   SM = {sm * 100:.1f}% MAC"
        f"   |   V_H = {V_H:.3f}   |   l_t = {l_t:.1f} m",
        fontsize=11,
        fontweight="bold",
        pad=10,
    )
    # "Lower right" collided with the l_t moment-arm arrow, which reaches
    # from the wing to the tail and so frequently spans most of the plot's
    # width along the bottom rows. The fuselage nose (upper-left) carries no
    # markers or annotations at all -- every stability marker clusters near
    # the wing/CG region or further aft at the tail.
    ax_side.legend(loc="upper left", fontsize=8, framealpha=0.85)
    ax_side.grid(True, alpha=0.2)
    ax_side.set_xlim(float(fus_x.min()) - 1.0, float(fus_x.max()) + 2.0)
    # Explicit y-limits sized to the actual label/annotation extent above and
    # below the fuselage -- letting autoscale handle this (as the original,
    # unsplit figure did) only accounts for the fuselage/wing/tail *shapes*,
    # not the annotate() calls layered on top, which can render right at (or
    # past) the axes' top edge where the title sits.
    ax_side.set_ylim(
        z_bot - gap * (n_below_rows + 1) - 0.5,
        z_top + 0.35 + n_rows * row_height + 0.5,
    )
    fig.subplots_adjust(top=0.90, bottom=0.10, left=0.07, right=0.97)
    return fig


def figure_stability_metrics(
    report: "AnalysisReport",
    config=None,
    fig: Optional[Figure] = None,
    theme: str | None = None,
) -> Figure:
    """Stability-number-line ruler + Cm-vs-CL polar + numeric metrics table.

    Companion to :func:`figure_stability_side_view` (fuselage side profile);
    split out of the former single 3-panel ``figure_stability_diagram`` so
    each half gets enough room to stay legible at typical GUI canvas sizes.
    """
    s = _stability_scalars(report)
    c_ref, x_cg_aero, x_cg_phys = s["c_ref"], s["x_cg_aero"], s["x_cg_phys"]
    x_wing_ac, sm, x_np = s["x_wing_ac"], s["sm"], s["x_np"]
    S_wing, S_tail = s["S_wing"], s["S_tail"]
    l_t, V_H, x_lemac = s["l_t"], s["V_H"], s["x_lemac"]

    def pct(x: float) -> float:
        return (x - x_lemac) / c_ref * 100.0

    polar = report.polar
    cl_arr = np.array(polar.get("CL", []))
    cm_arr = np.array(polar.get("Cm", []))
    cl_cruise = float(report.design_point.cl)

    fig = _ensure_fig(fig, (13, 5.5))
    fig.clf()
    fig.set_layout_engine(None)
    # Right margin widened (0.86->0.72): the metrics table below is placed
    # in this reserved strip, and at this figure's typical embedded GUI
    # width the previous strip was only wide enough for a couple of
    # characters of its 9-line monospace block before running off the
    # right edge of the canvas entirely.
    fig.subplots_adjust(top=0.88, bottom=0.16, left=0.06, right=0.72, wspace=0.30)
    ax_ruler, ax_cm = fig.subplots(1, 2)
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    # === Panel 1 -- % MAC STABILITY NUMBER LINE ===
    # CG limit values for the % MAC ruler -- calculated dynamically from NP
    np_pct = pct(x_np)
    if config is not None and hasattr(config, "requirements"):
        target_sm = config.requirements.target_static_margin
        cg_range = config.requirements.cg_range_pct_mac
    else:
        target_sm = 0.10
        cg_range = 15.0

    _aft_lim = np_pct - target_sm * 100.0
    _fwd_lim = _aft_lim - cg_range

    ruler_pts = [
        (pct(x_wing_ac), "#c0392b", "Wing AC", "-", False),
        (pct(x_cg_phys), "#e67e22", "Phys CG", "-", False),
        (pct(x_cg_aero), "#2980b9", "Aero CG", "-", False),
        (pct(x_np), "#8e44ad", "NP", "-", False),
        (_fwd_lim, "#e74c3c", "Fwd\nlim", ":", True),
        (_aft_lim, "#e74c3c", "Aft\nlim", ":", True),
    ]

    all_pts = [
        pct(x_wing_ac),
        pct(x_cg_phys),
        pct(x_cg_aero),
        pct(x_np),
        _fwd_lim,
        _aft_lim,
    ]
    min_p = min(all_pts) - 10.0
    max_p = max(all_pts) + 10.0
    min_tick = int(np.floor(min_p / 10.0) * 10.0)
    max_tick = int(np.ceil(max_p / 10.0) * 10.0)
    min_tick = max(-50, min_tick)
    max_tick = min(150, max_tick)

    ax_ruler.set_xlim(min_tick, max_tick)
    ax_ruler.set_ylim(-1.8, 1.8)
    ax_ruler.axhline(0, color=pal.title, lw=2.5)

    for t in range(min_tick, max_tick + 1, 10):
        ax_ruler.axvline(t, color=pal.title, lw=0.35, alpha=0.25)
        ax_ruler.text(t, -1.55, f"{t}%", ha="center", fontsize=7, color=pal.tick)

    # Safe CG band
    ax_ruler.fill_betweenx(
        [-0.20, 0.20], _fwd_lim, _aft_lim, color="#27ae60", alpha=0.12
    )
    # Static-margin band
    p_cg = pct(x_cg_aero)
    p_np = pct(x_np)
    ax_ruler.fill_betweenx(
        [-0.10, 0.10],
        min(p_cg, p_np),
        max(p_cg, p_np),
        color="#8e44ad",
        alpha=0.35,
        label=f"SM = {sm * 100:.1f}%",
    )

    # Alternating top/bottom by *list position* (i % 2) assumed neighbouring
    # entries in ruler_pts were never close together in x -- but Wing AC/Aero
    # CG (both near-neutral aircraft) or Phys CG/Aft lim commonly land within
    # a couple of percent of each other while landing on the *same* row (two
    # list slots apart is still i%2-equal), stacking their boxes on top of
    # one another. Stagger by actual x-proximity instead (see
    # figure_stability_side_view for the same fix), growing outward to a 3rd
    # row on either side if more than 2 points cluster together.
    visible_pts = [pt for pt in ruler_pts if min_tick <= pt[0] <= max_tick]
    order = sorted(range(len(visible_pts)), key=lambda k: visible_pts[k][0])
    min_sep = (max_tick - min_tick) * 0.08
    rows_sorted = _assign_label_rows([visible_pts[k][0] for k in order], min_sep)
    row_of = {k: r for k, r in zip(order, rows_sorted)}

    for i, (p_val, color, name, ls, is_lim) in enumerate(visible_pts):
        ax_ruler.axvline(p_val, color=color, lw=2.0 if is_lim else 1.5, ls=ls, zorder=5)
        row = row_of[i]
        y_lbl = (1.0 + 0.65 * (row // 2)) * (1 if row % 2 == 0 else -1)
        ax_ruler.text(
            p_val,
            y_lbl,
            f"{name}\n{p_val:.0f}%",
            ha="center",
            va="center",
            fontsize=7.5,
            color=color,
            fontweight="bold",
            bbox=dict(
                boxstyle="round,pad=0.15", fc="white", alpha=0.92, ec=color, lw=0.7
            ),
        )

    if rows_sorted:
        y_extent = 1.0 + 0.65 * (max(rows_sorted) // 2) + 0.5
        ax_ruler.set_ylim(-y_extent, y_extent)

    ax_ruler.legend(fontsize=8, loc="upper left", framealpha=0.85)
    ax_ruler.set_yticks([])
    ax_ruler.set_xlabel("% MAC from LEMAC", fontsize=8.5)
    ax_ruler.set_title("Stability Number Line (% MAC)", fontsize=10, fontweight="bold")

    # === Panel 3 -- Cm vs CL WITH METRICS TABLE ===
    if len(cl_arr) > 0 and len(cm_arr) > 0:
        ax_cm.plot(cl_arr, cm_arr, color=pal.title, lw=2.0, label="VLM  Cm(CL)")
        ax_cm.axhline(0, color="gray", ls="--", lw=1.0, label="Trim  Cm = 0")
        ax_cm.axvline(cl_cruise, color="#3498db", ls=":", lw=1.5)

        idx_c = int(np.argmin(np.abs(cl_arr - cl_cruise)))
        cm_c = float(cm_arr[idx_c])
        ax_cm.scatter(
            [cl_cruise],
            [cm_c],
            color="#e74c3c",
            s=80,
            zorder=10,
            label=f"Cruise  CL={cl_cruise:.3f},  Cm={cm_c:.4f}",
        )

        # Slope = dCm/dCL = -SM
        if len(cl_arr) >= 2:
            slope = (float(cm_arr[-1]) - float(cm_arr[0])) / (
                float(cl_arr[-1]) - float(cl_arr[0]) + 1e-12
            )
            ax_cm.text(
                0.05,
                0.95,
                f"dCm/dCL = {slope:.4f}\n(= −SM,  target = {-sm:.4f})",
                transform=ax_cm.transAxes,
                fontsize=8,
                va="top",
                color="#8e44ad",
                fontweight="bold",
                bbox=dict(
                    fc="white",
                    ec="#8e44ad",
                    lw=0.8,
                    boxstyle="round,pad=0.25",
                    alpha=0.92,
                ),
            )
    else:
        ax_cm.text(
            0.5,
            0.5,
            "No Cm data in polar",
            ha="center",
            va="center",
            transform=ax_cm.transAxes,
            fontsize=10,
            color="gray",
        )

    # Key metrics table (right of plot via text in axes-fraction coords)
    metrics = (
        f"MAC     = {c_ref:.2f} m\n"
        f"Wing AC = {pct(x_wing_ac):+.1f}% MAC\n"
        f"Aero CG = {pct(x_cg_aero):+.1f}% MAC\n"
        f"Phys CG = {pct(x_cg_phys):+.1f}% MAC\n"
        f"NP      = {pct(x_np):+.1f}% MAC\n"
        f"SM      = {sm * 100:.2f}%\n"
        f"V_H     = {V_H:.3f}\n"
        f"l_t     = {l_t:.2f} m\n"
        f"S_t/S   = {(S_tail / S_wing):.4f}"
    )
    # Anchored to the *figure*, not ax_cm's own axes-fraction, so its
    # available width is the reserved right margin above regardless of how
    # wide ax_cm itself ends up (an axes-fraction offset here would instead
    # shrink in lockstep with ax_cm at a narrow canvas -- the opposite of
    # what a text box overflowing that axes needs).
    # Explicit black text: this box's face is a fixed near-white regardless
    # of theme, so it must not inherit the GUI theme's process-wide
    # ``rcParams["text.color"]`` default (white on dark/grey themes) --
    # doing so renders invisible white-on-white text.
    fig.text(
        0.755,
        0.50,
        metrics,
        transform=fig.transFigure,
        fontsize=7.5,
        va="center",
        ha="left",
        family="monospace",
        color="black",
        bbox=dict(boxstyle="round,pad=0.4", fc="#f8f9fa", ec="#bdc3c7", lw=1.0),
    )

    ax_cm.set_xlabel("CL", fontsize=9)
    ax_cm.set_ylabel("Cm  (about aero CG)", fontsize=9)
    ax_cm.set_title("Pitching Moment vs Lift", fontsize=10, fontweight="bold")
    ax_cm.legend(fontsize=7.5, loc="best")
    ax_cm.grid(True, alpha=0.3)

    return fig


def figure_mass_distribution(
    report: "AnalysisReport", fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Plan-view mass distribution with component mass bubbles.

    Draws the aircraft top-view outline and overlays scaled circles at each
    component centroid, sized proportionally to the square root of mass.
    Works for any aircraft layout (wing-mounted engines, tail-mounted, etc.)
    """
    import matplotlib.patches as patches

    fig = _ensure_fig(fig, (12, 7))
    fig.clf()
    # Disable the auto layout engine and reserve a fixed top margin: several
    # component labels are offset a fixed number of *points* from their
    # bubble (see _LABEL_OFFSETS below), which -- at the small embedded GUI
    # canvas heights this figure gets squeezed into -- can push a label for
    # an aft/forward-most component (H-Stab, V-Stab, Gear) far enough to
    # overlap the title above the axes. Same fix as figure_mission_profile.
    fig.set_layout_engine(None)
    fig.subplots_adjust(top=0.88, bottom=0.08, left=0.09, right=0.97)
    ax = fig.add_subplot(111, aspect="equal")
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    plane = report.airplane
    masses = report.component_masses
    coords = report.mass_coordinates
    cg = report.physical_cg

    if not masses:
        ax.text(
            0, 0, "No mass data available", ha="center", va="center", color=pal.title
        )
        return fig

    total_mass = sum(max(0.0, v) for v in masses.values())

    # --- Draw wing planform fill (semi-transparent) ---
    for wing in plane.wings:
        le_pts = [s.xyz_le for s in wing.xsecs]
        te_pts = [s.xyz_le + np.array([s.chord, 0, 0]) for s in wing.xsecs]
        sides = [1, -1] if wing.symmetric else [1]
        for side in sides:
            y_le = [side * p[1] for p in le_pts]
            y_te = [side * p[1] for p in te_pts]
            x_le = [p[0] for p in le_pts]
            x_te = [p[0] for p in te_pts]
            poly_y = y_le + list(reversed(y_te))
            poly_x = x_le + list(reversed(x_te))
            col = "#3498db" if "Main" in wing.name else "#95a5a6"
            ax.fill(poly_y, poly_x, color=col, alpha=0.12, edgecolor=col, lw=1.0)

    # --- Draw fuselage outline ---
    fus = plane.fuselages[0]
    fus_xs = [xsec.xyz_c[0] for xsec in fus.xsecs]
    fus_ws = [xsec.width / 2 for xsec in fus.xsecs]
    y_pos = [w for w in fus_ws] + [-w for w in reversed(fus_ws)]
    x_pos = fus_xs + list(reversed(fus_xs))
    ax.fill(y_pos, x_pos, color="#bdc3c7", alpha=0.15, edgecolor="#7f8c8d", lw=1.0)

    # --- Draw engine nacelles (if any) ---
    for nac in plane.fuselages[1:]:
        if "Nacelle" in nac.name or "nacelle" in nac.name:
            nac_xs = [xsec.xyz_c[0] for xsec in nac.xsecs]
            nac_ys = [xsec.xyz_c[1] for xsec in nac.xsecs]
            nac_rs = [
                xsec.width / 2 if hasattr(xsec, "width") else xsec.radius
                for xsec in nac.xsecs
            ]
            ax.fill(
                [y + r for y, r in zip(nac_ys, nac_rs)]
                + [y - r for y, r in zip(reversed(nac_ys), reversed(nac_rs))],
                nac_xs + list(reversed(nac_xs)),
                color="#e67e22",
                alpha=0.25,
                edgecolor="#d35400",
                lw=0.8,
            )

    # --- Draw component mass bubbles ---
    COLORS = {
        "Payload": "#27ae60",
        "Fuel": "#e67e22",
        "Propulsion": "#c0392b",
        "Wing": "#2980b9",
        "Fuselage": "#7f8c8d",
        "Systems": "#8e44ad",
        "Furnishings": "#a569bd",
        "Gear": "#34495e",
        "H-Stab": "#1abc9c",
        "V-Stab": "#16a085",
    }

    fus_len = fus.xsecs[-1].xyz_c[0] - fus.xsecs[0].xyz_c[0]
    scale = (
        fus_len * 0.04
    )  # bubble radius scale: 4% of fuselage length per √(normalised mass)

    sorted_keys = sorted(masses.keys(), key=lambda k: masses.get(k, 0.0))
    for k in sorted_keys:
        m = masses.get(k, 0.0)
        if m <= 0 or k not in coords:
            continue
        x_comp, y_comp, _ = coords[k]
        r = np.sqrt(m / max(total_mass, 1.0)) * scale * 8
        color = COLORS.get(k, "#95a5a6")

        circ = patches.Circle(
            (y_comp, x_comp), r, color=color, alpha=0.55, ec="white", lw=1.2, zorder=10
        )
        ax.add_patch(circ)

        # Symmetric components (e.g. wing, gear) -- draw mirrored bubble
        if abs(y_comp) < 0.5 and k in ("Wing", "Fuel", "Gear"):
            pass  # these are already centered

        pct = m / total_mass * 100
        # Per-component offsets spread labels radially to avoid pile-up at the centreline.
        # Positive x-offset = rightward in plan view; positive y-offset = toward tail (aft).
        # A flat ~15-25pt offset is fine when bubbles happen to be far apart, but
        # Wing/Systems centroids commonly land within a few metres of each
        # other near the wing box (likewise Gear/Propulsion near the main
        # gear bay), close enough that those near-identical offsets put the
        # two label boxes right on top of one another. Spaced further apart
        # (and onto clearly different radial directions, not just nearby
        # points on the same side) so a close base position does not
        # guarantee a colliding label.
        _LABEL_OFFSETS = {
            "Payload": (70, 30),
            "Fuel": (-70, 35),
            "Propulsion": (85, -25),
            "Wing": (-50, -60),
            "Fuselage": (75, 8),
            "Systems": (-95, 20),
            "Furnishings": (-120, -45),
            "Gear": (40, -75),
            "H-Stab": (55, 38),
            "V-Stab": (-55, 38),
        }
        dx, dy = _LABEL_OFFSETS.get(k, (20, 12 if y_comp >= 0 else -20))
        ax.annotate(
            f"{k}\n{m / 1000:.1f} t  ({pct:.0f}%)",
            xy=(y_comp, x_comp),
            xytext=(dx, dy),
            textcoords="offset points",
            fontsize=7.5,
            fontweight="bold",
            color="black",
            arrowprops=dict(
                arrowstyle="->", color="#555", lw=0.7, connectionstyle="arc3,rad=0.15"
            ),
            bbox=dict(boxstyle="round,pad=0.2", fc="white", alpha=0.82, ec=color),
            zorder=20,
        )

    # --- Draw CG markers ---
    ax.scatter(
        [0],
        [cg[0]],
        s=180,
        c="#f39c12",
        marker="X",
        zorder=100,
        edgecolors="black",
        lw=0.8,
        label=f"Physical CG  x={cg[0]:.1f} m",
    )
    ax.scatter(
        [0],
        [plane.xyz_ref[0]],
        s=100,
        c="#3498db",
        marker="D",
        zorder=99,
        edgecolors="black",
        lw=0.8,
        label=f"Aero CG  x={plane.xyz_ref[0]:.1f} m",
    )

    # --- CG mismatch annotation ---
    delta = cg[0] - plane.xyz_ref[0]
    if abs(delta) > 0.2:
        ax.annotate(
            "",
            xy=(0, cg[0]),
            xytext=(0, plane.xyz_ref[0]),
            arrowprops=dict(arrowstyle="<->", color="#e74c3c", lw=2),
        )
        ax.text(
            fus.xsecs[len(fus.xsecs) // 2].width * 0.7,
            (cg[0] + plane.xyz_ref[0]) / 2,
            f"Δ={delta:+.1f} m",
            color="#e74c3c",
            fontsize=9,
            fontweight="bold",
            va="center",
        )

    half_span = plane.b_ref / 2
    ax.set_xlim(-half_span * 1.05, half_span * 1.05)
    # More generous longitudinal padding than a flat 8%: several component
    # labels sit near the fore/aft ends of the fuselage (H-Stab/V-Stab near
    # the tail especially) and are themselves offset further out in points
    # (see _LABEL_OFFSETS above) -- an 8% pad left their boxes right at, or
    # past, the axes edge where the title sits.
    ax.set_ylim(-fus_len * 0.14, fus_len * 1.20)
    ax.set_xlabel("y (lateral) [m]", fontsize=9)
    ax.set_ylabel("x (longitudinal) [m]", fontsize=9)
    # Spelled out rather than using "∝" (PROPORTIONAL TO, U+221D): that glyph
    # is missing from Arial, which AeroSandbox's draw_three_view can make the
    # process-wide default font (see figure_asb_threeview) -- spelling it out
    # means this title is never at the mercy of that global side effect.
    ax.set_title(
        "Mass Distribution — Plan View  (bubble area scales with mass)",
        fontsize=11,
        fontweight="bold",
    )
    # "Upper right" in this plot's (lateral, longitudinal) axes is the tail
    # region -- exactly where the H-Stab/V-Stab bubbles and their labels
    # live, so the CG legend was landing right on top of them. The nose
    # (bottom of the longitudinal axis) carries no mass-component labels.
    ax.legend(loc="lower right", fontsize=8, framealpha=0.85)
    ax.grid(True, linestyle=":", alpha=0.3)
    ax.set_aspect("equal")
    return fig


# --- cabin / payload layout figures ----------------------------------------
# Colours shared by the 2D deck plans and the 3D preview.
_CLASS_COLORS = {
    "First": "#8e44ad",
    "Business": "#2980b9",
    "Premium": "#16a085",
    "Economy": "#27ae60",
}
_KIND_COLORS = {
    "galley": "#e67e22",
    "lav": "#5dade2",
    "exit": "#e74c3c",
    "bag": "#95a5a6",
}
_DECK_ORDER = ["upper", "main", "lower"]  # top-to-bottom visual order


def _item_color(it) -> str:
    if it.kind == "seat_row":
        return _CLASS_COLORS.get(it.meta.get("cls", "Economy"), "#27ae60")
    if it.kind == "uld":
        return it.meta.get("color", "#3498db")
    return _KIND_COLORS.get(it.kind, "#bdc3c7")


def _draw_seat_row(ax, it, col, patches) -> None:
    """LOPA-style rendering of one seat row: individual seats in lateral
    blocks separated by the aisles (FAR/CS-25.817 block split from the layout
    engine's metadata). Unoccupied seat positions are drawn hollow/grey."""
    blocks = it.meta.get("blocks")
    if not blocks:
        # Legacy layouts without block metadata: fall back to one band.
        ax.add_patch(
            patches.Rectangle(
                (it.x - it.length / 2, it.y - it.width / 2),
                it.length,
                it.width,
                facecolor=col,
                edgecolor="white",
                lw=0.4,
                alpha=0.7,
                zorder=4,
            )
        )
        return
    seat_w = it.meta.get("seat_w", 0.45)
    aisle_w = it.meta.get("aisle_w", 0.51)
    ab = max(1, it.meta.get("abreast", sum(blocks)))
    filled = it.meta.get("filled", ab)
    total_w = sum(blocks) * seat_w + (len(blocks) - 1) * aisle_w
    y = it.y - total_w / 2
    seat_idx = 0
    x0 = it.x - it.length / 2
    for b in blocks:
        for k in range(b):
            occupied = seat_idx < filled
            ax.add_patch(
                patches.Rectangle(
                    (x0 + 0.06, y + k * seat_w + 0.03),
                    it.length - 0.12,
                    seat_w - 0.06,
                    facecolor=col if occupied else "none",
                    edgecolor=col if occupied else "#b0b7bd",
                    lw=0.45,
                    alpha=0.9 if occupied else 0.6,
                    zorder=4,
                )
            )
            seat_idx += 1
        y += b * seat_w + aisle_w


def _draw_deck_plan(ax, layout, cg, deck_name: str) -> None:
    """Top-view plan of one deck: cabin outline + payload items (LOPA style)."""
    import matplotlib.patches as patches

    if deck_name == "lower":
        deck = cg.lower_deck
    else:
        deck = next(
            (d for d in cg.passenger_decks if d.name == deck_name),
            cg.passenger_decks[0],
        )

    items = layout.by_deck(deck_name)
    xs_lo = cg.cabin_start_x
    xs_hi = cg.cabin_end_x + 0.3 * cg.tailcone_len
    if items:
        xs_lo = min(xs_lo, min(it.x - it.length / 2 for it in items))
        xs_hi = max(xs_hi, max(it.x + it.length / 2 for it in items))

    # Cabin envelope (usable floor width) for this deck.
    xs = np.linspace(xs_lo, xs_hi, 80)
    half = np.array([cg.usable_width(deck, x) / 2 for x in xs])
    ax.fill_between(xs, -half, half, color="#ecf0f1", alpha=0.5, zorder=0)
    ax.plot(xs, half, color="#7f8c8d", lw=0.8)
    ax.plot(xs, -half, color="#7f8c8d", lw=0.8)

    # Wing-box carry-through blocks the lower holds -- show why the hold splits.
    if deck_name == "lower":
        wb0, wb1 = cg.wing_box_x_range()
        ax.axvspan(wb0, wb1, color="#8395a7", alpha=0.20, zorder=1)
        ax.text(
            (wb0 + wb1) / 2,
            0.0,
            "wing box",
            ha="center",
            va="center",
            fontsize=7,
            color="#57606f",
            style="italic",
            zorder=2,
        )

    n_seats = 0
    for it in items:
        col = _item_color(it)
        if it.kind == "exit":
            # Door cutout drawn to its FAR/CS-25.807 minimum width on the wall.
            ax.add_patch(
                patches.Rectangle(
                    (it.x - it.length / 2, it.y - 0.14),
                    it.length,
                    0.28,
                    facecolor=col,
                    edgecolor="white",
                    lw=0.4,
                    zorder=6,
                )
            )
            ax.annotate(
                it.meta.get("type", it.label),
                (it.x, it.y),
                fontsize=6,
                color=col,
                fontweight="bold",
                ha="center",
                va="bottom" if it.y > 0 else "top",
                xytext=(0, 5 if it.y > 0 else -5),
                textcoords="offset points",
            )
            continue
        if it.kind == "seat_row":
            n_seats += it.meta.get("filled", 0)
            _draw_seat_row(ax, it, col, patches)
            continue
        alpha = 0.85
        hatch = None
        if it.kind == "uld" or (it.kind == "bag" and it.meta.get("uld")):
            alpha = 0.35 + 0.6 * it.meta.get("fill", 1.0)
        elif it.kind == "galley":
            hatch = "///"
        elif it.kind == "lav":
            hatch = "xx"
        rect = patches.Rectangle(
            (it.x - it.length / 2, it.y - it.width / 2),
            it.length,
            it.width,
            facecolor=col,
            edgecolor="white",
            lw=0.4,
            alpha=alpha,
            zorder=4,
            hatch=hatch,
        )
        ax.add_patch(rect)

    ax.axvline(layout.cg_x, color="#34495e", ls="--", lw=1.4, zorder=7)
    ax.set_xlim(xs_lo, xs_hi)
    maxhalf = float(half.max()) if len(half) else 2.0
    ax.set_ylim(-maxhalf * 1.25, maxhalf * 1.25)
    ax.set_aspect("equal")

    # Per-deck stats in the title: seats or ULDs, plus floor utilization.
    util = (layout.summary.get("deck_utilization") or {}).get(deck_name)
    parts = [f"{deck_name.capitalize()} deck"]
    if n_seats:
        parts.append(f"{n_seats} seats")
    n_cans = sum(
        1
        for it in items
        if it.kind == "uld" or (it.kind == "bag" and it.meta.get("uld"))
    )
    if n_cans:
        parts.append(f"{n_cans} ULDs")
    if util is not None:
        parts.append(f"floor {util:.0f}% used")
    ax.set_title("  —  ".join(parts), fontsize=9, fontweight="bold", loc="left")
    ax.set_yticks([])
    ax.grid(True, axis="x", alpha=0.2)


def _draw_payload_side(ax, layout, cg, plane) -> None:
    """Side profile (x-z): fuselage silhouette + deck floors + payload + CG."""
    import matplotlib.patches as patches

    fus = plane.fuselages[0]
    fx = np.array([float(s.xyz_c[0]) for s in fus.xsecs])
    fz = np.array([float(s.xyz_c[2]) for s in fus.xsecs])
    # FuselageXSec always has .height set (a circular section built via
    # radius= gets height == width at construction; there's no .radius
    # attribute to read back afterward). The previous `a or b or c` chain
    # meant a genuinely tapered-to-zero station (the nose/tail tip) fell
    # through every branch to the final hardcoded fallback (1.5 m radius),
    # drawing a blunt bump at both fuselage ends instead of following the
    # real taper to a point.
    fh = np.array([max(0.05, float(s.height) / 2) for s in fus.xsecs])
    ax.fill(
        np.concatenate([fx, fx[::-1]]),
        np.concatenate([fz + fh, (fz - fh)[::-1]]),
        color="#bdc3c7",
        alpha=0.25,
        edgecolor="#7f8c8d",
        lw=1.0,
        zorder=0,
    )

    decks = [d for d in cg.passenger_decks] + [cg.lower_deck]
    xs = np.linspace(cg.cabin_start_x, cg.cabin_end_x + 0.3 * cg.tailcone_len, 60)
    for d in decks:
        zf = np.array([cg.floor_z(d, x) for x in xs])
        ax.plot(xs, zf, color="#34495e", ls=":", lw=0.9, alpha=0.6)

    # Wing-box carry-through (lower-hold exclusion zone).
    wb0, wb1 = cg.wing_box_x_range()
    ax.axvspan(wb0, wb1, color="#8395a7", alpha=0.15, zorder=1)

    for it in layout.items:
        col = _item_color(it)
        if it.kind == "exit":
            # Door cutout at its FAR/CS-25.807 minimum width x height.
            ax.add_patch(
                patches.Rectangle(
                    (it.x - it.length / 2, it.z - it.height / 2),
                    it.length,
                    max(it.height, 0.3),
                    facecolor="none",
                    edgecolor=col,
                    lw=1.0,
                    zorder=6,
                )
            )
            continue
        ax.add_patch(
            patches.Rectangle(
                (it.x - it.length / 2, it.z - it.height / 2),
                it.length,
                max(it.height, 0.3),
                facecolor=col,
                edgecolor="white",
                lw=0.3,
                alpha=0.7,
                zorder=4,
            )
        )

    ax.axvline(
        layout.cg_x,
        color="#e74c3c",
        ls="--",
        lw=1.6,
        zorder=8,
        label=f"Payload CG  {cg.x_to_pct_mac(layout.cg_x):.1f}% MAC",
    )
    ax.set_xlim(float(fx.min()), float(fx.max()))
    ax.set_aspect("equal")
    ax.set_title(
        "Side profile (decks + payload + CG)", fontsize=9, fontweight="bold", loc="left"
    )
    ax.set_xlabel("x [m]", fontsize=8)
    ax.legend(loc="upper right", fontsize=7, framealpha=0.85)
    ax.grid(True, alpha=0.2)


def figure_cabin_payload(
    layout, plane, config, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Detailed cabin/payload layout: per-deck top-view plans + a side profile.

    Draws the seat map (passenger) or ULD manifest (cargo) inside the true
    fuselage outline for each occupied deck, plus a side profile showing the deck
    floors and the payload CG. Handles lower-deck holds and the A380 double deck.
    """
    from ..physics.payload import CabinGeometry

    fig = _ensure_fig(fig, (13, 9))
    fig.clf()
    pal = get_palette(theme)
    fig.patch.set_facecolor(pal.bg)

    def _apply_theme() -> None:
        _theme_figure(fig, pal)

    if layout is None or not layout.items:
        ax = fig.add_subplot(111)
        _apply_theme()
        ax.text(
            0.5,
            0.5,
            "No payload layout available",
            ha="center",
            va="center",
            color=pal.title,
        )
        return fig

    cg = CabinGeometry(plane, config.geometry, config.cabin.passenger.wall_thickness_m)
    decks_present = [d for d in _DECK_ORDER if layout.by_deck(d)]
    n = len(decks_present)
    if n + 1 > 1:
        axes = list(fig.subplots(n + 1, 1))
    else:
        axes = [fig.add_subplot(111)]
    _apply_theme()

    for ax, deck in zip(axes[:n], decks_present):
        _draw_deck_plan(ax, layout, cg, deck)
    _draw_payload_side(axes[n], layout, cg, plane)

    s = layout.summary
    if layout.mode == "cargo":
        title = (
            f"Cargo manifest — {s.get('payload_t', 0):.1f} t in "
            f"{s.get('n_ulds', 0)} ULDs "
            f"({s.get('fill_pct', 0):.0f}% of {s.get('capacity_t', 0):.0f} t capacity)\n"
            f"CG {s.get('achieved_cg_pct_mac', 0):.1f}% MAC "
            f"(target {s.get('target_cg_pct_mac', 0):.0f}%)   |   {s.get('strategy', '')}"
        )
    else:
        cls = s.get("classes", {})
        mix = "  ".join(f"{k[0]}{v}" for k, v in cls.items() if v)
        hold_bits = f"{s.get('bag_mass_t', 0):.1f} t bags"
        if s.get("belly_cargo_t", 0) > 0:
            hold_bits += f" + {s.get('belly_cargo_t', 0):.1f} t belly cargo"
        if s.get("hold_ulds", 0):
            hold_bits += f" in {s.get('hold_ulds', 0)} ULDs"
        title = (
            f"Passenger cabin — {s.get('seated_pax', 0)} seats [{mix}]  "
            f"({s.get('max_abreast', 0)}-abreast, "
            f"{s.get('n_aisles', 1)} aisle{'s' if s.get('n_aisles', 1) > 1 else ''} × "
            f"{s.get('aisle_width_m', 0.51) * 100:.0f} cm)\n"
            f"{s.get('exit_pairs', 0)}× Type {s.get('exit_type', '')} exit pairs   |   "
            f"{s.get('galleys', 0)} galleys / {s.get('lavatories', 0)} lavs   |   "
            f"{hold_bits}   |   CG {s.get('cg_pct_mac', 0):.1f}% MAC"
        )
    fig.suptitle(title, fontsize=9.5, fontweight="bold", color=pal.title)

    # Shared legend: classes present, monuments, exits, hold ULDs.
    import matplotlib.patches as mpatches

    handles = []
    if layout.mode == "passenger":
        for name, count in (s.get("classes") or {}).items():
            if count:
                handles.append(
                    mpatches.Patch(color=_CLASS_COLORS.get(name, "#27ae60"), label=name)
                )
        handles += [
            mpatches.Patch(
                facecolor=_KIND_COLORS["galley"], hatch="///", label="Galley"
            ),
            mpatches.Patch(facecolor=_KIND_COLORS["lav"], hatch="xx", label="Lav"),
            mpatches.Patch(color=_KIND_COLORS["bag"], label="Bags/cargo ULD"),
            mpatches.Patch(
                color=_KIND_COLORS["exit"], label=f"Type {s.get('exit_type', '')} exit"
            ),
        ]
    else:
        seen = {}
        for it in layout.items:
            if it.kind == "uld":
                code = it.meta.get("uld", "ULD")
                seen.setdefault(code, it.meta.get("color", "#3498db"))
        handles = [mpatches.Patch(color=c, label=code) for code, c in seen.items()]
    if handles:
        fig.legend(
            handles=handles,
            loc="lower center",
            ncol=min(len(handles), 7),
            fontsize=7,
            framealpha=0.85,
            bbox_to_anchor=(0.5, 0.0),
        )
    fig.tight_layout(rect=(0, 0.04, 1, 0.97))
    return fig


def figure_cabin_payload_3d(
    layout, plane, config, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """3D Cabin/Payload preview: fuselage wireframe + deck floors + item boxes.

    Mirrors the exterior 3D live preview's look so the two panels feel of a piece.
    Renders every deck (main / lower / upper) so lower-deck holds and the A380
    upper deck are visible.
    """
    from ..physics.payload import CabinGeometry

    fig = _ensure_fig(fig, (10, 6))
    fig.clf()
    pal = get_palette(theme)
    fig.patch.set_facecolor(pal.bg)
    ax = fig.add_subplot(111, projection="3d")
    ax.set_facecolor(pal.bg)

    is_dark_theme = pal.bg != "#ffffff"
    fus_color = "#888888" if is_dark_theme else "#555555"
    try:
        import matplotlib as mpl

        with (
            mpl.rc_context()
        ):  # see figure_wireframe_wing: draw_wireframe() global-rcParams guard
            plane.fuselages[0].draw_wireframe(
                ax=ax,
                show=False,
                color=fus_color,
                thin_linewidth=0.5,
                thick_linewidth=0.5,
            )
    except Exception:
        pass

    if layout is not None and layout.items:
        cg = CabinGeometry(
            plane, config.geometry, config.cabin.passenger.wall_thickness_m
        )
        # Deck floor planes (semi-transparent).
        decks = [d for d in cg.passenger_decks] + [cg.lower_deck]
        xs = np.linspace(cg.cabin_start_x, cg.cabin_end_x + 0.25 * cg.tailcone_len, 12)
        for d in decks:
            if not layout.by_deck(d.name):
                continue
            X = np.array([[x, x] for x in xs])
            half = np.array([cg.usable_width(d, x) / 2 for x in xs])
            Y = np.array([[-h, h] for h in half])
            Z = np.array([[cg.floor_z(d, x), cg.floor_z(d, x)] for x in xs])
            try:
                ax.plot_surface(X, Y, Z, color="#3498db", alpha=0.06, linewidth=0)
            except Exception:
                pass
        # Item boxes.
        for it in layout.items:
            if it.kind == "exit":
                ax.scatter(
                    [it.x], [it.y], [it.z], color=_item_color(it), s=14, zorder=5
                )
                continue
            col = _item_color(it)
            try:
                ax.bar3d(
                    it.x - it.length / 2,
                    it.y - it.width / 2,
                    it.z - it.height / 2,
                    it.length,
                    it.width,
                    max(it.height, 0.3),
                    color=col,
                    alpha=0.85,
                    shade=True,
                )
            except Exception:
                pass

    ax.set_axis_off()
    mode = layout.mode.capitalize() if layout is not None else "Payload"
    ax.set_title(
        f"Cabin / Payload — {mode}", color=pal.title, fontsize=10, fontweight="bold"
    )

    # Equal aspect.
    try:
        limits = [ax.get_xlim(), ax.get_ylim(), ax.get_zlim()]
        max_range = max(hi - lo for lo, hi in limits) / 2.0
        for i, (lo, hi) in enumerate(limits):
            mid = (lo + hi) / 2.0
            [ax.set_xlim, ax.set_ylim, ax.set_zlim][i](mid - max_range, mid + max_range)
    except Exception:
        pass
    return fig


def _mission_time_min(mission) -> np.ndarray:
    return np.asarray(mission.time_s, dtype=float) / 60.0


def figure_mission_profile(
    mission, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Four-panel SUAVE mission profile: altitude, mass, true airspeed, and SFC vs. time.

    ``mission`` is an ``alas.integration.suave_bridge.MissionResult`` with
    ``status == "ok"``. This is the "at a glance" plot -- altitude/mass/TAS
    from SUAVE's own ``plot_flight_conditions``/``plot_altitude_sfc_weight``,
    plus the SFC panel that completes ``plot_altitude_sfc_weight``. See
    ``figure_mission_velocities`` (TAS/EAS/Mach), ``figure_mission_flight_path``
    (range/pitch), ``figure_mission_aero_coefficients``,
    ``figure_mission_aero_forces``, and ``figure_mission_drag_components`` for
    the rest of the breadth SUAVE's own ``plot_mission()`` covered.
    """
    fig = _ensure_fig(fig, (10, 10.5))
    # MplCanvas figures are created with tight_layout=True, which recomputes
    # spacing automatically on every draw and silently overrides any manual
    # subplots_adjust() call below -- disable it so the reserved margins
    # below actually stick.
    #
    # Each panel is labelled with its own small title (above the axes)
    # instead of a rotated y-axis label (to its left): a rotated label's
    # footprint is a fixed text height regardless of how narrow or short the
    # canvas gets, so at a narrow embedded GUI width the four labels' text
    # can literally render on top of each other (each one centred on its
    # own row, but with no room between rows for the rotated glyphs) --
    # exactly the "Total mass (kg)" / "Altitude (ft)" overlap this was
    # reported with. A left-aligned title has nothing to collide with
    # *vertically* (each sits in its own subplot's reserved hspace gap) and
    # degrades gracefully (just clips/wraps) rather than catastrophically at
    # narrow widths.
    fig.set_layout_engine(None)
    fig.subplots_adjust(bottom=0.13, top=0.89, hspace=0.55, left=0.11, right=0.97)
    ax_alt, ax_mass, ax_tas, ax_sfc = fig.subplots(4, 1, sharex=True)
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    time_min = _mission_time_min(mission)
    altitude_ft = np.asarray(mission.altitude_m) / 0.3048
    tas_kt = np.asarray(mission.tas_m_s) * 1.943844
    # Tonnes, not kg: at this panel's typical embedded width, 6-digit kg
    # tick labels ("150000") were wide enough to get clipped against the
    # figure's left edge -- losing their leading digit ("50000") -- given
    # the fixed left-margin fraction reserved below. Tonnes keeps labels to
    # 3-4 digits regardless of aircraft size, which comfortably fits that
    # margin at any canvas width instead of just the widest ones.
    mass_t = np.asarray(mission.mass_kg) / 1000.0
    sfc = np.asarray(mission.columns.get("SFC_kg_kgf_hr", []))

    ax_alt.plot(time_min, altitude_ft, color="tab:blue", linewidth=1.5)
    ax_alt.set_title("Altitude (ft)", fontsize=9.5, fontweight="bold", loc="left")
    ax_alt.grid(True, alpha=0.3)

    ax_mass.plot(time_min, mass_t, color="tab:red", linewidth=1.5)
    ax_mass.set_title("Total mass (t)", fontsize=9.5, fontweight="bold", loc="left")
    ax_mass.grid(True, alpha=0.3)

    ax_tas.plot(time_min, tas_kt, color="tab:green", linewidth=1.5)
    ax_tas.set_title("True airspeed (kt)", fontsize=9.5, fontweight="bold", loc="left")
    ax_tas.grid(True, alpha=0.3)

    ax_sfc.plot(time_min, sfc, color="tab:orange", linewidth=1.5)
    ax_sfc.set(xlabel="Time (min)")
    ax_sfc.set_title("SFC (kg/kgf-hr)", fontsize=9.5, fontweight="bold", loc="left")
    ax_sfc.grid(True, alpha=0.3)

    fig.suptitle("Mission Profile", fontweight="bold", y=0.985, color=pal.title)

    summary = mission.summary or {}
    if summary:
        fig.text(
            0.5,
            0.02,
            f"Fuel burned: {summary.get('fuel_burned_kg', 0):,.0f} kg   |   "
            f"Block time: {summary.get('block_time_s', 0) / 3600.0:.2f} h",
            ha="center",
            va="bottom",
            fontsize=9,
            color="gray",
        )
    return fig


def figure_mission_velocities(
    mission, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Two-panel TAS/EAS (overlaid, directly comparable) and Mach vs. time.

    Mirrors SUAVE's own ``plot_aircraft_velocities``: EAS diverging from TAS
    as altitude/Mach increase is the compressibility signature that a lone
    TAS trace hides.
    """
    fig = _ensure_fig(fig, (10, 6))
    fig.set_layout_engine(None)
    fig.subplots_adjust(top=0.88, bottom=0.12, hspace=0.45, left=0.09, right=0.97)
    ax_v, ax_mach = fig.subplots(2, 1, sharex=True)
    pal = get_palette(theme)
    _theme_figure(fig, pal)
    time_min = _mission_time_min(mission)
    cols = mission.columns

    # Panel identity as a left-aligned title (above the axes) rather than a
    # rotated y-axis label (to its left): the rotated label's fixed text
    # footprint doesn't shrink with the canvas, so at a narrow embedded GUI
    # width it can clip off entirely / collide with a neighbouring panel's
    # label (see figure_mission_profile for the same fix, applied there
    # first after this exact failure was reported for its 4-panel layout).
    tas_kt = np.asarray(mission.tas_m_s) * 1.943844
    eas_kt = np.asarray(cols.get("EAS_m_s", [])) * 1.943844
    ax_v.plot(time_min, tas_kt, color="tab:green", linewidth=1.5, label="TAS")
    ax_v.plot(time_min, eas_kt, color="tab:blue", linewidth=1.5, label="EAS")
    ax_v.set_title("Airspeed (kt)", fontsize=9.5, fontweight="bold", loc="left")
    ax_v.legend(loc="best", fontsize=8)
    ax_v.grid(True, alpha=0.3)

    ax_mach.plot(time_min, cols.get("Mach", []), color="tab:purple", linewidth=1.5)
    ax_mach.set(xlabel="Time (min)")
    ax_mach.set_title("Mach", fontsize=9.5, fontweight="bold", loc="left")
    ax_mach.grid(True, alpha=0.3)

    fig.suptitle("Airspeeds", fontweight="bold", y=0.985, color=pal.title)
    return fig


def figure_mission_flight_path(
    mission, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Two-panel cumulative range and pitch angle vs. time.

    Completes the breadth of SUAVE's own ``plot_flight_conditions`` not
    already covered by ``figure_mission_profile``'s altitude panel.
    """
    fig = _ensure_fig(fig, (10, 6))
    fig.set_layout_engine(None)
    fig.subplots_adjust(top=0.88, bottom=0.12, hspace=0.45, left=0.09, right=0.97)
    ax_range, ax_pitch = fig.subplots(2, 1, sharex=True)
    pal = get_palette(theme)
    _theme_figure(fig, pal)
    time_min = _mission_time_min(mission)
    cols = mission.columns

    # See figure_mission_profile: left-aligned panel titles instead of
    # rotated y-axis labels, which can clip off or overlap a neighbouring
    # panel's label at a narrow embedded GUI canvas width.
    range_nm = np.asarray(cols.get("Range_m", [])) / 1852.0
    ax_range.plot(time_min, range_nm, color="tab:blue", linewidth=1.5)
    ax_range.set_title("Range (nm)", fontsize=9.5, fontweight="bold", loc="left")
    ax_range.grid(True, alpha=0.3)

    ax_pitch.plot(time_min, cols.get("Pitch_deg", []), color="tab:red", linewidth=1.5)
    ax_pitch.set(xlabel="Time (min)")
    ax_pitch.set_title("Pitch angle (deg)", fontsize=9.5, fontweight="bold", loc="left")
    ax_pitch.grid(True, alpha=0.3)

    fig.suptitle("Flight Path", fontweight="bold", y=0.985, color=pal.title)
    return fig


def figure_mission_aero_coefficients(
    mission, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Four-panel angle of attack / CL / CD / L-over-D vs. time.

    Mirrors SUAVE's own ``plot_aerodynamic_coefficients``.
    """
    fig = _ensure_fig(fig, (10, 8))
    fig.set_layout_engine(None)
    fig.subplots_adjust(
        top=0.87, bottom=0.09, hspace=0.4, wspace=0.28, left=0.08, right=0.97
    )
    (ax_aoa, ax_cl), (ax_cd, ax_ld) = fig.subplots(2, 2)
    pal = get_palette(theme)
    _theme_figure(fig, pal)
    time_min = _mission_time_min(mission)
    cols = mission.columns

    # Left-aligned per-panel titles instead of rotated y-axis labels -- see
    # figure_mission_profile for why (a fixed-size rotated label can clip
    # off entirely at a narrow embedded GUI canvas width; a title degrades
    # gracefully instead).
    ax_aoa.plot(time_min, cols.get("AoA_deg", []), color="tab:blue")
    ax_aoa.set_title("Angle of attack (deg)", fontsize=9, fontweight="bold", loc="left")
    ax_aoa.grid(True, alpha=0.3)

    ax_cl.plot(time_min, cols.get("CL", []), color="tab:orange")
    ax_cl.set_title("CL", fontsize=9, fontweight="bold", loc="left")
    ax_cl.grid(True, alpha=0.3)

    ax_cd.plot(time_min, cols.get("CD", []), color="tab:green")
    ax_cd.set(xlabel="Time (min)")
    ax_cd.set_title("CD", fontsize=9, fontweight="bold", loc="left")
    ax_cd.grid(True, alpha=0.3)

    ax_ld.plot(time_min, cols.get("L_over_D", []), color="tab:purple")
    ax_ld.set(xlabel="Time (min)")
    ax_ld.set_title("L/D", fontsize=9, fontweight="bold", loc="left")
    ax_ld.grid(True, alpha=0.3)

    fig.suptitle(
        "Aerodynamic Coefficients", fontweight="bold", y=0.985, color=pal.title
    )
    return fig


def figure_mission_aero_forces(
    mission, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Four-panel throttle / lift / thrust / drag vs. time.

    Mirrors SUAVE's own ``plot_aerodynamic_forces``.
    """
    fig = _ensure_fig(fig, (10, 8))
    fig.set_layout_engine(None)
    fig.subplots_adjust(
        top=0.87, bottom=0.09, hspace=0.4, wspace=0.28, left=0.08, right=0.97
    )
    (ax_thr, ax_lift), (ax_thrust, ax_drag) = fig.subplots(2, 2)
    pal = get_palette(theme)
    _theme_figure(fig, pal)
    time_min = _mission_time_min(mission)
    cols = mission.columns

    # Left-aligned per-panel titles instead of rotated y-axis labels -- see
    # figure_mission_profile.
    ax_thr.plot(time_min, cols.get("Throttle", []), color="tab:blue")
    ax_thr.set_title("Throttle", fontsize=9, fontweight="bold", loc="left")
    ax_thr.grid(True, alpha=0.3)

    ax_lift.plot(
        time_min, np.asarray(cols.get("Lift_N", [])) / 1000.0, color="tab:orange"
    )
    ax_lift.set_title("Lift (kN)", fontsize=9, fontweight="bold", loc="left")
    ax_lift.grid(True, alpha=0.3)

    ax_thrust.plot(
        time_min, np.asarray(cols.get("Thrust_N", [])) / 1000.0, color="tab:green"
    )
    ax_thrust.set(xlabel="Time (min)")
    ax_thrust.set_title("Thrust (kN)", fontsize=9, fontweight="bold", loc="left")
    ax_thrust.grid(True, alpha=0.3)

    ax_drag.plot(time_min, np.asarray(cols.get("Drag_N", [])) / 1000.0, color="tab:red")
    ax_drag.set(xlabel="Time (min)")
    ax_drag.set_title("Drag (kN)", fontsize=9, fontweight="bold", loc="left")
    ax_drag.grid(True, alpha=0.3)

    fig.suptitle(
        "Aerodynamic & Propulsive Forces", fontweight="bold", y=0.985, color=pal.title
    )
    return fig


def figure_mission_drag_components(
    mission, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Parasite / induced / compressible / miscellaneous / total CD vs. time.

    Mirrors SUAVE's own ``plot_drag_components``.
    """
    fig = _ensure_fig(fig, (10, 6))
    ax = fig.subplots(1, 1)
    pal = get_palette(theme)
    _theme_figure(fig, pal)
    time_min = _mission_time_min(mission)
    cols = mission.columns

    ax.plot(
        time_min, cols.get("CD_parasite", []), label="CD parasite", color="tab:purple"
    )
    ax.plot(time_min, cols.get("CD_induced", []), label="CD induced", color="tab:blue")
    ax.plot(
        time_min,
        cols.get("CD_compressible", []),
        label="CD compressibility",
        color="tab:green",
    )
    ax.plot(
        time_min,
        cols.get("CD_miscellaneous", []),
        label="CD miscellaneous",
        color="tab:olive",
    )
    ax.plot(
        time_min,
        cols.get("CD_total", []),
        label="CD total",
        color="tab:red",
        linewidth=2,
    )
    ax.set(xlabel="Time (min)", ylabel="CD", title="Drag components")
    ax.legend(loc="upper center", fontsize=8, ncol=3)
    ax.grid(True, alpha=0.3)
    return fig


def figure_payload_range(
    report: AnalysisReport,
    config,
    fig: Optional[Figure] = None,
    theme: str | None = None,
) -> Figure:
    """Classic A-B-C-D Breguet payload-range diagram.

    Built from :func:`physics.performance.payload_range_diagram` (no SUAVE
    mission run required -- this renders even when mission analysis is
    disabled/unavailable). Label offsets are in figure-relative "offset
    points" (matplotlib's resolution-independent unit) with the side chosen
    from each point's relative position, not fixed data-unit nudges tuned
    for one aircraft's range/payload magnitude -- so the diagram stays
    legible for anything from a regional jet to an ultra-long-range widebody.
    """
    from ..physics.performance import payload_range_diagram

    result = payload_range_diagram(report, config)

    fig = _ensure_fig(fig, (10, 6.5))
    ax = fig.add_subplot(111)
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    ranges = [p.range_nm for p in result.points]
    payloads_t = [p.payload_kg / 1000.0 for p in result.points]
    max_range = max(ranges) if ranges else 1.0
    max_payload = max(payloads_t) if payloads_t else 1.0

    ax.plot(
        ranges,
        payloads_t,
        color="tab:blue",
        linewidth=2.5,
        marker="o",
        markersize=7,
        zorder=3,
    )
    ax.fill_between(ranges, payloads_t, color="tab:blue", alpha=0.12, zorder=1)

    # Points can numerically coincide (e.g. B == C when the wing-tank volume,
    # not MTOW, is the binding fuel constraint even at max payload) -- merge
    # their labels instead of stacking two overlapping annotations.
    i = 0
    while i < len(result.points):
        x, y = ranges[i], payloads_t[i]
        j = i
        tags = [result.points[i].label]
        while (
            j + 1 < len(result.points)
            and abs(ranges[j + 1] - x) < 1e-6
            and abs(payloads_t[j + 1] - y) < 1e-6
        ):
            j += 1
            tags.append(result.points[j].label)
        near_right_edge = x > 0.85 * max_range
        ax.annotate(
            f"{'/'.join(tags)}\n{x:,.0f} nm | {y:.1f} t",
            (x, y),
            xytext=(-8 if near_right_edge else 8, 10),
            textcoords="offset points",
            ha="right" if near_right_edge else "left",
            va="bottom",
            fontsize=8.5,
            fontweight="bold",
            color=pal.title,
        )
        i = j + 1

    ax.set_xlabel("Range (nm)")
    ax.set_ylabel("Payload (t)")
    ax.set_title("Payload-Range Diagram")
    ax.set_xlim(-0.03 * max_range, max_range * 1.15)
    ax.set_ylim(0, max_payload * 1.25 if max_payload > 0 else 1.0)
    ax.grid(True, alpha=0.3)

    fig.text(
        0.5,
        0.01,
        f"Fuel capacity: {result.fuel_capacity_kg:,.0f} kg (limited by {result.fuel_capacity_limit})   |   "
        f"OEW: {result.oew_kg:,.0f} kg   |   MTOW: {result.mtow_kg:,.0f} kg",
        ha="center",
        va="bottom",
        fontsize=8.5,
        color="gray",
    )
    return fig


def figure_mission_route_2d(
    route,
    mass_profile: Optional[np.ndarray] = None,
    altitude_profile: Optional[np.ndarray] = None,
    fig: Optional[Figure] = None,
    theme: str | None = None,
) -> Figure:
    """2D ground track over a textured Earth map.

    ``mass_profile``/``altitude_profile`` are optional, per-waypoint arrays
    (same length as ``route.waypoints``, e.g. from
    ``reporting.route_globe.sync_mass_to_route``): when given, the route is
    colored by total aircraft mass (matching the same "jet" colormap the
    former 3D globe view used) with a colorbar and a cruise-point annotation;
    otherwise it's drawn as a plain line (before a mission has been run).
    """
    import matplotlib.patheffects as pe

    fig = _ensure_fig(fig, (12, 6))
    ax = fig.add_subplot(111)
    pal = get_palette(theme)
    _theme_figure(fig, pal)
    # Without this, imshow's default aspect='equal' shrinks the Axes box to
    # preserve 1:1 lat/lon scaling whenever the canvas's actual on-screen
    # aspect ratio doesn't match the image's 2:1 (360x180) data extent --
    # leaving the map tiny in one corner with the legend (positioned via the
    # *un-shrunk* axes fraction) overlapping empty space instead of the map.
    ax.set_aspect("auto")

    # Resolved at call time rather than by a __file__-relative expression: the
    # texture is downloaded after installation, so it may live in the per-user
    # data directory instead of inside the package.
    from ..integration.assets import default_texture_path

    texture_path = default_texture_path()
    if texture_path.exists():
        import matplotlib.image as mpimg

        img = mpimg.imread(str(texture_path))
        # The blue marble image is equirectangular: -180 to 180 lon, -90 to 90 lat.
        ax.imshow(img, extent=[-180, 180, -90, 90], origin="upper", aspect="auto")
    else:
        # No texture available: fall back to the theme background rather than
        # a hardcoded near-black -- retires the old divergent "#0d0d0d" look.
        ax.set_facecolor(pal.bg)

    lats = np.array([wp.lat for wp in route.waypoints])
    lons = np.array([wp.lon for wp in route.waypoints])

    # Handle longitude wrapping if the route crosses the antimeridian
    lons_clean = []
    lats_clean = []
    for i in range(len(lons)):
        if i > 0 and abs(lons[i] - lons[i - 1]) > 180:
            lons_clean.append(np.nan)
            lats_clean.append(np.nan)
        lons_clean.append(lons[i])
        lats_clean.append(lats[i])
    lons_clean = np.array(lons_clean)
    lats_clean = np.array(lats_clean)

    if mass_profile is not None and len(mass_profile) == len(lons):
        from matplotlib.collections import LineCollection

        points = np.column_stack([lons_clean, lats_clean]).reshape(-1, 1, 2)
        segments = np.concatenate([points[:-1], points[1:]], axis=1)
        # Segment color = the mass at its starting waypoint; NaN segments
        # (antimeridian break) are simply invisible, matching the plain plot.
        seg_mass = np.asarray(mass_profile, dtype=float)[:-1]
        lc = LineCollection(segments, cmap="jet", linewidth=3.0)
        lc.set_array(seg_mass)
        ax.add_collection(lc)
        cbar = fig.colorbar(lc, ax=ax, fraction=0.035, pad=0.02)
        cbar.set_label("Total Mass (kg)")

        idx_mid = len(lons) // 2
        mid_text = (
            f"Cruise\nAlt: {altitude_profile[idx_mid]:,.0f} m"
            if altitude_profile is not None
            else "Cruise"
        )
        mid_text += f"\nMass: {mass_profile[idx_mid]:,.0f} kg"
        ax.plot(lons_clean[idx_mid], lats_clean[idx_mid], "ko", markersize=5, zorder=5)
        ax.annotate(
            mid_text,
            (lons_clean[idx_mid], lats_clean[idx_mid]),
            xytext=(8, 8),
            textcoords="offset points",
            fontsize=8,
            color="black",
            bbox=dict(boxstyle="round", fc="white", alpha=0.75, lw=0),
        )
        route_label = f"Route ({route.source})"
    else:
        ax.plot(lons_clean, lats_clean, color="#ff9900", linewidth=2.5)
        route_label = f"Route ({route.source})"

    if len(lons_clean):
        origin_ident = route.waypoints[0].ident or "DEP"
        dest_ident = route.waypoints[-1].ident or "ARR"
        ax.plot(
            lons_clean[0], lats_clean[0], "go", markersize=7, label="Origin", zorder=5
        )
        ax.plot(
            lons_clean[-1],
            lats_clean[-1],
            "ro",
            markersize=7,
            label="Destination",
            zorder=5,
        )
        for lon, lat, ident, va in (
            (lons_clean[0], lats_clean[0], origin_ident, "bottom"),
            (lons_clean[-1], lats_clean[-1], dest_ident, "top"),
        ):
            ax.annotate(
                ident,
                (lon, lat),
                xytext=(0, 6 if va == "bottom" else -6),
                textcoords="offset points",
                ha="center",
                va=va,
                fontsize=9,
                fontweight="bold",
                color="white",
                path_effects=[pe.withStroke(linewidth=2, foreground="black")],
            )

    ax.set_xlim(-180, 180)
    ax.set_ylim(-90, 90)
    ax.set_xlabel("Longitude [deg]")
    ax.set_ylabel("Latitude [deg]")
    ax.set_title(f"2D Ground Track — {route_label}")
    ax.legend(loc="lower left", fontsize=8, framealpha=0.6)

    # Use a subtle grid over the texture
    ax.grid(True, color="white", alpha=0.3, linestyle=":")
    # The mass colorbar above (if added) is a whole new Axes, added after
    # _theme_figure ran; re-running it (idempotent) is the simplest way to
    # theme it too.
    _theme_figure(fig, pal)
    return fig


# ---------------------------------------------------------------------------
# Propulsion Analysis (physics.propulsion on-design turbofan cycle)
# ---------------------------------------------------------------------------
def _propulsion_design_point(config):
    from ..physics.propulsion import TurbofanCycleInputs

    eng = config.geometry.engine
    req = config.requirements
    return TurbofanCycleInputs(
        mach=req.cruise_mach,
        altitude_m=req.cruise_altitude_m,
        bypass_ratio=eng.bypass_ratio,
        overall_pressure_ratio=eng.overall_pressure_ratio,
        fan_pressure_ratio=eng.fan_pressure_ratio,
        turbine_inlet_temperature_k=eng.turbine_inlet_temp_k,
    )


def _plt_cmap(name: str, n: int):
    """n evenly-spaced colours from a named matplotlib colormap."""
    import matplotlib.pyplot as plt

    return plt.get_cmap(name)(np.linspace(0.1, 0.9, max(2, n)))


def figure_propulsion_cycle_summary(
    report: "AnalysisReport",
    config,
    fig: Optional[Figure] = None,
    theme: str | None = None,
) -> Figure:
    """On-design cruise cycle: station stagnation temperatures (bar chart)
    plus a text summary of specific thrust, TSFC, efficiencies, and a
    dimensional cruise-thrust estimate anchored to the engine's rated static
    thrust (see physics.propulsion.anchor_mass_flow_kg_s).

    This is a fast, closed-form, generic-component-efficiency conceptual
    cycle model -- a different fidelity level from the SUAVE mission
    analysis's numerically-solved engine deck, so its computed TSFC will not
    exactly match EngineConfig.cruise_tsfc_kg_kgf_hr (the real/published
    reference value the payload-range diagram uses). Both start from the
    identical thrust/BPR/OPR/FPR/TIT design values and the same component
    efficiency assumptions as the SUAVE turbofan builder (see
    physics/propulsion.py's module docstring) -- only the solution method
    differs, the same relationship the Model Comparison tab already
    documents between AeroSandbox/SUAVE/MSES.
    """
    from ..physics.propulsion import compute_turbofan_cycle

    fig = _ensure_fig(fig, (11, 6))
    cyc_cfg = config.propulsion_cycle

    out = compute_turbofan_cycle(_propulsion_design_point(config), cyc_cfg)
    ax1, ax2 = fig.subplots(1, 2, gridspec_kw={"width_ratios": [1.3, 1.0]})
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    if not out.cycle_feasible:
        for ax in (ax1, ax2):
            ax.axis("off")
        fig.text(
            0.5,
            0.5,
            f"Cycle infeasible at this design point:\n{out.infeasibility_reason}",
            ha="center",
            va="center",
            fontsize=11,
            color="#c0392b",
            wrap=True,
        )
        return fig

    stations = [
        "T0\n(static)",
        "Tt2\n(fan/LPC\nface)",
        "Tt13\n(fan exit)",
        "Tt25\n(LPC exit)",
        "Tt3\n(HPC exit)",
        "Tt4\n(TIT)",
        "Tt45\n(HPT exit)",
        "Tt5\n(LPT exit)",
    ]
    atmo = asb.Atmosphere(altitude=config.requirements.cruise_altitude_m)
    t_static = float(atmo.temperature())
    temps = [
        t_static,
        out.temperature_t0_k,
        out.temperature_t13_k,
        out.temperature_t25_k,
        out.temperature_t3_k,
        out.temperature_t4_k,
        out.temperature_t45_k,
        out.temperature_t5_k,
    ]

    colors = [
        "#95a5a6",
        "#3498db",
        "#2ecc71",
        "#f1c40f",
        "#e67e22",
        "#e74c3c",
        "#9b59b6",
        "#8e44ad",
    ]
    x = np.arange(len(stations))
    bars = ax1.bar(x, temps, color=colors, edgecolor="black", linewidth=0.6, alpha=0.85)
    for bar, t in zip(bars, temps):
        ax1.text(
            bar.get_x() + bar.get_width() / 2,
            bar.get_height() + 20,
            f"{t:.0f} K",
            ha="center",
            va="bottom",
            fontsize=8,
            fontweight="bold",
            color=pal.title,
        )
    ax1.set_xticks(x)
    ax1.set_xticklabels(stations, fontsize=7.5)
    ax1.set_ylabel("Stagnation temperature [K]")
    ax1.set_title("On-Design Cycle Station Temperatures", fontweight="bold", loc="left")
    ax1.set_ylim(0, max(temps) * 1.2)
    ax1.grid(True, axis="y", alpha=0.3)

    ax2.axis("off")
    lines = _propulsion_cycle_summary_lines(config, out, verbose=True)
    ax2.text(
        0.02,
        0.98,
        "\n".join(lines),
        transform=ax2.transAxes,
        ha="left",
        va="top",
        fontsize=9.5,
        family="monospace",
        color=pal.title,
    )
    ax2.set_title("Cruise Design-Point Summary", fontweight="bold", loc="left")

    return fig


def _propulsion_cycle_summary_lines(config, out, verbose: bool = True) -> List[str]:
    """Text lines for the on-design cycle summary panel -- shared by
    ``figure_propulsion_cycle_summary`` (wide Results-tab/PNG-export layout)
    and ``figure_engine_designer_preview`` (compact Engine Designer tab
    layout) so the two can never silently drift into showing different
    numbers for the same design. ``verbose=False`` drops the fuel-air-ratio
    and per-efficiency-term breakout lines for a shorter, narrow-column fit.
    """
    from ..physics.propulsion import anchor_mass_flow_kg_s, classify_engine_by_bpr

    eng = config.geometry.engine
    cyc_cfg = config.propulsion_cycle

    mdot_total, _static = anchor_mass_flow_kg_s(
        eng.thrust_kn,
        eng.overall_pressure_ratio,
        eng.fan_pressure_ratio,
        eng.bypass_ratio,
        eng.turbine_inlet_temp_k,
        cyc_cfg,
    )
    cruise_thrust_kn = (
        (out.specific_thrust_ms * mdot_total / 1000.0)
        if mdot_total == mdot_total
        else float("nan")
    )
    n_eng = len(eng.spanwise_positions_m)
    cruise_line = (
        f"Per-engine thrust, this cruise pt : {cruise_thrust_kn:7.1f} kN"
        if cruise_thrust_kn == cruise_thrust_kn
        else "Per-engine thrust, this cruise pt : n/a"
    )
    total_line = (
        f"Total installed thrust (x{n_eng})       : {cruise_thrust_kn * n_eng:7.1f} kN"
        if cruise_thrust_kn == cruise_thrust_kn
        else ""
    )

    lines = [
        f"Engine: {eng.engine_name}  ({classify_engine_by_bpr(eng.bypass_ratio)})",
        f"Design point: M{config.requirements.cruise_mach:.2f} @ {config.requirements.cruise_altitude_m / 1000:.1f} km",
        "",
        f"BPR = {eng.bypass_ratio:.1f}    OPR = {eng.overall_pressure_ratio:.1f}    "
        f"FPR = {eng.fan_pressure_ratio:.2f}    TIT = {eng.turbine_inlet_temp_k:.0f} K",
        "",
        f"Specific thrust   SFn = {out.specific_thrust_ms:6.1f} m/s",
        f"TSFC (computed)        = {out.tsfc_mg_ns:6.2f} mg/(N.s)",
        f"TSFC (reference)       = {eng.cruise_tsfc_kg_kgf_hr / (9.81 * 3600) * 1e6:6.2f} mg/(N.s)",
    ]
    if verbose:
        lines.append(f"Fuel-air ratio    f    = {out.fuel_air_ratio:6.4f}")
    lines += [
        "",
        f"Thermal efficiency     eta_th = {out.thermal_efficiency:.3f}",
        f"Propulsive efficiency  eta_p  = {out.propulsive_efficiency:.3f}",
        f"Overall efficiency     eta_o  = {out.overall_efficiency:.3f}",
        "",
        f"Per-engine thrust, static (rated) : {eng.thrust_kn:7.1f} kN",
        cruise_line,
        total_line,
    ]
    return lines


def figure_engine_designer_preview(
    config, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Compact, VERTICALLY-stacked preview for the Engine Designer tab's
    narrow side panel.

    ``figure_propulsion_cycle_summary`` above is laid out as two subplots
    SIDE BY SIDE at (11, 6) inches -- fine embedded in the Results tab's wide
    chart column or exported as a standalone PNG, but the Engine Designer
    tab embeds its preview canvas in a narrow QSplitter side panel (a few
    hundred px wide), which squeezes that same wide layout down to a nearly
    square aspect and makes both subplots' titles and tick labels crowd/
    overlap into illegibility.

    Two stacked panels instead:
      1. A literal drawing of ``EngineConfig.nacelle_profile`` -- the raw
         (x-station, radius-fraction) control-point grid the Engine
         Parameters form lets you edit as bare numbers with no visual
         feedback. Plotting it as a mirrored top/bottom silhouette (with the
         actual control points marked) answers "what shape am I actually
         making?" directly, instead of requiring the user to mentally
         reconstruct a nacelle profile from six raw (x, r) pairs.
      2. The same on-design cycle text summary
         ``figure_propulsion_cycle_summary`` shows (via
         ``_propulsion_cycle_summary_lines``), so this preview and the
         Propulsion Analysis Results tab can never disagree -- just without
         that figure's station-temperature bar chart, whose 8 rotated
         category labels are what least tolerates being squeezed narrow.
    """
    from ..physics.propulsion import compute_turbofan_cycle

    fig = _ensure_fig(fig, (5.5, 8.5))
    eng = config.geometry.engine
    cyc_cfg = config.propulsion_cycle

    ax_nacelle, ax_text = fig.subplots(2, 1, gridspec_kw={"height_ratios": [1.0, 1.35]})
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    profile = eng.nacelle_profile
    if profile:
        xs = np.array([p[0] for p in profile], dtype=float)
        r_frac = np.array([p[1] for p in profile], dtype=float)
        rs = r_frac * eng.radius_scale_m
        ax_nacelle.fill_between(xs, rs, -rs, color="tab:blue", alpha=0.15)
        ax_nacelle.plot(xs, rs, "o-", color="tab:blue", markersize=4, linewidth=1.5)
        ax_nacelle.plot(xs, -rs, "o-", color="tab:blue", markersize=4, linewidth=1.5)
        ax_nacelle.axhline(0.0, color="#888888", linewidth=0.6, linestyle="--")
        for xv, rv, fv in zip(xs, rs, r_frac):
            ax_nacelle.annotate(
                f"({xv:.1f}, {fv:.2f})",
                (xv, rv),
                textcoords="offset points",
                xytext=(0, 4),
                fontsize=6.5,
                ha="center",
                color=pal.title,
            )
        ax_nacelle.set_aspect("equal")
        ax_nacelle.set_xlabel("x-station [m]", fontsize=8)
        ax_nacelle.set_ylabel("radius [m]", fontsize=8)
        ax_nacelle.tick_params(labelsize=7.5)
        ax_nacelle.grid(True, alpha=0.3)
    else:
        ax_nacelle.axis("off")
        ax_nacelle.text(
            0.5,
            0.5,
            "No nacelle profile defined",
            ha="center",
            va="center",
            fontsize=9,
            color=pal.title,
        )
    ax_nacelle.set_title(
        "Nacelle profile silhouette\n(labels = editable (x-station, radius-fraction) points)",
        fontsize=8.5,
        fontweight="bold",
        loc="left",
    )

    ax_text.axis("off")
    out = compute_turbofan_cycle(_propulsion_design_point(config), cyc_cfg)
    if not out.cycle_feasible:
        ax_text.text(
            0.02,
            0.95,
            f"Cycle infeasible at this design point:\n{out.infeasibility_reason}",
            transform=ax_text.transAxes,
            ha="left",
            va="top",
            fontsize=8.5,
            color="#c0392b",
            wrap=True,
        )
        return fig

    lines = _propulsion_cycle_summary_lines(config, out, verbose=False)
    ax_text.text(
        0.0,
        0.98,
        "\n".join(lines),
        transform=ax_text.transAxes,
        ha="left",
        va="top",
        fontsize=7.8,
        family="monospace",
        color=pal.title,
    )
    ax_text.set_title(
        "Cruise Design-Point Summary", fontsize=8.5, fontweight="bold", loc="left"
    )

    return fig


def figure_propulsion_carpet_plot(
    report: "AnalysisReport",
    config,
    fig: Optional[Figure] = None,
    theme: str | None = None,
) -> Figure:
    """Carpet plot: specific thrust vs TSFC as overall pressure ratio and
    turbine inlet temperature are swept around the current engine's design
    point (BPR/FPR and flight condition held fixed at their current values)."""
    from ..physics.propulsion import compute_carpet_plot, compute_turbofan_cycle

    fig = _ensure_fig(fig, (10, 7))
    ax = fig.add_subplot(111)
    pal = get_palette(theme)
    _theme_figure(fig, pal)
    eng = config.geometry.engine
    cyc_cfg = config.propulsion_cycle
    req = config.requirements

    pi_c_vec = np.linspace(15.0, 70.0, 12)
    tit_vec = np.linspace(1300.0, 2000.0, 8)
    carpet = compute_carpet_plot(
        pi_c_vec,
        tit_vec,
        req.cruise_mach,
        req.cruise_altitude_m,
        eng.fan_pressure_ratio,
        eng.bypass_ratio,
        cyc_cfg,
    )

    colors_tit = _plt_cmap("plasma", len(tit_vec))
    for i, tit in enumerate(tit_vec):
        mask = carpet.feasible_mask[i, :]
        if mask.sum() < 2:
            continue
        ax.plot(
            carpet.specific_thrust_ms[i, mask],
            carpet.tsfc_mg_ns[i, mask],
            color=colors_tit[i],
            linewidth=1.4,
            label=f"T4t = {tit:.0f} K" if i % 2 == 0 else "_",
        )

    colors_pic = _plt_cmap("viridis", len(pi_c_vec))
    for j, pic in enumerate(pi_c_vec):
        mask = carpet.feasible_mask[:, j]
        if mask.sum() < 2:
            continue
        ax.plot(
            carpet.specific_thrust_ms[mask, j],
            carpet.tsfc_mg_ns[mask, j],
            color=colors_pic[j],
            linewidth=0.9,
            linestyle="--",
            alpha=0.6,
            label=f"OPR = {pic:.0f}" if j % 3 == 0 else "_",
        )

    design_out = compute_turbofan_cycle(_propulsion_design_point(config), cyc_cfg)
    if design_out.cycle_feasible:
        ax.plot(
            design_out.specific_thrust_ms,
            design_out.tsfc_mg_ns,
            "*",
            color=pal.title,
            markersize=16,
            markeredgecolor="black",
            markeredgewidth=0.6,
            zorder=10,
            label=f"{eng.engine_name} design point\n"
            f"(SFn={design_out.specific_thrust_ms:.0f} m/s, TSFC={design_out.tsfc_mg_ns:.1f} mg/Ns)",
        )

    ax.set_xlabel("Specific thrust  SFn = F / mdot_total  [m/s]")
    ax.set_ylabel("TSFC  [mg/(N.s)]")
    ax.set_title(
        f"On-Design Carpet Plot -- OPR x TIT sweep (BPR={eng.bypass_ratio:.1f}, FPR={eng.fan_pressure_ratio:.2f})",
        fontweight="bold",
        loc="left",
    )
    ax.legend(loc="upper right", fontsize=7.5, ncol=2, framealpha=0.9)
    ax.grid(True, alpha=0.3)
    return fig


def figure_propulsion_efficiency_decomposition(
    report: "AnalysisReport",
    config,
    fig: Optional[Figure] = None,
    theme: str | None = None,
) -> Figure:
    """Thermal / propulsive / overall efficiency vs overall pressure ratio,
    at the current engine's BPR/FPR/TIT and flight condition."""
    from ..physics.propulsion import compute_efficiency_decomposition

    fig = _ensure_fig(fig, (10, 6))
    ax = fig.add_subplot(111)
    pal = get_palette(theme)
    _theme_figure(fig, pal)
    eng = config.geometry.engine
    cyc_cfg = config.propulsion_cycle
    req = config.requirements

    pi_c_vec = np.linspace(15.0, 70.0, 60)
    dec = compute_efficiency_decomposition(
        pi_c_vec,
        eng.turbine_inlet_temp_k,
        eng.bypass_ratio,
        eng.fan_pressure_ratio,
        req.cruise_mach,
        req.cruise_altitude_m,
        cyc_cfg,
    )
    mask = dec.feasible_mask
    ax.plot(
        dec.compressor_pressure_ratio_vector[mask],
        dec.thermal_efficiency[mask],
        color="#27ae60",
        linewidth=2.0,
        label=r"$\eta_{th}$ (thermal)",
    )
    ax.plot(
        dec.compressor_pressure_ratio_vector[mask],
        dec.propulsive_efficiency[mask],
        color="#2980b9",
        linewidth=2.0,
        label=r"$\eta_p$ (propulsive)",
    )
    ax.plot(
        dec.compressor_pressure_ratio_vector[mask],
        dec.overall_efficiency[mask],
        color="#e67e22",
        linewidth=2.2,
        linestyle="--",
        label=r"$\eta_o = \eta_{th}\cdot\eta_p$ (overall)",
    )

    if eng.overall_pressure_ratio == eng.overall_pressure_ratio:
        ax.axvline(
            eng.overall_pressure_ratio,
            color=pal.title,
            linestyle=":",
            linewidth=1.3,
            label=f"{eng.engine_name} OPR = {eng.overall_pressure_ratio:.0f}",
        )

    ax.set_xlabel("Overall (core) pressure ratio  OPR  [-]")
    ax.set_ylabel("Efficiency  [-]")
    ax.set_title(
        f"Efficiency Decomposition vs OPR (BPR={eng.bypass_ratio:.1f}, TIT={eng.turbine_inlet_temp_k:.0f} K)",
        fontweight="bold",
        loc="left",
    )
    ax.set_ylim(0, 1.0)
    ax.legend(loc="center right", fontsize=9)
    ax.grid(True, alpha=0.3)
    return fig


def figure_propulsion_bpr_sensitivity(
    report: "AnalysisReport",
    config,
    fig: Optional[Figure] = None,
    theme: str | None = None,
) -> Figure:
    """Specific thrust and TSFC vs bypass ratio, at the current engine's
    OPR/FPR/TIT and flight condition."""
    from ..physics.propulsion import compute_bpr_sensitivity, compute_turbofan_cycle

    fig = _ensure_fig(fig, (10, 6))
    ax1 = fig.add_subplot(111)
    ax2 = ax1.twinx()
    pal = get_palette(theme)
    _theme_figure(fig, pal)
    eng = config.geometry.engine
    cyc_cfg = config.propulsion_cycle
    req = config.requirements

    bpr_lo, bpr_hi = max(1.0, eng.bypass_ratio * 0.3), eng.bypass_ratio * 1.8 + 1.0
    bpr_vec = np.linspace(bpr_lo, bpr_hi, 40)
    sens = compute_bpr_sensitivity(
        bpr_vec,
        eng.overall_pressure_ratio,
        eng.turbine_inlet_temp_k,
        eng.fan_pressure_ratio,
        req.cruise_mach,
        req.cruise_altitude_m,
        cyc_cfg,
    )
    mask = sens.feasible_mask

    color_sfn, color_tsfc = "#2980b9", "#c0392b"
    (l1,) = ax1.plot(
        sens.bypass_ratio_vector[mask],
        sens.specific_thrust_ms[mask],
        color=color_sfn,
        linewidth=2.0,
        label="SFn [m/s]",
    )
    (l2,) = ax2.plot(
        sens.bypass_ratio_vector[mask],
        sens.tsfc_mg_ns[mask],
        color=color_tsfc,
        linewidth=2.0,
        linestyle="--",
        label="TSFC [mg/(N.s)]",
    )

    design_out = compute_turbofan_cycle(_propulsion_design_point(config), cyc_cfg)
    if design_out.cycle_feasible:
        ax1.plot(
            eng.bypass_ratio,
            design_out.specific_thrust_ms,
            "o",
            color=color_sfn,
            markersize=9,
            zorder=10,
        )
        ax2.plot(
            eng.bypass_ratio,
            design_out.tsfc_mg_ns,
            "s",
            color=color_tsfc,
            markersize=9,
            zorder=10,
        )
        ax1.axvline(eng.bypass_ratio, color="gray", linestyle=":", linewidth=1.3)

    ax1.set_xlabel("Bypass ratio  BPR  [-]")
    ax1.set_ylabel("Specific thrust  SFn  [m/s]", color=color_sfn)
    ax2.set_ylabel("TSFC  [mg/(N.s)]", color=color_tsfc)
    ax1.tick_params(axis="y", labelcolor=color_sfn)
    ax2.tick_params(axis="y", labelcolor=color_tsfc)
    ax1.legend(
        [l1, l2], [l1.get_label(), l2.get_label()], loc="center right", fontsize=9
    )
    ax1.set_title(
        f"Bypass-Ratio Sensitivity (OPR={eng.overall_pressure_ratio:.0f}, TIT={eng.turbine_inlet_temp_k:.0f} K)",
        fontweight="bold",
        loc="left",
    )
    ax1.grid(True, alpha=0.3)
    return fig


def figure_propulsion_altitude_sweep(
    report: "AnalysisReport",
    config,
    fig: Optional[Figure] = None,
    theme: str | None = None,
) -> Figure:
    """Per-engine thrust and TSFC as contours over the full (altitude, Mach)
    flight envelope, holding the cycle design parameters fixed -- the
    closed-form, first-principles stand-in for a semi-empirical installed-
    thrust-lapse table (see physics/propulsion.py's module docstring).

    Contour style (filled levels + labelled black iso-lines) matches
    figure_airfoil_reynolds's NeuralFoil Cl/Cd/L-D/Cm-vs-(Re, alpha) panels,
    so every "how does X vary over a 2-variable operating envelope" figure in
    the app reads consistently.
    """
    import matplotlib.ticker as mticker
    from ..physics.propulsion import compute_altitude_sweep, anchor_mass_flow_kg_s

    fig = _ensure_fig(fig, (12, 6))
    ax1, ax2 = fig.subplots(1, 2)
    pal = get_palette(theme)
    _theme_figure(fig, pal)
    eng = config.geometry.engine
    cyc_cfg = config.propulsion_cycle
    req = config.requirements

    mach_vec = np.linspace(0.0, max(0.9, req.cruise_mach * 1.15), 26)
    alt_vec = np.linspace(0.0, 13000.0, 26)
    mdot_total, _static = anchor_mass_flow_kg_s(
        eng.thrust_kn,
        eng.overall_pressure_ratio,
        eng.fan_pressure_ratio,
        eng.bypass_ratio,
        eng.turbine_inlet_temp_k,
        cyc_cfg,
    )
    sweep = compute_altitude_sweep(
        alt_vec,
        list(mach_vec),
        eng.bypass_ratio,
        eng.overall_pressure_ratio,
        eng.fan_pressure_ratio,
        eng.turbine_inlet_temp_k,
        mdot_total,
        cyc_cfg,
    )

    alt_km = sweep.altitude_m / 1000.0
    ALT, MACH = np.meshgrid(alt_km, mach_vec)
    thrust = np.where(sweep.feasible_mask, sweep.dimensional_thrust_kn, np.nan)
    tsfc = np.where(sweep.feasible_mask, sweep.tsfc_mg_ns, np.nan)

    def plot_contour(ax, Z, title, label, cmap, mark_cruise=True):
        cnt = ax.contourf(ALT, MACH, Z, levels=20, cmap=cmap)
        cb = fig.colorbar(cnt, ax=ax, label=label, ticks=mticker.MaxNLocator(nbins=6))
        cb.ax.tick_params(labelsize=8)
        lines = ax.contour(
            ALT, MACH, Z, levels=6, colors="black", linewidths=0.5, alpha=0.5
        )
        ax.clabel(lines, inline=True, fontsize=8, fmt="%.3g", inline_spacing=12)
        if mark_cruise:
            ax.plot(
                req.cruise_altitude_m / 1000.0,
                req.cruise_mach,
                "w*",
                markersize=14,
                markeredgecolor="black",
                markeredgewidth=0.8,
                zorder=10,
                label=f"Cruise (M{req.cruise_mach:.2f} @ {req.cruise_altitude_m / 1000:.1f} km)",
            )
            ax.legend(fontsize=7.5, loc="lower right", framealpha=0.85)
        ax.set_title(title, fontweight="bold", loc="left")
        ax.set_xlabel("Altitude [km]")
        ax.set_ylabel("Mach number  $M_0$  [-]")
        ax.grid(True, alpha=0.3)

    plot_contour(
        ax1,
        thrust,
        "Per-Engine Thrust vs Altitude & Mach\n(anchored to rated static thrust)",
        "Thrust [kN]",
        "inferno",
    )
    plot_contour(ax2, tsfc, "TSFC vs Altitude & Mach", "TSFC [mg/(N.s)]", "viridis")

    # Both colorbars above are new Axes, added after _theme_figure ran;
    # re-running it (idempotent) is the simplest way to theme them too.
    _theme_figure(fig, pal)
    return fig


# --- Structural Analysis (wingbox FEM) figures ------------------------------
# Downstream/informational only (see physics.structural_sizing's module
# docstring): these read a pipeline `structural_result` (duck-typed, not
# imported, to avoid a circular import with pipeline.py -- the same pattern
# figure_model_comparison already uses for mission_result/mses_result) and
# degrade to figure_status_message when unavailable, matching the MSES/
# mission-analysis tabs' own graceful-degradation convention.


def _structures_unavailable_message(structural_result) -> tuple[bool, str]:
    if structural_result is None:
        return (
            False,
            "Structural analysis was not run for this design (Advanced Settings -> Structural Analysis).",
        )
    if structural_result.status != "ok":
        return (
            False,
            f"Structural analysis failed: {structural_result.error or 'unknown error'}",
        )
    return True, ""


def _true_spar_xy(wsg, y_stations: np.ndarray, frac: float) -> np.ndarray:
    """Chordwise (X) position of the spar at ``frac`` chord fraction, at each
    of ``y_stations``, using the *same* rib-perpendicular reference-line
    geometry :func:`alas.geometry.wing_mesh_bdf.build_wing_mesh_bdf`
    actually meshes (``wsg.rib_vector``/``get_rib_lengths``/
    ``compute_spar_intersections``) -- NOT a naive ``x_le + frac*local_chord``
    straight-percent-chord line.

    That naive line looks plausible but silently diverges from the real
    spar by up to ~2 m near the root on a real widebody design (confirmed
    directly): every spar station except the root is anchored along the
    rib direction *perpendicular to the local leading edge*, while the root
    rib is a special streamwise cut (see ``WingStructureGeometry``'s own
    docstring) -- a plain chord-fraction formula ignores that root/rest-of-
    span convention split entirely, which reads as a spurious sharp kink
    right at the root in a plan-view plot even though the actual FEM mesh
    has no such defect there.

    A partial-span spar (``WingStructureGeometry.spar_full_span[i] =
    False``, e.g. the optional center spar) returns NaN past its own
    break-station endpoint -- matplotlib simply stops drawing the line
    there, the same "this spar doesn't exist here" convention
    ``compute_spar_intersections`` itself uses (``None``).

    Root-adjacent "transition" ribs are also truncated short of a spar's
    nominal chordwise reach (``get_rib_lengths``' ``l_actual < l_nominal``,
    see this module's own docstring) -- ``get_rib_stations`` drops any spar
    node past its own rib's ``frac_actual`` for exactly this reason. Doing
    the same NaN-past-``l_actual`` check here is what keeps this preview's
    near-root spar line from drawing points the real mesh never places,
    which would otherwise read as a spurious
    "twist" right at the root that doesn't appear in the actual FEM/BDF.
    """
    frac_idx = (
        wsg.spar_fracs.index(frac)
        if frac in wsg.spar_fracs
        else int(np.argmin(np.abs(np.array(wsg.spar_fracs) - frac)))
    )
    x = np.zeros_like(y_stations, dtype=float)
    for i, y in enumerate(y_stations):
        eta = float(y) / wsg.semi_span
        x_le_val = wsg.x_le(eta)
        aft_x, aft_y = wsg.rib_vector(eta)
        l_nominal, l_actual = wsg.get_rib_lengths(float(y), x_le_val, aft_x, aft_y)
        s_spars = wsg.compute_spar_intersections(
            float(y), x_le_val, aft_x, aft_y, l_nominal
        )
        s = s_spars[frac_idx]
        if s is not None and s <= l_actual + 1e-9:
            x[i] = x_le_val + s * aft_x
        else:
            x[i] = np.nan
    return x


def figure_structures_designer_preview(
    wsg, sizing, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Narrow, vertically-stacked live preview for the Structural Analysis
    Advanced Settings tab -- same relationship as
    ``figure_engine_designer_preview`` vs. ``figure_propulsion_cycle_summary``:
    the Results tab's wide ``figure_structures_sizing`` is squeezed and
    illegible in this tab's narrow QSplitter column, so this is a distinct,
    cheap (sizing-only, no FEM mesh, no NASTRAN) figure built directly from
    ``WingStructureGeometry``/``WingboxSizing`` for a live per-keystroke
    preview."""
    fig = _ensure_fig(fig, (5, 8))
    ax_plan = fig.add_subplot(2, 1, 1)
    ax_text = fig.add_subplot(2, 1, 2)
    pal = get_palette(theme)
    _theme_figure(fig, pal)
    ax_text.axis("off")

    y = sizing.y_stations
    le = np.array([wsg.x_le(e) for e in sizing.eta_stations])
    te = le + sizing.chord
    ax_plan.plot(y, le, color=pal.title, lw=1.5)
    ax_plan.plot(y, te, color=pal.title, lw=1.5)
    ax_plan.plot([y[0], y[0]], [le[0], te[0]], color=pal.title, lw=1.5)
    ax_plan.plot([y[-1], y[-1]], [le[-1], te[-1]], color=pal.title, lw=1.5)
    colors = plt_cm_tab10()
    for i, spar in enumerate(sizing.spars):
        spar_x = _true_spar_xy(wsg, y, spar.chord_fraction)
        ax_plan.plot(
            y,
            spar_x,
            color=colors[i % len(colors)],
            lw=2.0,
            label=f"x/c={spar.chord_fraction:.2f}",
        )
    ax_plan.invert_yaxis()
    ax_plan.set_xlabel("Y [m]", fontsize=9)
    ax_plan.set_ylabel("X [m]", fontsize=9)
    ax_plan.set_title("Wingbox planform", fontsize=11, fontweight="bold")
    ax_plan.legend(fontsize=7, loc="upper right")
    ax_plan.set_aspect("equal")
    ax_plan.tick_params(labelsize=8)
    ax_plan.grid(True, alpha=0.3)

    lines = [
        f"Governing load case: {sizing.sizing_load_case}",
        f"Ribs: {sizing.num_ribs}  (spacing {sizing.rib_spacing_m:.2f} m)",
        f"Skin: {sizing.t_skin * 1000:.1f} mm",
        "",
    ]
    for s in sizing.spars:
        lines.append(
            f"Spar x/c={s.chord_fraction:.2f}:  cap {s.t_cap[0] * 1000:.1f}mm x "
            f"{s.w_cap[0] * 1000:.0f}mm,  web {s.t_web * 1000:.1f}mm"
        )
    lines += ["", f"Semi-wing mass: {sizing.total_mass_kg:,.0f} kg"]
    for comp, m_val in sizing.mass_breakdown_kg.items():
        lines.append(f"  {comp}: {m_val:,.0f} kg")
    ax_text.text(
        0.0,
        1.0,
        "\n".join(lines),
        transform=ax_text.transAxes,
        ha="left",
        va="top",
        fontsize=9,
        family="monospace",
        color=pal.title,
    )
    return fig


def figure_structures_sizing(
    structural_result, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Wingbox planform (spar lines + rib stations), mass breakdown, and a
    FEM-vs-Torenbeek wing mass comparison -- a read-only accuracy check
    (physics.mass's own Torenbeek estimate for this same design), not a
    feedback loop."""
    ok, msg = _structures_unavailable_message(structural_result)
    if not ok:
        return figure_status_message(
            "Structural Analysis", msg, ok=False, fig=fig, theme=theme
        )

    sizing = structural_result.sizing
    wsg = structural_result.wsg
    fig = _ensure_fig(fig, (13, 5))
    ax_plan = fig.add_subplot(1, 3, 1)
    ax_pie = fig.add_subplot(1, 3, 2)
    ax_bar = fig.add_subplot(1, 3, 3)
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    y = sizing.y_stations
    le = np.array([wsg.x_le(e) for e in sizing.eta_stations])
    te = le + sizing.chord
    ax_plan.plot(y, le, color=pal.title, lw=1.5)
    ax_plan.plot(y, te, color=pal.title, lw=1.5)
    ax_plan.plot([y[0], y[0]], [le[0], te[0]], color=pal.title, lw=1.5)
    ax_plan.plot([y[-1], y[-1]], [le[-1], te[-1]], color=pal.title, lw=1.5)
    spar_colors = plt_cm_tab10()
    for i, spar in enumerate(sizing.spars):
        spar_x = _true_spar_xy(wsg, y, spar.chord_fraction)
        ax_plan.plot(
            y,
            spar_x,
            color=spar_colors[i % len(spar_colors)],
            lw=2.2,
            label=f"Spar x/c={spar.chord_fraction:.2f}",
        )
    rib_ys = np.linspace(0, y[-1], sizing.num_ribs)
    for ry in rib_ys:
        le_r = wsg.x_le(ry / wsg.semi_span)
        te_r = le_r + wsg.local_chord(ry / wsg.semi_span)
        ax_plan.plot([ry, ry], [le_r, te_r], color="tab:gray", lw=0.4, alpha=0.6)
    ax_plan.invert_yaxis()
    ax_plan.set_xlabel("Spanwise position Y [m]")
    ax_plan.set_ylabel("Chordwise position X [m]")
    ax_plan.set_title(f"Wingbox planform  ({sizing.num_ribs} ribs)", fontweight="bold")
    ax_plan.legend(fontsize=8, loc="upper right")
    ax_plan.set_aspect("equal")
    ax_plan.grid(True, alpha=0.3)

    labels = list(sizing.mass_breakdown_kg.keys())
    values = list(sizing.mass_breakdown_kg.values())
    colors_pie = ["tab:blue", "tab:orange", "tab:green", "tab:red", "tab:purple"]
    # `textprops` must carry an explicit colour: without one, pie labels fall
    # back to Matplotlib's default black, which is unreadable against the dark
    # theme's background (the wedge labels -- "Ribs", "Spar caps", "Skin" --
    # were effectively invisible). The percentages sit *inside* the saturated
    # tab: wedges, so they get white regardless of theme, where the labels
    # outside follow the theme's own text colour.
    _wedges, _texts, autotexts = ax_pie.pie(
        values,
        labels=labels,
        autopct="%1.0f%%",
        startangle=120,
        colors=colors_pie[: len(labels)],
        textprops={"fontsize": 9, "color": pal.title},
    )
    for autotext in autotexts:
        autotext.set_color("white")
    ax_pie.set_title(
        f"Semi-wing structural mass\n{sizing.total_mass_kg:,.0f} kg", fontweight="bold"
    )

    fem_full_wing = 2.0 * sizing.total_mass_kg
    torenbeek = structural_result.torenbeek_wing_mass_kg
    cats = ["FEM wingbox\n(both wings)", "Torenbeek\nestimate"]
    vals = [fem_full_wing, torenbeek]
    bars = ax_bar.bar(cats, vals, color=["tab:blue", "tab:red"], width=0.5, alpha=0.85)
    for bar, val in zip(bars, vals):
        ax_bar.text(
            bar.get_x() + bar.get_width() / 2,
            val,
            f"{val:,.0f} kg",
            ha="center",
            va="bottom",
            fontsize=9,
            fontweight="bold",
            color=pal.title,
        )
    if np.isfinite(torenbeek) and torenbeek > 0:
        err_pct = (fem_full_wing - torenbeek) / torenbeek * 100.0
        ax_bar.set_title(
            f"FEM vs Torenbeek wing mass\nΔ = {err_pct:+.0f}%", fontweight="bold"
        )
    else:
        ax_bar.set_title("FEM vs Torenbeek wing mass", fontweight="bold")
    ax_bar.set_ylabel("Mass [kg]")
    ax_bar.grid(True, axis="y", alpha=0.3)

    fig.suptitle(
        f"Wingbox Sizing -- governing load case: {sizing.sizing_load_case}",
        fontweight="bold",
        color=pal.title,
    )
    return fig


def plt_cm_tab10() -> List[str]:
    return ["tab:blue", "tab:orange", "tab:green", "tab:red", "tab:purple", "tab:brown"]


def figure_structures_loads(
    structural_result, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Bending moment/EI distribution for the sizing-governing load case
    (left) and the spanwise deflection curve for every load case, with real
    NASTRAN tip-deflection markers overlaid when available (right)."""
    ok, msg = _structures_unavailable_message(structural_result)
    if not ok:
        return figure_status_message(
            "Structural Analysis -- Loads", msg, ok=False, fig=fig, theme=theme
        )

    analysis = structural_result.analysis
    sizing = structural_result.sizing
    fig = _ensure_fig(fig, (12, 5.5))
    ax_l = fig.add_subplot(1, 2, 1)
    ax_r = fig.add_subplot(1, 2, 2)
    # Created up-front (before any plotting/styling) so the theme block below
    # -- which iterates every Axes in fig.axes -- sees it too; twinx() shares
    # ax_l's x-axis and has no rendering side effect of its own, so creating
    # it earlier than its first .plot() call changes nothing about the figure.
    ax_l2 = ax_l.twinx()
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    governing = analysis.load_cases[sizing.sizing_load_case]
    ax_l.plot(
        analysis.y, analysis.ei_nm2 / 1e9, color="tab:blue", lw=2.0, label="EI(y)"
    )
    ax_l.fill_between(
        analysis.y, 0, analysis.ei_nm2 / 1e9, alpha=0.12, color="tab:blue"
    )
    ax_l.set_xlabel("Spanwise position Y [m]")
    ax_l.set_ylabel("Bending stiffness EI [GN.m^2]", color="tab:blue")
    ax_l.tick_params(axis="y", labelcolor="tab:blue")
    ax_l.grid(True, alpha=0.3)
    ax_l2.plot(
        governing.y,
        governing.moment_nm / 1e6,
        color="tab:orange",
        lw=1.8,
        ls="--",
        label=f"M(y), {sizing.sizing_load_case}",
    )
    ax_l2.set_ylabel("Bending moment M [MN.m]", color="tab:orange")
    ax_l2.tick_params(axis="y", labelcolor="tab:orange")
    ax_l.set_title("Stiffness & moment distribution", fontweight="bold")
    lines1, labs1 = ax_l.get_legend_handles_labels()
    lines2, labs2 = ax_l2.get_legend_handles_labels()
    ax_l.legend(lines1 + lines2, labs1 + labs2, fontsize=8, loc="upper right")

    colors = {"pull-up": "tab:red", "push-down": "tab:blue", "level": "tab:green"}
    for name, lc in analysis.load_cases.items():
        ax_r.plot(
            lc.y,
            lc.deflection_m,
            color=colors.get(name, "tab:gray"),
            lw=2.0,
            label=f"{name} (n={lc.load_factor:+.2f}): tip={lc.tip_deflection_m:+.2f} m",
        )

    nastran = structural_result.nastran
    if nastran is not None and nastran.static.status == "ok":
        for name, tip_defl in nastran.static.tip_deflection_m.items():
            color = colors.get(name, "tab:gray")
            ax_r.scatter(
                [analysis.y[-1]],
                [tip_defl],
                color=color,
                marker="D",
                s=60,
                edgecolor="black",
                zorder=5,
                label=f"{name} NASTRAN tip: {tip_defl:+.2f} m",
            )

    ax_r.axhline(0, color="k", lw=0.7, alpha=0.4)
    ax_r.set_xlabel("Spanwise position Y [m]")
    ax_r.set_ylabel("Deflection δ [m]")
    ax_r.set_title(
        "Spanwise deflection (analytical, Euler-Bernoulli)", fontweight="bold"
    )
    ax_r.legend(fontsize=7.5, loc="best")
    ax_r.grid(True, alpha=0.3)

    return fig


def figure_structures_stress(
    structural_result, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Spanwise margin-of-safety per spar cap, one subplot per spar, all
    three load cases overlaid -- MS >= 0 required everywhere for a valid
    design (MS = 0 exactly at the root for the sizing-governing case, by
    construction)."""
    ok, msg = _structures_unavailable_message(structural_result)
    if not ok:
        return figure_status_message(
            "Structural Analysis -- Stress", msg, ok=False, fig=fig, theme=theme
        )

    analysis = structural_result.analysis
    n_spars = len(structural_result.sizing.spars)
    fig = _ensure_fig(fig, (4.5 * n_spars, 5))
    colors = {"pull-up": "tab:red", "push-down": "tab:blue", "level": "tab:green"}

    for i in range(n_spars):
        ax = fig.add_subplot(1, n_spars, i + 1)
        for name, lc in analysis.load_cases.items():
            ss = lc.spar_stress[i]
            ms_plot = np.clip(ss.margin_of_safety, -1.0, 5.0)
            ax.plot(
                lc.y, ms_plot, color=colors.get(name, "tab:gray"), lw=2.0, label=name
            )
        ax.axhline(0, color="k", lw=1.2, ls="--", label="MS = 0")
        ax.set_xlabel("Spanwise position Y [m]")
        if i == 0:
            ax.set_ylabel("Margin of safety [-]")
        ax.set_title(
            f"Spar x/c={structural_result.sizing.spar_fracs[i]:.2f}", fontweight="bold"
        )
        ax.legend(fontsize=7.5)
        ax.grid(True, alpha=0.3)

    pal = get_palette(theme)
    _theme_figure(fig, pal)

    fig.suptitle(
        "Spar Cap Margin of Safety (analytical)", fontweight="bold", color=pal.title
    )
    return fig


def _nearest_freq_index(f_r: float, nastran_freqs: List[float]) -> Optional[int]:
    """Index into ``nastran_freqs`` of the frequency closest to ``f_r``.

    Direct port of ``05_validation.py``'s ``_best_nastran``: the Rayleigh
    quotient is a trial-shape estimate, not an exact bound, so it can land
    above OR below any given real mode -- and with ``cfg.n_modes`` real
    NASTRAN modes almost always including torsional/local-panel modes the
    Rayleigh table has no equivalent for, pairing by raw list position
    (mode 1 vs mode 1, etc.) routinely compares unrelated modes. Nearest-
    neighbor-by-frequency finds the physically corresponding mode instead.
    """
    if not nastran_freqs:
        return None
    arr = np.asarray(nastran_freqs)
    return int(np.argmin(np.abs(arr - f_r)))


def figure_structures_modes(
    structural_result, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Rayleigh-quotient natural frequency estimate (bar, + real NASTRAN
    SOL 103 bars if available, nearest-frequency-matched per mode -- see
    :func:`_nearest_freq_index`), with the Rayleigh and NASTRAN mode shapes
    in their own separate side-by-side panels (each mode's color matches
    across all three panels for comparison) -- overlaying both shape sets
    on one axes was tried first but reads as clutter once NASTRAN's own
    (noisier, less idealized) shapes are added on top of the smooth
    Rayleigh trial curves."""
    ok, msg = _structures_unavailable_message(structural_result)
    if not ok:
        return figure_status_message(
            "Structural Analysis -- Modes", msg, ok=False, fig=fig, theme=theme
        )

    modal = structural_result.analysis.modal
    y = structural_result.analysis.y
    fig = _ensure_fig(fig, (16, 5.5))
    ax_freq = fig.add_subplot(1, 3, 1)
    ax_ray = fig.add_subplot(1, 3, 2)
    ax_nas = fig.add_subplot(1, 3, 3)
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    n = len(modal.frequencies_hz)
    x = np.arange(n)
    nastran_modes = (
        structural_result.nastran.modes if structural_result.nastran else None
    )
    has_nastran = (
        nastran_modes is not None
        and nastran_modes.status == "ok"
        and nastran_modes.frequencies_hz
    )
    matched_idx: List[Optional[int]] = (
        [
            _nearest_freq_index(f_r, nastran_modes.frequencies_hz)
            for f_r in modal.frequencies_hz
        ]
        if has_nastran
        else [None] * n
    )

    w = 0.32 if has_nastran else 0.5
    ax_freq.bar(
        x - (w / 2 if has_nastran else 0),
        modal.frequencies_hz,
        w,
        color="tab:blue",
        alpha=0.88,
        label="Rayleigh (analytical)",
    )
    if has_nastran:
        nastran_f = [
            nastran_modes.frequencies_hz[j] if j is not None else 0.0
            for j in matched_idx
        ]
        ax_freq.bar(
            x + w / 2,
            nastran_f,
            w,
            color="tab:red",
            alpha=0.88,
            label="NASTRAN SOL 103 (nearest-frequency match)",
        )
    ax_freq.set_xticks(x)
    ax_freq.set_xticklabels([f"Mode {i + 1}" for i in range(n)])
    ax_freq.set_ylabel("Frequency [Hz]")
    ax_freq.set_title("Natural frequencies (bending)", fontweight="bold")
    ax_freq.legend(fontsize=8)
    ax_freq.grid(True, axis="y", alpha=0.3)

    colors = plt_cm_tab10()
    has_shapes = (
        has_nastran
        and nastran_modes.mode_shapes
        and nastran_modes.mode_shape_y_m is not None
    )
    for i, (phi, f_hz) in enumerate(zip(modal.mode_shapes, modal.frequencies_hz)):
        color = colors[i % len(colors)]
        ax_ray.plot(
            y, phi, color=color, lw=2.0, ls="--", label=f"Mode {i + 1}: {f_hz:.2f} Hz"
        )
        j = matched_idx[i]
        if has_shapes and j is not None and j < len(nastran_modes.mode_shapes):
            ax_nas.plot(
                nastran_modes.mode_shape_y_m,
                nastran_modes.mode_shapes[j],
                color=color,
                lw=2.0,
                label=f"Mode {i + 1}: {nastran_modes.frequencies_hz[j]:.2f} Hz",
            )

    for ax, title in (
        (ax_ray, "Rayleigh trial mode shapes"),
        (
            ax_nas,
            "NASTRAN mode shapes"
            if has_shapes
            else "NASTRAN mode shapes (unavailable)",
        ),
    ):
        ax.axhline(0, color="k", lw=0.7, alpha=0.4)
        ax.set_xlabel("Spanwise position Y [m]")
        ax.set_ylabel("Normalized mode shape")
        ax.set_title(title, fontweight="bold")
        ax.legend(fontsize=8)
        ax.grid(True, alpha=0.3)

    return fig


def figure_structures_vibration(
    structural_result, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Sine-sweep FRF and random-vibration RMS -- NASTRAN-only (Method D,
    the Miles-equation RMS check, needs a real frequency-response solve as
    input and has no analytical fallback; see physics.structural_analysis's
    module docstring)."""
    ok, msg = _structures_unavailable_message(structural_result)
    if not ok:
        return figure_status_message(
            "Structural Analysis -- Vibration", msg, ok=False, fig=fig, theme=theme
        )

    vib = structural_result.nastran.vibration if structural_result.nastran else None
    if vib is None or vib.status != "ok":
        error = (
            vib.error if vib is not None else None
        ) or "NASTRAN was not run for this design."
        return figure_status_message(
            "Structural Analysis -- Vibration",
            "Requires a real NASTRAN sine/random-vibration solve (no analytical fallback exists for this "
            f"check): {error}",
            ok=False,
            fig=fig,
            theme=theme,
        )

    fig = _ensure_fig(fig, (12, 5.5))
    ax_l = fig.add_subplot(1, 2, 1)
    ax_r = fig.add_subplot(1, 2, 2)
    pal = get_palette(theme)
    _theme_figure(fig, pal)

    if vib.frf_freq_hz is not None:
        ax_l.semilogy(
            vib.frf_freq_hz,
            vib.frf_tip_abs_m_per_n + 1e-20,
            color="tab:blue",
            lw=2.0,
            label="|H(f)| at tip",
        )
        if vib.peak_freq_hz:
            ax_l.axvline(
                vib.peak_freq_hz,
                color="tab:red",
                ls="--",
                lw=1.5,
                label=f"Peak: {vib.peak_freq_hz:.2f} Hz",
            )
    ax_l.set_xlabel("Frequency [Hz]")
    ax_l.set_ylabel("|H(f)| [m/N]")
    ax_l.set_title("Sine-sweep frequency response (tip)", fontweight="bold")
    ax_l.legend(fontsize=8)
    ax_l.grid(True, which="both", alpha=0.3)

    common = [lbl for lbl in vib.miles_rms_m if lbl in vib.nastran_rms_m]
    if common:
        x = np.arange(len(common))
        w = 0.32
        ax_r.bar(
            x - w / 2,
            [vib.miles_rms_m[lbl] for lbl in common],
            w,
            color="tab:blue",
            alpha=0.88,
            label="Miles equation",
        )
        ax_r.bar(
            x + w / 2,
            [vib.nastran_rms_m[lbl] for lbl in common],
            w,
            color="tab:red",
            alpha=0.88,
            label="NASTRAN random",
        )
        ax_r.set_yscale("log")
        ax_r.set_xticks(x)
        ax_r.set_xticklabels(common)
        ax_r.set_ylabel("RMS displacement [m]")
        ax_r.set_title("Random-vibration RMS check", fontweight="bold")
        ax_r.legend(fontsize=8)
        ax_r.grid(True, axis="y", alpha=0.3)
    else:
        ax_r.text(
            0.5,
            0.5,
            "Random-vibration RMS data\nnot available",
            ha="center",
            va="center",
            transform=ax_r.transAxes,
            fontsize=12,
            color="gray",
        )

    return fig


def figure_structures_patran(
    structural_result, fig: Optional[Figure] = None, theme: str | None = None
) -> Figure:
    """Displays the Patran deformation-plot PNGs (see
    :mod:`alas.integration.patran_runner`), one panel per load case --
    raw images via ``imshow``, not a live plot rebuilt from this process's
    own data, since these come from an external Patran subprocess render."""
    ok, msg = _structures_unavailable_message(structural_result)
    if not ok:
        return figure_status_message(
            "Structural Analysis -- Patran Renders", msg, ok=False, fig=fig, theme=theme
        )

    patran = structural_result.patran
    if patran is None or patran.status == "not_run":
        return figure_status_message(
            "Structural Analysis -- Patran Renders",
            "Not run for this design (Advanced Settings -> Structural Analysis -> Render Patran deformation "
            "plots). Requires a working Patran install and a successful NASTRAN SOL 101 solve.",
            ok=False,
            fig=fig,
            theme=theme,
        )
    if not patran.png_paths:
        return figure_status_message(
            "Structural Analysis -- Patran Renders",
            f"Patran export failed: {patran.error or 'unknown error'}",
            ok=False,
            fig=fig,
            theme=theme,
        )

    import matplotlib.image as mpimg

    names = list(patran.png_paths.keys())
    n = len(names)
    fig = _ensure_fig(fig, (5.0 * n, 5.0))
    for i, name in enumerate(names):
        ax = fig.add_subplot(1, n, i + 1)
        img = mpimg.imread(str(patran.png_paths[name]))
        ax.imshow(img)
        ax.set_title(name, fontweight="bold")
        ax.axis("off")
    pal = get_palette(theme)
    _theme_figure(fig, pal)
    if patran.error:
        fig.suptitle(
            f"Some load cases failed to render: {patran.error}",
            color="#c0392b",
            fontsize=9,
        )
    return fig
