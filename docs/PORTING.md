# Porting ledger

**This is a numerical-parity and licence-provenance record, not a project
status page.** For "does ALAS work today and what is wrong with it", read
`docs/STATUS.md` instead. The two used to answer the same question, when
every crate was a line-by-line translation; they no longer do, because
orchestration layers such as `alas-pipeline` and `alas-gui` were built
natively over already-parity-tested physics kernels rather than translated,
so their rows below correctly read `todo` for "never checked against a
Python counterpart" while the crates themselves build, run, and are exercised
end to end. A `todo` row here is not evidence the crate is unbuilt.

Every Python module in the reference implementation appears here exactly once,
with where it goes, what licence its content carries, and whether it has been
shown to agree with the original.

`cargo xtask gate` fails if a crate exists that no row mentions: code may not
appear without a statement of where it came from and what it must agree with.
The reverse is not checked, because rows naming crates that do not exist yet
are the normal state of a plan — most of them are `todo`, and they are the
schedule. This is a working document, not a report written afterwards.

**Reference:** `alas @ rust-port-baseline`
(the tag in the Python repository from which every fixture is generated).

---

## Status values

| Value | Meaning |
|---|---|
| `todo` | Not started. |
| `wip` | Being written. Nothing may depend on it. |
| `green` | Translated, and its parity test passes at the stated tier. |
| `native` | No Python counterpart; written for this implementation. |
| `dropped` | Deliberately not ported. The reason is stated in the row. |

A `deviation-candidate` note records upstream behaviour reproduced faithfully
that is believed to be wrong. Those are the improvement backlog; see
CONTRIBUTING.md. They are not fixed during translation.

## Tolerance tiers

Defined in `golden/tolerances.toml`. Summarized here because the table below
refers to them constantly.

| Tier | Bound | Applies to |
|---|---|---|
| `exact` | bitwise, or string equality | Statuses, integers, unit factors, generated input decks |
| `closed` | 1e-12 relative | Closed-form `f64` arithmetic |
| `linalg` | 1e-9 relative | Anything through a factorization, spline fit or least squares |
| `f32` | 1e-5 relative | The VORLAX kernel and the airfoil surrogate, which are `f32` upstream |
| `iter` | per-quantity table | Iteratively solved results: the mission, trim |
| `stat` | distributional | The design search and the screening ranking |

---

## Phase gates

A phase opens only when every row of the phases it depends on is `green`,
`native` or `dropped`.

| Phase | Contents | Opens after |
|---|---|---|
| P0 | Scaffolding, process docs, gate tooling, fixture framework | — |
| P1 | `alas-units`, `alas-math`, `alas-atmo`, `alas-i18n` | P0 |
| P2 | `alas-config` and the derive macro | P0 |
| P3 | `alas-geom` | P1, P2 |
| P4 | Closed-form physics: `alas-prop`, `alas-mass`, `alas-perf`, `alas-payload`, `alas-route`, analytical structures | P3 |
| P5 | Both vortex-lattice methods, the airfoil surrogate, the lift surrogate | P3 |
| P6 | `alas-mission` | P4, P5 |
| P7 | `alas-stab`, trim and the full analysis | P4, P5 |
| P8 | `alas-opt`, `alas-screen` | P7 |
| P9 | External solvers: MSES, Nastran, AVL | P4 |
| P10 | `alas-pipeline` and the headless command line. **First light.** | P6, P7, P8, P9 |
| P11 | Figures | P10 |
| P12 | The desktop interface | P11 |
| P13 | Packaging, the acceptance matrix, the benchmark harness | P12 |
| P14 | Model improvements, each config-gated and quantified | P13 |

`alas-stab` was listed under P4 as closed-form physics and has been moved to
P7. It is not closed-form: `alas/physics/stability.py`'s `static_margin` and
`autobalance` both build and run an `asb.VortexLatticeMethod` solve, and
`alas/physics/dynamics.py`'s `compute_dynamic_modes` does the same before
calling AeroSandbox's own `flight_dynamics.get_modes`. Those need
`alas-aero::asb_vlm`, a P5 row, so nothing in that crate could have been made
green while P4 was open without breaking the rule that nothing may depend on a
module that is not green. The phase table said P4 opened after P3 alone, which
was true of every other member and not of this one. `estimate_inertia` and the
`_xsec_width`/`_xsec_height` accessors are the standalone parts; the last two
are also wanted by `alas-payload::payload`, which reimplements them locally
rather than taking a dependency on a crate that cannot land yet.

---

## Core pipeline and orchestration

| Python module | Lines | Rust target | Provenance | Tier | Status |
|---|---:|---|---|---|---|
| `alas/pipeline.py` | 974 | `alas-pipeline` | — | `iter` | todo |
| `alas/analysis/full_analysis.py` | 247 | `alas-pipeline::full_analysis` | — | `iter` | todo |
| `alas/validation.py` | 123 | `alas-config::validation` | — | `exact` | green — `golden/config/validation.json` |
| `alas/cli.py` | 219 | `alas-app::cli` | — | — | todo |
| `alas/paths.py` | 220 | `alas-app::paths` | — | `exact` | todo |
| `alas/proc.py` | 76 | `alas-exec::process` | — | — | todo |
| `alas/i18n.py` | 90 | `alas-i18n` | — | — | green |
| `alas/__init__.py` | 36 | — | — | — | dropped: lazy-export shim, no native equivalent needed |
| — | — | `alas-fonts` | — | — | native: the bundled text face the desktop build draws with, which the Python implementation took from the host system |

## Configuration

Every module here becomes one module in `alas-config`, keeping the file
boundaries so a reviewer can compare them side by side.

