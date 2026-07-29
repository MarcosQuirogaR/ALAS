// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

// The sidebar's page model --
// _build_tabs() nav tree (Setup / Advanced Settings > Modeling / Advanced
// Settings > Analysis / Results). Each advanced page maps to one config
// dataclass group emitted by GET /schema; the special pages (Inputs, Design
// Space, Results) render bespoke screens instead.

export type PageKind = "inputs" | "design_space" | "results" | "form" | "setup" | "analyses" | "airfoil_screening";

export type Page = {
  id: string;
  title: string;
  kind: PageKind;
  /** Config group key (top-level ALASConfig field) for a "form" page. */
  group?: string;
  description?: string;
  /** Optional "How this works" deep-dive: each string renders as a paragraph
   * inside a collapsed-by-default panel (the app's detailed-walkthrough for
   * advanced users). Kept as plain strings so pages.ts stays pure data. */
  detail?: string[];
  /** Live-preview figure name (POST /preview/{preview}) shown beside the form,
   * mirroring the desktop app's per-Advanced-tab side chart. */
  preview?: string;
  previewTitle?: string;
  /** Which aux-preset registry (GET /config/aux-presets/{kind}) this page
   * offers a picker for -- the old Py6 GUI's analysis-fidelity/solver/
   * performance preset dropdowns, reintroduced per-page instead of as one
   * consolidated picker so each preset sits next to the fields it fills. */
  presetKind?: "fidelity" | "solver" | "performance";
};

export type NavGroup = {
  title: string;
  subgroups: { title?: string; pages: Page[] }[];
};

