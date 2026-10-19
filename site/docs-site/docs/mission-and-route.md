# Mission & route analysis

This is the chapter where AVE flies: a full climb, cruise and descent simulation
over a real route, run natively by `alas-mission` and reported as trajectory
figures and tables. It is native Rust, adapted in part from SUAVE (LGPL-2.1)
segment methods, and runs as an ordinary concurrent pipeline stage with no
external interpreter or setup step.

The segment solver turns the aircraft geometry, engine deck and operating profile
into take-off, climb, stepped-cruise, descent and reserve segments.

## The route

The default city pair is **London Heathrow (EGLL) to Dubai (OMDB)**
(`departure_airport`, `arrival_airport`). Routing is tried in order of fidelity:
a SimBrief or KML dispatch plan, then the airway graph (needs the optional
navdata download, see [Installation](installation.md#navigation-route-data)),
then a great-circle track.

This run used a **SimBrief plan, EGLLOMDB, AIRAC cycle 2610**, fetched through
the SimBrief API: 89 waypoints, 6,362 km (3,435 nmi). The great-circle distance
is 5,497 km (2,968 nmi), so the plan is 15.7 % longer.

<figure markdown>
  ![Mission route, colored by mass](assets/ave-mission-route-dark.png)
  <figcaption>The SimBrief EGLL to OMDB plan (89 waypoints, 6,362 km) coloured by aircraft mass, 265.9 t at departure to 221.3 t at arrival.</figcaption>
</figure>

<figure markdown>
  ![Mission route on a 3D globe](assets/ave-mission-route-3d-dark.png)
  <figcaption>The same route on the 3D globe.</figcaption>
</figure>

## Fuel burn and block time

<div class="ave-stat-grid" markdown>
<div class="ave-stat"><div class="label">Initial mass</div><div class="value">265.9 t</div></div>
<div class="ave-stat"><div class="label">Final mass</div><div class="value">221.3 t</div></div>
<div class="ave-stat"><div class="label">Fuel burned (native mission)</div><div class="value">44.5 t</div></div>
<div class="ave-stat"><div class="label">Block time</div><div class="value">7.49 h</div></div>
<div class="ave-stat"><div class="label">Route length</div><div class="value">6,362 km</div></div>
</div>

Several fuel quantities appear, and they are different things:

| Quantity | Value | Meaning |
|---|---|---|
| Native mission burn | 44,541 kg, 7.49 h | The flown trajectory in the figures below |
| Dispatch-plan trip fuel | 44,803 kg, 7.42 h | The plan flies the same route as a trip and sets takeoff and landing mass (0.6 % apart from the native flight) |
| Plan block fuel | 45,319 kg | `plan.block_fuel_kg` of the dispatch plan |
| Plan takeoff fuel | 52,164 kg | Also carries 5 % contingency (2,240 kg), 370 km alternate (3,353 kg) and 30 min final reserve (1,578 kg) |
| Design mission, great circle (5,497 km) | trip 38,770 kg at 259.5 t takeoff mass | The mission the optimizer closes the design on |

!!! note "Why the optimizer's 39.3 t differs from the 44.5 t flown here"
    The optimizer scores block fuel on its design mission, a great-circle track of
    5,497 km (2,968 nmi). Its trip fuel for the final design is 38.8 t, the same
    order as the optimizer's 39.3 t objective. The final analysis instead flies the
    6,362 km SimBrief plan, 15.7 % longer, and burns 14.9 % more fuel. So the two
    numbers are for different routes, not an inconsistency. Run-to-run optimizer
    variation and the exact objective definition (for example reserve and taxi
    treatment) were not separated out.

The 44.5 t burn is 28 % of the 159.0 t geometry-estimated usable fuel capacity
([Weight, balance & stability](weight-balance-and-stability.md)), well inside the
maximum-payload range of the payload-range diagram (6,524 nmi at 65 t).

!!! note "The mission is flown at the sized mass, not at MTOW"
    The flight starts at 265.9 t, the takeoff mass the dispatch closes at (payload
    plus the fuel this route needs), not at the 358.67 t MTOW. MTOW is a ceiling
    and the mass at which the payload-range corners are computed.

## The full profile

<figure markdown>
  ![Mission profile: altitude, mass, airspeed, SFC](assets/ave-mission-profile-dark.png)
  <figcaption>Altitude (FL370, then FL390), mass, true airspeed and specific fuel consumption against elapsed time over the 7.49 h block.</figcaption>
</figure>

The profile is take-off, an initial climb to FL370, a step climb to FL390, a long
cruise and a descent. Mass falls almost linearly through cruise (steady fuel flow
at steady thrust). SFC rises in the low-speed initial segments, where the engine
works harder per unit thrust. Segment boundaries are `MissionProfileConfig`
fields (Advanced Settings, Mission Analysis), not hardcoded.

## Aerodynamics and drag, in flight

<figure markdown>
  ![Aerodynamic coefficients through the mission](assets/ave-mission-aero-coefficients-dark.png)
  <figcaption>CL, CD, and L/D as they actually vary segment to segment, not the single design-point values from Aerodynamic analysis.</figcaption>
</figure>

<figure markdown>
  ![Drag component breakdown through the mission](assets/ave-mission-drag-components-dark.png)
  <figcaption>Induced, parasite, and wave drag, tracked across the same mission.</figcaption>
</figure>

The [aerodynamic analysis](aerodynamic-analysis.md) is one condition. These charts
evaluate the same model continuously along the flight: CL falls within each cruise
segment as fuel burns and jumps up at the step climb, where the dynamic pressure
is lower, and the drag-component split shifts with it. "Cruise CL" is a value at
one instant.

## Speeds, range and the forces behind them

Three more views come from the same solve.

<figure markdown>
  ![True and equivalent airspeed, and Mach, against time](assets/ave-mission-velocities-dark.png)
  <figcaption>TAS and EAS overlaid, with Mach below. The gap between the two speeds is the compressibility signature a single TAS trace hides.</figcaption>
</figure>

Equivalent airspeed (EAS) is the speed the airframe feels through dynamic
pressure; true airspeed (TAS) is the speed over the ground. In cruise the aircraft
flies roughly 480 kt TAS at about 250 kt EAS, which is why the V-n envelope in
[Structural analysis](structural-analysis.md#the-v-n-diagram) uses EAS.

<figure markdown>
  ![Cumulative range and pitch angle against time](assets/ave-mission-flight-path-dark.png)
  <figcaption>Ground covered and body pitch attitude through the flight.</figcaption>
</figure>

<figure markdown>
  ![Throttle, lift, thrust and drag against time](assets/ave-mission-aero-forces-dark.png)
  <figcaption>The force balance being solved at every timestep: throttle setting, lift, thrust and drag.</figcaption>
</figure>

The force panel shows this is a simulation and not a Breguet shortcut: lift tracks
weight down as fuel burns, thrust and drag stay matched through cruise because
each segment is solved to equilibrium, and throttle drifts down to hold speed at a
falling weight.

## Data this stage needs

Mission analysis needs no installation. A route better than a great-circle line
needs either a SimBrief username (External Tools) or the optional airway
navigation data (External Tools window, with consent, or `ALAS --download-navdata`).
Without them the stage flies the great-circle track and every other stage is
unaffected.
