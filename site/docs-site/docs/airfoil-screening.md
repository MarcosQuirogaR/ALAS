# Airfoil screening

Picking a wing section is usually an act of inheritance: you use the one
the last aircraft used, or the one the textbook example used. ALAS
ships a catalogue of roughly **1,665 sections** and can score every one of
them against *your* aircraft at *your* cruise condition, then hand back a
ranked shortlist.

This is an explicitly optional tool. Nothing in a normal run touches it,
and it never mutates the configuration you give it: every candidate is
scored against an isolated copy.

## Why it is a screen and not an answer

Re-running the full three-dimensional pipeline 1,665 times would take tens
of minutes at best. A screening tool that takes half an hour is not a
screening tool. So the search is staged: something very cheap looks at
everything, and progressively more expensive physics looks at
progressively fewer candidates.

### Stage 1: the fast two-dimensional pass

Every section in the library is evaluated with
[NeuralFoil](https://github.com/peterdsharpe/NeuralFoil), a neural-network
surrogate for panel-method airfoil analysis. A network forward pass is
microseconds, so the whole catalogue is swept in seconds. Each candidate is
scored at the lift coefficient your aircraft actually needs in cruise,
inside a tolerance band, and survivors are ranked on a weighted
combination of:

| Term | Default weight | Meaning |
|---|---|---|
| `ld_weight` | 0.7 | Section lift-to-drag at the target lift coefficient |
| `fuel_weight` | 0.3 | Fuel volume the section's thickness distribution allows in the wingbox |
| `robustness_weight` | 0.0 | Penalty for candidates whose performance is sharply peaked in angle of attack |

Both scored terms are min–max normalised across the surviving set before
they are combined, because lift-to-drag and cubic metres of tank volume do
not share a scale.

The fuel term is not decoration. A section that wins on drag by being thin
is quietly shrinking the tank, and
[Weight, balance & stability](weight-balance-and-stability.md#payload-range-and-fuel-volume)
is where that bill arrives. Scoring both together at the screening stage
stops the shortlist filling with sections that cannot carry the mission's
fuel.

### Stage 2: re-rank on your actual wing

Stage 1 has a systematic bias: it flatters low-Reynolds sections that win
an isolated two-dimensional polar but would never win on a large transport
wing. So the top survivors (20 by default) are **rebuilt into your real
wing geometry** and re-evaluated properly: vortex-lattice induced drag,
Raymer parasite build-up, Korn wave drag, and a genuine trim solve using
the live take-off weight, span and root chord.

A candidate whose trim solve cannot actually sustain the required lift is
*demoted outright*, not merely ranked lower. This is the stage that turns
"good airfoil" into "good airfoil **for this aircraft**".

### Stage 3: verify the finalists with real viscous physics

The top few Stage-2 survivors (5 by default) get a full
[MSES](transonic-analysis.md) coupled viscous/inviscid solve at the
sweep-corrected section Mach number: real shock capture, real wave drag.
This is the same solver the transonic chapter describes, applied here as a
final sanity check on a handful of candidates rather than a survey.

## Filters you control

Ranking is only useful after the obviously unsuitable candidates are gone:

| Filter | Purpose |
|---|---|
| `min_tc` / `max_tc` | Thickness-to-chord bounds: excludes sections too thin to build or too thick to fly fast |
| `min_static_margin` | Rejects candidates that would push the aircraft outside its stability floor |
| `name_filter` | Glob pattern over section names, for narrowing to a family |
| `cl_band` | How far from the target lift coefficient a candidate may be scored |
| `alpha_min/max/step` | The angle-of-attack sweep each candidate is evaluated over |

## The transonic caveat, stated plainly

At cruise Mach numbers at or above **0.75**, neither the Stage-1 surrogate
nor the Stage-2 vortex-lattice model represents wave drag. That has a
specific and predictable failure mode: a real supercritical section's whole
advantage is a delayed and softer drag rise near its design Mach, and
neither model can see it. Both will cheerfully rank a thin conventional
section above a supercritical one that would in reality perform far better.

There is no dependable way to detect "is this section supercritical" from
coordinates alone without risking a confidently wrong heuristic, so
ALAS does not try. Instead the result carries an explicit warning
whenever the design's cruise Mach crosses that threshold, and the interface
surfaces it prominently rather than presenting the top-ranked candidate as
a finished answer. Stage 3 exists precisely so the finalists get judged by
a solver that *does* model shocks.

Alongside the algorithmic candidates, a set of real wind-tunnel-validated
transonic sections (the NASA SC(2) supercritical family among them) is
carried as reference points, so a transonic design always has known-good
anchors in view rather than only proxy-scored entries.

## Using the result

The output is a ranked table, not a decision. The intended workflow is:

1. Screen, and read the shortlist.
2. Swap a promising candidate into the wing.
3. Run the real thing: a full [analysis](aerodynamic-analysis.md), and for
   a transonic design a [MSES check](transonic-analysis.md).
4. Compare against the section you started with.

A screening pass tells you which twenty sections out of one thousand six
hundred deserve that attention. It does not tell you which one to build.
