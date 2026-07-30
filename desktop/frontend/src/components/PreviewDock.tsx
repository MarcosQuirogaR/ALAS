// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { useState } from "react";
import { createPortal } from "react-dom";
import { Preview3D } from "./Preview3D";
import type { ThemeName } from "./MenuBar";

// The right-hand "3D Live Preview" dock (with the
// Exterior 3D / Cabin Payload sub-tabs). Rendered by Python/Matplotlib live
// from the current config + design vector (no run needed) and interactive:
// drag to rotate the camera, scroll to zoom -- the camera is sent to the
// preview endpoint and re-rendered, matching the desktop mplot3d canvases.
//
// Closable (the "x" button hides it; View > 3D Live Preview in the menu bar
// brings it back, mirrored from App's `previewOpen` state) and can be popped
// out into a freely-draggable/resizable floating panel (the "float" button),
// the same way the old Qt QDockWidget could be undocked -- there's no native
// window-manager-level floating here (this is a single browser webview), so
// it's a portal'd, fixed-position div with its own drag/resize handling.

const FLOAT_DEFAULT = { x: 120, y: 80, w: 420, h: 560 };
const FLOAT_MIN = { w: 300, h: 320 };

export function PreviewDock({
  config,
  design,
  theme,
  presetLabel,
  onClose,
}: {
  config: Record<string, any>;
  design: Record<string, number> | null;
  theme: ThemeName;
  presetLabel: string;
  onClose: () => void;
}) {
  const [tab, setTab] = useState<"exterior" | "cabin">("exterior");
  const [width, setWidth] = useState(360);
  const [floating, setFloating] = useState(false);
  const [rect, setRect] = useState(FLOAT_DEFAULT);
  const hasConfig = config && Object.keys(config).length > 0;

  function startResize(e: React.PointerEvent) {
    e.preventDefault();
    const startX = e.clientX;
    const startW = width;
    function move(ev: PointerEvent) {
      setWidth(Math.min(680, Math.max(260, startW - (ev.clientX - startX))));
    }
    function up() {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    }
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  function startDragFloat(e: React.PointerEvent) {
    e.preventDefault();
    const startX = e.clientX;
    const startY = e.clientY;
    const start = rect;
    function move(ev: PointerEvent) {
      const maxX = window.innerWidth - 80;
      const maxY = window.innerHeight - 40;
      setRect((r) => ({
        ...r,
        x: Math.min(maxX, Math.max(-r.w + 120, start.x + (ev.clientX - startX))),
        y: Math.min(maxY, Math.max(0, start.y + (ev.clientY - startY))),
      }));
    }
    function up() {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    }
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  function startResizeFloat(e: React.PointerEvent) {
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    const startY = e.clientY;
    const start = rect;
    function move(ev: PointerEvent) {
      setRect((r) => ({
        ...r,
        w: Math.max(FLOAT_MIN.w, start.w + (ev.clientX - startX)),
        h: Math.max(FLOAT_MIN.h, start.h + (ev.clientY - startY)),
      }));
    }
    function up() {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    }
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  const tabs = (
    <div className="af-tabs">
      <div className={"af-tab" + (tab === "exterior" ? " active" : "")} onClick={() => setTab("exterior")}>
        Exterior 3D
      </div>
      <div className={"af-tab" + (tab === "cabin" ? " active" : "")} onClick={() => setTab("cabin")}>
        Cabin / Payload
      </div>
    </div>
  );

  const body = hasConfig ? (
    // Column + stretch so the preview card spans the dock's full width while
    // its own max-height (see Preview3D) keeps it hugging the model instead of
    // stretching down the whole dock and framing it in empty space.
    <div
      style={{
        flex: "1 1 auto",
        minHeight: 0,
        display: "flex",
        flexDirection: "column",
        alignItems: "stretch",
      }}
    >
      {tab === "exterior" ? (
        <Preview3D key="exterior" name="exterior_3d" title="Exterior wireframe" config={config} design={design} theme={theme} />
      ) : (
        <Preview3D key="cabin" name="cabin_3d" title="Cabin / payload" config={config} design={design} theme={theme} />
      )}
    </div>
  ) : (
    <div className="af-placeholder">Loading configuration...</div>
  );

  if (floating) {
    return createPortal(
      <div
        className="af-dock-float"
        style={{ left: rect.x, top: rect.y, width: rect.w, height: rect.h }}
      >
        <div className="af-dock-float-title" onPointerDown={startDragFloat}>
          <span>3D Live Preview — {presetLabel}</span>
          <span className="af-dock-controls">
            <span className="af-expand" title="Dock to sidebar" onClick={() => setFloating(false)}>dock</span>
            <span className="af-expand" title="Close" onClick={onClose}>✕</span>
          </span>
        </div>
        <div className="af-dock-body">
          {tabs}
          {body}
        </div>
        <div className="af-dock-float-resizer" onPointerDown={startResizeFloat} />
      </div>,
      document.body
    );
  }

  return (
    <aside className="af-dock" style={{ ["--dock-w" as any]: `${width}px` }}>
      <div className="af-dock-resizer" onPointerDown={startResize} />
      <div className="af-dock-title">
        <span>3D Live Preview — {presetLabel}</span>
        <span className="af-dock-controls">
          <span className="af-expand" title="Pop out into a floating window" onClick={() => setFloating(true)}>float</span>
          <span className="af-expand" title="Close" onClick={onClose}>✕</span>
        </span>
      </div>
      <div className="af-dock-body">
        {tabs}
        {body}
      </div>
    </aside>
  );
}
