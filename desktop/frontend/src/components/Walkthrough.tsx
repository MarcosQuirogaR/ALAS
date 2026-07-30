// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { useEffect, useLayoutEffect, useState } from "react";

// First-run onboarding walkthrough:
// expanded with more steps. A dimming scrim spotlights one target element at a
// time (via a big box-shadow "hole") with a blue highlight ring, and a panel
// explains it. Some steps navigate to a page first so the highlighted area is
// on screen. "Seen" is persisted in localStorage; Help > Replay Walkthrough
// reopens it.

export type TourStep = {
  title: string;
  body: string;
  /** CSS selector of the element to spotlight. Omit for a centered step. */
  selector?: string;
  /** Navigate to this page id before showing the step. */
  page?: string;
};

export const TOUR_STEPS: TourStep[] = [
  {
    title: "Welcome to ALAS",
    body:
      "A conceptual transport-aircraft design environment. Set requirements, tune the design space, then optimize and analyze — with live 3D and chart previews throughout. This quick tour points out the main areas.",
  },
  {
    title: "Navigate the sidebar",
    body:
      "Hover the left edge to reveal the navigation tree, then click any leaf (Inputs, Design Space, Drag model, …) to switch the content pane. Use the pin button to keep it open. It slides away when you move back to the content.",
    selector: ".af-nav",
  },
  {
    title: "Set your requirements",
    body:
      "On the Inputs page pick a preset and engine, then edit the mission requirements. Every field is validated live; error-severity issues block a Run until fixed.",
    selector: "[data-tour='aircraft-config']",
    page: "inputs",
  },
  {
    title: "3D Live Preview",
    body:
      "This dock renders your aircraft geometry and cabin layout in real time — no run needed. It updates as you edit any input. Scroll to zoom, drag to pan, double-click to reset.",
    selector: ".af-dock",
    page: "inputs",
  },
  {
    title: "Live previews everywhere",
    body:
      "Advanced Settings pages pair each form with a live chart: drag-vs-Mach, the three-view, CG envelope, landing-gear planform, control-surface layout, wingbox sizing and the engine cycle — all recomputed as you type.",
    selector: ".af-content",
    page: "control_surfaces",
  },
  {
    title: "Help on every option",
    body:
      "Hover any field's label for a plain-language explanation, and open a page's “How this works” panel for the full method behind it. Prefer a leaner screen? Toggle View ▸ Learn-more help to hide these instantly.",
    selector: ".af-content",
    page: "drag_model",
  },
  {
    title: "Tune the design space",
    body:
      "The Design Space table holds the optimizer's search variables. Edit the initial value and the lower/upper bounds; loading a preset recenters them. DOE Sample draws a random point here.",
    selector: ".af-content",
    page: "design_space",
  },
  {
    title: "Randomize or explore",
    body:
      "DOE Sample draws one design within the bounds. Surprise draws ±30% beyond them and runs immediately — handy for probing edge cases.",
    selector: "[data-tour='randomizer']",
  },
  {
    title: "Choose your analyses",
    body:
      "Setup ▸ Analyses picks which disciplines a Run performs. Core aerodynamics, weight & balance, propulsion and field performance always run; SUAVE mission, MSES 2-D airfoil and the wingbox structures solve can be toggled — all on by default. External-tool paths (NASTRAN, MSES, SimBrief) live next door under External Tools.",
    selector: ".af-content",
    page: "setup_analyses",
  },
  {
    title: "Airfoil Screening",
    body:
      "Advanced Settings ▸ Airfoil Screening ranks every airfoil in the database against this exact design's cruise condition — no Run needed — in up to three fidelity stages (2-D proxy → real 3-D wing → MSES). Set the objective and filters, press Run, and you can switch tabs while it works or cancel any time.",
    selector: ".af-content",
    page: "airfoil_screening",
  },
  {
    title: "Run the pipeline",
    body:
      "Run optimizes the design, runs the aerodynamic analysis, and every discipline enabled under Setup ▸ Analyses — one pass. A stage label and status appear while it runs. Analyze baseline does a quick weight & balance check without the optimizer.",
    selector: "[data-tour='run']",
  },
  {
    title: "Follow the Run Log",
    body:
      "Progress messages stream here — preset loads, engine changes, each pipeline stage, and any errors. Drag its top edge to resize it.",
    selector: ".af-logdock",
  },
  {
    title: "Explore the Results",
    body:
      "After a run you land here. Tabs cover Optimization, Aerodynamics, Weight & Balance, Propulsion, Structures, Mission & Route and Model Comparison — dozens of figures. Slots reading “Not available” need data this run didn't produce.",
    selector: ".af-content",
    page: "results",
  },
  {
    title: "Themes and files",
    body:
      "Use the View menu to switch Dark / Light / Grey themes instantly, and File to save or load a configuration. Replay this tour any time from Help ▸ Replay Walkthrough.",
    selector: ".af-menubar",
    page: "inputs",
  },
];

const SEEN_KEY = "alas.onboarding.seen";

export function hasSeenWalkthrough(): boolean {
  try {
    return localStorage.getItem(SEEN_KEY) === "true";
  } catch {
    return false;
  }
}

function markSeen() {
  try {
    localStorage.setItem(SEEN_KEY, "true");
  } catch {
    /* no-op */
  }
}

