# ALAS aircraft-design research library

Research corpus assembled on 2026-08-26 for the requirements-first aircraft-design work. The individual notes are discipline reviews; the cross-discipline decision record is [AIRCRAFT_DESIGN_DOCTRINE.md](AIRCRAFT_DESIGN_DOCTRINE.md).

## How to use this library

1. Start with the doctrine for the common model, wizard behavior, fidelity ladder, and staged evaluator.
2. Read the discipline note before changing the corresponding crate or requirement.
3. Use the local PDF folder for primary-source reading. Each note contains a source register, access/rights note, source URL or DOI, and SHA-256 for the files it used.
4. Treat every numerical recommendation as conditional on its named load case, validity envelope, and evidence status.

## Discipline map

| Discipline | Research note | Local source folder | Design boundary |
|---|---|---|---|
| Requirements and systems engineering | [requirements-systems.md](requirements-systems.md) | [bib/requirements-systems](../../bib/requirements-systems/) | TLAR, ConOps, requirement quality, allocation, V&V, assurance scope |
| Mission analysis and preliminary sizing | [mission-sizing.md](mission-sizing.md) | [bib/mission-sizing](../../bib/mission-sizing/) | Payload-range, reserves, mass closure, matching diagrams, architecture seeds |
| Cabin, interiors, cargo, and human factors | [cabin-interiors-cargo.md](cabin-interiors-cargo.md) | [bib/cabin-interiors-cargo](../../bib/cabin-interiors-cargo/) | Seats, monuments, overhead bins, exits, evacuation, ULDs, capacity and loading |
| Geometry and configuration synthesis | [geometry-configuration.md](geometry-configuration.md) | [bib/geometry-configuration](../../bib/geometry-configuration/) | Mixed architecture, planform, fuselage, tail, gear, tanks, airfoils, meshes |
| Aerodynamics | [aerodynamics.md](aerodynamics.md) | [bib/aerodynamics](../../bib/aerodynamics/) | Drag buildup, VLM/lifting-line, airfoil/high-lift, transonic and CFD fidelity |
| Propulsion and energy | [propulsion-energy.md](propulsion-energy.md) | [bib/propulsion-energy](../../bib/propulsion-energy/) | Engine families, maps/decks, installation, OEI, hybrid/electric/hydrogen, thermal |
| Aircraft systems | [aircraft-systems.md](aircraft-systems.md) | Cross-references official sources and retained files in existing `bib/` folders | Electrical, hydraulic, pneumatic, ECS/pressurisation, thermal, APU, ice/fire protection, water/waste, normal and failure cases |
| Mass properties and balance | [mass-balance.md](mass-balance.md) | [bib/mass-balance](../../bib/mass-balance/) | Empty mass, systems, fuel, CG, inertia, loading sequences, uncertainty |
| Stability and control | [stability-control.md](stability-control.md) | [bib/stability-control](../../bib/stability-control/) | Static margin, trim, control authority, dynamic modes, handling qualities, gust |
| Performance and airport compatibility | [performance-airport.md](performance-airport.md) | [bib/performance-airport](../../bib/performance-airport/) | TOFL, landing, climb, OEI, VMO/MMO, approach, runway, pavement and airport |
| Structures and aeroelasticity | [structures-aeroelasticity.md](structures-aeroelasticity.md) | [bib/structures-aeroelasticity](../../bib/structures-aeroelasticity/) | Loads, wingbox, buckling, fatigue, damage tolerance, flutter and FEA gates |
| Optimization and MDO | [optimization-mdo.md](optimization-mdo.md) | [bib/optimization-mdo](../../bib/optimization-mdo/) | Mixed variables, DOE, feasibility-first ranking, multifidelity, robust search |
| Digital thread and interoperability | [digital-thread.md](digital-thread.md) | [bib/digital-thread](../../bib/digital-thread/) | CPACS, UID graph, units/frames, solver adapters, manifests, provenance |
| Certification and safety | [certification-safety.md](certification-safety.md) | [bib/certification-safety](../../bib/certification-safety/) | Certification profile, FHA/PSSA-style screening, failure conditions, evidence |
| Environment and lifecycle | [environment-lifecycle.md](environment-lifecycle.md) | [bib/environment-lifecycle](../../bib/environment-lifecycle/) | Fuel/energy climate impact, noise, non-CO2, LCA, manufacturing and end-of-life |
| Operations, economics, and maintainability | [operations-economics.md](operations-economics.md) | [bib/operations-economics](../../bib/operations-economics/) | Schedule, turnaround, DOC/LCC, dispatch, maintenance, crew, airport operations |

The dependency-ordered implementation programme and complete research-to-code
traceability matrix are maintained in
[INTEGRATION_PLAN.md](INTEGRATION_PLAN.md).
Machine-readable finding dispositions use
[`implementation-ledger.schema.json`](implementation-ledger.schema.json). A
finding cannot be called implemented until its ledger row is `Verified` and
names retained evidence; planned code or an unverified implementation is not a
release claim.

## Evidence boundary

The local corpus is a research archive, not a certification library. Public-domain, government, open-license, and openly accessible files were preferred. A source that is cited in a note but not copied locally is intentionally outside the redistribution boundary; the note records the official source instead.

Two retained files failed independent PDF parsing and must not be treated as evidence:

- `bib/cabin-interiors-cargo/nasa-20140011907-double-deck-aircraft-concept.pdf`
- `bib/digital-thread/nasa_sp_2016_6105_rev2.pdf`

The corresponding notes identify these files and, where available, the valid replacement or canonical source. They remain in place so a failed acquisition is auditable rather than silently erased.

## Review rule

No design decision should cite only a paper title. Cite the discipline note, identify the source ID or local PDF, state the modelling fidelity, and record whether the result is a hard constraint, soft target, objective, diagnostic, or evidence gap.
