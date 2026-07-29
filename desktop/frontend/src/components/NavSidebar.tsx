// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { useState } from "react";
import { NAV } from "../pages";
import { useT } from "../lib/i18n";

// Left navigation tree (QTreeWidget port) inside a hover-expand rail
// (hover_sidebar.py): collapsed to a thin accent strip, it slides out on hover
// as an overlay over the content. A pin toggle keeps it open, mirroring the
// desktop app's ability to dock the panel.

type NavSidebarProps = {
  activeId: string;
  onSelect: (id: string) => void;
};

export function NavSidebar({ activeId, onSelect }: NavSidebarProps) {
  const [pinned, setPinned] = useState(false);
  const t = useT();

  return (
    // `pinned` also goes on the WRAPPER: the wrapper is the actual flex item in
    // .af-body, so widening it there reserves real layout space and pushes the
    // content pane across instead of the (absolutely-positioned) panel covering
    // it. Unpinned it stays an 8px rail and the panel overlays on hover as before.
    <div className={"af-nav-wrap" + (pinned ? " pinned" : "")}>
      <nav className={"af-nav" + (pinned ? " pinned" : "")}>
        <div className="af-row" style={{ justifyContent: "space-between", padding: "2px 10px 6px" }}>
          <span className="af-help">{t("Navigation")}</span>
          <button
            title={pinned ? t("Unpin sidebar") : t("Pin sidebar open")}
            onClick={() => setPinned((prev) => !prev)}
            style={{ minHeight: 0, padding: "2px 8px", fontSize: 12 }}
          >
            {pinned ? t("Pinned") : t("Pin")}
          </button>
        </div>
        {NAV.map((group) => (
          <div key={group.title}>
            <div className="af-nav-group">{t(group.title)}</div>
            {group.subgroups.map((sub, si) => (
              <div key={sub.title ?? si}>
                {sub.title && <div className="af-nav-subgroup">{t(sub.title)}</div>}
                {sub.pages.map((page) => (
                  <div
                    key={page.id}
                    className={
                      "af-nav-item" +
                      (sub.title ? " sub" : "") +
                      (page.id === activeId ? " active" : "")
                    }
                    onClick={() => onSelect(page.id)}
                  >
                    {t(page.title)}
                  </div>
                ))}
              </div>
            ))}
          </div>
        ))}
      </nav>
    </div>
  );
}
