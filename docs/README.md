# ALAS documentation

Start with the section that matches what you are doing. The "Target section"
column is where the file will move when the repository layout in
[ADR 0014](adr/0014-repository-layout.md) is carried out; until then every file
stays at the path shown.

## Running and using ALAS

| Document | What it answers | Target section |
|---|---|---|
| [RUNNING.md](RUNNING.md) | Build, run, check; the fast iteration loop | user |
| [HEADLESS.md](HEADLESS.md) | Command-line options, output files, the `--mcp` tool server and its tools | user |
| [AGENT-GUIDE.md](AGENT-GUIDE.md) | Driving ALAS from a script or automation client: setup, workflow, worked case, honesty rules | user |
| [downloads.md](downloads.md) | Which packages exist and what each contains | user |
| [ADDONS.md](ADDONS.md) | Writing and applying add-on calculators | user |
| [tool-installer.md](tool-installer.md) | The one-click external tool installer | user |
| [CFD.md](CFD.md) | The standalone OpenFOAM airfoil study | user |
| [OPTIMIZATION.md](OPTIMIZATION.md) | What happens when optimization runs and which inputs decide it | user |
| [openvsp-preview.md](openvsp-preview.md) | Native OpenVSP screenshots on Windows | user |

## Architecture and decisions

| Document | What it answers | Target section |
|---|---|---|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Program shape, crate map, data flow of a run | architecture |
| [architecture/crates.md](architecture/crates.md) | Layers, responsibilities and boundary review of every crate | architecture |
| [architecture/README.md](architecture/README.md) | The capability map and what `xtask checks` enforces | architecture |
| [adr/](adr/README.md) | Why the cross-cutting decisions were taken | adr |
| [mass-model-architecture.md](mass-model-architecture.md) | The three mass questions, ownership of every kilogram, sizing basis | architecture |
| [route-model.md](route-model.md) | The generated airway route model | architecture |
| [optimizer-design-vector.md](optimizer-design-vector.md) | Units, frames, signs and validity of each design variable | architecture |
| [optimization-strategy.md](optimization-strategy.md) | Mission-sized objective and physical acceptance | architecture |
| [design-constraints.md](design-constraints.md) | Audit of configuration fields presented as constraints | architecture |
| [cargo-capacity.md](cargo-capacity.md) | Cargo capacity and its correlation | architecture |

## Methods, sources and evidence

| Document | What it answers | Target section |
|---|---|---|
| [methods.md](methods.md) | The models and their sources | methods |
| [flops-mass-model.md](flops-mass-model.md) | The FLOPS transport mass equations as implemented | methods |
| [flops-mass-sources.json](flops-mass-sources.json) | Per-equation source records for the FLOPS mass model | methods |
| [flops-robustness.md](flops-robustness.md) | OEW verification of the FLOPS-based mass path | developer |
| [propeller-data.md](propeller-data.md) | Propeller tables the UAV model reads (not distributed) | methods |
| [preset-correlation.md](preset-correlation.md) | Preset audit against evidence-graded datasets | methods |
| [aircraft-parity.md](aircraft-parity.md) | Real-aircraft parity audit | methods |
| [uav-db-schema.md](uav-db-schema.md), [uav-spec-schema.md](uav-spec-schema.md) | UAV database and spec schemas | methods |
| [PORTING.md](PORTING.md) | Parity and licence-provenance ledger of translated modules (stays at this path) | - |

## Developing and releasing

| Document | What it answers | Target section |
|---|---|---|
| [../CONTRIBUTING.md](../CONTRIBUTING.md) | Rules for a change: gate, parity, style | - |
| [developer/examples.md](developer/examples.md) | Catalogue of per-crate examples and probes | developer |
| [release-packaging.md](release-packaging.md) | `cargo xtask dist`, manifests, package kinds | developer |
| [NASTRAN95-BUNDLE.md](NASTRAN95-BUNDLE.md) | NASTRAN-95 bundle policy and validated performance | developer |
| [dependency-policy.md](dependency-policy.md) | `cargo deny` policy and recorded exceptions | developer |
| [cabin-section-renderer.md](cabin-section-renderer.md), [cabin-renderer-sources.md](cabin-renderer-sources.md) | The cabin cross-section renderer and its evidence | developer |
| [source-size-budgets.tsv](source-size-budgets.tsv) | Ceilings for files above 500 lines; only ever shrinks | - |

## Data and schemas read by code

`schemas/` (JSON schemas and fixtures for exported scenes and lofts), `data/`
(third-party datasets with their licences) and `benchmarks/` are inputs to tests
and tools; they stay where they are.
