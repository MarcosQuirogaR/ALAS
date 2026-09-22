// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The left navigation tree's page model and the detached Advanced Settings
//! tabs.
//!
//! The sidebar shows Setup, Modeling (routine discipline inputs) and Results.
//! Advanced controls live in the detached Advanced Settings window, whose tabs
//! come from [`ADVANCED_SETTINGS_PAGES`]. A page over a configuration group
//! declares which surface of that group it renders, so one authoritative
//! configuration is edited from either place without duplicated forms.

/// Which part of a configuration group a page renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// Every field of the group.
    All,
    /// Routine fields only (the Modeling sidebar page).
    Modeling,
    /// Advanced fields only (the detached Advanced Settings tab).
    Advanced,
}

/// A card of fields from another configuration group shown on a page, so a
/// discipline can gather controls that the schema stores elsewhere.
#[derive(Debug, Clone, Copy)]
pub struct ExtraSection {
    /// Card title.
    pub title: &'static str,
    /// JSON pointer of the node holding the fields, for example `/geometry/wing`.
    pub path: &'static str,
    /// Field names inside that node.
    pub names: &'static [&'static str],
}

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
    /// Which surface of that group the page renders.
    pub surface: Surface,
    /// Cards of fields from other groups rendered after the form.
    pub extra: &'static [ExtraSection],
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
            surface: Surface::All,
            extra: &[],
            description: None,
            detail: &[],
            preview: None,
            preview_title: None,
            preset_kind: None,
        }
    }

    const fn screen(id: &'static str, title: &'static str, kind: PageKind) -> Self {
        Self {
            id,
            title,
            kind,
            group: None,
            surface: Surface::All,
            extra: &[],
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
    Page::screen("inputs", "Inputs", PageKind::Inputs),
    Page::screen("design_space", "Design Space", PageKind::DesignSpace),
    Page::screen("setup_analyses", "Analyses", PageKind::Analyses),
    Page {
        description: Some("Select source-provenanced hardware, set mission and airframe assumptions, and verify generated UAV geometry with the shared production physics core."),
        ..Page::screen("uav", "Fixed-Wing UAV", PageKind::Uav)
    },
];

const AIRFOIL_SECTIONS: &[ExtraSection] = &[
    ExtraSection {
        title: "Wing airfoil sections",
        path: "/geometry/wing",
        names: &["root_airfoil", "tip_airfoil"],
    },
    ExtraSection {
        title: "Tail airfoil section",
        path: "/geometry/empennage",
        names: &["tail_airfoil"],
    },
];

const GEAR_LOAD_LIMITS: &[ExtraSection] = &[ExtraSection {
    title: "Gear load limits",
    path: "/mass_model",
    names: &[
        "nlg_x_fraction",
        "mlg_x_fraction_mac",
        "pct_load_nlg_max",
        "pct_load_mlg_max",
        "pct_load_nlg_min",
        "mlw_fraction_mtow",
    ],
}];

const PROPULSION_MASS_FACTORS: &[ExtraSection] = &[ExtraSection {
    title: "Other parameters",
    path: "/mass_model",
    names: &["propulsion_twr_factor", "propulsion_installation_factor"],
}];

const MODELING_PAGES: &[Page] = &[
    Page {
        description: Some("Passenger-class layout and cargo-deck loading that drive the detailed cabin/payload interior and its mass & CG contribution. Selecting this page shows the interior in the Live Preview."),
        ..Page::form("cabin", "Cabin & Cargo", "cabin")
    },
    Page {
        description: Some("Mass-method and CG-solver assumptions: Torenbeek/Raymer structural relations, compatibility-only MTOW fractions for systems and furnishings, or the architecture-dependent NASA FLOPS transport component build-up. FLOPS inputs, high-lift mass loads and propulsion mass factors are in Advanced Settings."),
        surface: Surface::Modeling,
        preview: Some("mass_cg"),
        preview_title: Some("CG envelope (illustrative)"),
        ..Page::form("mass_model", "Mass", "mass_model")
    },
    Page {
        description: Some("The drag model used by the aerodynamic analysis (parasite build-up plus Korn wave drag) and the airfoil sections assigned to the wing and tail. Induced drag comes from the vortex-lattice solver. The formulas shown match the implemented model."),
        extra: AIRFOIL_SECTIONS,
        preview: Some("drag"),
        preview_title: Some("Drag vs Mach (illustrative)"),
        ..Page::form("aerodynamics", "Aerodynamics", "drag_model")
    },
    Page {
        description: Some("Wingbox layout, materials and gauges for the main-wing structural sizing, plus the gear load limits used by the gear-load checks. Solver cases are selected on Analyses; rib, mesh and excitation settings are in Advanced Settings."),
        surface: Surface::Modeling,
        extra: GEAR_LOAD_LIMITS,
        preview: Some("structures"),
        preview_title: Some("Wingbox planform"),
        ..Page::form("structures", "Structures", "structures")
    },
    Page {
        description: Some("On-design turbofan cycle assumptions (component efficiencies and pressure ratios) of the selected engine. Rating anchors, installation and nacelle placement are in Advanced Settings > Propulsion."),
        surface: Surface::Modeling,
        preview: Some("engine"),
        preview_title: Some("Thermodynamic cycle preview"),
        ..Page::form("engine_designer", "Propulsion", "propulsion_cycle")
    },
];

/// The detached Advanced Settings window's tabs, in order.
pub const ADVANCED_SETTINGS_PAGES: &[Page] = &[
    Page {
        description: Some("The fixed geometric scaffold the design vector morphs against: fuselage, empennage and nacelle placement that is not part of the optimizer search space. Airfoil sections are chosen under Modeling > Aerodynamics. A preset aircraft keeps this geometry protected; use the sandbox for manual geometry experiments."),
        preview: Some("geometry"),
        preview_title: Some("Three-view schematic"),
        ..Page::form("geometry", "Geometry", "geometry")
    },
    Page {
        description: Some("Slat/flap/aileron/spoiler/elevator/rudder chord- and span-fraction bounds. A representation input for the control-surface sizing diagram only, not fed into the aero/mass models."),
        preview: Some("control_surfaces"),
        preview_title: Some("Control-surface layout"),
        ..Page::form("control_surfaces", "Control Surfaces", "control_surfaces")
    },
    Page {
        description: Some("NASA FLOPS transport inputs and structural technology factors, high-lift mass loads and legacy fraction controls consumed by the mass model."),
        surface: Surface::Advanced,
        ..Page::form("mass_advanced", "Mass", "mass_model")
    },
    Page {
        description: Some("Rib and mesh discretization, solver limits and the random-excitation force spectrum of the wingbox analyses."),
        surface: Surface::Advanced,
        ..Page::form("structures_advanced", "Structures", "structures")
    },
    Page {
        description: Some("Rating and cycle anchors of the selected engine, its installation and nacelle placement, and the propulsion mass factors. Preset geometry stays protected."),
        surface: Surface::Advanced,
        extra: PROPULSION_MASS_FACTORS,
        ..Page::form("propulsion_advanced", "Propulsion", "propulsion_cycle")
    },
    Page {
        description: Some("Main- and nose-gear placement, strut sizing and tip-over/strength assumptions used by the CG-envelope and gear-load checks."),
        preview: Some("landing_gear"),
        preview_title: Some("Landing-gear planform"),
        ..Page::form("landing_gear", "Landing Gear", "landing_gear")
    },
    Page {
        description: Some("High-lift and field-performance constants (CLmax, thrust lapse, OEI gradient, k_land) and the take-off/approach speed schedules behind the Matching Chart and Landing & Take-Off results tabs."),
        preset_kind: Some(PresetKind::Performance),
        ..Page::form("performance", "Performance", "performance")
    },
    Page {
        description: Some("Alpha-sweep and VLM fidelity for the final fine analysis: how many points and panels the winning design is scored with."),
        preset_kind: Some(PresetKind::Fidelity),
        ..Page::form("analysis", "Analysis fidelity", "analysis")
    },
    Page {
        description: Some("Native mission settings and the full climb/cruise/descent speed profile, plus route/asset paths. Runs as pipeline Stage 5 in every full run."),
        detail: &[
            "Mission analysis runs natively as pipeline Stage 5 over your departure->arrival airport pair, returning fuel burn, block time, and full climb/cruise/descent telemetry.",
            "The profile fields set every climb/cruise/descent speed, rate, and altitude fraction. Routing tries, in order: the SimBrief API (if a username is set), a manual SimBrief KML drop-in, the open-navdata airway graph (one-time download), and finally a great-circle, each falling through to the next so a route always renders.",
        ],
        surface: Surface::Advanced,
        ..Page::form("mission_advanced", "Mission Analysis", "mission")
    },
    Page {
        description: Some("The mission-sized objective, explicit requirement policies, and the differential-evolution search settings. Whether a run optimizes at all is chosen on Inputs; the selected catalogue engine remains fixed."),
        detail: &[
            "Every candidate is built, mass-balanced and trimmed, then closed by the design mission: mass, centre of gravity, trim, mission fuel and takeoff mass are iterated until the design weights converge. The objective is what that converged mission costs (block fuel, takeoff mass, empty mass or fuel per seat-kilometre); the frozen lift-to-drag formulation of the Python reference is replayed only by the parity fixtures.",
            "Each requirement family (mass and fuel, balance, airworthiness performance, geometry) is a hard constraint by default: a candidate that misses one is infeasible and ranks behind every compliant candidate. A family can be made soft (a priced preference), diagnostic (reported only) or switched off.",
            "The search is L-SHADE differential evolution under the epsilon-constrained method: a population of candidate aircraft evolves generation by generation, a shrinking constraint-violation boundary lets it explore past a locally-blocking limit early on and enforces the limit exactly once that boundary reaches zero, and the reported winner is always the least-violating (or best feasible) design any generation actually evaluated. The run records whether it reached its own convergence test, exhausted its generation budget, or was cancelled; that status describes termination, not a proof of global optimality.",
        ],
        ..Page::form("optimizer", "Optimizer", "optimizer")
    },
    Page {
        description: Some("Optional high-fidelity MSES airfoil analysis. MSES remains a user-supplied installation because its per-seat licence does not authorize redistribution; enable it on Analyses and configure the sweep here."),
        detail: &[
            "MSES is a coupled viscous/inviscid Euler + boundary-layer solver: the highest-fidelity 2-D airfoil analysis in the app, and the only one that captures shocks and true wave drag. It runs on the optimized design's root section for the Model Comparison tab, and optionally on Airfoil Screening's finalists.",
            "The section sees the swept effective Mach (M*cos L), not freestream. The alpha-sweep half-width brackets the trim CL; widen it if MSES reports the target CL outside its converged range. A non-convergent geometry is an expected solver outcome, not a crash: the run degrades gracefully.",
        ],
        ..Page::form("mses", "MSES Analysis", "mses")
    },
    Page::screen(
        "airfoil_screening",
        "Airfoil Screening",
        PageKind::AirfoilScreening,
    ),
    Page::screen("setup_tools", "External Tools", PageKind::Setup),
];

const RESULTS_PAGES: &[Page] = &[Page::screen("results", "Results", PageKind::Results)];

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
        title: "Modeling",
        subgroups: &[NavSubgroup {
            title: None,
            pages: MODELING_PAGES,
        }],
    },
    NavGroup {
        title: "Results",
        subgroups: &[NavSubgroup {
            title: None,
            pages: RESULTS_PAGES,
        }],
    },
];

