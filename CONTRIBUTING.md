# Contributing to ALAS

ALAS is a conceptual aircraft design tool used for teaching and research, so a
change that produces a *plausible but wrong* number is worse than one that
crashes. Most of what follows exists to protect that.

This implementation carries an additional obligation the Python one did not:
it is a translation of a working program, and it is only worth having if it
agrees with it. That is what the parity rule below is for.

---

## Getting set up

```
git clone <local path>
cd ALAS-native
cargo test
cargo xtask gate
```

The MSVC toolchain is required; `rust-toolchain.toml` pins the version.

Optional external tools — MSES, MSC Nastran, NASTRAN-95, AVL — are configured
in the application and are never needed to build or to run the test suite. The
tests that use them are marked `#[ignore]` and run with
`cargo test -- --ignored`.

> Do not move this repository inside a OneDrive- or Dropbox-synced folder. The
> sync client holds locks on freshly written files, and `target/` produces
> thousands of them per build. `cargo xtask backup` writes a single git bundle
> into the synced folder instead, which is what the backup copy should be.

---

## Before you push

1. `cargo xtask gate` passes. It runs formatting, lints, tests and the
   repository checks described below, and it is what the pre-commit hook runs.
2. If you touched anything numerical, say in the commit body **what changed in
   the output and why**. "No change expected" is a valid and useful claim — say
   it so a reviewer knows to verify it.

---

## The parity rule

**A translated module lands together with the test that compares it against the
Python implementation.** Not in a follow-up commit, not "once the crate
compiles". In the same commit, with its row in `docs/PORTING.md` flipped to
`parity: green` and naming the fixture and tolerance tier it passes at.

Nothing may depend on a module that has not passed parity. The phase gates in
`docs/PORTING.md` exist to enforce that ordering.

The reason is not process for its own sake. Once two modules are ported and
neither is checked, a disagreement in the second one can be caused by the
first, and the search space for the bug is the whole tree. Checked one at a
time, it is always the module just written.

Tolerances are not chosen per test. They come from the tiers in
`golden/tolerances.toml`, each of which has a documented reason to exist:
closed-form arithmetic is held to a much tighter bound than anything that has
been through a linear solve, and `f32` kernels tighter still than iterative
solutions. Picking a looser tier than a module deserves is how a real
disagreement gets absorbed into a rounding allowance. If a module cannot pass
at its tier, that is a finding to write down, not a number to raise.

---

## Deliberate deviations

Faithful translation is the default, including of upstream behaviour that is
wrong. `Airfoil.aerodynamic_center` does not rotate its chordwise offset by the
section twist; the SUAVE static margin is computed against wing origins because
`aerodynamic_center` is left at the origin. These are reproduced, because a
port that silently improves things cannot be validated — every disagreement
becomes ambiguous between a bug and an intended improvement.

Improvements are wanted. They are made afterwards, one at a time, each of them:

- **config-gated**, with the previous behaviour still selectable;
- **cited**, to a textbook, paper or published data, next to the value or
  method;
- **quantified** against the parity baseline: which case, which quantity,
  before and after.

A quirk noticed during translation is recorded as `deviation-candidate` in the
`docs/PORTING.md` row and left alone. That list is the improvement backlog.

---

## Code style

Formatting is automated; run `cargo fmt` and don't think about it. What follows
is the part a formatter cannot enforce.

### Comments explain *why*, in the present tense

A comment earns its place by explaining something the code cannot: why a
constant has that value, why an obvious simpler approach fails, what invariant
must hold. Anything else is maintenance burden.

```rust
// Good -- explains a non-obvious constraint
// The trailing legs extend along the freestream, so the influence matrix
// depends on alpha only through this direction. Perturbing alpha for the
// stability derivatives therefore changes the right-hand side and not the
// matrix, which is what lets one factorization serve all six runs.

// Bad -- narrates history
// Previously this used f32; changed to f64 after the parity test failed.

// Bad -- restates the code
// Increment the counter
counter += 1;
```

Specifically, **do not** write:

- **Dates, version numbers, or session references.** `git log` and the commit
  message already record when something changed and why. A comment saying
  "previously this did X" becomes unreadable the moment someone reads the file
  without that history in front of them.
- **Change narration.** "Now uses…", "Fixed…", "Note that we changed…". The
  code is the current state; describe it, not the diff that produced it.
- **References to anything not in this repository.** A file pointing at a
  document nobody can open is a dead end. Published references are the
  exception and are welcome: cite them fully enough to be found.
- **Attribution of requests.** "As requested", "per the review". Irrelevant to
  the reader.
- **Restatements of the obvious in the voice of a narrator.** "Here we compute
  the lift coefficient", "Note that this returns a Result". If a reader can see
  it, writing it down costs them a line and tells them nothing.

`cargo xtask gate` greps for the most common of these. Passing the grep is not
the standard; the standard is the paragraph above.

### Documentation comments

Every module needs one. Every public item needs at least a one-line summary.
Prose, not a tag soup:

```rust
/// Resolve a configured external-tool executable against `root`.
///
/// Returns `None` when the tool is unset or absent, so callers can report
/// "not configured" rather than trying to launch something.
```

### Provenance headers

Every file translated from another project opens with one, after the SPDX
lines:

```rust
// Ported from aerosandbox/atmosphere/_isa_atmo_functions.py
// Upstream: AeroSandbox 4.2.8, MIT.
// Reference: alas @ rust-port-baseline.
```

This is what makes `THIRD-PARTY-NOTICES.md` auditable rather than aspirational.
No tool can tell that a file is a translation, so this one is on the author:
the gate checks that the licence header is present, and nothing checks that the
provenance block is honest.

### Other conventions