| Python module | Lines | Rust target | Provenance | Tier | Status |
|---|---:|---|---|---|---|
| `alas/config/presets.py` | 673 | `alas-config::presets` | — | `exact` | green — `golden/config/aircraft_presets.json` |
| `alas/config/geometry_config.py` | 519 | `alas-config::geometry` | — | `exact` | green — `golden/config/defaults.json` |
| `alas/config/optimizer_config.py` | 461 | `alas-config::optimizer` | — | `exact` | green — `golden/config/defaults.json` |
| `alas/config/structures_config.py` | 344 | `alas-config::structures` | — | `exact` | green — `golden/config/defaults.json` |
| `alas/config/cabin_config.py` | 291 | `alas-config::cabin` | — | `exact` | green — `golden/config/defaults.json` |
| `alas/config/airports.py` | 271 | `alas-config::airports` | — | `exact` | green — `golden/config/` |
| `alas/config/requirements.py` | 232 | `alas-config::requirements` | — | `exact` | green — `golden/config/defaults.json` |
| `alas/config/design_variables.py` | 227 | `alas-config::design_variables` | — | `exact` | green — `golden/config/design_variables.json` |
| `alas/config/engines.py` | 189 | `alas-config::engines` | — | `exact` | green — `golden/config/` |
| `alas/config/materials.py` | 178 | `alas-config::materials` | — | `exact` | green — `golden/config/` |
| `alas/config/performance_config.py` | 175 | `alas-config::performance` | — | `exact` | green — `golden/config/defaults.json` |
| `alas/config/propulsion_config.py` | 175 | `alas-config::propulsion` | — | `exact` | green — `golden/config/defaults.json` |
| `alas/config/analysis_config.py` | 171 | `alas-config::analysis` | — | `exact` | green — `golden/config/defaults.json` |
| `alas/config/control_surfaces_config.py` | 154 | `alas-config::control_surfaces` | — | `exact` | green — `golden/config/defaults.json` |
| `alas/config/mass_config.py` | 151 | `alas-config::mass` | — | `exact` | green — `golden/config/defaults.json` |
| `alas/config/settings.py` | 149 | `alas-config::settings` | — | `exact` | green — `golden/config/settings.json` |
| `alas/config/mses_config.py` | 126 | `alas-config::mses` | — | `exact` | green — `golden/config/defaults.json` |
| `alas/config/mission_config.py` | 116 | `alas-config::mission` | — | `exact` | green — `golden/config/defaults.json` |
| `alas/config/landing_gear_config.py` | 113 | `alas-config::landing_gear` | — | `exact` | green — `golden/config/defaults.json` |
| `alas/config/performance_presets.py` | 108 | `alas-config::performance_presets` | — | `exact` | green — `golden/config/presets.json` |
| `alas/config/solver_presets.py` | 99 | `alas-config::solver_presets` | — | `exact` | green — `golden/config/presets.json` |
| `alas/config/fidelity_presets.py` | 76 | `alas-config::fidelity_presets` | — | `exact` | green — `golden/config/presets.json` |
| `alas/config/physics_config.py` | 74 | `alas-config::physics` | — | `exact` | green — `golden/config/defaults.json` |
| `alas/config/__init__.py` | 66 | `alas-config::lib` | — | — | green — re-export surface, checked by every parity test above compiling against it |
| — | — | `alas-config-derive` | — | — | native |

`alas-config::materials`, `::engines` and `::airports` change medium. Upstream
each is a few hundred lines of constructor calls registering immutable
records — a database written as code, because a dataclass constructor is the
shortest thing to hand in Python, not because anything in them is executable.
Here each is JSON under `crates/alas-config/data/`, embedded with
`include_str!` and parsed once, following the pattern `alas-i18n::es` already
set for its catalog. A table of published material properties stays reviewable
as a table, three files stay clear of the 500-line source limit they would
otherwise dominate, and correcting a published figure is a data change rather
than a code change. The embedded copies are deliberately separate from the
`golden/` fixtures — a shipped binary must not need `golden/` on disk — and
`tests/parity_databases.rs` is what stops the two drifting.

That test compares parsed values rather than the raw documents, because a
field elevation is a Python `int` and an `f64` here: `25` against `25.0` is a
difference in the text and not in the data. The one computed part of these
tables, `EngineSpec::nacelle_profile`, is compared separately at `closed`,
since its stations are derived from the nacelle length rather than stored.

`alas-config-derive` replaces `alas/sidecar/schema.py`, which walked a
dataclass at run time to build the form description. Doing it at expansion
time instead means a field that says nothing about itself does not compile,
which is the rule CONTRIBUTING.md states and which the reference had no way to
enforce. A good many of the reference's fields are in exactly that state:
the whole of `MissionProfileConfig`, most of `PropulsionCycleConfig`, and
this port supplies their explanations. That adds prose and changes no value,
and the parity test is written accordingly: `label` is compared always, including
where it was derived from the field name, and `help` only where the dataclass
declared one.

Two further differences at this boundary, both deliberate and neither
numerical. The reference translates every label and help string inside
`dataclass_schema`, because that function is its HTTP boundary and the
language belongs to the request; here the schema carries the canonical
English and `Node::translated` applies a language where something is about to
show it to a person, which is the arrangement `alas-i18n` already documents.
And the reference resolves the accepted values of the handful of string
fields that have them by importing the module that owns each list; those
modules sit above this crate in the layering, so a field names its list
through `OptionSource` and a crate that can see both resolves it. The parity
test checks the lists this crate can resolve and defers the rest to where the
resolution happens.

The three named-preset registries (`::solver_presets`,
`::performance_presets`, `::fidelity_presets`) stay code rather than
becoming data, unlike the three tables above. Each entry is one configuration
struct with a handful of fields overridden, so as data it would be a document
that had to repeat every field it did not change or invent a patch format;
as code it is `..Default::default()`, and the compiler checks the field names.
`golden/config/presets.json` records each preset's *whole* resulting
configuration rather than the fields its constructor named, because a preset
is defined as much by what it leaves alone as by what it sets, and one that
overreached would agree everywhere it meant to and silently reset something
else.

`deviation-candidate` at that boundary: `fidelity_presets` states that it is
scoped to three fields of `AnalysisConfig` precisely so that asking for a
finer mesh cannot clobber an assumption the user has tuned, and then hands
its consumer the whole struct, which clobbers them. The two are consistent
today only because every preset leaves the other sixteen fields at their
defaults, which `parity_presets.rs` and a unit test both check. The port
reproduces what the registry holds and does not decide the question; whoever
writes the consumer does.

