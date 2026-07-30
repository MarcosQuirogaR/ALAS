// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

// A titled chart card that lazily loads a figure as inline SVG via a
// caller-supplied async loader (GET a result figure, or POST a live preview),
// debounced. The SVG is injected inline and scaled by CSS to fill its
// container crisply at any size -- the "real Matplotlib output" embedded,
// vector, not a fixed-size raster. Double-click opens a full-screen view.

type CardSize = { width: number; height: number };

type FigureCardProps = {
  title: string;
  loader: (size?: CardSize) => Promise<string>;
  deps: unknown[];
  debounceMs?: number;
  /** Fill the parent's height (previews/dock) instead of sizing to content. */
  fill?: boolean;
  minHeight?: number;
  /** Re-run `loader` (debounced) with this card's own measured box whenever
   * it resizes, so a size-aware loader (see PreviewChart) can ask the
   * backend to render at that aspect instead of leaving CSS to letterbox a
   * mismatched fixed aspect ratio. Off by default -- result figures have no
   * use for it (their GET endpoint takes no size) and re-fetching on every
   * resize would be wasted work for them. */
  sizeAware?: boolean;
};

function prepSvg(svg: string): string {
  // matplotlib prepends an <?xml?> declaration + <!DOCTYPE>; drop everything
  // before the root <svg> so innerHTML parses cleanly, then remove the fixed
  // width/height attrs so CSS controls the size (the viewBox preserves aspect).
  const i = svg.indexOf("<svg");
  if (i > 0) svg = svg.slice(i);
  return svg.replace(/<svg([^>]*?)\swidth="[^"]*"([^>]*?)\sheight="[^"]*"/, "<svg$1$2");
}

export function FigureCard({ title, loader, deps, debounceMs = 200, fill = false, minHeight = 220, sizeAware = false }: FigureCardProps) {
  const [svg, setSvg] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [popped, setPopped] = useState(false);
  const [size, setSize] = useState<CardSize | null>(null);
  const reqId = useRef(0);
  const containerRef = useRef<HTMLDivElement>(null);

  // Track this card's own rendered box (debounced) so a size-aware loader
  // can ask the backend to render at that aspect instead of leaving CSS to
  // letterbox a mismatched one -- see the `sizeAware` doc comment above.
  useEffect(() => {
    if (!sizeAware || !containerRef.current) return;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const ro = new ResizeObserver((entries) => {
      const rect = entries[0]?.contentRect;
      if (!rect || rect.width <= 0 || rect.height <= 0) return;
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        setSize((prev) => {
          // Ignore sub-pixel jitter (rounding, scrollbar show/hide) so a
          // settled size doesn't keep re-triggering fetches forever.
          if (prev && Math.abs(prev.width - rect.width) < 4 && Math.abs(prev.height - rect.height) < 4) return prev;
          return { width: rect.width, height: rect.height };
        });
      }, debounceMs);
    });
    ro.observe(containerRef.current);
    return () => {
      ro.disconnect();
      if (timer) clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sizeAware]);

  useEffect(() => {
    const id = ++reqId.current;
    setLoading(true);
    setError(null);
    const timer = setTimeout(async () => {
      try {
        const text = await loader(sizeAware && size ? size : undefined);
        if (id !== reqId.current) return;
        setSvg(prepSvg(text));
      } catch (err: any) {
        if (id === reqId.current) {
          setError(String(err?.message ?? err));
          setSvg(null);
        }
      } finally {
        if (id === reqId.current) setLoading(false);
      }
    }, debounceMs);
    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, size?.width, size?.height]);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") setPopped(false);
    }
    if (popped) window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [popped]);

  const friendly =
    error && (error.includes("404") || error.includes("no data") || error.includes("unavailable"))
      ? "Not available for this run."
      : error;

  return (
    <div className={"af-chart" + (fill ? " fill" : "")} ref={containerRef}>
      <div className="af-chart-head">
        <span>{title}</span>
        {loading ? <span>loading…</span> : svg && <span className="af-expand" onClick={() => setPopped(true)}>expand ⤢</span>}
      </div>
      {error && !svg ? (
        <div className="af-placeholder" style={{ minHeight }}>{friendly}</div>
      ) : svg ? (
        <div
          className="af-svg"
          onDoubleClick={() => setPopped(true)}
          dangerouslySetInnerHTML={{ __html: svg }}
        />
      ) : (
        <div className="af-placeholder" style={{ minHeight }}>rendering…</div>
      )}
      {popped && svg &&
        createPortal(
          <div className="af-modal" onClick={() => setPopped(false)}>
            <div className="af-modal-body" onClick={(e) => e.stopPropagation()}>
              <div className="af-row" style={{ marginBottom: 8 }}>
                <strong>{title}</strong>
                <span className="af-spacer" />
                <button onClick={() => setPopped(false)}>Close</button>
              </div>
              <div className="af-svg af-svg-modal" dangerouslySetInnerHTML={{ __html: svg }} />
            </div>
          </div>,
          document.body
        )}
    </div>
  );
}
