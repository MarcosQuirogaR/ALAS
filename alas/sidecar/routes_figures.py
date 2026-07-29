# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Figures over the bridge (SVG), plus the field-performance numeric panel.

* ``GET  /pipeline/{run_id}/figures/{name}`` -- a RESULT_FIGURES factory from a
  completed run, as SVG (vector, scales crisply to any size).
* ``POST /preview/{name}`` -- a PREVIEW_FIGURES factory from a posted config +
  design vector, no pipeline run. Accepts an optional ``view`` (elev/azim/zoom)
  so the 3-D previews can be rotated/zoomed by the client.
* ``GET  /pipeline/{run_id}/field-performance`` -- V-speeds + field distances
  for the departure/arrival airports (the LTO widget's text panel).

Matplotlib isn't thread-safe and Starlette runs sync endpoints in a threadpool,
so concurrent figure requests (a Results tab mounts ~10 at once) could corrupt
global state -- the intermittent "Failed to fetch". A module-level lock
serializes rendering to make it reliable.
"""

from __future__ import annotations

import io
import threading
from collections import OrderedDict
from typing import Optional

from fastapi import APIRouter, HTTPException, Query, Response
from pydantic import BaseModel

from ..config.design_variables import DesignVector
from ..config.settings import ALASConfig
from . import figures, figures_extra, runs

router = APIRouter()

# Serialize all Matplotlib rendering (build + savefig): Matplotlib keeps global
# state and is not safe to drive from multiple threadpool workers at once.
_RENDER_LOCK = threading.Lock()

# Rendered result-figure SVGs, keyed (run_id, figure_name, theme). A finished
# run's figures are immutable, so re-requests (theme flip and back, remounted
# Results tab, expand/collapse churn) can skip the matplotlib render entirely
# -- meaningful because _RENDER_LOCK serializes all rendering, so every
# avoided render also stops queueing behind whatever else is drawing.
# Previews are deliberately NOT cached: their key space (full config + design
# vector) is unbounded and they change on every form edit.
# LRU with a small cap; stale run_ids age out naturally on eviction.
_SVG_CACHE: "OrderedDict[tuple, bytes]" = OrderedDict()
_SVG_CACHE_CAP = 96
_SVG_CACHE_LOCK = threading.Lock()


def _svg_cache_get(key: tuple) -> Optional[bytes]:
    with _SVG_CACHE_LOCK:
        data = _SVG_CACHE.get(key)
        if data is not None:
            _SVG_CACHE.move_to_end(key)
        return data


def _svg_cache_put(key: tuple, data: bytes) -> None:
    with _SVG_CACHE_LOCK:
        _SVG_CACHE[key] = data
        _SVG_CACHE.move_to_end(key)
        while len(_SVG_CACHE) > _SVG_CACHE_CAP:
            _SVG_CACHE.popitem(last=False)


def _translate_figure(fig) -> None:
    """Translate every visible text element of an already-built figure.

    Done here, at the one place every figure passes through on its way out,
    rather than wrapping ~160 individual ``set_title``/``set_xlabel``/
    ``set_ylabel`` calls scattered across the figure factories: the factories
    keep writing plain English, and nothing about adding a new chart has to
    remember i18n. Strings with no catalog entry pass through unchanged (see
    alas/i18n), so this can never blank out a label.

    A no-op in English, so the default path costs one language check.
    """
    from ..i18n import get_language, t

    if get_language() == "en":
        return

    def _retitle(setter, getter):
        try:
            current = getter()
            if current:
                setter(t(current))
        except Exception:
            pass

    try:
        suptitle = getattr(fig, "_suptitle", None)
        if suptitle is not None and suptitle.get_text():
            suptitle.set_text(t(suptitle.get_text()))
    except Exception:
        pass

    for ax in getattr(fig, "axes", []):
        _retitle(ax.set_title, ax.get_title)
        _retitle(ax.set_xlabel, ax.get_xlabel)
        _retitle(ax.set_ylabel, ax.get_ylabel)
        # 3-D axes only.
        if hasattr(ax, "set_zlabel"):
            _retitle(ax.set_zlabel, ax.get_zlabel)
        try:
            legend = ax.get_legend()
            if legend is not None:
                for text in legend.get_texts():
                    if text.get_text():
                        text.set_text(t(text.get_text()))
        except Exception:
            pass
        # Free-standing annotations/text (status messages, callouts).
        try:
            for text in list(getattr(ax, "texts", [])):
                if text.get_text():
                    text.set_text(t(text.get_text()))
        except Exception:
            pass


def _render_svg(fig, tight: bool = True) -> bytes:
    import matplotlib.pyplot as plt

    _translate_figure(fig)
    buf = io.BytesIO()
    try:
        fig.savefig(
            buf, format="svg", bbox_inches="tight" if tight else None, transparent=True
        )
    finally:
        # Always deregister from pyplot's global figure manager, even when
        # savefig raises -- otherwise every failed render pins a Figure (and
        # its canvas) in process memory for the sidecar's whole lifetime.
        try:
            plt.close(fig)
        except Exception:
            pass
    return buf.getvalue()


def _svg_response(fig, tight: bool = True) -> Response:
    return Response(content=_render_svg(fig, tight), media_type="image/svg+xml")


def _close_all_figures() -> None:
    """Recovery for a builder that raised mid-construction: whatever
    partially-built figures it registered with pyplot would otherwise leak.
    Safe to close everything -- _RENDER_LOCK means no other thread has a
    figure in flight."""
    import matplotlib.pyplot as plt

    try:
        plt.close("all")
    except Exception:
        pass


@router.get("/figures")
def list_figures() -> dict:
    return {
        "result": sorted(figures.RESULT_FIGURES.keys()),
        "preview": sorted(figures.PREVIEW_FIGURES.keys()),
    }


def _get_ready_run(run_id: str):
    """Look up a run and confirm it's finished (not running/errored),
    returning its ``PipelineResult``. Shared by the single-figure endpoint
    and the export-all endpoints below so they fail the same way (404 for an
    unknown id, 409 for one still running or that errored out)."""
    state = runs.REGISTRY.get(run_id)
    if state is None:
        raise HTTPException(status_code=404, detail=f"Unknown run id '{run_id}'")
    if state.status == "running":
        raise HTTPException(status_code=409, detail=f"Run '{run_id}' is still running")
    if state.status == "error":
        raise HTTPException(
            status_code=409, detail=f"Run '{run_id}' failed: {state.error}"
        )
    return state.result


@router.get("/pipeline/{run_id}/figures/{name}")
def get_figure(
    run_id: str, name: str, theme: Optional[str] = Query(default=None)
) -> Response:
    result = _get_ready_run(run_id)

    builder = figures.RESULT_FIGURES.get(name)
    if builder is None:
        raise HTTPException(status_code=404, detail=f"Unknown figure '{name}'")

    cache_key = (run_id, name, theme)
    cached = _svg_cache_get(cache_key)
    if cached is not None:
        return Response(content=cached, media_type="image/svg+xml")

    with _RENDER_LOCK:
        try:
            fig = builder(result, theme)
        except Exception as exc:
            _close_all_figures()
            raise HTTPException(
                status_code=404, detail=f"Figure '{name}' unavailable: {exc}"
            )
        if fig is None:
            raise HTTPException(
                status_code=404, detail=f"Figure '{name}' has no data for this run"
            )
        svg = _render_svg(fig)

    _svg_cache_put(cache_key, svg)
    return Response(content=svg, media_type="image/svg+xml")


def _render_png(fig, dpi: int = 200) -> bytes:
    """Same contract as ``_render_svg``, rasterized instead of vector -- for
    the export-all-figures ZIP, where PNG (not SVG) is what a user pastes
    into a slide deck or emails around."""
    import matplotlib.pyplot as plt

    buf = io.BytesIO()
    try:
        fig.savefig(
            buf,
            format="png",
            dpi=dpi,
            bbox_inches="tight",
            facecolor=fig.get_facecolor(),
        )
    finally:
        try:
            plt.close(fig)
        except Exception:
            pass
    return buf.getvalue()


def _iter_result_figures(result, theme: Optional[str]):
    """Yield ``(name, Figure)`` for every RESULT_FIGURES entry that has data
    for this run, in registry order (the same grouping the Results screen's
    tabs use, since figures.py was written in that order). Must be called
    with ``_RENDER_LOCK`` held. A factory that raises is skipped (same
    per-figure degradation as the single-figure endpoint) rather than
    failing the whole export over one bad figure."""
    for name, builder in figures.RESULT_FIGURES.items():
        try:
            fig = builder(result, theme)
        except Exception:
            _close_all_figures()
            continue
        if fig is not None:
            yield name, fig


@router.get("/pipeline/{run_id}/export/figures.zip")
def export_figures_zip(
    run_id: str, theme: Optional[str] = Query(default=None)
) -> Response:
    """Every available figure for this run as PNG, bundled into one ZIP --
    the "export all figures" button on the Results screen. Figures with no
    data for this run (mission plots on a no-mission run, etc.) are simply
    omitted, same graceful degradation as viewing them one at a time."""
    import zipfile

    result = _get_ready_run(run_id)

    buf = io.BytesIO()
    with _RENDER_LOCK:
        with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as zf:
            for name, fig in _iter_result_figures(result, theme):
                zf.writestr(f"{name}.png", _render_png(fig))

    return Response(
        content=buf.getvalue(),
        media_type="application/zip",
        headers={
            "Content-Disposition": f'attachment; filename="alas-{run_id}-figures.zip"'
        },
    )


def _build_summary_page(result, theme: Optional[str]):
    """A single title/stats page (Figure, not a Response) opening the PDF
    report -- the same numbers as the Results screen's Summary tab
    (``runs.summarize``), laid out as plain text rather than re-deriving
    them separately, so the two can't silently drift apart."""
    from matplotlib.figure import Figure

    from ..reporting.theme import get_palette

    summary = runs.summarize(result)
    pal = get_palette(theme)

    fig = Figure(figsize=(8.27, 11.69))  # A4 portrait, points match savefig's default
    fig.patch.set_facecolor(pal.bg)
    ax = fig.add_axes((0.0, 0.0, 1.0, 1.0))
    ax.axis("off")
    ax.set_facecolor(pal.bg)

    def pct(v: Optional[float]) -> str:
        return "-" if v is None else f"{v * 100:.1f}% MAC"

    base = summary.get("baseline") or {}
    opt = summary.get("optimized") or {}

    lines = [
        ("ALAS Design Summary", 24, "bold"),
        (f"Preset: {summary.get('preset') or '-'}", 13, "normal"),
        ("", 8, "normal"),
        ("Baseline", 15, "bold"),
        (f"  Static margin: {pct(base.get('static_margin'))}", 12, "normal"),
        (f"  CG: {pct(base.get('cg_pct_mac'))}", 12, "normal"),
        ("", 8, "normal"),
        ("Optimized", 15, "bold"),
        (f"  Static margin: {pct(opt.get('static_margin'))}", 12, "normal"),
        (
            f"  CG envelope: {'OK' if opt.get('cg_envelope_ok') else 'Violation' if 'cg_envelope_ok' in opt else '-'}",
            12,
            "normal",
        ),
    ]
    extras = []
    if summary.get("mission_status"):
        extras.append(f"Mission: {summary['mission_status']}")
    if summary.get("mses_status"):
        extras.append(f"MSES: {summary['mses_status']}")
    if summary.get("structural_status"):
        extras.append(f"Structures: {summary['structural_status']}")
    if extras:
        lines.append(("", 8, "normal"))
        lines.append(("Disciplines", 15, "bold"))
        lines.extend((f"  {e}", 12, "normal") for e in extras)

    y = 0.94
    for text, size, weight in lines:
        if text:
            ax.text(
                0.07,
                y,
                text,
                transform=ax.transAxes,
                fontsize=size,
                fontweight=weight,
                color=pal.title,
            )
        y -= (size / 400.0) + 0.012
    return fig


@router.get("/pipeline/{run_id}/export/report.pdf")
def export_report_pdf(
    run_id: str, theme: Optional[str] = Query(default=None)
) -> Response:
    """A generic multi-page PDF report for this run: a summary page (the
    same numbers as the Results screen's Summary tab) followed by every
    available figure, one per page, in the same order the Results screen's
    tabs present them. Uses matplotlib's own PdfPages writer directly on the
    already-built Figure objects -- no separate PDF/templating dependency
    needed since every RESULT_FIGURES entry already is a Figure."""
    from matplotlib.backends.backend_pdf import PdfPages

    result = _get_ready_run(run_id)

    buf = io.BytesIO()
    with _RENDER_LOCK:
        with PdfPages(buf) as pdf:
            summary_fig = _build_summary_page(result, theme)
            pdf.savefig(summary_fig, facecolor=summary_fig.get_facecolor())
            for name, fig in _iter_result_figures(result, theme):
                pdf.savefig(fig, facecolor=fig.get_facecolor())
                import matplotlib.pyplot as plt

                plt.close(fig)

    return Response(
        content=buf.getvalue(),
        media_type="application/pdf",
        headers={
            "Content-Disposition": f'attachment; filename="alas-{run_id}-report.pdf"'
        },
    )


class PreviewRequest(BaseModel):
    config: dict = {}
    design: Optional[dict] = None
    # Optional 3-D camera for exterior/cabin previews: {"elev","azim","zoom"}.
    view: Optional[dict] = None
    # Optional measured size (CSS px) of the panel the preview will actually
    # be displayed in. Every figure_*/preview builder renders at its own
    # fixed matplotlib aspect ratio, so a container of a different shape (the
    # common case -- a resized dock, an ultrawide window) could only ever be
    # letterboxed by CSS, never truly filled. Re-sizing the already-built
    # figure to match is a single generic step here rather than plumbing a
    # target aspect through every individual builder.
    width_px: Optional[float] = None
    height_px: Optional[float] = None


# Clamp so a not-yet-measured (0x0) or wildly out-of-range client value can't
# hand matplotlib a degenerate or absurd canvas size.
_MIN_PREVIEW_IN = 1.5
_MAX_PREVIEW_IN = 40.0


def _resize_to_container(
    fig, width_px: Optional[float], height_px: Optional[float]
) -> bool:
    """Resize ``fig`` to match the panel's measured box, so the SVG's own
    aspect equals the container's instead of leaving CSS to letterbox (blank
    space) or overflow (ultrawide) a mismatched one. Returns whether a resize
    was actually applied -- the caller uses this to decide whether saving
    with a tight bbox (which would re-crop back to the *content's* aspect,
    undoing this entirely) is safe.
    """
    if not width_px or not height_px or width_px <= 0 or height_px <= 0:
        return False
    dpi = fig.dpi or 100.0
    w_in = min(_MAX_PREVIEW_IN, max(_MIN_PREVIEW_IN, width_px / dpi))
    h_in = min(_MAX_PREVIEW_IN, max(_MIN_PREVIEW_IN, height_px / dpi))
    fig.set_size_inches(w_in, h_in, forward=True)
    # tight_layout() recomputes every axes position, which would undo the
    # deliberate full-bleed placement the 3-D previews set (figures._fill_3d_axes)
    # and shrink the model back to ~68% of the canvas width. Matplotlib also
    # doesn't properly support tight_layout on mplot3d axes in the first place,
    # so skip it entirely when any 3-D axes is present.
    has_3d = any(hasattr(ax, "get_zlim") for ax in fig.axes)
    if not has_3d:
        try:
            fig.tight_layout()
        except Exception:
            pass
    return True


def _build_config(data: dict) -> ALASConfig:
    # TypeError/ValueError cover malformed values (e.g. a string mid-edit
    # where a number belongs), not just KeyError's unknown-key case -- all
    # deserve a 422, not an unhandled 500.
    try:
        return ALASConfig.from_dict(data)
    except (KeyError, TypeError, ValueError) as exc:
        raise HTTPException(status_code=422, detail=str(exc))


@router.post("/preview/{name}")
def get_preview(
    name: str, req: PreviewRequest, theme: Optional[str] = Query(default=None)
) -> Response:
    builder = figures.PREVIEW_FIGURES.get(name)
    if builder is None:
        raise HTTPException(status_code=404, detail=f"Unknown preview '{name}'")

    config = _build_config(req.config)
    try:
        dv = DesignVector(**req.design) if req.design else DesignVector.default()
    except TypeError as exc:
        raise HTTPException(status_code=422, detail=f"Bad design vector: {exc}")

    with _RENDER_LOCK:
        try:
            fig = builder(config, dv, theme, req.view)
        except Exception as exc:
            _close_all_figures()
            raise HTTPException(
                status_code=422, detail=f"Preview '{name}' failed: {exc}"
            )
        if fig is None:
            raise HTTPException(
                status_code=404, detail=f"Preview '{name}' produced no figure"
            )
        resized = _resize_to_container(fig, req.width_px, req.height_px)
        return _svg_response(fig, tight=not resized)


@router.get("/pipeline/{run_id}/route-geo")
def get_route_geo(run_id: str) -> dict:
    state = runs.REGISTRY.get(run_id)
    if state is None:
        raise HTTPException(status_code=404, detail=f"Unknown run id '{run_id}'")
    if state.status != "done":
        raise HTTPException(status_code=409, detail=f"Run '{run_id}' not finished")
    try:
        data = figures_extra.route_geo_data(state.result)
    except Exception as exc:
        raise HTTPException(status_code=404, detail=f"Route data unavailable: {exc}")
    if data is None:
        raise HTTPException(status_code=404, detail="No route for this run")
    return data


@router.get("/pipeline/{run_id}/field-performance")
def get_field_performance(run_id: str) -> dict:
    state = runs.REGISTRY.get(run_id)
    if state is None:
        raise HTTPException(status_code=404, detail=f"Unknown run id '{run_id}'")
    if state.status != "done":
        raise HTTPException(status_code=409, detail=f"Run '{run_id}' not finished")
    try:
        data = figures_extra.field_performance_data(state.result)
    except Exception as exc:
        raise HTTPException(
            status_code=404, detail=f"Field performance unavailable: {exc}"
        )
    if data is None:
        raise HTTPException(status_code=404, detail="No optimized report for this run")
    return data
