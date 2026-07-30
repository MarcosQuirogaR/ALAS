// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import "./App.css";
import { sidecarGet, sidecarPost, sidecarEventSocket, retrySidecarStart, sidecarDownload } from "./lib/sidecarClient";
import { StorageDialog } from "./components/StorageDialog";

import { MenuBar, ThemeName } from "./components/MenuBar";
import { NavSidebar } from "./components/NavSidebar";
import { PreviewDock } from "./components/PreviewDock";
import { RunLog, LogLine } from "./components/RunLog";
import { ControlBar } from "./components/ControlBar";
import { InputsScreen, RunOptions } from "./components/InputsScreen";
import { FormPage } from "./components/FormPage";
import { DesignSpaceTable, DesignSpaceSpec, deriveInitialBounds } from "./components/DesignSpaceTable";
import { ResultsScreen, ResultSummary } from "./components/ResultsScreen";
import { SetupScreen } from "./components/SetupScreen";
import { AnalysesScreen } from "./components/AnalysesScreen";
import {
  AirfoilSweepScreen,
  DEFAULT_SWEEP_OPTIONS,
  SweepOptions,
  SweepResult,
} from "./components/AirfoilSweepScreen";
import { SchemaField } from "./components/DynamicForm";
import { Walkthrough, hasSeenWalkthrough } from "./components/Walkthrough";
import { AdvancedWalkthrough } from "./components/AdvancedWalkthrough";
import { Splash } from "./components/Splash";
import { HelpContext } from "./lib/helpContext";
import { ALL_PAGES } from "./pages";

type ValidationIssue = { field_path: string; message: string; severity: string };

const THEME_KEY = "alas.theme";
const FONT_LOCKED_ZOOM_KEY = "alas.fontLockedZoom";
const SETUP_PATHS_KEY = "alas.setup.paths";
const HELP_VERBOSE_KEY = "alas.helpVerbose";

// "Learn-more help" master switch (View menu), persisted like theme/zoom.
// When off, every ⓘ InfoTip and "How this works" deep-dive renders nothing --
// the in-app escape hatch for the denser-help presentation. Default on.
function loadHelpVerbose(): boolean {
  try {
    const v = localStorage.getItem(HELP_VERBOSE_KEY);
    return v === null ? true : v === "true";
  } catch {
    return true;
  }
}

// Setup > External Tools' fields, remembered across launches (localStorage,
// same pattern as theme/font above) since they're machine-local install
// paths/credentials, not part of an aircraft design a preset should own.
type SetupPaths = {
  mission?: {
    simbrief_username?: string;
    simbrief_timeout_s?: number;
    suave_venv_dir?: string;
    suave_runner_dir?: string;
  };
  structures?: { nastran_exe_path?: string; run_nastran?: boolean; patran_exe_path?: string; run_patran_export?: boolean };
  mses?: { mses_dir?: string; enabled?: boolean };
};

function loadSetupPaths(): SetupPaths {
  try {
    const raw = localStorage.getItem(SETUP_PATHS_KEY);
    return raw ? JSON.parse(raw) : {};
  } catch {
    return {};
  }
}

// Drop keys whose value is undefined or an empty/whitespace-only string, so a
// remembered-but-blank field can never overwrite a real config default.
// Critical for the tool-path fields: persisting "" for e.g. suave_venv_dir and
// re-applying it on every launch replaced the working ".suave-venv" default
// with "", which the backend resolved to the app root and reported as
// "mission: not_configured" even though SUAVE was bundled correctly. Blank
// means "not set", not "set to nothing".
function dropBlank<T extends Record<string, any>>(obj: T | undefined): Partial<T> {
  if (!obj) return {};
  const out: Record<string, any> = {};
  for (const [k, v] of Object.entries(obj)) {
    if (v === undefined) continue;
    if (typeof v === "string" && v.trim() === "") continue;
    out[k] = v;
  }
  return out as Partial<T>;
}

// Re-applied over a freshly-loaded default/preset config (both replace the
// mission/structures/mses groups wholesale), so a remembered NASTRAN path
// etc. survives switching aircraft presets instead of being clobbered by
// the preset's own (usually empty) values for these fields.
function mergeSetupPaths(config: Record<string, any>): Record<string, any> {
  const saved = loadSetupPaths();
  if (!saved.mission && !saved.structures && !saved.mses) return config;
  return {
    ...config,
    mission: { ...config.mission, ...dropBlank(saved.mission) },
    structures: { ...config.structures, ...dropBlank(saved.structures) },
    mses: { ...config.mses, ...dropBlank(saved.mses) },
  };
}

function loadTheme(): ThemeName {
  try {
    const t = localStorage.getItem(THEME_KEY);
    if (t === "dark" || t === "light" || t === "grey") return t;
  } catch {
    /* webview without storage -- fall through to default */
  }
  return "dark";
}

