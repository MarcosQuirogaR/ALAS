# ALAS GUI Release Validation

Dispatch-only tracking document for `alas-gui` release validation. Not shipped
with the application; not read by any build step. Owned exclusively by the
`form_page_parts/part_02.rs` + `tour_data.rs` dispatch lane for editing.

Two kinds of entries live here, and they must not be conflated:

- **Executed here** — an automated check that actually ran in a dispatch
  session, with the exact command and exact pass/fail test names.
- **Required platform interaction measurement** — a manual or scripted check
  that requires a live Windows or Linux desktop session (a window, a mouse,
  wall-clock timing, a process monitor). Static source reading is *not*
  a substitute and must never be logged here as if it were a measurement.

## 1. Automated tests

### 1.1 Session 2026-09-22 — NOT EXECUTED (permission classifier denial)

Every `cargo` invocation attempted in this dispatch job — `cargo test -p
alas-gui --lib ...`, `cargo fmt -p alas-gui -- --check`, even bare `cargo
--version` — was denied by the session's Bash permission classifier before
running, as were the process-listing commands (`tasklist`, `Get-Process`,
`ps -W`) used to check for a competing compile per the dispatch brief.
Retrying or scripting around the denial was not attempted (see
`alas-dispatcher-runner-denials` precedent: classifier denials in background
dispatch jobs are not to be worked around). A second, independent `ingeniero`
subagent hit the identical denial.

**Consequence:** no automated test in `alas-gui` was actually run in this
session. The two originally-failing tests and the two new regressions below
were verified only by static source tracing (§1.2), which is evidence for
code review, not a substitute for `cargo test` passing.

**Action required before merge/release:** a session or operator with cargo
access must run and confirm PASS:

```
cargo test -p alas-gui --lib "views::form_page_parts::part_02::tests::propulsion_page_routes_to_the_rendered_thermodynamic_preview" "views::tour_data::tests::" "views::form_page_parts::part_02::tests::propulsion_preview_title_tracks_the_configured_engine_name" "views::tour_data::tests::walkthrough_setup_analyses_step_marks_mission_analysis_optional_not_always_run"
cargo test -p alas-gui --lib
cargo fmt -p alas-gui -- --check
```

### 1.2 Static verification performed this session (not a test run)

Two tests were failing against a stale expected value; both fixes make the
test assert the actual, currently-correct product behavior rather than
weakening the assertion (still exact-match / exact-substring, no fields
dropped):

- `views::form_page_parts::part_02::tests::propulsion_page_routes_to_the_rendered_thermodynamic_preview`
  expected the static string `"Thermodynamic Cycle (T-s)"`. Traced
  `AppState::default()` -> `scene::build_page_preview(state, "engine")` ->
  `figure_engine_designer_preview` (`alas-report/src/families/propulsion/cycle.rs:167`)
  -> `ts_preview::preview` -> `diagram()`
  (`alas-report/src/families/propulsion/ts_preview.rs:81-83`), which sets
  `scene.title = format!("{} · T–s", config.geometry.engine.engine_name)`.
  Default `engine_name` is `"GE9X"`
  (`alas-config/src/geometry/engine.rs:231`), so the rendered title is
  `"GE9X · T–s"` — a different field (`page.preview_title`, the static page
  heading) already covers the string the test used to check. Corrected the
  assertion to the actual rendered title and documented why it differs from
  the page heading assertion above it.
- `views::tour_data::tests::the_walkthrough_has_fourteen_steps_in_the_requested_order_and_wording`
  expected `TOUR_STEPS[8].body.contains("mission analysis always run")`.
  Traced `alas-gui/src/views/analyses_view.rs:42-88`: exactly four
  `locked_row` disciplines are rendered under "Core (every run)"
  (aerodynamics, weight & balance/stability, propulsion cycle, field
  performance); native mission analysis is a `toggle_row` under "Optional
  (toggle per run)". The walkthrough text already names the same four
  disciplines as always-running and mission analysis as configurable; only
  the test's stale expected substring was wrong. Corrected it to match.

### 1.3 New regressions added this session (also not yet run — see §1.1)

- `views::form_page_parts::part_02::tests::propulsion_preview_title_tracks_the_configured_engine_name`
  — renames the configured engine and asserts the rendered preview title
  changes with it, locking the routing as dynamic (reads live config) rather
  than a hardcoded string.
- `views::tour_data::tests::walkthrough_setup_analyses_step_marks_mission_analysis_optional_not_always_run`
  — asserts the walkthrough step never regresses to claiming mission analysis
  always runs, and separately asserts it correctly names the four always-run
  disciplines and lists native mission analysis among the configurable ones.

## 2. Required platform interaction measurements

