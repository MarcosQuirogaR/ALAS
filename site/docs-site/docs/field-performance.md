# Low-speed and field performance

Whether an airliner can operate is settled at low speed: can it leave the runway
it departs from, and stop on the one it arrives at. This chapter covers the
matching chart, which sizes the aircraft, and the take-off and landing analysis,
which checks it against real airports.

## The matching chart

Classical design reduces the problem to two numbers: **wing loading** and
**thrust-to-weight ratio**. Most performance requirements are constraints on that
plane, and the region satisfying all of them is the design space.

<figure markdown>
  ![Matching chart](assets/ave-matching-chart-dark.png)
  <figcaption>Every sizing constraint drawn on the wing-loading / thrust-to-weight plane, with the feasible region shaded and the design point marked.</figcaption>
</figure>

Each line is a different requirement refusing to be violated:

- **Cruise thrust floor**: the thrust that balances drag at the cruise design
  point. It falls with wing loading, then rises again: a small wing means high CL
  and induced drag, a large one means excess wetted area.
- **Engine-out climb gradient**: a certified minimum gradient with one engine
  failed puts a floor under T/W for a twin. It is a performance constant
  (`oei_gradient`) and not a line on this chart, where the cruise floor is the
  binding thrust constraint.
- **Take-off distance**, per departure airport: rises with wing loading, since a
  more heavily loaded wing needs more speed and runway.
- **Landing distance**, per arrival airport: a vertical limit on wing loading,
  because approach speed follows from wing loading and maximum lift coefficient.

The feasible region lies above the cruise floor and the take-off lines and left of
the landing limit. AVE's design point, at about 675 kg/m² and T/W = 0.265, sits
inside it with margin on every side, which suits a preliminary stage because every
constraint will move as the design matures. Both airports appear on the chart; the
more demanding one governs the design.

## Take-off and landing, per airport

The next step checks actual metres of runway at the airports on the route.

<figure markdown>
  ![Take-off and landing at the departure airport](assets/ave-lto-departure-dark.png)
  <figcaption>Departure field analysis: the runway drawn to scale with decision speeds marked, and required distances against what is available.</figcaption>
</figure>

The upper panel draws the runway with the certification distances and decision speeds:

| Quantity | Meaning |
|---|---|
| **V₁** | Decision speed: past this, the take-off must be continued |
| **V_R** | Rotation speed: the nose is raised |
| **V₂** | Take-off safety speed: the engine-out climb speed to be achieved by screen height |
| **TODR** | Take-off distance required, all engines |
| **ASD** | Accelerate–stop distance: accelerate to V₁, lose an engine, and stop |
| **BFL** | Balanced field length: where TODR and ASD are equal |
| **LDR** | Landing distance required |

At London Heathrow, with 3,902 m available, AVE needs 2,857 m all-engines (V₁ 158 kt,
V_R 166 kt, V₂ 168 kt). The accelerate-stop and balanced-field proxies land at the
same 2,857 m, and the landing field length is 1,456 m. The **balanced field length
is the number that matters**: the runway needed for the take-off to stay safe
whichever side of V₁ an engine fails on. The margin is 1,045 m, at 25 m elevation
and ISA+0.

<figure markdown>
  ![Take-off and landing at the arrival airport](assets/ave-lto-arrival-dark.png)
  <figcaption>The same analysis at the arrival airport, where landing distance is the operative figure. Dubai (OMDB, 19 m elevation, ISA+15): 4,000 m available, landing field length 1,531 m. The take-off bars of this panel show 3,004 m.</figcaption>
</figure>

Landing is much less demanding than take-off here, because the aircraft arrives
with most of its fuel burned and approaches slower.

!!! note "Airport conditions are inputs"
    Field length is computed at each airport's own elevation and temperature.
    Thin air and heat both cut thrust and raise true airspeed for the same lift,
    so an aircraft comfortable at sea level can be runway-limited at a
    high-elevation airport in summer.

## Where these numbers come from

The high-lift coefficients, thrust lapse, engine-out climb gradient and
landing constant behind all of the above are on the **Performance** tab of
Advanced Settings, with matched presets for different aircraft classes.
See the [user guide](user-guide.md#advanced-settings). The formulas are in
[Formulas & theory](reference/formulas.md#the-v-n-diagram), alongside the
V-speed definitions shared with the
[V-n diagram](structural-analysis.md#the-v-n-diagram).
