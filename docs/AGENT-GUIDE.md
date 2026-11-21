# Driving ALAS from a script or an automation client

ALAS can be driven entirely without its interface: by the command line, or by
any program that speaks the Model Context Protocol (MCP), such as an assistant
client or your own script. This guide shows how to connect one, the workflow
that works, a complete example (case C1), and the rules that keep the results
honest. The reference for every option and tool is [HEADLESS.md](HEADLESS.md).

Working by hand in the desktop application is unchanged and remains the
primary way to use ALAS; everything the server does, the interface does too.

## Connecting a client

The client starts ALAS itself and talks to it over stdin and stdout:

```json
{
  "mcpServers": {
    "alas": {
      "command": "C:\\Program Files\\ALAS\\ALAS.exe",
      "args": ["--mcp"]
    }
  }
}
```

Use the path of your `ALAS.exe` (or the `ALAS` binary on Linux). The client's
own documentation says where this entry goes. Add `"--workspace", "<folder>"`
to the arguments to work in a folder of your choice; by default the server
uses `automation` in the per-user data folder (`%LOCALAPPDATA%\ALAS` on
Windows, the XDG data folder on Linux). Every path a tool takes is inside
that folder. Outside it the server uses only the user's tool preferences and
custom-airport store (shared with the desktop application), the navigation
data and route folders, the program's bundled data and the installed external
tools; [HEADLESS.md](HEADLESS.md) lists them. A configuration cannot point at
a program or folder of its own: tool and data locations come from the tool
preferences.

To check the server without a client, send it one request from a file. On
Windows `ALAS.exe` is a windowed program, so PowerShell waits for it only when
it is started like this:

```powershell
'{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | Set-Content request.txt
Start-Process ALAS.exe -ArgumentList '--mcp' -NoNewWindow -Wait `
  -RedirectStandardInput request.txt -RedirectStandardOutput reply.txt
```

On Linux, `echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | ALAS --mcp`.
The reply is one line listing the tools.

## The workflow

1. **Choose an aircraft.** `list_presets` gives the registered aircraft;
   `create_config` writes a configuration that selects one, optionally with a
   template mission (`list_mission_templates`).
2. **Look before you edit.** `get_effective_config` with a `field` such as
   `requirements`, `mission.plan` or `cabin` shows the values the run will use.
   Field units and meanings: [architecture/capability-map.md](architecture/capability-map.md);
   mission plans: [schemas/mission-plan.md](schemas/mission-plan.md).
3. **Edit by patch.** `edit_config` takes a JSON merge patch: only the keys
   you send change. An edit that does not load is refused and the file is
   left as it was.
4. **Validate.** `validate_config` lists errors (the run would stop) and
   warnings, each with its dotted field path. Fix errors before running.
5. **Run.** `start_run` with `optimize` and `baseline` stated. Start with both
   `false`: that evaluates the configured design and flies its mission in
   seconds to minutes. A full optimisation can take much longer.
6. **Wait.** Poll `run_status` every few seconds; it shows the stage in
   progress. `cancel_run` stops at the next stage boundary.
7. **Read the results.** `get_results` returns `results.json` (summary,
   findings, per-stage mission table), the run manifest and the file list;
   `read_output_file` reads a figure (SVG) or its data (CSV, JSON).

## Example requests

Requests a user might give an assistant connected to ALAS:

- "Create a configuration for the ATR 72-600, fly the commercial transport
  template at FL170, and tell me the fuel per stage."
- "Validate cases/a320.yaml and explain each warning."
- "Run cases/a320.yaml without optimisation and summarise the findings."
- "Compare the take-off field length the run needs with the runway available."

## Worked example: case C1

The competition mission LPA-VDE-LPA with a diversion: a regional turboprop
flies Gran Canaria to El Hierro, lands, takes off again on El Hierro's 1,250 m
runway with one engine failing at V1, flies back, holds 15 minutes, misses the
approach and diverts 86 NM to Fuerteventura with a 30-minute final reserve.

**1. The aerodromes.** None of the three is in the curated airport table, so
add them, stating the source of the data (here: AENA Spanish AIP, rounded):

```json
{"name": "add_airport", "arguments": {"icao": "GCHI", "name": "El Hierro",
  "latitude_deg": 27.815, "longitude_deg": -17.887, "elevation_m": 32.0,
  "runway_lengths_m": [1250.0], "toda_m": 1250.0, "lda_m": 1250.0,
  "source": "AENA, Spanish AIP, rounded (case input)"}}
