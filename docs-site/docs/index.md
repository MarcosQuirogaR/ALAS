# ALAS documentation

ALAS (Aircraft Layout, Analysis and Sizing) sizes an aircraft against a
specified mission and evaluates the resulting configuration using preliminary
engineering models. You define the operational requirements: speed, altitude,
payload capacity, and design range. ALAS searches the geometric design space
and executes coupled multidisciplinary stages across aerodynamics, wingbox
structures, turbofan propulsion cycles, mass and balance, longitudinal
stability, and trajectory simulation.

Stage execution provides detailed solver diagnostics and results in progress,
enabling engineers to examine intermediate convergence and evaluate off-design
behavior. Sizing models for Unmanned Aircraft Systems (UAS) configurations are
currently a work in progress.

## Where to start

| If you want to | Read |
|---|---|
| Install it and run something | [Installation](installation.md) |
| Know what every button does | [User guide](user-guide.md) |
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

**Get started** covers installation, interface controls, and a gallery of
engineering results.

**The AVE walkthrough** is the complete walkthrough: requirements definition,
design-space boundaries, numerical optimization, and discipline-specific
analysis stages.

**Reference** documents pipeline architecture, solver interface internals,
configuration schemas, and theoretical formulas.

**Help** provides troubleshooting guidance and a domain glossary.

## A note on scope & preliminary engineering models

ALAS provides preliminary design capabilities and conceptual exploration. It
addresses feasibility questions such as configuration closure, structural
mass trends, and aerodynamic performance margins.

Important boundaries to note:

- **Not certified or validated aircraft**: Designs generated or analyzed by
  ALAS represent preliminary engineering approximations. They are neither
  certified by aviation authorities nor validated for manufacturing.
- **Approximations & solver limits**: Models reflect conceptual-level physics.
  The engine cycle model simplifies secondary bleed and cooling losses, while
  structural modules provide analytical beam and wingbox estimates; advanced
  finite-element analysis requires external licensed solvers (such as MSC
  NASTRAN).
- **No exact runtime guarantees**: Convergence duration and solver stage
  runtimes depend heavily on optimization bounds, constraint tolerances, and
  mesh resolution.
- **Stage results and diagnostics**: Individual stages execute with active
  status reporting. When an external solver encounters non-convergence or
  mesh breakdown, ALAS records diagnostic status flags without halting the
  broader pipeline.
- **UAS configurations**: Sizing workflows for UAS configurations remain under
  active development.

Where limitations exist, each chapter states them explicitly. ALAS is open
source under AGPL-3.0-or-later and built on AeroSandbox and SUAVE (LGPL-2.1).
