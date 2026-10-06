# Structural analysis

ALAS builds a finite-element wingbox model for every design it analyzes: a real
shell and bar mesh solved under three load cases. This chapter shows that
analysis for the optimized AVE of the registered-preset run with every
structural tool on (analytical solver, MSC Nastran, NASTRAN-95 and Patran).

## Two spars, generalized

The wingbox has two spars, at 25 % and 70 % of local chord, and 32 ribs
(installed spacing 1.132 m against a 1.143 m maximum buckling spacing). The FEM
handles any spar count, so a third or fourth spar is a configuration change.
Ribs are spaced automatically to keep panel buckling in check.

## The mesh

The mesh has 2,850 nodes and 3,991 elements (3,877 shell and 114 bar): spar webs,
spar caps, rib webs and skin panels. It is built from the actual wing geometry
on every run, so no hand-remeshing is needed for a different aircraft.

<figure markdown>
  ![Wingbox sizing: planform, mass split, and the mass cross-check](assets/ave-structures-sizing-dark.png)
  <figcaption>Spar lines and rib stations over the planform (left), where the structural mass ends up (centre), and the finite-element wing box against the FLOPS complete-wing estimate, which has a different scope (right).</figcaption>
</figure>

The native-beam semi-wing structural mass is 13,068 kg: 37 % spar caps, 31 %
skin, 21 % spar webs, 11 % ribs. The split depends on the sizing and is this
run's result, not a rule.

## Three load cases

The wingbox is sized against three conditions:

| Load case | Analytical tip deflection | MSC Nastran SOL 101 | NASTRAN-95 SOL 101 |
|---|---|---|---|
| Pull-up (positive limit load) | **+4.8 m** | about +6.6 m | about +6.3 m |
| Push-down (negative limit load) | about -1.9 m | | |
| Level (1 g cruise) | about +1.3 m | Patran label 1.77 m | |

