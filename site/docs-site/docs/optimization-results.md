# Optimization results

This chapter runs the search described in
[Design space & optimizer](design-space-and-optimizer.md) on the registered AVE
preset (GUI Run path, seed 42, default budget: 30 s screening, 120 s refinement)
and shows what it does to AVE's baseline geometry.

## Convergence

The history plots every requested candidate, stage by stage. Valid candidates are
full-size in the accent colour; rejected candidates are small and muted, and a
candidate with no finite objective is a tick in a strip below the plot. The running
best is taken over valid candidates only, and a dashed line shows the screening
stage's own best when it scored with a cheaper model.

<figure markdown>
  ![Optimization convergence](assets/ave-optimization-history-dark.png)
  <figcaption>1,277 evaluations: 291 valid, 986 rejected, 0 failed. The ordinate is the search objective, block fuel from the native mission; the best valid candidate ends near 39.3 t.</figcaption>
</figure>

- **Almost nothing is valid in screening.** The first stage samples the whole box
  and one candidate survives, at about 42.1 t. The leading rejections in the search
  record are the minimum nose-gear load (584), the usable CG range (306), the
  declared fuel capacity (246) and the tail-scrape angle (151); a candidate can
  fail several. Constraints are hard, so a rejected candidate is not a design.
- **Refinement starts from the diverse elite and descends in bursts**, as
  differential evolution does, and the valid cloud tightens as it proceeds.
- **The search stopped on its time budget, not on convergence.** Both stages ended
  on their time limits (592 and 569 analysed candidates), so a longer run or another
  seed would probably keep improving. This is a local refinement around the preset,
  not a global search.
- **The outcome varies between runs.** With the same seed, three full runs of this
  configuration ended at spans of 71.28, 68.70 and 70.17 m. The 71.28 m run flew
  the preset's 5,645 km route rather than the 6,362 km SimBrief route of the
  other two, so it optimized a different mission. The other two differ because
  both stages stop on their wall-clock limits: they analysed 576 and 592
  screening and 518 and 569 refinement candidates, on refinement schedules
  planned from the measured throughput (581 and 596 evaluations). Replaying the
  recorded counts on the same route reproduces this guide's run bit for bit, and
  a run that stops on evaluation budgets only is likewise exact. Every number
  in this guide belongs to the one run named in the [Gallery](gallery.md).

!!! note "Two L/D numbers, on purpose"
    The optimizer scores candidates with a cheaper in-loop VLM evaluation, because
    a full sweep on every candidate would be impractically slow. The winner is then
    re-evaluated with the full drag-polar fit. The winner's in-loop L/D is 19.02;
    the final analysis reports 19.09 at the design point (19.20 trimmed). They are
    two calculations of the same aircraft; quote the second.

<figure markdown>
  ![Every candidate planform the search evaluated, overlaid](assets/ave-design-evolution-dark.png)
  <figcaption>Planform of every valid candidate, overlaid and coloured by evaluation progress. The planforms are close to each other: the search stays near the preset.</figcaption>
</figure>

