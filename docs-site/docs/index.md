# ALAS documentation

ALAS, Aircraft Layout, Analysis and Sizing, sizes an aircraft against a
mission and then analyses what it came up with. You supply the
requirements: how fast, how high, how heavy, how many passengers. It
searches the geometry for something that meets them, and puts the result
through aerodynamics, structures, propulsion, weight and balance,
stability and a flown mission.

## Where to start

| If you want to | Read |
|---|---|
| Install it and run something | [Installation](installation.md) |
| Know what every button does | [User guide](user-guide.md) |
| See what it produces | [Gallery](gallery.md) |
| Follow one aircraft all the way through | [Meet AVE](meet-ave.md) |
| Understand how it works internally | [The pipeline, end to end](pipeline-diagram.md) |

## About the worked example

Most chapters follow a single aircraft, called AVE, a long-range twin that
ships with the application as its reference design. Using one aircraft
throughout means the numbers in the aerodynamics chapter and the numbers in
the structures chapter describe the same wing, and you can check one
against the other.

Every figure and every number in these pages comes from an actual run. None
of it was written from memory or reconstructed afterwards.

## How the documentation is organised

**Get started** covers installation, the interface, and a gallery of
results.

**The AVE walkthrough** is the long read: requirements, the design space,
the optimiser, and then one chapter per discipline. Read it in order and
you follow an aircraft from a set of requirements to a finished analysis.

**Reference** holds the pipeline diagram, an explanation of the internals,
a field-by-field configuration index, and the formulas with worked numbers.

**Help** is troubleshooting and a glossary.

## A note on scope

ALAS does preliminary design. It answers questions like whether a
configuration closes, roughly what it weighs, and whether the wing survives
the loads. It does not replace a detailed design process, and several of
its models are documented as approximations where they are: the engine
cycle chapter says plainly where its fuel consumption diverges from a real
engine, and the structures chapter says which check needs a licensed
solver.

Where a limitation exists, the relevant chapter states it rather than
leaving you to discover it.
