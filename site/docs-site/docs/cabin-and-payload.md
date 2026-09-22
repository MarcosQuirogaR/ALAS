# Cabin & payload

`DesignRequirements.num_passengers` tells ALAS *how many* seats to
budget mass and fuel for. It doesn't say anything about *where* those
passengers sit, how many galleys and lavatories the cabin needs, or how
checked bags and belly freight get distributed: that's a separate
`CabinConfig`, and it's the difference between "350 passengers" as a
number in a weight budget and 350 passengers as an actual seat map with a
physically real center of gravity.

## The default: one class, auto-provisioned

Leave the `cabin:` block out of your config entirely (as AVE's own
`configs/example_config.yaml` does, commented out) and ALAS falls
back to a single economy cabin sized to `requirements.num_passengers`:
350 economy seats at 0.79 m pitch, 0.46 m width, with galley and lavatory
counts auto-derived from standard provisioning ratios (roughly one galley
per 100 passengers, one lavatory per 45). This is what every figure in
this guide that doesn't explicitly mention a class mix is showing.

## A multi-class layout

The same YAML file ships a commented-out example worth walking through,
because it's the more realistic case for a long-range twin:

```yaml
cabin:
  passenger:
    first:    {count: 8}
    business: {count: 42}
    premium:  {count: 36}
    economy:  {count: 264}
```

A class with `count: 0` (the default) is simply absent: set only the
classes you want and the rest drop out of the layout entirely. Each class
carries its own pitch, width, and per-passenger mass, calibrated to
reflect real cabin standards rather than one number for everybody:

| Class | Pitch | Width | Mass/pax |
|---|---|---|---|
| First | 1.93 m | 0.95 m | 96 kg |
| Business | 1.27 m | 0.70 m | 90 kg |
| Premium | 0.97 m | 0.52 m | 86 kg |
| Economy | 0.79 m | 0.46 m | 84 kg |

`abreast` (seats per row) defaults to 0 ("auto from cabin width") so you
only need to override it for an unusual configuration; ALAS derives a
sensible seats-per-row count from the fuselage cross-section and class
width on its own.

## What the layout engine actually produces

<figure markdown>
  ![Cabin and payload layout across decks](assets/ave-cabin-payload-light.png#only-light)
  ![Cabin and payload layout across decks](assets/ave-cabin-payload-dark.png#only-dark)
  <figcaption>Main-deck seat map, lower-deck hold plan, and a side profile carrying the payload centre of gravity.</figcaption>
</figure>

This is a single-class layout on AVE's fuselage, and the header line
records what the engine decided: **349 seats, ten abreast, two aisles at
51 cm, four Type A exit pairs, five galleys and eight lavatories, 5.6 t of
bags in seven containers, payload centre of gravity at 20.7 % MAC**.

Several things in that figure are the engine reasoning rather than
drawing:

- **Seats stop where the fuselage tapers.** The cabin narrows toward the
  tail and the seat blocks narrow with it, because the layout is placed
  inside the true fuselage cross-section rather than a rectangle.
- **Exits are placed to a rule, not to taste.** Type A pairs are
  distributed along the cabin at the spacing evacuation certification
  requires for the seat count, which is what makes the count itself
  self-limiting.
- **The lower deck respects the wingbox.** The centre section is blocked
  out, and containers are placed fore and aft of it. This is the single
  biggest constraint on belly capacity and the reason hold volume is not
  simply "fuselage length times cross-section".
- **The count came out at 349, not the 350 requested.** The layout engine
  places whole rows into real geometry; it does not manufacture a seat to
  hit a round number. That one-seat shortfall is the model being honest.

## Why this matters beyond a nicer chart

The detailed cabin layout isn't cosmetic. It feeds the **baseline-first
weight & balance pass** described in
[Weight, balance & stability](weight-balance-and-stability.md): ALAS
places every seat by class, routes checked bags to the lower-deck holds,
and (in passenger mode) fills remaining lower-deck belly volume with
revenue freight up to `max_structural_payload_kg` (0 = disabled for AVE).
The result is a **physically meaningful payload CG**, not a single point
mass parked at the fuselage midpoint. That's what makes the CG envelope
chart in that chapter trustworthy: the loading path it plots is built from
an actual seat-by-seat, bag-by-bag distribution, exit-compliant per
CS-25/FAR-25.807, not a lumped stand-in.

There's one deliberate exception: the **optimizer** itself uses a fast,
lumped payload model during its search: rebuilding a full seat map on
every one of hundreds of candidate evaluations would make optimization
far too slow for no accuracy benefit at that stage. The detailed layout
engine runs only on the **baseline pass** and the **final analysis** of
whichever design comes out the other end, which is exactly where you want
the expensive, physically detailed calculation to happen.

## Cargo mode

Setting `aircraft_type: cargo` switches the whole model: `num_passengers`
stops mattering and `cargo_payload_kg` becomes the target instead, with a
ULD (unit load device) solver packing pallets and containers across the
main deck and lower-deck holds:

```yaml
cabin:
  cargo:
    use_main_deck: true
    main_deck_uld: PMC
    lower_deck_uld: LD3
    loading_strategy: target_cg   # target_cg | min_pallets | door_proximity | uniform
    target_cg_pct_mac: 25.0
```

`loading_strategy` controls *how* the solver decides where pallets go:
chase a specific CG target, use the fewest pallets possible, load nearest
the doors first, or spread the load uniformly. AVE itself is configured as
a passenger aircraft in this guide, but the same baseline-first W&B logic
applies either way: swap `aircraft_type` and everything downstream, from
the CG envelope to the payload-range diagram, follows.
