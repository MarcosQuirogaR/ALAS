// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { useEffect, useRef, useState } from "react";
import logo from "../assets/images/logo-universal.png";
import { useLang } from "../lib/i18n";

// The window's File / View / Help menu bar -- a transcription of
// Application menu. File actions (load/save config) and the
// walkthrough are wired to the callbacks the shell provides; the View > Theme
// submenu drives the live theme swap (data-theme on <html>), the same
// no-restart theme switch the Qt app does via setStyleSheet.

export type ThemeName = "dark" | "light" | "grey";

type MenuBarProps = {
  theme: ThemeName;
  onThemeChange: (t: ThemeName) => void;
  onLoadConfig: () => void;
  onSaveConfig: () => void;
  onReplayWalkthrough: () => void;
  onAdvancedWalkthrough: () => void;
  /** File > export actions. Disabled (greyed) until a run has produced results. */
  onExportFigures: () => void;
  onExportReport: () => void;
  exportsEnabled: boolean;
  onManageStorage: () => void;
  helpVerbose: boolean;
  onToggleHelp: () => void;
  previewOpen: boolean;
  onTogglePreview: () => void;
  onZoomIn: () => void;
  onZoomOut: () => void;
  onZoomReset: () => void;
};

type MenuDef = {
  label: string;
  items: (
    | { label: string; onClick: () => void; checked?: boolean; keepOpen?: boolean; disabled?: boolean }
    | "sep"
  )[];
};

export function MenuBar({
  theme,
  onThemeChange,
  onLoadConfig,
  onSaveConfig,
  onReplayWalkthrough,
  onAdvancedWalkthrough,
  onExportFigures,
  onExportReport,
  exportsEnabled,
  onManageStorage,
  helpVerbose,
  onToggleHelp,
  previewOpen,
  onTogglePreview,
  onZoomIn,
  onZoomOut,
  onZoomReset,
}: MenuBarProps) {
  const [open, setOpen] = useState<string | null>(null);
  const barRef = useRef<HTMLDivElement>(null);
  const { lang, setLang, t } = useLang();

  useEffect(() => {
    function onDocClick(e: MouseEvent) {
      if (barRef.current && !barRef.current.contains(e.target as Node)) setOpen(null);
    }
    document.addEventListener("mousedown", onDocClick);
    return () => document.removeEventListener("mousedown", onDocClick);
  }, []);

  const menus: MenuDef[] = [
    {
      label: t("File"),
      items: [
        { label: t("Load configuration…"), onClick: onLoadConfig },
        { label: t("Save configuration…"), onClick: onSaveConfig },
        "sep",
        // Export actions live here rather than as buttons on the Results page:
        // they apply to the whole run, not to whichever tab is open, and a
        // menu is where users look for "export". Greyed out until a run exists.
        { label: t("Export figures (PNG)…"), onClick: onExportFigures, disabled: !exportsEnabled },
        { label: t("Generate PDF report…"), onClick: onExportReport, disabled: !exportsEnabled },
        "sep",
        { label: t("Manage storage…"), onClick: onManageStorage },
      ],
    },
    {
      label: t("View"),
      items: [
        { label: t("Dark theme"), onClick: () => onThemeChange("dark"), checked: theme === "dark" },
        { label: t("Light theme"), onClick: () => onThemeChange("light"), checked: theme === "light" },
        { label: t("Grey theme"), onClick: () => onThemeChange("grey"), checked: theme === "grey" },
        "sep",
        { label: t("3D Live Preview"), onClick: onTogglePreview, checked: previewOpen },
        { label: t("Learn-more help"), onClick: onToggleHelp, checked: helpVerbose },
        "sep",
        // Language switch: applies live (the sidecar is told via ?lang=, so
        // server-rendered form labels/help and figure text follow suit).
        { label: t("English"), onClick: () => setLang("en"), checked: lang === "en" },
        { label: t("Spanish"), onClick: () => setLang("es"), checked: lang === "es" },
        "sep",
        { label: t("Zoom in"), onClick: onZoomIn, keepOpen: true },
        { label: t("Zoom out"), onClick: onZoomOut, keepOpen: true },
        { label: t("Reset zoom (100%)"), onClick: onZoomReset },
      ],
    },
    {
      label: t("Help"),
      items: [
        { label: t("Replay Walkthrough"), onClick: onReplayWalkthrough },
        { label: t("Advanced Walkthrough…"), onClick: onAdvancedWalkthrough },
      ],
    },
  ];

  return (
    <div className="af-menubar" ref={barRef}>
      <span className="af-brand">
        <img src={logo} alt="" />
        ALAS
      </span>
      {menus.map((m) => (
        <div key={m.label} className={"af-menu" + (open === m.label ? " open" : "")}>
          <div
            className="af-menu-label"
            onClick={() => setOpen((o) => (o === m.label ? null : m.label))}
            onMouseEnter={() => open && setOpen(m.label)}
          >
            {m.label}
          </div>
          {open === m.label && (
            <div className="af-menu-dropdown">
              {m.items.map((it, i) =>
                it === "sep" ? (
                  <div key={i} className="af-menu-sep" />
                ) : (
                  <div
                    key={i}
                    className={"af-menu-item" + (it.disabled ? " disabled" : "")}
                    onClick={() => {
                      if (it.disabled) return;
                      it.onClick();
                      if (!it.keepOpen) setOpen(null);
                    }}
                  >
                    <span>{it.label}</span>
                    {it.checked && <span className="af-check-mark">✓</span>}
                  </div>
                )
              )}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
