# Contributing to ALAS

Thanks for considering it. ALAS is a conceptual aircraft design tool used for
teaching and research, so a change that produces a *plausible but wrong* number
is worse than one that crashes. Most of what follows exists to protect that.

---

## Getting set up

```bash
git clone https://github.com/MarcosQuirogaR/ALAS
cd ALAS
uv sync --all-extras --group dev
uv run pytest
```

For the desktop application you also need Go 1.22+, Node 20+ and Wails v2:

```bash
cd desktop && wails dev
```

Optional extras, none of which are required to contribute:

```bash
uv run python scripts/provision_suave_venv.py   # SUAVE mission analysis
uv run python scripts/download_navdata.py       # real airway routing (GPL-3.0)
```

> On Windows, avoid working inside a OneDrive- or Dropbox-synced folder. The
> sync client holds locks on freshly written files, which makes PyInstaller
> builds fail intermittently and for no visible reason.

---

## Before you open a pull request

1. `uv run pytest` passes.
2. `uv run ruff check .` and `uv run ruff format --check .` are clean.
3. For frontend changes, `npm run build` (which typechecks) succeeds.
4. If you touched anything numerical, say in the PR **what changed in the
   output and why**. "No change expected" is a valid and useful claim — say it
   so a reviewer knows to verify it.

Open an issue before starting anything large. It is much cheaper to disagree
about an approach in an issue than in a finished branch.

---

## Code style

Formatting is automated; run `ruff format` and don't think about it. What
follows is the part a formatter cannot enforce.

### Comments explain *why*, in the present tense

A comment earns its place by explaining something the code cannot: why a
constant has that value, why an obvious simpler approach fails, what invariant
must hold. Anything else is maintenance burden.

```python
# Good -- explains a non-obvious constraint
# A blank path must not fall through: Path("") normalises to ".", so
# `root / ""` is root itself, which exists and would then be launched.

# Bad -- narrates history
# Previously this used exists(); changed on 2026-07-04 to is_file()
# after the bug report about NASTRAN.

# Bad -- restates the code
# Increment the counter
counter += 1
```

Specifically, **do not** write:

- **Dates, version numbers, or session references.** `git log`, the pull
  request, and `CHANGELOG.md` already record when something changed and why.
  A comment saying "previously this did X" becomes unreadable the moment
  someone reads the file without that history in front of them.
- **Change narration.** "Now uses…", "Fixed…", "Note that we changed…". The
  code is the current state; describe it, not the diff that produced it.
- **References to anything not in this repository.** A public file pointing at
  a document nobody can open is a dead end.
- **Attribution of requests.** "As requested", "per the review". Irrelevant to
  the reader.

### Docstrings

Every module needs one. Every public function needs at least a one-line
summary. Prose, not a tag soup:

```python
def resolve_tool_exe(configured, root):
    """Resolve a configured external-tool executable against ``root``.

    Returns None when the tool is unset or absent, so callers can report
    "not configured" rather than trying to launch something.
    """
```

### Other conventions

- **Type hints** on public functions. `from __future__ import annotations` is
  already used throughout.
- **US spelling in identifiers and user-visible strings** (`color`, not
  `colour`) — a `colour` prop next to a `color` prop is a real bug. Prose in
  comments is not policed.
- **ASCII in source comments.** Documentation may use whatever typography it
  likes; source files stay ASCII so they behave identically on every platform
  and editor.
- **Library code logs; it does not print.** `print()` belongs in `cli.py`.
  Anything under `alas/` that wants to report something uses `logging` — the
  same code runs under the CLI, the sidecar and the desktop app, and only one
  of those has a console.
- **No new dependencies without discussion.** Every one is a licence to audit
  and a wheel that has to exist on three platforms.

---

## Physics and numerical changes

This is the part that matters most.

- **Cite your source.** New empirical coefficients, correlations or geometry
  scaffolds need a reference — textbook, paper, or published data — recorded
  next to the value and, if it is a method, in `docs/methods.md`.
- **No magic numbers.** If a user might reasonably want to change it, it
  belongs in a `config/` dataclass with `label` and `help` metadata. Those
  fields are what the settings UI is generated from, so an undocumented field
  appears in the interface as a blank mystery.
- **Say what moved.** If a change shifts results, quantify it: which case,
  which quantity, before and after. A characterization test asserting
  today's pipeline output makes unintended shifts visible; if a change
  shifts the assertion deliberately, explain why in the same PR.
- **Fail loudly, degrade honestly.** When an external tool or dataset is
  missing, report it clearly and fall back to a documented approximation. Never
  return a number that looks fine but isn't.

---

## Tests

`tests/` uses pytest. The suite favours characterization tests -- assert what
the pipeline produces today so unintended changes surface -- over unit tests
that would need reference answers nobody has.

Worth writing a test for:

- Anything with a fallback path (a tool missing, a file absent, a blank config).
- Anything that behaves differently inside a PyInstaller bundle than in a dev
  checkout. That class of bug is invisible until someone downloads a release.
- Any bug you fix: the test that would have caught it.

---

## Commits and pull requests

- Present tense, imperative: "Fix static margin sign", not "Fixed" or "Fixes".
- The subject says what changed; the body says **why**, and what you considered
  instead. If the reasoning is interesting, it belongs here rather than in a
  comment.
- One logical change per commit. Mechanical reformatting goes in its own
  commit, never mixed with behaviour.
- Rebase rather than merge to keep history readable.

---

## Licensing of contributions

ALAS is AGPL-3.0-or-later. By opening a pull request you confirm you wrote the
contribution (or have the right to submit it) and that it may be distributed
under that licence.

Sign off your commits with `git commit -s`, which certifies the
[Developer Certificate of Origin](https://developercertificate.org/).

If a contributor licence agreement becomes necessary — it would be, to keep the
commercial-licensing option described in `NOTICE` open — it will be requested
before merging, never applied retroactively.

**Do not paste code from a source you cannot license.** That includes
proprietary solvers, textbook code listings, and anything under an incompatible
licence. Implementing a *published method* from its equations is fine and
welcome; copying someone's implementation is not.

---

## Reviews

Every change is reviewed before merging; see `GOVERNANCE.md`. Expect questions
about numerical impact and about whether new configuration is discoverable.
None of it is personal — the aim is that someone can trust a number ALAS prints
without reading the source to check.
