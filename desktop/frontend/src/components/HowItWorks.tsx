// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { useState } from "react";
import type { ReactNode } from "react";
import { useHelp } from "../lib/helpContext";

// A collapsible "How this works" deep-dive panel. This is the app's "detailed
// walkthrough" for advanced users: a short summary stays always visible on a
// page, and the full explanation of every option/parameter lives in here,
// collapsed by default -- the density win. Matches the existing "Advanced (N)"
// disclosure idiom the auto-generated forms already use. Hidden entirely when
// View > Learn-more help is off (see helpContext.tsx).

type HowItWorksProps = {
  title?: string;
  children: ReactNode;
  /** Start expanded. Defaults to collapsed -- reducing visible density is the point. */
  defaultOpen?: boolean;
};

export function HowItWorks({ title = "How this works", children, defaultOpen = false }: HowItWorksProps) {
  const { verbose } = useHelp();
  const [open, setOpen] = useState(defaultOpen);
  if (!verbose) return null;

  return (
    <div className={"af-howto" + (open ? " open" : "")}>
      <button type="button" className="af-howto-toggle" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <span className="af-howto-caret">{open ? "▾" : "▸"}</span>
        {title}
      </button>
      {open && <div className="af-howto-body">{children}</div>}
    </div>
  );
}
