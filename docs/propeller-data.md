# Propeller performance data

ALAS does not distribute propeller performance tables. The UAV electric
propulsion model (`alas-uav`, thrust, motor current and battery power at the
selected speed) needs a Ct/Cp table per propeller; it reads APC `PER3_*.dat`
files from a local directory that you fill. Without them the rest of ALAS works
unchanged, and the UAV workflow reports
`propeller performance data ... are unavailable` with the reason; propellers
are not offered as analysis-ready and no propulsion number is invented.

## Where the files live

Resolved in this order:

1. `alas_uav::propulsion_electric::prop_data::set_prop_data_dir` (host apps);
2. the environment variable `ALAS_PROP_DATA_DIR`;
3. the per-user data directory:
   - Windows: `%LOCALAPPDATA%\ALAS\prop-data\apc`
   - macOS: `~/Library/Application Support/ALAS/prop-data/apc`
   - Linux: `$XDG_DATA_HOME/ALAS/prop-data/apc` (or `~/.local/share/...`)

The directory holds the original `PER3_<model>.dat` files, nothing else is
required. It is read once per process; restart the application after adding
files.

## Install the tables

Download (opt-in; nothing is fetched unless you run this):

```text
cargo run -p alas-uav --release --example fetch_props -- --models 12x6E,10x5E
cargo run -p alas-uav --release --example fetch_props -- --electric   # every E-series table
cargo run -p alas-uav --release --example fetch_props -- --all
cargo run -p alas-uav --release --example fetch_props -- --status
```

The tool uses the system `curl`, reads APC's robots.txt and public index page,
requests only `https://www.apcprop.com/files/PER3_*.dat`, waits at least 5 s
between requests, identifies itself with a generic User-Agent (no personal
data), validates every file by parsing it before keeping it, and skips files
already installed, so an interrupted run resumes. Three consecutive failures
stop the run. `--dir PATH` selects another directory.

Import files you already have (a folder or a `.zip`, for example a download from
APC's site made in a browser):

```text
cargo run -p alas-uav --release --example fetch_props -- --import C:\path\to\folder
cargo run -p alas-uav --release --example fetch_props -- --import C:\path\to\props.zip
```

Only valid `PER3_*.dat` tables are copied; anything else is reported and ignored.

## Notes

- The tables are APC's compilation, published without a licence statement. Keep
  them local, do not commit or redistribute them, and check APC's terms for your
  use. See `THIRD-PARTY-NOTICES.md`.
- APC's tables are analytical predictions, not measurements. Independent
  wind-tunnel data for many APC propellers are published by the University of
  Illinois (UIUC Propeller Data Site, <https://m-selig.ae.illinois.edu/props/propDB.html>);
  download and cite them from there if you want to compare.
- Lookups never extrapolate: a speed or RPM outside a table is an error.
- Tests use made-up synthetic tables in `crates/alas-uav/tests/fixtures/prop_data`
  that exercise the code paths only; they say nothing about any real propeller.
