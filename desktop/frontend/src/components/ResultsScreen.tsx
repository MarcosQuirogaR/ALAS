// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { useRef, useState } from "react";
import { ResultFigure } from "./ResultFigure";
import { FieldPerformancePanel } from "./FieldPerformancePanel";
import { RouteGlobe } from "./RouteGlobe";
import { StatTile } from "./StatTile";
import { sidecarDownload } from "../lib/sidecarClient";
import type { ThemeName } from "./MenuBar";
import { useT } from "../lib/i18n";

// Results page. Mirrors the PySide6 ResultsView tab strip: Summary + a tab per
// discipline, each a grid of figures pulled from the sidecar's RESULT_FIGURES
// registry (GET /pipeline/{id}/figures/{name}). Figures with no data for the
// current run render an "Not available for this run" slot, matching the Qt
// view's per-figure graceful degradation (e.g. mission plots on a run with
// mission analysis off, or every non-W&B figure on an Analyze-baseline run).

export type ResultSummary = {
  preset?: string;
  baseline?: Record<string, any>;
  optimized?: Record<string, any>;
  baseline_comparison?: Record<string, any>;
  mission_status?: string;
  mses_status?: string;
  structural_status?: string;
  [k: string]: any;
};

type Fig = { name: string; title: string };
type Tab = { id: string; title: string; figures: Fig[] };

const TABS: Tab[] = [
  {
    id: "optimization",
    title: "Optimization",
    figures: [
      { name: "optimization_history", title: "Convergence history" },
      { name: "design_evolution", title: "Design evolution" },
      { name: "airfoil_comparison", title: "Airfoil comparison" },
      { name: "airfoil_evolution", title: "Airfoil spanwise evolution" },
      { name: "polar_comparison", title: "Drag polar (baseline vs optimized)" },
      { name: "planform_comparison", title: "Planform comparison" },
      { name: "wireframe_wing", title: "Wing wireframe" },
      { name: "wireframe_fuselage", title: "Fuselage wireframe" },
      { name: "wireframe_empennage", title: "Empennage wireframe" },
      { name: "asb_threeview", title: "Three-view" },
    ],
  },
  {
    id: "aero",
    title: "Aerodynamics",
    figures: [
      { name: "aero_panel", title: "Lift / drag / moment" },
      { name: "drag_breakdown", title: "Drag component breakdown" },
      { name: "span_loading", title: "Span loading" },
      { name: "vn_diagram", title: "V-n flight envelope" },
      { name: "vlm_flow", title: "VLM flow streamlines" },
      { name: "dynamic_modes", title: "Dynamic modes (s-plane)" },
      { name: "control_surfaces", title: "Control surfaces and tail sizing" },
      { name: "airfoil_reynolds", title: "Airfoil vs Reynolds" },
      { name: "mses_pressure", title: "MSES pressure distribution" },
      { name: "mses_mach_contours", title: "MSES Mach contours" },
    ],
  },
  {
    id: "wb",
    title: "Weight & Balance",
    figures: [
      { name: "mass_breakdown", title: "Mass breakdown" },
      { name: "fuel_volume_check", title: "Fuel-volume check" },
      { name: "mass_distribution", title: "Plan-view mass distribution" },
      { name: "cg_envelope", title: "CG envelope" },
      { name: "landing_gear_planform", title: "Landing-gear planform" },
      { name: "stability_side_view", title: "Stability - side view" },
      { name: "stability_metrics", title: "Stability - metrics" },
      { name: "cabin_payload", title: "Cabin / payload layout" },
    ],
  },
  {
    id: "propulsion",
    title: "Propulsion",
    figures: [
      { name: "propulsion_cycle_summary", title: "Cycle summary" },
      { name: "propulsion_carpet_plot", title: "Carpet plot" },
      { name: "propulsion_efficiency_decomposition", title: "Efficiency decomposition" },
      { name: "propulsion_bpr_sensitivity", title: "BPR sensitivity" },
      { name: "propulsion_altitude_sweep", title: "Altitude and Mach sweep" },
    ],
  },
  {
    id: "structures",
    title: "Structures",
    figures: [
      { name: "structures_sizing", title: "Wingbox sizing" },
      { name: "structures_loads", title: "Static loads" },
      { name: "structures_stress", title: "Stress margins" },
      { name: "structures_modes", title: "Normal modes" },
      { name: "structures_vibration", title: "Vibration" },
      { name: "structures_patran", title: "Patran renders" },
    ],
  },
  {
    id: "mission",
    title: "Mission & Route",
    figures: [
      { name: "mission_route_2d", title: "Route map (2D)" },
      { name: "payload_range", title: "Payload-range" },
      { name: "mission_profile", title: "Mission profile" },
      { name: "mission_velocities", title: "Airspeeds" },
      { name: "mission_flight_path", title: "Flight path" },
      { name: "mission_aero_coefficients", title: "Aero coefficients" },
      { name: "mission_aero_forces", title: "Aero forces" },
      { name: "mission_drag_components", title: "Drag components" },
    ],
  },
  {
    id: "field",
    title: "Field Performance",
    figures: [
      { name: "matching_chart", title: "Matching chart (design space)" },
      { name: "lto_departure", title: "Landing & Take-Off - departure" },
      { name: "lto_arrival", title: "Landing & Take-Off - arrival" },
    ],
  },
  {
    id: "model",
    title: "Model Comparison",
    figures: [{ name: "model_comparison", title: "AeroSandbox vs SUAVE vs MSES" }],
  },
];

