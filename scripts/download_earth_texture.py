# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""Fetch the Earth texture used by the 3-D route globe and the 2-D route map.

NASA Earth Observatory "Land Shallow Topo" -- public domain, left out of the
distribution only to keep the download small. Without it the globe renders
untextured and the 2-D map falls back to a plain themed background.

The packaged application offers the same download from
Setup > External Tools, so this script is for dev checkouts and CI.

    uv run python scripts/download_earth_texture.py
    uv run python scripts/download_earth_texture.py --dest /somewhere/else.jpg
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from alas.integration.assets import (  # noqa: E402  (needs the path above)
    default_texture_path,
    download_earth_texture,
    texture_status,
)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--dest",
        type=Path,
        default=None,
        help="File to write (default: wherever ALAS itself looks for it).",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="Re-download even if the texture is already present.",
    )
    args = parser.parse_args()

    dest = args.dest or default_texture_path()
    status = texture_status(dest)
    if status.available and not args.force:
        print(
            f"Texture already {status.detail} -- nothing to do (use --force to refresh)."
        )
        return 0

    try:
        download_earth_texture(dest, progress=print)
    except OSError as exc:
        print(f"Download failed: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
