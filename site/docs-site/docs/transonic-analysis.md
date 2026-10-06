# Transonic section analysis

The [aerodynamic analysis](aerodynamic-analysis.md) treats compressibility with
correlations: the Korn equation estimates wave drag from thickness, sweep and
CL. That is right for an optimization loop, but a correlation cannot show where
the shock sits on the section, how strong it is, or whether the boundary layer
separates behind it.

For that, ALAS couples to [MSES](https://web.mit.edu/drela/Public/web/mses/)
(Mark Drela's coupled Euler and boundary-layer solver) on the wing's root
section. MSES is proprietary and user-supplied (see the
[External tools guide](external-tools.md#mses)).

## What it solves, and why that's different

MSES solves a steady Euler flow on a body-fitted streamline grid, coupled to a
two-equation integral boundary layer with transition modelling. The Euler part
captures compressibility directly, including supersonic pockets and the shock
that ends them (linearized vortex-lattice methods cannot). The boundary layer
gives displacement thickness, skin friction and shock interaction.

## Surface pressure and Mach

!!! warning "Post-lift-peak points, not cruise"
    Only 2 of the 7 sweep points converged, at α = 5.06 deg and 6.06 deg. The
    section lift coefficients are 1.147 and 1.080 (CD 0.0766 and 0.0913): lift
    is already falling with angle of attack, so these are past the lift peak,
    heavily shocked and probably separated. They are **not** cruise conditions.
    The figures show what MSES resolves; they are not a prediction for the
    cruise section. Non-converged points are excluded from every plot.

<figure markdown>
  ![Surface Cp and local Mach distributions from MSES](assets/ave-mses-pressure-dark.png)
  <figcaption>Surface pressure coefficient (left) and local Mach number (right) around the root section at α = 5.06 deg, section Mach 0.695, Re 7.65e7 (MSES 3.12).</figcaption>
</figure>

1. Flow accelerates round the leading edge and the **upper surface becomes
   supersonic**, peaking at local Mach 1.565.
2. The supercritical section's low upper-surface curvature holds the plateau
   over the forward chord.
3. Near **35 % chord** the flow recompresses through a shock, with a sharp
   pressure jump back below Mach 1.
4. Aft camber then carries rear loading.

Cp ranges from -1.95 to 1.11 over the 8,056-point field.

## The Mach field

<figure markdown>
  ![Filled Mach contours around the section](assets/ave-mses-mach-dark.png)
  <figcaption>Mach field around the solved section geometry. The dark region marks the supersonic pocket terminated by the recompression shock.</figcaption>
</figure>

<figure markdown>
  ![Cp field around the section](assets/ave-mses-cp-dark.png)
  <figcaption>MSES pressure-coefficient field around the optimized root section at α = 5.0°.</figcaption>
</figure>

The supersonic pocket sits above the surface, and the tight contour gradient at
its aft boundary marks the shock. The plotted outline is the actual paneled
coordinates passed to MSES, so geometry irregularities stay visible.

## Stage execution & convergence diagnostics

MSES runs as an optional post-analysis stage on the final configuration, outside
the optimization loop where its solve time and convergence variance would be
prohibitive. It returns a section polar over an angle-of-attack sweep
(`mses_result`) and surface Cp, local Mach and field contours (`mses_pressure`).

### Convergence

ALAS accepts a point only when MSES reports its own convergence ("Converged on
tolerance"); a finite contour file alone is not enough. Partial or
non-converged points keep an explicit status code, are excluded from plots, and
are never mixed into lift-slope regressions or polar interpolation. A failed
point does not stop the pipeline.

<figure markdown>
  ![MSES sweep convergence](assets/ave-mses-convergence-dark.png)
  <figcaption>Converged lift, drag and moment samples and the status of each requested point: 2 of 7 converged with 300 solver iterations.</figcaption>
</figure>

The solver limits matter. With the default 100 iterations and 60 s limit, none of
the 7 points converged in this run (0 of 7, no pressure figure). Raising them to
`max_iterations` 300 and a 300 s timeout (Advanced Settings, MSES Analysis) gave
the two points above. The seven requested points span 2.06 to 8.06 deg; the
lower-angle points nearest a cruise-like lift did not converge, so MSES gives no
confirmed cruise-section result for this design.

## Multi-model comparison

ALAS can overlay whole-aircraft polars from several models on shared axes: its
vortex-lattice solver, three lifting-line references (Prandtl, Fourier,
Helmbold), the polar flown by the mission stage and, when installed and
comparable, AVL and VSPAERO.

<figure markdown>
  ![Aerodynamic model polars overlaid](assets/ave-model-comparison-dark.png)
  <figcaption>Aerodynamic models on shared axes, with the AVL 3.52 cross-check in the lower four panels. VSPAERO is not installed on the machine that produced this run, so it is absent.</figcaption>
</figure>

- **ALAS vortex-lattice**: the full 3D airframe with the empirical drag build-up.
- **Lifting-line references**: closed-form and Fourier solutions for the wing
  alone, a check on lift slope and induced drag.
- **Mission polar**: the polar the flown mission uses, with its own
  flight-condition corrections.
- **AVL and VSPAERO**: independent vortex-lattice and panel solvers. AVL lift and
  pitching moment are overlaid only when they share a reference frame with ALAS;
  AVL near-field total drag is never plotted against the ALAS drag build-up.

A 2D MSES polar has no induced drag, so it is compared with section-level
quantities and not mixed into the whole-aircraft panels. The check is that lift
slope and drag-rise onset stay consistent across the models.
