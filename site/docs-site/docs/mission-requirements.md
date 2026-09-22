# Mission requirements

Everything ALAS does starts from one object: `DesignRequirements`. It's
the only place you state what you *want*: the optimizer and every analysis
stage downstream read from it, but nothing in the solver hardcodes a target
value. Change a field here and you're designing a different aircraft, not
patching a script.

This chapter walks through every field AVE sets, in the order they appear
on the Inputs tab (or in `configs/example_config.yaml`'s `requirements:`
block), and what each one actually constrains.

## Cruise design point

```yaml
cruise_mach: 0.84
cruise_altitude_m: 11887.2
```

These two numbers fix the point in the flight envelope everything else is
sized around. `cruise_mach` combined with `cruise_altitude_m` (via the
standard atmosphere) gives a true airspeed and dynamic pressure, from
which ALAS derives the **required cruise CL** for the current
candidate's weight and wing area:

$$
C_L = \frac{W}{q \, S}
$$

Every VLM sweep, every drag polar, every optimizer evaluation in this guide
is implicitly anchored to this one flight condition. Push `cruise_altitude_m`
up and required CL drops (lower density, higher TAS for the same Mach); push
`cruise_mach` up and wave drag starts fighting you through the Korn
equation (see [Aerodynamic analysis](aerodynamic-analysis.md)).

## Weight target

```yaml
mtow_kg: 358670.0
```

The single number the entire weight-and-balance pipeline is anchored to.
Every component mass in [Weight, balance & stability](weight-balance-and-stability.md)
is sized so that OEW + payload + fuel reconciles back to this figure; it's
not a soft target, it's the budget the mass model has to close against.

## Payload

```yaml
aircraft_type: passenger
num_passengers: 350
```

`aircraft_type` switches between two completely different payload models:
`passenger` (seat count → mass via `passenger_mass_kg`, default 100 kg/pax
per the FAA AC 120-27E standard) or `cargo` (a direct `cargo_payload_kg`
target). AVE is a passenger design, so `num_passengers` is what
[Cabin & payload](cabin-and-payload.md)'s seat-placement engine actually
fills the cabin against.

```yaml
max_structural_payload_kg: 0.0   # 0 = disabled for AVE
```

Left at zero for AVE: this field exists to cap how much extra revenue
freight the lower-deck belly-fill logic can add on top of passengers and
bags, for aircraft where you want a "max structural payload" scenario
distinct from a "max passengers" one.

## Sizing constraints

```yaml
max_wing_area_m2: 535.0
min_wing_loading_kg_m2: 485.0
max_cruise_cl: 0.95
```

These are the boundaries of *feasible* airframes, not tuning knobs the
optimizer aims for: a candidate design that violates any of them is
penalized or rejected outright. `max_wing_area_m2` keeps the search from
drifting toward an oversized wing chasing marginal induced-drag gains;
`min_wing_loading_kg_m2` keeps it from going the other way and undersizing
the wing relative to MTOW; `max_cruise_cl` is a stall guard: any candidate
whose required cruise CL would exceed this is rejected as too close to the
stall boundary to be a credible cruise point. When
[Optimization results](optimization-results.md) shows the optimizer
growing AVE's wing area from 529 m² to 567 m², it's growing inside this
535 m² ceiling being *raised* implicitly: the requirements file used for
that run relaxes it; the point of showing you both runs is to see the
constraint doing its job in the baseline and getting out of the way when
you widen it.

## Stability targets

```yaml
target_static_margin: 0.10
```

Static margin (how far the center of gravity sits ahead of the neutral
point, as a fraction of MAC) is the classic longitudinal-stability
metric. This field sets the *target* used to place the aft CG limit
(`Aft CG Limit = Neutral Point − target_static_margin × 100%MAC`) in the
CG envelope built in
[Weight, balance & stability](weight-balance-and-stability.md); it isn't
the static margin the final design ends up with (AVE's baseline actually
lands at 0.315 aerodynamically, 0.05 minimum-physical as a hard floor,
comfortably clear of this target in both directions).

## What's *not* here

A handful of things you might expect on this list live elsewhere by
design:

- **Design-space bounds** (span, sweep, chord limits, …): those describe
  *how the optimizer is allowed to search*, not what you want; see
  [Design space & optimizer](design-space-and-optimizer.md).
- **Cabin class mix, ULD strategy**: `DesignRequirements` only sets the
  passenger *count*; the actual seat map is a separate `CabinConfig`, see
  [Cabin & payload](cabin-and-payload.md).
- **Mission route, climb schedule**: a separate `MissionConfig`, see
  [Mission & route analysis](mission-and-route.md).

This split is deliberate: `DesignRequirements` is the one form a new user
fills in to describe *what aircraft they want*; everything else is either
a scaffold decision (design space, geometry family) or an analysis-fidelity
knob (mission profile, sweep resolution) that has a sensible default and
doesn't need touching to get a first result.
