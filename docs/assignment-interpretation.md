# Cabin layout overhaul: requirements interpretation

**Status:** implementation brief  
**Date:** 2026-08-29  
**Authority:** the user's cabin-preview and whole-application request  
**Evidence:** [cabin source note](research/cabin-layout-sources.md) and the broader
[cabin research](research/cabin-interiors-cargo.md)

## Outcome

ALAS must build one physically coherent cabin layout and use it everywhere:
configuration, sizing, mass/CG, feasibility, reports, 2-D views, and the 3-D live
preview. The cabin builder must allocate the complete usable floor to safety,
accessibility, service, and seating functions in that order. A rendered layout
or a passing geometric check is preliminary screening, never a certification
finding.

## Explicit requirements

| ID | Requirement | Acceptance evidence |
| --- | --- | --- |
| CAB-01 | Replace sparse cabin-preview points with recognizable seats, galleys, lavatories, aisles/exits, and overhead stowage. | A generated narrow-body and wide-body preview show the same positioned objects and counts as their layout summaries. |
| CAB-02 | Use one canonical layout across the whole application, not a preview-only reconstruction. | Given one configuration/hash, GUI estimates, preview, analysis, mass/CG, feasibility, and reports agree on object IDs, positions, classes, and counts. |
| CAB-03 | Support exactly three user-facing seating classes: First, Business, and Economy. | Named presets and Custom expose these three slots in forward-to-aft order; absent classes have zero allocation. |
| CAB-04 | Configure classes from real airline arrangements and class-appropriate geometry. | Preset provenance identifies airline, aircraft variant/configuration, source date, seat counts/shares, and geometric assumptions. |
| CAB-05 | Place galleys and lavatories throughout the cabin coherently. | Monuments occupy real longitudinal/lateral footprints, do not intersect seats/structure/egress, and are distributed to serve cabin zones rather than collected by an arbitrary ratio alone. |
| CAB-06 | Respect the three-dimensional cabin envelope, including vertical space. | Seats, standing/transfer paths, monuments, bins, passenger-service units, and bin-door sweeps have no envelope clashes; unusable tapered sections are rejected. |
| CAB-07 | Model overhead compartments. | Bins have side, deck, station range, envelope, usable capacity/load assumption, clearance, door-sweep, and mass/CG; stowage demand and mobility-aid priority produce explicit residuals. |
| CAB-08 | Screen applicable safety and accessibility constraints before assigning seats. | The layout reports rule/profile, applicable amendment/date, pass/fail/inconclusive status, measured value, limit/proxy, and source for every implemented check. |
| CAB-09 | Remove the user-facing `optimize passenger capacity` control and make complete floor allocation the normal behavior. | The control is absent from the schema/UI/saved-output contract; named and Custom passenger layouts deterministically allocate all usable floor. Legacy inputs are migrated or rejected with a clear message. |
| CAB-10 | Restrict Custom to the desired percentage of First, Business, and Economy. | Custom accepts three class percentages, normalizes or validates them deterministically, and derives seats from the remaining compliant geometry. It does not expose a capacity-optimization switch. |
| CAB-11 | Apply priority order: regulatory/reserved geometry, accessibility, service provisions, then seats. | Tests demonstrate that adding an exit-clearance, accessible-lavatory, wheelchair-stowage, or service requirement removes/relocates seats rather than overlapping or silently relaxing the requirement. |
| CAB-12 | Keep the existing workflow geometry-led; passenger count is not presently a hard requirement. | Capacity is an output of the usable cabin, class allocation, and seat geometry. Any requested-but-unseated count is not used to force an infeasible layout. |

## Required interpretation of “100% floor usage”

“100%” means **100% of usable cabin floor is assigned**, not 100% seats and not
100% geometric packing efficiency. The allocation ledger must close as:

`usable floor = safety/egress + accessibility + service/crew + seating zones`

The usable-floor denominator excludes structure and geometry that the cabin
envelope proves cannot be occupied. Within it, exit access, cross-aisles,
passageways, wheelchair/transfer space, galleys, lavatories, closets and crew
functions are legitimate allocations. Small fragments that cannot safely serve
a function must be reported as an allocation residual; the solver must not
stretch seats or monuments into them merely to display 100%. Gross floor,
usable floor, allocated floor, seat-zone floor, and residual area/length must
remain separate metrics.

## Preset semantics

Published airline counts are **seat shares**, not floor shares. Premium cabins
consume more floor per seat, so the builder must convert a target seat mix into
longitudinal class zones using class pitch, seats abreast, aisle geometry, cabin
section, and reserved monuments/egress. It must not copy seat percentages into
floor-length percentages.

The sourced reference anchors are aircraft-specific, not airline-wide averages:

| Preset | Reference configuration | First | Business | Economy slot | Mapping note |
| --- | --- | ---: | ---: | ---: | --- |
| Ryanair | Boeing 737-8200, 197 seats | 0 (0.00%) | 0 (0.00%) | 197 (100.00%) | All-economy reference. |
| Iberia | A350-900, 348 seats | 0 (0.00%) | 31 (8.91%) | 317 (91.09%) | The source has 24 Premium Economy and 293 Economy seats. With exactly three slots, Premium Economy is deliberately folded into the Economy target bucket; it must not be mislabeled as First. |
| Emirates | A380 three-class configuration, 519 seats | 14 (2.70%) | 76 (14.64%) | 429 (82.66%) | A dated, explicitly three-class A380 anchor; current Emirates A380s have multiple configurations. |

