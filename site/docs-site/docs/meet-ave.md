# Meet AVE

Every example in this guide uses one reference aircraft: **AVE**, a preliminary
long-range widebody twin. It is the preset the desktop application opens on.

## Why a reference case exists

An optimizer over sixteen geometric variables and a multidisciplinary physics
stack needs a fixed baseline to be runnable and checkable from the first launch.
AVE is an uncertified conceptual model dimensioned around open-literature
777X-class twin-aisle data. Its numbers are estimates with no manufacturer
validation. It is the starting point of [Sandbox mode](sandbox.md): the first time you
open the sandbox it loads AVE's geometry.

## The airframe, as ALAS sees it

Main design-vector values set by the AVE preset (the remaining variables sit at
their defaults):

| Parameter | Value |
|---|---|
| Span | 71.75 m |
| Root chord | 16.00 m |
| Break (yehudi) chord | 8.00 m |
| Tip chord | 1.60 m |
| Leading-edge sweep (inboard) | 34.0° |
| Tip washout | 0.0° |
| Wing longitudinal shift | +0.5 m |
| Fuselage length | 76.72 m |
| Root airfoil | SC(2)-0714 (NASA supercritical) |
| Tip airfoil | sc20410 |

The geometry builder resolves that vector to:

<div class="ave-stat-grid" markdown>
<div class="ave-stat"><div class="label">Wing area</div><div class="value">525.2 m²</div></div>
<div class="ave-stat"><div class="label">Aspect ratio</div><div class="value">9.80</div></div>
<div class="ave-stat"><div class="label">MAC</div><div class="value">9.49 m</div></div>
<div class="ave-stat"><div class="label">Taper ratio</div><div class="value">0.100</div></div>
<div class="ave-stat"><div class="label">H-stab area</div><div class="value">112.2 m²</div></div>
<div class="ave-stat"><div class="label">V-stab area</div><div class="value">60.3 m²</div></div>
</div>

<figure markdown>
  ![Four-panel three-dimensional view](assets/ave-threeview-3d-dark.png)
  <figcaption>The optimized AVE geometry rendered in three dimensions (top, front, side and isometric) directly from the geometry pipeline.</figcaption>
</figure>

## The mission

M0.84 cruise at 11,887 m (FL390), a 358.7 t MTOW and 350 passengers. These are
sizing inputs, not certified limits.

## The engine

Two GE9X-class turbofans, modelled by the cycle in [Propulsion analysis](propulsion-analysis.md):

| Parameter | Value |
|---|---|
| Bypass ratio (BPR) | 10.0 |
| Overall pressure ratio (OPR) | 60.0 |
| Fan pressure ratio (FPR) | 1.45 |
| Turbine inlet temperature (TIT) | 1670 K |
| Rated static thrust, per engine | 467.0 kN |

## Presets

Eleven other presets ship: the airliners A340-300, A380-800, B787-9, A320-200,
A220-300, DC-10, ATR72-600, E195-E2, C919 and B747-400, and the A400M freighter.
Select one on the Inputs page or under File, Load preset; geometry, cabin,
engine, landing gear, fuel tanks and design-space bounds change together. From
v1.3.2 (not in the published v1.3.1 package) the B747-400 has its upper-deck
hump and seven presets carry nose geometry fitted to manufacturer
general-dimensions drawings. Published values are inputs, not validation (see
[Validation](validation.md)).

## Where AVE's numbers come from

Every number and figure in the walkthrough comes from one run of the registered AVE
preset, started as the desktop application opens it and launched through the same
Run path:

- seed 42, optimizer on, baseline comparison on, vortex-lattice aerodynamics
  with the AVL 3.52 cross-check;
- MSES 3.12 (300 iterations, 300 s limit per point), MSC Nastran 2026.1 Student
  Edition (SOL 101, 103, 111), NASTRAN-95 (SOL 101, 103) and Patran 2026.1;
- a SimBrief dispatch plan, EGLL to OMDB (89 waypoints, 6,362 km, AIRAC 2610);
- OpenVSP, VSPAERO and FLOWUnsteady not installed, so their stages report
  unavailable and have no figures here.

The figures are drawn by the same code path as the desktop results view and
`ALAS --plots`, and the numbers are read from the run's output files
(`design_database.json`, `solvers/vlm/optimization_search.json`,
`payload_layout.json`, the run log and the solver files).

!!! warning "Read these results with their limits"
    - The optimizer stages stop on their time limits, so the same seed gave spans
      of 71.28, 68.70 and 70.17 m in three runs; the numbers describe this run
      only. Stopping on evaluation budgets only, or replaying the recorded stage
      counts, reproduces a run exactly.
    - With the finite-element solvers on, the delivered design is rejected after
      downstream analysis on four root-stress flags (see
      [Structural analysis](structural-analysis.md#finite-element-strength-flags)).
      The same run without them had no blocking findings.
    - MSES converged at only 2 of 7 sweep points, both past the lift peak, so the
      transonic figures are not cruise predictions
      ([Transonic section analysis](transonic-analysis.md)).
    - The static margin (42.6 % MAC) is a known model residual.

Stage execution times vary with hardware and solver tolerances, and ALAS gives no
runtime guarantee. Differences between the baseline and optimized evaluations are
in [Optimization results](optimization-results.md).
