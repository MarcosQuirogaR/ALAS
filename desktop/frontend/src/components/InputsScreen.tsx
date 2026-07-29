// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { DynamicForm, SchemaField } from "./DynamicForm";
import { useT } from "../lib/i18n";

// The Setup > Inputs page: an "Aircraft
// Configuration" card with Preset + Engine selectors, a "Mission requirements"
// card (the DesignRequirements auto-form), a "Route" card, and a "Run options"
// card. Layout and grouping match the Qt QGroupBox stack.

export type RunOptions = {
  optimize: boolean;
  compareBaseline: boolean;
  writeOutputs: boolean;
  parallel: boolean;
};

type InputsScreenProps = {
  requirementsFields: SchemaField[];
  configValues: Record<string, any>;
  onGroupChange: (group: string) => (name: string, value: any) => void;
  onTopLevelChange: (name: string, value: any) => void;
  presetNames: string[];
  presetDisplay: Record<string, string>;
  selectedPreset: string;
  onPresetChange: (name: string) => void;
  engineNames: string[];
  selectedEngine: string;
  onEngineChange: (name: string) => void;
  airportNames: string[];
  runOptions: RunOptions;
  onRunOptionChange: (key: keyof RunOptions, value: boolean) => void;
  errorFields: Set<string>;
};

function Card({ title, children, tour }: { title: string; children: React.ReactNode; tour?: string }) {
  const t = useT();
  return (
    <div className="af-card" data-tour={tour}>
      <div className="af-card-title">{t(title)}</div>
      {children}
    </div>
  );
}

export function InputsScreen(p: InputsScreenProps) {
  const t = useT();
  const req = p.configValues.requirements ?? {};
  const engineOptions =
    p.engineNames.includes(p.selectedEngine) || !p.selectedEngine
      ? p.engineNames
      : [p.selectedEngine, ...p.engineNames];
  // Defensive, same pattern as engineOptions: a config loaded from an older
  // saved file (or hand-edited YAML) can carry an airport string outside the
  // 20-entry list -- keep it selectable rather than silently mismatching the
  // <select>'s value against every <option>.
  function airportOptions(current: string): string[] {
    return !current || p.airportNames.includes(current) ? p.airportNames : [current, ...p.airportNames];
  }
  const departureOptions = airportOptions(p.configValues.departure_airport ?? "");
  const arrivalOptions = airportOptions(p.configValues.arrival_airport ?? "");

  return (
    // Was a flat `maxWidth: 1100` -- fine at a "normal" window, but on a
    // wide/ultrawide window this left most of .af-content empty. `min(...)`
    // caps the page at a comfortable reading width while still shrinking
    // gracefully (the 92% term) on a narrower window instead of touching
    // the edges.
    <div className="af-stack" style={{ maxWidth: "min(1600px, 92%)" }}>
      <Card title="Aircraft Configuration" tour="aircraft-config">
        <div className="af-row" style={{ gap: 24, flexWrap: "wrap" }}>
          <label className="af-row" style={{ flex: "1 1 320px", minWidth: 280 }}>
            <span style={{ minWidth: 56 }}>{t("Preset")}:</span>
            <select
              className="af-input"
              style={{ flex: 1 }}
              value={p.selectedPreset}
              onChange={(e) => p.onPresetChange(e.target.value)}
            >
              {p.presetNames.map((n) => (
                <option key={n} value={n}>
                  {p.presetDisplay[n] ?? n}
                </option>
              ))}
            </select>
          </label>
          <label className="af-row" style={{ flex: "1 1 240px", minWidth: 220 }}>
            <span style={{ minWidth: 56 }}>{t("Engine")}:</span>
            <select
              className="af-input"
              style={{ flex: 1 }}
              value={p.selectedEngine}
              onChange={(e) => p.onEngineChange(e.target.value)}
            >
              {engineOptions.map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </select>
          </label>
        </div>
      </Card>

      <Card title="Mission requirements">
        <DynamicForm
          fields={p.requirementsFields}
          values={req}
          onChange={p.onGroupChange("requirements")}
          errorFields={p.errorFields}
          columns
        />
      </Card>

      <Card title="Route">
        <div className="af-form-row" title="Overridden by a configured SimBrief flight plan (Mission Advanced Settings) whenever its origin/destination match.">
          <span className="af-form-label">{t("Departure airport")}</span>
          <select
            className="af-input"
            value={p.configValues.departure_airport ?? ""}
            onChange={(e) => p.onTopLevelChange("departure_airport", e.target.value)}
          >
            {departureOptions.map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </select>
        </div>
        <div className="af-form-row" title="Overridden by a configured SimBrief flight plan (Mission Advanced Settings) whenever its origin/destination match.">
          <span className="af-form-label">{t("Arrival airport")}</span>
          <select
            className="af-input"
            value={p.configValues.arrival_airport ?? ""}
            onChange={(e) => p.onTopLevelChange("arrival_airport", e.target.value)}
          >
            {arrivalOptions.map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </select>
        </div>
        <div className="af-help">
          Used as-is unless a SimBrief username is set (Mission Advanced Settings) and its fetched flight plan's
          origin/destination match -- see the Run Log after a run for which route was actually flown.
        </div>
      </Card>

      <Card title="Run options">
        <div className="af-stack" style={{ gap: 8 }}>
          <label className="af-check">
            <input
              type="checkbox"
              checked={p.runOptions.optimize}
              onChange={(e) => p.onRunOptionChange("optimize", e.target.checked)}
            />
            {t("Optimize design space")}
          </label>
          <label className="af-check">
            <input
              type="checkbox"
              checked={p.runOptions.compareBaseline}
              onChange={(e) => p.onRunOptionChange("compareBaseline", e.target.checked)}
            />
            {t("Compare against baseline design")}
          </label>
          <label className="af-check">
            <input
              type="checkbox"
              checked={p.runOptions.writeOutputs}
              onChange={(e) => p.onRunOptionChange("writeOutputs", e.target.checked)}
            />
            {t("Write output files (JSON, airfoil .dat)")}
          </label>
          <label
            className="af-check"
            title={
              "Runs the SUAVE mission, MSES 2-D and wingbox structural analyses concurrently once the " +
              "aerodynamic analysis finishes, instead of one after another. The aerodynamic analysis itself " +
              "always runs first and can't be parallelized alongside them -- it's what all three depend on."
            }
          >
            <input
              type="checkbox"
              checked={p.runOptions.parallel}
              onChange={(e) => p.onRunOptionChange("parallel", e.target.checked)}
            />
            Run stages in parallel (faster)
          </label>
        </div>
      </Card>
    </div>
  );
}
