// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

// Setup > Analyses: one place to choose which analysis disciplines a Run
// performs. The toggles write the same config flags the Advanced Settings
// pages own (mission.enabled / mses.enabled / structures.enabled), so the
// two stay in sync automatically; the always-on rows are the pipeline's
// core stages, listed so the page honestly answers "what will a Run do?"
// rather than only showing what's switchable. All optional analyses are
// enabled by default (the config defaults).

type AnalysesScreenProps = {
  configValues: Record<string, any>;
  onGroupChange: (group: string) => (name: string, value: any) => void;
};

function AnalysisRow({
  title,
  desc,
  checked,
  onToggle,
  locked,
}: {
  title: string;
  desc: string;
  checked: boolean;
  onToggle?: (v: boolean) => void;
  locked?: boolean;
}) {
  return (
    <label
      className="af-card af-row"
      style={{ alignItems: "flex-start", gap: 12, cursor: locked ? "default" : "pointer" }}
      title={locked ? "Core pipeline stage — always runs" : undefined}
    >
      <input
        type="checkbox"
        checked={checked}
        disabled={locked}
        onChange={(e) => onToggle?.(e.target.checked)}
        style={{ marginTop: 3 }}
      />
      <span style={{ flex: 1 }}>
        <span style={{ display: "block", fontWeight: 600, fontSize: 13 }}>
          {title}
          {locked && (
            <span className="af-help" style={{ marginLeft: 8, fontWeight: 400 }}>
              always runs
            </span>
          )}
        </span>
        <span className="af-help" style={{ display: "block", marginTop: 3 }}>{desc}</span>
      </span>
    </label>
  );
}

export function AnalysesScreen({ configValues, onGroupChange }: AnalysesScreenProps) {
  const mission = configValues.mission ?? {};
  const mses = configValues.mses ?? {};
  const structures = configValues.structures ?? {};

  return (
    <div className="af-stack" style={{ maxWidth: "min(1000px, 92%)" }}>
      <div className="af-page-header">
        <div>
          <h2 className="af-page-title">Analyses</h2>
          <div className="af-page-desc">
            Choose which analysis disciplines a Run performs. Everything is on by default; disabling an optional
            discipline skips its pipeline stage (its Results tab will read "Not available for this run"). Fine-grained
            settings for each live under Advanced Settings; external-tool paths under Setup ▸ External Tools.
          </div>
        </div>
      </div>

      <div className="af-help" style={{ fontWeight: 600 }}>Core (every run)</div>
      <AnalysisRow
        locked
        checked
        title="Aerodynamics — VLM + drag build-up"
        desc="AeroSandbox vortex-lattice analysis, drag polar, span loading, V-n envelope and dynamic modes of the optimized design. The pipeline's backbone; cannot be skipped."
      />
      <AnalysisRow
        locked
        checked
        title="Weight & Balance / Stability"
        desc="Torenbeek component masses, CG solve and envelope, static margin, landing-gear placement and cabin/payload layout."
      />
      <AnalysisRow
        locked
        checked
        title="Propulsion cycle"
        desc="On-design turbofan cycle analysis of the selected engine (Engine Designer settings) — cycle summary, carpet plot, efficiency decomposition and sweeps on the Propulsion results tab."
      />
      <AnalysisRow
        locked
        checked
        title="Field performance"
        desc="V-speeds, balanced field length and landing distances for the selected departure/arrival airports — the Matching Chart and Landing & Take-Off results tabs."
      />

      <div className="af-help" style={{ fontWeight: 600, marginTop: 6 }}>Optional (toggle per run)</div>
      <AnalysisRow
        checked={mission.enabled !== false}
        onToggle={(v) => onGroupChange("mission")("enabled", v)}
        title="Mission analysis — SUAVE"
        desc="Flies the full route (climb/cruise/descent) in SUAVE: fuel burn, flight profile, aero coefficient histories and the 3D route globe. Needs the SUAVE environment provisioned; degrades gracefully when missing."
      />
      <AnalysisRow
        checked={mses.enabled !== false}
        onToggle={(v) => onGroupChange("mses")("enabled", v)}
        title="2-D airfoil analysis — MSES"
        desc="High-fidelity viscous/transonic polar and pressure/Mach-contour analysis of the optimized root section (Drela's MSES). Sweep settings under Advanced Settings ▸ MSES Analysis."
      />
      <AnalysisRow
        checked={structures.enabled !== false}
        onToggle={(v) => onGroupChange("structures")("enabled", v)}
        title="Structures — wingbox FEM"
        desc="Sizes a generic wingbox (skin/spars/ribs) from strength requirements — always available analytically; add a NASTRAN path under External Tools for a real solve, plus optional Patran export."
      />
    </div>
  );
}
