# Add-ons

An add-on is a small calculator you can add or edit without rebuilding ALAS:
for example a mini Raymer wing-weight calculator whose result goes into the
design. Open **Setup > Add-ons**. Two kinds exist.

| | `script` | `process` |
|---|---|---|
| Language | [Rhai](https://rhai.rs) | anything (Python by default for `.py`) |
| Runs | inside ALAS, sandboxed | external program, your user rights |
| Enabled by default | yes | **no**: you enable each add-on and version |
| Access to files, network, processes | none | whatever the program does |

A process add-on is not sandboxed. Enable it only if you trust its author and
have read its code. A changed version number has to be enabled again.

## Where add-ons live

In the `addons` folder of the per-user data folder (`%LOCALAPPDATA%\ALAS\addons` on
Windows, the XDG data folder on Linux; the same folder the File menu shows, in
every build). The page shows the path and has an **Open add-ons folder** button
and **Reload**. On the first run two examples are installed if absent; delete one
and it is not brought back (delete `.examples-installed` to offer them again).
Each add-on is one folder: `addons/<id>/addon.toml` plus its entry file.

## The manifest

```toml
name = "My calculator"        # required
version = "1.0.0"             # required; recorded with every applied value
author = "You"
description = "What it does and where the method comes from (cite the source)."
kind = "script"               # "script" or "process"
entry = "main.rhai"           # relative to this folder
timeout_s = 10.0              # optional, default 10, at most 120

[[inputs]]
name = "wing_area_m2"         # identifier: letters, digits, underscore
unit = "m^2"                  # label only; the value is passed unchanged
default = 122.4
description = "Wing area"

[[outputs]]
name = "wing_mass_kg"
unit = "kg"
description = "Wing mass"
target = "design_vector.span_m"   # optional, see below
```

At least one output is required. Unknown keys are rejected.

### Targets

An output with a `target` may be written to the design, **only after you click
Apply to design** and see the before/after table. A target is either a dotted
configuration path (`group.field`, a numeric field that exists in the current
configuration) or `design_vector.<name>` for a design variable. The value is
written as is, so it must already be in the field's unit (SI). The candidate
design is decoded and validated first; it is refused if it adds a validation
error or, for a design variable, leaves the cross-preset guardrail, and in that
case nothing changes. Applied values are logged as
`... set by add-on <name> <version>` when you start a run, and
`addon-provenance.json` is written into the run's output folder. A value you
later change by hand is no longer listed.

An output without a target is display-only. The Raymer examples are
display-only because ALAS has no wing-mass override field.

## Script add-ons (Rhai)

The script ends with a map of output name to number:

```rhai
let area = inputs.wing_area_m2;          // inputs: the form values
let span = design.design_vector.span_m;  // design: read-only summary
#{ wing_mass_kg: area * 10.0 }
```

Helpers: `ft_to_m`, `m_to_ft`, `ft2_to_m2`, `m2_to_ft2`, `lb_to_kg`,
`kg_to_lb`, `kt_to_ms`, `ms_to_kt`, `deg_to_rad`, `rad_to_deg`, `clamp`, plus
Rhai's own math (`sqrt`, `cos`, `**`, ...). `print` goes to the add-on log.
There is no `import`, `eval`, file, network, process or clock access. Limits per
run: the manifest timeout, 5 million operations, call depth 32, strings of at
most 64 KiB, arrays of 10 000 and maps of 1 000 entries (Rhai does not meter
bytes; these caps bound memory). Exceeding one is an error, never a hang.

## Process add-ons (JSON protocol, version 1)

ALAS starts the entry file in the add-on folder: a `.py` file with the
interpreter set on the page (default `python` on Windows, `python3` elsewhere),
anything else as an executable. It writes one JSON object to the program's stdin:

```json
{"protocol": 1, "inputs": {"wing_area_m2": 122.4}, "design": {"preset": "...", "design_vector": {"span_m": 71.75}}}
```

and reads one JSON object from stdout:

```json
{"outputs": {"wing_mass_kg": 5896.8}, "messages": ["optional text"]}
```

stderr is kept as the add-on log. A non-zero exit, a timeout (the process is
killed together with every process it started: `taskkill /F /T` on Windows, a signal to its process group on Unix), output over 1 MiB, invalid JSON,
or a missing or non-finite declared output is an error. Undeclared outputs are
ignored. `main.py` of the bundled `raymer_wing_weight_py` example is a complete
program.

## The design summary

`design` holds `preset`, `design_vector` (the design variables in SI, as in the
Design Space page) and, after a run has completed, `analysis` with
`geometry_summary`, `component_masses_kg` and `static_margin` of the optimized
or baseline report. Before any run there is no `analysis`.

## The Raymer examples

Both compute Raymer, *Aircraft Design: A Conceptual Approach*, 6th edition,
Eq. 15.25 (section 15.3.2, printed page 574; symbols on pp. 577-579), the
cargo/transport wing weight in pounds and square feet. They convert from SI and
return kilograms. The default inputs are illustrative narrow-body numbers, not a
preset. This is a statistical conceptual-design equation; ALAS's own mass model
is the reference for the design, and the add-on result is not validated against
any aircraft.
