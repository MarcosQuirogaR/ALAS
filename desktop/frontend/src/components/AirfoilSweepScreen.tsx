// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { useState } from "react";
import { sidecarFetchText } from "../lib/sidecarClient";
import { StatTile } from "./StatTile";
import { FigureCard } from "./FigureCard";
import { HelpHover } from "./InfoTip";
import { HowItWorks } from "./HowItWorks";
import type { ThemeName } from "./MenuBar";

// Optional tool: screens every airfoil in the ~1600-entry database
// (alas/geometry/airfoils.py's AirfoilLibrary) against the current
// design's cruise condition via alas/analysis/airfoil_screening.py in up
// to three fidelity stages -- a fast 2-D NeuralFoil proxy over the whole
// database, a real 3-D re-simulation (VLM + Raymer/Korn drag) of the shortlist
// on this design's actual wing, then optional MSES verification of the finalists.
//
// This is a *controlled* component: the run lifecycle (runId/result/running/
// status/error) and the options object live in App.tsx and are passed in as
// props, so switching tabs mid-run doesn't unmount this component and lose
// track of it -- a lost run would keep churning in the sidecar's daemon
// thread with nothing to cancel it, and a second Run could silently stack on
// top of it. See App.tsx's runAirfoilSweep/cancelAirfoilSweep.

export type Candidate = {
  name: string;
  status: string;
  error?: string | null;
  l_over_d?: number | null;
  cl?: number | null;
  cd?: number | null;
  alpha_deg?: number | null;
  max_thickness_frac?: number | null;
  tank_volume_m3?: number | null;
  tank_capacity_kg?: number | null;
  score?: number | null;
  robustness?: number | null;
  refined?: boolean;
  l_over_d_3d?: number | null;
  cd_3d?: number | null;
  alpha_3d_deg?: number | null;
  cm_residual_3d?: number | null;
  static_margin_3d?: number | null;
  score_3d?: number | null;
  mses_verified?: boolean;
  l_over_d_mses?: number | null;
  cd_mses?: number | null;
  cdw_mses?: number | null;
  mses_status?: string | null;
  mses_error?: string | null;
  is_reference?: boolean;
};

export type SweepResult = {
  baseline_airfoil: string;
  cruise_mach: number;
  cruise_reynolds: number;
  cruise_altitude_m: number;
  cl_target: number;
  transonic_caveat: boolean;
  refined_3d: boolean;
  n_total: number;
  n_ok: number;
  n_error: number;
  n_refined: number;
  n_mses_verified: number;
  cancelled?: boolean;
  candidates: Candidate[];
  errors: { name: string; error: string }[];
};

// The full option set, lifted to App.tsx so it survives tab switches and stays
// stable for an in-flight run. Field names mirror the POST /airfoil-sweep/run
// request (snake_case is applied in App.tsx's runAirfoilSweep).
export type SweepOptions = {
  ldWeight: number;
  fuelWeight: number;
  robustnessWeight: number;
  clBand: number;
  topN: number;
  modelSize: string;
  alphaMin: number;
  alphaMax: number;
  alphaStep: number;
  minTc: number;
  maxTc: number;
  nameFilter: string;
  refine3d: boolean;
  refineTopN: number;
  minStaticMargin: number | null;
  verifyMses: boolean;
  msesTopN: number;
};

export const DEFAULT_SWEEP_OPTIONS: SweepOptions = {
  ldWeight: 0.7,
  fuelWeight: 0.3,
  robustnessWeight: 0.0,
  clBand: 0.05,
  topN: 30,
  modelSize: "large",
  alphaMin: -4.0,
  alphaMax: 14.0,
  alphaStep: 0.5,
  minTc: 0.005,
  maxTc: 0.25,
  nameFilter: "",
  refine3d: true,
  refineTopN: 20,
  minStaticMargin: null,
  verifyMses: true,
  msesTopN: 5,
};

