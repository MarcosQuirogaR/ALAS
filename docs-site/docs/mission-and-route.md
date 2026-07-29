# Mission & route analysis

Every other chapter in this guide analyzes AVE sitting still: one flight
condition, one design point. This chapter is the one place the aircraft
actually *flies*: a full climb/cruise/descent simulation over a real route,
run through [SUAVE](https://suave.stanford.edu) in its own isolated
environment and bridged back into ALAS as ordinary figures and
numbers.

## Why a subprocess, not an import

SUAVE 2.5.2 needs an old numpy/scipy/scikit-learn stack that directly
conflicts with the versions ALAS's own optimizer and aerodynamics
depend on. Rather than force one Python environment to satisfy both,
ALAS shells out: `alas/integration/suave_bridge.py` calls the
isolated `.suave-venv` interpreter as a subprocess, running a small runner
script that builds a SUAVE vehicle and mission from a plain JSON request
and writes results back as CSV. The main process never imports SUAVE
directly: it's a clean process boundary, not a shared-environment hack.

## The route

ALAS's default route is **London Heathrow (EGLL) → Dubai (OMDB)**:
`departure_airport`/`arrival_airport` in the top-level app settings. This
run used the great-circle routing tier (2,968 nm / 5,497 km) rather than
the airway-graph tier, since that requires the optional navdata download
described in [Installation](installation.md#optional-route-data).

<figure markdown>
  ![Mission route, colored by mass](assets/ave-mission-route-light.png#only-light)
  ![Mission route, colored by mass](assets/ave-mission-route-dark.png#only-dark)
  <figcaption>EGLL → OMDB, colored by total aircraft mass along the route. The annotation marks the cruise midpoint.</figcaption>
</figure>

The color gradient is the fuel-burn story made visual: orange near
departure (heavy, full fuel load), fading through green toward blue as the
aircraft burns down toward its arrival mass. This is the same 2D
equirectangular map that replaced an earlier 3D textured-globe view, a
deliberate simplification that keeps the figure legible without needing an
offscreen 3D render for every run.

## Fuel burn and block time

<div class="ave-stat-grid" markdown>
<div class="ave-stat"><div class="label">Initial mass</div><div class="value">358.67 t</div></div>
<div class="ave-stat"><div class="label">Final mass</div><div class="value">285.26 t</div></div>
<div class="ave-stat"><div class="label">Fuel burned</div><div class="value">73.41 t</div></div>
<div class="ave-stat"><div class="label">Block time</div><div class="value">7.34 h</div></div>
<div class="ave-stat"><div class="label">Segments</div><div class="value">12</div></div>
</div>

73.4 t burned against a 135.3 t max fuel load (from
[Weight, balance & stability](weight-balance-and-stability.md)) means this
particular EGLL–OMDB mission uses about 54% of AVE's fuel capacity: well
inside the payload-range diagram's max-payload plateau, consistent with
Dubai being nowhere near AVE's ~8,177 nm harmonic range.

## The full profile

<figure markdown>
  ![Mission profile: altitude, mass, airspeed, SFC](assets/ave-mission-profile-light.png#only-light)
  ![Mission profile: altitude, mass, airspeed, SFC](assets/ave-mission-profile-dark.png#only-dark)
  <figcaption>Twelve segments, top to bottom: altitude, total mass, true airspeed, and specific fuel consumption, all against elapsed time.</figcaption>
</figure>

This is the climb schedule described in
[Installation](installation.md) and
[Design space & optimizer](design-space-and-optimizer.md)'s sibling
chapter made concrete: takeoff, an initial climb to roughly 31,000 ft,
a two-step climb ladder up to final cruise altitude (visible as the two
distinct steps in the altitude trace around minute 115 and minute 250),
a long cruise, then a four-step descent ladder back down. The **mass**
trace is almost perfectly linear through cruise (steady fuel flow at a
steady cruise thrust setting), and the **SFC** trace tells the same story
from the engine's side: it climbs during the low-airspeed initial segments
(the engine working relatively harder per unit thrust at low speed). Every
segment boundary here is a `MissionProfileConfig` field
([Installation](installation.md)), editable, not hardcoded, if you want a
different climb schedule for a different aircraft.

## Aerodynamics and drag, in flight

<figure markdown>
  ![Aerodynamic coefficients through the mission](assets/ave-mission-aero-coefficients-light.png#only-light)
  ![Aerodynamic coefficients through the mission](assets/ave-mission-aero-coefficients-dark.png#only-dark)
  <figcaption>CL, CD, and L/D as they actually vary segment to segment, not the single design-point values from Aerodynamic analysis.</figcaption>
</figure>

<figure markdown>
  ![Drag component breakdown through the mission](assets/ave-mission-drag-components-light.png#only-light)
  ![Drag component breakdown through the mission](assets/ave-mission-drag-components-dark.png#only-dark)
  <figcaption>Induced, parasite, and wave drag, tracked across the same mission.</figcaption>
</figure>

[Aerodynamic analysis](aerodynamic-analysis.md) gives you AVE's
performance at *one* condition: the cruise design point. These two charts
are the same aerodynamic model evaluated continuously across an actual
flight, which is a meaningfully different (and more honest) picture: CL
drifts upward through cruise as the aircraft gets lighter and needs less
lift at the same speed and altitude, and the drag-component split shifts
accordingly. If you only ever look at the single design-point numbers,
it's easy to forget that "cruise CL" is really "cruise CL at this one
instant." This chapter is the reminder.

## Speeds, range and the forces behind them

Three more views come out of the same solve, each answering a question the
profile above leaves open.

<figure markdown>
  ![True and equivalent airspeed, and Mach, against time](assets/ave-mission-velocities-light.png#only-light)
  ![True and equivalent airspeed, and Mach, against time](assets/ave-mission-velocities-dark.png#only-dark)
  <figcaption>TAS and EAS overlaid, with Mach below. The gap between the two speeds is the compressibility signature a single TAS trace hides.</figcaption>
</figure>

True airspeed and *equivalent* airspeed diverge as the aircraft climbs:
EAS is the speed the airframe structurally "feels" (it carries the dynamic
pressure), while TAS is the speed it actually covers ground at. In cruise
the aircraft is doing roughly 480 kt TAS but only around 250 kt EAS, which
is precisely why the V-n envelope in
[Structural analysis](structural-analysis.md#the-v-n-diagram) is plotted
against equivalent airspeed and not true.

<figure markdown>
  ![Cumulative range and pitch angle against time](assets/ave-mission-flight-path-light.png#only-light)
  ![Cumulative range and pitch angle against time](assets/ave-mission-flight-path-dark.png#only-dark)
  <figcaption>Ground covered and body pitch attitude through the flight.</figcaption>
</figure>

<figure markdown>
  ![Throttle, lift, thrust and drag against time](assets/ave-mission-aero-forces-light.png#only-light)
  ![Throttle, lift, thrust and drag against time](assets/ave-mission-aero-forces-dark.png#only-dark)
  <figcaption>The force balance being solved at every timestep: throttle setting, lift, thrust and drag.</figcaption>
</figure>

The force panel is the most direct evidence that this is a simulation and
not a Breguet-range shortcut. Lift tracks weight downward as fuel burns;
thrust and drag stay matched through cruise because the segment is solved
to equilibrium, not assumed; and throttle drifts as the engine is asked for
progressively less thrust to hold the same speed at a falling weight.

## What this run needs that others don't

Every other analysis chapter in this guide runs from a bare
`--no-optimize` pass. This one additionally needs the isolated SUAVE environment
(`scripts/setup_suave_env.ps1`, one-time, ~2 minutes) and, for a route more
detailed than a great-circle line, the optional navdata download. Skip
either and `DesignPipeline.run()` degrades gracefully: the mission stage
reports itself unavailable and every other stage runs normally, which is
why this is the one chapter in this guide with its own explicit
prerequisite rather than assuming a bare install gets you here.
