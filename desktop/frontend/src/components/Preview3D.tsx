// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { sidecarPostText } from "../lib/sidecarClient";
import type { ThemeName } from "./MenuBar";

// Interactive 3D live preview (Inputs dock). The figure is still rendered by
// Python/Matplotlib server-side, but drag-to-rotate and wheel-to-zoom send the
// camera (elev/azim/zoom) to POST /preview and re-render -- so the user can
// actually move the camera around, not just watch it change with the preset.
// Renders settle-to-render (debounced) so a continuous drag doesn't flood the
// single-threaded renderer.

type View = { elev: number; azim: number; zoom: number };
const DEFAULT_VIEW: View = { elev: 22, azim: -125, zoom: 1 };

// Tallest the preview card may get, as a multiple of its own width. A measured
// sweep of the rendered wireframe put its natural silhouette at roughly
// 1.0 x 0.25 of the panel width; 1.1 leaves comfortable room around it while
// keeping the card from stretching to the dock's full height and framing the
// model in mostly-empty space.
const PREVIEW_ASPECT_CAP = 1.1;

function prepSvg(svg: string): string {
  const i = svg.indexOf("<svg");
  if (i > 0) svg = svg.slice(i);
  return svg.replace(/<svg([^>]*?)\swidth="[^"]*"([^>]*?)\sheight="[^"]*"/, "<svg$1$2");
}

export function Preview3D({
  name,
  title,
  config,
  design,
  theme,
}: {
  name: string;
  title: string;
  config: Record<string, any>;
  design: Record<string, number> | null;
  theme: ThemeName;
}) {
  const [view, setView] = useState<View>(DEFAULT_VIEW);
  const [svg, setSvg] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [popped, setPopped] = useState(false);
  const [size, setSize] = useState<{ width: number; height: number } | null>(null);
  const reqId = useRef(0);
  const drag = useRef<{ x: number; y: number } | null>(null);
  const cardRef = useRef<HTMLDivElement>(null);
  // Last time a render request actually went out (as opposed to merely being
  // scheduled) -- see the fetch effect below.
  const lastFetchAt = useRef(0);

  const cfgKey = useMemo(() => JSON.stringify({ config, design }), [config, design]);

  // Track this panel's own rendered box (debounced) so the backend can
  // render at that aspect instead of leaving CSS to letterbox a mismatched
  // fixed one -- the same "doesn't actually resize with the dock" fix as
  // PreviewChart/FigureCard's `sizeAware`, applied here since this component
  // has its own fetch logic rather than going through FigureCard.
  useEffect(() => {
    if (!cardRef.current) return;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const ro = new ResizeObserver((entries) => {
      const rect = entries[0]?.contentRect;
      if (!rect || rect.width <= 0 || rect.height <= 0) return;
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        setSize((prev) => {
          if (prev && Math.abs(prev.width - rect.width) < 4 && Math.abs(prev.height - rect.height) < 4) return prev;
          return { width: rect.width, height: rect.height };
        });
      }, 180);
    });
    ro.observe(cardRef.current);
    return () => {
      ro.disconnect();
      if (timer) clearTimeout(timer);
    };
  }, []);

  // Throttle-with-trailing-edge, not a plain debounce. `view` changes on
  // every pointermove during a drag (many times a second); a plain debounce
  // clears and restarts its timer on each one, so as long as the mouse kept
  // moving faster than the wait, no render ever fired -- the rotation
  // visibly froze at the pre-drag angle for the whole gesture and only
  // jumped to the final position ~180ms after release, reading as "laggy"
  // even though each individual render wasn't slow. Firing immediately once
  // RENDER_MIN_GAP_MS has elapsed since the last actual request (not the
  // last scheduled one) gives a render roughly every RENDER_MIN_GAP_MS
  // *during* continuous dragging too, while still coalescing bursts faster
  // than that and guaranteeing a final trailing render once movement stops.
  const RENDER_MIN_GAP_MS = 150;

  useEffect(() => {
    const id = ++reqId.current;
    setLoading(true);

    const fetchNow = () => {
      lastFetchAt.current = Date.now();
      sidecarPostText(`/preview/${name}?theme=${theme}`, {
        config, design, view,
        width_px: size?.width, height_px: size?.height,
      })
        .then((text) => {
          if (id !== reqId.current) return;
          setSvg(prepSvg(text));
          setError(null);
        })
        .catch((err: any) => {
          if (id === reqId.current) setError(String(err?.message ?? err));
        })
        .finally(() => {
          if (id === reqId.current) setLoading(false);
        });
    };

    const elapsed = Date.now() - lastFetchAt.current;
    if (elapsed >= RENDER_MIN_GAP_MS) {
      fetchNow();
      return;
    }
    const timer = setTimeout(fetchNow, RENDER_MIN_GAP_MS - elapsed);
    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [name, cfgKey, theme, view, size?.width, size?.height]);

  function onPointerDown(e: React.PointerEvent) {
    drag.current = { x: e.clientX, y: e.clientY };
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
  }
  function onPointerMove(e: React.PointerEvent) {
    if (!drag.current) return;
    const dx = e.clientX - drag.current.x;
    const dy = e.clientY - drag.current.y;
    drag.current = { x: e.clientX, y: e.clientY };
    setView((v) => ({
      ...v,
      azim: v.azim - dx * 0.6,
      elev: Math.max(-89, Math.min(89, v.elev + dy * 0.6)),
    }));
  }
  function onPointerUp() {
    drag.current = null;
  }
  function onZoom(deltaY: number) {
    setView((v) => ({ ...v, zoom: Math.min(4, Math.max(0.4, v.zoom * Math.exp(deltaY * 0.001))) }));
  }

  const viewport = (large: boolean) =>
    !svg ? (
      <div className="af-placeholder" style={{ flex: 1 }}>rendering…</div>
    ) : (
      <SvgViewport
        svg={svg}
        dragging={!!drag.current}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onZoom={onZoom}
        onDoubleClick={large ? undefined : () => setPopped(true)}
      />
    );

  // An aircraft (and a cabin) is far wider than it is tall, so once the model
  // spans the panel's full width it physically cannot also fill a very tall
  // dock -- scaling further would just crop the wingtips. Stretching the card
  // to the dock's whole height therefore left a large empty band above and
  // below the wireframe, which read as "the preview doesn't fill its window".
  // Capping the card's height to a sensible multiple of its width makes it hug
  // the model instead: the same on-screen model size, without the void.
  const cappedHeight = size?.width ? Math.round(size.width * PREVIEW_ASPECT_CAP) : undefined;

  return (
    <div
      className="af-chart fill"
      ref={cardRef}
      style={cappedHeight ? { maxHeight: cappedHeight } : undefined}
    >
      <div className="af-chart-head">
        <span>{title}</span>
        <span>
          {loading && <span style={{ marginRight: 10 }}>rotating…</span>}
          <span className="af-expand" onClick={() => setView(DEFAULT_VIEW)} title="Reset camera">reset</span>
          <span className="af-expand" style={{ marginLeft: 10 }} onClick={() => setPopped(true)}>expand</span>
        </span>
      </div>
      {error && !svg ? <div className="af-placeholder">{error}</div> : viewport(false)}
      <div className="af-help" style={{ textAlign: "center" }}>Drag to rotate - scroll to zoom</div>
      {popped &&
        createPortal(
          <div className="af-modal" onClick={() => setPopped(false)}>
            <div className="af-modal-body" onClick={(e) => e.stopPropagation()}>
              <div className="af-row" style={{ marginBottom: 8 }}>
                <strong>{title}</strong>
                <span className="af-spacer" />
                <button onClick={() => setView(DEFAULT_VIEW)}>Reset camera</button>
                <button onClick={() => setPopped(false)}>Close</button>
              </div>
              {viewport(true)}
            </div>
          </div>,
          document.body
        )}
    </div>
  );
}

