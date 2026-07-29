# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Figures for the Airfoil Screening tab.

Consistent with the rest of the app's charts (server-rendered themed Matplotlib
SVGs, see ``sidecar/figures.py`` and ``reporting/theme.py``), but built from an
:class:`~alas.analysis.airfoil_screening.AirfoilScreeningResult` instead of
a ``PipelineResult``. Each factory is ``fn(result, theme) -> Figure | None`` --
``None`` means "no data for this figure" (e.g. the 2-D->3-D re-rank chart when
Stage-2 refinement was disabled), which the HTTP layer turns into an empty slot.

The through-line of these four figures is the story the two-stage screen tells:
a fast 2-D proxy nominates candidates, then re-simulating them as this design's
actual 3-D wing (real MTOW/span/chord, real cruise Mach/altitude) reshuffles the
ranking -- which is exactly what stops a 2-D-flattering low-Reynolds section from
being recommended for a transonic transport it would never suit.
"""

from __future__ import annotations

from typing import Callable, Dict, List, Optional

import numpy as np
from matplotlib.figure import Figure

from .theme import get_palette

# Registry the sidecar route iterates. Keys are the figure names in the URL
# (GET /airfoil-sweep/{id}/figures/{name}); order is display order.
SWEEP_FIGURES: Dict[str, Callable[[object, Optional[str]], Optional[Figure]]] = {}


def _register(name: str):
    def deco(fn):
        SWEEP_FIGURES[name] = fn
        return fn

    return deco


def _new_fig(theme: Optional[str], figsize=(7.2, 4.6)):
    """A themed Figure + Axes, styled like the app's other charts (no pyplot,
    so no global-state leak) -- returns ``(fig, ax, palette)``."""
    pal = get_palette(theme)
    fig = Figure(figsize=figsize)
    fig.patch.set_facecolor(pal.bg)
    ax = fig.add_subplot(111)
    ax.set_facecolor(pal.bg)
    for spine in ax.spines.values():
        spine.set_edgecolor(pal.spine)
    ax.tick_params(colors=pal.tick)
    ax.xaxis.label.set_color(pal.tick)
    ax.yaxis.label.set_color(pal.tick)
    ax.title.set_color(pal.title)
    ax.grid(True, color=pal.spine, alpha=0.25, linewidth=0.6)
    return fig, ax, pal


def _ok(result) -> List:
    """Candidates that survived screening, best-rank first (result order)."""
    return [
        c for c in getattr(result, "candidates", []) if getattr(c, "status", "") == "ok"
    ]


def _refined(result) -> List:
    return [c for c in _ok(result) if getattr(c, "refined", False)]


def _mses_verified(result) -> List:
    return [c for c in _ok(result) if getattr(c, "mses_verified", False)]


# Distinct marker for a real, wind-tunnel-validated reference section
# (airfoil_screening.REFERENCE_AIRFOILS) across every chart in this module --
# a gold star reads as "this one is known to actually work" at a glance,
# distinct from the algorithmic candidates' plain dots/bars.
_REFERENCE_MARKER_COLOR = "#f5c518"


def _mark_references(ax, cands, xs, ys, pal):
    """Overlay a gold star + label on every ``is_reference`` candidate in
    ``cands`` (already plotted as regular points by the caller) -- the
    "known real section" anchor called out on every relevant chart, not just
    the dedicated MSES-verification one."""
    ref_pts = [
        (x, y, c.name)
        for c, x, y in zip(cands, xs, ys)
        if getattr(c, "is_reference", False)
    ]
    if not ref_pts:
        return
    ax.scatter(
        [p[0] for p in ref_pts],
        [p[1] for p in ref_pts],
        marker="*",
        s=220,
        color=_REFERENCE_MARKER_COLOR,
        edgecolors=pal.spine,
        linewidths=0.6,
        zorder=6,
        label="Real reference section",
    )
    for x, y, name in ref_pts:
        ax.annotate(
            name,
            (x, y),
            fontsize=7,
            color=_REFERENCE_MARKER_COLOR,
            fontweight="bold",
            xytext=(5, -4),
            textcoords="offset points",
            zorder=7,
        )


def _legend(ax, pal):
    leg = ax.legend(
        facecolor=pal.panel, edgecolor=pal.border, labelcolor=pal.tick, fontsize=8
    )
    if leg is not None:
        leg.get_frame().set_alpha(0.9)
    return leg


@_register("trade_map")
def fig_trade_map(result, theme=None) -> Optional[Figure]:
    """Trade map: cruise L/D vs resulting wing fuel-tank capacity, coloured by
    section thickness. Uses the 3-D L/D where the candidate was refined, else
    the 2-D proxy. The two design levers the ranking blends are the axes, so a
    user can see the Pareto front directly and why the top pick sits where it
    does (a fat high-fuel section vs a slick low-drag one)."""
    cands = _refined(result) or _ok(result)
    if not cands:
        return None
    use_3d = bool(_refined(result))
    xs = [(c.l_over_d_3d if use_3d else c.l_over_d) for c in cands]
    ys = [c.tank_capacity_kg for c in cands]
    tc = [(c.max_thickness_frac or 0.0) * 100 for c in cands]
    baseline = getattr(result, "baseline_airfoil", "")

    fig, ax, pal = _new_fig(theme)
    sc = ax.scatter(
        xs,
        ys,
        c=tc,
        cmap="viridis",
        s=70,
        edgecolors=pal.spine,
        linewidths=0.5,
        zorder=3,
    )
    cbar = fig.colorbar(sc, ax=ax)
    cbar.set_label("Section t/c (%)", color=pal.tick)
    cbar.ax.tick_params(colors=pal.tick)
    cbar.outline.set_edgecolor(pal.spine)

    # Annotate the top pick and the current section.
    top = cands[0]
    ax.annotate(
        f"  {top.name} (best)",
        (xs[0], ys[0]),
        color=pal.title,
        fontsize=9,
        fontweight="bold",
        zorder=4,
    )
    for c, x, y in zip(cands, xs, ys):
        if c.name == baseline:
            ax.scatter(
                [x],
                [y],
                s=160,
                facecolors="none",
                edgecolors=pal.accent,
                linewidths=2.0,
                zorder=5,
            )
            ax.annotate(
                f"  {c.name} (current)", (x, y), color=pal.accent, fontsize=9, zorder=6
            )
            break
    _mark_references(ax, cands, xs, ys, pal)

    ax.set_xlabel(f"Cruise L/D ({'3-D wing' if use_3d else '2-D proxy'})")
    ax.set_ylabel("Wing fuel-tank capacity (kg)")
    ax.set_title("Trade map — L/D vs fuel capacity")
    _legend(ax, pal)
    return fig


@_register("rerank_2d_3d")
def fig_rerank_2d_3d(result, theme=None) -> Optional[Figure]:
    """2-D proxy L/D vs 3-D-wing L/D for the refined shortlist, against a y=x
    reference. Points far below the diagonal are exactly the sections the 2-D
    screen over-rated: they lose most of their apparent advantage once induced
    and wave drag on this actual planform are counted. Makes the whole reason
    for Stage 2 legible at a glance. ``None`` when no candidate was refined."""
    cands = _refined(result)
    if not cands:
        return None
    xs = [c.l_over_d for c in cands]
    ys = [c.l_over_d_3d for c in cands]
    baseline = getattr(result, "baseline_airfoil", "")

    fig, ax, pal = _new_fig(theme)
    lo = 0.0
    hi = max(max(xs), max(ys)) * 1.05
    ax.plot(
        [lo, hi],
        [lo, hi],
        color=pal.spine,
        linestyle="--",
        linewidth=1.0,
        label="2-D = 3-D",
        zorder=2,
    )
    ax.scatter(
        xs, ys, s=60, color=pal.accent, edgecolors=pal.spine, linewidths=0.5, zorder=3
    )

    # Label only a legible subset -- labelling every point (all of
    # refine_top_n, which can be 20+) crammed illegible overlapping text in
    # practice. Always label the baseline and every real reference section
    # (both handled by _mark_references below); beyond that, only the two
    # candidates whose 2-D->3-D gap is largest -- exactly the ones this chart
    # exists to call out (the 2-D proxy over-rated them most).
    gaps = sorted(range(len(cands)), key=lambda i: xs[i] - ys[i], reverse=True)
    highlight_idx = set(gaps[:2])
    for i, (c, x, y) in enumerate(zip(cands, xs, ys)):
        if (
            i not in highlight_idx
            or c.name == baseline
            or getattr(c, "is_reference", False)
        ):
            continue
        ax.annotate(
            c.name,
            (x, y),
            fontsize=7,
            color=pal.tick,
            xytext=(4, 2),
            textcoords="offset points",
            zorder=4,
        )
    for c, x, y in zip(cands, xs, ys):
        if c.name == baseline:
            ax.annotate(
                f"{c.name} (current)",
                (x, y),
                fontsize=8,
                color=pal.accent,
                fontweight="bold",
                xytext=(4, 2),
                textcoords="offset points",
                zorder=5,
            )
            break
    _mark_references(ax, cands, xs, ys, pal)

    ax.set_xlim(lo, hi)
    ax.set_ylim(lo, hi)
    ax.set_xlabel("2-D proxy L/D (isolated section)")
    ax.set_ylabel("3-D wing L/D (this design, cruise)")
    ax.set_title("How the 2-D shortlist reshuffles in 3-D")
    _legend(ax, pal)
    return fig


@_register("ranking_bars")
def fig_ranking_bars(result, theme=None) -> Optional[Figure]:
    """Top candidates ranked by final cruise L/D (3-D where available), current
    section highlighted. The plain 'what should I pick' read-out."""
    cands = (_refined(result) or _ok(result))[:12]
    if not cands:
        return None
    use_3d = bool(_refined(result))
    names = [c.name for c in cands]
    vals = [(c.l_over_d_3d if use_3d else c.l_over_d) for c in cands]
    baseline = getattr(result, "baseline_airfoil", "")

    fig, ax, pal = _new_fig(theme, figsize=(7.2, max(3.2, 0.42 * len(cands) + 1.0)))
    colors = [
        pal.accent
        if n == baseline
        else _REFERENCE_MARKER_COLOR
        if c.is_reference
        else "#7aa2ff"
        for c, n in zip(cands, names)
    ]
    y = list(range(len(cands)))[::-1]  # best at top
    ax.barh(y, vals, color=colors, edgecolor=pal.spine, linewidth=0.5, zorder=3)
    ax.set_yticks(y)
    labels = [
        f"{n} (current)" if n == baseline else f"{n} ★" if c.is_reference else n
        for c, n in zip(cands, names)
    ]
    ax.set_yticklabels(labels, fontsize=8, color=pal.tick)
    for yi, v in zip(y, vals):
        ax.annotate(
            f"{v:.1f}",
            (v, yi),
            xytext=(4, 0),
            textcoords="offset points",
            va="center",
            fontsize=8,
            color=pal.tick,
        )
    ax.set_xlabel(f"Cruise L/D ({'3-D wing' if use_3d else '2-D proxy'})")
    ax.set_title("Top airfoils for this design  (★ = real reference section)")
    return fig


@_register("section_shapes")
def fig_section_shapes(result, theme=None) -> Optional[Figure]:
    """Overlaid section geometries of the top few picks and the current
    section -- the shape context behind the numbers (is the optimizer reaching
    for a thin low-Reynolds sliver, or a sensible transport section?)."""
    cands = _refined(result) or _ok(result)
    if not cands:
        return None
    baseline = getattr(result, "baseline_airfoil", "")
    names: List[str] = []
    for c in cands[:4]:
        if c.name not in names:
            names.append(c.name)
    if baseline and baseline not in names:
        names.append(baseline)

    try:
        from ..geometry.airfoils import AirfoilLibrary
    except Exception:
        return None

    fig, ax, pal = _new_fig(theme, figsize=(7.2, 3.8))
    # Overlaid thin outlines on a dark panel were reported as hard to read, so
    # separation now comes from three cues rather than hue alone: a
    # high-contrast, colour-blind-safe palette (Okabe-Ito derived), a distinct
    # dash pattern per series, and vertical offsets so the sections are stacked
    # instead of superimposed on one another. The baseline keeps the accent
    # colour, a heavier weight and a solid line so it stays the visual anchor.
    series = ["#4f8cff", "#ff8a5c", "#43c59e", "#f5c518", "#c792ea"]
    dashes = [(None, None), (6, 2), (2, 1.6), (7, 2, 1.5, 2), (4, 1.5, 1, 1.5)]
    drew = False
    plotted = 0
    # Small vertical stagger so overlapping cambers stay distinguishable; the
    # y-axis is relabelled to say so, since y/c is no longer read literally.
    offset_step = 0.16
    for i, name in enumerate(names):
        try:
            coords = AirfoilLibrary.get(name).coordinates
        except Exception:
            continue
        if coords is None or len(coords) == 0:
            continue
        is_base = name == baseline
        dash = (None, None) if is_base else dashes[plotted % len(dashes)]
        offset = -offset_step * plotted
        ax.plot(
            coords[:, 0],
            coords[:, 1] + offset,
            color=(pal.accent if is_base else series[plotted % len(series)]),
            linewidth=(2.4 if is_base else 1.8),
            dashes=dash if dash != (None, None) else (None, None),
            solid_capstyle="round",
            label=f"{name} (current)" if is_base else name,
            zorder=3,
        )
        # Faint chord line per section: gives the eye a baseline to judge
        # camber against once the sections are vertically staggered.
        ax.axhline(offset, color=pal.spine, linewidth=0.5, alpha=0.35, zorder=1)
        drew = True
        plotted += 1
    if not drew:
        return None
    ax.set_aspect("equal", adjustable="datalim")
    ax.set_xlabel("x/c")
    ax.set_ylabel("y/c  (sections offset vertically)")
    ax.set_title("Section shapes — top picks vs current")
    _legend(ax, pal)
    return fig


@_register("mses_verification")
def fig_mses_verification(result, theme=None) -> Optional[Figure]:
    """Stage-3 MSES verification: VLM(+Korn) Stage-2 L/D vs the real MSES
    L/D per verified candidate, with wave drag (CDw -- the shock/transonic-
    mismatch indicator) annotated above each MSES bar. This is the only figure
    in this module that can show whether a candidate is actually handling this
    cruise Mach well: neither the 2-D NeuralFoil proxy nor VLM+Korn model
    shocks, so a supercritical section's real advantage (or a conventional
    section's real wave-drag penalty) only shows up here. ``None`` if no
    candidate was MSES-verified (Stage 3 off, MSES not configured, or every
    attempt failed to converge/bracket the target CL)."""
    cands = _mses_verified(result)
    if not cands:
        return None
    baseline = getattr(result, "baseline_airfoil", "")
    names = [c.name for c in cands]
    ld_vlm = [c.l_over_d_3d for c in cands]
    ld_mses = [c.l_over_d_mses for c in cands]
    cdw = [c.cdw_mses for c in cands]

    fig, ax, pal = _new_fig(theme, figsize=(7.2, max(3.2, 0.6 * len(cands) + 1.2)))
    y = np.arange(len(cands))
    height = 0.35
    ax.barh(
        y + height / 2,
        ld_vlm,
        height=height,
        color="#7aa2ff",
        edgecolor=pal.spine,
        linewidth=0.5,
        label="VLM + Korn (Stage 2)",
        zorder=3,
    )
    ax.barh(
        y - height / 2,
        ld_mses,
        height=height,
        color="#ff8a5c",
        edgecolor=pal.spine,
        linewidth=0.5,
        label="MSES (Stage 3, real shock/viscous)",
        zorder=3,
    )
    for yi, ld, cdw_i in zip(y, ld_mses, cdw):
        ax.annotate(
            f"CDw={cdw_i * 1e4:.0f} cts",
            (ld, yi - height / 2),
            xytext=(4, 0),
            textcoords="offset points",
            va="center",
            fontsize=7,
            color=pal.tick,
        )
    ax.set_yticks(list(y))
    ax.set_yticklabels(
        [f"{n} (current)" if n == baseline else n for n in names],
        fontsize=8,
        color=pal.tick,
    )
    ax.set_xlabel("Cruise L/D")
    ax.set_title(
        "MSES verification — real shock/viscous effects vs the VLM+Korn estimate"
    )
    _legend(ax, pal)
    return fig