The analytical values are read from the figure below; the pull-up root bending
moment peaks at about 70 MN m. **Pull-up governs sizing.** A +4.8 m deflection
on a 70.17 m span (about 14 % of semispan) is the *limit*-load deflection under
the full positive-g manoeuvre of the [V-n diagram](#the-v-n-diagram), not a
cruise value.

The finite-element solvers give a pull-up tip deflection about 31 to 38 % larger
than the analytical beam. The app flags why: the ultimate-load response exceeds
the linear-beam curvature budget (curvature fraction 0.19 against 0.05), so
deflections and stresses need geometric-nonlinearity review. Neither number is validated
against a measured wing.

<figure markdown>
  ![Bending moment and spanwise deflection for each load case](assets/ave-structures-loads-dark.png)
  <figcaption>Bending-moment and stiffness distribution for the governing case (left) and the analytical spanwise deflection for all three cases (right). Markers at the tip are the SOL 101 results of MSC Nastran (diamonds) and NASTRAN-95 (circles).</figcaption>
</figure>

Bending moment is largest at the root, while stiffness falls toward the tip as
the box gets shallower and thinner, so the deflection curve is flat inboard and
steep outboard.

## Margins of safety

<figure markdown>
  ![Spanwise margin of safety per spar, all load cases](assets/ave-structures-stress-dark.png)
  <figcaption>Analytical margin of safety along the span for each spar cap, three load cases overlaid. The pull-up case at the front spar (x/c 0.25) is sized to MS = 0. The finite-element models report root stresses above the allowable and flag the candidate; see below.</figcaption>
</figure>

Margin of safety is the fractional headroom between applied stress and
allowable. The governing case sits at **zero along the inboard front spar cap by
construction** (out to about 30 m); that is what "sized for this case" means.
Elsewhere margins are positive and grow outboard.

### Finite-element strength flags

!!! warning "The delivered design is rejected after the finite-element solves"
    With the finite-element solvers on, the feasibility stage raised four
    `StructuralStrengthViolation` errors, and the run log reads "Delivered
    candidate rejected after downstream analysis":

    | Solver | Case | Root stress | Allowable |
    |---|---|---|---|
    | MSC Nastran | pull-up | 1.38 GPa | 480 MPa |
    | MSC Nastran | push-down | 0.55 GPa | 480 MPa |
    | NASTRAN-95 | pull-up (two checks) | 0.74 GPa | 480 MPa and 380 MPa |

    These peak values come from the constrained root of the shell model, and the
    app marks them for element-level and mesh review. They are not evidence that the real wing
    fails, and they are not evidence that it passes. The same run with no FE
    solvers had no blocking findings (13 warnings). The analytical beam, sized to
    MS = 0, does not see root-constraint effects, so the two views disagree.

## Mass: estimates of different scope

```
native beam, one semi-wing : 13,068 kg  (both wings: 26,136 kg)
FE deck primary material   : 36,514 kg  (both wings)
FLOPS complete wing        : 41,928 kg  (both wings)
```

The three figures measure different things. The native beam gives 13,068 kg per
semi-wing. The finite-element deck carries 36,514 kg of shell and bar material for
the whole wing. The FLOPS equations estimate the **complete wing group**, 41,928
kg, adding leading and trailing edges, movable surfaces and fittings; that is the
figure used in [Weight, balance & stability](weight-balance-and-stability.md).
ALAS does not difference them into one error percentage, because FLOPS exposes no
primary-box share. The FE model is not calibrated against a weighed wing, and the
FLOPS equations are regressions.

## Solvers and the analytical fallback

The analytical solver always runs and needs no external tool. MSC Nastran
(here the 2026.1 Student Edition) and NASTRAN-95 are opt-in cross-checks. In this
run MSC Nastran solved SOL 101 (statics), 103 (modes) and 111 (sine response);
NASTRAN-95 solved SOL 101 and 103; Patran rendered the deformations. Setup is in
the [External tools guide](external-tools.md#msc-nastran-nastran-95).

The wing-box sizing is also checked against MSC Nastran by the
`wingbox_nastran_validation` tool of the acceptance crate, which writes the
native beam as a Nastran deck and compares displacement, bending-moment and
spar-stress profiles with the analytical solution (optional `--shell` plate
model). It is not part of the standard run.

## Natural frequencies

<figure markdown>
  ![Natural frequencies and mode shapes](assets/ave-structures-modes-dark.png)
  <figcaption>Bending frequencies from the Rayleigh estimate, MSC Nastran SOL 103 and NASTRAN-95 SOL 103, with mode shapes.</figcaption>
</figure>

| Method | First four bending modes |
|---|---|
| Rayleigh quotient | 1.67, 4.50, 10.83, 24.72 Hz |
| MSC Nastran SOL 103 | about 1.2, 5.2, 12.0, 24.3 Hz |
| NASTRAN-95 SOL 103 | 18.4 to 23.6 Hz |

The Rayleigh and MSC values agree in order of magnitude and mode shape
(increasing node count). **NASTRAN-95 does not reproduce the low modes**: its
frequencies sit at 18 to 24 Hz. That is a disagreement between the two solvers on
the same deck, not a result to trust either way; the cause was not investigated.
The first bending frequency matters most: it must stay clear of the rigid-body
modes (below) and of likely excitation.

### Vibration (MSC Nastran SOL 111)

<figure markdown>
  ![Sine-sweep and random-vibration response](assets/ave-structures-vibration-dark.png)
  <figcaption>Tip sine-sweep response and force-PSD RMS displacement from MSC Nastran SOL 111: tip peak at 3.00 Hz, tip RMS displacement 3.51e-6 m.</figcaption>
</figure>

This check has no analytical fallback: the Miles-equation RMS check needs a real
frequency-response solve. Without Nastran the stage reports itself unavailable.

## Deformation renders

<figure markdown>
  ![Patran deformation renders](assets/ave-structures-patran-dark.png)
  <figcaption>Patran renders of the SOL 101 pull-up, push-down and level cases, deformed against undeformed model.</figcaption>
</figure>

With a licensed Patran, ALAS drives it headlessly to render each load case (three
renders took about 5 to 9 s here). Without Patran the panel says so and nothing
else changes.

## The V-n diagram

<figure markdown>
  ![V-n diagram](assets/ave-vn-diagram-dark.png)
  <figcaption>AVE's flight envelope at 358.7 t MTOW: CS-25-style limit/ultimate load factors against equivalent airspeed.</figcaption>
</figure>

V_A (manoeuvring speed, 261 kt) is where the positive limit load factor (2.5 g,
from `ultimate_load_factor` / 1.5) meets the stall boundary; V_D (design dive
speed, 428 kt) is the never-exceed edge. The cruise point (245 kt, n = 1) is well
inside the envelope. The structural work happens at the corners of the diagram.

## Dynamic modes

<figure markdown>
  ![Dynamic stability modes](assets/ave-dynamic-modes-dark.png)
  <figcaption>The five classical rigid-body dynamic modes at trimmed cruise (α = 2.9°), all stable.</figcaption>
</figure>

| Mode | Period | Damping | Status |
|---|---|---|---|
| Phugoid | 114.4 s | 0.020 | stable |
| Short period | 3.7 s | 0.163 | stable |
| Roll subsidence | 13.7 s | 1.000 | stable |
| Dutch roll | 10.0 s | 0.133 | stable |
| Spiral | 195.0 s | 1.000 | stable |

All five poles are in the left half-plane, so every mode is stable, consistent
with the large static margin in [Aerodynamic analysis](aerodynamic-analysis.md).
Low phugoid damping (0.020) is normal for a large transport. The short-period and
Dutch-roll damping ratios (0.163, 0.133) matter more for handling and are
comfortably positive.

## Control surfaces

<figure markdown>
  ![Control surface layout](assets/ave-control-surfaces-dark.png)
  <figcaption>Slat, flap, aileron, spoiler, elevator and rudder layout with the tail-volume checks (V_h 0.767 in range; V_v 0.054 flagged below the 0.06-0.13 target).</figcaption>
</figure>

Chord and span fractions come from the `control_surfaces` configuration group,
not from per-aircraft code, so another preset re-derives them from its own
planform. The figure also checks tail volume coefficients: V_h = 0.767 is inside
0.75 to 1.25, but V_v = 0.054 is below the 0.06 to 0.13 range and is flagged.
