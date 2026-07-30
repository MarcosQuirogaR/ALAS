// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { useEffect, useRef, useState } from "react";

// Bottom "Run Log" dock (a read-only
// QPlainTextEdit). Streams the same human-readable progress strings the
// pipeline's progress_callback emits, plus preset/engine change lines. A
// top-edge resizer mimics the draggable Qt dock splitter.

export type LogLine = { text: string; kind?: "info" | "error" | "warn" };

export function RunLog({ lines }: { lines: LogLine[] }) {
  const [height, setHeight] = useState(140);
  const bodyRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (bodyRef.current) bodyRef.current.scrollTop = bodyRef.current.scrollHeight;
  }, [lines]);

  function startResize(e: React.PointerEvent) {
    e.preventDefault();
    const startY = e.clientY;
    const startH = height;
    function move(ev: PointerEvent) {
      setHeight(Math.min(420, Math.max(70, startH - (ev.clientY - startY))));
    }
    function up() {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    }
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  return (
    <div className="af-logdock" style={{ ["--log-h" as any]: `${height}px` }}>
      <div className="af-logdock-resizer" onPointerDown={startResize} />
      <div className="af-logdock-title">Run Log</div>
      <div className="af-log" ref={bodyRef}>
        {lines.length === 0 ? (
          <span style={{ opacity: 0.5 }}>Ready.</span>
        ) : (
          lines.map((l, i) => (
            <div key={i} className={l.kind === "error" ? "err" : l.kind === "warn" ? "warn" : undefined}>
              {l.text}
            </div>
          ))
        )}
      </div>
    </div>
  );
}
