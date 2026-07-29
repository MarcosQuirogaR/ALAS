// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { useEffect, useState } from "react";
import logo from "../assets/images/logo-universal.png";

// Full-window boot splash: shown from the very first paint until the sidecar
// answers the initial /schema load, so a double-clicked ALAS.exe
// immediately shows *something alive* instead of an inert empty window while
// the frozen Python engine unpacks (first launch) or cold-starts. The
// elapsed counter + first-launch hint appear after a few seconds so a long
// (but normal) cold start reads as "loading", not "stuck".

export function Splash({
  state,
  error,
  onRetry,
}: {
  state: "loading" | "error";
  error?: string | null;
  onRetry: () => void;
}) {
  const [elapsed, setElapsed] = useState(0);

  useEffect(() => {
    if (state !== "loading") return;
    const started = Date.now();
    const t = setInterval(() => setElapsed(Math.floor((Date.now() - started) / 1000)), 1000);
    return () => clearInterval(t);
  }, [state]);

  return (
    <div className="af-splash">
      <img src={logo} alt="ALAS" className="af-splash-logo" />
      <div className="af-splash-title">ALAS</div>
      {state === "loading" ? (
        <>
          <div className="af-splash-status">
            Starting analysis engine…{elapsed >= 5 ? ` (${elapsed}s)` : ""}
          </div>
          {elapsed >= 15 && (
            <div className="af-splash-hint">
              First launch after an install or update can take a minute while the engine unpacks.
            </div>
          )}
        </>
      ) : (
        <>
          <div className="af-splash-status af-splash-error">
            Could not start the analysis engine.
          </div>
          {error && <div className="af-splash-hint">{error}</div>}
          <button className="af-btn-primary" style={{ marginTop: 14 }} onClick={onRetry}>
            Retry
          </button>
        </>
      )}
    </div>
  );
}
