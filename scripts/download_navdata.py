# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""Fetch the open enroute navdata used for airway routing.

The data is GPLv3 (X-Plane project, via the mcantsin/x-plane-navdata mirror)
and is therefore not distributed with ALAS. Without it, routing falls
back to a great circle; with it, missions follow real waypoints and airways.

The packaged application offers the same download from
Setup > External Tools, so this script is for dev checkouts and CI.

    uv run python scripts/download_navdata.py
    uv run python scripts/download_navdata.py --dest /somewhere/else
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from alas.integration.assets import (  # noqa: E402  (needs the path above)
    default_navdata_dir,
    download_navdata,
    navdata_status,
)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--dest",
        type=Path,
        default=None,
        help="Directory to install into (default: wherever ALAS itself looks for it).",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Re-download even if the data is already present.",
    )
    args = parser.parse_args()

    dest = args.dest or default_navdata_dir()
    status = navdata_status(dest)
    if status.available and not args.force:
        print(
            f"Navdata already {status.detail} -- nothing to do (use --force to refresh)."
        )
        return 0

    try:
        download_navdata(dest, progress=print)
    except OSError as exc:
        print(f"Download failed: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
