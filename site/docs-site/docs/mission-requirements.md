# Mission requirements

Everything starts from one object, `DesignRequirements`: the place where you
state what you want. The optimizer and every analysis stage read from it; no
solver hardcodes a target. This chapter walks through the fields the AVE preset
sets, in the order of the Inputs page (or the `requirements:` block of a saved
configuration such as the packaged `configs/ave.yaml`).

## Cruise design point

```yaml
cruise_mach: 0.84
cruise_altitude_m: 11887.2
```

Mach and altitude (through the standard atmosphere) give true airspeed and
dynamic pressure, from which ALAS derives the **required cruise CL** for the
candidate's weight and wing area:

$$
C_L = \frac{W}{q \, S}
$$

Every polar and optimizer evaluation is anchored to this condition. Higher
altitude lowers the required CL for the same Mach; higher Mach raises wave drag
through the Korn equation (see [Aerodynamic analysis](aerodynamic-analysis.md)).

## Weight target

```yaml
mtow_kg: 358670.0
```

The budget the mass model closes against: OEW + payload + fuel must reconcile
to this figure (see [Weight, balance & stability](weight-balance-and-stability.md)).
For registered presets it is **Hard MTOW**, the default
[takeoff-mass mode](design-space-and-optimizer.md#takeoff-mass-modes), and the
takeoff fuel is capped at the usable tank capacity. A brief with no preset is
a [clean-sheet design](design-space-and-optimizer.md#clean-sheet-design), which
derives its start, bounds and geometry from the stated requirements.

## Payload

```yaml
aircraft_type: passenger
num_passengers: 350
```

`aircraft_type` selects the payload model: `passenger` (seat count times
`passenger_mass_kg`, default 100 kg per passenger including baggage) or `cargo`
(a direct `cargo_payload_kg`). AVE is a passenger design; `num_passengers` is
the count the [Cabin & payload](cabin-and-payload.md) seat placement fills.

```yaml
max_structural_payload_kg: 65000.0
```

The most the airframe may carry (MZFW minus OEW). The AVE preset sets a notional
65,000 kg, the maximum payload of a 777-300ER. It defines the maximum-payload
point of the payload-range diagram and caps the lower-deck belly freight added on
top of passengers and bags. 0 disables it.

## Sizing constraints

```yaml
max_wing_area_m2: 535.0
min_wing_loading_kg_m2: 485.0
max_cruise_cl: 0.95
```

These bound the feasible airframes; a candidate that violates one is invalid.
`max_wing_area_m2` stops the search drifting toward an oversized wing,
`min_wing_loading_kg_m2` stops it undersizing the wing for the MTOW, and
`max_cruise_cl` rejects candidates whose required cruise CL is too close to
stall. In [Optimization results](optimization-results.md) the optimizer moves AVE's
wing area from 525.2 m² to 531.4 m²: it grows, but stays inside this
535 m² ceiling, which is the constraint doing its job.

## Stability targets

```yaml
target_static_margin: 0.10
```

Static margin is the distance of the CG ahead of the neutral point, as a
fraction of MAC. This field sets the aft CG limit
(`Aft CG Limit = Neutral Point - target_static_margin x 100 %MAC`) in the CG
envelope of [Weight, balance & stability](weight-balance-and-stability.md). It is
not the margin the final design ends up with (the optimized AVE lands at
about 0.43 aerodynamically, against this 0.10 target and a 0.05 minimum-physical
hard floor; see the note on this large value in
[Weight, balance & stability](weight-balance-and-stability.md#static-margin-and-the-neutral-point)).

## What is not here

- **Design-space bounds** (span, sweep, chords): how the optimizer may search;
  see [Design space & optimizer](design-space-and-optimizer.md).
- **Cabin class mix, container strategy**: `DesignRequirements` sets only the
  passenger count; the seat map is a separate `CabinConfig`
  ([Cabin & payload](cabin-and-payload.md)).
- **Mission route and climb schedule**: a separate `MissionConfig`
  ([Mission & route analysis](mission-and-route.md)).