// The whole UI (fonts, padding, gaps -- not just text) scales as one unit
// via the CSS `zoom` property (supported by every Chromium-based webview,
// which is what Wails embeds), the same way a browser's Ctrl+/Ctrl- zoom
// does. A height/width-driven "auto-fit" heuristic can't be made reliable
// across window shapes (ultrawide vs. tall-narrow), so View > Zoom
// in/out/reset (below) is the only zoom control: a plain user-chosen
// multiplier that doesn't change on its own when the window resizes.
const ZOOM_MIN = 0.75;
const ZOOM_MAX = 2.2;

function loadLockedZoom(): number {
  try {
    const v = Number(localStorage.getItem(FONT_LOCKED_ZOOM_KEY));
    if (Number.isFinite(v) && v > 0) return Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, v));
  } catch {
    /* no-op */
  }
  return 1.0;
}

function App() {
  // ---- Theme -------------------------------------------------------------
  const [theme, setTheme] = useState<ThemeName>(loadTheme);
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    try {
      localStorage.setItem(THEME_KEY, theme);
    } catch {
      /* no-op */
    }
  }, [theme]);

  // ---- Font/zoom scaling ---------------------------------------------------
  const [lockedZoom, setLockedZoom] = useState<number>(loadLockedZoom);

  useEffect(() => {
    (document.body.style as any).zoom = lockedZoom;
    try {
      localStorage.setItem(FONT_LOCKED_ZOOM_KEY, String(lockedZoom));
    } catch {
      /* no-op */
    }
  }, [lockedZoom]);

  // ---- Learn-more help toggle -------------------------------------------
  const [helpVerbose, setHelpVerbose] = useState<boolean>(loadHelpVerbose);
  useEffect(() => {
    try {
      localStorage.setItem(HELP_VERBOSE_KEY, String(helpVerbose));
    } catch {
      /* no-op */
    }
  }, [helpVerbose]);

  // ---- Config / schema state --------------------------------------------
  const [schemaFields, setSchemaFields] = useState<SchemaField[]>([]);
  const [configValues, setConfigValues] = useState<Record<string, any>>({});
  const [designVector, setDesignVector] = useState<Record<string, number> | null>(null);
  const [designSpecs, setDesignSpecs] = useState<DesignSpaceSpec[]>([]);
  const [presetNames, setPresetNames] = useState<string[]>([]);
  const [presetDisplay, setPresetDisplay] = useState<Record<string, string>>({});
  const [selectedPreset, setSelectedPreset] = useState<string>("");
  // Aux presets (Py6-era analysis-fidelity/solver/performance pickers, one
  // per relevant Advanced page) -- each only fills in a single config group
  // (or, for "solver", one nested sub-group), unlike the aircraft preset
  // above which replaces the whole config.
  const [auxPresetNames, setAuxPresetNames] = useState<Record<string, string[]>>({});
  const [auxPresetDisplay, setAuxPresetDisplay] = useState<Record<string, Record<string, string>>>({});
  const [selectedAuxPreset, setSelectedAuxPreset] = useState<Record<string, string>>({});
  const [engineNames, setEngineNames] = useState<string[]>([]);
  const [selectedEngine, setSelectedEngine] = useState<string>("");
  const [airportNames, setAirportNames] = useState<string[]>([]);

  const [initialDesign, setInitialDesign] = useState<Record<string, number>>({});
  const [bounds, setBounds] = useState<[number, number][]>([]);
  const [issues, setIssues] = useState<ValidationIssue[]>([]);

  const [runOptions, setRunOptions] = useState<RunOptions>({
    optimize: true,
    compareBaseline: true,
    writeOutputs: true,
    parallel: true,
  });

  // ---- UI / run state ----------------------------------------------------
  const [previewOpen, setPreviewOpen] = useState(true);
  const [activePage, setActivePage] = useState<string>("inputs");
  const [log, setLog] = useState<LogLine[]>([]);
  const [running, setRunning] = useState(false);
  const [status, setStatus] = useState("Ready.");
  const [stage, setStage] = useState("");
  const [elapsedMs, setElapsedMs] = useState(0);
  const [runId, setRunId] = useState<string | null>(null);
  const [summary, setSummary] = useState<ResultSummary | null>(null);
  const [showTour, setShowTour] = useState(false);
  const [showGuide, setShowGuide] = useState(false);

  // ---- Airfoil-screening sweep (lifted out of AirfoilSweepScreen) --------
  // Held here, not in the component, so navigating away and back mid-sweep
  // doesn't discard the run: component-local state would unmount on
  // navigation while the sidecar's daemon-thread sweep keeps churning
  // invisibly, and pressing Run again would stack a second sweep on top.
  const [sweepOptions, setSweepOptions] = useState<SweepOptions>(DEFAULT_SWEEP_OPTIONS);
  const [sweepRunning, setSweepRunning] = useState(false);
  const [sweepStatus, setSweepStatus] = useState("");
  const [sweepResult, setSweepResult] = useState<SweepResult | null>(null);
  const [sweepRunId, setSweepRunId] = useState<string | null>(null);
  const [sweepError, setSweepError] = useState<string | null>(null);
  // Boot splash: covers the window from first paint until the sidecar
  // answers the initial load, so a fresh .exe launch immediately shows the
  // logo + a live status instead of an inert window (see Splash.tsx).
  const [boot, setBoot] = useState<"loading" | "error" | "done">("loading");
  const [bootError, setBootError] = useState<string | null>(null);

  // First-run onboarding walkthrough (port of onboarding.py's "seen" flag).
  // Gated on `boot === "done"` (not just mount) -- the walkthrough's first
  // real step navigates to pages and spotlights selectors like
  // `[data-tour='aircraft-config']` that only exist once schemaFields/
  // presets/etc. have actually loaded from the sidecar. A cold sidecar
  // start can take well over a minute (see sidecarClient.ts), so the old
  // fixed 900ms-after-mount timer routinely fired while the Splash screen
  // was still up and the rest of the app hadn't rendered real content yet,
  // spotlighting nothing (or navigating the still-loading shell) and
  // leaving the tour visibly broken.
  useEffect(() => {
    if (boot === "done" && !hasSeenWalkthrough()) {
      const t = setTimeout(() => setShowTour(true), 900);
      return () => clearTimeout(t);
    }
  }, [boot]);

  // Cap retained log lines: a long optimization emits one line per progress
  // update, and an unbounded array re-renders the whole RunLog on every
  // append -- keep the most recent window instead (nobody scrolls back
  // through thousands of superseded progress lines).
  const MAX_LOG_LINES = 1500;
  const appendLog = (text: string, kind: LogLine["kind"] = "info") =>
    setLog((prev) => {
      const next = [...prev, { text, kind }];
      return next.length > MAX_LOG_LINES ? next.slice(next.length - MAX_LOG_LINES) : next;
    });

  // ---- Initial load ------------------------------------------------------
  // Cold sidecar startup (see desktop/sidecar.go) can legitimately take up
  // to a couple of minutes -- pulled out of the mount-only useEffect so a
  // failed attempt (timeout, or a real transient error) can be retried from
  // the UI without relaunching the whole app.
  const initFromSidecar = useCallback(async () => {
    setStatus("Connecting to sidecar...");
    setBoot("loading");
    setBootError(null);
    try {
      const [schema, defaults, presets, specs, airports] = await Promise.all([
        sidecarGet<{ fields: SchemaField[] }>("/schema"),
        sidecarGet<{ config: Record<string, any>; design_vector: Record<string, number> }>("/config/default"),
        sidecarGet<{ names: string[]; display_names?: Record<string, string> }>("/config/presets"),
        sidecarGet<{ specs: DesignSpaceSpec[] }>("/design-space/specs"),
        sidecarGet<{ names: string[] }>("/airports"),
      ]);
      setSchemaFields(schema.fields);
      setConfigValues(mergeSetupPaths(defaults.config));
      setDesignVector(defaults.design_vector);
      setDesignSpecs(specs.specs);
      setPresetNames(presets.names);
      setPresetDisplay(presets.display_names ?? {});
      setSelectedEngine(defaults.config?.geometry?.engine?.engine_name ?? "");
      setAirportNames(airports.names);
      appendLog("Sidecar ready.");
      setStatus("Ready.");
      setBoot("done");

      for (const kind of ["fidelity", "solver", "performance"] as const) {
        sidecarGet<{ names: string[]; display_names: Record<string, string> }>(`/config/aux-presets/${kind}`)
          .then((res) => {
            setAuxPresetNames((prev) => ({ ...prev, [kind]: res.names }));
            setAuxPresetDisplay((prev) => ({ ...prev, [kind]: res.display_names }));
          })
          .catch(() => {
            /* non-critical -- the page just won't show a preset picker */
          });
      }

      // Load the first preset the way the Qt app loads combo index 0 at
      // startup, so mass/perf/geometry reflect a real preset, not bare
      // ALASConfig() defaults.
      if (presets.names.length > 0) {
        await loadPreset(presets.names[0]);
      }
    } catch (err: any) {
      appendLog(`Failed to reach sidecar: ${String(err?.message ?? err)}`, "error");
      setStatus("Sidecar error.");
      setBootError(String(err?.message ?? err));
      setBoot("error");
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    initFromSidecar();
  }, [initFromSidecar]);

  // Full retry: initFromSidecar() alone only re-polls GetSidecarPort(), which
  // stays stuck at the same failure if the one Start() attempt in Go's
  // OnStartup already failed. retrySidecarStart() first asks Go to actually
  // attempt Start() again before the frontend retries loading off of it.
  async function retryConnection() {
    try {
      await retrySidecarStart();
    } catch {
      /* surfaces via GetSidecarError()/GetSidecarPort() polling inside initFromSidecar */
    }
    await initFromSidecar();
  }

  async function loadPreset(name: string) {
    setSelectedPreset(name);
    try {
      const data = await sidecarGet<{ config: Record<string, any>; design_vector: Record<string, number> }>(
        `/config/preset/${encodeURIComponent(name)}`
      );
      setConfigValues(mergeSetupPaths(data.config));
      setDesignVector(data.design_vector);
      setSelectedEngine(data.config?.geometry?.engine?.engine_name ?? selectedEngine);
      appendLog(`Loaded preset: ${presetDisplay[name] ?? name}`);
    } catch (err: any) {
      appendLog(`Preset load failed: ${String(err?.message ?? err)}`, "error");
    }
  }

  async function applyAuxPreset(kind: "fidelity" | "solver" | "performance", name: string) {
    setSelectedAuxPreset((prev) => ({ ...prev, [kind]: name }));
    try {
      const data = await sidecarGet<{ group: string; subpath: string | null; values: Record<string, any> }>(
        `/config/aux-preset/${kind}/${encodeURIComponent(name)}`
      );
      setConfigValues((prev) => {
        const group = { ...(prev[data.group] ?? {}) };
        if (data.subpath) {
          group[data.subpath] = { ...(group[data.subpath] ?? {}), ...data.values };
        } else {
          Object.assign(group, data.values);
        }
        return { ...prev, [data.group]: group };
      });
      appendLog(`Applied ${kind} preset: ${auxPresetDisplay[kind]?.[name] ?? name}`);
    } catch (err: any) {
      appendLog(`Preset load failed: ${String(err?.message ?? err)}`, "error");
    }
  }

  useEffect(() => {
    // Engine names now come from the sidecar (GET /engines, backed by
    // config/engines.py's ENGINE_DATABASE) instead of a hardcoded mirror
    // that silently drifted whenever an engine was added to the database.
    // The static list stays only as a fallback for a not-yet-ready sidecar.
    sidecarGet<{ names: string[] }>("/engines")
      .then((res) => setEngineNames(res.names))
      .catch(() =>
        setEngineNames(["CF6-50", "CFM56-5C", "GE9X", "GEnx-1B", "LEAP-1A", "PW1500G", "Trent 900"])
      );
  }, []);

  function onEngineChange(name: string) {
    setSelectedEngine(name);
    setConfigValues((prev) => ({
      ...prev,
      geometry: { ...prev.geometry, engine: { ...(prev.geometry?.engine ?? {}), engine_name: name } },
    }));
    appendLog(`Engine changed to: ${name}`);
  }

  // Persist Setup > External Tools fields (localStorage) whenever they
  // change, so they survive an app restart -- see mergeSetupPaths above for
  // the read side.
  useEffect(() => {
    if (!configValues.mission && !configValues.structures && !configValues.mses) return;
    const paths: SetupPaths = {
      mission: {
        simbrief_username: configValues.mission?.simbrief_username ?? "",
        simbrief_timeout_s: configValues.mission?.simbrief_timeout_s,
        suave_venv_dir: configValues.mission?.suave_venv_dir ?? "",
        suave_runner_dir: configValues.mission?.suave_runner_dir ?? "",
      },
      structures: {
        nastran_exe_path: configValues.structures?.nastran_exe_path ?? "",
        run_nastran: configValues.structures?.run_nastran,
        patran_exe_path: configValues.structures?.patran_exe_path ?? "",
        run_patran_export: configValues.structures?.run_patran_export,
      },
      mses: {
        mses_dir: configValues.mses?.mses_dir ?? "",
        enabled: configValues.mses?.enabled,
      },
    };
    try {
      localStorage.setItem(SETUP_PATHS_KEY, JSON.stringify(paths));
    } catch {
      /* no-op */
    }
  }, [
    configValues.mission?.simbrief_username,
    configValues.mission?.simbrief_timeout_s,
    configValues.mission?.suave_venv_dir,
    configValues.mission?.suave_runner_dir,
    configValues.structures?.nastran_exe_path,
    configValues.structures?.run_nastran,
    configValues.structures?.patran_exe_path,
    configValues.structures?.run_patran_export,
    configValues.mses?.mses_dir,
    configValues.mses?.enabled,
  ]);

  // ---- Editing helpers ---------------------------------------------------
  const onGroupChange = (group: string) => (name: string, value: any) =>
    setConfigValues((prev) => ({ ...prev, [group]: { ...prev[group], [name]: value } }));

  const onTopLevelChange = (name: string, value: any) =>
    setConfigValues((prev) => ({ ...prev, [name]: value }));

  function resetGroup(group: string) {
    (async () => {
      try {
        const defaults = await sidecarGet<{ config: Record<string, any> }>("/config/default");
        setConfigValues((prev) => ({ ...prev, [group]: defaults.config[group] }));
        appendLog(`Reset ${group} to defaults.`);
      } catch (err: any) {
        appendLog(`Reset failed: ${String(err?.message ?? err)}`, "error");
      }
    })();
  }

  // Keep the run's initial design + bounds synced to the current design
  // vector (preset load, DOE sample, default) so a Run uses the RIGHT geometry
  // even if the Design Space page was never opened. The DesignSpaceTable, when
  // mounted and edited, overrides these via its own onChange.
  useEffect(() => {
    if (designSpecs.length === 0) return;
    const { initial, bounds: b } = deriveInitialBounds(designSpecs, designVector);
    setInitialDesign(initial);
    setBounds(b);
  }, [designVector, designSpecs]);

  // ---- Live validation (debounced, mirrors Qt's 300ms preview timer) -----
  useEffect(() => {
    if (Object.keys(configValues).length === 0) return;
    const timer = setTimeout(async () => {
      try {
        const resp = await sidecarPost<{ issues: ValidationIssue[] }>("/validate", { config: configValues });
        setIssues(resp.issues);
      } catch {
        /* transient mid-edit config -- try again next tick */
      }
    }, 300);
    return () => clearTimeout(timer);
  }, [configValues]);

  const errorFields = useMemo(() => {
    const s = new Set<string>();
    for (const i of issues) s.add(i.field_path.split(".").pop() as string);
    return s;
  }, [issues]);
  const blocked = issues.some((i) => i.severity === "error");

  // ---- Pipeline runs -----------------------------------------------------
  async function streamRun(runIdToWatch: string): Promise<void> {
    const ws = await sidecarEventSocket(`/pipeline/${runIdToWatch}/events`);
    await new Promise<void>((resolve, reject) => {
      let settled = false;
      const settle = (fn: () => void) => {
        if (!settled) {
          settled = true;
          fn();
        }
      };
      ws.onmessage = (ev) => {
        let msg: any;
        try {
          msg = JSON.parse(ev.data);
        } catch {
          return; // malformed frame -- skip rather than kill the stream
        }
        if (msg.done) {
          appendLog(`Run finished (status=${msg.status}).`, msg.status === "ok" ? "info" : "warn");
          settle(resolve);
          ws.close();
        } else if (msg.message) {
          appendLog(msg.message);
          setStage(msg.message);
        }
      };
      ws.onerror = () => settle(() => reject(new Error("WebSocket error")));
      // Without this, a socket that drops before the final "done" frame
      // (sidecar restart, transient hiccup) leaves the promise pending
      // forever -- `running` never clears and the Run button stays dead
      // until an app relaunch.
      ws.onclose = () => settle(() => reject(new Error("Progress stream closed before the run finished")));
    });
  }

  async function fetchSummary(id: string) {
    try {
      const res = await sidecarGet<{ status: string; result?: ResultSummary; error?: string }>(
        `/pipeline/${id}/result`
      );
      if (res.status === "error") {
        appendLog(`Pipeline error: ${res.error}`, "error");
        return;
      }
      setSummary(res.result ?? null);
    } catch (err: any) {
      appendLog(`Could not fetch result: ${String(err?.message ?? err)}`, "error");
    }
  }

  // Elapsed-run stopwatch: ControlBar shows this next to "Running" in place
  // of the last stage message (still fully visible in the Run Log below --
  // this is purely "is it still alive and how long has it taken" feedback).
  useEffect(() => {
    if (!running) return;
    const startedAt = Date.now();
    setElapsedMs(0);
    const timer = setInterval(() => setElapsedMs(Date.now() - startedAt), 250);
    return () => clearInterval(timer);
  }, [running]);

  async function doRun(
    kind: "run" | "baseline",
    // Surprise passes the design it just sampled directly: doRun is called
    // from the render that sampled it, whose `initialDesign`/`bounds` state
    // (synced by a useEffect) still holds the PREVIOUS design -- reading
    // state here would silently run the wrong geometry.
    overrides?: { initialDesign: Record<string, number>; bounds: [number, number][] }
  ) {
    if (running) return;
    setRunning(true);
    setStatus("Running…");
    setStage("");
    setSummary(null);
    try {
      let id: string;
      if (kind === "baseline") {
        appendLog("Analyzing baseline (weight & balance + stability)…");
        const resp = await sidecarPost<{ run_id: string }>("/pipeline/baseline", { config: configValues });
        id = resp.run_id;
      } else {
        appendLog(`Starting run (optimize=${runOptions.optimize})…`);
        // Surface exactly what NASTRAN/Patran settings are being sent, so a
        // "not detected" result is easy to diagnose: an empty path here means
        // the field didn't capture; a set path with run_nastran=false means the
        // solve is simply off (paths alone don't trigger a run).
        {
          const st = (configValues.structures ?? {}) as Record<string, any>;
          if (st.nastran_exe_path || st.run_nastran || st.patran_exe_path || st.run_patran_export) {
            appendLog(
              `Structures solve: run_nastran=${!!st.run_nastran}, ` +
                `nastran_exe_path="${st.nastran_exe_path ?? ""}", ` +
                `run_patran_export=${!!st.run_patran_export}, ` +
                `patran_exe_path="${st.patran_exe_path ?? ""}"`
            );
          }
        }
        {
          const m = (configValues.mission ?? {}) as Record<string, any>;
          appendLog(
            `Mission: enabled=${m.enabled !== false}, ` +
              `simbrief_username="${m.simbrief_username ?? ""}", ` +
              `route ${configValues.departure_airport ?? "?"} -> ${configValues.arrival_airport ?? "?"}`
          );
        }
        const resp = await sidecarPost<{ run_id: string }>("/pipeline/run", {
          config: configValues,
          optimize: runOptions.optimize,
          compare_baseline: runOptions.compareBaseline,
          parallel: runOptions.parallel,
          initial_design: overrides?.initialDesign ?? initialDesign,
          bounds: overrides?.bounds ?? bounds,
        });
        id = resp.run_id;
      }
      setRunId(id);
      await streamRun(id);
      await fetchSummary(id);
      setStatus("Done.");
      setActivePage("results");
    } catch (err: any) {
      appendLog(`Run failed: ${String(err?.message ?? err)}`, "error");
      setStatus("Failed.");
    } finally {
      setRunning(false);
      setStage("");
    }
  }

  // ---- Airfoil screening sweep -------------------------------------------
  const updateSweepOptions = (patch: Partial<SweepOptions>) =>
    setSweepOptions((prev) => ({ ...prev, ...patch }));

  async function runAirfoilSweep() {
    if (sweepRunning) return;
    setSweepRunning(true);
    setSweepError(null);
    setSweepResult(null);
    setSweepRunId(null);
    setSweepStatus("Starting…");
    const o = sweepOptions;
    try {
      const resp = await sidecarPost<{ run_id: string }>("/airfoil-sweep/run", {
        config: configValues,
        design: designVector,
        ld_weight: o.ldWeight,
        fuel_weight: o.fuelWeight,
        robustness_weight: o.robustnessWeight,
        cl_band: o.clBand,
        top_n: o.topN,
        model_size: o.modelSize,
        alpha_min_deg: o.alphaMin,
        alpha_max_deg: o.alphaMax,
        alpha_step_deg: o.alphaStep,
        min_tc: o.minTc,
        max_tc: o.maxTc,
        name_filter: o.nameFilter,
        refine_3d: o.refine3d,
        refine_top_n: o.refineTopN,
        min_static_margin: o.minStaticMargin,
        verify_mses: o.verifyMses,
        mses_top_n: o.msesTopN,
      });
      const id = resp.run_id;
      // Set the id immediately: a mid-sweep Cancel (or a tab switch and back)
      // needs it even before the stream resolves.
      setSweepRunId(id);

      const ws = await sidecarEventSocket(`/airfoil-sweep/${id}/events`);
      await new Promise<void>((resolve, reject) => {
        let settled = false;
        const settle = (fn: () => void) => {
          if (!settled) {
            settled = true;
            fn();
          }
        };
        ws.onmessage = (ev) => {
          let msg: any;
          try {
            msg = JSON.parse(ev.data);
          } catch {
            return;
          }
          if (msg.done) {
            settle(resolve);
            ws.close();
          } else if (msg.message) {
            setSweepStatus(msg.message);
          }
        };
        ws.onerror = () => settle(() => reject(new Error("WebSocket error")));
        ws.onclose = () => settle(() => reject(new Error("Progress stream closed before the sweep finished")));
      });

      const res = await sidecarGet<{ status: string; result?: SweepResult; error?: string }>(
        `/airfoil-sweep/${id}/result`
      );
      if (res.status === "error") {
        setSweepError(res.error ?? "Sweep failed");
      } else {
        setSweepResult(res.result ?? null);
      }
    } catch (err: any) {
      setSweepError(String(err?.message ?? err));
    } finally {
      setSweepRunning(false);
      setSweepStatus("");
    }
  }

  async function cancelAirfoilSweep() {
    if (!sweepRunId) return;
    setSweepStatus("Cancelling…");
    try {
      await sidecarPost(`/airfoil-sweep/${sweepRunId}/cancel`, {});
    } catch {
      /* server-side cancel is best-effort; the stream still resolves on finish */
    }
  }

  // ---- DOE / Surprise (client-side LHS approximation) --------------------
  function sampleDesign(widen: number): Record<string, number> {
    const out: Record<string, number> = {};
    for (const s of designSpecs) {
      const span = s.upper - s.lower;
      const lo = s.lower - span * widen;
      const hi = s.upper + span * widen;
      out[s.name] = lo + Math.random() * (hi - lo);
    }
    return out;
  }

  function onDoeSample() {
    if (designSpecs.length === 0) return;
    setDesignVector(sampleDesign(0));
    appendLog("DOE Sample: wrote a new design point to the Initial Value column.");
    setActivePage("design_space");
  }

  function onSurprise() {
    if (designSpecs.length === 0) return;
    const sampled = sampleDesign(0.3);
    setDesignVector(sampled);
    appendLog("Surprise: sampled ±30% beyond the bounds, starting a run.");
    // Hand the sampled design straight to doRun: the setDesignVector above
    // won't have flowed into `initialDesign` state yet (that sync happens in
    // a useEffect on a later render), so the old setTimeout-then-read-state
    // version ran the PREVIOUS design, not the one just sampled.
    const { initial, bounds: b } = deriveInitialBounds(designSpecs, sampled);
    doRun("run", { initialDesign: initial, bounds: b });
  }

  // ---- File menu: exports + storage --------------------------------------
  // Exports act on the whole run, so they live here (and in the File menu)
  // rather than as buttons on the Results page.
  const [showStorage, setShowStorage] = useState(false);
  const [exportBusy, setExportBusy] = useState(false);

  async function runExport(kind: "zip" | "pdf") {
    if (!runId || exportBusy) return;
    setExportBusy(true);
    appendLog(kind === "zip" ? "Exporting figures…" : "Generating PDF report…");
    try {
      if (kind === "zip") {
        await sidecarDownload(`/pipeline/${runId}/export/figures.zip?theme=${theme}`, `alas-${runId}-figures.zip`);
        appendLog("Figures exported.");
      } else {
        await sidecarDownload(`/pipeline/${runId}/export/report.pdf?theme=${theme}`, `alas-${runId}-report.pdf`);
        appendLog("PDF report generated.");
      }
    } catch (err: any) {
      appendLog(`Export failed: ${String(err?.message ?? err)}`, "error");
    } finally {
      setExportBusy(false);
    }
  }

  // ---- Load / save config ------------------------------------------------
  const fileInputRef = useRef<HTMLInputElement>(null);
  function onSaveConfig() {
    const blob = new Blob([JSON.stringify(configValues, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "alas-config.json";
    a.click();
    URL.revokeObjectURL(url);
    appendLog("Saved configuration to alas-config.json.");
  }
  function onLoadConfig() {
    fileInputRef.current?.click();
  }
  function onFilePicked(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      try {
        const parsed = JSON.parse(String(reader.result));
        setConfigValues(parsed);
        appendLog(`Loaded configuration from ${file.name}.`);
      } catch (err: any) {
        appendLog(`Load failed: ${String(err?.message ?? err)}`, "error");
      }
    };
    reader.readAsText(file);
    e.target.value = "";
  }

  // ---- Render ------------------------------------------------------------
  const page = ALL_PAGES.find((p) => p.id === activePage) ?? ALL_PAGES[0];
  const requirementsFields = schemaFields.find((f) => f.name === "requirements")?.fields ?? [];

  function renderContent() {
    if (page.kind === "inputs") {
      return (
        <InputsScreen
          requirementsFields={requirementsFields}
          configValues={configValues}
          onGroupChange={onGroupChange}
          onTopLevelChange={onTopLevelChange}
          presetNames={presetNames}
          presetDisplay={presetDisplay}
          selectedPreset={selectedPreset}
          onPresetChange={loadPreset}
          engineNames={engineNames}
          selectedEngine={selectedEngine}
          onEngineChange={onEngineChange}
          airportNames={airportNames}
          runOptions={runOptions}
          onRunOptionChange={(k, v) => setRunOptions((prev) => ({ ...prev, [k]: v }))}
          errorFields={errorFields}
        />
      );
    }
    if (page.kind === "design_space") {
      return (
        // See InputsScreen's identical fix: cap width for readability but
        // still shrink gracefully on a narrower window.
        <div className="af-stack" style={{ maxWidth: "min(1600px, 92%)" }}>
          <div className="af-page-header">
            <div>
              <h2 className="af-page-title">Design Space</h2>
              <div className="af-page-desc">
                The optimizer's search variables. Edit the Initial Value (nominal/starting design) and the
                Lower/Upper bounds; loading a preset recenters these around its design vector.
              </div>
            </div>
          </div>
          <DesignSpaceTable
            specs={designSpecs}
            designVector={designVector}
            onChange={(initial, b) => {
              setInitialDesign(initial);
              setBounds(b);
            }}
          />
        </div>
      );
    }
    if (page.kind === "results") {
      return <ResultsScreen runId={runId} theme={theme} summary={summary} running={running} />;
    }
    if (page.kind === "setup") {
      return <SetupScreen configValues={configValues} onGroupChange={onGroupChange} />;
    }
    if (page.kind === "analyses") {
      return <AnalysesScreen configValues={configValues} onGroupChange={onGroupChange} />;
    }
    if (page.kind === "airfoil_screening") {
      return (
        <AirfoilSweepScreen
          options={sweepOptions}
          onOptionsChange={updateSweepOptions}
          running={sweepRunning}
          status={sweepStatus}
          result={sweepResult}
          runId={sweepRunId}
          error={sweepError}
          onRun={runAirfoilSweep}
          onCancel={cancelAirfoilSweep}
          theme={theme}
        />
      );
    }
    // form page
    const group = page.group!;
    const fields = schemaFields.find((f) => f.name === group)?.fields ?? [];
    return (
      <FormPage
        page={page}
        fields={fields}
        values={configValues[group] ?? {}}
        onChange={onGroupChange(group)}
        onReset={() => resetGroup(group)}
        errorFields={errorFields}
        config={configValues}
        design={designVector}
        theme={theme}
        auxPresetNames={page.presetKind ? auxPresetNames[page.presetKind] ?? [] : []}
        auxPresetDisplay={page.presetKind ? auxPresetDisplay[page.presetKind] ?? {} : {}}
        selectedAuxPreset={page.presetKind ? selectedAuxPreset[page.presetKind] ?? "" : ""}
        onAuxPresetChange={(name) => page.presetKind && applyAuxPreset(page.presetKind, name)}
      />
    );
  }

  return (
    <HelpContext.Provider value={{ verbose: helpVerbose }}>
    <div className="af-window">
      {boot !== "done" && <Splash state={boot} error={bootError} onRetry={retryConnection} />}
      <input ref={fileInputRef} type="file" accept=".json,.yaml,.yml" style={{ display: "none" }} onChange={onFilePicked} />
      <MenuBar
        theme={theme}
        onThemeChange={setTheme}
        onLoadConfig={onLoadConfig}
        onSaveConfig={onSaveConfig}
        onReplayWalkthrough={() => setShowTour(true)}
        onAdvancedWalkthrough={() => setShowGuide(true)}
        onExportFigures={() => runExport("zip")}
        onExportReport={() => runExport("pdf")}
        exportsEnabled={!!runId && !exportBusy}
        onManageStorage={() => setShowStorage(true)}
        helpVerbose={helpVerbose}
        onToggleHelp={() => setHelpVerbose((v) => !v)}
        previewOpen={previewOpen}
        onTogglePreview={() => setPreviewOpen((o) => !o)}
        onZoomIn={() => setLockedZoom((z) => Math.min(ZOOM_MAX, Math.round((z + 0.1) * 100) / 100))}
        onZoomOut={() => setLockedZoom((z) => Math.max(ZOOM_MIN, Math.round((z - 0.1) * 100) / 100))}
        onZoomReset={() => setLockedZoom(1.0)}
      />
      <div className="af-body">
        <NavSidebar activeId={activePage} onSelect={setActivePage} />
        <div className="af-center">
          <div className="af-content-row">
            <div className="af-content">{renderContent()}</div>
            {previewOpen && (
              <PreviewDock
                config={configValues}
                design={designVector}
                theme={theme}
                presetLabel={presetDisplay[selectedPreset] ?? selectedPreset ?? "ALAS"}
                onClose={() => setPreviewOpen(false)}
              />
            )}
          </div>
          <RunLog lines={log} />
          <ControlBar
            onDoeSample={onDoeSample}
            onSurprise={onSurprise}
            onAnalyzeBaseline={() => doRun("baseline")}
            onRun={() => doRun("run")}
            running={running}
            blocked={blocked}
            progress={null}
            stage={stage}
            status={status}
            elapsedMs={elapsedMs}
            onRetrySidecar={retryConnection}
          />
        </div>
      </div>
      {showTour && <Walkthrough onNavigate={setActivePage} onClose={() => setShowTour(false)} />}
      {showStorage && <StorageDialog onClose={() => setShowStorage(false)} />}
      {showGuide && <AdvancedWalkthrough onClose={() => setShowGuide(false)} />}
    </div>
    </HelpContext.Provider>
  );
}

export default App;
