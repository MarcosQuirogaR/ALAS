# Aerodynamic analysis

Every optimizer evaluation runs a cheap version of this; the final design is
re-run at full fidelity. This chapter is that analysis for the optimized AVE
(registered preset, seed 42), the same run behind
[Weight, balance & stability](weight-balance-and-stability.md) and
[Structural analysis](structural-analysis.md).

## The method: VLM plus two corrections

The model is hybrid:

1. **Vortex-lattice method (VLM)**, a native Rust solver based on the
   AeroSandbox method. It solves the inviscid lifting-surface problem for the 3D
   geometry (wing, tail, fuselage interference) at each angle of attack and gives
   lift and *induced* drag.
2. **Raymer component build-up** for parasite drag: per component (wing,
   fuselage, nacelles) from wetted area and a turbulent flat-plate skin-friction
   coefficient, with compressibility and form-factor corrections. VLM has no
   viscosity, so this term is empirical.
3. **Korn equation** for transonic wave drag, a function of thickness ratio,
   sweep and CL that inviscid subsonic-linearized VLM cannot predict.

Total drag is the sum of the three; every drag polar in this guide is that sum.

## AVE's cruise design point

<figure markdown>
  ![Four-panel aerodynamic sweep](assets/ave-aero-panel-dark.png)
  <figcaption>Lift curve, drag polar (induced-only vs. total corrected), L/D efficiency, and longitudinal stability: the standard sweep run at every final analysis.</figcaption>
</figure>

At the M0.84, 11,887 m cruise point, the sweep point closest to the cruise lift
coefficient (the sweep steps about 1 deg in angle of attack, so it lands on
CL 0.524 rather than exactly 0.543):

<div class="ave-stat-grid" markdown>
<div class="ave-stat"><div class="label">Cruise α</div><div class="value">−0.40°</div></div>
<div class="ave-stat"><div class="label">Cruise C_L</div><div class="value">0.524</div></div>
<div class="ave-stat"><div class="label">Cruise C_D</div><div class="value">0.0275</div></div>
<div class="ave-stat"><div class="label">L/D</div><div class="value">19.09</div></div>
<div class="ave-stat"><div class="label">C_D0 (parasite)</div><div class="value">0.0152</div></div>
<div class="ave-stat"><div class="label">Oswald efficiency (polar fit)</div><div class="value">0.598</div></div>
</div>

Two "zero-lift" numbers exist. C_D0 = 0.0152 is the parasite slice of the
design-point drag in the breakdown below. The quadratic polar fit also absorbs
CL-dependent viscous drag, so its intercept is 0.0211 and its Oswald efficiency
0.598; the induced slice alone implies e of about 0.83.

The **efficiency panel** (bottom-left) shows L/D peaking near 19.3 at CL of
about 0.6. The design CL (0.524 at the nearest sweep point, dotted line; 0.543
at the exact mid-cruise mass) sits a little below the peak. The design CL is set
by the mid-cruise weight, wing area and cruise condition; the optimizer does not
choose it directly.

The **drag polar** (top-right) separates induced-only drag (dashed, VLM) from
the total corrected polar (solid). The gap between them is the Raymer parasite
plus Korn wave drag, roughly constant in CD across the CL range because parasite
drag depends mainly on Reynolds number and wetted area.

## Trim changes the picture slightly