`alas-config::geometry` hides its engine group from the generated form, as
upstream does: the engine has a dedicated editor, and two forms writing the
same fields is how the two come to disagree. That leaves thirteen fields no
type in the fixture would otherwise describe, so `EngineConfig` is captured as
a fixture entry of its own rather than left unverified: a label or a unit
ported wrong there produces a wrong engine as readily as one anywhere else.

Two findings from that group, both recorded rather than corrected. Its
defaults are the GE9X entry of the engine table written out longhand, because
a bare `EngineConfig` exists before anything has selected an engine and has to
answer a request for thrust with a real number. The written-out nacelle
silhouette is the same shape as the one selecting GE9X produces, rounded to a
decimetre: the table's stations are fractions of the nacelle length (0.624 m,
1.17 m) and the fallback carries them rounded (0.6 m, 1.2 m). Same overall
length, same radius fractions, so the two draw the same nacelle; a unit test
states the bound rather than asserting an equality that does not hold.
`deviation-candidate`: the fallback should be the computed profile.

Second, the engine's cycle data lives in the design rather than being looked
up from the table each time a discipline needs it. `engine_name` is a
selector: choosing one copies the entry's values in once, and mass estimation,
the mission, the matching chart and the cycle analysis all read the copy. A
look-up cannot see an edit, so the alternative is an engine that has been
modified for some consumers and not others.

`alas-config::optimizer` keeps upstream's split between what the search is
looking for and how hard it looks, as two nested groups of one
`OptimizerConfig`. Its `ObjectiveWeights` is where the reference's rule for
offering a positive real as a ratio slider actually bites: the rule keys off
the field's name ending in `_scale`, `_per_m`, `_weight`, `_floor`, `_cost` or
`_floor_m`, and four of the fields it catches (`thickness_floor`,
`fuselage_floor_m`, `failure_cost` and `instability_failure_cost`) are
physical thresholds and flat costs rather than relative weights, so a slider
whose position means nothing on its own is the wrong control for them.
Reproduced rather than corrected; `deviation-candidate`.

`alas-config::presets` is the one registry here whose entries are whole
aircraft rather than overrides on one struct, so its fixture records every
field of all seven rather than the ones its constructors named — several
hundred dimensions read off published specification sheets, where a transposed
digit produces an aeroplane that flies and is not the one on the sheet. Two
things a preset deliberately does not settle are reproduced rather than tidied
up. It names its engine without copying the table entry in, so every preset
carries `EngineConfig`'s GE9X fallback cycle until the geometry builder calls
`apply_engine_spec`; resolving it at registration would make a preset disagree
with the same preset loaded from a saved file. And a preset does not fit the
design space it is offered in — the bounds in `::design_variables` are one
global set describing AVE's family, so an A320's fuselage is twenty-eight
metres shorter than the shortest the search will consider. Upstream
acknowledges this and expects the caller to narrow the bounds around whatever
design it starts from. Both are stated as unit tests, since both look like
defects.

`alas-config::settings` keeps the dictionary representation and not the two
file codecs. Upstream's YAML and JSON paths are, by its own comment, thin
wrappers over one `to_dict`/`from_dict` pair rather than two independently
maintained serializers; that pair is what lives here, as `serde_json::Value`.
Opening a path is the concern of the crate that owns paths, and a
serialization-format dependency in the configuration model would be a
dependency of every crate that reads a setting. `alas-app` picks the YAML
crate when it needs one.

`alas-config::validation` is why this crate depends on `alas-atmo`. Its cruise
rule converts the design point to an equivalent airspeed before comparing it
with the certification dive speed, which needs the density and the speed of
sound at altitude; `alas-atmo` is a P1 leaf with no dependencies of its own,
and P1 was green before this edge was added. Upstream evaluates that
atmosphere through AeroSandbox's fitted default rather than the closed form,
and the two agree to about 1e-11 — well inside the whole metre per second the
rule renders. The fixture's cases are chosen away from the VC and VD
thresholds so that no verdict is decided by that last digit. One further
difference, non-numerical: upstream wraps each rule in a bare `except` because
it runs on every keystroke of a debounced preview and a field caught mid-edit
could break a unit conversion. A rule here reads typed fields and has nothing
to raise, so there is no equivalent.

`alas-config-derive` is the only member that depends on `syn`, `quote` and
`proc-macro2`. A derive macro receives its input as a token stream and has to
parse Rust syntax to do anything at all; there is no standard-library
facility for that, and no alternative to those three that is not a fork of
them.

## Geometry

| Python module | Lines | Rust target | Provenance | Tier | Status |
|---|---:|---|---|---|---|
| `alas/geometry/wing_mesh_bdf.py` | 681 | `alas-struct::mesh` | — | `exact` | todo |
| `alas/geometry/wing_structure.py` | 435 | `alas-geom::wing_structure` | — | `closed` | green — `golden/geom/wing_structure.json` |
| `alas/geometry/aircraft_builder.py` | 253 | `alas-geom::builder` | — | `closed` | green — `golden/geom/builder.json` |
| `alas/geometry/airfoils.py` | 247 | `alas-geom::airfoil_library` | — | `linalg` | green — `golden/geom/airfoil_library.json` |
| `alas/data/airfoil_data.py` | 232 | `alas-geom::airfoil_data` | — | `exact` | green — `golden/geom/airfoil_data.json` |
| — | — | `alas-geom::asb::airfoil` | AeroSandbox, MIT | `linalg` | green — `golden/geom/asb_airfoil.json` |
| — | — | `alas-geom::asb::wing` | AeroSandbox, MIT | `closed` | green — `golden/geom/asb_wing.json` |
| — | — | `alas-geom::asb::fuselage` | AeroSandbox, MIT | `closed` | green — `golden/geom/asb_fuselage.json` |
| — | — | `alas-geom::asb::airplane` | AeroSandbox, MIT | `closed` | green — `golden/geom/builder.json` |
| — | — | `alas-geom::asb::mesh` | AeroSandbox, MIT | `closed` | todo |
| — | — | `alas-geom::selig` | UIUC, see notices | `exact` | green — `golden/geom/selig.json` |

