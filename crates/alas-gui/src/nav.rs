// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The left navigation tree's page model.
//!
//! A direct port of the reference desktop app's `pages.ts`: the Setup /
//! Advanced Settings (Modeling, Analysis) / Results tree, where most pages map
//! to one configuration group rendered by the generic form and a handful are
//! bespoke screens. Adding a page is a data change here, exactly as it was a
//! data change there.

/// What a page renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageKind {
    /// The bespoke Inputs screen.
    Inputs,
    /// The design-space bounds table.
    DesignSpace,
    /// The results gallery.
    Results,
    /// A generic form over one configuration group.
    Form,
    /// The External Tools screen.
    Setup,
    /// The Analyses toggles screen.
    Analyses,
    /// The airfoil-screening screen.
    AirfoilScreening,
    /// The fixed-wing UAV component and sizing workflow.
    Uav,
}

/// Which aux-preset registry a form page offers a picker for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetKind {
    /// Analysis-fidelity presets.
    Fidelity,
    /// Differential-evolution solver presets.
    Solver,
    /// Field-performance presets.
    Performance,
}

impl PresetKind {
    /// The registry key, matching the reference's `/config/aux-presets/{kind}`.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Fidelity => "fidelity",
            Self::Solver => "solver",
            Self::Performance => "performance",
        }
    }
}

/// One navigable page.
#[derive(Debug, Clone)]
pub struct Page {
    /// Its stable id, used as the active-page key.
    pub id: &'static str,
    /// Its title in the sidebar and header.
    pub title: &'static str,
    /// What it renders.
    pub kind: PageKind,
    /// The configuration group a `Form` page edits.
    pub group: Option<&'static str>,
    /// A one-paragraph description shown under the title.
    pub description: Option<&'static str>,
    /// "How this works" deep-dive paragraphs, hidden unless help is verbose.
    pub detail: &'static [&'static str],
    /// A live preview figure id shown beside the form.
    pub preview: Option<&'static str>,
    /// The preview panel's title.
    pub preview_title: Option<&'static str>,
    /// Which aux-preset picker, if any, this page offers.
    pub preset_kind: Option<PresetKind>,
}

impl Page {
    const fn form(id: &'static str, title: &'static str, group: &'static str) -> Self {
        Self {
            id,
            title,
            kind: PageKind::Form,
            group: Some(group),
            description: None,
            detail: &[],
            preview: None,
            preview_title: None,
            preset_kind: None,
        }
    }
}

/// A titled subgroup of pages within a top-level nav group.
#[derive(Debug, Clone)]
pub struct NavSubgroup {
    /// The subgroup's heading, or `None` for the group's ungrouped pages.
    pub title: Option<&'static str>,
    /// Its pages, in order.
    pub pages: &'static [Page],
}

/// A top-level nav group.
#[derive(Debug, Clone)]
pub struct NavGroup {
    /// Its heading.
    pub title: &'static str,
    /// Its subgroups.
    pub subgroups: &'static [NavSubgroup],
}

const SETUP_PAGES: &[Page] = &[
    Page {
        id: "inputs",
        title: "Inputs",
        kind: PageKind::Inputs,
        group: None,
        description: None,
        detail: &[],
        preview: None,
        preview_title: None,
        preset_kind: None,
    },
    Page {
        id: "design_space",
        title: "Design Space",
        kind: PageKind::DesignSpace,
        group: None,
        description: None,
        detail: &[],
        preview: None,
        preview_title: None,
        preset_kind: None,
    },
    Page {
        id: "uav",
        title: "Fixed-Wing UAV",
        kind: PageKind::Uav,
        group: None,
        description: Some("Select source-provenanced hardware, set mission and airframe assumptions, and verify generated UAV geometry with the shared production physics core."),
        detail: &[],
        preview: None,
        preview_title: None,
        preset_kind: None,
    },
    Page {
        id: "setup_analyses",
        title: "Analyses",
        kind: PageKind::Analyses,
        group: None,
        description: None,
        detail: &[],
        preview: None,
        preview_title: None,
        preset_kind: None,
    },
    Page {
        id: "setup_tools",
        title: "External Tools",
        kind: PageKind::Setup,
        group: None,
        description: None,
        detail: &[],
        preview: None,
        preview_title: None,
        preset_kind: None,
    },
];

