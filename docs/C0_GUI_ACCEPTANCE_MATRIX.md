# C0 GUI acceptance matrix

This document is the executable-facing specification for the user-facing GUI
work in C0/W0.3. It turns the observations in `MISSING-THINGS.md` into
repeatable scenarios. A scenario is accepted only when every assertion is
observable in the shipped window; a passing Rust pipeline or SVG test does not
close a scenario.

## Test envelope

Run every applicable scenario with a clean application profile, the same
aircraft preset, and the same seeded input data. Record the build identifier,
Windows version, display scale, window bounds in physical pixels, theme, and
whether an external tool is disabled, absent, incomplete, failing, or
available. Use these representative envelopes:

| Envelope | Window bounds | Windows display scale | Purpose |
|---|---:|---:|---|
| E1 | 1366 x 768 | 100% | Small baseline window; exposes clipping and reachability failures. |
| E2 | 1280 x 720 | 125% | Small scaled window; exposes DPI rounding and compact-form failures. |
| E3 | 1920 x 1080 | 150% | Common large scaled window; exercises responsive reflow. |
| E4 | 2560 x 1440 | 200% | Large high-DPI window; exercises maximum useful figure sizing. |

For each scenario, “no unintended change” means that the named state remains
unchanged after the action, not merely that the screen appears similar. Capture
before/after screenshots and, where the UI exposes them, the viewport zoom,
scroll offset, selected view, selected option, and tool-status text.

Independent zoom is required for each figure and each active viewport; a page
scroll, Fit action, or fullscreen transition must not silently mutate another
figure's zoom state.

Multi-column form placement is required whenever the available width supports
it; the scenario records the exact reflow point rather than relying on a
judgment that the form is compact.

## Scenarios

| ID | Envelope | Preconditions | Actions | Observable assertions | Source bullet |
|---|---|---|---|---|---|
| GUI-01 | E1, E3 | Load the same aircraft in Exterior/3-D Preview. | Select Top, Front, Side, then Isometric; reset the camera between selections. | Each named camera displays the corresponding orthographic direction; Front and Side are not exchanged; Isometric shows the nose-facing direction specified by the camera contract. | `MISSING-THINGS.md:6` |
| GUI-02 | E1, E3 | Exterior preview is visible at its default fit. | Zoom in twice; record the zoom state and visible bounds; press Fit. | Fit restores the complete aircraft in the active viewport and does not silently reset the stored zoom state for another viewport or another figure. | `MISSING-THINGS.md:7` |
| GUI-03 | E1, E3 | Exterior preview is visible. | Zoom in until the aircraft reaches each viewport edge; pan/orbit if available. | The aircraft remains recoverable by Fit; no required geometry is permanently outside the viewport and no blank, unreachable state is produced. | `MISSING-THINGS.md:7` |
| GUI-04 | E1, E3 | Cabin/Payload preview has a known cabin configuration with multiple rows and columns and two decks where applicable. | Select Top, Front, Side, and Isometric; inspect the preview at fit. | Seat rows and columns are rendered as distinguishable repeated geometry; the fuselage wireframe and deck views are present; camera selection does not exchange views or discard the cabin geometry. | `MISSING-THINGS.md:8` |
| GUI-05 | E1, E2 | A run has produced enough log lines to exceed the initial log height. | Resize the window from the initial size to full height; drag the run-log divider through at least three positions; scroll the log to its end. | The log occupies the released lower area; the 3-D preview does not leave a persistent unused blank region; the log remains reachable and its final line can be read. | `MISSING-THINGS.md:9` |
| GUI-06 | E1, E2, E3 | Setup/Inputs contains at least eight fields and the Design Space contains `sweep_deg` and `tail_scale` data keys. | Resize the window across E1/E2/E3; switch between Setup/Inputs and Design Space. | Fields form at least two columns when the available width permits and return to a readable single-column arrangement when it does not; no field or value is clipped; the visible labels are `Sweep` and `Tail Scale`, not the internal snake-case names. | `MISSING-THINGS.md:12-13` |
| GUI-07 | E2, E3, E4 | Open a form with a long label/value and a form with short labels. | Increase/decrease the window width and Windows scale envelope; do not change the data. | Text remains readable and field labels, controls, and validation messages remain associated with the correct field; typography changes only within the defined bounds; no overlap or clipping is present. | `MISSING-THINGS.md:12` |
| GUI-08 | E1, E3 | Open each representative figure family: 3-D preview, cabin preview, and at least one 2-D result. | Double-click the figure; press Escape; repeat with the 2-D result. | Double-click enters a full-window figure mode; the active figure is identifiable; Escape returns to the same page and selected figure; the page does not navigate to a different tab. | `MISSING-THINGS.md:14` |
| GUI-09 | E1, E3 | A figure is in normal mode and then fullscreen mode. | Use the visible zoom-in and zoom-out controls independently in each mode; switch to another figure and zoom it. | Zoom changes only the active figure and active mode; another figure's camera/zoom is unchanged; both modes retain a Fit/recovery action. | `MISSING-THINGS.md:14` |
| GUI-10 | E1, E2, E3 | Open a figure with a dark background and known plot bounds, including Advanced Settings previews where available. | Resize the window through the envelopes; compare the figure bounds with its assigned card; open fullscreen and return. | The figure uses the available card width/height subject to its declared aspect rule; no persistent unused left gutter or clipped plotted line remains; returning from fullscreen preserves the page layout. | `MISSING-THINGS.md:15` |
| GUI-11 | E1, E2, E3 | Open a form/figure pair with enough fields to require reflow. | Increase the text/UI scale or narrow the window until the side-by-side arrangement no longer fits. | The text/variables remain readable on the left while they fit; when they do not, the figure moves below them and occupies the remaining width; neither control nor figure is hidden behind the other. | `MISSING-THINGS.md:16` |
| GUI-12 | E1, E3 | Open landing-gear and control-surface previews for the same aircraft in a light and dark theme. | Fit each preview; toggle symmetry/component visibility if available; compare left/right geometry and component colors. | No isolated cyan geometry appears near the wing; every visible component maps to a declared aircraft part; mirrored geometry is symmetric within the preview's stated tolerance. | `MISSING-THINGS.md:17` |
| GUI-13 | E1, E2, E3 | Open Setup/Inputs and Structures for a preset containing tire classes, class-mix options, and materials. | Open each option list; select one non-default option; reload the form and inspect the effective value. | The lists contain every declared option, including `auto`, `light`, `narrow body`, and `wide body` where declared; the Structures material list is non-empty; the selected value is displayed and survives the form round trip. | `MISSING-THINGS.md:18` |
| GUI-14 | E1, E2 | Complete an airfoil screening run with more result cards than fit vertically in the window. | Scroll the Results/screening page from its top to its bottom without resizing the window. | Section shapes, MSES verification state, top-airfoils list, and 2-D shortlist result cards are reachable by page scrolling; each card has a stable identity; no card requires enlarging the window. | `MISSING-THINGS.md:19`, `MISSING-THINGS.md:30-33` |
| GUI-15 | E1, E2, E3 | A figure is visible inside a vertically scrollable Results or menu page. | Scroll with the pointer over blank/page area, then over the figure's non-control area; compare the figure zoom before and after. | Page scrolling changes only page scroll offset; it does not change figure zoom. Figure zoom changes only through the figure's explicit zoom control or defined gesture. | `MISSING-THINGS.md:20` |
| GUI-16 | E1, E3 | Place `ALAS.exe` beside a valid MSES installation and launch the app from that directory; repeat with MSES disabled and with an absent executable. | Open tool settings; run the MSES-dependent action; inspect the status and result panel. | Executable-relative discovery identifies the valid installation; disabled, absent, incomplete, launch failure, timeout, parse failure, and success each produce a distinct actionable status; an unavailable result never masquerades as a successful MSES figure. | `MISSING-THINGS.md:23`, `docs/MISSING_THINGS_EXECUTION_PLAN.md:282-287` |
| GUI-17 | E1, E3 | Configure no Patran path, an invalid Patran path, and a valid existing-tool path in separate clean profiles. | Run the Patran scene action in each profile. | The UI states whether Patran is disabled, absent, invalid/incomplete, failed, or available; unavailable states include a reason and recovery guidance; available output lists ordered PNG references and displays them. | `MISSING-THINGS.md:25`, `docs/MISSING_THINGS_EXECUTION_PLAN.md:289-294` |
| GUI-18 | E1, E3 | Configure no MSC Nastran path, an invalid path, and an installed path in separate profiles. | Run the Nastran-dependent structures action; inspect modes/vibration/structures results. | The UI distinguishes disabled, absent, invalid/incomplete, launch failure, timeout/parse failure, and success; unavailable states do not show fabricated solver results; successful results identify their source artifact. | `MISSING-THINGS.md:25`, `MISSING-THINGS_EXECUTION_PLAN.md:289-294` |
| GUI-19 | E1, E3 | Have a completed run with figures available for export. | Use File > Export Figures; inspect the user-visible completion/error result and the generated report if the feature is enabled. | The UI identifies the output location and completion state; an archive/download result is distinguishable from an error; a PDF report, when offered, contains ordered figures separated by named sections. If the Wave 1 export/PDF decision is deferred, the UI states that capability is unavailable rather than claiming success. | `MISSING-THINGS.md:24`, `docs/MISSING_THINGS_EXECUTION_PLAN.md:97-103` |