`alas-geom::asb::airfoil` is scoped to what this program's Python package
actually calls onto AeroSandbox's `Airfoil`: construction from explicit
coordinates or a 4-digit NACA name, the upper/lower surface split, cosine-spaced
`repanel` through a cubic spline, and `local_thickness`/`max_thickness`. Left
untranslated: polar/XFoil/NeuralFoil generation, Kulfan parameterization,
plotting, `normalize`, `scale`/`translate`, `LE_radius`, `TE_angle`,
`TE_thickness` — a prior grep of every call site onto an `Airfoil` instance
found none of these reached.

Upstream's name-resolution fallback (`Airfoil(name, coordinates=None)`) tries
three sources in order: the closed-form 4-digit NACA generator, then the UIUC
database, then a `.dat` file on disk. Every name this program's own
configuration resolves through that fallback is `"naca0012"` (checked against
`alas/config/presets.py`), which only ever reaches the first branch, so
`Airfoil::from_name` implements only that one and returns `None` for anything
that does not parse as a 4-digit NACA designation — the same outcome upstream's
`except (ValueError, NotImplementedError):` produces on that branch, without
then trying the UIUC lookup or a file read. This is a documented boundary, not
a `deviation-candidate`: both unreached branches are absent because nothing
in this program's inputs takes them, not because either was reproduced
wrongly. A future caller that needs an arbitrary named or file-backed airfoil
should reach for `alas-geom::selig` (the UIUC corpus this program already
embeds) or extend this module deliberately, rather than assume the fallback
silently continues past NACA.

`alas-geom::asb::wing` reproduces `aerodynamic_center` without rotating the
chordwise offset by section twist, matching an acknowledged upstream omission.
`deviation-candidate`.

`alas-geom::asb::wing`'s `mean_sweep_angle` and `control_surface_area`, and
`alas-geom::asb::fuselage`'s `area_wetted`, were added when `alas-mass::torenbeek`
(below) reached them; this row's scope grew to match, and its fixture is
unchanged since the geometry it already covers exercises the same wings and
fuselage these methods run on.

`alas-geom::asb::mesh` stays `todo` and does not block phase P4 opening. A
prior grep of the whole `alas/` package found exactly two AeroSandbox meshing
entry points this program ever calls onto a `Wing`/`Fuselage`:
`mesh_thin_surface` (bucketing a VLM run's own output for a span-loading
plot) and `draw_wireframe` (3D preview), and both call sites live in
`alas/reporting/visualization.py` and `alas/sidecar/figures.py`, i.e. P11
(Figures), which opens only after P10. Nothing in P4 through P9 reaches this
row, so it is left `todo` here rather than pulled forward; whoever starts
P11's figure families should translate it then, against real VLM/wireframe
output, instead of against synthetic geometry now.

`alas-geom::asb::airplane` is scoped to the one call site that constructs an
`Airplane`: `alas-geom::builder`'s `AircraftBuilder.build`, which always
supplies `name`, `xyz_ref`, `wings`, `fuselages`, `s_ref`, `c_ref` and `b_ref`
explicitly. Left untranslated: the `propulsors` field (this program's engines
are `Fuselage`-shaped nacelles, never a `Propulsor`), `analysis_specific_options`,
and the constructor's fallback that derives `s_ref`/`c_ref`/`b_ref` from
`wings[0]` when they are not supplied — `AircraftBuilder.build` never omits
any of the three.

`alas-geom::selig` is the third table in this port to change medium, and the
one where the choice was closest. Upstream reads its 1,665 coordinate sets out
of `alas/data/coord_seligFmt.zip` — 1.07 MB deflated, 2.52 MB raw — through
Python's `zipfile`. Here the archive is unpacked into one embedded text corpus,
`crates/alas-geom/data/selig.txt`, at a cost of 1.45 MB of binary and no new
dependency. Reading a zip in Rust means a decompressor and its supporting
crates: half a dozen entries in the supply chain, and a licence to audit for
each, bought so that static data can be stored smaller than it is used.
`THIRD-PARTY-NOTICES.md` already described the bundled database as repacked
into a single blob, so this records the shape rather than choosing it.

The corpus stores each entry's bytes verbatim, behind a delimiter line `@` plus
the zip entry's stem, including the name line the parser then skips. Two
properties of the archive make that safe and were checked rather than assumed:
every entry is ASCII, and no line in any of them begins with `@`. Storing the
raw text and not parsed coordinates is deliberate — upstream's reader drops any
line whose first two fields will not parse as floats, and pre-digesting the
corpus into numbers would retire that behaviour to a Python script instead of
translating it. The archive also has no two entries whose stems collide under
`to_lowercase`, so the name index has no ordering to reproduce.

The `golden/` fixture does not carry a second copy of 2.52 MB. It records a
per-entry digest manifest, which is what stops the corpus drifting from the
archive, and full resolved coordinates for a sample plus every airfoil a
configuration can name.

That last set is the reason this row is not deferrable to `alas-screen`, where
an arbitrary-airfoil sweep would otherwise be its only consumer. The default
aircraft's three sections each resolve through a different branch of
`AirfoilLibrary.get`, in its order: `naca2410` (the wing tip) is *in* the
archive and is read from it; `SC2-0714` (the root) is not, and falls to the
built-in `NAMED_COORDINATES`; `naca0012` (the tail) is not either — the archive
spells it `n0012` — and falls through to AeroSandbox's NACA generator. So
`alas-geom::builder` could not reach parity on its nominal case until all
three branches existed — which they now do, and `golden/geom/builder.json`
is the fixture that exercises all three together on the actual default
aircraft rather than on the three branches in isolation.

## Aerodynamics