function fmtPct(v?: number | null) {
  return v === undefined || v === null ? "-" : `${(v * 100).toFixed(1)}% MAC`;
}

export function ResultsScreen({
  runId,
  theme,
  summary,
  running,
}: {
  runId: string | null;
  theme: ThemeName;
  summary: ResultSummary | null;
  running: boolean;
}) {
  const t = useT();
  const [active, setActive] = useState("summary");
  // Tabs the user has opened at least once for this run. Figures are real
  // Matplotlib renders fetched over HTTP, and React only renders the
  // `active` tab's content by default, so switching tabs would unmount and
  // remount each <ResultFigure>, re-fetching and re-rendering every SVG from
  // scratch on every revisit. Once a tab has been opened, keep its content
  // mounted (hidden via CSS, not unmounted) instead -- switching back is then
  // instant with no network call, while a tab that's never been opened still
  // isn't fetched at all (no eager-loading every discipline's figures up
  // front just because the run finished).
  const [visited, setVisited] = useState<Set<string>>(new Set(["summary"]));
  const prevRunId = useRef(runId);
  if (prevRunId.current !== runId) {
    prevRunId.current = runId;
    // A different run: the old cached tabs' figures belong to a run that's
    // no longer current. Restart from Summary rather than carry forward
    // (now stale-until-refetched) mounted figure grids for tabs that happen
    // to share a name across runs.
    if (visited.size > 1 || active !== "summary") {
      setVisited(new Set(["summary"]));
      setActive("summary");
    }
  }

  function selectTab(id: string) {
    setActive(id);
    setVisited((prev) => (prev.has(id) ? prev : new Set(prev).add(id)));
  }

  if (!runId) {
    return (
      <div className="af-placeholder" style={{ height: 260 }}>
        <strong style={{ fontSize: 15 }}>{t("No results yet")}</strong>
        {running
          ? t("Running the pipeline...")
          : t("Press Run to optimize and analyze, or Analyze baseline for a quick weight and balance pass.")}
      </div>
    );
  }

  const b = summary?.baseline;
  const o = summary?.optimized;

  return (
    // `af-results-root`: cancels the scroll container's top padding (see
    // App.css) so the sticky tab strip below pins flush to the very top --
    // otherwise a padding-height band sits above it and scrolled figures
    // bleed through that gap.
    <div className="af-stack af-results-root">
      {/* One sticky unit -- tabs AND export buttons need to dock to the top of
          the scroll area together, since a non-sticky header above a sticky
          tab strip lets the header (and the gap it leaves) scroll away
          underneath the pinned tabs: the tabs read as floating, detached from
          the menu bar, and the exporter no longer tracks the tab selector on
          scroll. A single sticky row avoids both. */}
      {/* Tabs only. The export actions live in the File menu because they act
          on the whole run rather than the open tab, and as buttons here they
          either crowd the tab strip or float awkwardly on a second row. */}
      <div className="af-tabs sticky" style={{ flexWrap: "wrap" }}>
        <div className={"af-tab" + (active === "summary" ? " active" : "")} onClick={() => selectTab("summary")}>
          {t("Summary")}
        </div>
        {TABS.map((tab0) => (
          <div key={tab0.id} className={"af-tab" + (active === tab0.id ? " active" : "")} onClick={() => selectTab(tab0.id)}>
            {t(tab0.title)}
          </div>
        ))}
      </div>

      <div style={{ display: active === "summary" ? undefined : "none" }}>
        <div className="af-stack">
          {running && <div className="af-page-desc">{t("Run in progress - figures refresh when it completes.")}</div>}
          <div className="af-row" style={{ flexWrap: "wrap", gap: 12 }}>
            <StatTile label={t("Preset")} value={summary?.preset || "-"} />
            <StatTile label={t("Baseline static margin")} value={fmtPct(b?.static_margin)} />
            <StatTile label={t("Baseline CG")} value={b?.cg_pct_mac == null ? "-" : `${b.cg_pct_mac.toFixed(1)}% MAC`} />
            <StatTile label={t("Optimized static margin")} value={fmtPct(o?.static_margin)} />
            <StatTile label={t("CG envelope")} value={o ? (o.cg_envelope_ok ? "OK" : "Violation") : "-"} />
          </div>
          <div className="af-row" style={{ flexWrap: "wrap", gap: 12 }}>
            {summary?.mission_status && <StatTile label={t("Mission")} value={summary.mission_status} />}
            {summary?.mses_status && <StatTile label="MSES" value={summary.mses_status} />}
            {summary?.structural_status && <StatTile label="Structures" value={summary.structural_status} />}
          </div>
          {/* Why a discipline didn't produce data. These messages name the exact
              paths/reasons involved; without them a "not_configured" tile is
              undiagnosable, which is precisely how a broken SUAVE runtime went
              unexplained. */}
          {(summary?.mission_error || summary?.mses_error || summary?.structural_error) && (
            <details>
              <summary className="af-help" style={{ cursor: "pointer" }}>
                {t("Some analyses did not produce data — show why")}
              </summary>
              <ul className="af-issue-list">
                {summary?.mission_error && (
                  <li className="warning">
                    <strong>Mission ({summary.mission_status}):</strong>{" "}
                    <span style={{ whiteSpace: "pre-wrap" }}>{summary.mission_error}</span>
                  </li>
                )}
                {summary?.mses_error && (
                  <li className="warning">
                    <strong>MSES ({summary.mses_status}):</strong>{" "}
                    <span style={{ whiteSpace: "pre-wrap" }}>{summary.mses_error}</span>
                  </li>
                )}
                {summary?.structural_error && (
                  <li className="warning">
                    <strong>Structures ({summary.structural_status}):</strong>{" "}
                    <span style={{ whiteSpace: "pre-wrap" }}>{summary.structural_error}</span>
                  </li>
                )}
              </ul>
            </details>
          )}
          <div className="af-page-desc">
            Open a discipline tab above for its figures. Slots that read "Not available for this run" need data this
            run did not produce (e.g. mission plots when mission analysis is off, or non-W&B figures on an
            Analyze-baseline run).
          </div>
        </div>
      </div>

      {TABS.filter((t) => visited.has(t.id)).map((tab) => (
        <div key={tab.id} style={{ display: active === tab.id ? undefined : "none" }}>
          <div className="af-stack">
            {tab.id === "field" && <FieldPerformancePanel runId={runId} />}
            <div className="af-figure-grid">
              {tab.id === "mission" && <RouteGlobe runId={runId} />}
              {tab.figures.map((f) => (
                <ResultFigure key={f.name} runId={runId} name={f.name} title={t(f.title)} theme={theme} />
              ))}
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}