The commercial ordering requested by the user is implemented through both mix
and geometry: Ryanair is densest; Iberia adds a Business zone; Emirates adds
First and a materially larger premium-service footprint. Brand names are
reference archetypes, not claims that every aircraft in an airline fleet uses
the same layout.

## Regulatory and standards boundary

The application must select an explicit certification/operations profile. It
must not silently combine the most convenient parts of different jurisdictions.

- **Type-design screening:** EASA CS-25 or FAA 14 CFR Part 25 governs large-aircraft
  cabin safety topics such as seats/restraints, exit access, aisle width,
  seats-abreast, evacuation, stowage retention, and interior fireworthiness.
- **Operating accessibility:** EU Regulation 1107/2006 and US 14 CFR Part 382
  attach to routes, carriers, aircraft dates/capacities, and operating context;
  they are not interchangeable with type-certification rules.
- **Industry practice:** airline LOPAs and manufacturer/operator data calibrate
  class pitch, width, services, and stowage. “First”, “Business”, and “Economy”
  are commercial products, not regulatory seat categories.
- **No invented minima:** CS/FAR-25 does not establish a universal economy seat
  pitch or seat width. Any comfort/industry bounds must be named, sourced, and
  labeled as design targets rather than regulatory limits.
- **No simple monument compliance ratio:** a heuristic such as one lavatory per
  45 passengers or one galley per 100 passengers is provisioning, not proof of
  regulatory compliance, accessibility, catering adequacy, evacuation, water/
  waste capacity, or crew workflow.

Every result must distinguish `geometric-screen`, `operational-screen`,
`certification-constraint`, and `evidence-required`. Preliminary geometry may
reject a concept, but it may not certify one. Evacuation capability, accessible
lavatory usability, seat/monument crashworthiness, bin retention, oxygen-mask
reach, fire/smoke behavior, and approved LOPA substantiation remain inconclusive
until supported by the required analysis, test, and authority-approved data.

## Inferred engineering requirements

- Give every seat, row, aisle, exit, monument, bin segment, wheelchair space,
  crew station, and deck a stable ID and source-layout hash.
- Use SI internally; retain source units and conversions in provenance.
- Build forward-to-aft service zones with exits and cross-aisles reserved before
  row packing. Prevent monuments and bins from obstructing exits or passageways.
- Couple overhead-bin/PSU segmentation to row pitch and lateral seat blocks;
  check oxygen-mask reach and bin-door/head clearance as evidence gaps unless a
  qualified method and data are present.
- Feed installed seats, monuments, bins, contents assumptions, and positions to
  mass, CG, and loading cases; avoid zero-mass visual-only cabin objects.
- Make the 3-D view a projection of canonical geometry with legible solid glyphs
  and useful layer/legend controls. Rendering quality must not alter physics.
- Fail explicitly when geometry cannot place all required functions. Never
  lower a clearance, omit a monument, or change a class share silently.

## Verification and success criteria

1. **Contract:** one serialized layout round-trips with stable IDs and the same
   summary in every consumer.
2. **Determinism:** identical inputs give identical object positions and floor
   ledger; changes in preset or geometry invalidate/rebuild the preview.
3. **Geometry:** collision, containment, lateral/vertical clearance, aisle,
   exit-access, and deck-envelope tests cover tapered narrow- and wide-body cases.
4. **Allocation:** category totals close to usable floor within an explicitly
   documented numerical tolerance; seating is always the final claimant.
5. **Preset provenance:** tests pin the sourced counts/shares and the Iberia
   Premium-Economy-to-Economy surrogate mapping.
6. **Accessibility:** applicable wheelchair stowage, on-board-wheelchair route,
   transfer/companion seating, movable-armrest, and lavatory checks yield pass,
   fail, or inconclusive, not an unsupported “compliant”.
7. **Visualization:** golden/render review confirms recognizable seats,
   galleys, lavatories, bins, aisles, and exits from useful camera angles without
   clipping or misleading scale.
8. **Regression:** formatting, static checks, relevant crate/workspace tests,
   parity/deviation records, and a reviewed diff form the stable increment.

## Decisions and evidence gaps

- **Decision:** enforce three public class slots. Iberia Premium Economy is a
  documented Economy-bucket surrogate until a future four-class model exists.
- **Decision:** retain airline presets as configuration-specific archetypes and
  expose their provenance; do not imply fleet-wide percentages.
- **Assumption:** “percentages” means desired installed seat share. If product
  intent instead means floor-length share, the UI label and sourced preset
  conversion must change; the two quantities must never share a field.
- **Gap:** no target certification basis, aircraft entry-into-service date,
  route jurisdiction, occupant anthropometry, service level, baggage mix,
  flight duration, or catering concept was supplied. These become explicit
  profile inputs/default assumptions, not hidden universal facts.
- **Gap:** public airline sources do not provide enough dimensional data to
  reproduce approved operator LOPAs, galleys, lavatories, bins, crew stations,
  or exit substantiation. The presets are sizing archetypes, not replicas.

## Requirement-traced implementation order

1. Define the canonical object model, applicability profile, floor ledger, and
   typed residuals (CAB-02, CAB-08, CAB-11).
2. Make the builder reserve envelope/safety/accessibility/service geometry and
   then solve the three-class seat layout (CAB-03 to CAB-07, CAB-12).
3. Replace named presets and Custom semantics; remove/migrate the capacity
   switch (CAB-04, CAB-09, CAB-10).
4. Route the canonical result into mass/CG, feasibility, reports, GUI estimates,
   2-D layout, and 3-D preview (CAB-01, CAB-02).
5. Run contract, geometry, allocation, applicability, preset, rendering, and
   regression verification. Record remaining certification evidence as
   inconclusive rather than closing it by assertion.
