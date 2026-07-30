// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { useCallback, useEffect, useState } from "react";
import { BrowserOpenURL } from "../../wailsjs/runtime/runtime";
import { sidecarGet, sidecarPost } from "../lib/sidecarClient";

// Setup > External Tools: a single hub for the integration paths/credentials
// (SimBrief username, NASTRAN/Patran executable paths, MSES install dir) that
// the Structures / Mission Analysis / MSES Advanced Settings pages otherwise
// leave scattered across their own forms -- see App.tsx's `mergeSetupPaths`/
// persistence effect for how these survive an app restart (localStorage, same
// pattern as theme/font). Each card links out to where the tool itself can be
// downloaded/licensed: none of these ship with ALAS, so every one of
// them has to be provisioned by the user before the feature it drives works.

function openLink(url: string) {
  try {
    BrowserOpenURL(url);
  } catch {
    window.open(url, "_blank");
  }
}

function Card({ title, link, linkLabel, children }: { title: string; link: string; linkLabel: string; children: React.ReactNode }) {
  return (
    <div className="af-card">
      <div className="af-card-title">{title}</div>
      {children}
      <div className="af-help" style={{ marginTop: 8 }}>
        <span className="af-expand" onClick={() => openLink(link)}>
          {linkLabel} ↗
        </span>
      </div>
    </div>
  );
}

type AssetInfo = {
  available: boolean;
  detail: string;
  path: string;
  licence: string;
  purpose: string;
};

type AssetsStatus = { navdata: AssetInfo; texture: AssetInfo };

// The navdata is GPL-3.0, so it cannot be distributed with ALAS. A
// packaged install has no scripts/ directory and no Python interpreter of its
// own, which would leave airway routing permanently unreachable without this.
function NavdataCard() {
  const [status, setStatus] = useState<AssetsStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setStatus(await sidecarGet<AssetsStatus>("/assets/status"));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const download = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const result = await sidecarPost<{ ok: boolean; error?: string } & AssetsStatus>(
        "/assets/navdata",
        {},
      );
      setStatus({ navdata: result.navdata, texture: result.texture });
      if (!result.ok) setError(result.error ?? "Download failed");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  const navdata = status?.navdata;

  return (
    <Card
      title="Navigation data"
      link="https://github.com/mcantsin/x-plane-navdata"
      linkLabel="open-navdata (GitHub)"
    >
      <div className="af-help" style={{ marginTop: 0, marginBottom: 8 }}>
        Real waypoint and airway routing. Without it, mission routes are drawn as great circles.
        This data is licensed <strong>GPL-3.0</strong> by the X-Plane project, so it is not
        distributed with ALAS -- downloading it is your choice to make.
      </div>
      <div className="af-form-row">
        <span className="af-form-label">Status</span>
        <span>
          {navdata ? (navdata.available ? `Installed -- ${navdata.detail}` : navdata.detail) : "checking..."}
        </span>
      </div>
      <button
        className="af-btn-primary"
        disabled={busy}
        onClick={() => void download()}
        style={{ marginTop: 8 }}
      >
        {busy ? "Downloading..." : navdata?.available ? "Re-download (~10 MB)" : "Download (~10 MB)"}
      </button>
      {error && (
        <div className="af-help" style={{ marginTop: 8 }}>
          {error}
        </div>
      )}
    </Card>
  );
}

type SetupScreenProps = {
  configValues: Record<string, any>;
  onGroupChange: (group: string) => (name: string, value: any) => void;
};

