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

Where limitations exist, each chapter states them explicitly. ALAS is open
source under AGPL-3.0-or-later and built on AeroSandbox and SUAVE (LGPL-2.1).
