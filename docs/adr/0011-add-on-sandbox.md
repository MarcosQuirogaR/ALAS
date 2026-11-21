# 0011. Add-on sandbox

Status: Accepted (2026-10-22)

## Context

Users want small calculators (for example a Raymer wing-weight equation) that
feed the design without rebuilding the program. Running user code is a trust
and reproducibility problem.

## Decision

1. Two add-on kinds with different trust levels. A `script` add-on is Rhai
   code run inside ALAS with no `import`, `eval`, file, network, process or
   clock access, bounded by the manifest timeout (default 10 s, at most 120 s),
   5 million operations, call depth 32, strings of 64 KiB, arrays of 10,000 and
   maps of 1,000 entries; exceeding a limit is an error, never a hang. A
   `process` add-on is an external program, not sandboxed, disabled by
   default, enabled per add-on and per version, speaking a versioned JSON
   protocol over stdin and stdout (output limit 1 MiB; the whole process tree
   is killed on timeout).
2. Manifests reject unknown keys, require at least one output, and record the
   add-on version with every applied value.
3. An add-on never writes the design on its own. An output with a `target`
   is applied only after the user clicks Apply and sees the before/after table;
   the candidate is decoded and validated first, and refused if it adds a
   validation error or leaves a design variable's cross-preset guardrail.
4. Add-on results are not validated physics: the documentation says so, and
   the bundled examples cite their method (ADR 0003).

## Consequences

- A changed process add-on version must be enabled again.
- The sandbox is a resource limit and capability removal, not a security
  boundary against a malicious native process add-on, which is why that kind
  needs explicit trust.

## Evidence

`crates/alas-addons`, `docs/ADDONS.md`.
