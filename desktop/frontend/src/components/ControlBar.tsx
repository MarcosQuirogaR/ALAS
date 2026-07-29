// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

// Bottom control bar: the randomizer group
// (DOE Sample / Surprise), a separator, then the run group (Analyze baseline /
// Run), an indeterminate progress bar, a stage/elapsed label and a status
// label. While a run is in flight the stage slot shows a ticking m:ss
// stopwatch instead of the raw last-stage message (hoverable for that detail,
// and it's always fully logged in the Run Log below regardless) -- otherwise
// it shows the last stage message as before. Run is the accent "primary"
// button; it is disabled while a run is in flight or a blocking
// (error-severity) validation issue exists.

type ControlBarProps = {
  onDoeSample: () => void;
  onSurprise: () => void;
  onAnalyzeBaseline: () => void;
  onRun: () => void;
  running: boolean;
  blocked: boolean;
  progress: number | null;
  stage: string;
  status: string;
  /** Milliseconds since the current run started (0 when idle). */
  elapsedMs: number;
  /** Re-attempts the initial sidecar connection; shown only on sidecar error. */
  onRetrySidecar?: () => void;
};

function formatElapsed(ms: number): string {
  const totalSec = Math.floor(ms / 1000);
  const m = Math.floor(totalSec / 60);
  const s = totalSec % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
}

import { useT } from "../lib/i18n";

export function ControlBar(p: ControlBarProps) {
  const t = useT();
  const indeterminate = p.running && p.progress === null;
  return (
    <div className="af-controlbar">
      <button
        data-tour="randomizer"
        onClick={p.onDoeSample}
        disabled={p.running}
        title="Draw one Latin-Hypercube design point within the Design Space bounds"
      >
        {t("DOE Sample")}
      </button>
      <button
        onClick={p.onSurprise}
        disabled={p.running}
        title="Draw a design +/-30% beyond the bounds, then run the full pipeline"
      >
        {t("Surprise")}
      </button>

      <span className="sep" />

      <button
        onClick={p.onAnalyzeBaseline}
        disabled={p.running}
        title="Weight and balance + stability of the current design, no optimizer"
      >
        {t("Analyze baseline")}
      </button>
      <button
        data-tour="run"
        className="af-btn-primary"
        onClick={p.onRun}
        disabled={p.running || p.blocked}
        title={
          p.blocked
            ? "Fix error-severity validation issues first"
            : "Optimize, analyze, export and run the mission in one pass"
        }
      >
        {p.running ? t("Running...") : t("Run")}
      </button>

      <div className={"af-progress" + (indeterminate ? " indeterminate" : "")}>
        <div
          className="bar"
          style={{ width: indeterminate ? undefined : `${Math.round((p.progress ?? 0) * 100)}%` }}
        />
      </div>
      {p.running ? (
        <span className="af-stage" title={p.stage || undefined}>{formatElapsed(p.elapsedMs)}</span>
      ) : (
        p.stage && <span className="af-stage">{p.stage}</span>
      )}
      <span className="af-status">{t(p.status)}</span>
      {p.status === "Sidecar error." && p.onRetrySidecar && (
        <button onClick={p.onRetrySidecar} title="Try connecting to the sidecar again">
          Retry
        </button>
      )}
    </div>
  );
}