- **US spelling in identifiers and user-visible strings** (`color`, not
  `colour`) — a `colour` field next to a `color` field is a real bug. Prose in
  comments is not policed.
- **ASCII in source files.** Documentation may use whatever typography it
  likes; source files stay ASCII so they behave identically on every platform
  and editor. Greek letters go in the prose of a doc comment, not in an
  identifier.
- **Library code logs; it does not print.** `println!` belongs in `alas-app`.
  Anything in a library crate that wants to report something uses `tracing` —
  the same code runs under the desktop application and the headless command
  line, and only one of those has a console. The lint denies it, and the deny
  is the policy, not a suggestion.
- **No panics in library code.** `unwrap` and `expect` are denied outside
  tests. An analysis that cannot produce a number reports that through the
  stage status contract in `alas-types`; a genuine invariant violation returns
  an error and lets the caller decide.
- **Every `#[allow]` carries a comment saying why**, on the line above. An
  unexplained allow is an unreviewed decision.
- **No module over 500 assembled production lines**, tests excluded. This is
  enforced on the *assembled* module: `cargo xtask checks` parses each file,
  follows every `include!()` recursively, and counts what the compiler sees
  as one module, so splicing a file into `*_parts/` fragments does not make
  it smaller. Long modules are where context gets lost — by a reader, by a
  reviewer, and by a tool with a finite window. The Python implementation has
  a 6,200-line visualization module that nobody can hold in their head at
  once, and reproducing that here would undo half the point of the rewrite.
  Modules that were already over the limit when the assembled check landed
  are listed in `docs/source-size-budgets.tsv` with an explicit ceiling equal
  to their reviewed size and a one-line rationale. A listed module may
  shrink but not grow; once it drops to 500 lines the check asks for its row
  to be removed. Do not add rows for new modules — split them into real
  `mod`s with interfaces instead.
- **No new dependencies without justification.** Every one is a licence to
  audit and a supply chain to trust. Add it in `[workspace.dependencies]`, and
  say in the commit body what it does and what the alternative was.

---

## Physics and numerical changes

This is the part that matters most.

- **Cite your source.** New empirical coefficients, correlations or geometry
  scaffolds need a reference — textbook, paper, or published data — recorded
  next to the value and, if it is a method, in `docs/methods.md`.
- **No magic numbers.** If a user might reasonably want to change it, it
  belongs in a configuration struct with `label` and `help` metadata. Those
  fields are what the settings interface is generated from, so an undocumented
  field appears in the interface as a blank mystery. The derive macro refuses
  to compile a public field that has neither metadata nor an explicit skip.
- **Say what moved.** If a change shifts results, quantify it: which case,
  which quantity, before and after. The parity tests assert agreement with the
  Python implementation; if a change moves one of them deliberately, explain
  why in the same commit and record it as a deviation.
- **Fail loudly, degrade honestly.** When an external tool or dataset is
  missing, report it clearly and fall back to a documented approximation. Never
  return a number that looks fine but isn't. A fallback that was taken is
  visible in the result, not only in the log.

---

## Tests

Three kinds, and they answer different questions.

- **Parity tests** compare against fixtures generated from the Python
  implementation at the `rust-port-baseline` tag. They answer "is this the same
  program". Every translated module has one; see the parity rule above.
- **Property and unit tests** answer "is this self-consistent" — a wing area
  that scales quadratically with span, a round trip through serialization, an
  atmosphere that is monotonic in altitude. These catch the errors parity
  cannot, because a fixture only covers the points it was generated at.
- **Characterization tests** assert what the program produces today, so that
  unintended changes surface. These are the right tool for anything with no
  reference answer, which includes most of the pipeline's end-to-end output.

Also worth a test: anything with a fallback path (a tool missing, a file
absent, a blank configuration); anything that behaves differently in a release
build than under `cargo test`; and any bug you fix — the test that would have
caught it.

---

## Commits

- Present tense, imperative: "Fix static margin sign", not "Fixed" or "Fixes".
- The subject says what changed; the body says **why**, and what you considered
  instead. If the reasoning is interesting, it belongs here rather than in a
  comment.
- One logical change per commit. Mechanical reformatting goes in its own
  commit, never mixed with behaviour.
- A translated module and its parity test are one commit. That is the one case
  where "one logical change" spans two files by design.
- Rebase rather than merge to keep history readable.
- Sign off with `git commit -s`, which certifies the
  [Developer Certificate of Origin](https://developercertificate.org/).

---

## On tooling assistance

Parts of this codebase are written with the help of AI coding tools. That is
disclosed here, and it is disclosed in any academic work that reports on this
program, because the alternative is worse for everyone.

The conventions above are not there to disguise how the code was produced. They
are there because generated code has characteristic failure modes — restating
the obvious, narrating its own history, drifting in style between sessions,
producing plausible numbers with no reference behind them — and every one of
those is a maintenance cost or a correctness risk on its own terms. A human
writing "// Here we compute the drag" is making the same mistake and should get
the same review comment.

What is not negotiable is the numerical evidence. No module is trusted because
it looks right. It is trusted because it agrees with a reference to a stated
tolerance, and because the disagreement it does have is written down.

---

## Licensing of contributions

This program is AGPL-3.0-or-later. By contributing you confirm you wrote the
contribution, or have the right to submit it, and that it may be distributed
under that licence.

**Do not paste code from a source you cannot license.** That includes
proprietary solvers, textbook code listings, and anything under an incompatible
licence. Implementing a *published method* from its equations is fine and
welcome; copying someone's implementation is not — and where this program does
translate someone's implementation, it says so in a provenance header and
carries their licence. See `THIRD-PARTY-NOTICES.md`.
