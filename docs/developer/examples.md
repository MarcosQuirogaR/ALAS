# Examples and probes

Cargo examples under `crates/<crate>/examples/`. They are developer tools:
probes that answer one question about the model, harnesses that reproduce a
measurement, renderers that regenerate documentation or screenshots, and a few
exporters. They are not part of the shipped program; a script, document or test
that uses one is named in the last column (a search of docs, CI, site, xtask, tools and the acceptance tests).

All 58 compile with the workspace (`cargo check --workspace --all-targets`,
checked 2026-10-22 on the repository state of that date). Run one with
`cargo run -p <crate> --release --example <name> -- <args>`; the module comment
at the top of each file states its arguments and output. Examples read the
production code paths; they must not carry model logic of their own, and one
that a script or document depends on is named in the last column.

Cargo discovers `examples/<name>.rs` and `examples/<name>/main.rs` only. Moving
an example into a subdirectory such as `examples/dev/` would need an explicit
`[[example]]` entry per file, so the catalogue below is the organising device
instead (ADR 0014).

Conventions: a new diagnostic goes here with a one-line module comment saying
what question it answers; delete it when the question is closed. Output that
other work depends on goes to a directory named on the command line, never into
the repository.

| Crate | Example | Question it answers (module comment) | Referenced by |
|---|---|---|---|
| `alas-acceptance` | `all_preset_cg_closeout` | Generate all-preset CG closeout evidence through the real two-pass payload path. | aircraft-parity.md |
| `alas-acceptance` | `plausibility_rules_md` | Print the plausibility rule table as Markdown (generated from the rule code). |  |
| `alas-acceptance` | `row_contour_probe` | For one preset, the seat row whose box leaves the outer fuselage contour the furthest (the `CABIN-SEATS-IN-CONTOUR` rule's own corner test), with the corner, the contour half-width there and the row's occupancy. Lengths m. |  |
| `alas-aero` | `parasite_area_correction` | Quantify the exposed-main-wing correction against the gross-area convention. | methods.md |
| `alas-aero` | `vlm_fill_bench` | Timing and bit-digest harness for the rotating-stabilizer lattice update. |  |
| `alas-cfd` | `generate_airfoil_case` | Generate an inspectable OpenFOAM case without launching a solver. |  |
| `alas-cfd` | `run_airfoil_case` | Run one complete native OpenFOAM airfoil study from a fresh case folder. | CFD.md |
| `alas-config` | `high_lift_probe` | Declared high-lift take-off CLmax per preset (diagnostic): the Raymer Eq. 12.21 breakdown, the landing value of the same method and the Vref-recovered landing coefficient it can be compared with. |  |
| `alas-gui` | `render_cabin_section_gallery` | Renders a dimensioned cabin cross-section for every registered preset. | cabin-section-renderer.md |
| `alas-gui` | `render_clean_sheet_design_space` | Drive the desktop clean-sheet path headlessly: load the A320-200, choose New aircraft, enter a C919-class brief through the configuration tree the forms edit, render the Design Space page (with its derived bounds) to a PNG, then optionally start the run through the same `start_pipeline` the Run button calls and report the search's feasible count. |  |
| `alas-gui` | `render_gui_tour` | Capture the real desktop shell, page by page, as PNG screenshots for visual review: every sidebar page, every results tab and every Advanced Settings tab, scrolled top to bottom, in English and Spanish and in the light and dark themes. |  |
| `alas-gui` | `render_load_trim_presets` | Render the load-and-trim sheet of every registered transport preset from one seeded analysis (no optimization) each, as PNG in the light theme and, for the first preset, the dark theme, plus the Mass navigation preview of the same run at the dock's default width, for visual review. |  |
| `alas-gui` | `render_mass_preview` | Export the Mass navigation preview and surface-figure raster samples. |  |
| `alas-gui` | `render_optimization_history` | Render the optimization history of a small seeded run, exactly as the results view builds it, plus a 40 000-request synthetic trace, as SVG and PNG in both themes, and time the egui shape conversion of the large one. |  |
| `alas-gui` | `render_plausibility_set` | Dump every result figure of one aircraft case for the plausibility audit: one PNG per figure, the raw plot data of every figure (the numbers plotted) and a mission segment table. The case is a registered preset or a clean-sheet brief (a JSON patch merged onto a base preset), flown in one MTOW mode on the registered design (no optimization), native analyses only. |  |
| `alas-gui` | `render_site_gallery` | Regenerate the documentation gallery (`site/docs-site/docs/assets/ave-*`) as light and dark PNGs from one AVE pipeline run with native analyses, through the same dispatch the CLI `--plots` path and the GUI results view use (`alas_gui::scene::build_result_figure`). | meet-ave.md |
| `alas-mass` | `flops_audit_inputs` | Emit the complete resolved input deck used by the pure FLOPS product. |  |
| `alas-mass` | `flops_preset_comparison` | Emit the controlled pure-FLOPS comparison record for the eight registered aircraft presets. | flops-mass-model.md,mass-model-architecture.md,report_flops_comparison.py |
| `alas-mass` | `oew_composite_probe` | Diagnostic probe for the OEW composite: engine mass and the wing, fuselage and systems terms of the FLOPS ledger against registered values. |  |
| `alas-opt` | `beam_comparison` | Reproduce strength-only and in-loop wing structural inventories in SI. |  |
| `alas-opt` | `cell_probe` | Why one matrix cell's candidates fail: every violated hard residual with its achieved value and limit, for the preset nominal and for chosen rows of a search history. |  |
| `alas-opt` | `eval_profile` | Where one optimizer evaluation spends its time: a repeatable stage split. |  |
| `alas-opt` | `lever_replay` | Equivalence harness for the optimizer evaluation speed-ups: replays a candidate with and without them and compares the results. |  |
| `alas-opt` | `optimizer_revision_probe` | Reproducible coupled-search measurement with complete residual evidence. | optimization-strategy.md |
| `alas-opt` | `vlm_equivalence` | Numerical evidence of the registered nominals for comparing two builds or two settings of the VLM speed switches (`ALAS_VLM_SCHUR_LU`, `ALAS_VLM_FAST_KERNEL`). |  |
| `alas-opt` | `vlm_speed_probe` | Reproducible candidate timing and numerical evidence. |  |
| `alas-pipeline` | `aero_unify_audit` | Reproducible preset aerodynamic and design-point fuel diagnostics. |  |
| `alas-pipeline` | `baseline_stage_profile` | Stage durations of one native baseline pipeline run (no optimization), with the `alas_math::stage_profile` split of the mission stage. |  |
| `alas-pipeline` | `cabin_census` | Cabin census of every registered preset: what the passenger layout seats, how long the cabin it seats them in is, and what that does to the empty and loaded aircraft. |  |
| `alas-pipeline` | `corner_survey` | Diagnostic: the product's payload-range corners of every registered preset (`quick_analysis::payload_range_corners`), one JSON line each, for a before/after comparison of a corner-pricing change. |  |
| `alas-pipeline` | `dispatch_probe` | Diagnostic: the dispatch of a preset's configured route at its declared MTOW, printing the closure's iterates. |  |
| `alas-pipeline` | `export_airplane_geometry` | Dump one built aircraft's lifting geometry as JSON, for cross-checking the native vortex lattice against AeroSandbox on the *same* wing. | vlm_resolution_aerosandbox.py |
| `alas-pipeline` | `export_cabin_scene_fixtures` | Generate preset-design cabin-scene fixtures for renderer development. |  |
| `alas-pipeline` | `export_cpacs` | Write a CPACS 3.5 aircraft-data artifact for the default design. |  |
| `alas-pipeline` | `field_fleet_check` | Export field-method diagnostics over every CADO row and every registered preset. | methods.md |
| `alas-pipeline` | `flops_robustness` | Reference-conditioned FLOPS input cases through the product mass path. | flops-robustness.md |
| `alas-pipeline` | `freighter_probe` | Main-deck freight positions of a passenger body configured as a freighter: the count, the station range and the container used. Lengths m. |  |
| `alas-pipeline` | `headroom_probe` | Floor, floor plus headroom and ceiling of the main and the lower deck at the cabin ends and the middle of every passenger preset: the three must agree (`floor + deck_height = ceiling`). Heights m. |  |
| `alas-pipeline` | `mass_experiment_matrix` | The controlled mass-model experiment matrix. | mass-model-architecture.md |
| `alas-pipeline` | `model_reference_dump` | Dump every model output that has a real-world counterpart, per preset. | aircraft-parity.md,preset-correlation.md,release-packaging.md |
| `alas-pipeline` | `payload_floor_probe` | Cabin floor and seat-containment census of every passenger preset: the main-deck floor height along the seat rows (level or sloping) and how many rows protrude through the wall-inset fuselage ellipse at armrest and shoulder height. Lengths m. |  |
| `alas-pipeline` | `payload_probe` | Payload-only census of every registered preset: seats against the exit limit and the type-certificate maximum, and the installed lower-hold container positions per hold with the hold stations and volumes. |  |
| `alas-pipeline` | `phase_census` | Mass, balance and payload-range census of every registered preset, from the baseline full analysis. |  |
| `alas-pipeline` | `range_factor_probe` | Breguet-factor decomposition probe (model side only). |  |
| `alas-pipeline` | `sec_probe` | Per registered preset: the mid-cabin section (width, height, centre line), the main and lower deck floors and ceilings in the product frame, and the belly inner skin. Lengths m. |  |
| `alas-pipeline` | `tofl_probe` | ISA sea-level take-off field length breakdown per jet preset (diagnostic). |  |
| `alas-pipeline` | `vspaero_refinement` | First-hand native OpenVSP/VSPAERO lifting-mesh sensitivity protocol. |  |
| `alas-report` | `load_trim_sheet` | Render the manufacturer-style load-and-trim sheet for registered presets. |  |
| `alas-report` | `loft_fair` | Diagnostic: Rust validator and sampled fairness of loft JSON files. |  |
| `alas-report` | `loft_outlines` | Diagnostic: model outlines of presets with the fuselage from a loft, as JSON for the overlay-against-drawing acceptance. |  |
| `alas-report` | `stepb_probe` | Diagnostic: reference quantities of a preset's built geometry. |  |
| `alas-report` | `wireframe_figures` | Export the shared live-preview surface renderer as standalone report SVGs. |  |
| `alas-struct` | `nastran95_benchmark` | Reproducible wall-time harness for an already generated NASTRAN-95 deck. |  |
| `alas-struct` | `wingbox_scope_report` | What each registered aircraft's sized wingbox was sized to, and what it was not sized to. |  |
| `alas-struct` | `wingbox_sizing_diagnosis` | Why the strength-sized wingbox outweighs the empirical wing it belongs to. |  |
| `alas-uav` | `collect_rc_innovations` | Optional live RC Innovations collector using the system curl executable. |  |
| `alas-uav` | `fetch_props` | Install propeller performance tables on this machine (opt-in, on demand). | propeller-data.md |
