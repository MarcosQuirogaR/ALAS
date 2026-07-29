# Propulsion analysis

AVE's two GE9X-class turbofans aren't a fixed thrust number pulled from a
spec sheet: ALAS runs an actual on-design thermodynamic cycle
analysis, station by station through the engine, and that cycle is what
[Aerodynamic analysis](aerodynamic-analysis.md)'s drag numbers are
balanced against.

## The on-design cycle

<figure markdown>
  ![GE9X on-design cycle summary](assets/ave-propulsion-cycle-light.png#only-light)
  ![GE9X on-design cycle summary](assets/ave-propulsion-cycle-dark.png#only-dark)
  <figcaption>Station stagnation temperatures through the engine at AVE's M0.84/11.9 km cruise design point.</figcaption>
</figure>

Reading the bar chart left to right follows the air (and, from the
combustor onward, combustion gas) through the engine:

| Station | Temperature | What's happening |
|---|---|---|
| T0 (static) | 218 K | Freestream at 11.9 km |
| Tt2 (fan/LPC face) | 249 K | After the inlet's ram recovery |
| Tt13 (fan exit) | 279 K | Bypass-stream fan work |
| Tt25 (LPC exit) | 263 K | Core-stream low-pressure compression |
| Tt3 (HPC exit) | 876 K | After the high-pressure compressor: OPR = 60 doing its work |
| Tt4 (TIT) | 1,670 K | Combustor exit: the turbine inlet temperature limit |
| Tt45 (HPT exit) | 1,146 K | After extracting work to drive the HPC |
| Tt5 (LPT exit) | 877 K | After extracting work to drive the fan: this is what leaves through the core nozzle |

The jump from Tt3 to Tt4 (876 K → 1,670 K) is the combustor doing its job;
everything downstream of Tt4 is the turbine giving that energy back as
shaft work to drive the compressor and fan, which is why Tt5 (877 K) ends
up close to Tt3: most of the combustor's temperature rise has been
extracted as work by the time the gas reaches the core nozzle.

## Cycle parameters and performance

<div class="ave-stat-grid" markdown>
<div class="ave-stat"><div class="label">Bypass ratio</div><div class="value">10.0</div></div>
<div class="ave-stat"><div class="label">Overall pressure ratio</div><div class="value">60.0</div></div>
<div class="ave-stat"><div class="label">Fan pressure ratio</div><div class="value">1.45</div></div>
<div class="ave-stat"><div class="label">Turbine inlet temp</div><div class="value">1,670 K</div></div>
<div class="ave-stat"><div class="label">Thermal efficiency</div><div class="value">0.518</div></div>
<div class="ave-stat"><div class="label">Propulsive efficiency</div><div class="value">0.645</div></div>
<div class="ave-stat"><div class="label">Overall efficiency</div><div class="value">0.334</div></div>
<div class="ave-stat"><div class="label">Fuel-air ratio</div><div class="value">0.026</div></div>
</div>

Overall efficiency (0.334) is the product of thermal efficiency (how much
of the fuel's energy becomes useful work: 0.518) and propulsive
efficiency (how much of that work becomes useful thrust power rather than
wasted kinetic energy in the exhaust: 0.645). A BPR-10 engine like this
one leans hard on propulsive efficiency: moving a large mass of bypass air
gently is more efficient than moving a small core mass violently, which is
the entire reason high-bypass turbofans replaced low-bypass designs on
long-range transports.

## Thrust

<div class="ave-stat-grid" markdown>
<div class="ave-stat"><div class="label">Per-engine, static rated</div><div class="value">467.0 kN</div></div>
<div class="ave-stat"><div class="label">Per-engine, this cruise pt.</div><div class="value">242.4 kN</div></div>
<div class="ave-stat"><div class="label">Total installed (×2)</div><div class="value">484.8 kN</div></div>
</div>

Cruise thrust (242.4 kN/engine) is roughly half the static rated figure:
expected, since thrust falls with altitude (lower density) and forward
speed (lower pressure ratio across the engine relative to static
conditions) even as the engine keeps running at its design point.

## TSFC: computed vs. reference

```
TSFC (computed)   =  17.39 mg/(N·s)
TSFC (reference)  =  14.16 mg/(N·s)
```

Worth stating plainly rather than glossing over: ALAS's computed
thrust-specific fuel consumption runs about 23% higher than the public
reference figure for a real GE9X. This is a real, known gap between the
on-design cycle model's simplifying assumptions (idealized component
efficiencies, no bleed/extraction losses modeled in detail) and a real
engine's certified performance; the cycle model captures the right
*trends* (efficiency vs. OPR/BPR, thrust vs. altitude) reliably, which is
what makes it useful for the sensitivity studies below, but its absolute
TSFC shouldn't be taken as a substitute for manufacturer data when the
actual number matters. This is documented as a known limitation, not
silently smoothed over.

## Sensitivity studies

Beyond the single on-design point, ALAS sweeps the cycle across
several parameters to show how AVE's specific choice of BPR/OPR sits
relative to the wider design space:

<figure markdown>
  ![Efficiency vs. OPR](assets/ave-propulsion-efficiency-light.png#only-light)
  ![Efficiency vs. OPR](assets/ave-propulsion-efficiency-dark.png#only-dark)
  <figcaption>Thermal, propulsive, and overall efficiency as OPR varies, BPR and TIT held at AVE's values. The GE9X's actual OPR = 60 is marked.</figcaption>
</figure>

Overall efficiency keeps climbing with OPR across this whole range: GE9X's
OPR = 60 isn't sitting at a plateau, it's a point chosen against real
constraints (compressor stage count, material temperature limits, cost)
this simplified cycle model doesn't represent, not against a diminishing-
returns curve visible here.

<figure markdown>
  ![Bypass ratio sensitivity](assets/ave-propulsion-bpr-light.png#only-light)
  ![Bypass ratio sensitivity](assets/ave-propulsion-bpr-dark.png#only-dark)
  <figcaption>Specific thrust and TSFC as BPR varies, OPR and TIT held fixed. AVE's BPR = 10 marked.</figcaption>
</figure>

This is the clearest illustration of the high-bypass trade: both specific
thrust and TSFC fall steeply as BPR increases from 3 to 10, then keep
falling but more gradually beyond that: consistent with real turbofan
design history, where bypass ratios climbed sharply through the 1970s-2000s
and have grown more slowly since as the easy efficiency gains were
captured.

<figure markdown>
  ![OPR × TIT carpet plot](assets/ave-propulsion-carpet-light.png#only-light)
  ![OPR × TIT carpet plot](assets/ave-propulsion-carpet-dark.png#only-dark)
  <figcaption>The classic carpet plot: TSFC vs. specific thrust across a 2D grid of OPR (15–60) and turbine inlet temperature (1,300–1,900 K), with the GE9X design point starred.</figcaption>
</figure>

The carpet plot is how a propulsion engineer actually reads a design
point: not "is TSFC good," but "where does this OPR/TIT combination sit on
the whole achievable surface." The GE9X point sits near the OPR = 60 edge
of the carpet, at a TIT with visible headroom to the 1,900 K curve, a
choice that buys efficiency from pressure ratio rather than from pushing
turbine materials to their limit.

<figure markdown>
  ![Thrust and TSFC across the flight envelope](assets/ave-propulsion-altitude-light.png#only-light)
  ![Thrust and TSFC across the flight envelope](assets/ave-propulsion-altitude-dark.png#only-dark)
  <figcaption>Per-engine thrust and TSFC as functions of altitude and Mach, anchored to the rated static thrust. The cruise point (M0.84, 11.9 km) is starred on both maps.</figcaption>
</figure>

And this last pair explains a number from earlier in the chapter: why
cruise thrust (242 kN) is roughly half the rated static figure. Follow the
thrust map from the bottom-left corner (sea-level static, ~480 kN) up and
right to the starred cruise point: thrust falls smoothly with both
altitude and Mach the whole way. It's the same cycle model at every point
on these maps, which is also what [Mission & route analysis](mission-and-route.md)'s
segment-by-segment fuel-burn numbers implicitly rely on: the engine has to
be believable across the whole climb and descent, not just at one cruise
condition.
