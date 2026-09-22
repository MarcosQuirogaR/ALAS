# Reporting & export

Every run this guide describes wrote its results to disk in the same
handful of formats: this chapter is the map from "a folder full of files"
to "which chapter explains this number."

## What a run writes

```
outputs/
├── design_data.json         # the full design-database export
├── optimized_airfoil.dat    # winning root-airfoil coordinates
├── flight_data.csv          # mission simulation, if enabled
└── *.png                    # every figure, if --plots was passed
```

### `design_data.json`

The machine-readable handoff: everything
[Meet AVE](meet-ave.md#the-airframe-as-alas-sees-it),
[Aerodynamic analysis](aerodynamic-analysis.md), and
[Weight, balance & stability](weight-balance-and-stability.md) quote a
number from ultimately traces back to this file:

```json
{
  "design_vector": { "span_m": 71.75, "root_chord_m": 16.5, ... },
  "geometry": { "wing_area_m2": 529.04, "aspect_ratio": 9.89, ... },
  "aerodynamics": {
    "design_point": { "alpha_deg": 1.02, "cl": 0.656, "cd": 0.037, "l_over_d": 17.82 },
    "trimmed_design_point": { ... },
    "static_margin": 0.315
  },
  "weights": {
    "component_masses_kg": { "Wing": 41913.6, "Fuselage": 32301.3, ... },
    "physical_cg_m": [36.27, -0.002, -1.62],
    "mtow_kg": 358670.0
  }
}
```

This is also what SUAVE's mission bridge
([Mission & route analysis](mission-and-route.md)) reads from to build a
vehicle request: it was written from the start "as a machine-readable
handoff," in the original design docs' own words, not repurposed after the
fact. If you're integrating ALAS's output into another tool, this
file is the stable contract to build against.

### `optimized_airfoil.dat`

The winning root-airfoil section, as plain Selig-format coordinates:
`x y` pairs running from the trailing edge along the upper surface, around
the leading edge, and back along the lower surface:

```
ALAS_Optimized
1.000000 -0.009500
0.993289 -0.007345
0.986577 -0.005227
...
```

Any tool that reads Selig-format `.dat` files (XFOIL, XFLR5, most airfoil
databases) opens this directly: it's the same format ALAS's own
`AirfoilLibrary` indexes 1,600+ reference sections from.

### `flight_data.csv`

One row per simulated timestep across the whole mission, every column
[Mission & route analysis](mission-and-route.md)'s figures are drawn from:

```
Time_s, Segment, Altitude_m, TAS_m_s, EAS_m_s, Mach, Density_kg_m3,
Range_m, Pitch_deg, AoA_deg, CL, CD, L_over_D, Throttle, Lift_N, Drag_N,
Thrust_N, CD_parasite, CD_induced, CD_compressible, CD_miscellaneous,
CD_total, Mass_kg, MassFlowRate_kg_s, SFC_kg_kgf_hr
```

Notably, this is the *full* breadth SUAVE computes (drag broken into its
four physical components at every timestep, not just altitude/speed/mass)
because ALAS's export was built to match everything the original
validation script's own plotting code used, not a trimmed-down subset.

### Figures

Every `figure_*` function behind this guide's charts lives in
`alas/reporting/visualization.py`, and every one takes an optional
`theme` argument: figures render consistently whether they're saved as a
static PNG (`--plots`) or drawn live in the desktop app, in either light
or dark mode.

## Reusing outputs across runs

Because `design_data.json` is a complete, self-contained description of
one design, a natural pattern is feeding one run's output back in as a
starting point for another, e.g., taking an optimized design and running
[Mission & route analysis](mission-and-route.md) against a different route
without re-running the optimizer, or loading a winning `design_vector`
back into the GUI's Design Space tab as a new "initial value" for a
follow-up search. Nothing in ALAS treats a design database as a
one-way export; it's plain JSON, meant to be read back in.
