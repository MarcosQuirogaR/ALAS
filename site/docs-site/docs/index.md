# ALAS documentation

ALAS (Aircraft Layout, Analysis and Sizing) sizes a transport aircraft against
a mission and evaluates the result with conceptual-level models. You set speed,
altitude, payload and range. ALAS searches the geometric design space and runs
coupled aerodynamics, wingbox structures, turbofan cycle, mass and balance,
longitudinal stability and mission stages. Each stage records whether it
completed, produced partial results or was unavailable.

Twelve aircraft presets ship with the application. [Sandbox mode](sandbox.md)
lets you draw an aircraft that is not in the list, and a separate workflow
covers small electric fixed-wing UAVs.

## Where to start

| If you want to | Read |
|---|---|
| Install it and run something | [Installation](installation.md) |
| Know what every button does | [User guide](user-guide.md) |
| Draw and analyse your own aircraft geometry | [Sandbox mode](sandbox.md) |
| Configure external solvers (AVL, OpenVSP, MSES, Nastran) | [External tools guide](external-tools.md) |
| See what it produces | [Gallery](gallery.md) |
| Follow one aircraft all the way through | [Meet AVE](meet-ave.md) |
| Understand how it works internally | [The pipeline, end to end](pipeline-diagram.md) |
| See what has and has not been checked against published data | [Validation](validation.md) |

## About the worked example

Most chapters follow **AVE**, the preliminary long-range twin-engine widebody
preset that ships with the application, so every chapter refers to the same
airframe. Every figure and number in the walkthrough comes from one recorded run
of the registered AVE preset (seed 42, optimizer on, with AVL, MSES, MSC Nastran,
NASTRAN-95 and Patran, and a SimBrief route). The numbers are read from that run's
output files; see [Meet AVE](meet-ave.md#where-aves-numbers-come-from).

## How the documentation is organised

- **Get started**: installation, interface, Sandbox mode, external tools, gallery.
- **The AVE walkthrough**: requirements, design space, optimization, then each
  analysis stage.
- **Reference**: pipeline, architecture, configuration schema, formulas, validation.
- **Help**: troubleshooting, glossary, licensing.

## Scope and limits

ALAS targets conceptual design trade studies. Its output is not certified and
not validated for manufacturing.

- **Models**: conceptual-level. Wingbox sizing uses analytical beam
  representations; finite-element checks need a user-supplied solver such as
  MSC Nastran.
- **External solvers**: AVL, OpenVSP/VSPAERO, MSES and Nastran run as child
  processes. When one is absent or fails, ALAS reports an explicit status
  (`not_run`, `not_configured`, `partial_convergence`) and does not substitute
  synthetic results.
- **Validation**: output has not been validated against measured aircraft
  performance. Comparisons so far against published transport data are mostly
  transcription checks and calibrated fits; the few independent predictions do
  not yet meet their declared tolerances. See [Validation](validation.md).
- **UAV workflow**: separate catalogue-and-sizing workflow with its own
  evidence limits; it is not part of the transport-aircraft pipeline.

ALAS is open source under AGPL-3.0-or-later. It contains code translated from
AeroSandbox and NeuralFoil (MIT) and SUAVE (LGPL-2.1); see
[Licensing](licensing.md).
