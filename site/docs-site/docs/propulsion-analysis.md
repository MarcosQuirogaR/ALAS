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
  <figcaption>Station stagnation temperatures through the engine at AVE's M0.84/11.9 km cruise design point, throttled to the mission deck's maximum-climb thrust at this point.</figcaption>
</figure>

An engine at cruise is throttled below its take-off rating, so the cycle is
not run at the 1,670 K take-off turbine inlet temperature. ALAS throttles
the on-design cycle along a turbine-compatibility operating line: the
high-pressure turbine nozzle stays choked, so the core flow scales with
the turbine inlet pressure over the square root of its temperature, and
the shaft work balance makes the compressor and fan temperature rises
scale with the turbine inlet temperature. The overall and fan pressure
ratios therefore fall as the engine is throttled. The thrust target is
the engine's catalogue cruise thrust (for the GE9X the catalogue's
generic fallback, 0.2 x rated thrust + 890 N = 94.3 kN at 10,668 m, not a
published figure), carried to the design altitude at constant thrust over
ambient pressure (Torenbeek, *Advanced Aircraft Design*, 2013, p. 67), which
gives 77.9 kN at the 11.9 km design altitude, and capped at the deck's
maximum climb thrust. **For AVE the cap binds**: the cycle is matched to
the deck's maximum climb thrust of 76.1 kN per engine, so the point is
not a part-power cruise point (a cruise thrust at or above maximum climb is
itself an open plausibility question, below). For AVE the
operating point is a turbine inlet temperature of 1,495 K, an overall
pressure ratio of 65.3 and a fan pressure ratio of 1.47. The overall
pressure ratio is *higher* than the 60 of the take-off datum although the
engine is throttled: the compression temperature rise scales with
Tt4/Tt2, and the inlet air is much colder at 11.9 km (Tt2 = 249 K against
288 K) so Tt4/Tt2 is 6.0 at cruise against 5.8 at take-off (a factor of
1.04 on the temperature rise, which gives 65 from 60). Throttling alone
would lower it. This is a consistency check of the cycle against
the deck, not a calibration of either. Reading the bar chart left to right
follows the air (and, from the combustor onward, combustion gas) through
the engine:

| Station | Temperature | What's happening |
|---|---|---|
| T0 (static) | 218 K | Freestream at 11.9 km |
| Tt2 (fan/LPC face) | 249 K | After the inlet's ram recovery |
| Tt13 (fan exit) | 280 K | Bypass-stream fan work |
| Tt25 (LPC exit) | 263 K | Core-stream low-pressure compression |
| Tt3 (HPC exit) | 899 K | After the high-pressure compressor: OPR = 65.3 doing its work |
| Tt4 (TIT) | 1,495 K | Combustor exit at the throttled cruise point (take-off: 1,670 K) |
| Tt45 (HPT exit) | 948 K | After extracting work to drive the HPC |
| Tt5 (LPT exit) | 667 K | After extracting work to drive the fan: this is what leaves through the core nozzle |

The jump from Tt3 to Tt4 (899 K → 1,495 K) is the combustor doing its job;
everything downstream of Tt4 is the turbine giving that energy back as
shaft work to drive the compressor and fan, which is why Tt5 (667 K) ends
up below Tt3: most of the combustor's temperature rise has been extracted
as work by the time the gas reaches the core nozzle.

## Cycle parameters and performance

<div class="ave-stat-grid" markdown>
<div class="ave-stat"><div class="label">Bypass ratio</div><div class="value">10.0</div></div>
<div class="ave-stat"><div class="label">Overall pressure ratio</div><div class="value">65.3</div></div>
<div class="ave-stat"><div class="label">Fan pressure ratio</div><div class="value">1.47</div></div>
<div class="ave-stat"><div class="label">Turbine inlet temp (cruise)</div><div class="value">1,495 K</div></div>
<div class="ave-stat"><div class="label">Thermal efficiency</div><div class="value">0.508</div></div>
<div class="ave-stat"><div class="label">Propulsive efficiency</div><div class="value">0.744</div></div>
<div class="ave-stat"><div class="label">Overall efficiency</div><div class="value">0.378</div></div>
<div class="ave-stat"><div class="label">Fuel-air ratio</div><div class="value">0.021</div></div>
</div>

