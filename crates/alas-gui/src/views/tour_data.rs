// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The first-run walkthrough's steps.
//!
//! A direct port of the reference desktop app's `Walkthrough.tsx` `TOUR_STEPS`.
//! The reference spotlights a DOM element per step. This port names the same
//! semantic shell targets; [`crate::app`] records their actual egui response
//! rectangles after layout rather than predicting their pixel geometry.

/// A shell region measured from a real egui response for the spotlight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TourTarget {
    /// The pinned navigation panel or expanded hover rail.
    Navigation,
    /// The Inputs content containing aircraft configuration.
    AircraftConfig,
    /// The live-preview dock.
    PreviewDock,
    /// The active central content panel.
    Content,
    /// The control-bar randomizer group.
    Randomizer,
    /// The control-bar run group.
    Run,
    /// The resizable run-log panel.
    RunLog,
    /// The application menu bar.
    MenuBar,
}

/// One walkthrough step.
pub struct TourStep {
    /// The step's heading.
    pub title: &'static str,
    /// The step's explanation.
    pub body: &'static str,
    /// The page to navigate to before showing this step, if any.
    pub page: Option<&'static str>,
    /// The measured shell region to spotlight, if this is not centered.
    pub target: Option<TourTarget>,
}

/// The full first-run tour, in order.
pub const TOUR_STEPS: &[TourStep] = &[
    TourStep {
        title: "Welcome to ALAS",
        body: "A conceptual transport-aircraft design environment. Set requirements, tune the design space, then optimize and analyze -- with live 3D and chart previews throughout. This quick tour points out the main areas.",
        page: None,
        target: None,
    },
    TourStep {
        title: "Navigate the sidebar",
        body: "The left navigation tree switches the content pane. Click any leaf (Inputs, Design Space, Drag model, ...) to open it.",
        page: None,
        target: Some(TourTarget::Navigation),
    },
    TourStep {
        title: "Set your requirements",
        body: "On the Inputs page pick a preset and engine, then edit the mission requirements. Every field is validated live; error-severity issues block a Run until fixed.",
        page: Some("inputs"),
        target: Some(TourTarget::AircraftConfig),
    },
    TourStep {
        title: "3D Live Preview",
        body: "This dock renders your aircraft geometry and cabin layout in real time -- no run needed. It updates as you edit any input. Scroll to zoom, drag to rotate.",
        page: Some("inputs"),
        target: Some(TourTarget::PreviewDock),
    },
    TourStep {
        title: "Live previews everywhere",
        body: "Advanced Settings pages pair each form with a live chart: drag-vs-Mach, the three-view, CG envelope, landing-gear planform, control-surface layout, wingbox sizing and the engine cycle -- all recomputed as you type.",
        page: Some("control_surfaces"),
        target: Some(TourTarget::Content),
    },
    TourStep {
        title: "Help on every option",
        body: "Hover any field's label for a plain-language explanation, and open a page's \"How this works\" panel for the full method behind it. Prefer a leaner screen? Toggle View > Learn-more help to hide these instantly.",
        page: Some("drag_model"),
        target: Some(TourTarget::Content),
    },
    TourStep {
        title: "Tune the design space",
        body: "The Design Space table holds the optimizer's search variables. Edit the initial value and the lower/upper bounds; loading a preset recenters them. DOE Sample draws a random point here.",
        page: Some("design_space"),
        target: Some(TourTarget::Content),
    },
    TourStep {
        title: "Randomize or explore",
        body: "DOE Sample draws one design within the bounds. Random draws +/-30% beyond them and runs immediately -- handy for probing edge cases.",
        page: None,
        target: Some(TourTarget::Randomizer),
    },
    TourStep {
        title: "Choose your analyses",
        body: "Setup > Analyses picks which disciplines a Run performs. Core aerodynamics, weight & balance, propulsion and field performance always run; native mission, MSES 2-D airfoil and the wingbox structures solve can be toggled -- all on by default. External-tool paths live next door under External Tools.",
        page: Some("setup_analyses"),
        target: Some(TourTarget::Content),
    },
    TourStep {
        title: "Airfoil Screening",
        body: "Advanced Settings > Airfoil Screening ranks every airfoil in the database against this exact design's cruise condition -- no Run needed -- in up to three fidelity stages (2-D proxy -> real 3-D wing -> MSES). Set the objective and filters, press Run, and you can switch tabs while it works or cancel any time.",
        page: Some("airfoil_screening"),
        target: Some(TourTarget::Content),
    },
    TourStep {
        title: "Run the pipeline",
        body: "Run optimizes the design, runs the aerodynamic analysis, and every discipline enabled under Setup > Analyses -- one pass. A stage label and status appear while it runs. Analyze baseline does a quick weight & balance check without the optimizer.",
        page: None,
        target: Some(TourTarget::Run),
    },
    TourStep {
        title: "Follow the Run Log",
        body: "Progress messages stream here -- preset loads, engine changes, each pipeline stage, and any errors.",
        page: None,
        target: Some(TourTarget::RunLog),
    },
    TourStep {
        title: "Explore the Results",
        body: "After a run you land here. Tabs cover Optimization, Aerodynamics, Weight & Balance, Structures, Field Performance and Mission & Route -- dozens of figures. Slots reading \"Not available\" need data this run didn't produce.",
        page: Some("results"),
        target: Some(TourTarget::Content),
    },
    TourStep {
        title: "Themes and files",
        body: "Use the View menu to switch Dark / Light / Grey themes instantly, and File to save or load a configuration. Replay this tour any time from Help > Replay Walkthrough.",
        page: Some("inputs"),
        target: Some(TourTarget::MenuBar),
    },
];

#[cfg(test)]
mod tests {
    use super::TOUR_STEPS;

    #[test]
    fn every_walkthrough_sentence_has_a_spanish_desktop_translation() {
        let catalog = alas_i18n::es::desktop_catalog();
        for step in TOUR_STEPS {
            assert!(
                catalog.contains_key(step.title),
                "missing title: {}",
                step.title
            );
            assert!(
                catalog.contains_key(step.body),
                "missing body: {}",
                step.body
            );
        }
    }
}