const MODEL_SIZES = ["small", "medium", "large", "xlarge", "xxlarge"];

const SWEEP_FIGURES: { name: string; title: string }[] = [
  { name: "trade_map", title: "Trade map — L/D vs fuel capacity" },
  { name: "rerank_2d_3d", title: "2-D shortlist reshuffled in 3-D" },
  { name: "ranking_bars", title: "Top airfoils for this design" },
  { name: "mses_verification", title: "MSES verification — real shock/viscous effects" },
  { name: "section_shapes", title: "Section shapes — top picks vs current" },
];

function bestLd(c: Candidate): { value: number | null; source: string } {
  if (c.mses_verified && c.l_over_d_mses != null) return { value: c.l_over_d_mses, source: "MSES" };
  if (c.refined && c.l_over_d_3d != null) return { value: c.l_over_d_3d, source: "3-D wing" };
  if (c.l_over_d != null) return { value: c.l_over_d, source: "2-D proxy" };
  return { value: null, source: "" };
}

// One labelled option row. The label itself is the hover-help target (no ⓘ
// icon), matching how every auto-generated config field behaves.
function OptRow({ label, help, children }: { label: string; help: React.ReactNode; children: React.ReactNode }) {
  return (
    <div className="af-form-row">
      <span className="af-form-label">
        <HelpHover heading={label} content={help} className="af-label-text">
          {label}
        </HelpHover>
      </span>
      {children}
    </div>
  );
}

