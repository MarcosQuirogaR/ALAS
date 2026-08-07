# Working on ALAS-native

You are translating a working aircraft design program from Python to Rust, one
module at a time, proving each one agrees with the original before anything
depends on it.

Read `docs/PORTING.md` first. It is the task list, the licence record and the
progress report, and it is authoritative over anything remembered from a
previous session.

---

## Is this task yours?

**This project routes work by reasoning difficulty, and taking a task above
your tier is the most expensive mistake available here.** A translation that
looks finished but is subtly wrong costs more to find than it saved.

Do the task if you are running as Opus or Fable. If you are running as Sonnet,
do it unless it appears below:

| Reserved for Opus/Fable | Why |
|---|---|
| Diagnosing any failing parity test | Forty wrong numbers is a reasoning problem. Read the report, form a hypothesis about *which* step diverged, and test it. Guessing at it burns more than it saves. |
| The FITPACK bicubic spike (`alas-math::bicubic`) | Matching SciPy's knot placement and end conditions, with no fixture to lean on until it works. |
| The MINPACK `hybrd` translation (`alas-math::hybrd`) | Convergence behaviour has to match, not just the answer. |
| The `ConfigNode` derive macro (`alas-config-derive`) | A proc macro whose shape the next 5,600 lines are written against. |
| The VORLAX kernel (`alas-aero::vorlax`) | `f32` summation order determines whether parity is achievable at all. |
| Changing a tolerance tier, or declaring a deviation | These are judgement calls that get written into the ledger and defended later. |
| Adding a dependency, or changing the crate layering | Architecture. |

If a task is reserved and you are Sonnet: **stop and say so.** Do not attempt a
partial version, and do not lower a tolerance to make something pass. Report
what you found and hand back.

Everything else — mechanical translation with a fixture to check against, new
figure families, configuration structs, external-tool orchestration, tests — is
yours. Most of the remaining work is in that category.

---

## The rules that are not negotiable

1. **A translated module lands with its parity test.** Same change, not a
   follow-up. Its `docs/PORTING.md` row moves to `green` and names the fixture
   and tier. Nothing may depend on a module that is not `green`.
2. **Never loosen a tolerance to get a pass.** Tiers come from
   `alas-testkit::Tier` and describe what the code does, not what today's
   numbers need. A module that cannot pass at its tier is a finding to write
   down and escalate.
3. **Translate faithfully, including upstream mistakes.** Quirks are recorded
   as `deviation-candidate` in the ledger and fixed later, deliberately, in
   their own phase. A port that silently improves things cannot be validated.
4. **Do not edit the Python implementation.** It is the reference. It lives at
   `C:\Users\Marcos\OneDrive\Proyectos\Universidad\ALAS` and is read-only to
   you, including its git state.
5. **Do not commit unless asked.** The author commits.

`CONTRIBUTING.md` has the rest: why-comments only, no change narration, ASCII
source, US spelling in identifiers, `tracing` never `println!`, no `unwrap` or
`expect` outside tests, every `#[allow]` explained, no file over 500 lines.
`cargo xtask gate` enforces what can be enforced mechanically; passing it is the
floor, not the standard.

---

## Write like these files

Imitate a concrete file, not an abstract description of style:

- Physics and numerics: `crates/alas-units/src/lib.rs`
- A parity test: `crates/alas-units/tests/parity_units.rs`
- A fixture generator: `golden/generators/gen_units.py`
- Types and contracts: `crates/alas-types/src/lib.rs`

What they have in common: a module doc that says why the module exists and what
would go wrong without it, references for every constant, and tests named as
sentences that state the property being checked.

---

## How a module gets translated

1. Read the Python source completely. Note anything surprising — it is probably
   load-bearing, and the ledger may already flag it.
2. Write or extend a generator in `golden/generators/`, run it, and check the
   fixture into `golden/`.
3. Write the Rust, with a provenance header when it is a translation:
   ```rust
   // Ported from aerosandbox/atmosphere/_isa_atmo_functions.py
   // Upstream: AeroSandbox 4.2.8, MIT.
   // Reference: alas @ 7d1555c1f4db5110cf6cd187c156718e1a033b50.
   ```
4. Write the parity test against the fixture, at the tier the ledger names.
5. Write unit tests for what parity cannot see: a fixture only covers the
   points it was generated at.
6. `cargo xtask gate`, then update the ledger row.

---

## Commands

`cargo` is not on `PATH` in a fresh shell. Prefix PowerShell calls:

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"; Set-Location C:\Proyectos\ALAS-native; cargo test --workspace
```

| Command | What it does |
|---|---|
| `cargo test --workspace` | Everything, parity tests included |
| `cargo test -p alas-units` | One crate |
| `cargo xtask gate` | Format, lints, tests, repository checks |
| `cargo xtask checks` | The repository checks alone, no compilation |
| `cargo fmt --all` | Apply formatting |

Fixture generators run under the reference implementation's environments:

```powershell
# Anything touching SUAVE
& "C:\Users\Marcos\OneDrive\Proyectos\Universidad\ALAS\.suave-venv\Scripts\python.exe" golden/generators/gen_x.py
# Anything touching AeroSandbox or ALAS itself
& "C:\Users\Marcos\OneDrive\Proyectos\Universidad\ALAS\.venv\Scripts\python.exe" golden/generators/gen_x.py
```

Generators refuse to run against a dirty reference tree, on purpose: a fixture
that cannot be reproduced is not evidence.

---

## Things that have already caught someone out

- SUAVE's unit table reads `g` as a **gram**, not gravity.
- SUAVE monkeypatches `Quantity.__getattr__`, so `float()` on a unit fails.
  Multiplying is how its own code reads a ratio out.
- AeroSandbox's default atmosphere is a CasADi B-spline fit, not the closed
  form. Translate the `isa` branch; they agree to 1e-11.
- The reference's `mesh_line` has a latent indexing bug that is harmless where
  it is called. Do not reproduce it.
- SUAVE's mission builds six aerodynamic configurations that are identical,
  because the model it uses does not discretize control surfaces.
