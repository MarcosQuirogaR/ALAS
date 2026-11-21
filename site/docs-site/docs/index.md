# ALAS documentation

ALAS (Aircraft Layout, Analysis and Sizing) sizes an aircraft against a
specified mission and evaluates the resulting configuration using preliminary
engineering models. You define the operational requirements: speed, altitude,
payload capacity, and design range. ALAS searches the geometric design space
and executes coupled multidisciplinary stages across aerodynamics, wingbox
structures, turbofan propulsion cycles, mass and balance, longitudinal
stability, and trajectory simulation.

The pipeline tracks stage-level execution status, recording whether each solver
completed, produced partial results, or was unavailable. Sizing workflows for
Unmanned Aircraft Systems (UAS) configurations remain in active development.

## Where to start

| If you want to | Read |
|---|---|
| Install it and run something | [Installation](installation.md) |
| Know what every button does | [User guide](user-guide.md) |
| Configure external solvers (AVL, OpenVSP, MSES, Nastran) | [External tools guide](external-tools.md) |
| See what it produces | [Gallery](gallery.md) |
| Follow one aircraft all the way through | [Meet AVE](meet-ave.md) |
| Understand how it works internally | [The pipeline, end to end](pipeline-diagram.md) |

## About the worked example

Most chapters follow a single reference aircraft, called **AVE**, a preliminary
long-range widebody twin design that ships with the application. Using one
reference model throughout ensures that metrics in the aerodynamic chapter and
the structural chapter refer to the exact same airframe, allowing direct
cross-discipline evaluation.

Every figure and metric in these documentation pages was recorded from an actual
computational run. None of it was fabricated or reconstructed after the fact.

## How the documentation is organised

**Get started** covers installation, interface controls, external tool setup,
and a gallery of engineering results.

**The AVE walkthrough** is the complete walkthrough: requirements definition,
design-space boundaries, numerical optimization, and discipline-specific
analysis stages.

**Reference** documents pipeline architecture, solver interface internals,
configuration schemas, and theoretical formulas.

**Help** provides troubleshooting guidance and a domain glossary.

## Scope and engineering approximations

ALAS targets conceptual design trade studies and preliminary airframe sizing.
Key operational boundaries include:

- **Preliminary approximations**: Sized airframes represent conceptual engineering
  estimates for trade studies. They are neither certified by aviation authorities
  nor validated for manufacturing.
- **Physical models and solver limits**: Physics models use conceptual-level
  approximations. Wingbox calculations use analytical beam representations;
  detailed structural finite-element checks require user-supplied licensed solvers
  (such as MSC Nastran).
- **External solver dependencies**: High-fidelity stages (Athena AVL, OpenVSP/VSPAERO,
  MSES, and Nastran) run as isolated child processes. When an external tool is absent
  or fails to converge, ALAS logs an explicit status code (`not_run`, `not_configured`,
  or `partial_convergence`) rather than substituting synthetic results.
- **Validation status**: ALAS output has not been validated against measured aircraft
  performance. The comparisons run so far against published transport data are mostly
  transcription checks (confirming that reference values entered as preset inputs are
  carried through the pipeline) and calibrated fits, rather than independent
  predictions. The small number of genuinely independent predictive comparisons
  attempted do not currently meet their declared tolerances. Treat all output as
  unvalidated preliminary estimates.
- **UAS configurations**: Fixed-wing UAS sizing modules remain in active development.

### Known limitations in v1.5

- **Centre of gravity and neutral point of some presets.** On several presets the
  model's absolute centre-of-gravity or neutral-point station is wrong, so the
  balance checks report results no real aircraft has. The cause is the absolute
  station, not the %MAC frame: on every checked preset the clean neutral point
  sits at or behind the main gear. Affected: the operating-empty CG against the
  published or class reference (A320-200, E195-E2, DC-10, A400M); an aerodynamic
  aft CG limit behind the main gear (B747-400, DC-10); and CG-envelope violations
  (A380-800 static-stability floor, B747-400 and E195-E2 minimum nose-gear load,
  A400M nose-gear strength). These are reported as findings, not hidden. Fix
  planned for v1.6 (cards CGNP-V16 and, for the A400M, A400MT). Evidence: the
  v1.5 engineering briefs NP747 (747-class neutral-point residual), FIX2-FRAME
  (%MAC frame and absolute stations) and MASS2 (operating empty CG ledger).
- **Take-off obstacle clearance.** No obstacle survey ALAS is licensed to use
  establishes completeness (trees, buildings, recent construction), so on the
  curated aerodromes the obstacle check can only show a violation, never a
  clearance. A margin inside the survey uncertainty is reported as a warning
  ("obstacle survey completeness not established"), not as a rejection; a
  certain violation still rejects the design. See `docs/design-constraints.md`.
- **Clean-sheet engine sizing near a cruise-plan step.** The cruise plan can change
  the number of cruise levels as the engine scale changes (for one brief: six
  levels to FL350 below an engine factor of about 1.05, one level near 6.7 km
  above it). The thrust requirement then jumps by about 2 %, and the engine
  sizing can find no rating inside its 1 % match band. Such a brief is reported
  as not sized; a slightly different brief mass usually sizes. Fix planned for
  v1.6 (card PLAN-STEP-V16).
- **B747-400 wave drag at its design cruise Mach.** The wing's wave drag at the
  design cruise Mach is 28.4 counts, 5.5 % above the sweep-consistency bound of
  26.9 counts. It is reported, not hidden.

Where limitations exist, each chapter states them explicitly. ALAS is open
source under AGPL-3.0-or-later and built on AeroSandbox and SUAVE (LGPL-2.1).