export function SetupScreen({ configValues, onGroupChange }: SetupScreenProps) {
  const mission = configValues.mission ?? {};
  const structures = configValues.structures ?? {};
  const mses = configValues.mses ?? {};
  const onMission = onGroupChange("mission");
  const onStructures = onGroupChange("structures");
  const onMses = onGroupChange("mses");

  return (
    <div className="af-stack" style={{ maxWidth: "min(1200px, 92%)" }}>
      <div className="af-page-header">
        <div>
          <h2 className="af-page-title">External Tools</h2>
          <div className="af-page-desc">
            Integration paths and credentials for the optional external tools ALAS can drive during a run --
            consolidated here instead of split across their own Advanced Settings pages. Set once; these are
            remembered across app launches on this machine.
          </div>
        </div>
      </div>

      <Card title="SimBrief" link="https://www.simbrief.com/" linkLabel="Open SimBrief">
        <div className="af-form-row" title="When set, a matching SimBrief flight plan (by origin/destination) overrides the Route card's airports.">
          <span className="af-form-label">SimBrief username</span>
          <input
            className="af-input"
            type="text"
            value={mission.simbrief_username ?? ""}
            onChange={(e) => onMission("simbrief_username", e.target.value)}
            placeholder="your SimBrief pilot ID"
          />
        </div>
        <div className="af-form-row">
          <span className="af-form-label">Fetch timeout</span>
          <input
            className="af-input"
            type="number"
            value={mission.simbrief_timeout_s ?? 15}
            onChange={(e) => onMission("simbrief_timeout_s", Number(e.target.value))}
          />
        </div>
      </Card>

      <NavdataCard />

      <Card title="NASTRAN" link="https://hexagon.com/" linkLabel="Get MSC/Hexagon Nastran">
        <div className="af-form-row">
          <span className="af-form-label">Executable path</span>
          <input
            className="af-input"
            type="text"
            value={structures.nastran_exe_path ?? ""}
            onChange={(e) => onStructures("nastran_exe_path", e.target.value)}
            placeholder="C:\\Program Files\\...\\nastran.exe"
          />
        </div>
        <label className="af-check" style={{ marginTop: 6 }}>
          <input
            type="checkbox"
            checked={!!structures.run_nastran}
            onChange={(e) => onStructures("run_nastran", e.target.checked)}
          />
          Run a real NASTRAN solve (otherwise the always-available analytical wingbox is used)
        </label>
      </Card>

      <Card title="Patran" link="https://hexagon.com/" linkLabel="Get MSC/Hexagon Patran">
        <div className="af-form-row">
          <span className="af-form-label">Executable path</span>
          <input
            className="af-input"
            type="text"
            value={structures.patran_exe_path ?? ""}
            onChange={(e) => onStructures("patran_exe_path", e.target.value)}
            placeholder="C:\\Program Files\\...\\patran.exe"
          />
        </div>
        <label className="af-check" style={{ marginTop: 6 }}>
          <input
            type="checkbox"
            checked={!!structures.run_patran_export}
            onChange={(e) => onStructures("run_patran_export", e.target.checked)}
          />
          Export/run Patran (requires NASTRAN above to also be enabled)
        </label>
      </Card>

      <Card title="MSES" link="https://web.mit.edu/drela/Public/web/mses/" linkLabel="Get MSES (Drela, MIT)">
        <div className="af-form-row">
          <span className="af-form-label">Install directory</span>
          <input
            className="af-input"
            type="text"
            value={mses.mses_dir ?? ""}
            onChange={(e) => onMses("mses_dir", e.target.value)}
            placeholder="external tools/MSES"
          />
        </div>
        <label className="af-check" style={{ marginTop: 6 }}>
          <input
            type="checkbox"
            checked={!!mses.enabled}
            onChange={(e) => onMses("enabled", e.target.checked)}
          />
          Run MSES 2-D airfoil analysis
        </label>
      </Card>

      <Card title="AeroSandbox" link="https://github.com/peterdsharpe/AeroSandbox" linkLabel="AeroSandbox (GitHub)">
        <div className="af-help" style={{ marginTop: 0 }}>
          ALAS's core aerodynamic & structural analysis engine.
        </div>
      </Card>

      <Card title="SUAVE" link="https://github.com/suavecode/SUAVE" linkLabel="SUAVE (GitHub)">
        <div className="af-help" style={{ marginTop: 0, marginBottom: 8 }}>
          Mission simulation and trajectory analysis for aircraft performance. SUAVE needs its own isolated Python
          environment (see <code>scripts/provision_suave_venv.py</code>) -- the defaults below only work from a source
          checkout. In a packaged/installed build, both paths below must be set to absolute paths on this machine
          (copy the venv + <code>external tools/suave_runner</code> folder here first), or mission analysis will
          report "not configured" and no mission figures will be produced.
        </div>
        <div className="af-form-row">
          <span className="af-form-label">SUAVE venv directory</span>
          <input
            className="af-input"
            type="text"
            value={mission.suave_venv_dir ?? ""}
            onChange={(e) => onMission("suave_venv_dir", e.target.value)}
            placeholder=".suave-venv (or an absolute path)"
          />
        </div>
        <div className="af-form-row">
          <span className="af-form-label">SUAVE runner script directory</span>
          <input
            className="af-input"
            type="text"
            value={mission.suave_runner_dir ?? ""}
            onChange={(e) => onMission("suave_runner_dir", e.target.value)}
            placeholder="external tools/suave_runner (or an absolute path)"
          />
        </div>
      </Card>
    </div>
  );
}
