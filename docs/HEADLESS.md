# Running ALAS without the interface

The `ALAS` executable is also a command-line program and a local tool server.
Everything here runs the same pipeline the desktop application runs; nothing
changes for someone who only uses the interface. For a step-by-step workflow
aimed at scripts and automation clients, see [AGENT-GUIDE.md](AGENT-GUIDE.md).

Units are SI unless a field name says otherwise (`_ft`, `_kt`, `_deg`, `_min`).

## Command line

```
ALAS [OPTIONS]
```

With no options the desktop application opens. Any of `--config`,
`--save-config` or another headless option runs without a window.

| Option | Value | What it does |
|---|---|---|
| `-c`, `--config` | path | Load a configuration overlay (YAML, or JSON for a `.json` file). Fields left out keep their defaults; `preset` selects a registered aircraft. |
| `--preset` | name | Analyse a registered aircraft (for example `A320-200`, `C295`) instead of the default one. Excludes `--config`; an unknown name is an error that lists the registered ones. |
| `--only` | stages | Run only these physics stages, comma-separated, and the stages they need: `geometry`, `mass`, `aero`, `stability`, `field`, `mission`, `structures`, `feasibility` (or `gate`), or `all`. Prints JSON and writes `<output>/partial_run.json`. Uses the configured design, as `--no-optimize` does. See [Partial runs](#partial-runs). |
| `--cache-dir` | path | Where `--only` keeps the analysed report (default `<output>/.alas-cache`). |
| `--refresh-cache` | | Recompute the report `--only` would read from the cache, and rewrite it. |
| `--trust-config-tool-paths` | | Run the external programs the `--config` file names (MSES, NASTRAN, Patran folders). Without it those locations are ignored, with a warning, and the tool preferences supply them. |
| `--cpacs-input` | path | Use a CPACS 3.5 aircraft as the geometry input. Requires `--no-optimize` and `--no-baseline`. |
| `--live-dispatch` | | Fetch the route from SimBrief (www.simbrief.com) for the account in `mission.simbrief_username`. Without it no request is made, and `--save-config` leaves the account out. |
| `-o`, `--output` | path | Output folder (default `outputs`). |
| `--gui` | | Open the desktop application. |
| `--no-optimize` | | Skip the design-space optimisation; evaluate the configured design. |
| `--no-baseline` | | Skip the baseline high-fidelity polar comparison. |
| `--no-mission` | | Do not fly the native mission. |
| `--no-parallel` | | Run the stages one after another. |
| `--plots` | | Write the report figures as SVG files under `<output>/plots`. |
| `--plot-data` | | Also write each figure's data as CSV and JSON (implies `--plots`). |
| `--show` | | Same as `--plots`. There is no viewer; open the SVG files. |
| `--seed` | integer | Optimiser random seed (default 20260922). |
| `--aero-solver` | `vlm`, `avl`, `both` | Which aerodynamic result family the reports use. |
| `--optimization-solver` | `vlm`, `avl`, `both` | Which backend the optimiser evaluates. |
| `--quiet` | | Print less. |
| `--save-config` | path | Write the effective configuration (defaults and overlay merged) as YAML and exit. |
| `--download-navdata` | | Download missing navigation-data files and exit. |
| `--install-tools` | | Show the external-tool install plan; with `--yes`, download and install. |
| `--yes` | | Consent to the plan `--install-tools` shows. |
| `--uninstall-tool` | id | Remove one installer-managed external tool and exit. |
| `--mcp` | | Serve the tools below on stdin and stdout (see [Tool server](#tool-server)). |
| `--workspace` | path | Folder `--mcp` reads and writes in (default `<user data>/automation`). |
| `-h`, `--help` | | Print the option list. |

Exit status: 0 when an accepted design is delivered; 1 on any error (bad option,
configuration that does not load, failed run, figure export failure); 2 for an
unknown `--only` stage; 3 when the run completed and wrote its files but
delivered no accepted design (verdict NOT ACCEPTED or physical status
INFEASIBLE). Errors go to stderr.

On Windows the executable is a windowed program: run it from a terminal (or
redirect its output) to see what it prints.

### Partial runs

`--only` runs one or a few physics stages on the pipeline's own code and
inputs: the route is planned and recorded first, and the stages read the same
analysed report a full `--no-optimize` run delivers, so each stage reports
what it reports in a full run.

| Stage | Needs |
|---|---|
| `geometry` | the configuration |
| `mass` | the built geometry (a fast mass pass when no later stage is asked for) |
| `aero`, `stability`, `field`, `structures` | the analysed report |
| `mission` | the report and the planned route |
| `feasibility` (`gate`) | the report, the mission and the structural result |

The analysed report (the lattice trims and the sizing closure) is the
expensive part. It is cached in `--cache-dir`, keyed on the complete effective
configuration and the design vector, and read back by a later `--only` run of
the same case unless it asks for `aero` or `stability` or passes
`--refresh-cache`. A rebuilt program does **not** invalidate the cache: after
a change to geometry, mass or aerodynamics code, pass `--refresh-cache`. The
`cache` entry of the output says whether the report was read or computed.

```text
alas --preset C295 --no-optimize --only mission,gate -o out/c295
alas --preset A320-200 --only mass -o out/a320
```

### Configuration files

The configuration schema is the one the desktop application saves. Write the
complete effective configuration of a preset with

```
ALAS --config overlay.yaml --save-config effective.yaml
```

where `overlay.yaml` holds just `preset: A320-200`; the result lists every
field with its current value. The fields, their units and whether a model reads
them are tabulated in [architecture/capability-map.md](architecture/capability-map.md).
Validation errors (which stop a run) and warnings name the dotted field path.
The versioned configuration schema is `alas.config.v1`, generated with the
capability map (`cargo xtask capability-map`, checked by `cargo xtask checks`).
The option list is [architecture/cli-reference.md](architecture/cli-reference.md):
[schemas/alas.config.v1.schema.json](schemas/alas.config.v1.schema.json). It
types every field from the inventory and rejects unknown keys; enum and nested
section types are not expanded and are marked `x-alas.type`. The capability map
and `--save-config` remain the reference for meaning and units.

The mission plan (`mission.plan`, the flown stages and their events) is
described in [schemas/mission-plan.md](schemas/mission-plan.md), with an example
in [schemas/mission-plan.example.json](schemas/mission-plan.example.json).
Payload items (zones, point masses, hold items) are part of the cabin
configuration (`cabin`) and are edited the same way.

Custom airports the desktop application stores (`custom-airports.json` in the
per-user data folder) are used by headless runs too, as are the airports a
saved workspace file carries (`alas_workspace.custom_airports`); the workspace
copy wins when both have the same ICAO code.

### What a run writes

In the output folder:

| File | Content |
|---|---|
| `run_manifest.json` | What the run did: per-stage timings, the search method that executed and its evaluation count, the files written. |
| `design_database.json` | The design and its analysis results. |
| `plots/*.svg` | Report figures (`--plots`). |
| `plots/*.csv`, `plots/*.json` | Data behind each figure (`--plot-data`). |
| `plots/plot_manifest.json` | Every registered figure: written, or why not (stage not run, data missing). |
| `payload_layout.json` | The payload layout of the run. |
| `*.vsp3` and solver folders | Present when OpenVSP, AVL, MSES, NASTRAN or other external tools ran. |
| `results.json` | Written by the tool server only (below). |

The exact set depends on the stages and tools that ran; `run_manifest.json`
lists it.

## Tool server

`ALAS --mcp` serves ALAS as tools of the Model Context Protocol (MCP) on stdin
and stdout. The transport is JSON-RPC 2.0, one compact JSON message per line,
UTF-8. Protocol revisions 2025-06-18, 2025-03-26 and 2024-11-05 are accepted.
Methods: `initialize`, `ping`, `tools/list`, `tools/call`. A plain JSON-RPC
client may call `tools/list` and `tools/call` directly. Batches are not
supported. A message without an `id` is a notification and never gets a
reply, well formed or not; notifications need no action (`notifications/cancelled`
does not stop a run: use `cancel_run`). A message is at most 4 MiB, newline
included; a longer one is refused (-32600) and the next line is read normally.

A tool result carries its value twice: as JSON text in `content[0].text` and
as `structuredContent`. A tool that refuses or fails returns `isError: true`
and `{"error": "..."}`; an unknown tool or malformed call is a JSON-RPC error
(-32602), an unknown method -32601, a line that is not JSON -32700, a request
that is not JSON-RPC 2.0 (wrong `jsonrpc`, `method` not a string, `id` not a
string or number) -32600.

Rules the server keeps:

- Every path a tool takes is relative to the workspace folder. Absolute paths,
  `..` and `.` are refused, as is a path that leads out through a link or
  junction.
- A configuration cannot name a program or a folder. These fields must stay at
  their defaults: `mses.mses_dir`, `mses.osmap_path`,
  `structures.nastran_exe_path`, `structures.nastran_solver_path`,
  `structures.nastran95_dir_path`, `structures.nastran95_runtime_path`,
  `structures.nastran95_rf_stage_path`, `structures.patran_exe_path`,
  `mission.navdata_dir`, `mission.routes_dir`, `mission.texture_path`.
  `create_config`, `edit_config`, `validate_config` and `start_run` refuse a
  document that sets one (an `edit_config` patch with `null` removes it). Runs
  take tool and data locations from the user's tool preferences instead, as
  the command line does without `--config`.
- Outside the workspace the server reads only: the tool preferences
  (`tool-preferences.json`) and the custom-airport store
  (`custom-airports.json`, which `add_airport` also writes), both in the
  per-user data folder; the navigation data and route folders those
  preferences or the defaults name; the program's own bundled data; and the
  external tools the preferences or discovery locate, which a run starts when
  the configuration enables them.
- No network. A configuration that names a SimBrief user is refused by
  `start_run`; navigation data and external tools are installed from the
  command line or the desktop application, not from the server.
- One run at a time, on its own thread; requests are answered while it runs.
  Closing stdin cancels a run in progress and waits for it to stop.
- External solvers run if they are installed and enabled, as in the
  application. They are local programs, started with stdin closed so they
  cannot read the protocol stream.
- `add_airport` writes the user's custom-airport store, which the desktop
  application shares.

### Tools

| Tool | Arguments | Returns |
|---|---|---|
| `list_presets` | | Registered aircraft: `name` (the `preset` key), display name, description. |
| `list_mission_templates` | | The four mission profiles of Raymer's Fig. 3.2 (`simple_cruise`, `commercial_transport`, `low_level_strike`, `air_superiority`). |
| `create_config` | `path`, `preset`, `mission_template`?, `cruise_altitude_ft`?, `overwrite`? | Writes `{"preset": ...}` (plus the template's `mission.plan`) and returns it with its validation. |
| `read_config` | `path` | The document as written. |
| `get_effective_config` | `path`, `field`? | The loaded configuration, or one dotted field of it (`mission.plan`, `requirements.cruise_mach`). |
| `edit_config` | `path`, `patch` | Applies a JSON merge patch (RFC 7386: objects merge, `null` deletes a key, arrays are replaced whole). Refused, with the file unchanged, when the result does not load. |
| `validate_config` | `path` | `valid`, `errors`, `warnings` (each with `field` and `message`), `load_notes`. |
| `list_airports` | `query`? | Up to 50 airports (curated and custom) whose ICAO code or name contains the query. |
| `add_airport` | `icao`, `name`, `latitude_deg`, `longitude_deg`, `elevation_m`, `runway_lengths_m`, `toda_m`?, `lda_m`?, `isa_deviation_c`?, `source` | Adds or replaces a custom airport. `source` records where the data come from. |
| `start_run` | `config_path`, `run_name`, `optimize`, `baseline`, `mission`?, `plots`?, `plot_data`?, `parallel`?, `seed`?, `aero_solver`?, `optimization_solver`? | Starts the run into `runs/<run_name>` (a new folder) and returns its `run_id`. Figures and their data are on by default. |
| `run_status` | `run_id`? | `running`, `completed`, `failed` or `cancelled`, elapsed time, the latest stage events, the error. Without `run_id`, every run of the session. |
| `cancel_run` | `run_id` | Asks the run to stop at its next stage boundary. |
| `get_results` | `run_name` | `results.json`, `run_manifest.json` and the list of files the run wrote. |
| `read_output_file` | `path` | A UTF-8 text file of the workspace (SVG, CSV, JSON), up to 1 MiB. |

`results.json` (schema `alas.headless-results.v1`) holds the text summary of the
design report, the feasibility statement, every physical finding (`code`,
`severity`, `message`, `actual`, `limit`, `unit`) and, for a flown mission plan,
one row per stage: fuel, time, distance, start and end mass, centre of gravity
(% MAC) and, for a take-off stage that names a field, the field-length check
(`required_m`, `available_m`, `feasible`).
