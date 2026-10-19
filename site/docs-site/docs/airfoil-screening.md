# Airfoil screening

ALAS carries a catalogue of 1,665 airfoil sections (the UIUC coordinate
database) and can score every one against your aircraft at your cruise
condition, then return a ranked shortlist.

Open it from **Analysis, Open Airfoil Screening**. It runs in its own window,
is not available while the sandbox is open, and is optional: a normal run never
touches it, and every candidate is scored against an isolated copy of your
configuration.

## Why it is staged

Running the full three-dimensional pipeline 1,665 times would take tens of
minutes at best. So something very cheap looks at everything, and progressively
more expensive physics looks at progressively fewer candidates.

### Stage 1: fast two-dimensional pass

Every section is evaluated with
[NeuralFoil](https://github.com/peterdsharpe/NeuralFoil), a neural-network
surrogate for panel-method airfoil analysis (model size `large`), over an
angle-of-attack sweep (default -4 to 14 deg in 0.5 deg steps). Each candidate is
scored at the lift coefficient your aircraft needs in cruise (or an explicit
target) and ranked on a weighted blend:

| Term | Default weight | Meaning |
|---|---|---|
| `ld_weight` | 0.7 | Section lift-to-drag at the target lift coefficient |
| `fuel_weight` | 0.3 | Fuel volume the section's thickness distribution allows in the wingbox |
| `robustness_weight` | 0.0 | Drag-bucket retention within `cl_band` (default 0.05) of the target lift coefficient |

The terms are min-max normalised across the surviving set before combining. An
**objective** selector (Balanced, Efficiency, Fuel capacity, Robustness) puts all
the weight on one term, or keeps the blend. The fuel term keeps the shortlist
from filling with thin sections that cannot carry the mission fuel (see
[Weight, balance & stability](weight-balance-and-stability.md#payload-range-and-fuel-volume)).

### Stage 2: re-rank on your wing

Stage 1 flatters low-Reynolds sections that would not win on a large transport
wing. The top 20 survivors by default are rebuilt into your actual wing and
re-evaluated with vortex-lattice induced drag, the Raymer parasite build-up,
Korn wave drag and a trim solve at the live take-off weight, span and root
chord. A candidate whose trim cannot sustain the required lift is demoted
outright.

### Stage 3: MSES check

The top 5 Stage-2 survivors by default get an [MSES](transonic-analysis.md)
coupled viscous/inviscid solve at the sweep-corrected section Mach number. It
needs a user-supplied MSES installation.

## Filters

| Filter | Purpose |
|---|---|
| `min_tc` / `max_tc` | Thickness-to-chord bounds (defaults 0.005 and 0.25) |
| `min_static_margin` | Rejects candidates that push the aircraft below a static-margin floor (Stage 2) |
| `name_filter` | Glob or comma-separated substrings over section names |
| `cl_band` | Lift-coefficient window used for the robustness score |
| `alpha_min/max/step` | Angle-of-attack sweep |
| `top_n` | Number of ranked candidates returned (default 50) |

## Transonic caveat

At section Mach numbers of 0.75 or above, neither the Stage-1 surrogate nor the
Stage-2 vortex-lattice model represents wave drag. A supercritical section's
advantage is a delayed, softer drag rise, which neither model sees, so both can
rank a thin conventional section above a supercritical one that would perform
better. ALAS does not try to detect supercritical shapes from coordinates. The
result carries an explicit warning when the Mach threshold is crossed, and
Stage 3 judges the finalists with a solver that models shocks. Real
wind-tunnel-validated transonic sections (the NASA SC(2) family among them) are
carried as reference points.

## Using the result

The output is a ranked table, not a decision:

1. Screen and read the shortlist.
2. Put a promising candidate into the wing.
3. Run a full [analysis](aerodynamic-analysis.md) and, for a transonic design, an
   [MSES check](transonic-analysis.md).
4. Compare against the starting section.
