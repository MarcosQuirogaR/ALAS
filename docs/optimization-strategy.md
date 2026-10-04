# Product optimization and physical acceptance

Product optimization uses a mission-sized multidisciplinary objective. The frozen
Python comparison path remains separate and is not the production search.

## Search and runtime

The design vector is searched in normalized coordinates. Fixed coordinates do not
consume population size or mutation opportunities. The nominal aircraft is
evaluated explicitly; the refinement's initial population combines the
baseline, a diverse elite of the screening sample (with its best point) and a
fresh Latin hypercube.

A screening stage nominates starts. Those starts are scored by the refinement
model before they can win. Screening results cannot become an accepted design
directly.
Exact repeated vectors at the same full fidelity share an evaluation cache;
screening and search caches are separate. One persistent Rayon pool bounds both
independent candidates and their nested VLM work to the requested worker count.
Screening and full analysis share that pool; single-candidate evaluations also
run inside it. Results enter history in input order. Worker count changes
scheduling, not seeded candidate generation or selection order; through a
time-limit stop it changes how many evaluations fit, so a time-limited run is
replayed with its recorded evaluation counts. The coordinator
forwards cancellation to an owned token checked inside multidisciplinary and
mission loops, including during a single long candidate evaluation.

The population uses L-SHADE parameter adaptation and population reduction.
Feasible candidates always precede infeasible ones. The decaying epsilon boundary
can order infeasible candidates during restoration, but cannot replace a feasible
incumbent with a lower-cost violation. Restoration improvements adapt the search
using normalized constraint reduction, even when raw failure costs are equal.

If the evolutionary stage exhausts a positive generation budget without a
feasible point, a bounded local restoration stage polls the least-violating
candidate in the original normalized coordinates. It uses coordinate trials
and an evaluated violation-gradient proposal; predictions alone cannot establish
feasibility. Fixed coordinates and original bounds remain unchanged. At most
four waves request `4 * (2 * free_dimensions + 1)` additional scores, with exact
cache reuse and the same worker pool. This work is recorded separately in the
optimizer diagnostics and run manifest. A repaired feasible point is not evidence
that the objective has converged. Cancellation and zero-generation requests do
not initiate restoration.

Convergence, budget exhaustion, cancellation and absence of a feasible candidate
are distinct outcomes. No feasible design is a legitimate result of contradictory
requirements, insufficient search effort or an unsupported model domain. None
authorizes publication of an infeasible design as a successful optimization.

## Structural coupling

Every product candidate has mandatory structural checks, independently of the
switch that enables downstream structural files and external solvers. Loads use
the structural design gross mass. A bounded study must support its declared MTOW;
a light mission dispatch cannot reduce this requirement. An unconstrained study
uses its converged design mass.

Strength sizing includes wing-carried fuel, engine masses and the wingbox's own
inertia. The self-relief fixed point must converge at its configured numerical
tolerance. The production structural solve also sizes cap area for stiffness,
subject to local section dimensions and cap packaging. Added cap material changes
both stiffness and structural mass; its response is recomputed with its own final
mass. An unachievable layout remains infeasible.

Structural residuals retain measured quantities and normalized violation:

- finite geometry, loads, stiffness, response and material inputs;
- strength under the evaluated sizing and response cases;
- rib spacing and nonoverlapping spar-cap packaging;
- convergence of the structural sizing;
- the declared linear-model validity budget.

Native sizing and actual product FE mesh material inventories are retained for
every candidate; building the latter requires no external solve. Their signed
differences from the empirical FLOPS complete-wing estimate are diagnostic only.
They neither reject candidates nor penalize the objective. FLOPS W1/W2/W3 are
empirical groups whose boundaries and assumptions do not exactly partition the
explicit primary box, so disagreement is not by itself a physical violation.
Missing, nonfinite or nonpositive structural inventories still fail closed.
The explicit structural model does not replace the authoritative FLOPS mass
ledger or silently add a mass-model correction. Actual strength, stiffness and
model-domain limits remain independent constraints.