/// Every page: the sidebar tree first, then the Advanced Settings tabs.
pub fn all_pages() -> impl Iterator<Item = &'static Page> {
    NAV.iter()
        .flat_map(|g| g.subgroups.iter())
        .flat_map(|s| s.pages.iter())
        .chain(ADVANCED_SETTINGS_PAGES.iter())
}

/// The page with the given id, if any.
pub fn page(id: &str) -> Option<&'static Page> {
    all_pages().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {
    use super::{all_pages, page, PageKind, Surface, ADVANCED_SETTINGS_PAGES, NAV};

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
            }
        }
        for page in all_pages() {
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
                .chain(page.extra.iter().map(|section| section.title))
            {
                assert!(catalog.contains_key(text), "missing page text: {text}");
            }
        }
    }

    #[test]
    fn the_sidebar_is_setup_modeling_and_results_with_the_five_modeling_disciplines() {
        let titles: Vec<&str> = NAV.iter().map(|group| group.title).collect();
        assert_eq!(titles, ["Setup", "Modeling", "Results"]);
        let modeling: Vec<&str> = NAV[1].subgroups[0]
            .pages
            .iter()
            .map(|page| page.title)
            .collect();
        assert_eq!(
            modeling,
            [
                "Cabin & Cargo",
                "Mass",
                "Aerodynamics",
                "Structures",
                "Propulsion"
            ]
        );
        assert!(NAV.iter().all(|group| group.title != "Advanced Settings"));
    }

    #[test]
    fn geometry_control_surfaces_and_external_tools_live_only_in_advanced_settings() {
        for id in [
            "geometry",
            "control_surfaces",
            "setup_tools",
            "optimizer",
            "mses",
        ] {
            assert!(
                ADVANCED_SETTINGS_PAGES.iter().any(|page| page.id == id),
                "{id} missing from the Advanced Settings tabs"
            );
            assert!(
                !NAV.iter()
                    .flat_map(|g| g.subgroups.iter())
                    .flat_map(|s| s.pages.iter())
                    .any(|page| page.id == id),
                "{id} still in the sidebar"
            );
        }
        assert_eq!(page("setup_tools").map(|p| p.kind), Some(PageKind::Setup));
    }

    #[test]
    fn split_groups_render_a_modeling_surface_and_an_advanced_surface_exactly_once() {
        for group in ["mass_model", "structures", "propulsion_cycle"] {
            let surfaces: Vec<Surface> = all_pages()
                .filter(|page| page.group == Some(group))
                .map(|page| page.surface)
                .collect();
            assert_eq!(surfaces, [Surface::Modeling, Surface::Advanced], "{group}");
        }
        let ids: Vec<&str> = all_pages().map(|page| page.id).collect();
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(ids.len(), unique.len(), "duplicate page id");
    }
}