| Python module | Lines | Rust target | Provenance | Tier | Status |
|---|---:|---|---|---|---|
| `alas/physics/aerodynamics.py` | 328 | `alas-aero::analysis` | — | `linalg` | todo |
| `alas/physics/mses_analysis.py` | 389 | `alas-aero::mses` | — | `exact` | todo |
| — | — | `alas-aero::asb_vlm` | AeroSandbox, MIT | `linalg` | todo |
| — | — | `alas-aero::neuralfoil` | NeuralFoil, MIT | `f32` | todo |
| — | — | `alas-aero::kulfan` | AeroSandbox, MIT | `linalg` | todo |
| — | — | `alas-aero::vorlax` | SUAVE, LGPL-2.1 | `f32` | todo |
| — | — | `alas-aero::drag_buildup` | SUAVE, LGPL-2.1 | `closed` | todo |
| — | — | `alas-aero::lift_surrogate` | SUAVE, LGPL-2.1 | `linalg` | todo |
| — | — | `alas-aero::operating_point` | AeroSandbox, MIT | `closed` | todo |

The VORLAX kernel is `f32` upstream and is reproduced in `f32`.
`deviation-candidate`: an `f64` path is expected to be more accurate and is a
P14 study, not a translation decision.

## Propulsion, mass, stability, performance, payload

| Python module | Lines | Rust target | Provenance | Tier | Status |
|---|---:|---|---|---|---|
| `alas/physics/propulsion.py` | 488 | `alas-prop::cycle` | — | `closed` | green — `golden/prop/cycle.json` |
| `alas/physics/mass.py` | 288 | `alas-mass::breakdown` | — | `closed` | green — `golden/mass/breakdown.json` |
| `alas/physics/stability.py` | 347 | `alas-stab::trim` | — | `linalg` | todo — P7, not P4; needs `alas-aero::asb_vlm` |
| `alas/physics/dynamics.py` | 96 | `alas-stab::dynamics` | — | `closed` | todo — P7, not P4; needs `alas-aero::asb_vlm` |
| `alas/physics/performance.py` | 555 | `alas-perf::performance` | — | `closed` | green — `golden/perf/performance.json` (point-performance surface; see scope note) |
| `alas/physics/landing_gear.py` | 298 | `alas-perf::landing_gear` | — | `closed` | green — `golden/perf/landing_gear.json` |
| `alas/physics/payload.py` | 540 | `alas-payload::{geometry,layout,oew}` | — | `closed` | green — `golden/payload/layout.json`; see the split below |
| `alas/physics/cabin_layout.py` | 693 | `alas-payload::cabin` | — | `exact` | green — `golden/payload/layout.json` (item sequence and counts at `exact`, positions and masses at `closed`; the compatibility interior replays the frozen premium-economy slot) |
| `alas/physics/cargo_loader.py` | 381 | `alas-payload::cargo` | — | `exact` | green — `golden/payload/layout.json` (frozen hold grid keeps its loose bulk position; the product envelope path fit-checks it) |
| — | — | `alas-mass::torenbeek` | AeroSandbox, MIT | `closed` | green — `golden/mass/torenbeek.json` |
| — | — | `alas-mass::suave_transport` | SUAVE, LGPL-2.1 | `closed` | todo |
| — | — | `alas-stab::modes` | AeroSandbox, MIT | `closed` | todo |
| — | — | `alas-stab::suave_static` | SUAVE, LGPL-2.1 | `closed` | todo |
| — | — | `alas-prop::suave_turbofan` | SUAVE, LGPL-2.1 | `closed` | todo |

`alas-mass::torenbeek` is scoped to the two entry points `alas/physics/mass.py`
(not yet ported) calls: `mass_wing`, which itself composes three private
helpers, `mass_wing_high_lift_devices`, `mass_wing_basic_structure` and
`mass_wing_spoilers_and_speedbrakes`, all translated as part of that
computation, and `mass_fuselage_simple`. A grep of the whole `alas/`
package, not only `mass.py`, found no other caller. Left untranslated:
`mass_wing_simple` (a cruder wing weight model, superseded everywhere by the
Appendix C method this module implements), `mass_fuselage` (dead code
upstream; it raises `NotImplementedError` partway through, after
referencing `S_g`, `W_str` and `W_fr`, none of which it ever assigns; not a
`deviation-candidate`, since there is no behaviour to reproduce from code that
cannot run), and `mass_propeller` (unused; this program's engines are
turbofans). `mass_wing` and `mass_wing_basic_structure`'s `return_dict: bool`
is narrowed to the `float`-returning path, since `mass.py` never passes
`return_dict=True` at either of its call sites and `Union[float, Dict]` has no
natural Rust type; `mass_wing_basic_structure`'s `strut_y_location` stays a
real `Option<f64>` even though `mass.py` never passes it non-`None`, since
Torenbeek Eq. C-5's branch on it is physically meaningful.

