// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The glossary of concepts the global search knows beyond field labels.
//!
//! A concept that describes a whole group of settings (the wing, the tail)
//! names the group's own header, not one of its leaves, so the jump lands on
//! the card the user means.

/// A concept the user may search for that is not a field label.
pub(crate) struct Concept {
    pub(crate) title: &'static str,
    pub(crate) synonyms: &'static str,
    pub(crate) page: &'static str,
    pub(crate) section: Option<&'static str>,
    pub(crate) field: Option<&'static str>,
}

const fn concept(
    title: &'static str,
    synonyms: &'static str,
    page: &'static str,
    section: Option<&'static str>,
    field: Option<&'static str>,
) -> Concept {
    Concept {
        title,
        synonyms,
        page,
        section,
        field,
    }
}

pub(crate) const CONCEPTS: &[Concept] = &[
    concept(
        "MTOW",
        "maximum take-off mass weight sizing",
        "inputs",
        None,
        Some("requirements.mtow_kg"),
    ),
    // The design range is set by the clean-sheet brief and the airport pair,
    // not by a configuration field of its own; likewise the payload page edits
    // its masses in its own cards.
    concept(
        "Design Range",
        "range distance km nmi",
        "inputs",
        None,
        None,
    ),
    concept(
        "Cruise Mach",
        "speed mach number",
        "inputs",
        None,
        Some("requirements.cruise_mach"),
    ),
    concept(
        "Cruise Altitude",
        "flight level fl height",
        "inputs",
        None,
        Some("requirements.cruise_altitude_m"),
    ),
    // Passengers, Engine Selection and Reserves are edited in bespoke cards
    // that are not form fields: they land on the page that holds the card.
    concept("Passengers", "pax seats capacity", "inputs", None, None),
    concept(
        "Departure Airport",
        "origin icao route",
        "inputs",
        None,
        Some("departure_airport"),
    ),
    concept(
        "Arrival Airport",
        "destination icao route",
        "inputs",
        None,
        Some("arrival_airport"),
    ),
    concept(
        "Field Length",
        "toda lda tofl lfl runway take-off landing distance",
        "inputs",
        None,
        Some("toda_override_m"),
    ),
    // The Reference Aircraft and Clean Sheet cards hold no configuration
    // field: choosing them is the page itself.
    concept(
        "Reference Aircraft",
        "preset starting point wizard",
        "inputs",
        None,
        None,
    ),
    concept(
        "Clean Sheet",
        "new aircraft tlar requirements start",
        "inputs",
        None,
        None,
    ),
    concept(
        "Optimization",
        "optimizer objective minimize fuel",
        "inputs",
        Some("Optimizer"),
        Some("optimizer.objective.kind"),
    ),
    concept(
        "Wing Geometry",
        "span area aspect ratio sweep taper chord",
        "inputs",
        Some("Geometry"),
        Some("geometry.wing"),
    ),
    concept(
        "Tail Sizing",
        "empennage stabiliser volume coefficient",
        "inputs",
        Some("Geometry"),
        Some("geometry.empennage"),
    ),
    concept(
        "Engine Selection",
        "thrust turbofan catalogue",
        "inputs",
        None,
        None,
    ),
    concept(
        "Engine Cycle",
        "bypass ratio opr overall pressure ratio efficiency",
        "inputs",
        Some("Propulsion"),
        Some("propulsion_cycle.fan_polytropic_efficiency"),
    ),
    concept(
        "Mass Method",
        "oew empty mass flops torenbeek raymer",
        "inputs",
        Some("Mass"),
        Some("mass_model.mass_architecture"),
    ),
    concept(
        "Drag Model",
        "parasite wave drag polar cd0",
        "inputs",
        Some("Aerodynamics"),
        Some("drag_model.interference_factor_wing"),
    ),
    concept(
        "Airfoil Sections",
        "root tip tail airfoil profile",
        "inputs",
        Some("Aerodynamics"),
        Some("geometry.wing.root_airfoil"),
    ),
    concept(
        "Landing Gear",
        "nose main strut tip-over",
        "inputs",
        Some("Landing Gear"),
        Some("landing_gear.n_nlg_wheels"),
    ),
    concept(
        "Control Surfaces",
        "flap slat aileron spoiler elevator rudder",
        "inputs",
        Some("Control Surfaces"),
        Some("control_surfaces.flap_chord_fraction"),
    ),
    concept(
        "Seat Pitch",
        "economy business first class",
        "cabin",
        None,
        Some("cabin.passenger.economy.pitch_m"),
    ),
    concept(
        "Cabin Layout",
        "seats abreast aisle galley lavatory",
        "cabin",
        None,
        Some("cabin.passenger.economy.abreast"),
    ),
    concept(
        "Mission Profile",
        "climb cruise descent speed rate stages",
        "mission",
        None,
        None,
    ),
    concept(
        "Reserves",
        "alternate diversion contingency fuel policy",
        "mission",
        None,
        None,
    ),
    concept(
        "Field Performance Constants",
        "clmax thrust lapse oei gradient",
        "mission",
        Some("Performance"),
        Some("performance.cl_max_to"),
    ),
    concept(
        "Route",
        "simbrief navdata airway great circle",
        "mission",
        Some("Mission Analysis"),
        None,
    ),
    concept("Payload Mass", "cargo freight load", "payload", None, None),
    // The Analyses page toggles disciplines in its own cards.
    concept(
        "Analyses",
        "enable disable run disciplines",
        "setup_analyses",
        None,
        None,
    ),
    concept(
        "Wing Structure",
        "wingbox nastran sol 101 static modes",
        "setup_analyses",
        Some("Structures"),
        Some("structures.center_spar_enabled"),
    ),
    concept(
        "Analysis Fidelity",
        "vlm panels alpha sweep resolution",
        "setup_analyses",
        Some("Analysis Fidelity"),
        Some("analysis.sweep_n_points"),
    ),
    concept(
        "MSES",
        "airfoil viscous euler solver",
        "setup_analyses",
        Some("MSES Analysis"),
        Some("mses.max_iterations"),
    ),
    // Pages that are a screen of their own, not a configuration field.
    concept(
        "External Tools",
        "install nastran openvsp avl paths",
        "setup_tools",
        None,
        None,
    ),
    concept(
        "Files",
        "save open load aircraft library export import",
        "files",
        None,
        None,
    ),
    concept(
        "Design Space",
        "bounds variables limits",
        "design_space",
        None,
        None,
    ),
    concept(
        "Add-ons",
        "scripts plugins extensions",
        "addons",
        None,
        None,
    ),
    concept(
        "Results",
        "summary figures weight balance polar",
        "results",
        None,
        None,
    ),
    // The load & trim sheet and its CG gate are drawn in the preview of the
    // Mass modelling page (Advanced).
    concept(
        "CG gate",
        "centre of gravity limits envelope loading points load trim sheet balance",
        "mass_model",
        Some("Load & trim sheet"),
        None,
    ),
    concept(
        "Load & trim sheet",
        "cg gate envelope loading points balance centre of gravity",
        "mass_model",
        Some("Load & trim sheet"),
        None,
    ),
    concept(
        "Sandbox",
        "shape aircraft by hand free-form geometry drag AVE",
        "sandbox",
        None,
        None,
    ),
    concept(
        "Files",
        "library saved designs results folder open save storage",
        "files",
        None,
        None,
    ),
    concept(
        "External Tools",
        "executables paths nastran patran mses avl openvsp vspaero openfoam solver",
        "setup_tools",
        None,
        None,
    ),
    concept(
        "Add-ons",
        "plugins scripts extensions python process",
        "addons",
        None,
        None,
    ),
];
