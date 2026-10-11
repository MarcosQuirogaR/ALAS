## What and why

<!-- One paragraph. The PR title becomes the squash-commit subject and must follow
Conventional Commits: type(scope): summary, e.g. "fix(aero): correct the flap
increment sign". Types: feat, fix, perf, refactor, docs, test, build, ci, chore. -->

## Numerical effect

<!-- Which case, which quantity, before and after. "No change expected" is a valid
answer. See CONTRIBUTING.md, "Physics and numerical changes". -->

## Checklist

- [ ] `cargo xtask gate --quick` passes locally
- [ ] New or changed behaviour has a test
- [ ] New empirical values cite their source
- [ ] Third-party code or data is listed in `THIRD-PARTY-NOTICES.md`
- [ ] I wrote this contribution or have the right to submit it under AGPL-3.0-or-later
