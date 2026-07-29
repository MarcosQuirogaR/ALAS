# Running ALAS

Three ways to drive it. They all run the same pipeline, so what gets
computed does not change between them.

The desktop application is what the download gives you, and it is what most
people should use. The command line and the library exist in a source
checkout and are worth knowing about if you want to script a sweep or embed
ALAS in something larger.

## The desktop app

Launching the executable opens a window whose layout follows the pipeline:
**Inputs → Design Space → Advanced Settings → Run → Results**. (From a
source checkout, `python main.py` with no arguments opens the same thing.)

The [user guide](user-guide.md) walks through every page. A few points
about how the application is built are worth knowing while you use it:

- **Every form is auto-generated.** The Inputs tab, the Design Space
  bounds table, Advanced Settings: none of them are hand-built widgets.
  A `DataclassForm` walks whichever config dataclass it's pointed at
  (`DesignRequirements`, `DesignVector`, `OptimizerConfig`, …) and emits
  one labeled, unit-annotated field per attribute, with decimal precision
  inferred from the value's physical unit. Every field in
  [Mission requirements](mission-requirements.md) and
  [Design space & optimizer](design-space-and-optimizer.md) is exactly
  what you'd see here: this guide isn't describing a subset.
- **Presets swap the whole design space at once.** Loading AVE, A380-800,
  or any of the other six presets doesn't just prefill values: it swaps
  the geometry scaffold, engine, and cabin defaults together, consistently.
- **A live 3D preview updates as you edit**, debounced so it doesn't
  redraw on every keystroke, alongside a Cabin/Payload preview showing the
  actual seat map or ULD layout from [Cabin & payload](cabin-and-payload.md).
- **Results are grouped by discipline**: a text design-point summary,
  then optimization, aerodynamics, weight and balance, propulsion,
  structures, and mission and route, each with the figures that belong to
  it. A **Live / PNG** toggle switches the whole results view between
  interactive charts and flat images (useful for pasting into a report),
  and **Export PNGs…** saves every current chart to a folder in one step.
- **"Analyze baseline" runs without the optimizer**: a fast way to sanity-
  check a preset or a hand-edited design space before committing to a full
  optimization run.

The packaged application is a compiled shell around a local analysis
service. That structure is why the download needs nothing else installed:
the service and everything it depends on travel inside the executable. See
[How ALAS works inside](architecture.md#the-desktop-application).

## The command line

Available in a source checkout. Everything the application does, it does
headlessly, which is what makes it useful for batch work:

```bash
# The default reference case (optimize + analyze + mission)
python main.py -c configs/example_config.yaml --plots

# Skip the optimizer, analyze the nominal design directly
python main.py --no-optimize --plots

# Skip SUAVE mission analysis (faster, if you don't need it)
python main.py --no-mission --plots

# Reproducible run with an explicit seed
python main.py --no-optimize --plots --seed 42

# See the full effective config as YAML (including every default)
python main.py --save-config full_config.yaml
```

`--no-optimize` is what this guide's analysis chapters are built from:
it's the fastest path to a complete, trustworthy evaluation of a known
design, without waiting on an optimizer search.
[Optimization results](optimization-results.md), by contrast, needs the
full `-c configs/example_config.yaml --plots` run.

!!! note "`--plots` renders a subset of the available figures"
    The analysis itself is identical whichever front end runs it, but
    figure *rendering* is not uniform: `--plots` writes the aerodynamic,
    geometry, stability, weight-and-balance and propulsion figures, while
    several others (the structural suite, the cabin layout, the transonic
    section fields, the drag breakdown and the mission charts) are wired
    into the desktop application's Results view and its **Export PNGs…**
    button instead. They come from the same factories in
    `reporting/visualization.py` and the same `PipelineResult`; if you are
    scripting and want one that `--plots` skips, call its factory directly
    with the result object, which is exactly how the figures in this guide
    were produced.

Every CLI flag maps directly onto something in this guide:

| Flag | Effect |
|---|---|
| `-c, --config` | Load a YAML requirements/overrides file (see [Mission requirements](mission-requirements.md)) |
| `-o, --output` | Where figures and data files land (default `./outputs/`) |
| `--no-optimize` | Analyze the nominal design; skip [Design space & optimizer](design-space-and-optimizer.md)'s search |
| `--no-baseline` | Skip the baseline comparison pass |
| `--no-mission` | Skip [Mission & route analysis](mission-and-route.md) even if enabled in config |
| `--no-parallel` | Run pipeline stages sequentially (deterministic debugging) |
| `--plots` / `--show` | Save figures / also display them interactively |
| `--seed` | Override the optimizer's random seed |
| `--save-config` | Write the full effective config and exit |

## As a library

For anything more custom than a config file covers (scripting a sweep
across multiple requirement sets, integrating ALAS into a larger
tool), `DesignPipeline` is the same object the CLI and both GUIs call:

```python
from alas import ALASConfig, DesignPipeline

config = ALASConfig.from_yaml("configs/example_config.yaml")
result = DesignPipeline(config).run(make_plots=False)

print(result.optimized_report.design_point)   # cruise alpha, CL, CD, L/D
print(result.optimized_design.span_m)          # winning geometry, by name
print(result.baseline_report.design_point)     # AVE's un-optimized baseline
print(result.mission_result.summary)           # fuel burn, block time, …
```

`PipelineResult` carries everything this guide has walked through as
plain attributes: `optimized_report`/`baseline_report` (the
`AnalysisReport` objects [Aerodynamic analysis](aerodynamic-analysis.md)
and [Weight, balance & stability](weight-balance-and-stability.md) are
built from), `mission_result` (the `MissionResult`
[Mission & route analysis](mission-and-route.md) reads), and
`optimized_design` (the winning `DesignVector` from
[Optimization results](optimization-results.md)). Nothing in the GUI or
CLI has access to information this object doesn't also expose.
