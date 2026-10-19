# Propulsion analysis

AVE's two GE9X-class turbofans are modelled by an on-design thermodynamic cycle,
station by station, not by a fixed thrust number. That cycle is what the drag in
[Aerodynamic analysis](aerodynamic-analysis.md) is balanced against.

## The on-design cycle

<figure markdown>
  ![GE9X on-design cycle summary](assets/ave-propulsion-cycle-dark.png)
  <figcaption>Station stagnation temperatures through the engine at AVE's M0.84/11.9 km cruise design point.</figcaption>
</figure>

The bars follow the air, and from the combustor onward the gas, through the engine:

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

The Tt3 to Tt4 jump (876 K to 1,670 K) is the combustor. Downstream of Tt4 the
turbine returns that energy as shaft work for the compressor and fan, so Tt5
(877 K) ends up close to Tt3.

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

Overall efficiency (0.334) is thermal efficiency (0.518, fuel energy into useful
work) times propulsive efficiency (0.645, work into thrust power rather than
exhaust kinetic energy). A BPR-10 engine leans on propulsive efficiency: moving
a large mass of air gently beats moving a small mass violently.

## Thrust

<div class="ave-stat-grid" markdown>
<div class="ave-stat"><div class="label">Per-engine, static rated</div><div class="value">467.0 kN</div></div>
<div class="ave-stat"><div class="label">Per-engine, this cruise pt.</div><div class="value">242.4 kN</div></div>
<div class="ave-stat"><div class="label">Total installed (×2)</div><div class="value">484.8 kN</div></div>
</div>

Cruise thrust (242.4 kN per engine) is about half the static rating, because
thrust falls with altitude (lower density) and forward speed.

## TSFC: computed vs. reference

```
TSFC (computed)   =  17.39 mg/(N s)
TSFC (reference)  =  14.16 mg/(N s)
```

ALAS's computed TSFC is about 23 % above the public reference figure for a real
GE9X. This is a known gap: the on-design model uses idealized component
efficiencies and no detailed bleed or extraction losses. It captures trends
(efficiency against OPR and BPR, thrust against altitude) reliably, which is what
the sensitivity studies need, but its absolute TSFC is no substitute for
manufacturer data.

## Sensitivity studies

The cycle is swept across several parameters to place AVE's BPR and OPR in the wider design space.

<figure markdown>
  ![Efficiency vs. OPR](assets/ave-propulsion-efficiency-dark.png)
  <figcaption>Thermal, propulsive, and overall efficiency as OPR varies, BPR and TIT held at AVE's values. The GE9X's actual OPR = 60 is marked.</figcaption>
</figure>

Overall efficiency keeps rising with OPR across the whole range. OPR = 60 is not a
plateau here; it reflects limits this simplified cycle does not model (compressor
stage count, material temperature, cost).

<figure markdown>
  ![Bypass ratio sensitivity](assets/ave-propulsion-bpr-dark.png)
  <figcaption>Specific thrust and TSFC as BPR varies, OPR and TIT held fixed. AVE's BPR = 10 marked.</figcaption>
</figure>

Specific thrust and TSFC both fall steeply as BPR rises from 3 to 10, then more
slowly, as in real turbofan history.

<figure markdown>
  ![OPR × TIT carpet plot](assets/ave-propulsion-carpet-dark.png)
  <figcaption>The classic carpet plot: TSFC vs. specific thrust across a 2D grid of OPR (15–60) and turbine inlet temperature (1,300–1,900 K), with the GE9X design point starred.</figcaption>
</figure>

On the carpet the GE9X point sits near the OPR = 60 edge, with visible headroom to
the 1,900 K curve: efficiency is bought from pressure ratio rather than from
pushing turbine temperature.

<figure markdown>
  ![Thrust and TSFC across the flight envelope](assets/ave-propulsion-altitude-dark.png)
  <figcaption>Per-engine thrust and TSFC as functions of altitude and Mach, anchored to the rated static thrust. The cruise point (M0.84, 11.9 km) is starred on both maps.</figcaption>
</figure>

The thrust map explains the cruise figure: from sea-level static (about 480 kN)
thrust falls smoothly with altitude and Mach to the starred cruise point. The same
cycle model supplies every point of the [mission](mission-and-route.md) climb and
descent.