This is the same story in shape space. The candidates cluster tightly compared with
the full bound box because refinement starts from the screening elite (see
[Design space & optimizer](design-space-and-optimizer.md#screening-then-refinement)).

The Optimization tab of the desktop Results page shows the same history:

<figure markdown>
  ![The Results page, Optimization tab](assets/gui-results-optimization-dark.png)
  <figcaption>The Optimization tab in the application (a shorter-budget run than this chapter's): optimization history, design evolution, airfoil comparison and spanwise airfoil evolution.</figcaption>
</figure>

## Baseline vs. optimized

| Metric | Baseline (AVE preset) | Optimized | Δ |
|---|---|---|---|
| Span | 71.75 m | 70.17 m | −2.2 % |
| Wing area | 525.2 m² | 531.4 m² | +1.2 % |
| Aspect ratio | 9.80 | 9.27 | −5.4 % |
| Leading-edge sweep | 34.0° | 37.0° | +3.0° |
| Peak L/D (polar) | about 18.1 | about 19.3 | +7 % |
| L/D at design CL 0.524 | | 19.09 | |
| **Block fuel (search objective)** | **42,066 kg** | **39,286 kg** | **−6.6 %** |
| MTOW | 358,670 kg | 358,670 kg | 0 |

!!! note "Which fuel number"
    The search objective is block fuel from the native mission. The design mission
    the sizing closes on is the 5,497 km great circle, whose trip fuel for the
    final design is 38.8 t, the same order as the 39.3 t objective. The final
    analysis flies the 6,362 km SimBrief plan and burns 44.5 t. The numbers are
    for different routes (the exact objective definition was not separated from
    the run files); see
    [Mission & route analysis](mission-and-route.md#fuel-burn-and-block-time).

MTOW is a hard requirement (358.67 t either way) and the payload is the declared
35.4 t load case, so there is no payload to trade: the gain appears as fuel, about
6.6 %, with about 7 % more peak L/D behind it. The baseline objective is itself
flagged infeasible against the hard constraints, which is part of why the optimizer
moves.

The optimized static margin is 42.6 % MAC. That is far larger than a real
transport's and is a known model residual, not an achievement; see
[Weight, balance & stability](weight-balance-and-stability.md#static-margin-and-the-neutral-point).

!!! warning "Feasible for the optimizer, rejected after downstream analysis"
    The optimizer's finalist was accepted at reporting fidelity. In the run with
    every finite-element solver on, the feasibility stage then rejected the
    delivered design on four structural strength flags from the MSC Nastran and
    NASTRAN-95 root stresses (see
    [Structural analysis](structural-analysis.md#finite-element-strength-flags)).
    The optimizer does not run those solvers.

<figure markdown>
  ![Drag polar, baseline vs optimized](assets/ave-polar-comparison-dark.png)
  <figcaption>Baseline versus optimized drag polar and L/D: the optimized peak L/D is about 19.3 against about 18.1 for the baseline.</figcaption>
</figure>

## Planform, before and after

<figure markdown>
  ![Planform comparison](assets/ave-planform-comparison-dark.png)
  <figcaption>Baseline (dashed, 71.75 m span) versus optimized (solid, 70.17 m span): wing and tail planform to the same scale.</figcaption>
</figure>

The changes are modest and mostly in shape rather than size: sweep 34.0° to 37.0°,
root chord 16.00 m to 16.20 m, break chord 8.00 m to 8.38 m, tip twist 0° to 1.7°,
wing shift +0.39 m, and `tail_scale` 1.00 to 1.03 (the delivered design database
reports 1.028; the raw search record lists 1.0 for its best design, and this page
quotes the database).

## Airfoil evolution

<figure markdown>
  ![Initial and optimized root airfoil](assets/ave-airfoil-comparison-dark.png)
  <figcaption>Initial versus optimized root airfoil (SC2-0714 family, thickness scale 0.966, camber scale 0.979): the outlines nearly coincide.</figcaption>
</figure>

<figure markdown>
  ![Wing sections along the span](assets/ave-airfoil-evolution-dark.png)
  <figcaption>Wing cross-sections at 25 spanwise stations, root to tip (0 to 35.1 m), coloured by span station.</figcaption>
</figure>

`airfoil_thickness_scale` landed at 0.966 and `airfoil_camber_scale` at 0.979, a
section about 3 % thinner and 2 % less cambered, plus small Hicks-Henne bump
adjustments (a few hundredths of a percent of chord). The supercritical section is
already close to what the objective rewards at this Mach number.

## What this run does and does not show

This is **one** run with a fixed seed and the default 2.5 minute budget. A longer
run, a different seed or a wider space would likely find a different optimum, as the
span spread above shows. What it demonstrates is the shape of the tool: requirements
and bounds in, a constraint-respecting airframe out, every number traceable to a
real evaluation. It does not show the design is a good aircraft; the static-margin
residual and the finite-element flags are the reminders that a model-feasible
design is not a validated one.
