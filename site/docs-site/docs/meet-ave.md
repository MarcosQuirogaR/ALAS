# Meet AVE

Every example in this guide uses the same reference aircraft: **AVE**, a
preliminary long-range widebody twin that ships as ALAS's default test case.
When you launch ALAS with default settings, this is the design space explored
and the operational mission sized against.

## Why a reference case exists

An optimizer navigating sixteen geometric degrees of freedom alongside a
physics stack spanning aerodynamics, structures, propulsion, and mission
simulation requires an established baseline for benchmarking. AVE exists so
the application is runnable and verifiable immediately.

AVE is an uncertified preliminary engineering model, dimensioned around
open-literature 777X-class twin-aisle transport data. It provides a computational
baseline for sanity checks (e.g., verifying that static margin, fuel mass fraction,
and payload-range curves reflect typical long-range transport trends) and stage
execution testing. Its numbers are conceptual estimates and carry no manufacturer
validation.

AVE also serves as the test vehicle for external solver integrations on a
Madrid-departure flight profile. Additional airframe classes, including
Unmanned Aircraft Systems (UAS), are in active development.

## The airframe, as ALAS sees it

These are the sixteen design-vector values defining AVE, corresponding to the
nominal values returned by `DesignVector.default()`:

| Parameter | Value |
|---|---|
| Span | 71.75 m |
| Root chord | 16.50 m |
| Break (yehudi) chord | 7.80 m |
| Tip chord | 1.60 m |
| Leading-edge sweep (inboard) | 34.0° |
| Tip washout | 0.0° |
| Fuselage length | 76.72 m |
| Root airfoil | SC(2)-0714 (NASA supercritical) |
| Tip airfoil | sc20410 |

Run through ALAS's geometry builder, that design vector resolves to
the preliminary aircraft model analyzed across subsequent chapters:

<div class="ave-stat-grid" markdown>
<div class="ave-stat"><div class="label">Wing area</div><div class="value">529.0 m²</div></div>
<div class="ave-stat"><div class="label">Aspect ratio</div><div class="value">9.89</div></div>
<div class="ave-stat"><div class="label">MAC</div><div class="value">9.63 m</div></div>
<div class="ave-stat"><div class="label">Taper ratio</div><div class="value">0.097</div></div>
<div class="ave-stat"><div class="label">H-stab area</div><div class="value">112.7 m²</div></div>
<div class="ave-stat"><div class="label">V-stab area</div><div class="value">62.2 m²</div></div>
</div>

<figure markdown>
  ![Four-panel three-dimensional view](assets/ave-threeview-3d-light.png#only-light)
  ![Four-panel three-dimensional view](assets/ave-threeview-3d-dark.png#only-dark)
  <figcaption>The preliminary AVE geometry rendered in three dimensions (top, front, side and isometric) directly from the geometry pipeline.</figcaption>
</figure>

## The mission

AVE's conceptual requirements specify a M0.84 cruise at 11,887 m (approx. FL390),
a 358.7 t nominal MTOW, and a 350-passenger cabin arrangement. These values
serve as engineering target inputs for sizing checks, not certified flight limits.

## The engine

Two GE9X-class high-bypass turbofans, sized and modeled via 1D thermodynamic
cycle relationships in [Propulsion analysis](propulsion-analysis.md):

| Parameter | Value |
|---|---|
| Bypass ratio (BPR) | 10.0 |
| Overall pressure ratio (OPR) | 60.0 |
| Fan pressure ratio (FPR) | 1.45 |
| Turbine inlet temperature (TIT) | 1670 K |
| Rated static thrust, per engine | 467.0 kN |

## Presets & ongoing UAS development

ALAS provides six other airliner presets (A340-300, A380-800, B787-9,
A320-200, A220-300, DC-10) selectable from the Inputs tab; selecting one
re-scales the design space, cabin layout, and engine parameters accordingly.
AVE serves as the default reference because widebody twin configurations exercise
every coupled discipline simultaneously: transoceanic mission simulation,
wingbox structural sizing, and high-bypass thermodynamic cycle modeling.

Work is in progress to introduce dedicated presets and empirical sizing rules
for Unmanned Aircraft Systems (UAS), addressing distinct low-Reynolds flight
regimes and novel propulsion architectures.

## Where AVE's numbers come from

Every figure and statistic in this guide was generated from computational
runs of ALAS (`alas --no-optimize --plots` or `target/release/alas.exe --seed 42 --output isolated-dir --plots`)
against AVE inputs: either evaluating the baseline geometry without optimization
or performing a full optimization pass using `-c configs/example_config.yaml --plots`.

Because numerical convergence depends on variable bounds, solver tolerances,
and atmospheric conditions, stage execution times vary; ALAS provides no exact
runtime guarantees. Where baseline and optimized evaluations diverge, the
differences are documented in [Optimization results](optimization-results.md).
