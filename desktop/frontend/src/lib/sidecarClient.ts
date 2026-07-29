// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { GetSidecarPort, GetSidecarError, RetrySidecar } from "../../wailsjs/go/main/App";
import { getLang } from "./i18n";

// Append the active UI language so server-rendered text (auto-generated form
// labels/help, figure titles and axis labels) comes back in the same language
// as the rest of the interface. The sidecar reads it per request -- see
// alas/sidecar/server.py's language middleware.
function withLang(path: string): string {
  const lang = getLang();
  if (lang === "en") return path; // canonical language: nothing to add
  return path + (path.includes("?") ? "&" : "?") + "lang=" + encodeURIComponent(lang);
}

// Thin fetch/WebSocket wrapper around the Python sidecar's local HTTP API
// (alas/sidecar/server.py). The frontend talks to it directly over
// 127.0.0.1 rather than proxying through Go -- see docs/architecture.md's
// migration plan for the rationale. Go's only job is starting the process
// and handing us the port it landed on.

let cachedPort: number | null = null;

// Go's OnStartup (which spawns the Python sidecar and waits for it to
// announce its port + pass a /healthz check, see desktop/sidecar.go) runs
// concurrently with the frontend's own load rather than blocking it, so a
// naive single GetSidecarPort() call made right on mount can race ahead of
// the sidecar actually being ready and see a 0. Poll instead of failing fast.
//
// Deliberately larger than desktop/sidecar.go's own sidecarPortTimeout +
// sidecarHealthTimeout budget (180s each, 360s total) rather than equal to
// it: extraction of the ~800MB embedded bundle into the OS temp dir happens
// BEFORE either of Go's timeouts starts counting, so it's additional time
// this frontend timer has to cover but Go's own budget doesn't. An equal
// budget left zero margin -- Go could still be legitimately working (or
// about to succeed) at the exact moment this timer gave up. If this window
// ever needs to change, change desktop/sidecar.go's constants together with
// it, keeping this one comfortably larger.
const READY_POLL_INTERVAL_MS = 300;
const READY_TIMEOUT_MS = 480_000;

export async function sidecarBaseUrl(): Promise<string> {
  if (cachedPort !== null) {
    return `http://127.0.0.1:${cachedPort}`;
  }
  const deadline = Date.now() + READY_TIMEOUT_MS;
  while (Date.now() < deadline) {
    const port = await GetSidecarPort();
    if (port) {
      cachedPort = port;
      return `http://127.0.0.1:${cachedPort}`;
    }
    // A failed Start() attempt (bad DLL, missing embedded bundle, sidecar
    // exited before announcing a port, ...) leaves GetSidecarPort() stuck at
    // 0 forever, which would otherwise burn the whole READY_TIMEOUT_MS before
    // reporting a generic, undiagnosable timeout. Surface the real reason as
    // soon as Go knows it instead.
    const err = await GetSidecarError();
    if (err) throw new Error(err);
    await new Promise((r) => setTimeout(r, READY_POLL_INTERVAL_MS));
  }
  throw new Error(`Sidecar did not report a port within ${READY_TIMEOUT_MS}ms`);
}

// Used by the Splash screen's "Retry" button: re-runs the Go-side sidecar
// Start() attempt from scratch, so a failed start can actually be retried
// instead of relaunching the whole app -- merely re-polling
// GetSidecarPort()/GetSidecarError() above would stay stuck at the same
// failure forever.
export async function retrySidecarStart(): Promise<void> {
  cachedPort = null;
  await RetrySidecar();
}

export async function sidecarGet<T>(path: string): Promise<T> {
  const base = await sidecarBaseUrl();
  const resp = await fetch(`${base}${withLang(path)}`);
  if (!resp.ok) {
    throw new Error(`GET ${path} failed: ${resp.status} ${await resp.text()}`);
  }
  return resp.json() as Promise<T>;
}

export async function sidecarPost<T>(path: string, body: unknown): Promise<T> {
  const base = await sidecarBaseUrl();
  const resp = await fetch(`${base}${withLang(path)}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!resp.ok) {
    throw new Error(`POST ${path} failed: ${resp.status} ${await resp.text()}`);
  }
  return resp.json() as Promise<T>;
}

export async function sidecarEventSocket(path: string): Promise<WebSocket> {
  const base = await sidecarBaseUrl();
  const wsUrl = base.replace(/^http/, "ws") + path;
  return new WebSocket(wsUrl);
}

// SVG figures (routes_figures.py returns image/svg+xml). Fetched as text
// and injected inline so CSS can scale the <svg> to fill its container crisply.
export async function sidecarFetchText(path: string): Promise<string> {
  const base = await sidecarBaseUrl();
  const resp = await fetch(`${base}${withLang(path)}`);
  if (!resp.ok) {
    throw new Error(`GET ${path} failed: ${resp.status} ${await resp.text()}`);
  }
  return resp.text();
}

// Binary exports (figures ZIP, PDF report): fetch as a blob and trigger a
// browser-style download via a throwaway object URL + <a download>, the same
// pattern App.tsx's onSaveConfig already uses for the config-JSON download.
export async function sidecarDownload(path: string, filename: string): Promise<void> {
  const base = await sidecarBaseUrl();
  const resp = await fetch(`${base}${withLang(path)}`);
  if (!resp.ok) {
    throw new Error(`GET ${path} failed: ${resp.status} ${await resp.text()}`);
  }
  const blob = await resp.blob();
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  a.click();
  URL.revokeObjectURL(url);
}

export async function sidecarPostText(path: string, body: unknown): Promise<string> {
  const base = await sidecarBaseUrl();
  const resp = await fetch(`${base}${withLang(path)}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!resp.ok) {
    throw new Error(`POST ${path} failed: ${resp.status} ${await resp.text()}`);
  }
  return resp.text();
}
