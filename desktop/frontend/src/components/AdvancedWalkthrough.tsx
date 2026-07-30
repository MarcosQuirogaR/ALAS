// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useT } from "../lib/i18n";

// The in-depth companion to the first-run spotlight tour (Walkthrough.tsx).
//
// Where that one points at the five things you need to press a button, this is
// the reference: what each discipline actually computes, which physical model
// sits behind it, what the numbers mean, and where each one can mislead you.
// It is a reading document rather than a spotlight overlay -- a scrollable,
// chaptered panel with a contents rail, because the material is far too long to
// live in tooltip-sized steps.
//
// Content is data (CHAPTERS) so every string flows through the same t()
// catalog as the rest of the UI and translates with the app.

type Section = { heading: string; body: string[] };
type Chapter = { id: string; title: string; blurb: string; sections: Section[] };

const CHAPTERS: Chapter[] = [
  {
    id: "overview",
    title: "What ALAS does",
    blurb: "The pipeline, and what 'conceptual design' means here.",
    sections: [
      {
        heading: "The five stages",
        body: [
          "A Run walks a fixed pipeline. Stage 0 analyses the baseline design you started from — weight, balance and static margin — so you can sanity-check a preset before spending time optimising it. Stage 1 searches the design space. Stage 2 re-analyses the winner at high fidelity. Stage 3 exports. Stage 4 draws the figures. Stage 5 flies the mission in SUAVE.",
          "Everything after Stage 2 is additive: mission analysis, MSES and the structural solve are downstream consumers of the design. They never feed back into it, so switching them off changes what you see, never what the optimizer chose.",
        ],
      },
      {
        heading: "Two fidelity levels, on purpose",
        body: [
          "Inside the optimizer loop each candidate is scored with a fast two-point aerodynamic estimate, because it runs hundreds of times. The winning design is then re-run with a full alpha sweep. Both share the same parasite and wave-drag build-up, so the fast estimate is a consistent reduction of the fine one rather than a different model.",
          "This is why the reported cruise L/D can differ slightly from the value the optimizer was ranking on. The reported one is the trustworthy number.",
        ],
      },
      {
        heading: "What this tool is not",
        body: [
          "This is preliminary sizing. The aerodynamics are a vortex-lattice method with empirical drag build-ups, not CFD. Masses come from Torenbeek and Raymer statistical formulas fitted to real transports, not from a structural weights breakdown. Treat outputs as a well-founded starting point that tells you which direction to move, not as certification evidence.",
        ],
      },
    ],
  },
  {
    id: "inputs",
    title: "Requirements & design space",
    blurb: "What you specify, what the optimizer is allowed to change.",
    sections: [
      {
        heading: "Requirements are targets and limits",
        body: [
          "Cruise Mach, altitude and MTOW anchor the whole sizing. Maximum wing area, minimum wing loading and maximum cruise CL are constraints the optimizer is penalised for breaking. Target static margin and CG range define the stability envelope every candidate is checked against.",
          "Maximum cruise CL is a genuine feasibility gate, not a preference: a candidate whose required cruise CL exceeds it has no sensible operating point and is rejected outright rather than scored badly.",
        ],
      },
      {
        heading: "The design space table",
        body: [
          "Each row is one degree of freedom the optimizer may vary, with an initial value and lower/upper bounds. The initial value does double duty: it is the design analysed when you skip optimisation, and the baseline the optimised result is compared against.",
          "Loading a preset recentres the bounds around that aircraft. Widening bounds explores more but takes longer and produces more invalid candidates; narrowing them is how you ask 'what is the best version of roughly this aeroplane'.",
        ],
      },
      {
        heading: "Cabin by percentage, not seat count",
        body: [
          "Class mix is specified as a share of cabin floor length, and seat counts are solved from that share together with each class's pitch, seats abreast and the real fuselage geometry. This matches how a cabin is actually specified — you cannot pick a seat count independently of the geometry that has to hold it.",
          "Switch Class mix mode to 'count' when you need to pin exact numbers instead.",
        ],
      },
    ],
  },
  {
    id: "optimizer",
    title: "The optimizer",
    blurb: "How candidates are scored, and why penalties are shaped the way they are.",
    sections: [
      {
        heading: "The cost function",
        body: [
          "SciPy's differential evolution minimises −L/D plus a set of weighted penalties: wing area, wing loading, tail volume coefficients, static-margin deviation, CG-envelope violation, fuel-volume shortfall and several geometric-realism terms.",
          "The weights are yours to tune, but the static-margin term is deliberately kept small relative to L/D. Push it much past ~30 and the optimizer chases an exact stability match instead of exploring shape.",
        ],
      },
      {
        heading: "Why invalid designs still get scored",
        body: [
          "A candidate that fails to build or evaluate returns a large flat cost and is abandoned. But a candidate that builds fine and is merely physically invalid — unstable, or outside the CG envelope — is treated differently: it still gets a real aerodynamic evaluation, and the violation adds a large but continuous penalty on top.",
          "That distinction is load-bearing. An earlier version blanked out L/D for such candidates, and the solver lost the ability to feel its way toward the feasible region: it could no longer tell 'unstable but aerodynamically promising' from 'unstable and hopeless', and converged on an infeasible best-of-a-bad-lot design.",
        ],
      },
      {
        heading: "Reading convergence",
        body: [
          "The convergence figure shows best-so-far cost against evaluation count. A curve that flattens early with a wide population spread usually means the bounds are too tight. A curve still descending at the last generation means you stopped too early.",
        ],
      },
    ],
  },
  {
    id: "aero",
    title: "Aerodynamics & stability",
    blurb: "VLM, drag build-up, trim, and the dynamic modes.",
    sections: [
      {
        heading: "Where drag comes from",
        body: [
          "Induced drag comes from the vortex-lattice solution. Parasite drag is a component-by-component flat-plate skin-friction and form-factor build-up with interference factors. Wave drag is a Korn-equation rise past the drag-divergence Mach.",
          "The drag breakdown figure separates these, which is the fastest way to see whether a design is losing to induced drag (fix the span or the loading) or to wave drag (fix the sweep, thickness or cruise Mach).",
        ],
      },
      {
        heading: "Trim is solved, not assumed",
        body: [
          "Cruise performance is evaluated at a genuinely trimmed condition: a closed-form solve finds the angle of attack and horizontal-stabiliser incidence that simultaneously produce the required lift and zero pitching moment, then one non-linear VLM point is run there. The L/D you see therefore includes real trim drag.",
        ],
      },
      {
        heading: "Static margin and the neutral point",
        body: [
          "Static margin is the distance from the centre of gravity to the neutral point, as a fraction of mean aerodynamic chord. Positive means pitch-stable. The app distinguishes the aerodynamic reference point from the real mass-model CG, and enforces the minimum against the physical one — the honest test.",
          "Tail efficiency and the fuselage's destabilising contribution both move the neutral point. Turning off the fuselage term will flatter your stability; it is on by default for a reason.",
        ],
      },
    ],
  },
  {
    id: "screening",
    title: "Airfoil screening",
    blurb: "Three fidelity stages over ~1,600 sections, and how to read the ranking.",
    sections: [
      {
        heading: "Why three stages",
        body: [
          "Stage 1 scores every section in the database with a fast 2-D neural-network model, at the swept-section effective Mach and your chord Reynolds. It is cheap enough to sweep everything but flatters thin low-Reynolds sections that would never suit a transport.",
          "Stage 2 rebuilds the shortlist into your actual wing and re-evaluates with the full 3-D model and a real trim solve. This is what demotes those 2-D flatterers once induced and wave drag on your planform are counted.",
          "Stage 3 runs MSES, a coupled viscous/inviscid solver, on the finalists. It is the only stage that captures shocks and true wave drag, so it is the only one that can properly credit a genuinely supercritical section.",
        ],
      },
      {
        heading: "The reference sections",
        body: [
          "A curated set of real, wind-tunnel-validated transonic sections — the NASA SC(2) family, the original Whitcomb airfoil, RAE 2822 — is forced through every stage regardless of its score, and marked with a star. They are there as a physical anchor: they tell you how the algorithm's picks compare against sections known to work on real jets, rather than only against each other.",
        ],
      },
      {
        heading: "Reading the result honestly",
        body: [
          "This is a shortlisting tool. The correct workflow is to take the top few candidates and verify them with a real Run, not to adopt the winner directly. At a transonic cruise Mach the app says so explicitly in a banner.",
          "Off-design robustness is worth enabling: a section that wins only at exactly the design CL is fragile, because real cruise CL wanders with weight and altitude.",
        ],
      },
    ],
  },
  {
    id: "weights",
    title: "Weight, balance & structures",
    blurb: "Where mass comes from, and what the wingbox solve does.",
    sections: [
      {
        heading: "The mass build-up",
        body: [
          "Component masses come from Torenbeek and Raymer statistical relations driven by MTOW, geometry and load factors. The detailed interior — a real seat map or ULD load — is built and its true mass-weighted CG overrides the lumped estimate.",
          "That detail matters: the lumped payload CG (the geometric centre of the occupied cabin) can differ from the real one by several percent MAC, which is enough to move a design from inside the CG envelope to outside it.",
        ],
      },
      {
        heading: "The CG envelope",
        body: [
          "The envelope is bounded by stability at the aft limit and by landing-gear load limits — maximum nose and main gear strength, and the minimum nose load needed for steering authority. A design is only compliant if every loading state, from empty to maximum take-off, sits inside it.",
        ],
      },
      {
        heading: "The wingbox",
        body: [
          "The structural solve sizes a generic wingbox — skin, spars, ribs — from strength requirements, then computes deflections, stresses and natural frequencies analytically. NASTRAN is optional: without it you still get every analytical result, and the .bdf files are written either way.",
          "It is a downstream analysis. It does not feed the mass model, so a heavy wingbox will not change the optimizer's answer; compare it against the Torenbeek estimate shown beside it as an accuracy check.",
        ],
      },
    ],
  },
  {
    id: "mission",
    title: "Mission, routing & propulsion",
    blurb: "SUAVE, the four routing tiers, and the engine cycle.",
    sections: [
      {
        heading: "Mission analysis",
        body: [
          "SUAVE flies a full climb/cruise/descent profile for the chosen design over your airport pair, returning fuel burn, block time and complete telemetry. It runs in an isolated Python environment because it needs an older scientific stack than the rest of the app, and it never blocks a Run: if it cannot complete you get a status and a reason, not a failure.",
        ],
      },
      {
        heading: "Routing has four tiers",
        body: [
          "In order: your live SimBrief flight plan, a manually exported SimBrief KML, an open airway graph, and finally a great circle. Each falls through to the next, so a route always renders.",
          "When SimBrief returns a plan for a different city pair than the one selected, it takes precedence by default and the mission is sized against the airports actually flown — a real dispatched plan is the most accurate routing available. Turn that off in Mission settings if you want your manual selection to win.",
        ],
      },
      {
        heading: "The engine cycle",
        body: [
          "The propulsion tab evaluates a two-spool turbofan cycle from first-principles thermodynamics with polytropic component efficiencies. Because those efficiencies are generic rather than a specific manufacturer's hardware, the computed fuel consumption runs systematically optimistic-to-pessimistic by roughly 20% against published figures. Engine ordering is reliable; absolute values are not.",
        ],
      },
    ],
  },
  {
    id: "practice",
    title: "Working effectively",
    blurb: "Habits that make the tool tell you the truth.",
    sections: [
      {
        heading: "Start from a preset",
        body: [
          "Presets are calibrated so their nominal design and the all-variables-at-minimum corner both stay feasible. Run Analyze baseline first and confirm the CG and static margin look sane — if the baseline is wrong, everything downstream is wrong.",
        ],
      },
      {
        heading: "Change one thing at a time",
        body: [
          "The cost function couples nearly everything. If you change three weights and the answer improves, you have learned very little. Change one, re-run, and read the convergence and drag-breakdown figures before moving on.",
        ],
      },
      {
        heading: "Trust the reported numbers, not the loop's",
        body: [
          "The optimizer's in-loop estimates exist to rank candidates cheaply. Quote the final analysis figures. If the two disagree sharply, the usual cause is a coarse in-loop panel resolution under-capturing a cambered section's camber line.",
        ],
      },
      {
        heading: "Keep an eye on disk",
        body: [
          "Extracted runtimes and solver scratch directories accumulate. File ▸ Manage storage shows exactly what is being kept and reclaims it safely — it will never delete the runtime the running app is executing from.",
        ],
      },
    ],
  },
];

