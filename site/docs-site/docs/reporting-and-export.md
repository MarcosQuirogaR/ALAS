# Reporting & export

Every run this guide describes wrote its results to disk in the same
handful of formats: this chapter is the map from "a folder full of files"
to "which chapter explains this number."

## What a run writes

```
outputs/
├── design_database.json         # the design-database export
├── airfoils/optimized_root.dat  # winning root-airfoil coordinates
├── payload_layout.json          # seat-by-seat cabin and hold layout
├── cabin_scene_v2.json          # cabin scene for the renderer
├── cpacs/                       # CPACS aircraft, adapter and run manifests
├── solvers/vlm/                 # optimizer search record
├── structures/                  # wing-box mesh and Nastran decks
├── openvsp/, flowunsteady/      # downstream geometry and request files
└── plots/                       # every figure, if --plots was passed
```

Which files appear depends on what ran: the structures folder, for example,
holds the decks of the analyses that were enabled. The desktop application
adds a PDF report and a ZIP of every figure on request (File, Generate report
and Export figures).

### `design_database.json`

The machine-readable record of the design: everything
[Meet AVE](meet-ave.md#the-airframe-as-alas-sees-it),
[Aerodynamic analysis](aerodynamic-analysis.md), and
[Weight, balance & stability](weight-balance-and-stability.md) quote a
number from ultimately traces back to this file. Its top-level groups are
`design_vector` (the sixteen variables), `geometry` (areas, spans, chords,
sweep, the analysis mass basis), `aerodynamics` (design point, trimmed design
point, drag coefficients, Oswald efficiency, static margin), `weights`
(component masses, the FLOPS mass build-up, coordinates, physical CG),
`feasibility` (cruise equilibrium, fuel loading and dispatch, mass balance),
`cpacs` and `metadata`. A shortened excerpt:

```json
{
  "design_vector": { "span_m": ..., "root_chord_m": ..., ... },
  "geometry": { "reference_area_m2": ..., "aspect_ratio": ..., ... },
  "aerodynamics": {
    "design_point": { "alpha_deg": ..., "cl": ..., "cd": ..., "l_over_d": ... },
    "trimmed_design_point": { ... },
    "static_margin": ...
  },
  "weights": {
    "component_masses_kg": { "Wing": ..., "Fuselage": ..., ... },
    "physical_cg_m": [..., ..., ...],
    "mtow_kg": ...
  }
}
```

If you are integrating ALAS output into another tool, this file and the CPACS
export are the stable contracts to build against.

### `optimized_root.dat`

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
databases) opens this directly.

### Mission data

The mission figures in [Mission & route analysis](mission-and-route.md) are
drawn from the flown trajectory held in the run result: time, segment,
altitude, speeds, Mach, range, pitch and angle of attack, lift, drag, thrust,
the four drag components, mass and fuel flow at every step. They appear in the
Results tabs and in the figure archive and PDF report.

### Figures

Every figure is a backend-neutral vector scene (see
[How ALAS works inside](architecture.md)), drawn with the same code whether it
is saved as a static PNG or SVG (`--plots`) or shown live in the desktop
application, in light or dark theme.

## Reusing outputs across runs

Because `design_database.json` describes one design completely, a natural
pattern is feeding one run's winning `design_vector` back in as the initial
design of a follow-up search, or keeping an optimized aircraft as a custom
baseline: [promote it from the sandbox](sandbox.md#leaving-the-sandbox-promote-or-discard)
and the saved configuration reloads it. Nothing in ALAS treats the database as
a one-way export.