const MODELING_PAGES: &[Page] = &[
    Page {
        description: Some("Coefficients of the Raymer/Korn parasite- and wave-drag build-up used by the aerodynamic model. Tune the CD0 floor and the drag-divergence Mach behaviour."),
        detail: &[
            "The aerodynamic model builds cruise drag as parasite drag (a flat-plate skin-friction + form-factor build-up per component, Raymer Ch. 12) plus compressibility/wave drag (a Korn-equation drag rise past the drag-divergence Mach). Induced drag comes from the VLM, not from this page.",
            "The CD0 floor sets a minimum parasite level so a degenerate geometry can't score an unrealistically clean polar. The drag-divergence behaviour (technology factor and the Mach at which wave drag switches on) controls how sharply drag rises toward cruise Mach -- raise the technology factor for a more modern supercritical wing.",
        ],
        preview: Some("drag"),
        preview_title: Some("Drag vs Mach (illustrative)"),
        ..Page::form("drag_model", "Drag model", "drag_model")
    },
    Page {
        description: Some("The fixed geometric scaffold the design vector morphs against - fuselage, empennage and nacelle placement that isn't part of the optimizer search space."),
        preview: Some("geometry"),
        preview_title: Some("Three-view schematic"),
        ..Page::form("geometry", "Geometry scaffold", "geometry")
    },
    Page {
        description: Some("Mass-method and CG-solver assumptions: Torenbeek/Raymer structural relations, compatibility-only MTOW fractions for systems and furnishings, or the architecture-dependent NASA FLOPS transport component build-up."),
        preview: Some("mass_cg"),
        preview_title: Some("CG envelope (illustrative)"),
        ..Page::form("mass_model", "Mass model", "mass_model")
    },
    Page {
        description: Some("Main- and nose-gear placement, strut sizing and tip-over/strength assumptions used by the CG-envelope and gear-load checks."),
        preview: Some("landing_gear"),
        preview_title: Some("Landing-gear planform"),
        ..Page::form("landing_gear", "Landing Gear", "landing_gear")
    },
    Page {
        description: Some("On-design turbofan cycle assumptions (component efficiencies and pressure ratios). The engine itself is selected on the Inputs page; these feed the Propulsion Analysis results tab."),
        preview: Some("engine"),
        preview_title: Some("Nacelle profile preview"),
        ..Page::form("engine_designer", "Engine Designer", "propulsion_cycle")
    },
    Page {
        description: Some("Passenger-class layout and cargo-deck loading that drive the detailed cabin/payload interior and its mass & CG contribution."),
        ..Page::form("cabin", "Cabin & Payload", "cabin")
    },
    Page {
        description: Some("Slat/flap/aileron/spoiler/elevator/rudder chord- and span-fraction bounds. A representation input for the control-surface sizing diagram only - not fed into the aero/mass models."),
        preview: Some("control_surfaces"),
        preview_title: Some("Control-surface layout"),
        ..Page::form("control_surfaces", "Control Surfaces", "control_surfaces")
    },
    Page {
        description: Some("Sizes a generic wingbox (skin/spars/ribs) for the main wing from strength requirements alone - always available, no NASTRAN needed. Set the NASTRAN path and enable run_nastran below to also run a real solve."),
        preview: Some("structures"),
        preview_title: Some("Wingbox planform"),
        ..Page::form("structures", "Structures", "structures")
    },
];

