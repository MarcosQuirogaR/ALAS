# Optimization results

This chapter runs the search described in
[Design space & optimizer](design-space-and-optimizer.md) for real
(`python main.py -c configs/example_config.yaml --plots`) and shows what
differential evolution actually does to AVE's baseline geometry over 15
generations.

## Convergence

<figure markdown>
  ![Optimization convergence](assets/ave-optimization-history-light.png#only-light)
  ![Optimization convergence](assets/ave-optimization-history-dark.png#only-dark)
  <figcaption>466 valid evaluations (of the ~1,440 the population/generation settings imply; the rest were rejected by a penalty or hard constraint before scoring). Color is span; the red line is the best-so-far L/D.</figcaption>
</figure>

Two things are worth reading off this chart rather than skimming past it.
First, the climb isn't smooth; it happens in **discrete steps** (the red
line jumps at roughly evaluation 160 and again near 320), which is exactly
what you'd expect from differential evolution: the population improves in
bursts when a mutation lands in a better region, then holds while the
population explores around that point before the next improving mutation
is found. Second, the color gradient makes the story visible without
reading a single number: the best designs cluster in yellow-green (larger
span) while the early, worse designs skew teal-purple, smaller span. The
search is visibly discovering that, within AVE's constraints, more span is
worth its structural and wing-area cost.

!!! note "Two L/D numbers, on purpose"
    The L/D axis here is the optimizer's **fast cruise-estimate**
    aerodynamics: a deliberately cheaper VLM evaluation used *inside* the
    search loop, because a full high-fidelity sweep on every one of ~1,400
    candidate evaluations would make optimization impractically slow. Once
    the search picks a winner, ALAS re-evaluates that *one* design
    with the full, final drag-polar fit: a different, more accurate
    number, reported below and used throughout the rest of this guide.
    Seeing ~21.0 in this chart and ~19.2 in the next section isn't a bug;
    it's the two-fidelity-level architecture doing its job.

<figure markdown>
  ![Every candidate planform the search evaluated, overlaid](assets/ave-design-evolution-light.png#only-light)
  ![Every candidate planform the search evaluated, overlaid](assets/ave-design-evolution-dark.png#only-dark)
  <figcaption>Every valid candidate's planform, overlaid with transparency: the dense red cluster is where the search spent most of its evaluations once it found a promising region.</figcaption>
</figure>

This is the convergence chart's L/D-vs-evaluation story redrawn in shape
space instead of score space: the faint, scattered outlines are early,
exploratory candidates; the solid dark-red mass is hundreds of candidates
stacked on nearly the same planform, late in the search, refining rather
than exploring. You can see the seeded-near-initial-design starting point
(from [Design space & optimizer](design-space-and-optimizer.md#starting-near-home-not-from-scratch))
directly in how tightly clustered *even the early* candidates are, span-wise,
compared to how wide the full ±60–80 m bound would allow.

## Baseline vs. optimized, the final numbers

| Metric | Baseline (AVE) | Optimized | Δ |
|---|---|---|---|
| Span | 71.75 m | 77.26 m | +7.7% |
| Wing area | 529.0 m² | 567.5 m² | +7.3% |
| Aspect ratio | 9.89 | 10.67 | +7.9% |
| Sweep | 34.0° | 34.7° | +2.1% |
| **L/D (final analysis)** | **17.82** | **19.22** | **+7.9%** |
| Static margin | 0.315 | 0.281 | −0.034 |
| Payload capacity at MTOW | 35.0 t | 50.5 t | +44% |
| Fuel capacity at MTOW | 135.3 t | 104.7 t | −22.6% |

The headline number is the 7.9% L/D improvement, but the payload/fuel
trade underneath it is the more interesting story. MTOW is a hard
requirement (358.67 t either way); what changed is how the airframe
*spends* that fixed weight budget. A larger, more efficient wing let the
optimizer close the weight equation with less fuel and more structure,
and rather than leave that margin unused, it filled the freed-up weight
budget with payload capacity instead. Static margin dropped slightly
(0.315 → 0.281) but stayed well clear of the 0.05 hard floor: the search
found a genuinely more efficient point, not one that borrowed stability
margin to get there.

<figure markdown>
  ![Drag polar, baseline vs optimized](assets/ave-polar-comparison-light.png#only-light)
  ![Drag polar, baseline vs optimized](assets/ave-polar-comparison-dark.png#only-dark)
  <figcaption>The optimized design's drag polar sits measurably below the baseline's across the shared CL range: the L/D gain isn't confined to one point, it holds across the polar.</figcaption>
</figure>

## Planform, before and after

<figure markdown>
  ![Planform comparison](assets/ave-planform-comparison-light.png#only-light)
  ![Planform comparison](assets/ave-planform-comparison-dark.png#only-dark)
  <figcaption>Baseline (dashed blue) vs. optimized (solid red): wing, tail, and fuselage outline, drawn to the same scale.</figcaption>
</figure>

The optimized wing is visibly longer-spanned and the tail visibly larger:
`tail_scale` moved from 1.00 to 1.07, consistent with the aft-CG shift that
a longer, further-aft-loaded wing implies. This is the same information as
the table above, but it's worth looking at directly: a 7.7% span increase
doesn't look dramatic as a percentage, but drawn to scale against the
baseline it's an unmistakably bigger aircraft.

## Airfoil evolution

<figure markdown>
  ![Airfoil evolution across the optimizer's population](assets/ave-airfoil-evolution-light.png#only-light)
  ![Airfoil evolution across the optimizer's population](assets/ave-airfoil-evolution-dark.png#only-dark)
  <figcaption>The spread of root-airfoil shapes the search explored: thickness and camber scaling plus the four Hicks-Henne bump variables, sampled across the population.</figcaption>
</figure>

This is the airfoil-shape end of the same 16-D search: `airfoil_thickness_scale`
landed at 0.90 (10% thinner than AVE's baseline section) and
`airfoil_camber_scale` at 0.96: a thinner, slightly-less-cambered section,
consistent with the higher aspect ratio letting the optimizer trade section
thickness for span without paying an unacceptable wave-drag or structural
penalty.

## What this run does and doesn't tell you

This is **one** optimizer run, with a fixed seed (42) for reproducibility
and a deliberately modest 15-generation budget (this whole guide's runs
are sized to be reproducible in a few minutes, not to represent ALAS's
ceiling). A longer run, a different seed, or a wider design space would
likely find a different (plausibly better) optimum. What this run *does*
demonstrate reliably is the shape of the tool: requirements and bounds in,
a physically coherent, constraint-respecting airframe out, with every
number in between traceable back to a real evaluation rather than an
optimizer's internal fiction.