## Unavailable-tool state contract

The following statuses are part of the acceptance oracle, not implementation
suggestions. For each status, the UI must expose the tool name, the status, a
human-actionable reason, and the next available action. “Unavailable” alone is
not sufficient.

| State | Reproducible setup | Required observable result |
|---|---|---|
| Disabled | Tool preference explicitly disabled. | No launch is attempted; status says disabled. |
| Absent | Preference points to a missing executable. | Status says absent/not found and identifies the checked path. |
| Incomplete | Executable exists but a required sibling/binary/input is missing. | Status says incomplete and names the missing requirement. |
| Launch failure | Executable exists but cannot start. | Status says launch failure and preserves the diagnostic. |
| Timeout | Process exceeds the configured timeout. | Status says timeout; the UI remains responsive and offers retry/cancel. |
| Parse failure | Process exits but output is malformed or incomplete. | Status says parse failure; no result figure is presented as valid. |
| Success | Tool and required inputs are valid and the process completes. | Status says success and the result carries the tool/source identity. |

## Coverage and evidence rules

The matrix covers every bullet in the `GUI` and `Hooks` sections of
`MISSING-THINGS.md` that describes a user-observable behavior. The Results
figure bullets are renderer/content contracts and remain routed to W0.2 and
W3.x; GUI-14 covers their screening reachability and GUI-19 covers export
presentation. Physics discrepancies in `MISSING-THINGS.md:89-95` are not GUI
acceptance scenarios and remain W6 work.

For each run, record `PASS`, `FAIL`, or `BLOCKED` per scenario. `BLOCKED`
requires the missing prerequisite and its impact (for example, no installed
MSES binary). A screenshot without the precondition, action, and assertion
record is evidence of appearance only, not acceptance.