export function AirfoilSweepScreen({
  options,
  onOptionsChange,
  running,
  status,
  result,
  runId,
  error,
  onRun,
  onCancel,
  theme,
}: {
  options: SweepOptions;
  onOptionsChange: (patch: Partial<SweepOptions>) => void;
  running: boolean;
  status: string;
  result: SweepResult | null;
  runId: string | null;
  error: string | null;
  onRun: () => void;
  onCancel: () => void;
  theme: ThemeName;
}) {
  const [selectedCandidate, setSelectedCandidate] = useState<string | null>(null);
  const o = options;
  const set = onOptionsChange;

  const top = result?.candidates?.[0];
  const topLd = top ? bestLd(top) : null;
  // Every candidate with a real MSES solve, best MSES L/D first -- each gets
  // its own Cp + Mach-contour figures below.
  const msesVerified = (result?.candidates ?? [])
    .filter((c) => c.mses_verified)
    .sort((a, b) => (b.l_over_d_mses ?? 0) - (a.l_over_d_mses ?? 0));

  return (
    <div className="af-stack" style={{ maxWidth: "min(1400px, 92%)" }}>
      <div className="af-page-header">
        <div>
          <h2 className="af-page-title">Airfoil Screening</h2>
          <div className="af-page-desc">
            Ranks every airfoil in the database (~1600 sections) against <em>this design's own</em> cruise
            condition — target CL, Mach and Reynolds derived from your MTOW, span and root chord, so no Run is
            needed — then recommends the best fits. Set the objective and filters below; open{" "}
            <strong>How this works</strong> for the full three-stage method and every parameter.
          </div>
        </div>
      </div>

      <HowItWorks title="How airfoil screening works (3 stages + every parameter)">
        <p>
          <strong>Stage 1 — 2-D proxy (whole database).</strong> A fast NeuralFoil forward pass scores every
          section at the design CL, at the swept-section effective Mach (M·cos Λ) and chord Reynolds. Cheap enough
          to sweep all ~1600 entries in seconds, but on its own it flatters thin low-Reynolds sections that win an
          isolated 2-D polar yet wouldn't suit this aircraft.
        </p>
        <p>
          <strong>Stage 2 — real 3-D wing.</strong> The top <em>Re-simulate top</em> survivors are rebuilt into
          your actual wing and given a real closed-form trim solve (VLM induced drag + Raymer parasite + Korn wave
          drag — the same rigour the app's trusted reported numbers use). A candidate whose trim solve can't
          sustain the required cruise CL (L must equal Weight) is excluded outright, not just down-ranked.
        </p>
        <p>
          <strong>Stage 3 — MSES (finalists).</strong> The top <em>Verify top</em> Stage-2 survivors get a real
          MSES coupled viscous/inviscid solve — the only stage that captures shocks and true wave drag, which is
          what lets a genuinely supercritical section's transonic advantage show up. Needs MSES enabled under
          Setup ▸ External Tools; skipped silently otherwise.
        </p>
        <p>
          <strong>Scoring.</strong> Survivors are ranked by a min-max-normalized blend of cruise L/D and the
          resulting wing's fuel-tank capacity (your weights), optionally plus off-design robustness. A curated set
          of real wind-tunnel-validated transonic sections (marked ★ — the NASA SC(2) family, the Whitcomb
          airfoil, RAE 2822) is always carried through every stage as a known-good physical anchor.
        </p>
        <p>
          <strong>Parameters.</strong> <em>L/D vs Fuel-volume weight</em> trade cruise efficiency against tank
          volume. <em>Off-design robustness</em> (+ CL band) rewards a flat drag bucket. <em>Thickness window</em>{" "}
          restricts t/c to a structurally/volumetrically realistic band. <em>Static-margin floor</em> demotes any
          section whose swap would leave the aircraft too weakly stable in pitch. <em>Name/family filter</em>{" "}
          scopes the sweep (e.g. <code>sc2, naca23</code>). <em>Model size</em> and the <em>alpha sweep</em> trade
          NeuralFoil accuracy against speed.
        </p>
      </HowItWorks>

      <div className="af-card">
        <div className="af-card-title">Options</div>

        <div className="af-opt-group">
          <div className="af-opt-legend">Objective — how survivors are ranked</div>
          <OptRow label="L/D weight" help="How much ranking weight goes to cruise lift-to-drag ratio. Higher favours slicker, lower-drag sections.">
            <input className="af-input" type="number" step={0.05} min={0} max={1} value={o.ldWeight}
              onChange={(e) => set({ ldWeight: Number(e.target.value) })} />
          </OptRow>
          <OptRow label="Fuel-volume weight" help="How much ranking weight goes to the resulting wing's fuel-tank capacity. Higher favours thicker sections with more internal volume.">
            <input className="af-input" type="number" step={0.05} min={0} max={1} value={o.fuelWeight}
              onChange={(e) => set({ fuelWeight: Number(e.target.value) })} />
          </OptRow>
          <OptRow label="Off-design robustness weight" help="Reward sections whose L/D holds across CL ± the band below (a flat drag bucket), not just at exactly the design CL. 0 disables it — the default, matching the classic L/D+fuel ranking.">
            <input className="af-input" type="number" step={0.05} min={0} max={1} value={o.robustnessWeight}
              onChange={(e) => set({ robustnessWeight: Number(e.target.value) })} />
          </OptRow>
          {o.robustnessWeight > 0 && (
            <OptRow label="CL band (±)" help="Half-width of the CL interval used for the robustness metric: L/D is sampled at the design CL and at ± this amount. ~0.05 covers realistic weight/altitude-driven CL variation in cruise.">
              <input className="af-input" type="number" step={0.01} min={0.01} max={0.3} value={o.clBand}
                onChange={(e) => set({ clBand: Number(e.target.value) })} />
            </OptRow>
          )}
        </div>

        <div className="af-opt-group">
          <div className="af-opt-legend">Filters — which sections qualify</div>
          <OptRow label="Name / family filter" help="Restrict the sweep to matching names. Comma-separated substrings or globs, e.g. 'sc2, naca23' or 'e*'. Blank = the whole database. Focuses the comparison and speeds the run.">
            <input className="af-input" type="text" placeholder="all sections" value={o.nameFilter}
              onChange={(e) => set({ nameFilter: e.target.value })} />
          </OptRow>
          <OptRow label="Thickness min (t/c)" help="Lower bound of the thickness window. Sections thinner than this are excluded (too little spar depth / fuel volume). 0.005 keeps every real section.">
            <input className="af-input" type="number" step={0.01} min={0.005} max={0.3} value={o.minTc}
              onChange={(e) => set({ minTc: Number(e.target.value) })} />
          </OptRow>
          <OptRow label="Thickness max (t/c)" help="Upper bound of the thickness window. Sections thicker than this are excluded (excess wave drag). A transport-like band is roughly 0.10–0.16; 0.25 keeps everything realistic.">
            <input className="af-input" type="number" step={0.01} min={0.02} max={0.3} value={o.maxTc}
              onChange={(e) => set({ maxTc: Number(e.target.value) })} />
          </OptRow>
          <OptRow label="Enforce static-margin floor" help="When on, a Stage-2 candidate whose real trim solve leaves static margin below the floor is demoted — so a high-L/D but destabilizing section can't top the ranking. Off by default (static margin is only displayed).">
            <label className="af-check">
              <input type="checkbox" checked={o.minStaticMargin !== null}
                onChange={(e) => set({ minStaticMargin: e.target.checked ? 0.05 : null })} />
            </label>
          </OptRow>
          {o.minStaticMargin !== null && (
            <OptRow label="Static-margin floor (%)" help="Minimum acceptable trimmed static margin, in % MAC. Candidates below this are demoted with a reason. 5% is a common conservative transport floor.">
              <input className="af-input" type="number" step={1} min={-10} max={40}
                value={(o.minStaticMargin * 100).toFixed(0)}
                onChange={(e) => set({ minStaticMargin: Number(e.target.value) / 100 })} />
            </OptRow>
          )}
        </div>

        <div className="af-opt-group">
          <div className="af-opt-legend">Fidelity — accuracy vs speed</div>
          <OptRow label="NeuralFoil model size" help="Accuracy/speed of the Stage-1 2-D proxy. Larger models are more accurate but slower per section; 'large' is a good default for a full-database sweep.">
            <select className="af-input" value={o.modelSize} onChange={(e) => set({ modelSize: e.target.value })}>
              {MODEL_SIZES.map((m) => (
                <option key={m} value={m}>{m}</option>
              ))}
            </select>
          </OptRow>
          <OptRow label="Alpha sweep min (deg)" help="Lowest angle of attack in the Stage-1 polar. The design CL must fall within the swept range, or a section is excluded as out-of-range.">
            <input className="af-input" type="number" step={1} min={-12} max={0} value={o.alphaMin}
              onChange={(e) => set({ alphaMin: Number(e.target.value) })} />
          </OptRow>
          <OptRow label="Alpha sweep max (deg)" help="Highest angle of attack in the Stage-1 polar. Raise it if high-CL designs report 'target CL outside swept range'.">
            <input className="af-input" type="number" step={1} min={4} max={24} value={o.alphaMax}
              onChange={(e) => set({ alphaMax: Number(e.target.value) })} />
          </OptRow>
          <OptRow label="Alpha sweep step (deg)" help="Angle-of-attack resolution of the Stage-1 polar. Finer steps interpolate CD at the target CL more precisely but cost more NeuralFoil evaluations per section.">
            <input className="af-input" type="number" step={0.1} min={0.1} max={2} value={o.alphaStep}
              onChange={(e) => set({ alphaStep: Number(e.target.value) })} />
          </OptRow>
        </div>

        <div className="af-opt-group">
          <div className="af-opt-legend">Stages — how deep to go</div>
          <OptRow label="Show top" help="How many ranked survivors to list in the results table.">
            <input className="af-input" type="number" step={5} min={5} max={200} value={o.topN}
              onChange={(e) => set({ topN: Number(e.target.value) })} />
          </OptRow>
          <OptRow label="3-D wing re-rank" help="Re-simulate the shortlist as this design's real 3-D wing and re-rank by 3-D L/D. Adds a VLM evaluation per candidate but is what makes the ranking reflect THIS aircraft.">
            <label className="af-check">
              <input type="checkbox" checked={o.refine3d} onChange={(e) => set({ refine3d: e.target.checked })} />
            </label>
          </OptRow>
          {o.refine3d && (
            <OptRow label="Re-simulate top" help="How many of the 2-D shortlist to rebuild and re-evaluate as the real 3-D wing (Stage 2).">
              <input className="af-input" type="number" step={5} min={5} max={60} value={o.refineTopN}
                onChange={(e) => set({ refineTopN: Number(e.target.value) })} />
            </OptRow>
          )}
          <OptRow label="Verify with MSES" help="Verify the final few 3-D survivors with a real MSES coupled viscous/inviscid solve (shock capture, true wave drag). Slower per candidate (~10–30s each) but the only check that can reward a genuinely supercritical section. Needs MSES enabled under Setup ▸ External Tools.">
            <label className="af-check">
              <input type="checkbox" checked={o.verifyMses} onChange={(e) => set({ verifyMses: e.target.checked })} />
            </label>
          </OptRow>
          {o.verifyMses && (
            <OptRow label="Verify top" help="How many of the Stage-2 survivors to verify with MSES (Stage 3).">
              <input className="af-input" type="number" step={1} min={1} max={20} value={o.msesTopN}
                onChange={(e) => set({ msesTopN: Number(e.target.value) })} />
            </OptRow>
          )}
        </div>

        <div className="af-row" style={{ marginTop: 12, gap: 12 }}>
          <button className="af-btn-primary" onClick={onRun} disabled={running}>
            {running ? "Screening…" : "Run screening"}
          </button>
          {running && (
            <button onClick={onCancel} title="Stop the sweep and keep whatever was ranked so far">
              Cancel
            </button>
          )}
          <button
            onClick={() => onOptionsChange(DEFAULT_SWEEP_OPTIONS)}
            disabled={running}
            title="Restore every option to its default value"
          >
            Reset options
          </button>
          {running && <span className="af-help">{status}</span>}
        </div>
      </div>

      {error && (
        <ul className="af-issue-list">
          <li className="error">{error}</li>
        </ul>
      )}

      {result && (
        <div className="af-stack">
          {result.cancelled && (
            <div className="af-banner warn">
              Sweep cancelled — the results below are a partial ranking of whatever finished before you stopped it.
            </div>
          )}

          {top && topLd?.value != null && (
            <div className="af-card af-recommend">
              <div className="af-card-title">Recommended pick</div>
              <div className="af-row" style={{ gap: 18, flexWrap: "wrap", alignItems: "baseline" }}>
                <span style={{ fontSize: 20, fontWeight: 700 }}>
                  {top.is_reference ? "★ " : ""}
                  {top.name}
                  {top.name === result.baseline_airfoil ? " (current)" : ""}
                </span>
                <span className="af-help">
                  Cruise L/D {topLd.value.toFixed(1)} ({topLd.source})
                  {top.max_thickness_frac != null ? ` · t/c ${(top.max_thickness_frac * 100).toFixed(1)}%` : ""}
                  {top.tank_capacity_kg != null ? ` · tank ${top.tank_capacity_kg.toFixed(0)} kg` : ""}
                  {top.static_margin_3d != null ? ` · static margin ${(top.static_margin_3d * 100).toFixed(1)}%` : ""}
                </span>
              </div>
              <div className="af-help" style={{ marginTop: 6 }}>
                Best blended score for your objective. This is a shortlist head, not a final answer — verify it with
                a real Run (and MSES for a transonic design) before committing.
              </div>
            </div>
          )}

          <div className="af-row" style={{ flexWrap: "wrap", gap: 12 }}>
            <StatTile label="Screened" value={`${result.n_ok}/${result.n_total} usable`} />
            <StatTile label="Excluded" value={String(result.n_error)} />
            <StatTile label="Re-simulated in 3-D" value={result.refined_3d ? String(result.n_refined) : "off"} />
            <StatTile label="MSES-verified" value={String(result.n_mses_verified)} />
            <StatTile label="Cruise Mach" value={result.cruise_mach.toFixed(3)} />
            <StatTile label="Target CL" value={result.cl_target.toFixed(3)} />
            <StatTile label="Cruise Re" value={result.cruise_reynolds.toExponential(2)} />
            <StatTile label="Current root airfoil" value={result.baseline_airfoil} />
          </div>

          {result.transonic_caveat && (
            <div className="af-banner warn">
              Cruise Mach {result.cruise_mach.toFixed(2)} is transonic/high-subsonic. Neither NeuralFoil nor this
              app's VLM aerodynamic model captures wave drag from a truly supercritical section, so a real
              supercritical airfoil's transonic advantage may not fully show up in the 2-D/3-D ranking — the MSES
              stage is the one that can. Treat this as a starting shortlist and verify top candidates with a real
              Run before committing.
            </div>
          )}

          {runId && (
            <div className="af-figure-grid">
              {SWEEP_FIGURES.map((f) => (
                <FigureCard
                  key={f.name}
                  title={f.title}
                  deps={[runId, f.name, theme]}
                  loader={() => sidecarFetchText(`/airfoil-sweep/${runId}/figures/${f.name}?theme=${theme}`)}
                />
              ))}
            </div>
          )}

          <div style={{ overflowX: "auto" }}>
            <table className="af-table">
              <thead>
                <tr>
                  <th>#</th>
                  <th>Airfoil</th>
                  <th>Score</th>
                  <th title="Real MSES coupled viscous/inviscid L/D (Stage 3) -- captures shocks and true wave drag">
                    L/D (MSES)
                  </th>
                  <th title="Wave drag from MSES -- the shock/transonic-mismatch indicator, in drag counts">CDw</th>
                  <th title="Cruise L/D of this design's real 3-D wing with this section (real trim solve)">
                    L/D (3-D)
                  </th>
                  <th title="Isolated 2-D section L/D from the NeuralFoil proxy">L/D (2-D)</th>
                  <th title="Off-design robustness: L/D across CL ± band divided by at-target L/D. ~1.0 = a flat drag bucket.">
                    Robustness
                  </th>
                  <th title="Static margin at the real trim solve -- flags if this airfoil swap breaks pitch stability">
                    Static margin
                  </th>
                  <th>Alpha (deg)</th>
                  <th>t/c</th>
                  <th>Tank (kg)</th>
                </tr>
              </thead>
              <tbody>
                {result.candidates.map((c, i) => (
                  <tr
                    key={c.name}
                    className={selectedCandidate === c.name ? "af-row-selected" : undefined}
                    onClick={c.mses_verified ? () => setSelectedCandidate(c.name) : undefined}
                    style={c.mses_verified ? { cursor: "pointer" } : undefined}
                    title={c.mses_verified ? "Click to view this candidate's MSES Cp/Mach figures" : undefined}
                  >
                    <td>{i + 1}</td>
                    <td>
                      {c.is_reference && <span title="Real, wind-tunnel-validated reference section">★ </span>}
                      {c.name}
                      {c.name === result.baseline_airfoil ? " (current)" : ""}
                      {c.mses_verified ? " ·MSES" : c.refined ? "" : " ·2-D"}
                    </td>
                    <td>{(c.refined ? c.score_3d : c.score)?.toFixed(3) ?? "-"}</td>
                    <td title={c.mses_error ?? undefined}>{c.l_over_d_mses?.toFixed(2) ?? "-"}</td>
                    <td>{c.cdw_mses != null ? `${(c.cdw_mses * 1e4).toFixed(0)} cts` : "-"}</td>
                    <td>{c.l_over_d_3d?.toFixed(2) ?? "-"}</td>
                    <td>{c.l_over_d?.toFixed(2) ?? "-"}</td>
                    <td>{c.robustness != null ? c.robustness.toFixed(2) : "-"}</td>
                    <td>{c.static_margin_3d != null ? `${(c.static_margin_3d * 100).toFixed(1)}%` : "-"}</td>
                    <td>{(c.refined ? c.alpha_3d_deg : c.alpha_deg)?.toFixed(2) ?? "-"}</td>
                    <td>{c.max_thickness_frac != null ? `${(c.max_thickness_frac * 100).toFixed(1)}%` : "-"}</td>
                    <td>{c.tank_capacity_kg?.toFixed(0) ?? "-"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {result.n_mses_verified > 0 && (
            <div className="af-help">
              ★ = real reference section. Rows marked ·MSES were verified with a real viscous/inviscid solve — every
              one has its own surface Cp and Mach-contour figures below; click a row to jump it open.
            </div>
          )}

          {/* Every MSES-verified candidate gets its own real Cp/Mach-contour
              pair, not just one clicked row: these are the physically
              trustworthy shock/pressure results of the whole screen, and
              comparing them across candidates is the point. Collapsed into a
              <details> per candidate so a 20-candidate verification doesn't
              render 40 figures at once -- the top pick is open by default. */}
          {runId && msesVerified.length > 0 && (
            <div className="af-stack">
              <div className="af-card-title">MSES detail — surface Cp and shock structure</div>
              {msesVerified.map((c, i) => (
                <details key={c.name} className="af-mses-detail" open={i === 0 || selectedCandidate === c.name}>
                  <summary>
                    {c.is_reference ? "★ " : ""}
                    {c.name}
                    {c.name === result.baseline_airfoil ? " (current)" : ""}
                    <span className="af-help" style={{ marginLeft: 10 }}>
                      L/D {c.l_over_d_mses?.toFixed(1) ?? "-"}
                      {c.cdw_mses != null ? ` · CDw ${(c.cdw_mses * 1e4).toFixed(0)} cts` : ""}
                    </span>
                  </summary>
                  <div className="af-figure-grid" style={{ marginTop: 10 }}>
                    <FigureCard
                      title={`Surface Cp / Mach — ${c.name}`}
                      deps={[runId, c.name, "mses_pressure", theme]}
                      loader={() =>
                        sidecarFetchText(
                          `/airfoil-sweep/${runId}/candidates/${encodeURIComponent(c.name)}/figures/mses_pressure?theme=${theme}`
                        )
                      }
                    />
                    <FigureCard
                      title={`Mach contours (shock structure) — ${c.name}`}
                      deps={[runId, c.name, "mses_mach_contours", theme]}
                      loader={() =>
                        sidecarFetchText(
                          `/airfoil-sweep/${runId}/candidates/${encodeURIComponent(c.name)}/figures/mses_mach_contours?theme=${theme}`
                        )
                      }
                    />
                  </div>
                </details>
              ))}
            </div>
          )}

          {result.errors.length > 0 && (
            <details>
              <summary className="af-help" style={{ cursor: "pointer" }}>
                {result.n_error} airfoils excluded — show why
              </summary>
              <ul className="af-issue-list">
                {result.errors.map((e) => (
                  <li key={e.name}>
                    <strong>{e.name}</strong>: {e.error}
                  </li>
                ))}
              </ul>
            </details>
          )}
        </div>
      )}

      {!result && !running && !error && (
        <div className="af-placeholder" style={{ minHeight: 160 }}>
          <strong style={{ fontSize: 15 }}>No screening yet</strong>
          Set your objective and filters above, then press Run screening. You can switch tabs while it runs — the
          progress and results are kept.
        </div>
      )}
    </div>
  );
}