export const NAV: NavGroup[] = [
  {
    title: "Setup",
    subgroups: [
      {
        pages: [
          { id: "inputs", title: "Inputs", kind: "inputs" },
          { id: "design_space", title: "Design Space", kind: "design_space" },
          { id: "setup_analyses", title: "Analyses", kind: "analyses" },
          { id: "setup_tools", title: "External Tools", kind: "setup" },
        ],
      },
    ],
  },
  {
    title: "Advanced Settings",
    subgroups: [
      {
        title: "Modeling",
        pages: [
          {
            id: "drag_model",
            title: "Drag model",
            kind: "form",
            group: "drag_model",
            description:
              "Coefficients of the Raymer/Korn parasite- and wave-drag build-up used by the aerodynamic model. Tune the CD0 floor and the drag-divergence Mach behaviour.",
            detail: [
              "The aerodynamic model builds cruise drag as parasite drag (a flat-plate skin-friction + form-factor build-up per component, Raymer Ch. 12) plus compressibility/wave drag (a Korn-equation drag rise past the drag-divergence Mach). Induced drag comes from the VLM, not from this page.",
              "The CD0 floor sets a minimum parasite level so a degenerate geometry can't score an unrealistically clean polar. The drag-divergence behaviour (technology factor and the Mach at which wave drag switches on) controls how sharply drag rises toward cruise Mach — raise the technology factor for a more modern supercritical wing.",
            ],
            preview: "drag",
            previewTitle: "Drag vs Mach (illustrative)",
          },
          {
            id: "geometry",
            title: "Geometry scaffold",
            kind: "form",
            group: "geometry",
            description:
              "The fixed geometric scaffold the design vector morphs against - fuselage, empennage and nacelle placement that isn't part of the optimizer search space.",
            preview: "geometry",
            previewTitle: "Three-view schematic",
          },
          {
            id: "mass_model",
            title: "Mass model",
            kind: "form",
            group: "mass_model",
            description:
              "Torenbeek component-mass and CG-solver assumptions: structural fractions, margins and reference values behind the weight & balance build-up.",
            preview: "mass_cg",
            previewTitle: "CG envelope (illustrative)",
          },
          {
            id: "landing_gear",
            title: "Landing Gear",
            kind: "form",
            group: "landing_gear",
            description:
              "Main- and nose-gear placement, strut sizing and tip-over/strength assumptions used by the CG-envelope and gear-load checks.",
            preview: "landing_gear",
            previewTitle: "Landing-gear planform",
          },
          {
            id: "engine_designer",
            title: "Engine Designer",
            kind: "form",
            group: "propulsion_cycle",
            description:
              "On-design turbofan cycle assumptions (component efficiencies and pressure ratios). The engine itself is selected on the Inputs page; these feed the Propulsion Analysis results tab.",
            preview: "engine",
            previewTitle: "Engine cycle preview",
          },
          {
            id: "cabin",
            title: "Cabin & Payload",
            kind: "form",
            group: "cabin",
            description:
              "Passenger-class layout and cargo-deck loading that drive the detailed cabin/payload interior and its mass & CG contribution.",
          },
          {
            id: "control_surfaces",
            title: "Control Surfaces",
            kind: "form",
            group: "control_surfaces",
            description:
              "Slat/flap/aileron/spoiler/elevator/rudder chord- and span-fraction bounds. A representation input for the control-surface sizing diagram only - not fed into the aero/mass models.",
            preview: "control_surfaces",
            previewTitle: "Control-surface layout",
          },
          {
            id: "structures",
            title: "Structures",
            kind: "form",
            group: "structures",
            description:
              "Sizes a generic wingbox (skin/spars/ribs) for the main wing from strength requirements alone - always available, no NASTRAN needed. Set the NASTRAN path and enable run_nastran below to also run a real solve.",
            preview: "structures",
            previewTitle: "Wingbox planform",
          },
        ],
      },
      {
        title: "Analysis",
        pages: [
          {
            id: "optimizer",
            title: "Optimizer & weights",
            kind: "form",
            group: "optimizer",
            description:
              "Differential-evolution solver settings and the cost-function weights. The optimizer minimises -L/D plus the penalty terms weighted here.",
            detail: [
              "The optimizer runs SciPy differential evolution, minimising −L/D plus penalty terms. Each candidate is built, mass-balanced, trimmed, and scored with the fast 2-point aerodynamic estimate; only the winner is then re-run at full sweep fidelity.",
              "The weights scale the soft penalties: static-margin target deviation, tail-volume-coefficient shortfalls, and CG-envelope pressure. A physically invalid candidate (unstable, or outside the CG envelope) still gets a real L/D plus a large continuous penalty — dominant enough that a compliant design always wins, but not so absolute that the solver loses all gradient toward the feasible region.",
              "Population size and max iterations trade run time against thoroughness; seeding near the initial design starts the search from a known-valid point instead of sampling the whole space blindly.",
            ],
            presetKind: "solver",
          },
          {
            id: "analysis",
            title: "Analysis fidelity",
            kind: "form",
            group: "analysis",
            description:
              "Alpha-sweep and VLM fidelity for the final fine analysis - how many points and panels the winning design is scored with.",
            presetKind: "fidelity",
          },
          {
            id: "performance",
            title: "Performance",
            kind: "form",
            group: "performance",
            description:
              "High-lift and field-performance constants (CLmax, thrust lapse, OEI gradient, k_land) behind the Matching Chart and Landing & Take-Off results tabs.",
            presetKind: "performance",
          },
          {
            id: "mission",
            title: "Mission Analysis",
            kind: "form",
            group: "mission",
            description:
              "SUAVE mission settings and the full climb/cruise/descent speed profile, plus route/asset paths. Runs automatically as pipeline Stage 5 when enabled.",
            detail: [
              "Mission analysis runs SUAVE (in an isolated Python 3.10 venv, as pipeline Stage 5) over your departure→arrival airport pair, returning fuel burn, block time, and full climb/cruise/descent telemetry. It's additive — a normal Run never fails because mission analysis couldn't complete.",
              "The profile fields set every climb/cruise/descent speed, rate, and altitude fraction. Routing tries, in order: the SimBrief API (if a username is set), a manual SimBrief KML drop-in, the open-navdata airway graph (one-time download), and finally a great-circle — each falling through to the next so a route always renders.",
            ],
          },
          {
            id: "mses",
            title: "MSES Analysis",
            kind: "form",
            group: "mses",
            description:
              "Optional high-fidelity MSES airfoil analysis. The executables ship bundled - just enable and configure the sweep here.",
            detail: [
              "MSES is a coupled viscous/inviscid Euler + boundary-layer solver — the highest-fidelity 2-D airfoil analysis in the app, and the only one that captures shocks and true wave drag. It runs on the optimized design's root section for the Model Comparison tab, and optionally on Airfoil Screening's finalists.",
              "The section sees the swept effective Mach (M·cos Λ), not freestream. The alpha-sweep half-width brackets the trim CL; widen it if MSES reports the target CL outside its converged range. A non-convergent geometry is an expected solver outcome, not a crash — the run degrades gracefully.",
            ],
          },
          { id: "airfoil_screening", title: "Airfoil Screening", kind: "airfoil_screening" },
        ],
      },
    ],
  },
  {
    title: "Results",
    subgroups: [{ pages: [{ id: "results", title: "Results", kind: "results" }] }],
  },
];

export const ALL_PAGES: Page[] = NAV.flatMap((g) => g.subgroups.flatMap((s) => s.pages));