For a spanwise deflection graph `z(y)`, exact curvature is
`z'' / (1 + z'^2)^(3/2)`. The small-slope curvature approximation has relative
error `(1 + z'^2)^(3/2) - 1`. The configurable default budget of 0.05 is a 5%
numerical approximation budget. It is **not a certification deflection limit**.
Stiffness sizing against it restricts the design study to wings supported by the
linear model. A flexible aircraft outside this domain may be physically viable;
accepting it requires a qualified nonlinear structural assessment and appropriate
aerodynamic coupling, rather than an invented deflection allowance.
Manoeuvre response includes ultimate positive and negative cases; level flight
uses `n = 1`. Stations and deflection are metres, stiffness is N m², forces are N,
moments are N m, masses are kg and signed load factors are dimensionless.

The native model remains a prescribed-load Euler–Bernoulli beam model. This check
does not establish swept-wing bending/torsion coupling, aeroelastic or gust
response, fatigue, damage tolerance, laminate failure, or aircraft certification.
Linear FE results also need domain qualification; agreement between two solvers
does not validate a shared deck.

## Final delivery

The finalist is re-evaluated at reporting fidelity. Native structural checks use
the same mass/load authority as the search. The downstream structural solve uses
the stiffness-sized section, matching fuel relief and design gross weight.
Search and reporting use the same multi-condition critical neutral-point model
for stability boundaries, rather than substituting the clean-flight probe into
both clean and critical conditions. An unavailable critical calculation fails
closed.
Fuel masses mapped to the FE mesh conserve prescribed mass and spanwise first
moment.

Product FE caps are separate upper and lower rectangular flanges at their physical
centroids. Their shell web is not duplicated inside a beam section. Before external
solves, the full-wing structural material mass is integrated from actual FE shell
areas and cap centerline lengths, including offsets. Its discrepancy from the
empirical FLOPS estimate is reported quantitatively as a warning and does not
block an external solve or acceptance. Concentrated fuel and engine masses are
excluded from that material inventory. A missing or invalid inventory still
blocks delivery.

When solved, static FE results must contain displacement and stress outputs for
each required load case and finite reported quantities. Product readers retain
the complete sampled front-spar response, including all six basic-frame degrees
of freedom and explicit subcase identities. Missing or ambiguous curves fail
verification. The maximum sampled vertical-displacement secant provides a lower
bound on maximum spanwise slope. A bound exceeding the linear-model budget vetoes
acceptance; passing that necessary check does not validate the complete 3D
rotation field or establish mesh convergence.
The mixed-material root stress maximum is also vetoed if it exceeds every
declared material allowable. A lower value is not a material-specific strength
certificate: element attribution, local stress interpretation and the composite
failure model still require qualification.
Downstream failures can revoke an earlier finalist acceptance, clear its valid
flag and convergence claim, and retain the reasons in the final report.

Shape priors are separate opt-in soft preferences. Box proportions, trailing-edge
sweep and taper preferences cannot pay for or relax structural failures. Invalid
numerical measurements cannot turn into a satisfied constraint through NaN
comparison or clipping.

## Reproducing a native search measurement

Build the measurement example with an optimized profile:

```text
cargo build -p alas-opt --example optimizer_revision_probe --profile test
target/debug/examples/optimizer_revision_probe CONFIG.json RESULT.json 8 8
```

The positional numbers select workers and generations; a generation count of zero
evaluates the nominal only. The probe records the full configuration, seed 7,
residual table, candidate geometry, structural metrics and search diagnostics.
Its search timer excludes nominal/final diagnostic probes, external solvers and
the full reporting pipeline. Full-pipeline time must be measured separately.

Required regression evidence includes malformed/nonfinite constraint rejection,
analytical beam and frozen-parity checks, mass/inertia conservation, achievable
and impossible stiffness cases, fixed-variable/cache behavior, seeded worker-count
identity, and agreement between final rejection and published optimizer status.
