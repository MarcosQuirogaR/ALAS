# Propulsion subsystem investigation brief

## Objective

Audit the current ALAS propulsion analysis as a coupled engineering pipeline, determine which hypotheses and model choices are physically or numerically weak, trace propulsion inputs through mission, mass, performance, feasibility, export, and figures, and define a research-backed replacement architecture that can host conventional, hydrogen, electric, hybrid, and other propulsion families. This turn is diagnostic and architectural only: it must not implement the replacement.

## System boundary

In scope: `alas-config`, `alas-prop`, `alas-mission`, `alas-pipeline`, `alas-perf`, `alas-mass`, `alas-report`, propulsion-related exports/golden data, existing research documents, NotebookLM project notebooks, public scientific libraries, and primary literature or official validation data.

Out of scope: changing Rust production code, recalibrating the current engine catalog, selecting a final hydrogen architecture, certification claims, and unrelated aerodynamics, structures, or NASTRAN work except where their interfaces consume propulsion outputs.

## Required deliverables

1. A complete source-to-figure dataflow with input units, authority, transformations, outputs, and downstream consumers.
2. An evidence-labelled audit of physical inaccuracies, hidden assumptions, broken couplings, extrapolations, numerical risks, and validation gaps.
3. A staged, multi-fidelity propulsion/energy architecture capable of coexisting propulsion families and explicit resource, thermal, mass, and installation coupling.
4. A comparison of public Python, MATLAB/Simulink, C/C++, and numerical libraries for reference, wrapping, selective porting, validation, licensing, and effort.
5. A reproducible report with local file references, NotebookLM provenance, external primary sources, limitations, and next verification actions.

## Evidence policy

- Treat current Rust source and executed tests as implementation evidence.
- Use NotebookLM for project-specific reconstruction, keeping its claims separate from local source inspection and external literature.
- Prefer official repositories, NASA/ICAO/OEM or regulator data, peer-reviewed papers, and published experimental comparisons.
- Separate implementation correctness, numerical verification, calibration, and physical validation. Code-to-code parity is not hardware validation.
- Use SI units internally; make gravity, atmospheric model, frames, force boundaries, and validity domains explicit.
- Mark direct observations, source-grounded claims, engineering inferences, and unresolved hypotheses separately.

## Working risks to resolve

- Multiple propulsion authorities may be active: a closed-form cycle, a product cycle, an empirical mission deck, legacy compatibility models, and turboprop surrogates.
- Mission fuel/thrust may be decoupled from the cycle variables shown in reports and used by optimization.
- Phase-specific ratings, throttle limits, installation effects, and OEI behavior may not share one capability contract.
- The current mass/mission state appears Jet-A-specific and may not carry electric, hydrogen, thermal, or tank states.
- Public library comparisons can demonstrate numerical or code-to-code agreement without demonstrating real-engine accuracy.

## Success criteria

The investigation is complete when a reviewer can identify the active propulsion model at every pipeline stage, reproduce the principal transformations and units, distinguish validated quantities from calibrated or extrapolated quantities, understand the highest-risk physical consequences, and follow a concrete migration/verification plan without needing to infer missing architecture.