This row also adds `Wing::mean_sweep_angle`, `Wing::control_surface_area` and
`Fuselage::area_wetted` to `alas-geom::asb`, all reached by this module and
previously out of that row's scope. `control_surface_area` always returns
`0.0`: this crate's `WingXSec` carries no `control_surfaces` field at all
(`alas-geom::asb::wing`'s own module doc), so upstream's summing loop is
always empty on every wing this program builds: confirmed against the
fixture, where every AeroSandbox-side `mass_wing_high_lift_devices` case
also computes zero for the same reason.

`alas/physics/payload.py` is `wip` rather than `green` because it is three
things and only two of them have landed. `alas-payload::geometry`
(`DeckSpec`, `CabinGeometry`) and `alas-payload::oew` (`oew_and_cg`) are
translated and compared against `golden/payload/layout.json` by
`tests/parity_geometry.rs`: the deck table at `exact`, since those fractions
are transcribed constants, and every sampler at `closed`.
`alas-payload::layout` (`DeckItem`, `PayloadLayout`) is the vocabulary both
engines produce and carries unit tests but no parity of its own, since nothing
constructs one yet. Still `todo` in that file: `build_payload_layout`'s
dispatcher, `simulate_passenger_counts` and `apply_cabin_preset`, all three of
which need one or both layout engines. **Nothing may depend on this row until
it is `green`**, which needs the two `todo` rows above it as well.

The fixture is complete ahead of the code, deliberately. `golden/payload/layout.json`
already records what the reference produces for all three modules: the cabin
frame on four fuselages, the *entire item sequence and summary* of seventeen
passenger and freighter layouts, `simulate_passenger_counts` across the shipped
class mixes, and every branch of `apply_cabin_preset`. The item list is
recorded in placement order because a layout is a sequence, two
implementations that place the same items in a different order have not agreed,
and the cases were chosen to reach the branches that are invisible from the
totals: the exit-derived capacity ceiling binding before the floor does, the
monument count exceeding the bay count so `_stack_y` narrows rather than
overlaps, the bulk-overflow guard, all four cargo loading strategies, and a
narrowbody hold too shallow for an LD3 so the loader degrades through
`LOWER_HOLD_FALLBACKS`.

Two decisions this row has already required. `oew_and_cg` takes
`alas-mass`'s typed `MassBreakdown`/`MassCoordinates` rather than upstream's
two `Dict`s, which makes upstream's "a component has a mass and no coordinate"
branch unreachable by construction instead of incidentally: all three callers
pass `calculate_component_masses` and `define_mass_coordinates` together and
those always populate the same ten names. The negative-mass guard is kept,
because that one *is* reachable: an empirical weight correlation on a
degenerate candidate can go below zero, and the optimizer evaluates those.
And `alas-payload::numeric` reproduces CPython's float `//` and `round` and
NumPy's `interp` from those implementations' own sources rather than from
their documented behaviour. This is not fastidiousness: `int((usable -
aisle_w) // seat_w)` is how many people sit in a row, and CPython's floor
division answers 9 where `(a / b).floor()` answers 10 on inputs as ordinary as
`1.0 // 0.1`. It is private to the crate; if a second crate needs it, it moves
to `alas-math` rather than being copied.

`alas-stab::modes` reproduces closed-form mode approximations whose author
records a factor-of-two error in the phugoid root against AVL.
`deviation-candidate`: replace with a state-space eigensolve, validated against
AVL, in P14.

`alas-stab::suave_static` computes the static margin against wing origins
because the upstream aerodynamic centre is left at the origin.
`deviation-candidate`.

`alas-perf::performance` is green for the point-performance surface every
input of which already exists in P4: `density_ratio`, the four matching-chart
constraint curves, `build_matching_chart`, the FAR-25 V-speed schedule
(`compute_v_speeds`), `compute_field_performance`, `breguet_range_m` and
`build_vn_diagram`. All agree at `closed`: the `asb.Atmosphere(...)` calls
map to `Atmosphere::new`, the same fitted model `alas-prop::cycle` already
rides at this tier, and everything else is algebra over its result. Two
faithful-translation details are recorded in the code: `density_ratio` uses a
hardcoded `287.05` gas constant and applies the ISA offset by hand rather than
reaching for `Atmosphere::density`, and `build_vn_diagram` reports speeds in a
six-figure knots factor (`1.943844`) distinct from the module's five-figure
`_MS_TO_KT`. `build_vn_diagram` is scoped to take `s_ref` directly, the only
field it reads off its airplane argument, so this row needs no `alas-geom`
edge.

Four functions of `performance.py` are deferred rather than ported here, none
reachable from a P4 consumer. `wing_fuel_volume_m3` reads a built
`alas-geom`-side `Wing`, an edge this crate does not yet take;
`payload_range_diagram` and `fuel_volume_check` both orchestrate a
full-analysis `report` object (`component_masses`, `airplane`,
`trimmed_design_point`) that only exists in P10; and `static_thrust_to_weight`
is an `ALASConfig` accessor with a bare-`except` fallback, an app-layer
concern. The first three land with the `alas-geom` edge and the `DesignReport`
type in P10; the fourth belongs to `alas-app`. This is a documented scope
boundary, not a `deviation-candidate`: the deferred functions are absent
because nothing in P4 through P9 calls them, not reproduced wrongly.

## Mission

| Python module | Lines | Rust target | Provenance | Tier | Status |
|---|---:|---|---|---|---|
| `alas/integration/suave_bridge.py` | 272 | — | — | — | dropped: the subprocess boundary disappears with the translation |
| `alas/integration/suave_vehicle.py` | 96 | `alas-mission::vehicle` | — | `closed` | green — `golden/mission/vehicle.json`; source corrections pinned two-sidedly |
| `alas/integration/suave_mission.py` | 38 | `alas-mission::profile` | — | `closed` | green — `golden/mission/profile.json`; replays the recorded baseline TAS profile |
| — | — | `alas-mission::segments` | SUAVE, LGPL-2.1 | `iter` | todo |
| — | — | `alas-mission::numerics` | SUAVE, LGPL-2.1 | `linalg` | todo |
| — | — | `alas-mission::solve` | SUAVE, LGPL-2.1 | `iter` | todo |
| — | — | `alas-math::hybrd` | MINPACK, public domain | `linalg` | todo |

The six mission configurations differ only in high-lift deflections, which the
upstream aerodynamic model does not discretize, so all six evaluate identically.
The lift surrogate is therefore built once rather than six times. This changes
run time and not results; it is recorded here because a reader comparing the two
implementations will notice the difference.

Dropped upstream branches, each unreachable from this program's inputs:
supersonic influence kernels; control-surface panelization; the segmented-wing
weight integral; the dynamic-stability branch, which is gated on a moment-of-
inertia tensor that is never populated.

## Structures

| Python module | Lines | Rust target | Provenance | Tier | Status |
|---|---:|---|---|---|---|
| `alas/integration/nastran_runner.py` | 737 | `alas-struct::nastran` | — | `exact` | todo |
| `alas/physics/structural_sizing.py` | 222 | `alas-struct::sizing` | — | `closed` | green — `golden/struct/sizing.json` |
| `alas/physics/structural_analysis.py` | 215 | `alas-struct::analytical` | — | `closed` | green — `golden/struct/analytical.json` |
| `alas/physics/structural_loads.py` | 109 | `alas-struct::loads` | — | `closed` | green — `golden/struct/loads.json` |
| `alas/integration/_nastran_compat.py` | 32 | — | — | — | dropped: a numpy 2.x shim for pyNastran |
| `alas/integration/patran_runner.py` | 236 | — | — | — | dropped: rendered images only; drawn natively from the displacements |
| — | — | `alas-struct::op2` | — | `exact` | native |
| — | — | `alas-struct::nastran95` | — | `exact` | native |

`alas-struct::nastran95` targets an open-source solver that predates several
cards the mesh uses: `RBE3`, `PBARL` and `EIGRL` need reformulating, and modal
frequency response has no equivalent. Statics and normal modes are in scope;
vibration is not, and reports itself unavailable.

## Optimization and screening

| Python module | Lines | Rust target | Provenance | Tier | Status |
|---|---:|---|---|---|---|
| `alas/optimization/objective.py` | 638 | `alas-opt::objective` | — | `closed` | todo |
| `alas/optimization/sampling.py` | 156 | `alas-opt::sampling` | — | `stat` | todo |
| `alas/optimization/optimizer.py` | 143 | `alas-opt::differential_evolution` | — | `stat` | todo |
| `alas/analysis/airfoil_screening.py` | 970 | `alas-screen` | — | `stat` | todo |

## Routing

| Python module | Lines | Rust target | Provenance | Tier | Status |
|---|---:|---|---|---|---|
| `alas/routing/navdata_graph.py` | 251 | `alas-route::navdata` | — | `closed` | todo |
| `alas/routing/simbrief_route.py` | 175 | `alas-route::simbrief` | — | `exact` | todo |
| `alas/routing/route.py` | 169 | `alas-route::route` | — | `closed` | todo |
| `alas/routing/kml_import.py` | 33 | `alas-route::kml` | — | `exact` | todo |
| `alas/integration/assets.py` | 126 | `alas-route::assets` | — | — | todo |
| `alas/reporting/route_globe.py` | 186 | `alas-report::route_geometry` | — | `closed` | todo |

## Reporting

`visualization.py` is 5,596 lines and does not survive as one module. It is
split by figure family, none over the file-size limit. The families are
enumerated in `docs/FIGURE_REVIEW.md`, which also carries the per-figure review
sign-off.

| Python module | Lines | Rust target | Provenance | Tier | Status |
|---|---:|---|---|---|---|
| `alas/reporting/visualization.py` | 5,596 | `alas-report::families::*` | — | `closed` | todo |
| `alas/sidecar/figures_extra.py` | 567 | `alas-report::families::field_performance` | — | `closed` | todo |
| `alas/sidecar/figures.py` | 496 | `alas-report::registry` | — | — | todo |
| `alas/reporting/airfoil_sweep_figures.py` | 405 | `alas-report::families::screening` | — | `closed` | todo |
| `alas/reporting/design_report.py` | 137 | `alas-report::document` | — | — | todo |
| `alas/reporting/theme.py` | 84 | `alas-report::theme` | — | `exact` | todo |
| — | — | `alas-report::scene` | — | — | native |
| — | — | `alas-report::svg` | — | — | native |
| — | — | `alas-viz` | — | — | native |
| — | — | `alas-gui` | — | — | native |

## Translations

| Python module | Lines | Rust target | Provenance | Tier | Status |
|---|---:|---|---|---|---|
| `alas/translations/es.py` | 1,124 | `alas-i18n::es` | — | `exact` | green — `golden/i18n/es_catalog.json` |

## Dropped: the HTTP sidecar

The desktop interface calls the library directly, so the transport layer and
everything that existed to serve it has no counterpart. Run management and the
figure registry survive as library concerns, listed above.

| Python module | Lines | Reason |
|---|---:|---|
| `alas/sidecar/routes_figures.py` | 432 | HTTP transport |
| `alas/sidecar/routes_airfoil_sweep.py` | 254 | HTTP transport |
| `alas/sidecar/runs.py` | 211 | Run persistence moves into `alas-pipeline` |
| `alas/sidecar/schema.py` | 195 | The settings schema is generated by the derive macro |
| `alas/sidecar/routes_maintenance.py` | 186 | HTTP transport |
| `alas/sidecar/routes_config.py` | 181 | HTTP transport |
| `alas/sidecar/server.py` | 141 | The server itself |
| `alas/sidecar/airfoil_sweep_runs.py` | 125 | Run persistence moves into `alas-screen` |
| `alas/sidecar/routes_pipeline.py` | 123 | HTTP transport |
| `alas/sidecar/routes_assets.py` | 71 | HTTP transport |
| `alas/sidecar/routes_validate.py` | 28 | HTTP transport |
| `alas/sidecar/lazy_imports.py` | 27 | Deferred an import cost that no longer exists |

## Foundations with no Python counterpart

| Rust target | Provenance | Tier | Status |
|---|---|---|---|
| `alas-types` | — | `exact` | green |
| `alas-units` | SUAVE, LGPL-2.1 | `closed` | green — `golden/units/factors.json` |
| `alas-math::spline` | — | `linalg` | green — `golden/math/spline.json` |
| `alas-math::chebyshev` | SUAVE, LGPL-2.1 | `linalg` | green — `golden/math/chebyshev.json` |
| `alas-math::bicubic` | — | `linalg` | green — `golden/math/bicubic.json` |
| `alas-math::bspline` | — | `linalg` | green — `golden/math/bspline.json` |
| `alas-atmo::isa` | AeroSandbox, MIT | `closed` | green — `golden/atmo/isa.json` |
| `alas-atmo::differentiable` | AeroSandbox, MIT | `linalg` | green — `golden/atmo/differentiable.json` |
| `alas-atmo::atmosphere` | AeroSandbox, MIT | `linalg` | green — `golden/atmo/differentiable.json`, `golden/atmo/isa.json` |
| `alas-atmo::us1976` | SUAVE, LGPL-2.1 | `closed` | green — `golden/atmo/us1976.json` |
| `alas-cfd` | OpenCFD OpenFOAM v2606 and Gmsh 4.15.2; no Python counterpart | `native` | native — reusable two-dimensional airfoil study contract, Gmsh extrusion, OpenFOAM lifecycle, parsers and surface results |
| `alas-exec` | — | — | todo |
| `alas-testkit` | — | — | native |
| `alas-acceptance` | — | — | todo |
| `alas-uav` | — | — | native |
| `xtask` | — | — | native |

`alas-math::bicubic` reproduces `scipy.interpolate.RectBivariateSpline` at its
defaults, which is how every SUAVE aerodynamic surrogate is built. Its fixture
records the knot vectors as well as the values, and the knots are compared at
`exact` while the row's `linalg` tier applies to the coefficients and the
evaluated surface. The reason is that the knot placement is a choice, not a
computation: a bicubic through the same grid with the knots one data point
over still reproduces every grid value and disagrees everywhere between them,
which is most of where the mission samples. Comparing values alone would not
have distinguished the two.

`alas-atmo::differentiable` is the most consequential row in this table, and
it was opened after `alas-prop::cycle` stalled against it. **AeroSandbox's
`Atmosphere` defaults to a fitted model, not to the ISA.** Writing
`asb.Atmosphere(altitude=...)` with no `method` argument selects
`"differentiable"`, a cubic B-spline interpolating the ISA at thirty-eight
altitudes, and that is what `alas/physics/propulsion.py`,
`alas/physics/performance.py`, `alas/physics/aerodynamics.py`,
`alas/physics/stability.py` and `alas/analysis/full_analysis.py` all
construct. It is not a refinement of the closed form: over 0-25 km it
disagrees with the ISA by up to 1.1% in temperature, 0.4% in density and
0.6% in the speed of sound. Substituting `alas-atmo::isa` wherever upstream
wrote the default, which is what a reader who knew only that this program
uses "the standard atmosphere" would do: puts every P4 through P7 result
nine orders of magnitude outside its own tier before any physics happens.

Reproducing it needed a spline this project did not have. `alas-math::spline`
is SciPy's `CubicSpline` construction, which takes an explicit derivative
condition at each end; what CasADi's `interpolant(..., "bspline", ...)` builds
is the *not-a-knot* interpolant, and the two differ by a per-cent-scale amount
near the ends of the data. `alas-math::bspline` is that second object. Nothing
in it is translated from CasADi: in the one-dimensional cubic case the
not-a-knot interpolant is uniquely determined by the data, so the module
solves the collocation system directly and the fixture records CasADi's own
output as the check. That the knot rule turned out to be the same one
`alas-math::bicubic` already implements for FITPACK is why the two modules
share their knot, basis and solve primitives rather than carrying two copies;
they differ only outside the data range, where the bicubic surface clamps and
this one returns NaN (upstream's `fill_value=np.nan`, reaching through
`InterpolatedModel` and `interpn`). Both rows' fixtures guard the shared code.

Three judgements this row required, recorded because each is the kind that
gets re-argued later:

*The tier is `linalg`, not `closed`.* Every case in the fixture in fact agrees
to better than 1e-12, which is what keeps `closed` reachable for the
disciplines built on top of it, but a tier states what the construction is,
and this one is a spline fit through a solve, which is the case the tier table
names outright. A grid spanning seven million metres is not a place to bet on
the last two digits surviving another platform's `pow`.

*The altitude grid is compared at two tiers.* It is computed here from
upstream's construction (a hand-picked list plus two geometric fans) rather
than transcribed as thirty-eight literals, so that what it is stays legible.
Thirty-seven of the thirty-eight are bit-identical to NumPy's. One:
418,445.4 m, a knot 400 km up that exists only to keep an optimizer's
gradients finite: lands one ulp away, because `10**x` is evaluated by two
different libm implementations. The altitudes upstream *assigns* (the
hand-picked list, and each fan's endpoints, which `geomspace` overwrites after
its logarithmic pass) are still compared at `exact`; the interior fan points
are compared at `closed`. This is a tier correction and not a loosening:
`exact` is defined for values that are copied rather than computed, and a
transcendental function's last bit is not a copied value. The resulting
perturbation of the fit is 1e-16 relative.

*`Atmosphere::new` means the fit, and `Atmosphere::isa` means the closed form.*
The default follows upstream's default, so that a call site naming no method
reproduces one that named no method. This changed `alas-config::validation`,
whose two call sites were already written as `Atmosphere::new`: that module
now evaluates the same model its Python counterpart does, and the note above
about the two agreeing "to about 1e-11" describes an approximation that is no
longer being made. Its fixture's verdicts are unchanged, which
`parity_validation.rs` confirms. `alas-atmo` gains a dependency on
`alas-math`; both are P1 leaves and `alas-math` depends on nothing in the
workspace, so the edge introduces no layering.

`alas-units` is compared at `closed` rather than `exact`, which its row would
otherwise call for. Its values are written as the exact legal definitions:
the international yard and pound, standard gravity; while SUAVE reaches
several of the same quantities by division: its inch is a twelfth of its foot,
which lands one bit away from 0.0254. Every factor agrees to within two ulps.

Reproducing the artifact would mean writing `0.025400000000000002` into a
constant that claims to define an inch, and the deviation is a hundred times
smaller than any quantity it feeds. `deviation-candidate` is not the right note
either, since nothing here should later change: this is a decision, recorded
once.

One finding from generating that fixture, recorded because it would be an
expensive thing to assume: SUAVE's unit table reads the name `g` as a gram,
not as gravitational acceleration. Nothing this program calls asks it for
gravity (the only use is an emission index in grams per kilogram, on a path
that is never taken) but a translator who assumed otherwise would be wrong by
four orders of magnitude.
