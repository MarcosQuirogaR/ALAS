# Transonic section analysis

Everything in [Aerodynamic analysis](aerodynamic-analysis.md) treats
compressibility with a correlation: the Korn equation returns a wave-drag
*number* from thickness, sweep and lift coefficient. That is the right
tool for an optimizer loop: it costs nothing and it captures the trend.
What it cannot tell you is *where the shock actually sits on the section*,
how strong it is, or whether the boundary layer survives it.

This chapter is the answer to that. ALAS couples to
[MSES](https://web.mit.edu/drela/Public/web/mses/) (Drela's coupled
Euler/boundary-layer solver) and runs the real viscous, compressible
problem on the wing's root section at the trimmed cruise condition.

## What it solves, and why that's different

MSES solves a steady Euler flow field on a body-fitted streamline grid,
coupled to a two-equation integral boundary-layer formulation with a
transition model. In plain terms:

- The **Euler** side gets compressibility right, including embedded
  supersonic regions and the shock that terminates them. A vortex-lattice
  method is incompressible and linear by construction; it has no
  mechanism to produce a shock at all.
- The **boundary-layer** side gets viscosity right, including the
  displacement effect that thickens the effective body, and the shock/
  boundary-layer interaction that decides whether the flow separates.

That combination is why the figures below carry information no other stage
in ALAS produces: the earlier chapters give integrated coefficients,
this one gives a *field*.

## Surface pressure and Mach

<figure markdown>
  ![Surface Cp and local Mach distributions from MSES](assets/ave-mses-pressure-light.png#only-light)
  ![Surface Cp and local Mach distributions from MSES](assets/ave-mses-pressure-dark.png#only-dark)
  <figcaption>Surface pressure coefficient (left) and local Mach number (right) around the root section at the trimmed cruise point.</figcaption>
</figure>

Read the two panels together and the physics is unambiguous:

1. The flow accelerates hard around the leading edge, and by about 6 % chord
   the **upper surface has gone supersonic**: local Mach peaks near 1.24
   even though the freestream is at M 0.70 for this section.
2. That supersonic pocket is sustained across the forward third of the
   chord, held roughly flat by the supercritical section's characteristic
   low-curvature roof.
3. At about **32 % chord the flow shocks down**: a near-vertical jump in
   Cp, with local Mach dropping abruptly back through 1.0. This is the
   shock the Korn equation only ever knew as a scalar penalty.
4. Downstream of the shock the upper surface hovers just around sonic
   before recovering smoothly to the trailing edge, and the lower surface
   pressure rises steeply over the aft region: the rear loading a
   supercritical section relies on to make up the lift it gives away by
   flattening its roof.

A designer reads this plot for shock *position* and *strength*. A shock
far forward and weak is cheap; a strong shock near mid-chord is expensive
in drag and risks separating the boundary layer behind it.

## The Mach field

<figure markdown>
  ![Filled Mach contours around the section](assets/ave-mses-mach-light.png#only-light)
  ![Filled Mach contours around the section](assets/ave-mses-mach-dark.png#only-dark)
  <figcaption>Mach field around the exact panelled geometry MSES solved. The supersonic pocket over the forward upper surface is the dark region; the sharp boundary running down from it is the shock.</figcaption>
</figure>

The contour plot makes the extent of the supersonic region visible in a
way a surface plot cannot: it is not a thin skin over the section but a
pocket that reaches a noticeable distance into the flow. The near-vertical
contour crowding at its aft edge is the shock, and the two blue lobes at
the leading and trailing edges are the stagnation regions.

!!! note "The outline is the geometry that was actually solved"
    The section drawn on this plot is MSES's own panelled geometry, taken
    straight from the solver's output, not a re-plot of the airfoil object
    ALAS sent in. If the two ever disagreed (a meshing failure, a
    degenerate section from an extreme optimizer candidate) the figure
    would show it rather than hide it behind a tidy re-draw.

## Where this sits in a run

MSES runs as one of the parallel post-analysis stages, on the **final**
design only, never inside the optimizer loop, where its cost and its
convergence behaviour would both be unacceptable. Two things come back:

| Result | Contents |
|---|---|
| `mses_result` | A converged 2-D polar: lift, drag and moment across an angle-of-attack sweep |
| `mses_pressure` | The surface Cp/Mach distributions and the Mach field above |

Both are optional. The solver ships with the application, so the usual
reasons this stage produces nothing are a mesh that could not be generated
from a degenerate section, or a solve that did not converge at the requested
condition, neither of which is a crash. Non-convergence is an expected
solver outcome on some geometries. The stage records a non-`ok` status and
the rest of the pipeline continues untouched: the same degrade-don't-crash
contract the
[mission](mission-and-route.md) and
[NASTRAN](structural-analysis.md#the-analytical-fallback-no-nastran-required)
stages follow.

## Three models, three answers

Because ALAS ends up holding an AeroSandbox polar, a SUAVE mission
polar and an MSES polar for the same aircraft, it can put them on shared
axes:

<figure markdown>
  ![AeroSandbox, SUAVE and MSES polars overlaid](assets/ave-model-comparison-light.png#only-light)
  ![AeroSandbox, SUAVE and MSES polars overlaid](assets/ave-model-comparison-dark.png#only-dark)
  <figcaption>The three models on common CL–CD and CL–α axes.</figcaption>
</figure>

This is deliberately **not** presented as a validation exercise, and you
should not read the spread as one model being "wrong". They answer
different questions:

- **AeroSandbox**: the full 3-D aircraft, inviscid lifting surfaces plus
  empirical viscous and wave-drag build-ups.
- **SUAVE**: the same aircraft as flown through the mission, with its own
  internal drag model and at the actual in-flight conditions.
- **MSES**: a single 2-D section, viscous and compressible, with no
  three-dimensional effects at all (no induced drag, no tip losses).

A 2-D section polar sitting well below a 3-D aircraft polar in drag is the
expected result, not a discrepancy: the 3-D number carries induced drag
the 2-D one structurally cannot contain. The value of the overlay is
seeing that the *shapes* agree: that three independent codes describe the
same lift-curve slope and the same drag rise onset.
