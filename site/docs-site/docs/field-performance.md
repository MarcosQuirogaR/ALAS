# Low-speed and field performance

Cruise is where an airliner spends its time, but it is not what decides
whether the aircraft can operate. That is settled at low speed, at the two
ends of the flight: whether it can get off the runway it departs from, and
whether it can stop on the runway it arrives at.

This chapter covers the two results that answer those questions: the
matching chart, which sizes the aircraft, and the take-off and landing
analysis, which checks it against real airports.

## The matching chart

Before any geometry exists, classical aircraft design reduces the problem
to two numbers: **wing loading** (weight per unit wing area) and
**thrust-to-weight ratio**. Nearly every performance requirement can be
expressed as a constraint on that plane, and the region satisfying all of
them at once is the design space you are allowed to choose from.

<figure markdown>
  ![Matching chart](assets/ave-matching-chart-light.png#only-light)
  ![Matching chart](assets/ave-matching-chart-dark.png#only-dark)
  <figcaption>Every sizing constraint drawn on the wing-loading / thrust-to-weight plane, with the feasible region shaded and the design point marked.</figcaption>
</figure>

Each line is a different requirement refusing to be violated:

- **Cruise thrust floor**: the thrust needed to balance drag at the cruise
  design point. It falls with wing loading, then turns back up: a small
  wing means high lift coefficients and high induced drag, a large one
  means excess wetted area and parasite drag. The minimum between those is
  the aerodynamically natural wing size.
- **Engine-out climb gradient**: with one engine failed the aircraft must
  still climb at a certified minimum gradient. Here that fixes a floor of
  T/W ≥ 0.211, independent of wing loading, and it is often the binding
  constraint on thrust for a twin.
- **Take-off distance**, per departure airport: rising with wing loading,
  because a more heavily loaded wing needs more speed before it will fly,
  and more runway to reach it.
- **Landing distance**, per arrival airport: a vertical limit on wing
  loading. Approach speed follows from wing loading and maximum lift
  coefficient, and landing distance follows from approach speed.

The **feasible design space** is everything above the climb and cruise
lines and left of the landing limit. AVE's design point sits at 682 kg/m²
and T/W = 0.265, inside the region with margin on every side rather than
pressed against a boundary, which is what you want at a preliminary stage,
because every one of those constraints will move as the design matures.

Both airports appear separately on the chart. If you fly between two
airports with very different runway lengths or elevations, the more
demanding one silently governs the design, and this is where you see which.

## Take-off and landing, per airport

The matching chart works in normalised ratios. The next step checks actual
metres of runway at the specific airports on the route.

<figure markdown>
  ![Take-off and landing at the departure airport](assets/ave-lto-departure-light.png#only-light)
  ![Take-off and landing at the departure airport](assets/ave-lto-departure-dark.png#only-dark)
  <figcaption>Departure field analysis: the runway drawn to scale with decision speeds marked, and required distances against what is available.</figcaption>
</figure>

The upper panel draws the runway itself, with the certification distances
laid over it and the decision speeds marked where they occur:

| Quantity | Meaning |
|---|---|
| **V₁** | Decision speed: past this, the take-off must be continued |
| **V_R** | Rotation speed: the nose is raised |
| **V₂** | Take-off safety speed: the engine-out climb speed to be achieved by screen height |
| **TODR** | Take-off distance required, all engines |
| **ASD** | Accelerate–stop distance: accelerate to V₁, lose an engine, and stop |
| **BFL** | Balanced field length: where TODR and ASD are equal |
| **LDR** | Landing distance required |

At London Heathrow, with 3,902 m available, AVE needs 2,889 m all-engines
and a 3,409 m balanced field. The **balanced field length is the number
that matters**: it is the runway required for the take-off to remain safe
whichever side of V₁ an engine fails on. Its 493 m of margin here is real
but not generous, and this is at sea level, in standard conditions.

<figure markdown>
  ![Take-off and landing at the arrival airport](assets/ave-lto-arrival-light.png#only-light)
  ![Take-off and landing at the arrival airport](assets/ave-lto-arrival-dark.png#only-dark)
  <figcaption>The same analysis at the arrival airport, where landing distance is the operative figure.</figcaption>
</figure>

Landing is much less demanding than take-off for this aircraft: unsurprising,
since it arrives having burned most of its fuel, and a lighter aircraft
approaches more slowly and stops sooner.

!!! note "Airport conditions are inputs, not constants"
    Field length is computed at each airport's own elevation and
    temperature, because both matter and both are unforgiving. Thinner air
    at altitude reduces engine thrust and increases true airspeed for the
    same lift, and a hot day does the same thing. An aircraft comfortable
    at sea level on a standard day can be runway-limited at a
    high-elevation airport in summer, and the only way to find out is to
    compute it there.

## Where these numbers come from

The high-lift coefficients, thrust lapse, engine-out climb gradient and
landing constant behind all of the above are on the **Performance** page in
Advanced Settings, with matched presets for different aircraft classes.
See the [user guide](user-guide.md#performance). The formulas are in
[Formulas & theory](reference/formulas.md#the-v-n-diagram), alongside the
V-speed definitions shared with the
[V-n diagram](structural-analysis.md#the-v-n-diagram).
