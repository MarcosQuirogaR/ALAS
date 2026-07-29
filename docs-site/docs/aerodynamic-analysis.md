# Aerodynamic analysis

Every optimizer evaluation runs a cheap version of this; once a design is
final, ALAS re-runs it properly. This chapter is that final,
full-fidelity aerodynamic analysis of AVE's baseline geometry
(`python main.py --no-optimize --plots`), the same run behind
[Weight, balance & stability](weight-balance-and-stability.md) and
[Structural analysis](structural-analysis.md).

## The method: VLM plus two corrections

ALAS's aerodynamics is a **hybrid** model, not a single tool doing
everything:

1. **Vortex-lattice method (VLM)**, via AeroSandbox: solves the
   inviscid, lifting-surface problem for the actual 3D geometry (wing,
   tail, fuselage interference) at each angle of attack, giving lift and
   *induced* drag directly from the flow solution.
2. **Raymer component build-up** for parasite (viscous, skin-friction)
   drag: computed per component (wing, fuselage, nacelles) from wetted
   area and a turbulent flat-plate skin-friction coefficient, corrected
   for compressibility and form factor. VLM doesn't see viscosity at all,
   so this term has to come from empirical methods.
3. **Korn equation** for transonic wave drag: the penalty for operating
   near the drag-divergence Mach number, a function of thickness ratio,
   sweep, and CL that VLM (inviscid, subsonic-linearized) can't predict on
   its own.

Total drag is the sum of all three, and it's this combination, not raw
VLM output, that produces every drag polar in this guide.

## AVE's cruise design point

<figure markdown>
  ![Four-panel aerodynamic sweep](assets/ave-aero-panel-light.png#only-light)
  ![Four-panel aerodynamic sweep](assets/ave-aero-panel-dark.png#only-dark)
  <figcaption>Lift curve, drag polar (induced-only vs. total corrected), L/D efficiency, and longitudinal stability: the standard sweep run at every final analysis.</figcaption>
</figure>

At AVE's M0.84/11,887 m cruise point:

<div class="ave-stat-grid" markdown>
<div class="ave-stat"><div class="label">Cruise α</div><div class="value">1.02°</div></div>
<div class="ave-stat"><div class="label">Cruise C_L</div><div class="value">0.656</div></div>
<div class="ave-stat"><div class="label">Cruise C_D</div><div class="value">0.0368</div></div>
<div class="ave-stat"><div class="label">L/D</div><div class="value">17.82</div></div>
<div class="ave-stat"><div class="label">C_D0</div><div class="value">0.0205</div></div>
<div class="ave-stat"><div class="label">Oswald efficiency</div><div class="value">0.859</div></div>
</div>

The **efficiency panel** (bottom-left) makes the design point's placement
visible: L/D peaks around CL ≈ 0.7, and AVE's design CL (0.656, the dotted
line) sits almost exactly there: the geometry isn't just *capable* of
flying near its best-efficiency point at the specified cruise condition,
it's actually doing so, which is the aerodynamic sizing working correctly
rather than coincidentally.

The **drag polar** (top-right) separates induced-only drag (dashed,
VLM-only) from the total corrected polar (solid): the horizontal gap
between them at any given CL *is* the Raymer parasite + Korn wave drag
contribution. It's roughly constant in CD terms across the CL range shown,
which is expected: parasite drag depends mostly on Reynolds number and
wetted area (both nearly fixed here), not on CL.

## Trim changes the picture slightly

The sweep above is *untrimmed*: AoA swept with the tail at a fixed
incidence. ALAS separately solves for the **trimmed** condition (the
horizontal-tail incidence that zeroes pitching moment at the cruise CL):

| | Untrimmed | Trimmed |
|---|---|---|
| α | 1.02° | 1.05° |
| C_L | 0.656 | 0.684 |
| C_D | 0.0368 | 0.0383 |
| L/D | 17.82 | 17.87 |
| Tail incidence | – | −2.00° |

Trimming costs a small amount of L/D here (the tail is generating a small
download to balance the wing's pitching moment, which both adds trim drag
and slightly changes the CL needed for the same lift): the trimmed L/D is
the number that actually matters for range, and it's what
[Mission & route analysis](mission-and-route.md)'s fuel-burn figures are
built from.

## Longitudinal stability

The bottom-right panel of the sweep plots C_m vs. α: its slope is the
static-margin story in miniature. A negative slope (as shown) means
increasing angle of attack produces a nose-down restoring moment: the
aircraft is statically stable. Where that line crosses C_m = 0 is the
trimmed angle of attack; how steep it is (relative to the lift-curve
slope) sets the static margin discussed fully in
[Weight, balance & stability](weight-balance-and-stability.md).

## Where the drag actually goes

<figure markdown>
  ![Drag breakdown at the cruise design point](assets/ave-drag-breakdown-light.png#only-light)
  ![Drag breakdown at the cruise design point](assets/ave-drag-breakdown-dark.png#only-dark)
  <figcaption>Left: the design-point drag split into parasite, induced and wave contributions. Right: the efficiency curve with the design point marked.</figcaption>
</figure>

This is the same total drag the polar reports, taken apart into the three
physical mechanisms described above. It is the most directly actionable
chart in the chapter, because each slice points at a different design
lever: parasite drag is bought down with wetted area and surface finish,
induced drag with span and lift distribution, wave drag with thickness and
sweep. Reading the split tells you which lever is worth pulling, and the
answer changes completely between a regional turboprop and a transonic
widebody.

## Span loading

<figure markdown>
  ![Span loading distribution](assets/ave-span-loading-light.png#only-light)
  ![Span loading distribution](assets/ave-span-loading-dark.png#only-dark)
  <figcaption>Spanwise lift distribution from the VLM solve: the same load distribution the wingbox FEM in the next chapter integrates into bending moment.</figcaption>
</figure>

This isn't a separate calculation; it's the same VLM solution the drag
polar comes from, just read along the span instead of integrated into a
single CL. It matters here because it's the direct link to
[Structural analysis](structural-analysis.md): this distribution, times
the design load factor, *is* the applied load the wingbox has to carry.

## VLM flow visualization

<figure markdown>
  ![VLM streamlines around AVE](assets/ave-vlm-flow-light.png#only-light)
  ![VLM streamlines around AVE](assets/ave-vlm-flow-dark.png#only-dark)
  <figcaption>Streamlines from the vortex-lattice solution at the cruise design point.</figcaption>
</figure>

## Airfoil-level check: NeuralFoil sweep

<figure markdown>
  ![NeuralFoil Reynolds sweep](assets/ave-airfoil-reynolds-light.png#only-light)
  ![NeuralFoil Reynolds sweep](assets/ave-airfoil-reynolds-dark.png#only-dark)
  <figcaption>2D airfoil lift/drag behavior across a Reynolds-number sweep, independent of the 3D VLM solve: a sanity check that the section itself behaves reasonably before it's swept into a wing.</figcaption>
</figure>

This runs the wing's root section through
[NeuralFoil](https://github.com/peterdsharpe/NeuralFoil) (a neural-network
surrogate for panel-method airfoil analysis) across a range of Reynolds
numbers. It's a 2D check, deliberately independent of the 3D VLM/Raymer/Korn
stack above: if the section itself looks wrong here (an unrealistic
lift-curve slope, a drag bucket in the wrong place), that's a geometry
problem to fix before trusting anything the 3D analysis says about it.
