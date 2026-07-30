# Installation

## Windows

Download the executable from the
[downloads page](/#download) and run it. There is no
installer and no separate runtime to fetch: the whole analysis stack is
inside the file.

Windows will show a SmartScreen warning the first time, because the build
is not code-signed. Choose **More info**, then **Run anyway**.

The download is around 300 MB, which is large for what looks like a desktop
application. The reason is that it carries a complete scientific computing
stack: the aerodynamic solver, the optimiser, the finite-element code and
the plotting library are all packaged in rather than installed separately.

### What you get without doing anything else

Optimisation, aerodynamics, wingbox structures, engine cycle, weight and
balance, stability, cabin layout, field performance and the transonic
section solver all work on a fresh install.

Two things do not, and both are optional:

| Feature | Needs |
|---|---|
| [Mission simulation](mission-and-route.md) | A one-time setup step, below |
| [Nastran and Patran cross-checks](structural-analysis.md) | Your own licensed installation |

## Linux

A packaged build is in progress. Until it lands, Linux users can run from
source: see below. The application itself is already Linux-native; what
remains is packaging the bundled analysis stack, which has to be built on
Linux rather than cross-compiled.

## Mission simulation setup

Mission analysis flies the aircraft through climb, cruise and descent using
[SUAVE](https://suave.stanford.edu). SUAVE needs an older numerical stack
than the rest of ALAS uses, so it lives in its own isolated
environment rather than being forced to share one.

Setting it up takes a couple of minutes and only has to be done once. Open
**Setup → External Tools** in the application, where the SUAVE row shows
its status and how to provision it.

Skipping this costs you one results tab. Everything else runs normally, and
the mission stage reports itself unavailable rather than failing the run.

### Optional route data

Two downloads make the mission more realistic, and both are on
**Advanced Settings → Mission Analysis**:

- **Airway navdata** lets routes follow published airways instead of a
  great-circle line. It is third-party GPLv3 data, so the application asks
  before fetching it.
- **Earth texture** gives the route map a textured background.

Without either, routing falls back to a great-circle track and the map
still draws.

## Running from source

Useful if you are on Linux, want to script ALAS, or intend to modify
it. You will need Python 3.10 or newer.

=== "pip"

    ```bash
    pip install -r requirements.txt
    python main.py
    ```

=== "uv"

    ```bash
    uv sync
    uv run python main.py
    ```

Running with no arguments opens the same application the executable does.
[Running ALAS](running-alas.md) covers the command-line and
library interfaces, which exist only in a source checkout.

Optional dependency groups:

| Group | Adds |
|---|---|
| `gui` | 3D previews and the textured route map |
| `sidecar` | The local API the desktop application talks to |
| `package` | The tooling that freezes it into an executable |

```bash
pip install -e .[gui]
```

## Checking it works

Open the application, leave every setting alone, and press
**Analyze baseline**. It runs in seconds and fills in the weight, balance
and stability tabs. If those look sensible, your installation is fine.

If something does not work, [Troubleshooting](troubleshooting.md) covers
the failures that actually happen.
