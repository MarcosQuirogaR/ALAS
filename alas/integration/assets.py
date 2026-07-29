# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""Downloadable mission-analysis assets: open enroute navdata and the Earth
texture, plus the canonical answer to where each one lives.

Neither asset ships with ALAS:

* The navdata (waypoints + jet airways) is GPLv3, published by the X-Plane
  project via the ``mcantsin/x-plane-navdata`` mirror. Distributing it would
  impose that licence on anyone redistributing ALAS, so it is neither
  committed nor frozen into the executable.
* The Earth texture is public domain (NASA Earth Observatory, "Land Shallow
  Topo"), left out purely to keep the download small.

Both are optional. Without the navdata, routing falls back to a great circle;
without the texture, the globe renders untextured.

This module is deliberately dependency-light -- ``urllib`` and ``pathlib``
only -- so the routing, reporting and sidecar layers can all import it for
asset locations without dragging in anything heavier.
"""

from __future__ import annotations

import os
import urllib.request
from pathlib import Path
from typing import Callable, NamedTuple, Optional

# Tracks a third party's default branch, so the data can change underneath a
# reproducible design run. Pinning to a commit SHA would fix that; it is left
# unpinned only because the mirror publishes no tags to pin to.
_NAVDATA_BASE_URL = "https://raw.githubusercontent.com/mcantsin/x-plane-navdata/master"
_NAVDATA_FILES = ["earth_fix.dat", "earth_awy.dat", "earth_nav.dat"]
_TEXTURE_URL = (
    "https://upload.wikimedia.org/wikipedia/commons/c/c4/Land_shallow_topo_2048.jpg"
)

# Repo-root-relative locations, matching the MissionConfig defaults. Resolution
# against a real directory is paths.resolve_data_path's job.
NAVDATA_REL = "alas/data/navdata"
TEXTURE_REL = "alas/data/textures/earth_blue_marble.jpg"

# A truncated download must not look like a valid asset: the navdata parser
# consumes a half-written earth_fix.dat happily and produces a quietly wrong
# route. These are conservative floors, not exact sizes -- upstream revises
# the data periodically.
_MIN_BYTES = {
    "earth_fix.dat": 1_000_000,
    "earth_awy.dat": 1_000_000,
    "earth_nav.dat": 500_000,
}
_MIN_TEXTURE_BYTES = 200_000
_TIMEOUT_S = 60.0


class AssetStatus(NamedTuple):
    available: bool
    detail: str


def default_navdata_dir() -> Path:
    """Where the navdata is, or where :func:`download_navdata` will put it."""
    from ..paths import resolve_data_path

    return resolve_data_path(NAVDATA_REL)


def default_texture_path() -> Path:
    """Where the Earth texture is, or where the downloader will put it."""
    from ..paths import resolve_data_path

    return resolve_data_path(TEXTURE_REL)


def navdata_status(navdata_dir: Optional[Path] = None) -> AssetStatus:
    navdata_dir = (
        Path(navdata_dir) if navdata_dir is not None else default_navdata_dir()
    )
    have = [f for f in _NAVDATA_FILES if (navdata_dir / f).exists()]
    if len(have) == len(_NAVDATA_FILES):
        return AssetStatus(True, f"present at {navdata_dir}")
    return AssetStatus(
        False, f"missing ({len(have)}/{len(_NAVDATA_FILES)} files) at {navdata_dir}"
    )


def texture_status(texture_path: Optional[Path] = None) -> AssetStatus:
    texture_path = (
        Path(texture_path) if texture_path is not None else default_texture_path()
    )
    if texture_path.exists():
        return AssetStatus(True, f"present at {texture_path}")
    return AssetStatus(False, f"missing at {texture_path}")


def _download(
    url: str, dest: Path, min_bytes: int, report: Callable[[str], None]
) -> None:
    """Fetch ``url`` to ``dest`` so that a failure leaves no usable file.

    Downloads land on a ``.part`` sibling and are moved into place with
    ``os.replace`` only after clearing ``min_bytes``. An interrupted transfer
    therefore leaves either the previous good file or nothing -- never a
    truncated one that a parser would accept.
    """
    dest.parent.mkdir(parents=True, exist_ok=True)
    part = dest.with_name(dest.name + ".part")
    report(f"Downloading {url} -> {dest} ...")
    try:
        with (
            urllib.request.urlopen(url, timeout=_TIMEOUT_S) as response,
            open(part, "wb") as handle,
        ):
            while chunk := response.read(1 << 20):
                handle.write(chunk)
        size = part.stat().st_size
        if size < min_bytes:
            raise OSError(
                f"{dest.name} was only {size:,} bytes (expected at least {min_bytes:,}); "
                "the download was truncated or the URL returned an error page"
            )
        os.replace(part, dest)
    finally:
        part.unlink(missing_ok=True)


def download_navdata(navdata_dir: Optional[Path] = None, progress=None) -> Path:
    """Download the GPLv3 open enroute navdata, returning the directory used.

    ``progress`` is an optional ``Callable[[str], None]`` for status updates.
    """
    report = progress or (lambda _msg: None)
    navdata_dir = (
        Path(navdata_dir) if navdata_dir is not None else default_navdata_dir()
    )
    for name in _NAVDATA_FILES:
        _download(
            f"{_NAVDATA_BASE_URL}/{name}", navdata_dir / name, _MIN_BYTES[name], report
        )
    report(f"Navdata installed at {navdata_dir} (GPLv3, mcantsin/x-plane-navdata).")
    return navdata_dir


def download_earth_texture(texture_path: Optional[Path] = None, progress=None) -> Path:
    """Download the public-domain NASA Blue Marble texture, returning its path."""
    report = progress or (lambda _msg: None)
    texture_path = (
        Path(texture_path) if texture_path is not None else default_texture_path()
    )
    _download(_TEXTURE_URL, texture_path, _MIN_TEXTURE_BYTES, report)
    report(
        f"Earth texture installed at {texture_path} (public domain, NASA Earth Observatory)."
    )
    return texture_path
