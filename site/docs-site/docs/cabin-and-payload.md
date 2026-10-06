# Cabin & payload

`DesignRequirements.num_passengers` says how many seats to budget mass and fuel
for. The `CabinConfig` says where they sit, how many galleys and lavatories the
cabin has, and how bags and belly freight are distributed. It turns "350
passengers" from a number in a weight budget into a seat map with a real payload
CG.

## Cabin presets and the seat target

The cabin is chosen by the **cabin preset** on the Inputs page. For a
passenger aircraft the options are three seat-geometry sets and `Custom`:

| Preset (as shown) | Seat mix |
|---|---|
| High-density single-class | All economy at a tight pitch |
| Two-class (Business/Economy) | A business section forward, economy behind |
| Three-class (First/Business/Economy) | First, business and economy |
| Custom | You set the class shares |

A freighter offers `Max payload`, `Dense payload` and `Custom` instead. The
preset supplies the seat geometry (pitch, width and mass per occupant of each
class); only `Custom` exposes the class shares, as percentages of the
passengers. The layout converts the normalised mix into whole rows that fit the
usable, regulation-compliant cabin, so the shares are targets and the seat
count that results is a property of the geometry.

`requirements.num_passengers` is the brief's seat target. For a registered
preset it is the declared load case; for a clean-sheet brief it sizes the cabin
(see [Clean-sheet design](design-space-and-optimizer.md#clean-sheet-design)).
`requirements.min_passenger_capacity` is an optional hard floor on the
geometry-resolved capacity (0 disables it). Otherwise capacity is dynamic: the
payload the optimizer closes the mass budget against is recomputed from each
candidate's cabin, whatever the shares and the candidate's fuselage produce.

Galley and lavatory counts and the aisle width are derived from standard
provisioning ratios and the fuselage cross-section unless you set them, and the
exits are the smallest arrangement whose per-exit seat allowance satisfies
CS-25.807(g), placed at the declared door stations of the aircraft where it has
them.

The class geometry that `Custom` starts from is:

| Class | Pitch | Width | Mass/pax |
|---|---|---|---|
| First | 1.93 m | 0.95 m | 96 kg |
| Business | 1.55 m | 0.70 m | 90 kg |
| Economy | 0.79 m | 0.46 m | 84 kg |

The load-case mass per passenger is `requirements.passenger_mass_kg` (default
100 kg including baggage), of which `cabin.passenger.checked_bag_mass_kg` (16 kg)
is the baggage share.

## What the layout engine actually produces

<figure markdown>
  ![Cabin and payload layout across decks](assets/ave-cabin-payload-dark.png)
  <figcaption>Main-deck seat map, lower-deck hold plan, and a side profile carrying the payload centre of gravity.</figcaption>
</figure>

This is AVE's two-class cabin (36 business, 314 economy), and the header line
records what the engine decided: **350 seats, ten abreast, two aisles at
51 cm, four Type A exit pairs, five galleys and eight lavatories, 5.6 t of
bags in five containers, payload centre of gravity at 19.6 % MAC**.

What the engine decided, as opposed to drew:

- **Seats follow the fuselage taper.** Seat blocks narrow toward the tail
  because rows are placed inside the true cross-section.
- **Exits follow a rule.** Type A pairs are spaced as evacuation certification
  requires for the seat count.
- **The lower deck respects the wingbox.** The centre section is blocked out and
  containers go fore and aft of it, which is the main limit on belly capacity.
- **Exits can bind before the floor does.** The geometry could place 440 seats,
  but AVE keeps its source exit layout (four Type A pairs) and the declared
  350-seat load case. The engine places whole rows and never adds a seat to hit a
  round number.

<figure markdown>
  ![Cabin cross-section](assets/ave-cabin-section-dark.png)
  <figcaption>Cabin cross-section at x = 36.0 m inside the 6.20 × 6.20 m outer fuselage section: seats, aisle, overhead bins and lower-deck containers.</figcaption>
</figure>

## Where the layout is used

The detailed layout feeds the baseline and final weight and balance in
[Weight, balance & stability](weight-balance-and-stability.md). ALAS places every
seat by class, routes checked bags to the lower-deck holds and, in passenger
mode, fills remaining belly volume with freight up to `max_structural_payload_kg`
(AVE: 65,000 kg; 0 disables). The result is a payload CG built from a seat-by-seat
and bag-by-bag distribution, not a point mass at mid-fuselage.

The optimizer is the exception. It uses a lumped payload model, because
rebuilding a seat map for every candidate would make the search far slower for
no accuracy benefit at that stage. The detailed layout runs on the baseline pass
and on the final analysis of the winning design.

## Cargo mode

With `aircraft_type: cargo`, `num_passengers` is ignored and `cargo_payload_kg`
becomes the target. A unit-load-device solver packs pallets and containers across
the main deck and lower-deck holds:

```yaml
cabin:
  cargo:
    use_main_deck: true
    main_deck_uld: PMC
    lower_deck_uld: LD3
    loading_strategy: target_cg   # target_cg | min_pallets | door_proximity | uniform
    target_cg_pct_mac: 25.0
```

`loading_strategy` chases a CG target, uses the fewest pallets, loads nearest the
doors first, or spreads the load uniformly. Everything downstream, from the CG
envelope to the payload-range diagram, follows the payload type.