const ANALYSIS_PAGES: &[Page] = &[
    Page {
        description: Some("The mission-sized objective, explicit requirement policies, and the single MADS search. Choose clean-sheet or reference adaptation on Design Space; the selected catalogue engine remains fixed."),
        detail: &[
            "Every candidate is built, mass-balanced and trimmed, then closed by the design mission: mass, centre of gravity, trim, mission fuel and takeoff mass are iterated until the design weights converge. The objective is what that converged mission costs (block fuel, takeoff mass, empty mass or fuel per seat-kilometre); the frozen lift-to-drag formulation of the Python reference is replayed only by the parity fixtures.",
            "Each requirement family (mass and fuel, balance, airworthiness performance, geometry) is a hard constraint by default: a candidate that misses one is infeasible and ranks behind every compliant candidate. A family can be made soft (a priced preference), diagnostic (reported only) or switched off.",
            "MADS (Mesh Adaptive Direct Search) polls deterministic, signed directions on a refining mesh and uses a progressive barrier to rank hard feasibility before mission cost. The run records whether its evaluation budget, mesh, iteration limit or fixed bounds stopped the search; that status describes termination, not a proof of global optimality.",
        ],
        preset_kind: None,
        ..Page::form("optimizer", "Optimizer", "optimizer")
    },
    Page {
        description: Some("Alpha-sweep and VLM fidelity for the final fine analysis - how many points and panels the winning design is scored with."),
        preset_kind: Some(PresetKind::Fidelity),
        ..Page::form("analysis", "Analysis fidelity", "analysis")
    },
    Page {
        description: Some("High-lift and field-performance constants (CLmax, thrust lapse, OEI gradient, k_land) behind the Matching Chart and Landing & Take-Off results tabs."),
        preset_kind: Some(PresetKind::Performance),
        ..Page::form("performance", "Performance", "performance")
    },
    Page {
        description: Some("Native mission settings and the full climb/cruise/descent speed profile, plus route/asset paths. Runs automatically as pipeline Stage 5 when enabled."),
        detail: &[
            "Mission analysis runs natively as pipeline Stage 5 over your departure->arrival airport pair, returning fuel burn, block time, and full climb/cruise/descent telemetry.",
            "The profile fields set every climb/cruise/descent speed, rate, and altitude fraction. Routing tries, in order: the SimBrief API (if a username is set), a manual SimBrief KML drop-in, the open-navdata airway graph (one-time download), and finally a great-circle -- each falling through to the next so a route always renders.",
        ],
        ..Page::form("mission", "Mission Analysis", "mission")
    },
    Page {
        description: Some("Optional high-fidelity MSES airfoil analysis. The executables ship bundled - just enable and configure the sweep here."),
        detail: &[
            "MSES is a coupled viscous/inviscid Euler + boundary-layer solver -- the highest-fidelity 2-D airfoil analysis in the app, and the only one that captures shocks and true wave drag. It runs on the optimized design's root section for the Model Comparison tab, and optionally on Airfoil Screening's finalists.",
            "The section sees the swept effective Mach (M*cos L), not freestream. The alpha-sweep half-width brackets the trim CL; widen it if MSES reports the target CL outside its converged range. A non-convergent geometry is an expected solver outcome, not a crash -- the run degrades gracefully.",
        ],
        ..Page::form("mses", "MSES Analysis", "mses")
    },
    Page {
        id: "airfoil_screening",
        title: "Airfoil Screening",
        kind: PageKind::AirfoilScreening,
        group: None,
        description: None,
        detail: &[],
        preview: None,
        preview_title: None,
        preset_kind: None,
    },
];

const RESULTS_PAGES: &[Page] = &[Page {
    id: "results",
    title: "Results",
    kind: PageKind::Results,
    group: None,
    description: None,
    detail: &[],
    preview: None,
    preview_title: None,
    preset_kind: None,
}];

/// The whole navigation tree, in display order.
pub const NAV: &[NavGroup] = &[
    NavGroup {
        title: "Setup",
        subgroups: &[NavSubgroup {
            title: None,
            pages: SETUP_PAGES,
        }],
    },
    NavGroup {
        title: "Advanced Settings",
        subgroups: &[
            NavSubgroup {
                title: Some("Modeling"),
                pages: MODELING_PAGES,
            },
            NavSubgroup {
                title: Some("Analysis"),
                pages: ANALYSIS_PAGES,
            },
        ],
    },
    NavGroup {
        title: "Results",
        subgroups: &[NavSubgroup {
            title: None,
            pages: RESULTS_PAGES,
        }],
    },
];

/// The page with the given id, if any.
pub fn page(id: &str) -> Option<&'static Page> {
    NAV.iter()
        .flat_map(|g| g.subgroups.iter())
        .flat_map(|s| s.pages.iter())
        .find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::NAV;

    #[test]
    fn every_navigation_sentence_has_a_spanish_desktop_translation() {
        let catalog = alas_i18n::es::desktop_catalog();
        for group in NAV {
            assert!(
                catalog.contains_key(group.title),
                "missing nav group: {}",
                group.title
            );
            for subgroup in group.subgroups {
                if let Some(title) = subgroup.title {
                    assert!(catalog.contains_key(title), "missing nav subgroup: {title}");
                }
                for page in subgroup.pages {
                    assert!(
                        catalog.contains_key(page.title),
                        "missing page title: {}",
                        page.title
                    );
                    for text in page
                        .description
                        .into_iter()
                        .chain(page.detail.iter().copied())
                        .chain(page.preview_title)
                    {
                        assert!(catalog.contains_key(text), "missing page text: {text}");
                    }
                }
            }
        }
    }
}
