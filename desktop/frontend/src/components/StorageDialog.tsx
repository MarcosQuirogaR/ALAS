// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { useCallback, useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { sidecarGet, sidecarPost } from "../lib/sidecarClient";
import { useT } from "../lib/i18n";

// File > Manage storage... -- shows what this installation is keeping on disk
// (extracted runtimes, scratch dirs from MSES/SUAVE runs, exported outputs)
// and lets the user reclaim it. Backed by /maintenance/storage and
// /maintenance/clear; the sidecar refuses to delete the runtime the running
// process is executing from, and marks it "in use" here so the checkbox is
// disabled rather than silently ignored.

type StorageItem = {
  id: string;
  path: string;
  label: string;
  bytes: number;
  removable: boolean;
  category: string;
};

type StorageInfo = {
  items: StorageItem[];
  total_bytes: number;
  reclaimable_bytes: number;
};

function fmtBytes(n: number): string {
  if (n <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const i = Math.min(units.length - 1, Math.floor(Math.log(n) / Math.log(1024)));
  return `${(n / Math.pow(1024, i)).toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

const CATEGORY_LABEL: Record<string, string> = {
  cache: "Extracted runtimes",
  temp: "Temporary scratch files",
  outputs: "Exported outputs",
};

export function StorageDialog({ onClose }: { onClose: () => void }) {
  const t = useT();
  const [info, setInfo] = useState<StorageInfo | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const data = await sidecarGet<StorageInfo>("/maintenance/storage");
      setInfo(data);
      // Pre-select everything safely removable except the user's own exports --
      // deleting exported designs/figures should be a deliberate choice.
      setSelected(new Set(data.items.filter((i) => i.removable && i.category !== "outputs").map((i) => i.id)));
    } catch (err: any) {
      setError(String(err?.message ?? err));
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  async function clear() {
    if (busy || selected.size === 0) return;
    setBusy(true);
    setError(null);
    setMessage(null);
    try {
      const res = await sidecarPost<{ freed_bytes: number; removed: string[]; failed: { id: string; error: string }[] }>(
        "/maintenance/clear",
        { ids: Array.from(selected) }
      );
      const parts = [`Freed ${fmtBytes(res.freed_bytes)} (${res.removed.length} items).`];
      if (res.failed.length > 0) {
        parts.push(`${res.failed.length} could not be removed (in use or locked).`);
      }
      setMessage(parts.join(" "));
      await load();
    } catch (err: any) {
      setError(String(err?.message ?? err));
    } finally {
      setBusy(false);
    }
  }

  const groups = (info?.items ?? []).reduce<Record<string, StorageItem[]>>((acc, item) => {
    (acc[item.category] ??= []).push(item);
    return acc;
  }, {});
  const selectedBytes = (info?.items ?? [])
    .filter((i) => selected.has(i.id))
    .reduce((s, i) => s + i.bytes, 0);

  return createPortal(
    <div className="af-modal" onClick={onClose}>
      <div
        className="af-modal-body"
        style={{ width: "min(760px, 92vw)", height: "auto", maxHeight: "86vh" }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="af-row" style={{ marginBottom: 10 }}>
          <strong style={{ fontSize: 15 }}>{t("Manage storage")}</strong>
          <span className="af-spacer" />
          <button onClick={onClose}>{t("Close")}</button>
        </div>

        <div className="af-page-desc" style={{ marginBottom: 10 }}>
          {t(
            "Disk used by this installation. Extracted runtimes are re-created automatically on the next launch if removed — clearing them only costs a one-off extraction. The runtime the app is currently running from cannot be removed while it is in use."
          )}
        </div>

        {error && (
          <ul className="af-issue-list">
            <li className="error">{error}</li>
          </ul>
        )}
        {message && <div className="af-banner" style={{ marginBottom: 10 }}>{message}</div>}

        {!info ? (
          <div className="af-placeholder" style={{ minHeight: 120 }}>{t("Scanning…")}</div>
        ) : info.items.length === 0 ? (
          <div className="af-placeholder" style={{ minHeight: 120 }}>{t("Nothing to clean up.")}</div>
        ) : (
          <div style={{ overflowY: "auto", flex: "1 1 auto", minHeight: 0 }}>
            {Object.entries(groups).map(([category, items]) => (
              <div key={category} className="af-opt-group">
                <div className="af-opt-legend">{t(CATEGORY_LABEL[category] ?? category)}</div>
                {items.map((item) => (
                  <label
                    key={item.id}
                    className="af-row"
                    style={{ gap: 10, padding: "4px 0", opacity: item.removable ? 1 : 0.55 }}
                    title={item.path}
                  >
                    <input
                      type="checkbox"
                      disabled={!item.removable}
                      checked={selected.has(item.id)}
                      onChange={(e) =>
                        setSelected((prev) => {
                          const next = new Set(prev);
                          if (e.target.checked) next.add(item.id);
                          else next.delete(item.id);
                          return next;
                        })
                      }
                    />
                    <span style={{ flex: 1, fontSize: 13 }}>{item.label}</span>
                    <span style={{ fontVariantNumeric: "tabular-nums", fontSize: 12, color: "var(--muted)" }}>
                      {fmtBytes(item.bytes)}
                    </span>
                  </label>
                ))}
              </div>
            ))}
          </div>
        )}

        <div className="af-row" style={{ marginTop: 12, gap: 10 }}>
          <span className="af-help">
            Total {fmtBytes(info?.total_bytes ?? 0)} · reclaimable {fmtBytes(info?.reclaimable_bytes ?? 0)}
          </span>
          <span className="af-spacer" />
          <button onClick={load} disabled={busy}>{t("Rescan")}</button>
          <button className="af-btn-primary" onClick={clear} disabled={busy || selected.size === 0}>
            {busy ? t("Clearing…") : `${t("Clear selected")} (${fmtBytes(selectedBytes)})`}
          </button>
        </div>
      </div>
    </div>,
    document.body
  );
}
