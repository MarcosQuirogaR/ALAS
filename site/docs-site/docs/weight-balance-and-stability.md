# Weight, balance & stability

Everything upstream (geometry, aerodynamics, structural sizing) closes
here. This is where ALAS checks whether AVE's mass budget actually
balances, where its center of gravity sits relative to what the wing and
tail can control, and whether that stays true not just at one loading
condition but across the full range from empty to MTOW.

## The mass budget

<figure markdown>
  ![Mass breakdown waterfall](assets/ave-mass-breakdown-light.png#only-light)
  ![Mass breakdown waterfall](assets/ave-mass-breakdown-dark.png#only-dark)
  <figcaption>Component masses building up through OEW, MZFW, and MTOW.</figcaption>
</figure>

| Component | Mass |
|---|---|
| Wing | 41.9 t |
| Fuselage | 32.3 t |
| Gear | 14.3 t |
| Propulsion | 20.6 t |
| Systems | 39.5 t |
| Furnishings | 35.9 t |
| **OEW** | **188.4 t** |
| Payload | 35.0 t |
| **MZFW** | **223.4 t** |
| Fuel | 135.3 t |
| **MTOW** | **358.67 t** |

The component masses in this table were recorded with the Torenbeek
statistical build-up that earlier versions of ALAS used. The production mass
model is now the FLOPS-based transport model (`PureFlopsTransportV1`), so a
current run gives different component values. In either model the sum closes
exactly against the 358.67 t `mtow_kg` requirement, because fuel is the
remainder of that closure: the table is a ledger of one solved budget, not an
independent build-up. Wing mass (41.9 t in the recorded run) is shown next to
the FEM's own structural estimate in
[Structural analysis](structural-analysis.md#mass-two-estimates-with-different-scopes).
The two inventories have different model scopes, so their difference is
diagnostic only; it is not an accuracy check of either number.

## Where the mass actually sits

<figure markdown>
  ![Mass distribution plan view](assets/ave-mass-distribution-light.png#only-light)
  ![Mass distribution plan view](assets/ave-mass-distribution-dark.png#only-dark)
  <figcaption>Every component's mass and location, bubble area scaling with mass. Physical CG and Aero CG both land at x = 36.3 m.</figcaption>
</figure>

Fuel dominates the mass map (135.3 t, 38% of MTOW) and sits centered near
the CG almost by necessity: it's carried in the wing, which is exactly
where you want the largest, most range-dependent mass to live. Burning
fuel over the course of a flight shifts CG far less than it would if that
same mass were carried in the tail or nose. The **physical CG** (the real,
mass-weighted center of gravity from this component map) and the
**aerodynamic CG** (the reference point the VLM analysis uses) land at the
same 36.3 m station, confirmation that `wing_x_shift_m` (currently 0.0
for AVE's baseline) has the wing positioned correctly for these two
numbers to agree, rather than silently compensating for a mismatch
elsewhere.

## Static margin and the neutral point

<figure markdown>
  ![Stability metrics](assets/ave-stability-metrics-light.png#only-light)
  ![Stability metrics](assets/ave-stability-metrics-dark.png#only-dark)
  <figcaption>Left: CG, neutral point, and the configured design-target CG range (neutral point minus the requested static margin, and the requested CG range) on a %MAC number line; these target markers are not the model's CG-envelope limits, which the feasibility assessment reports. Right: pitching-moment slope at the cruise design point.</figcaption>
</figure>

<div class="ave-stat-grid" markdown>
<div class="ave-stat"><div class="label">Neutral point</div><div class="value">51.8% MAC</div></div>
<div class="ave-stat"><div class="label">CG (physical & aero)</div><div class="value">20.3% MAC</div></div>
<div class="ave-stat"><div class="label">Static margin</div><div class="value">31.5%</div></div>
<div class="ave-stat"><div class="label">Forward CG target</div><div class="value">12% MAC</div></div>
<div class="ave-stat"><div class="label">Aft CG target (NP − target margin)</div><div class="value">42% MAC</div></div>
<div class="ave-stat"><div class="label">Tail volume coeff. (V_H)</div><div class="value">0.746</div></div>
</div>

Static margin is simply the distance between CG and neutral point. Here,
51.8% â 20.3% = 31.5% of MAC. That's a large, comfortable margin (AVE's
`target_static_margin` requirement is only 10%, and the hard floor is 5%),
and it shows up directly on the right-hand panel: the pitching-moment
slope (dCm/dCL = â0.517) is steep and clearly negative, meaning a
disturbance that increases lift produces a strong nose-down restoring
moment. The tail volume coefficient (V_H = 0.746, tail arm 33.75 m, tail
area 21.3% of wing area) is what's producing that authority: it's the
classic sizing parameter for "does the tail have enough leverage to
control the CG range this aircraft needs."

<figure markdown>
  ![Longitudinal stability side view](assets/ave-stability-side-view-light.png#only-light)
  ![Longitudinal stability side view](assets/ave-stability-side-view-dark.png#only-dark)
  <figcaption>The same numbers drawn on the actual airframe: CG, wing aerodynamic center, neutral point, and the 33.7 m tail arm that gives the horizontal stabilizer its leverage.</figcaption>
</figure>

This side view is worth a pause because it makes the *mechanism* visible
rather than just the metric: the neutral point (52% MAC) sits behind the
wing's own aerodynamic center (25% MAC) precisely *because* of the
horizontal tail. Without it, NP and wing AC would nearly coincide and the
31.5% static margin would collapse to almost nothing. The long green arrow
is the tail arm doing that work.

## The CG envelope, across the whole flight

A single static-margin number describes one loading condition. Real
aircraft load and unload fuel and payload constantly, and CG has to stay
inside limits throughout.

<figure markdown>
  ![CG operational envelope](assets/ave-cg-envelope-light.png#only-light)
  ![CG operational envelope](assets/ave-cg-envelope-dark.png#only-dark)
  <figcaption>CG position vs. weight, from OEW through MZFW to MTOW, against every relevant physical and regulatory limit.</figcaption>
</figure>

Reading this chart: the **payload loading path** (blue) runs from OEW up
to MZFW as passengers and bags board; the **fuel loading path** (orange)
continues from MZFW up to MTOW as the tanks fill. Both paths stay
comfortably inside the green operational envelope, clear of every boundary
condition plotted alongside it: the forward aerodynamic limit, the
stability limit (neutral point minus the target margin), the nose-gear
steering minimum-load line, and the main-gear tip-over line. This is the
**baseline-first** check ALAS runs before anything else: confirming a
design balances correctly at its nominal loading is a fast, cheap sanity
check that catches a broken mass model or a badly placed wing long before
you'd want to spend an optimizer's time on it.

Two of those boundary lines come from physical landing gear, and the gear
itself is sized and placed by the same run:

<figure markdown>
  ![Landing gear planform](assets/ave-landing-gear-light.png#only-light)
  ![Landing gear planform](assets/ave-landing-gear-dark.png#only-dark)
  <figcaption>AVE's gear layout: twin nose wheels, four main-gear struts (wing + body) of four wheels each, 11.67 m track, 31.46 m wheelbase, turnover angle 53Â°, within limits.</figcaption>
</figure>

The nose-gear steering line on the envelope exists because a CG too far
aft leaves too little weight on the nose wheels to steer; the tip-over
line exists because a CG behind the main gear would sit the aircraft on
its tail. Both derive directly from this geometry, which is why the gear
layout is checked *with* the CG envelope rather than as an afterthought.

## Load & trim sheet

The **LOAD & TRIM SHEET** is the airline-style form of the CG envelope:
airplane gross weight against an index (a linear measure of moment) with
the CG in %MAC. A side panel carries three boxes. The *limit definitions*
key names the ground, takeoff, flight and landing limit sets. The
*loading points* table is the worked case: each step lists its added mass and
index (`dW`, `dI`) and the running weight, index and %MAC. The *CG gate* table
gives the run's verdict for each loading state (limit, forward and aft
bounds, margin). A state the gate fails is ringed, and a loading step the gate
does not evaluate is marked `not gated`. It is available in the desktop
application and in the exported report.

Each loading state is gated only by the mechanisms of its phase: rotation and
the static-margin floor at takeoff, landing trim and ground mechanisms at
landing, ground mechanisms (nose-gear load, tip-back) at OEW.

### Rotation: the forward CG limit

The forward limit at takeoff is the nose-wheel lift-off balance about the
main-gear contact at the rotation speed (1.10 of the stall speed) with
takeoff thrust and runway friction. The required pitch acceleration is
7 deg/s^2, the midpoint of the 6-8 deg/s^2 range in the conceptual-design
literature. Three parts matter:

- **Pitch inertia with the parallel axis.** The inertia about the contact is
  `I_P = I_yy,cg + m[(x_P - x_cg)^2 + h_cg^2]`, with `I_yy,cg` from the mass
  ledger's takeoff state. The balance is then quadratic in the CG and is
  solved exactly.
- **Tail authority from geometry.** The tail lift at full up-elevator is
  `CL_h = a_h [i_h - epsilon + tau_e (b_e/b_h) delta_e,max]`: the DATCOM
  lift-curve slope of the built tail and its incidence, the wing downwash
  reduced for ground effect, the elevator effectiveness of the configured
  chord multiplied by the USAF DATCOM large-deflection factor for plain flaps
  (0.60 at 25 deg) and by its span fraction, with `delta_e,max` = -25 deg. The
  result is limited to the tail section's stall.
- **Published limits are checks only.** Manufacturer limits (A320, A220,
  A340) are compared against the model result in the tests and are never used
  to tune it. The model reproduces some of them and misses others (A340 is
  aft of its published value), so treat the limit as a conceptual-design
  estimate.

## Payload-range and fuel volume

<figure markdown>
  ![Payload-range diagram](assets/ave-payload-range-light.png#only-light)
  ![Payload-range diagram](assets/ave-payload-range-dark.png#only-dark)
  <figcaption>The classic three-segment payload-range diagram: max-payload plateau, then a straight fuel-limited line down to zero payload.</figcaption>
</figure>

| Point | Range | Payload |
|---|---|---|
| A: max payload, tanks not full | 0 nm | 35.0 t |
| B: max payload, max fuel (harmonic range) | 8,177 nm | 35.0 t |
| C/D: max fuel, zero payload (ferry range) | 11,119 nm | 0 t |

Point B is the aircraft's most efficient real-world operating point: the
furthest you can fly *fully loaded*. Beyond it, every additional mile has
to come from trading payload for fuel, which is exactly the downward slope
from B to C/D. Fuel capacity here (170,239 kg) is itself capped by the
MTOW budget rather than physical tank volume (takeoff fuel is capped at
the usable tank capacity); a separate fuel-volume
check confirms the wing can physically hold the fuel the mass budget
assumes it carries, which matters because a wing sized purely for
aerodynamics and structure could, in principle, be too thin to hold its
own design fuel load.

<figure markdown>
  ![Wing fuel-volume check](assets/ave-fuel-volume-light.png#only-light)
  ![Wing fuel-volume check](assets/ave-fuel-volume-dark.png#only-dark)
  <figcaption>That check, explicitly: AVE's wingbox holds 228 mÂ³ (183.2 t of fuel) against the 135.3 t the mission budget requires: a +47.9 t margin.</figcaption>
</figure>

For AVE the check passes with plenty of room, but this is exactly the
constraint the optimizer's fuel-capacity check enforces during a
search, because a candidate that thins the airfoil
(`airfoil_thickness_scale` toward its 0.80 lower bound) to chase wave-drag
gains is quietly shrinking this tank at the same time.