// Module-level (stable identity) on purpose: the previous nested
// `function Body()` inside Preview3D got a new component identity every
// render, so React unmounted and remounted the whole SVG subtree on every
// setView during a drag -- tearing down the pointer-captured element
// mid-gesture and re-parsing the injected SVG each frame.
function SvgViewport({
  svg,
  dragging,
  onPointerDown,
  onPointerMove,
  onPointerUp,
  onZoom,
  onDoubleClick,
}: {
  svg: string;
  dragging: boolean;
  onPointerDown: (e: React.PointerEvent) => void;
  onPointerMove: (e: React.PointerEvent) => void;
  onPointerUp: () => void;
  onZoom: (deltaY: number) => void;
  onDoubleClick?: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const zoomRef = useRef(onZoom);
  zoomRef.current = onZoom;

  // Native non-passive wheel listener: React attaches `onWheel` passively
  // (the browser default for wheel), so preventDefault() there is ignored --
  // the page scrolls while zooming and the console fills with "Unable to
  // preventDefault inside passive event listener" warnings.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      zoomRef.current(e.deltaY);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, []);

  return (
    <div
      ref={ref}
      className="af-svg"
      style={{ flex: 1, minHeight: 0, cursor: dragging ? "grabbing" : "grab", touchAction: "none" }}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerLeave={onPointerUp}
      onDoubleClick={onDoubleClick}
      dangerouslySetInnerHTML={{ __html: svg }}
    />
  );
}