The sweep above is *untrimmed*: the tail sits at a fixed incidence and the
design point is the sweep point nearest the cruise CL. ALAS separately solves
the **trimmed** condition (the horizontal-tail incidence that zeroes pitching
moment) at the exact cruise CL, the level-flight value at the **mid-cruise mass**
(takeoff mass less half the cruise fuel, about 286.2 t, so CL 0.543; see
[Formulas & theory](reference/formulas.md#cruise-design-point)):

| | Untrimmed (nearest sweep point) | Trimmed (at the cruise CL) |
|---|---|---|
| α | −0.40° | 0.55° |
| C_L | 0.524 | 0.543 |
| C_D | 0.0275 | 0.0283 |
| L/D | 19.09 | 19.20 |
| Tail incidence | – | −1.84° |

The columns differ because they sit at different CL: L/D rises toward the polar
peak (about CL 0.6), and the trimmed point includes its own trim-drag increment.
The trimmed value is the one that matters for range; the
[mission](mission-and-route.md) fuel figures are built from it.

## Longitudinal stability

The bottom-right panel plots C_m against α. A negative slope means a nose-down
restoring moment as angle of attack grows, so the aircraft is statically stable;
the C_m = 0 crossing is the trimmed angle of attack, and the slope relative to
the lift-curve slope sets the static margin ([Weight, balance & stability](weight-balance-and-stability.md)).

## Where the drag actually goes

<figure markdown>
  ![Drag breakdown at the cruise design point](assets/ave-drag-breakdown-dark.png)
  <figcaption>Left: the design-point drag split into parasite, induced and wave contributions. At design CL 0.524: C_D0 0.0152, induced 0.0114, wave 0.0008. Right: the efficiency curve with the design point marked.</figcaption>
</figure>

The same total drag as the polar, split into three mechanisms. Each points at a
different lever: parasite drag falls with wetted area and surface finish,
induced drag with span and lift distribution, wave drag with thickness and sweep.

## Span loading

<figure markdown>
  ![Span loading distribution](assets/ave-span-loading-dark.png)
  <figcaption>Spanwise lift distribution from the VLM solve: the same load distribution the wingbox FEM in the next chapter integrates into bending moment.</figcaption>
</figure>

This is the same VLM solution read along the span. It links directly to
[Structural analysis](structural-analysis.md): this distribution times the design
load factor is the load the wingbox carries.

## VLM flow visualization

<figure markdown>
  ![VLM streamlines around AVE](assets/ave-vlm-flow-dark.png)
  <figcaption>Streamlines from the vortex-lattice solution at the cruise design point.</figcaption>
</figure>

## Airfoil-level check: NeuralFoil sweep

<figure markdown>
  ![NeuralFoil Reynolds sweep](assets/ave-airfoil-reynolds-dark.png)
  <figcaption>2D airfoil lift/drag behavior across a Reynolds-number sweep, independent of the 3D VLM solve: a sanity check that the section itself behaves reasonably before it's swept into a wing.</figcaption>
</figure>

The wing's root section is run through
[NeuralFoil](https://github.com/peterdsharpe/NeuralFoil), a neural-network
surrogate for panel-method airfoil analysis, across a range of Reynolds numbers.
It is a 2D check independent of the 3D stack; an unrealistic lift-curve slope or
a misplaced drag bucket here is a geometry problem to fix first.

## Cross-check against AVL

<figure markdown>
  ![Model comparison with the AVL cross-check](assets/ave-avl-crosscheck-dark.png)
  <figcaption>Lift, pitching moment, induced drag and span efficiency against angle of attack: ALAS vortex-lattice (lower curve in lift and induced drag) and AVL 3.52 (upper curve). These are the lower four panels of the model-comparison figure in the transonic chapter.</figcaption>
</figure>

AVL 3.52 ran 15 cases on the optimized aircraft at a take-off condition (Mach
0.379, 254 m altitude; 284 strips, 4,544 vortices) and finished
`CompletedComparable`, so its results share ALAS's reference frame. AVL's CL is
about 0.07 above the ALAS vortex-lattice over the sweep. The two are independent
implementations of the same method with different panelling and wake treatment,
so the gap measures model spread, not error against a real aircraft. Setup is in
the [External tools guide](external-tools.md#athena-avl).

## Airfoil CFD

The Analysis menu also opens an OpenFOAM airfoil study. SC2-0714 at 2 deg and
Re 3.45e6 (182,319 cells, 2,000 SIMPLE iterations, 890 s) ended **unconverged**
(pressure residual 2.4e-5 against the 1e-5 target), giving a provisional Cl 0.767
and Cd 0.0122. The app stamps such results as unconverged and provisional, and
they are not used elsewhere in this guide.

<figure markdown>
  ![OpenFOAM pressure field around SC2-0714](assets/cfd-airfoil-pressure.png)
  <figcaption>Gauge pressure around the section, -3.1 kPa to 1.6 kPa, native ParaView render (unconverged, provisional). The ParaView background is light and has no dark variant.</figcaption>
</figure>
