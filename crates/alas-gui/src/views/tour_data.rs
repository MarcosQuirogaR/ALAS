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
    /// The Inputs starting-design card's Sandbox Mode button.
    SandboxEntry,
    /// The live-preview dock.
    PreviewDock,
    /// The active central content panel.
    Content,
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

/// The full first-run tour, in order: fourteen displayed steps.
pub const TOUR_STEPS: &[TourStep] = &[
    TourStep {
        title: "Welcome to ALAS",
        body: "A conceptual aircraft design environment. Set requirements, shape or pick an aircraft, then optimize and analyze, with live 3D and chart previews throughout. This quick tour points out the main areas.",
        page: None,
        target: None,
    },
    TourStep {
        title: "Navigate the sidebar",
        body: "The left navigation tree switches the content panel. Click any leaf to open it. You can also pin the sidebar.",
        page: None,
        target: Some(TourTarget::Navigation),
    },
    TourStep {
        title: "Set your requirements",
        body: "On the Inputs page pick a preset aircraft or enter the sandbox mode, then edit the mission requirements and design variables. Which variables you can edit depends on the mode: preset geometry stays protected, and the optimizer may only vary its design variables inside a bounded envelope around the preset. Every field is validated live.",
        page: Some("inputs"),
        target: Some(TourTarget::AircraftConfig),
    },
    TourStep {
        title: "Sandbox mode",
        body: "Sandbox mode lets you shape the aircraft in Discipline Windows. Use Quick Analysis for initial estimates or Full Analysis for detailed results. When you leave, choose whether to discard the sandbox changes or use them as the active design.",
        page: Some("inputs"),
        target: Some(TourTarget::SandboxEntry),
    },
    TourStep {
        title: "3D Live Preview",
        body: "This dock renders your aircraft geometry and cabin layout in real time when in the standard mode. It updates as you edit any input. Scroll to zoom, drag to rotate.",
        page: Some("inputs"),
        target: Some(TourTarget::PreviewDock),
    },
    TourStep {
        title: "Discipline previews",
        body: "Each Modeling page pairs its form with a discipline preview: Cabin & Cargo switches the Live Preview to the interior, Mass shows the CG envelope, Aerodynamics the drag-versus-Mach curve, Structures the wingbox planform and Propulsion the engine installation, all recomputed as you edit.",
        page: Some("cabin"),
        target: Some(TourTarget::Content),
    },
    TourStep {
        title: "Help on every option",
        body: "Hover any field's label for an explanation. Prefer a leaner screen? Toggle in the top bar's \"Help\" > \"Learn-more help\" to hide these instantly.",
        page: Some("aerodynamics"),
        target: Some(TourTarget::Content),
    },
    TourStep {
        title: "Tune the design space",
        body: "In sandbox mode you edit the permitted parameters and their bounds directly in the Parameter Panel and Discipline Windows. With a preset aircraft, this page shows the optimizer's variables with the initial value and bounds of the preset envelope; the preset geometry itself stays protected.",
        page: Some("design_space"),
        target: Some(TourTarget::Content),
    },
    TourStep {
        title: "Choose your analyses",
        body: "Setup > Analyses lists what a full run performs. Core aerodynamics, weight & balance, propulsion, field performance and the mission analysis always run; MSES 2-D airfoil analysis, the wingbox structures solve and its solver cases are configurable here. External-tool paths live under Advanced Settings > External Tools.",
        page: Some("setup_analyses"),
        target: Some(TourTarget::Content),
    },
    TourStep {
        title: "Airfoil Screening",
        body: "The top bar's Airfoil Screening window ranks every airfoil in the database against this design's cruise condition using the Balanced ranking, which weighs aerodynamic efficiency first and then wing fuel capacity and drag-bucket robustness. Set the filters, press Run, and keep working while it runs or cancel any time.",
        page: Some("airfoil_screening"),
        target: Some(TourTarget::Content),
    },
    TourStep {
        title: "Run the analysis",
        body: "Run performs the full analysis of the current design; with Optimize design space on, the optimizer searches the design space first. In sandbox mode, Quick Analysis gives in-process initial estimates and Full Analysis runs the complete pipeline. A stage label and status appear while it runs.",
        page: None,
        target: Some(TourTarget::Run),
    },
    TourStep {
        title: "Follow the Run Log",
        body: "Progress messages stream here: preset loads, engine changes, each pipeline stage, and any errors. In sandbox mode the log opens as a floating window when an analysis starts; it can be minimized, closed and reopened without losing the calculation.",
        page: None,
        target: Some(TourTarget::RunLog),
    },
    TourStep {
        title: "Explore the Results",
        body: "After a run you land here. Tabs cover dozens of figures across all analyzed disciplines. Slots reading \"Not available\" need data this run didn't produce.",
        page: Some("results"),
        target: Some(TourTarget::Content),
    },
    TourStep {
        title: "Explore configuration options",
        body: "Navigate the top bar to switch Dark / Light / Grey themes instantly, to save or load a configuration, and many more options. Replay this tour any time from \"Help\" > \"Replay Walkthrough\".",
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

    #[test]
    fn walkthrough_omits_removed_randomizer_controls() {
        for step in TOUR_STEPS {
            assert!(!step.title.contains("DOE"));
            assert!(!step.title.contains("Random"));
            assert!(!step.body.contains("DOE"));
            assert!(!step.body.contains("Random"));
        }
    }
    #[test]
    fn the_walkthrough_has_fourteen_steps_in_the_requested_order_and_wording() {
        assert_eq!(TOUR_STEPS.len(), 14);
        assert!(TOUR_STEPS[0]
            .body
            .starts_with("A conceptual aircraft design environment."));
        assert_eq!(
            TOUR_STEPS[1].body,
            "The left navigation tree switches the content panel. Click any leaf to open it. You can also pin the sidebar."
        );
        assert!(TOUR_STEPS[2].body.starts_with(
            "On the Inputs page pick a preset aircraft or enter the sandbox mode, then edit the mission requirements and design variables."
        ));
        assert_eq!(TOUR_STEPS[3].title, "Sandbox mode");
        assert_eq!(TOUR_STEPS[3].page, Some("inputs"));
        assert!(TOUR_STEPS[4].body.contains("when in the standard mode"));
        assert!(TOUR_STEPS[6]
            .body
            .contains("\"Help\" > \"Learn-more help\""));
        assert!(TOUR_STEPS[8].body.contains("mission analysis always run"));
        assert!(TOUR_STEPS[9].body.contains("Balanced"));
        assert!(TOUR_STEPS[11].body.contains("floating window"));
        assert_eq!(
            TOUR_STEPS[12].body,
            "After a run you land here. Tabs cover dozens of figures across all analyzed disciplines. Slots reading \"Not available\" need data this run didn't produce."
        );
        assert_eq!(TOUR_STEPS[13].title, "Explore configuration options");
        assert!(TOUR_STEPS[13]
            .body
            .contains("\"Help\" > \"Replay Walkthrough\""));
        for step in TOUR_STEPS {
            for text in [step.title, step.body] {
                for banned in [
                    "--",
                    "no run needed",
                    "no Run needed",
                    "Analyze reference",
                    "MADS",
                    "View > ",
                    "View menu",
                    "barto",
                ] {
                    assert!(!text.contains(banned), "{banned:?} in {text:?}");
                }
            }
            if let Some(page) = step.page {
                assert!(
                    crate::nav::page(page).is_some(),
                    "tour page {page} does not exist"
                );
            }
        }
    }
}