```

and likewise GCLP (27.932, -15.387, 24 m, 3,100 m) and GCFV (28.453, -13.864,
26 m, 3,400 m).

**2. The configuration.**

```json
{"name": "create_config", "arguments": {"path": "c1/c1.json", "preset": "ATR72-600"}}
```

**3. The route and the mission plan,** in one patch. The cruise legs are the
great-circle distance between GCLP and GCHI less about 40 km of climb and
descent (compute it from the coordinates `list_airports` returns). The
missed approach, diversion, final hold and last landing are reserve stages.
The missed approach climbs to the preset profile's last descent level: read it
with `get_effective_config`, field `mission.profile.descent_4_altitude_ft`
(1,500 ft for the ATR 72-600).
The structural solvers are switched off: this is a mission study.

```json
{"name": "edit_config", "arguments": {"path": "c1/c1.json", "patch": {
  "departure_airport": "GCLP", "arrival_airport": "GCHI",
  "structures": {"run_nastran": false, "run_patran_export": false},
  "mission": {"plan": {"name": "C1 competition mission", "stages": [
    {"id": "taxi-lpa", "kind": {"type": "taxi", "time_min": 8}},
    {"id": "takeoff-lpa", "kind": {"type": "takeoff"}},
    {"id": "climb-lpa", "kind": {"type": "climb", "altitude_ft": 10000}},
    {"id": "cruise-lpa-vde", "kind": {"type": "cruise", "extent": {"distance_km": 206}}},
    {"id": "descent-vde", "kind": {"type": "descent", "altitude_ft": 1500, "agl": true}},
    {"id": "landing-vde", "kind": {"type": "landing", "field_elevation_ft": 105}},
    {"id": "taxi-vde", "kind": {"type": "taxi", "time_min": 5}},
    {"id": "critical-takeoff-vde", "kind": {"type": "takeoff", "field": {
      "name": "El Hierro", "elevation_ft": 105, "toda_m": 1250, "isa_deviation_k": 0}}},
    {"id": "climb-vde", "kind": {"type": "climb", "altitude_ft": 10000}},
    {"id": "cruise-vde-lpa", "kind": {"type": "cruise", "extent": {"distance_km": 206}}},
    {"id": "atc-hold", "kind": {"type": "loiter", "time_min": 15, "altitude_ft": 5000}},
    {"id": "descent-lpa", "kind": {"type": "descent", "altitude_ft": 3000}},
    {"id": "approach-lpa", "kind": {"type": "approach", "decision_height_ft": 200}},
    {"id": "missed-approach", "kind": {"type": "go_around", "altitude_ft": 1500, "agl": true}, "reserve": true},
    {"id": "divert-fue", "kind": {"type": "diversion", "distance_km": 159.3, "cruise_altitude_ft": 10000}, "reserve": true},
    {"id": "final-hold", "kind": {"type": "loiter", "time_min": 30, "altitude_ft": 1500}, "reserve": true},
    {"id": "landing-fue", "kind": {"type": "landing"}, "reserve": true},
    {"id": "taxi-fue", "kind": {"type": "taxi", "time_min": 5}, "reserve": true}
  ]}}}}}
```

The great-circle distance is about 246 km, so each cruise leg is about 206 km. The returned
`validation` must show `"valid": true`.

**4. Run and read.**

```json
{"name": "start_run", "arguments": {"config_path": "c1/c1.json", "run_name": "c1",
  "optimize": false, "baseline": false}}
```

Poll `run_status` until it is `completed`, then `get_results` with
`run_name: "c1"`. `results.mission_stages` has the 18 stages with fuel, time,
distance, mass and centre of gravity; the `critical-takeoff-vde` row carries
the field-length check (`required_m` against `available_m` = 1,250 m), which
says whether the continued take-off is possible at that mass. The findings
list everything the physical checks flagged. The mission profile and the other
figures are in `runs/c1/plots/`.

The same case, entered through the tools exactly as above, is one of ALAS's
automated tests. It checks that the served stage fuel and masses equal those
of a direct pipeline run of the same document, and that with the cabin full
the take-off from 1,250 m fails its field check (finding
`FieldTakeoffDistanceViolation`).

## Honesty rules

These apply to anyone driving ALAS, by hand or by program:

- **Report findings with results.** A result with an error-severity finding is
  not a valid design point; say so instead of quoting its numbers alone.
- **Do not tune inputs to hit a target.** Changing coefficients, efficiencies
  or margins until a result matches a published figure hides model error. Use
  the inputs the aircraft and the case actually have, and report the
  difference.
- **State the source of every input you add** (airports, payload items, mission
  data), as `add_airport` requires.
- **Keep units explicit.** Fields are SI unless their name says otherwise;
  aviation inputs in feet, knots or minutes say so in the name.
- **Distinguish what was computed from what was checked.** ALAS's models are
  conceptual-design methods with stated sources ([methods.md](methods.md)); a
  run is not a validation against a real aircraft.
- **Keep the record.** `run_manifest.json` and `results.json` stay beside the
  outputs; cite the run folder with any number you report.

## Limits

- One run at a time per server. Runs cannot be resumed after the server stops.
- The server does not download anything: install navigation data and
  external tools from the desktop application or the command line first.
- Cancellation takes effect at the next stage boundary; an external solver
  already running finishes its call.
- `read_output_file` reads text files up to 1 MiB; binary outputs (for
  example `.vsp3` geometry) are listed by `get_results` and opened with their
  own programs.