None of the following have been performed in any session; they require a
live desktop on the named platform. Build first: Windows
`cargo build --release -p alas-gui`; Linux `cargo build --release -p
alas-gui` (needs `zenity` or `kdialog` on `PATH` for native file/directory
pickers — `alas-gui/src/path_picker.rs` resolves the backend and returns an
actionable error if neither is found; there is no picker fallback).

### 2.1 Startup

- Launch the built binary from a cold shell (no prior run this boot) on
  Windows and separately on Linux. Measure wall-clock from process start to
  the first interactive frame (main window visible and responsive to a
  click) using a stopwatch or `time`/`Measure-Command` around the launch
  wrapper, not around compilation.
- Repeat once warm (immediately re-launch). Record cold and warm numbers
  separately; note the DPI/monitor scale factor used (§2.4 covers scaling).
- Pass criteria: no crash/panic, no unhandled native dialog, default preset
  loads and renders a 3D preview without a manual refresh.

### 2.2 Optimization cancellation

- Start a full analysis with "Optimize design space" on, from both the
  standard workspace and Sandbox Mode's Full Analysis, on each platform.
- After the run log shows the optimizer stage active, trigger cancellation
  (the control the UI exposes for `AppState::request_pipeline_cancel`,
  `alas-gui/src/run.rs:307`) and measure wall-clock from the cancel action to
  the "Run cancelled safely." log line (`run.rs:290`).
- Pass criteria: the app remains responsive during that interval (no
  frozen frame), the worker thread actually stops (no CPU pegged after
  cancellation completes — check via Task Manager on Windows, `top`/`htop`
  on Linux), and the last-good pipeline result (if any) stays displayed
  rather than showing a partial/corrupt one.

### 2.3 Error and picker recovery

- Windows: trigger the native file/directory picker (Advanced Settings >
  External Tools, or Setup > Analyses > Open Airfoil CFD path fields), cancel
  it without choosing a path, and confirm the field keeps its prior value and
  the app stays usable.
- Linux: repeat with `zenity` present, then with only `kdialog` present (move
  or rename `zenity` off `PATH` for the test), then with neither present —
  confirm the last case surfaces the actionable "neither backend is on PATH"
  error from `path_picker.rs` in the UI instead of hanging or panicking.
- Also exercise one non-picker error path per platform (e.g. point an
  external-tool path at a non-existent binary and Run): confirm the run log
  reports the failure and the app remains interactive for a retry after
  correcting the path.

### 2.4 Scaling and navigation

- On Windows and Linux, run at 100%, 150%, and 200% OS display scaling (or
  the OS equivalent). Confirm the sidebar navigation tree, pinned vs.
  hover-expanded rail, and all Modeling-page forms remain readable and
  clickable with no clipped or overlapping controls.
- Walk every leaf of the navigation tree once per scale factor; confirm each
  page's discipline preview (per `tour_data.rs`'s "Discipline previews" step)
  actually swaps content in the Live Preview dock rather than sticking on the
  previous page's figure.

### 2.5 Event loop and memory behavior

- Leave the app idle (no input) for 10+ minutes on each platform with a
  process monitor attached (Task Manager / `top`). Confirm CPU usage settles
  near zero between frames (no busy-loop) and RSS stays flat (no idle leak).
- Then repeat the full navigate-every-page walk from §2.4 ten times in a
  loop, recording RSS after each pass. A monotonically growing RSS across
  passes (beyond one-time cache warm-up) is a leak; flag it rather than
  averaging it away.

### 2.6 CFD and UAV responsiveness

- CFD: from Setup > Analyses, open the standalone Airfoil CFD study, start an
  OpenFOAM run (Windows and Linux; OpenFOAM is serial-only in this
  environment per prior tooling notes, no MS-MPI), and confirm the UI stays
  interactive (can switch pages, cancel) while it runs, with progress/log
  output updating rather than appearing frozen until completion.
- UAV: run an electric mission plan through `uav::request::run_worker`
  (`alas-gui/src/uav/request.rs:226`) via its UI entry point; confirm results
  stream back through the worker channel without blocking the main thread,
  and that a malformed/incomplete component selection surfaces the
  `Result::Err` message in the UI instead of stalling.

### 2.7 Temporary-output cleanup

- Run a full analysis that produces Patran/NASTRAN or OpenVSP artifacts on
  each platform. Note every path written under the OS temp directory during
  the run (`Get-ChildItem $env:TEMP` before/after on Windows;
  `find /tmp -newer <marker>` on Linux around the run).
- Confirm the app does not accumulate unbounded temp artifacts across
  repeated runs in one session (re-run 5x, compare temp-directory entry count
  before the first and after the fifth run) and that leaving Sandbox Mode
  (discarding the sandbox draft) does not leave orphaned workspace state that
  reappears on the next sandbox entry.