export function AdvancedWalkthrough({ onClose }: { onClose: () => void }) {
  const t = useT();
  const [activeId, setActiveId] = useState(CHAPTERS[0].id);
  const active = useMemo(
    () => CHAPTERS.find((c) => c.id === activeId) ?? CHAPTERS[0],
    [activeId]
  );
  const contentRef = useRef<HTMLElement>(null);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  // Switching chapters (via the nav rail or Next/Back) should always land at
  // the top of the new chapter, not wherever the previous one left the
  // scroll position.
  useEffect(() => {
    contentRef.current?.scrollTo({ top: 0 });
  }, [activeId]);

  const index = CHAPTERS.findIndex((c) => c.id === activeId);

  return createPortal(
    <div className="af-modal" onClick={onClose}>
      <div
        className="af-modal-body af-guide"
        style={{ width: "min(1080px, 94vw)", height: "min(860px, 90vh)" }}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="af-row" style={{ marginBottom: 10 }}>
          <strong style={{ fontSize: 16 }}>{t("Advanced Walkthrough")}</strong>
          <span className="af-help" style={{ marginLeft: 10 }}>
            {t("An in-depth guide to what ALAS computes and how to read it")}
          </span>
          <span className="af-spacer" />
          <button onClick={onClose}>{t("Close")}</button>
        </div>

        <div className="af-guide-body">
          <nav className="af-guide-nav">
            {CHAPTERS.map((c, i) => (
              <button
                key={c.id}
                className={"af-guide-navitem" + (c.id === activeId ? " active" : "")}
                onClick={() => setActiveId(c.id)}
              >
                <span className="af-guide-num">{i + 1}</span>
                <span>
                  <span className="af-guide-navtitle">{t(c.title)}</span>
                  <span className="af-guide-navblurb">{t(c.blurb)}</span>
                </span>
              </button>
            ))}
          </nav>

          <article className="af-guide-content" ref={contentRef}>
            <h2 className="af-guide-title">{t(active.title)}</h2>
            {active.sections.map((s) => (
              <section key={s.heading} className="af-guide-section">
                <h3>{t(s.heading)}</h3>
                {s.body.map((para, i) => (
                  <p key={i}>{t(para)}</p>
                ))}
              </section>
            ))}

            <div className="af-row" style={{ marginTop: 22, gap: 10 }}>
              <button
                disabled={index === 0}
                onClick={() => setActiveId(CHAPTERS[Math.max(0, index - 1)].id)}
              >
                {t("← Back")}
              </button>
              <span className="af-spacer" />
              <span className="af-help">
                {index + 1} / {CHAPTERS.length}
              </span>
              <span className="af-spacer" />
              {index < CHAPTERS.length - 1 ? (
                <button
                  className="af-btn-primary"
                  onClick={() => setActiveId(CHAPTERS[index + 1].id)}
                >
                  {t("Next →")}
                </button>
              ) : (
                <button className="af-btn-primary" onClick={onClose}>
                  {t("Done")}
                </button>
              )}
            </div>
          </article>
        </div>
      </div>
    </div>,
    document.body
  );
}
