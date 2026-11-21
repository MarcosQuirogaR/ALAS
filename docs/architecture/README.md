# Architecture: the capability map

`capability-map.json` and `capability-map.md` list everything ALAS can do and
where each capability lives. They are generated from the source tree by
`cargo xtask capability-map`; never edit them by hand.

## Read it before changing an area

Open `capability-map.md` (overview) or `capability-map.json` (per-field detail)
and look up the area you are about to touch:

| Section | Answers |
|---|---|
| Configuration schema | Which fields exist, their unit and level (routine or advanced), which crates read them, which GUI page edits them |
| Pipeline stages | The stages of a full run, in order |
| Report figures | Every figure id, which is also its plot-data export id |
| External tools | The tools a run can use, their saved preferences and `ALAS_*` overrides |
| GUI pages | Every page and window, which configuration group it edits |
| CLI flags | Every command-line flag and whether it takes a value |
| Add-ons | The manifest keys and kinds |
| File types | Extensions the app names in code, and the configuration dialog filter |
| Smoke tests | The cheap end-to-end test of each family |

## Keep it current

Change code that adds, removes or renames any of the above, then run

```
cargo xtask capability-map
```

and commit the regenerated files in the same commit. `cargo xtask checks`
regenerates the map in memory and fails when the committed copy differs, so a
stale map cannot be committed past the hook or CI.

`cargo xtask capability-map --check` runs the same verification without writing.

## What `checks` enforces

1. **The map is current** (see above).
2. **Every configuration field is used.** A field passes when a model or
   pipeline crate reads it (`computed`), or when an `alas-config` function that is
   not validation or loading reads it and model code calls that function by name
   (`config_method`). Otherwise the check fails with the field's path. A field
   that is read only by validation, the editor, or nothing at all is either wired
   into a calculation or listed in `field-exemptions.txt`.
3. **Exemptions stay honest.** `field-exemptions.txt` holds one line per field:
   `path | display_only | deprecated | reason`. An entry fails the
   check when the field is now read by model code or is not a field any more.
   There is no "known gap" kind (project rule, ADR 0010): a field nothing reads is wired into a
   calculation, retired through `crates/alas-config/src/retired_keys.rs`, or, when it
   is only a label or a deprecated compatibility field, listed as above.
4. **Every capability family has a smoke test**: a test named
   `capability_smoke_<family>_...`. The families are listed in
   `xtask/src/capmap/mod.rs` (`FAMILIES`).

## Limits of the scan

The scan reads syntax, not types and not run-time behaviour.

- A field is matched by name (and by full-path string literals such as
  `"/geometry/wing/span"`). A name shared by several configuration structs is marked
  `shared_name` in the map: one read of that name satisfies all of them.
- The `config_method` rule follows function names, so it can credit a read to a
  method that a same-named function elsewhere calls.
- Reads in test code, `tests/` directories and the acceptance and test-support
  crates never count as use.
- A field set by a preset and never read is reported as unread; presets are
  writes.

The scan establishes that a field is wired to something, not that the
calculation is physically right. It does not replace the plausibility tier.

## Smoke tests

| Family | Test | Crate |
|---|---|---|
| config | `capability_smoke_config_preset_loads_validates_and_round_trips` | alas-config |
| pipeline | `capability_smoke_pipeline_baseline_analysis_runs_for_a_preset` | alas-pipeline |
| report | `capability_smoke_report_figure_renders_and_exports_plot_data` | alas-report |
| tools | `capability_smoke_tools_preferences_persist_and_missing_tools_stay_absent` | alas-exec |
| gui | `capability_smoke_gui_pages_resolve_to_schema_groups` | alas-gui |
| cli | `capability_smoke_cli_flags_parse_into_run_options` | alas-app |
| addons | `capability_smoke_addons_example_installs_discovers_and_runs` | alas-addons |
| filetypes | `capability_smoke_filetypes_config_loads_from_json_and_yaml` | alas-app |

Run them all with the filter `test(capability_smoke)`:

```
cargo nextest run --workspace -E "test(capability_smoke)"
```

They belong to the default (fast) tier; keep each under a few seconds.
