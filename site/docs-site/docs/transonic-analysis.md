# Transonic section analysis

Everything in [Aerodynamic analysis](aerodynamic-analysis.md) treats
compressibility with empirical correlations: the Korn equation estimates wave-drag
penalties from section thickness, leading-edge sweep, and lift coefficient. That is
the appropriate tool for an optimization loop: it carries near-zero computational
cost and captures primary design trends.

What a correlation cannot reveal is *where the shock actually sits on the section*,
how strong it is, or whether the boundary layer separates behind it.

To investigate those local phenomena, ALAS couples to
[MSES](https://web.mit.edu/drela/Public/web/mses/) (Mark Drela's coupled
Euler / boundary-layer solver) to evaluate the viscous, compressible flow field on
the wing's root or defining sections at the cruise condition.

## What it solves, and why that's different

MSES solves a steady Euler flow field on a body-fitted streamline grid coupled to
a two-equation integral boundary-layer formulation with transition modeling:

- The **Euler formulation** captures compressible effects directly, including
  embedded supersonic regions and the recompression shock terminating them.
  (Linearized vortex-lattice methods are incompressible and lack shock-capturing
  mechanisms.)
- The **integral boundary layer** captures viscous displacement thickness, skin
  friction, and shock/boundary-layer interaction to evaluate separation risk.

This provides local field diagnostics that integrated 3D vortex-lattice models
cannot produce.

## Surface pressure and Mach

<figure markdown>
  ![Surface Cp and local Mach distributions from MSES](assets/ave-mses-pressure-light.png#only-light)
  ![Surface Cp and local Mach distributions from MSES](assets/ave-mses-pressure-dark.png#only-dark)
  <figcaption>Surface pressure coefficient (left) and local Mach number (right) around the root section at the nominal cruise condition.</figcaption>
</figure>

Reading the two panels together illustrates the underlying physics:

1. Rapid flow acceleration occurs around the leading edge; by approximately 6% chord
   the **upper surface becomes supersonic**, with local Mach peaking near 1.24 at
   a section freestream Mach of 0.70.
2. The supersonic plateau is maintained across the forward portion of the chord
   by the supercritical section's low upper-surface curvature.
3. Near **32% chord the flow recompresses through a shock**, characterized by a
   sharp pressure jump and deceleration back below Mach 1.0.
4. Downstream of the shock, the flow recovers toward the trailing edge, while aft
   camber provides rear loading to compensate for reduced forward lift.

Engineers inspect these distributions to evaluate shock strength, location, and
downstream boundary-layer health.

## The Mach field

<figure markdown>
  ![Filled Mach contours around the section](assets/ave-mses-mach-light.png#only-light)
  ![Filled Mach contours around the section](assets/ave-mses-mach-dark.png#only-dark)
  <figcaption>Mach field around the solved section geometry. The dark region marks the supersonic pocket terminated by the recompression shock.</figcaption>
</figure>

The contour field illustrates the vertical extent of the supersonic pocket above the
airfoil surface. The tight contour gradient at the aft boundary marks the shock.

!!! note "Discretized section coordinates"
    The section outline plotted above reflects the actual paneled coordinates
    passed to MSES. Geometric irregularities or meshing anomalies remain visible
    in the diagnostic plot rather than masked by idealized splines.

## Stage execution & convergence diagnostics

MSES executes as an optional post-analysis diagnostic stage on the final configuration,
outside the primary optimization loop where solve time and convergence variance
would be prohibitive. Outputs include:

| Result | Contents |
|---|---|
| `mses_result` | 2D section polar data across an angle-of-attack sweep |
| `mses_pressure` | Surface pressure ($C_p$), local Mach number, and field contours |

### Distinguishing converged vs. partial or non-converged points

Transonic viscous-inviscid coupling can be sensitive to strong shock separation or
severe adverse pressure gradients. In ALAS, **converged points are explicitly
distinguished from partial or non-converged points**:

- **Converged points**: Points where the Newton solver reduces Euler and
  boundary-layer residuals below the specified convergence tolerance ($10^{-4}$).
  These points are recorded as valid flow states and included in polar plots.
- **Partial or non-converged points**: Points where MSES exceeds iteration limits,
  encounters boundary-layer separation unsteadiness, or fails to converge on a valid
  streamline grid.
- **Diagnostic transparency**: Non-converged points are flagged with explicit solver
  status codes. In diagnostic outputs, they are visually distinguished (or excluded)
  and never blended into lift-curve slope regressions or polar interpolations.

When an individual point or section solve fails, the stage records diagnostic warning
flags and the broader analysis pipeline proceeds uninterrupted.

## Multi-model comparison

ALAS can compare polars from AeroSandbox (3D VLM + empirical drag build-up),
SUAVE (flown mission simulation drag polar), and MSES (2D viscous section) on shared
axes:

<figure markdown>
  ![AeroSandbox, SUAVE and MSES polars overlaid](assets/ave-model-comparison-light.png#only-light)
  ![AeroSandbox, SUAVE and MSES polars overlaid](assets/ave-model-comparison-dark.png#only-dark)
  <figcaption>Overlay of AeroSandbox, SUAVE, and 2D MSES polars on shared CL–CD and CL–α axes.</figcaption>
</figure>

This comparison provides an engineering consistency check across preliminary models of differing fidelity:

- **AeroSandbox**: Models the full 3D airframe, accounting for induced drag and
  empirical drag build-ups.
- **SUAVE**: Evaluates the airframe along the mission trajectory with independent
  flight-condition corrections.
- **MSES**: Evaluates a 2D section without 3D induced drag or finite-span effects.

A 2D section polar exhibiting lower drag than a 3D aircraft polar is physically
expected due to the absence of induced drag. The diagnostic value lies in checking
that the lift-curve slope and drag-rise onset remain consistent across preliminary
models.
