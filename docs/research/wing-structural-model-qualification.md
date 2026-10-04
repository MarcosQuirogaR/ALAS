# Qualification of the fast wing structural model

The product search currently uses a preliminary scalar Euler-Bernoulli response.
Actual mesh material mass is checked during every candidate evaluation, and
completed external static analyses can reject final delivery. These checks do
not make the native response an equivalent swept-wingbox model.

## Reproduced discrepancy

The retained B787 reference-adaptation diagnostic, seed 7 and 24 generations,
had 23,868.762530 kg of product FE primary material against a 23,645.833421 kg
complete-wing FLOPS allowance. It was rejected before any production FE solve.
A separate diagnostic reconstructed the unchanged rejected deck and loads:

| Quantity, ultimate pull-up n = 3.75 | Native | MSC Nastran | NASTRAN-95 |
| --- | ---: | ---: | ---: |
| Front-spar/tip vertical displacement, m | 2.571136 | 3.005643 | 2.961674 |
| Projected curvature-approximation error bound | 0.049998942 | 0.074493917 | 0.072826690 |

The configured numerical error budget is 0.05. It is not an aircraft
certification deflection limit. MSC element 458, GRID 722, is a 6 mm CFRP-QI skin
shell (PSHELL 1 / MAT1 1); its reported 817.212928 MPa exceeds the declared
effective-isotropic allowable of 450 MPa. This comparison is not a laminate
failure analysis. Reducing the remaining mass violation alone cannot qualify
this candidate.

## Required model contract

A replacement fast model needs three independently testable layers:

1. **Section properties.** Construct actual sections normal to a documented
   reference line. Integrate skin, web and cap geometry and material stiffness;
   retain axial, shear, bending, torsional and relevant coupling terms in a
   finite, symmetric, positive-definite sectional constitutive matrix. Define
   the centroid, shear centre, principal axes and eccentric transformations.
   Streamwise airfoil sections cannot be silently reused as normal sections.
2. **Loads and reference frames.** Preserve the actual lift application points,
   shell/bar material inertia, engine and fuel mass positions, and any applied
   couples. Convert projected-span load densities to arc-length densities with
   the correct Jacobian. Force and moment resultants must agree with the
   product deck at section cuts, including eccentric-force torque.
3. **Spatial response and sizing.** Integrate equilibrium and small-rotation
   kinematics along the reference line using the qualified section matrix.
   Update section properties, material mass and self-weight whenever sizing
   changes. Cap-only resizing must not be presented as a remedy for skin/web
   shear, torsional or local panel failure. Reconstruct response at the actual
   front-spar GRID locations, rather than comparing a centroid displacement to
   an eccentric surface-node displacement.

Use SI: positions and translations in m, forces in N, moments in N m,
rotations in rad, stresses/moduli in Pa, material mass in kg. Record each
right-handed local frame explicitly; all comparisons must transform back into
the deck's basic frame. Do not infer a sign convention from plot orientation.

For a specified reference line r(s), distributed forces f(s), applied couples
l(s), and discrete forces F_i at their actual positions r_i, the outboard
resultants include both `(r(u) - r(s)) cross f(u)` and
`(r_i - r(s)) cross F_i`. After the chosen sign convention is verified on a
straight cantilever, global small-rotation kinematics are
`u' = theta cross t + R * gamma` and `theta' = R * kappa`.
The local strains `(gamma, kappa)` come from the complete sectional compliance
acting on consistently transformed force and moment resultants.

The closed thin-wall relation `GJ = 4 A^2 / integral(ds / (G t))` is a
single-cell, free-warping relation. Multicell layouts require compatibility and
cell shear-flow equations. Open sections, restrained warping, unsupported
laminates and singular section matrices require an explicit unsupported-domain
result, not a guessed stiffness or universal cosine correction.

## Evidence required before production substitution

- Exact straight-cantilever tip force and distributed-load solutions; axial,
  two-axis bending and torsion cases with independently known constants.
- Rigid-frame rotation, translation and section-reference invariance; energy
  positivity and reciprocal compliance; dimensional and sign checks.
- Straight swept-beam limits with correct arc-length load conversion; kinked
  reference lines and eccentric point/distributed forces with explicit torque.
- Thin-wall single-cell torsion and validated multicell cases, followed by
  asymmetric section/coupling tests within the supported material model.
- Section-cut force/moment balance against the unchanged product FE deck,
  including fuel/engine and structural-inertia contributions.
- Matched observable and mesh-refinement comparisons across unswept, swept,
  tapered, kinked and dihedral wings. Fix comparison tolerances before claiming
  qualification; do not fit a multiplier to one rejected aircraft.
- Final FE strength/domain acceptance and explicit accounting for secondary
  wing material. Passing the primary-material budget remains necessary only.

These establish implementation and numerical verification. Aircraft physical
validation additionally needs independent evidence for the actual structure,
materials, loads and applicability domain. Two solvers using the same deck are
not independent validation of that deck.

## Primary formulation references

- [Bleyer: finite-rotation beam formulation and linear limits](https://bleyerj.github.io/comet-fenicsx/tours/beams/finite_rotation_nonlinear_beam/finite_rotation_nonlinear_beam.html).
- [OpenFAST BeamDyn theory: sectional coupling and cross-section analysis](https://openfast.readthedocs.io/en/dev/source/user/beamdyn/theory.html).
- [MIT 16.20: thin-wall closed-section torsion](https://ocw.mit.edu/courses/16-20-structural-mechanics-fall-2002/a58ea050460c29f7389ff55e084521ed_ho3.pdf).