Overall efficiency (0.378) is the product of thermal efficiency (how much
of the fuel's energy becomes useful work: 0.508) and propulsive
efficiency (how much of that work becomes useful thrust power rather than
wasted kinetic energy in the exhaust: 0.744). Torenbeek quotes about 0.35
overall for present-day turbofans and 0.75 propulsive for a bypass-ratio-5
engine at Mach 0.8 (*Advanced Aircraft Design*, 2013, pp. 63-65). A BPR-10 engine like this
one leans hard on propulsive efficiency: moving a large mass of bypass air
gently is more efficient than moving a small core mass violently, which is
the entire reason high-bypass turbofans replaced low-bypass designs on
long-range transports.

## Thrust

<div class="ave-stat-grid" markdown>
<div class="ave-stat"><div class="label">Per-engine, static rated</div><div class="value">467.0 kN</div></div>
<div class="ave-stat"><div class="label">Per-engine, this cruise pt.</div><div class="value">76.1 kN</div></div>
<div class="ave-stat"><div class="label">Total installed (×2)</div><div class="value">152.2 kN</div></div>
</div>

Cruise thrust (76.1 kN/engine) is about 16 % of the static rated figure:
thrust falls roughly in proportion to the ambient pressure (at 11.9 km
about a fifth of its sea-level value) and the engine is
throttled to the deck's maximum climb thrust at that altitude (the
catalogue cruise thrust, 77.9 kN, is above it; see the note under TSFC).

## TSFC: computed vs. reference

```
TSFC (computed)        =  15.38 mg/(N·s)
TSFC (mission deck)    =  14.59 mg/(N·s)
TSFC (reference)       =  14.16 mg/(N·s)
```

The "reference" is the catalogue's own estimate of 0.5 kg/(kgf h): no
public cruise TSFC for the GE9X was found, and the catalogue labels the
value unsourced and not validated.

At the throttled cruise point the cycle's thrust-specific fuel
consumption is 5.4 % above the mission deck at the same thrust and 8.6 %
above that unsourced catalogue estimate. Run at the take-off
temperature, the cycle gives 17.4 mg/(N·s), 23 % above the reference: that
is an engine at full rating at altitude with a core jet far faster than the
flight speed, which is why the cruise point is throttled.

This residual is **not closed**. The cycle holds component efficiencies
constant and has no bleed, cooling or extraction flows and one fan pressure
ratio for the whole bypass stream, which pushes its TSFC above the engines'
published values (the other registered turbofans show the cycle 7 to 19 %
above their catalogue cruise TSFC on the same operating line, where that figure is a
published datum for only three of the ten engines: the others are family proxies or
unsourced estimates; for the six whose catalogue cruise thrust exceeds the deck's maximum climb thrust, AVE, A380, B787, A220, E195 and C919, that is a comparison at maximum climb, not like-for-like), but no
sourced band exists yet that says how far a cycle of this fidelity may
differ. It is an open task of the v15 programme (a sourced TSFC band for
the cycle, from the engines' published cruise pressure ratios and turbine
temperatures or a cited cycle study); the numbers above are a consistency
check, not an agreement. The cycle also takes its inlet temperature from the
fitted atmosphere used by the cycle code (218 K at 11.9 km) while the deck it is
matched to uses the 1976 standard atmosphere (216.65 K); the two are not yet aligned.

## Sensitivity studies

Beyond the single on-design point, ALAS sweeps the cycle across
several parameters to show how AVE's specific choice of BPR/OPR sits
relative to the wider design space:

<figure markdown>
  ![Efficiency vs. OPR](assets/ave-propulsion-efficiency-light.png#only-light)
  ![Efficiency vs. OPR](assets/ave-propulsion-efficiency-dark.png#only-dark)
  <figcaption>Thermal, propulsive, and overall efficiency as OPR varies, BPR and TIT held at AVE's values. AVE's throttled cruise OPR (65.3) is marked.</figcaption>
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
  <figcaption>The classic carpet plot: TSFC vs. specific thrust across a 2D grid of OPR (15–70) and turbine inlet temperature (1,300–1,900 K), with the GE9X design point starred.</figcaption>
</figure>

The carpet plot is how a propulsion engineer actually reads a design
point: not "is TSFC good," but "where does this OPR/TIT combination sit on
the whole achievable surface." The GE9X cruise point sits in the upper part of the OPR axis (65.3 of 15 to 70)
of the carpet, at a TIT with visible headroom to the 1,900 K curve, a
choice that buys efficiency from pressure ratio rather than from pushing
turbine materials to their limit.

<figure markdown>
  ![Thrust and TSFC across the flight envelope](assets/ave-propulsion-altitude-light.png#only-light)
  ![Thrust and TSFC across the flight envelope](assets/ave-propulsion-altitude-dark.png#only-dark)
  <figcaption>Per-engine thrust and TSFC as functions of altitude and Mach, anchored to the rated static thrust. The cruise point (M0.84, 11.9 km) is starred on both maps.</figcaption>
</figure>

And this last pair shows why cruise thrust is a small fraction of the
rated static figure (76.1 kN against 467 kN per engine). Follow the thrust
map from the bottom-left corner (sea-level static) up and right towards the
star: thrust falls smoothly with both altitude and Mach. The maps are the
cycle scaled to the rated static thrust, so they are not the throttled
76.1 kN operating point of the cycle summary (the star marks the flight
condition, not that thrust), and the mission analysis does not run on this
cycle: it uses the empirical engine deck, whose cruise and climb thrust the
cycle summary is compared with above.
