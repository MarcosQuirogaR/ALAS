# Structural analysis

ALAS builds an actual finite-element wingbox model for every design
it analyzes. Not a scaling law dressed up as an FEM: a real mesh with real
elements solved under real load cases. This chapter is that analysis run
against AVE's baseline geometry.

## Two spars, generalized

AVE's wingbox uses two spars, at 25% and 70% of local chord:

```
spars            : 2 @ x/c=['0.25', '0.70']
ribs              : 39 (spacing 0.97 m)
```

The FEM itself doesn't assume two spars; it's built to handle any spar
count, so a three- or four-spar wingbox is a config change, not a code
change. Ribs are spaced automatically (39 of them here, ~0.97 m apart) to
keep panel buckling behavior reasonable across the span.

## The mesh

```
FEM mesh         : 3,494 nodes, 4,746 elements  [OK]
```

This is a real shell/beam mesh of the wingbox structure (spar webs, spar
caps, rib webs, skin panels), not a beam-column simplification. It's built
fresh from AVE's actual wing geometry (span, chord distribution, spar
locations) every analysis run, which is what lets ALAS generalize
across completely different aircraft without hand-remeshing.

<figure markdown>
  ![Wingbox sizing: planform, mass split, and the mass cross-check](assets/ave-structures-sizing-light.png#only-light)
  ![Wingbox sizing: planform, mass split, and the mass cross-check](assets/ave-structures-sizing-dark.png#only-dark)
  <figcaption>Spar lines and rib stations over the planform (left), where the structural mass ends up (centre), and the finite-element result against an independent empirical estimate (right).</figcaption>
</figure>

The mass split is worth dwelling on: skin dominates at 44 %, with spar
caps at 27 %, spar webs at 15 % and ribs at 14 %. That ordering is
characteristic of a transport wingbox: the skin is carrying both bending
(as the outer fibre of the box) and torsion (as the closed cell), which is
why it outweighs the caps that carry bending alone.

## Three load cases

The wingbox is sized against three flight conditions, each producing a
different bending sign and magnitude on the structure:

| Load case | Tip deflection |
|---|---|
| Pull-up (positive limit load) | **+5.52 m** |
| Push-down (negative limit load) | −2.21 m |
| Level (1g cruise) | +1.47 m |

**Pull-up governs sizing here**: the highest-magnitude case is what the
structure is actually built to survive, and it's the one ALAS reports
as the sizing driver. A +5.52 m tip deflection on a 71.75 m-span wing
(about 15% of semispan) reads as large in isolation, but it's the
*limit*-load deflection: the wing bending under the full positive-g
maneuver case from the [V-n diagram](#the-v-n-diagram) below, not cruise
flight. Real long-range twins flex visibly at their wingtips under load for
exactly this reason.

<figure markdown>
  ![Bending moment and spanwise deflection for each load case](assets/ave-structures-loads-light.png#only-light)
  ![Bending moment and spanwise deflection for each load case](assets/ave-structures-loads-dark.png#only-dark)
  <figcaption>Bending-moment and stiffness distribution for the governing case (left), and the spanwise deflection curve for all three cases (right).</figcaption>
</figure>

The bending moment is largest at the root and decays toward the tip (the
root carries everything outboard of it), while stiffness falls the other
way as the box gets shallower and thinner. Deflection is the integral of
that ratio twice over, which is why the curves are so much flatter inboard
than outboard even though the load is highest at the root.

## Margins of safety

Sizing is only meaningful if you can show the result survives the load it
was sized against.

<figure markdown>
  ![Spanwise margin of safety per spar, all load cases](assets/ave-structures-stress-light.png#only-light)
  ![Spanwise margin of safety per spar, all load cases](assets/ave-structures-stress-dark.png#only-dark)
  <figcaption>Margin of safety along the span for each spar cap, with all three load cases overlaid. Anything at or above zero survives.</figcaption>
</figure>

Margin of safety is the fractional headroom between applied stress and
allowable: zero means exactly at the limit, positive means surplus. The
governing case sits at **exactly zero at the root by construction**: that
is what "sized for this case" means, and seeing it land anywhere else
would indicate the sizing loop had not converged. Everywhere else the
margins are positive, and they grow outboard as the load falls faster than
the structure can practically be thinned.

## Mass: two independent estimates, cross-checked

```
semi-wing mass   : 20,301 kg  (both wings: 40,601 kg, Torenbeek estimate: 41,914 kg)
```

This line is doing more than it looks like. ALAS computes wing mass
**two different ways** and reports both: the FEM's own structural mass
(integrating the sized spar caps, webs, and skin: 40,601 kg for both
wings) and Torenbeek's empirical statistical method (41,914 kg, the number
actually used in the weight-and-balance budget in
[Weight, balance & stability](weight-balance-and-stability.md)). They agree
to within **3.1%**, which is the point of running both: an FEM built from
first principles landing close to a decades-validated empirical method is
a real accuracy check, not a coincidence you'd get if either method had a
scale error.

## The analytical fallback: no NASTRAN required

```
NASTRAN static    : error (Failed to launch ...)
```

This AVE run didn't have MSC Nastran available on the machine it ran on,
and it didn't need to. ALAS's own analytical FEM solver (the one that
produced the mesh, tip deflections, and mass above) runs standalone; MSC
Nastran integration is opt-in, for when you want a second, independently-
solved cross-check against an industry-standard solver, not a hard
dependency. If you do have a Nastran license installed and discoverable,
ALAS will use it automatically for static loads, normal modes, and
vibration (sine/random) analysis in addition to its own solver, but every
number in this chapter came from the always-available analytical path.

## Natural frequencies

<figure markdown>
  ![Natural frequencies and mode shapes](assets/ave-structures-modes-light.png#only-light)
  ![Natural frequencies and mode shapes](assets/ave-structures-modes-dark.png#only-dark)
  <figcaption>Rayleigh-quotient bending frequencies (left) and their mode shapes (centre). The right-hand panel is reserved for the Nastran comparison and is empty here because no licence was available.</figcaption>
</figure>

The first four bending modes come out at roughly 3.0, 8.1, 15.9 and
34.0 Hz, with the classic increasing-node-count shapes: the first mode is
a simple tip-up bend, the second has one node, the third two, the fourth
three. The first bending frequency matters most: it needs to stay clear
of the rigid-body flight dynamics from
[the dynamic modes below](#dynamic-modes), which live two orders of
magnitude lower in frequency, and away from likely excitation.

That empty third panel is the honest face of the optional-solver contract:
when Nastran *is* available, its SOL 103 mode shapes are drawn there
alongside, frequency-matched mode by mode. Without a licence the panel
states so rather than quietly disappearing.

!!! warning "Vibration analysis is Nastran-only"
    A separate sine-sweep and random-vibration check exists, but unlike
    every other structural result it has **no analytical fallback**: the
    Miles-equation RMS check needs a real frequency-response solve as its
    input. Without a Nastran licence that stage reports its unavailability
    and produces nothing, which is why no vibration figure appears in this
    guide.

## Deformation renders

<figure markdown>
  ![Patran deformation renders](assets/ave-structures-patran-light.png#only-light)
  ![Patran deformation renders](assets/ave-structures-patran-dark.png#only-dark)
  <figcaption>Deformed-shape renders exported headlessly from Patran, when a licensed installation is available.</figcaption>
</figure>

Like the Nastran cross-check, this panel is opt-in: with a licensed Patran
on the machine, ALAS drives it headlessly to render the deformed
structure under each load case. Without one, the panel says so and
everything else in this chapter is unaffected.

## The V-n diagram

<figure markdown>
  ![V-n diagram](assets/ave-vn-diagram-light.png#only-light)
  ![V-n diagram](assets/ave-vn-diagram-dark.png#only-dark)
  <figcaption>AVE's flight envelope at 359 t MTOW: CS-25-style limit/ultimate load factors against equivalent airspeed.</figcaption>
</figure>

This is where "pull-up load case" and "+2.5g" connect to an actual speed.
V_A (maneuvering speed, 262 kt) is where the positive limit load factor
(2.5g, from `ultimate_load_factor` ÷ 1.5) intersects the stall boundary;
V_D (design dive speed, 428 kt) sets the never-exceed edge of the diagram.
AVE's cruise point (245 kt, n=1) sits well inside the normal envelope, as
it should. The interesting structural work happens at the corners of this
diagram, not at the cruise point.

## Dynamic modes

<figure markdown>
  ![Dynamic stability modes](assets/ave-dynamic-modes-light.png#only-light)
  ![Dynamic stability modes](assets/ave-dynamic-modes-dark.png#only-dark)
  <figcaption>The five classical rigid-body dynamic modes at trimmed cruise (α = 1.0°), all stable.</figcaption>
</figure>

| Mode | Period | Damping | Status |
|---|---|---|---|
| Phugoid | 131.0 s | 0.015 | stable |
| Short period | 9.8 s | 0.108 | stable |
| Roll subsidence | 34.7 s | 1.000 | stable |
| Dutch roll | 18.2 s | 0.137 | stable |
| Spiral | 736.0 s | 1.000 | stable |

All five poles sit in the left half of the s-plane, so every mode is stable,
consistent with the healthy static margin from
[Aerodynamic analysis](aerodynamic-analysis.md). The phugoid's low damping
(0.015) is normal for a large transport, not a red flag: it's a slow,
lightly-damped oscillation pilots and autopilots both handle routinely; the
short-period and Dutch-roll damping ratios (0.108, 0.137) are the ones
that matter more for handling qualities, and both are comfortably positive.

## Control surfaces

<figure markdown>
  ![Control surface layout](assets/ave-control-surfaces-light.png#only-light)
  ![Control surface layout](assets/ave-control-surfaces-dark.png#only-dark)
  <figcaption>Ailerons, elevator, and rudder sizing and placement, derived from the same geometry and design-space parameters as everything else in this guide.</figcaption>
</figure>

Control-surface chord and span fractions come from configurable defaults
(`config/control_surfaces_config.py`) rather than being hardcoded per
aircraft. Swap AVE for a different preset and the control surfaces
re-derive from that aircraft's own planform.
