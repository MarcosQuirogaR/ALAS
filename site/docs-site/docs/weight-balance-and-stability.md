# Weight, balance & stability

Geometry, aerodynamics and structural sizing close here. This chapter checks
whether AVE's mass budget balances, where the CG sits relative to what the wing
and tail can control, and whether that holds across the whole range from empty to
MTOW.

## The mass budget

<figure markdown>
  ![Mass breakdown waterfall](assets/ave-mass-breakdown-dark.png)
  <figcaption>Component masses building up through OEW, MZFW, and MTOW.</figcaption>
</figure>

| Component | Mass |
|---|---|
| Wing | 41.9 t |
| Fuselage | 34.8 t |
| Gear | 17.3 t |
| Propulsion | 30.2 t |
| Systems | 11.3 t |
| Furnishings | 37.3 t |
| Tails (horizontal and vertical) | 5.4 t |
| **OEW** | **178.3 t** |
| Payload | 35.4 t |
| **MZFW** | **213.7 t** |
| Fuel | 145.0 t |
| **MTOW** | **358.67 t** |

Component masses come from the NASA FLOPS transport weight equations (regressions
against real aircraft; see
[Formulas & theory](reference/formulas.md#mass-the-flops-transport-weight-equations)).
The sum closes against the 358.67 t `mtow_kg` requirement because fuel is what
remains once OEW and payload are fixed: 144,969 kg carried, within the 159.0 t
usable tank capacity. The wing mass (41.9 t) is the FLOPS complete-wing figure;
the finite-element wing box has a different scope (primary box against complete
wing group), so the two are not compared as a ratio (see
[Structural analysis](structural-analysis.md#mass-estimates-of-different-scope)).
None of the masses is weighed: this is an evaluated model, not a validated one.

## Where the mass actually sits

<figure markdown>
  ![Mass distribution plan view](assets/ave-mass-distribution-dark.png)
  <figcaption>Every component's mass and location, bubble area scaling with mass. Physical CG and Aero CG both land at x = 37.0 m.</figcaption>
</figure>

Fuel dominates the mass map (145.0 t, 40 % of MTOW) and sits near the CG because it
is carried in the wing, so burning it shifts the CG far less than it would from
the tail or nose. The **physical CG** (mass-weighted, from this component map) and
the **aerodynamic CG** (the VLM reference point) land at the same 37.0 m station
because the aero CG is taken at the physical CG; their agreement checks the
bookkeeping, not the design.

## Static margin and the neutral point

<figure markdown>
  ![Stability metrics](assets/ave-stability-metrics-dark.png)
  <figcaption>Left: CG, neutral point, and CG-envelope limits on a %MAC number line. Right: pitching-moment slope at the cruise design point.</figcaption>
</figure>

<div class="ave-stat-grid" markdown>
<div class="ave-stat"><div class="label">Neutral point</div><div class="value">65.3% MAC</div></div>
<div class="ave-stat"><div class="label">CG (physical & aero)</div><div class="value">22.7% MAC</div></div>
<div class="ave-stat"><div class="label">Static margin</div><div class="value">about 42%</div></div>
<div class="ave-stat"><div class="label">Forward CG limit</div><div class="value">25% MAC</div></div>
<div class="ave-stat"><div class="label">Aft CG limit</div><div class="value">55% MAC</div></div>
<div class="ave-stat"><div class="label">Tail volume coeff. (V_H)</div><div class="value">0.767</div></div>
</div>

Static margin is the distance between CG and neutral point: 65.3 % - 22.7 % =
42.6 % MAC. That passes the 10 % `target_static_margin` and the 5 % hard floor with
room to spare, and the steep negative pitching-moment slope on the right panel
shows the strong nose-down restoring moment. The tail volume coefficient
(V_H = 0.767, tail arm 33.41 m, tail area 22.3 % of wing area) is what provides
that authority.

!!! warning "A 42 % static margin is a model residual, not a design achievement"
    A real long-range transport sits at a small fraction of that. The neutral
    point at 65 % MAC is far aft of a real aircraft's, a known residual of the
    model that follows from the aft-CG and nose geometry of the baseline. It is
    also why the aft CG limit (neutral point minus the 10 % target, 55 % MAC) is so
    far from the CG. Use the static margin to compare designs inside ALAS, not as
    a prediction. The same figure set flags the vertical tail volume coefficient:
    V_v = 0.054 is below the 0.06 to 0.13 target on the
    [control-surfaces figure](structural-analysis.md#control-surfaces).

<figure markdown>
  ![Longitudinal stability side view](assets/ave-stability-side-view-dark.png)
  <figcaption>The same numbers drawn on the actual airframe: CG, wing aerodynamic center, neutral point, and the 33.4 m tail arm that gives the horizontal stabilizer its leverage.</figcaption>
</figure>

The neutral point (65 % MAC) sits behind the wing's own aerodynamic center
(25 % MAC) because of the horizontal tail; without it the two would nearly
coincide and the static margin would collapse. The long green arrow is the tail
arm doing that work.

## The CG envelope, across the whole flight

One static-margin number describes one loading condition; CG has to stay inside
limits as fuel and payload change.

<figure markdown>
  ![CG operational envelope](assets/ave-cg-envelope-dark.png)
  <figcaption>CG position vs. weight, from OEW through MZFW to MTOW, against every relevant physical and regulatory limit.</figcaption>
</figure>

The **payload loading path** (blue) runs from the dry operating weight (DOW,
178,291 kg) through the hold loading to the zero-fuel weight (ZFW, 213,701 kg);
the **fuel loading path** (orange) continues to the takeoff weight (TOW, 358,670 kg)
and burns down to the landing weight (LW, 228,198 kg). The CG gate table gives the
verdict per state against its own limits:

| State | Forward / aft limit (% MAC) | Margin (% MAC) |
|---|---|---|
| DOW | -15.8 / 30.3 | +10.6 |
| ZFW | 13.9 / 30.3 | +5.8 |
| TOW | 21.3 / 30.3 | +1.4 (tightest; rotation limit) |
| LW | 13.9 / 30.3 | +6.3 |

Every gated state passes; the hold-loading step is not gated. This baseline check
runs first: confirming that a design balances at its nominal loading is cheap and
catches a broken mass model or a badly placed wing before an optimizer spends time
on it.

!!! note "Two limit sets, three masses and two CG bookkeepings"
    The numbers on this page come from more than one model level and are not all
    the same quantity.

    - **Two sets of CG limits.** The stability-metrics figure draws the
      aerodynamic envelope for the design CL: a forward limit near 25 % MAC and an
      aft limit at the neutral point less the 10 % target margin (55 % MAC). The
      load-and-trim sheet gates each state against its own phase limits (rotation
      and static-margin floor at takeoff, nose-gear load and tip-back at empty
      weight, landing trim at landing), hence forward bounds of 21.3 % MAC at
      takeoff, 13.9 % at zero fuel and landing, and -15.8 % at operating empty
      weight.
    - **Three landing masses.** The sheet's landing weight (228,198 kg) is the
      zero-fuel weight plus 10 % of the carried fuel (14.5 t reserve loading
      state). The dispatch plan lands the flown mission at 221,062 kg (the 6,362 km
      SimBrief trip burned off the sized takeoff mass; see
      [Mission & route analysis](mission-and-route.md)). The structural design
      landing mass, 271,663 kg (75.7 % of MTOW), is the certified-style landing
      weight the gear and structure are sized for.
    - **Two CG bookkeepings.** The physical CG of 22.7 % MAC (x = 36.96 m) used by
      the stability figure, the static margin and the load sheet comes from the
      lumped component-coordinate model. The design database also holds an
      item-level ledger (every system, seat group and tank placed individually),
      which puts the maximum-fuel takeoff at 25.4 % MAC and the operating-empty
      state at 25.2 % MAC. At the flown takeoff (52.2 t of fuel) the ledger gives
      29.7 % MAC against 21.1 % for the lumped model, an 8.6 % MAC disagreement
      that ALAS reports as a warning (limit 1 % MAC). This page quotes the lumped
      values; a shift of that size would move the static margin by as much, which
      does not change the conclusion that it is far larger than a real
      transport's.

Two of those boundary lines come from physical landing gear, and the gear
itself is sized and placed by the same run:

<figure markdown>
  ![Landing gear planform](assets/ave-landing-gear-dark.png)
  <figcaption>AVE's gear layout: twin nose wheels, body and wing main bogies, 11.67 m track, 31.94 m wheelbase, turnover angle 31°, within limits.</figcaption>
</figure>

The nose-gear steering line exists because a CG too far aft leaves too little
weight on the nose wheels to steer; the tip-over line exists because a CG behind
the main gear would sit the aircraft on its tail. Both follow from the gear
geometry, so the gear is checked with the CG envelope.

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
takeoff thrust and runway friction, after Sadraey, *Aircraft Design: A Systems
Engineering Approach* (2012), sec. 9.6.2 and 12.6. The required pitch
acceleration is 5 deg/s^2, the midpoint of Sadraey's 4-6 deg/s^2 transport
range for a 3-5 s rotation (sec. 12.3); it is a class requirement, not a
measured value for any aircraft, and can be overridden with
`landing_gear.rotation_pitch_acceleration_deg_s2`. Four parts matter:

- **Pitch inertia with the parallel axis.** The inertia about the contact is
  `I_P = I_yy,cg + m[(x_P - x_cg)^2 + h_cg^2]`, with `I_yy,cg` from the mass
  ledger's takeoff state. The balance is then quadratic in the CG and is
  solved exactly.
- **Tail authority from geometry.** The tail lift is
  `CL_h = a_h [i_h - epsilon + tau_e (b_e/b_h) delta_e,max]`: the DATCOM
  lift-curve slope of the built tail, the wing downwash reduced for ground
  effect, the elevator effectiveness of the configured chord multiplied by the
  USAF DATCOM large-deflection factor for plain flaps (Fig. 6.1.1.1-40) and by
  its span fraction, with full up-elevator `delta_e,max` of 25 deg (the class
  value; `landing_gear.elevator_up_travel_deg` replaces it where the flight
  control travel is published, for example the ATR 72). The download is limited
  to the tail section's stall.
- **Takeoff stabiliser trim.** A trimmable horizontal stabiliser is set nose-up
  for the takeoff CG before the roll, so the tail incidence `i_h` at rotation
  is `landing_gear.takeoff_stabilizer_nose_up_deg` whenever that gives more
  download than the built incidence. The registered trimmable-stabiliser
  presets use 4.3 deg nose-up, a lower bound for any forward-limit CG taken
  from Airbus Safety First, "Incorrect pitch trim setting at takeoff": 4.3 deg
  at a 26.3 %MAC takeoff CG, and about 0.5 deg per %MAC more nose-up further
  forward. Unset keeps a fixed stabiliser (the ATR 72-600).
- **Published limits are checks only.** Manufacturer limits (A320, A220,
  A340) are compared against the model result in the tests and are never used
  to tune it. The model reproduces some of them and misses others (the A340
  limit is aft of its published value), so treat the limit as a
  conceptual-design estimate.

### Tail-down, tip-back and main-gear placement

Two ground-attitude checks sit beside the rotation limit, and both depend on
where the main gear is.

- **Tail-down angle.** The *tail-scrape* angle is the smallest angle, over
  every lower-fuselage point aft of the main gear, that the aircraft can
  rotate nose-up before that point touches the ground:
  `min over x > x_mlg of atan((z_bottom(x) - z_ground) / (x - x_mlg))`.
  The optional `geometry.fuselage.belly_upsweep_length_m` lofts a straight
  rising lower line ahead of the tailcone; presets set it so the model
  tail-down angle matches the published pitch to ground contact (A320 11.7 deg,
  A340-300 10.1 deg, B787-9 9.7 deg, ATR 72-600 8 deg). Unset keeps the
  tailcone loft.
- **Tip-back.** The tip-back angle is
  `atan((x_mlg,aft - x_cg,aft) / h_cg)` at the most-aft main-gear axle and the
  most-aft design CG, and the requirement is that it clears both the tail-down
  angle and the configured floor `landing_gear.min_tip_back_deg` (default 0,
  so the tail-down criterion applies alone; Raymer's 15 deg rule of thumb
  remains selectable).
- **Main-gear placement for redesigned candidates.** A registered aircraft
  keeps its published gear stations. A candidate that moves the wing, the
  fuselage or the payload has no published gear, so ALAS translates the
  whole main-gear group (keeping the published nose-gear station and leg
  spacing) to the feasible station nearest the published one: far enough aft
  that the tip-back angle clears the requirement at the most-aft, highest CG,
  while the static nose reaction stays between its steering minimum and its
  handling maximum at every loading state and the nose wheel can still be
  lifted at rotation (Raymer, *Aircraft Design: A Conceptual Approach*, 6th
  ed., sec. 11.2; Currey, *Aircraft Landing Gear Design*, ch. 3). The solved
  translation is saved with the delivered design, and the unchanged envelope
  remains the verdict. If no station inside the installation interval
  satisfies every state, the candidate is infeasible.

These are conceptual-design checks; none of the limits is a certified or
validated boundary.

## Payload-range and fuel volume

<figure markdown>
  ![Payload-range diagram](assets/ave-payload-range-dark.png)
  <figcaption>The classic three-segment payload-range diagram: max-payload plateau, then a straight fuel-limited line down to zero payload.</figcaption>
</figure>

| Point | Range | Payload |
|---|---|---|
| A: max payload, tanks not full | 0 nm | 65.0 t |
| B: max payload, max fuel (harmonic range) | 6,524 nmi | 65.0 t |
| C: max fuel, reduced payload | 9,888 nmi | 21.7 t |
| D: max fuel, zero payload (ferry range) | 10,697 nmi | 0 t |

Point B is the furthest the aircraft flies fully loaded; beyond it every extra
mile trades payload for fuel (the slope from B to C and D). The diagram uses the
structural payload cap (65.0 t, above the 35.4 t load case of the 350-seat cabin)
and a reserve-inclusive fuel plan, with OEW 178,291 kg and takeoff weight limited
to the 358,670 kg MTOW. Takeoff fuel is capped at the usable tank capacity, and a
fuel-volume check confirms the wing can hold the fuel the mass budget assumes.

<figure markdown>
  ![Wing fuel-volume check](assets/ave-fuel-volume-dark.png)
  <figcaption>That check, explicitly: the geometry-estimated usable wing capacity is 159.0 t against the 145.0 t carried at MTOW, a 14.0 t margin.</figcaption>
</figure>

The check passes with a margin of about 10 %. It is the constraint the
optimizer's fuel-capacity check enforces, because thinning the airfoil
(`airfoil_thickness_scale` toward its 0.80 lower bound) for wave-drag gains also
shrinks the tank.
