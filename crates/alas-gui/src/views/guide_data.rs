// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Advanced Walkthrough's chapter content.
//!
//! A direct port of the reference desktop app's `AdvancedWalkthrough.tsx`
//! `CHAPTERS`: what each discipline computes, what model sits behind it, and
//! where it can mislead. A reading document, not a spotlight tour.

/// One section within a chapter: a heading and its paragraphs.
pub struct Section {
    /// The section heading.
    pub heading: &'static str,
    /// Its paragraphs, in order.
    pub body: &'static [&'static str],
}

/// One chapter of the guide.
pub struct Chapter {
    /// Its stable id.
    pub id: &'static str,
    /// Its title, shown in the contents rail and as the chapter heading.
    pub title: &'static str,
    /// A one-line summary shown under the title in the contents rail.
    pub blurb: &'static str,
    /// Its sections, in order.
    pub sections: &'static [Section],
}

/// The whole guide, in reading order.
pub const CHAPTERS: &[Chapter] = &[
    Chapter {
        id: "overview",
        title: "What ALAS does",
        blurb: "The pipeline, and what 'conceptual design' means here.",
        sections: &[
            Section {
                heading: "The five stages",
                body: &[
                    "A Run walks a fixed pipeline. Stage 0 analyses the baseline design you started from -- weight, balance and static margin -- so you can sanity-check a preset before spending time optimising it. Stage 1 searches the design space. Stage 2 re-analyses the winner at high fidelity. Stage 3 exports. Stage 4 draws the figures. Stage 5 flies the mission natively.",
                    "Everything after Stage 2 is additive: mission analysis, MSES and the structural solve are downstream consumers of the design. They never feed back into it, so switching them off changes what you see, never what the optimizer chose.",
                ],
            },
            Section {
                heading: "Two fidelity levels, on purpose",
                body: &[
                    "Inside the optimizer loop each candidate is scored with a fast two-point aerodynamic estimate, because it runs hundreds of times. The winning design is then re-run with a full alpha sweep. Both share the same parasite and wave-drag build-up, so the fast estimate is a consistent reduction of the fine one rather than a different model.",
                    "This is why the reported cruise L/D can differ slightly from the value the optimizer was ranking on. The reported one is the trustworthy number.",
                ],
            },
            Section {
                heading: "What this tool is not",
                body: &[
                    "This is preliminary sizing. The aerodynamics are a vortex-lattice method with empirical drag build-ups, not CFD. Structural masses use Torenbeek and Raymer statistical formulas fitted to real transports; systems and furnishings use explicit compatibility fractions unless the architecture-dependent NASA FLOPS transport method is selected. Treat outputs as a well-founded starting point that tells you which direction to move, not as certification evidence.",
                ],
            },
        ],
    },
    Chapter {
        id: "inputs",
        title: "Requirements & design space",
        blurb: "What you specify, what the optimizer is allowed to change.",
        sections: &[
            Section {
                heading: "Requirements are targets and limits",
                body: &[
                    "Cruise Mach, altitude and MTOW anchor the whole sizing. Maximum wing area and maximum cruise CL are hard constraints; minimum wing loading remains a graded design preference. Target static margin and CG range define the stability envelope every candidate is checked against.",
                    "Maximum wing area and maximum cruise CL are genuine feasibility gates, not preferences: a candidate that exceeds either is rejected outright rather than scored badly.",
                ],
            },
            Section {
                heading: "The design space table",
                body: &[
                    "Each row is one degree of freedom the optimizer may vary, with an initial value and lower/upper bounds. The initial value does double duty: it is the design analysed when you skip optimisation, and the baseline the optimised result is compared against.",
                    "Loading a preset recentres the bounds around that aircraft. Widening bounds explores more but takes longer and produces more invalid candidates; narrowing them is how you ask 'what is the best version of roughly this aeroplane'.",
                ],
            },
            Section {
                heading: "Cabin by percentage, not seat count",
                body: &[
                    "Class mix is specified as a share of cabin floor length, and seat counts are solved from that share together with each class's pitch, seats abreast and the real fuselage geometry. This matches how a cabin is actually specified -- you cannot pick a seat count independently of the geometry that has to hold it.",
                    "Switch Class mix mode to 'count' when you need to pin exact numbers instead.",
                ],
            },
        ],
    },
    Chapter {
        id: "optimizer",
        title: "The optimizer",
        blurb: "How candidates are scored, and why penalties are shaped the way they are.",
        sections: &[
            Section {
                heading: "The cost function",
                body: &[
                    "SciPy's differential evolution minimises -L/D plus a set of weighted penalties: wing area, wing loading, tail volume coefficients, static-margin deviation, CG-envelope violation, fuel-volume shortfall and several geometric-realism terms.",
                    "The weights are yours to tune, but the static-margin term is deliberately kept small relative to L/D. Push it much past ~30 and the optimizer chases an exact stability match instead of exploring shape.",
                ],
            },
            Section {
                heading: "Why invalid designs still get scored",
                body: &[
                    "A candidate that fails to build or evaluate returns a large flat cost and is abandoned. But a candidate that builds fine and is merely physically invalid -- unstable, or outside the CG envelope -- is treated differently: it still gets a real aerodynamic evaluation, and the violation adds a large but continuous penalty on top.",
                    "That distinction is load-bearing. An earlier version blanked out L/D for such candidates, and the solver lost the ability to feel its way toward the feasible region: it could no longer tell 'unstable but aerodynamically promising' from 'unstable and hopeless', and converged on an infeasible best-of-a-bad-lot design.",
                ],
            },
            Section {
                heading: "Reading convergence",
                body: &[
                    "The convergence figure shows best-so-far cost against evaluation count. A curve that flattens early with a wide population spread usually means the bounds are too tight. A curve still descending at the last generation means you stopped too early.",
                ],
            },
        ],
    },
    Chapter {
        id: "aero",
        title: "Aerodynamics & stability",
        blurb: "VLM, drag build-up, trim, and the dynamic modes.",
        sections: &[
            Section {
                heading: "Where drag comes from",
                body: &[
                    "Induced drag comes from the vortex-lattice solution. Parasite drag is a component-by-component flat-plate skin-friction and form-factor build-up with interference factors. Wave drag is a Korn-equation rise past the drag-divergence Mach.",
                    "The drag breakdown figure separates these, which is the fastest way to see whether a design is losing to induced drag (fix the span or the loading) or to wave drag (fix the sweep, thickness or cruise Mach).",
                ],
            },
            Section {
                heading: "Trim is solved, not assumed",
                body: &[
                    "Cruise performance is evaluated at a genuinely trimmed condition: a closed-form solve finds the angle of attack and horizontal-stabiliser incidence that simultaneously produce the required lift and zero pitching moment, then one non-linear VLM point is run there. The L/D you see therefore includes real trim drag.",
                ],
            },
            Section {
                heading: "Static margin and the neutral point",
                body: &[
                    "Static margin is the distance from the centre of gravity to the neutral point, as a fraction of mean aerodynamic chord. Positive means pitch-stable. The app distinguishes the aerodynamic reference point from the real mass-model CG, and enforces the minimum against the physical one -- the honest test.",
                    "Tail efficiency and the fuselage's destabilising contribution both move the neutral point. Turning off the fuselage term will flatter your stability; it is on by default for a reason.",
                ],
            },
        ],
    },
    Chapter {
        id: "screening",
        title: "Airfoil screening",
        blurb: "Three fidelity stages over ~1,600 sections, and how to read the ranking.",
        sections: &[
            Section {
                heading: "Why three stages",
                body: &[
                    "Stage 1 scores every section in the database with a fast 2-D neural-network model, at the swept-section effective Mach and your chord Reynolds. It is cheap enough to sweep everything but flatters thin low-Reynolds sections that would never suit a transport.",
                    "Stage 2 rebuilds the shortlist into your actual wing and re-evaluates with the full 3-D model and a real trim solve. This is what demotes those 2-D flatterers once induced and wave drag on your planform are counted.",
                    "Stage 3 runs MSES, a coupled viscous/inviscid solver, on the finalists. It is the only stage that captures shocks and true wave drag, so it is the only one that can properly credit a genuinely supercritical section.",
                ],
            },
            Section {
                heading: "The reference sections",
                body: &[
                    "A curated set of real, wind-tunnel-validated transonic sections -- the NASA SC(2) family, the original Whitcomb airfoil, RAE 2822 -- is forced through every stage regardless of its score, and marked with a star. They are there as a physical anchor: they tell you how the algorithm's picks compare against sections known to work on real jets, rather than only against each other.",
                ],
            },
            Section {
                heading: "Reading the result honestly",
                body: &[
                    "This is a shortlisting tool. The correct workflow is to take the top few candidates and verify them with a real Run, not to adopt the winner directly. At a transonic cruise Mach the app says so explicitly.",
                    "Off-design robustness is worth enabling: a section that wins only at exactly the design CL is fragile, because real cruise CL wanders with weight and altitude.",
                ],
            },
        ],
    },
    Chapter {
        id: "weights",
        title: "Weight, balance & structures",
        blurb: "Where mass comes from, and what the wingbox solve does.",
        sections: &[
            Section {
                heading: "The mass build-up",
                body: &[
                    "Structural component masses use Torenbeek and Raymer statistical relations driven by MTOW, geometry and load factors. The compatibility baseline keeps systems/equipment and furnishings/operations as explicit MTOW fractions; selecting NASA FLOPS instead requires declared architecture and builds avionics, electrical, hydraulics, cabin and operating items separately. The detailed interior -- a real seat map or ULD load -- is built and its true mass-weighted CG overrides the lumped estimate.",
                    "That detail matters: the lumped payload CG (the geometric centre of the occupied cabin) can differ from the real one by several percent MAC, which is enough to move a design from inside the CG envelope to outside it.",
                ],
            },
            Section {
                heading: "The CG envelope",
                body: &[
                    "The envelope is bounded by stability at the aft limit and by landing-gear load limits -- maximum nose and main gear strength, and the minimum nose load needed for steering authority. A design is only compliant if every loading state, from empty to maximum take-off, sits inside it.",
                ],
            },
            Section {
                heading: "The wingbox",
                body: &[
                    "The structural solve sizes a generic wingbox -- skin, spars, ribs -- from strength requirements, then computes deflections, stresses and natural frequencies analytically. NASTRAN is optional: without it you still get every analytical result.",
                    "It is a downstream analysis. It does not feed the mass model, so a heavy wingbox will not change the optimizer's answer; compare it against the Torenbeek estimate shown beside it as an accuracy check.",
                ],
            },
        ],
    },
    Chapter {
        id: "mission",
        title: "Mission, routing & propulsion",
        blurb: "Native mission, the four routing tiers, and the engine cycle.",
        sections: &[
            Section {
                heading: "Mission analysis",
                body: &[
                    "The native mission flies a full climb/cruise/descent profile for the chosen design over your airport pair, returning fuel burn, block time and complete telemetry. If it cannot complete you get a status and a reason, not fabricated telemetry.",
                ],
            },
            Section {
                heading: "Routing has four tiers",
                body: &[
                    "In order: your live SimBrief flight plan, a manually exported SimBrief KML, an open airway graph, and finally a great circle. Each falls through to the next, so a route always renders.",
                    "When SimBrief returns a plan for a different city pair than the one selected, it takes precedence by default and the mission is sized against the airports actually flown -- a real dispatched plan is the most accurate routing available.",
                ],
            },
            Section {
                heading: "The engine cycle",
                body: &[
                    "The propulsion tab evaluates a two-spool turbofan cycle from first-principles thermodynamics with polytropic component efficiencies. Because those efficiencies are generic rather than a specific manufacturer's hardware, the computed fuel consumption runs systematically optimistic-to-pessimistic by roughly 20% against published figures. Engine ordering is reliable; absolute values are not.",
                ],
            },
        ],
    },
    Chapter {
        id: "practice",
        title: "Working effectively",
        blurb: "Habits that make the tool tell you the truth.",
        sections: &[
            Section {
                heading: "Start from a preset",
                body: &[
                    "Presets are calibrated so their nominal design and the all-variables-at-minimum corner both stay feasible. Run Analyze baseline first and confirm the CG and static margin look sane -- if the baseline is wrong, everything downstream is wrong.",
                ],
            },
            Section {
                heading: "Change one thing at a time",
                body: &[
                    "The cost function couples nearly everything. If you change three weights and the answer improves, you have learned very little. Change one, re-run, and read the convergence and drag-breakdown figures before moving on.",
                ],
            },
            Section {
                heading: "Trust the reported numbers, not the loop's",
                body: &[
                    "The optimizer's in-loop estimates exist to rank candidates cheaply. Quote the final analysis figures. If the two disagree sharply, the usual cause is a coarse in-loop panel resolution under-capturing a cambered section's camber line.",
                ],
            },
        ],
    },
];

#[cfg(test)]
mod tests {
    use super::CHAPTERS;

    #[test]
    fn every_advanced_guide_sentence_has_a_spanish_desktop_translation() {
        let catalog = alas_i18n::es::desktop_catalog();
        for chapter in CHAPTERS {
            for text in [chapter.title, chapter.blurb] {
                assert!(catalog.contains_key(text), "missing guide text: {text}");
            }
            for section in chapter.sections {
                assert!(
                    catalog.contains_key(section.heading),
                    "missing guide heading: {}",
                    section.heading
                );
                for paragraph in section.body {
                    assert!(
                        catalog.contains_key(*paragraph),
                        "missing guide paragraph: {paragraph}"
                    );
                }
            }
        }
    }
}