type Rect = { top: number; left: number; width: number; height: number };

export function Walkthrough({
  steps = TOUR_STEPS,
  onClose,
  onNavigate,
}: {
  steps?: TourStep[];
  onClose: () => void;
  onNavigate: (pageId: string) => void;
}) {
  const [index, setIndex] = useState(0);
  const [rect, setRect] = useState<Rect | null>(null);
  const step = steps[index];
  const isLast = index === steps.length - 1;

  // Navigate for steps that need a particular page on screen.
  useEffect(() => {
    if (step.page) onNavigate(step.page);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [index]);

  useLayoutEffect(() => {
    let raf = 0;
    function measure() {
      if (!step.selector) {
        setRect(null);
        return;
      }
      const el = document.querySelector(step.selector) as HTMLElement | null;
      if (!el) {
        setRect(null);
        return;
      }
      // Snap the target into view before measuring it -- e.g. step 3's
      // `[data-tour='aircraft-config']` sits inside `.af-content`'s own
      // scroll area, and if the user scrolled that area away from it on an
      // earlier visit to the Inputs page, getBoundingClientRect() would
      // return an off-screen rect, so the spotlight hole (and often the
      // whole step) would render somewhere invisible -- the tour would
      // appear to "disappear". Only scrolls when actually needed, so it
      // doesn't fight a scroll position that's already fine.
      const r0 = el.getBoundingClientRect();
      const pad = 8;
      const fullyVisible =
        r0.top >= pad && r0.left >= pad && r0.bottom <= window.innerHeight - pad && r0.right <= window.innerWidth - pad;
      if (!fullyVisible) {
        el.scrollIntoView({ block: "center", inline: "nearest", behavior: "auto" });
      }
      const r = el.getBoundingClientRect();
      setRect({ top: r.top, left: r.left, width: r.width, height: r.height });
    }
    // Defer a tick so a page navigation has laid out before we measure.
    const t = setTimeout(() => {
      measure();
      raf = requestAnimationFrame(measure);
    }, 60);
    window.addEventListener("resize", measure);
    return () => {
      clearTimeout(t);
      cancelAnimationFrame(raf);
      window.removeEventListener("resize", measure);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [index]);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") finish();
      else if (e.key === "Enter" || e.key === "ArrowRight" || e.key === " ") next();
      else if (e.key === "ArrowLeft") setIndex((i) => Math.max(0, i - 1));
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  });

  function next() {
    if (isLast) finish();
    else setIndex((i) => i + 1);
  }
  function finish() {
    markSeen();
    onClose();
  }

  const pad = 8;
  const hole = rect
    ? { top: rect.top - pad, left: rect.left - pad, width: rect.width + 2 * pad, height: rect.height + 2 * pad }
    : null;

  // Panel placement: below the hole, else above, else centered.
  const panelW = 390;
  const vh = window.innerHeight;
  const vw = window.innerWidth;
  let panelStyle: React.CSSProperties = {
    left: Math.max(16, (vw - panelW) / 2),
    top: Math.max(16, vh / 2 - 120),
  };
  if (hole) {
    let top = hole.top + hole.height + 14;
    if (top + 220 > vh) top = Math.max(16, hole.top - 220);
    const left = Math.max(16, Math.min(hole.left, vw - panelW - 16));
    panelStyle = { left, top };
  }

  return (
    <div style={{ position: "fixed", inset: 0, zIndex: 2000 }}>
      {hole ? (
        <div
          style={{
            position: "absolute",
            top: hole.top,
            left: hole.left,
            width: hole.width,
            height: hole.height,
            borderRadius: 8,
            boxShadow: "0 0 0 9999px rgba(0,0,0,0.62), 0 0 0 2px #5b9bf0, 0 0 22px 6px rgba(91,155,240,0.35)",
            pointerEvents: "none",
            transition: "all 160ms ease",
          }}
        />
      ) : (
        <div style={{ position: "absolute", inset: 0, background: "rgba(0,0,0,0.62)" }} />
      )}

      <div
        style={{
          position: "absolute",
          width: panelW,
          background: "var(--panel)",
          border: "1px solid var(--accent)",
          borderRadius: 10,
          padding: "18px 20px",
          boxShadow: "0 12px 40px rgba(0,0,0,0.5)",
          ...panelStyle,
        }}
      >
        <div className="af-help" style={{ letterSpacing: 0.5 }}>
          Step {index + 1} of {steps.length}
        </div>
        <div style={{ fontSize: 17, fontWeight: 700, margin: "6px 0 8px" }}>{step.title}</div>
        <div style={{ fontSize: 13, lineHeight: 1.55, color: "var(--muted)" }}>{step.body}</div>
        <div className="af-row" style={{ marginTop: 16 }}>
          <button onClick={finish} style={{ minHeight: 0 }}>
            Skip
          </button>
          <span className="af-spacer" />
          {index > 0 && (
            <button onClick={() => setIndex((i) => i - 1)} style={{ minHeight: 0 }}>
              ← Back
            </button>
          )}
          <button className="af-btn-primary" onClick={next} style={{ minHeight: 0 }}>
            {isLast ? "Get started ✓" : "Next →"}
          </button>
        </div>
      </div>
    </div>
  );
}
