# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""Optional downloadable assets: enroute navdata and the Earth texture.

These cannot simply ship with the application -- the navdata is GPLv3, which
this project's licence cannot redistribute -- so the packaged app has to be
able to fetch them itself. A dev checkout can use ``scripts/download_navdata.py``
instead, but a packaged install has no ``scripts/`` directory and no Python,
which would otherwise leave airway routing permanently unreachable.

Downloads are synchronous. Each is a few seconds over a normal connection and
FastAPI runs plain ``def`` handlers in a worker thread, so a slow fetch delays
only the caller rather than the whole sidecar.
"""

from __future__ import annotations

from fastapi import APIRouter

from ..integration.assets import (
    default_navdata_dir,
    default_texture_path,
    download_earth_texture,
    download_navdata,
    navdata_status,
    texture_status,
)

router = APIRouter()


def _snapshot() -> dict:
    navdata = navdata_status()
    texture = texture_status()
    return {
        "navdata": {
            "available": navdata.available,
            "detail": navdata.detail,
            "path": str(default_navdata_dir()),
            "licence": "GPL-3.0 (X-Plane project, mcantsin/x-plane-navdata)",
            "purpose": "Real waypoint/airway routing. Without it, routes are great circles.",
        },
        "texture": {
            "available": texture.available,
            "detail": texture.detail,
            "path": str(default_texture_path()),
            "licence": "Public domain (NASA Earth Observatory)",
            "purpose": "Textured Earth on the route map and 3-D globe.",
        },
    }


@router.get("/assets/status")
def get_assets_status() -> dict:
    """Where each optional asset is expected, and whether it is there yet."""
    return _snapshot()


@router.post("/assets/navdata")
def fetch_navdata() -> dict:
    """Download the GPLv3 enroute navdata.

    Callers are expected to have shown the licence first: accepting it is the
    user's decision, not this project's, which is precisely why the data is
    not shipped.
    """
    log: list[str] = []
    try:
        download_navdata(progress=log.append)
    except OSError as exc:
        return {"ok": False, "error": str(exc), "log": log, **_snapshot()}
    return {"ok": True, "log": log, **_snapshot()}


@router.post("/assets/texture")
def fetch_texture() -> dict:
    """Download the public-domain Earth texture.

    Normally already present -- it ships in the bundle -- so this exists for
    an install where it was removed to reclaim disk.
    """
    log: list[str] = []
    try:
        download_earth_texture(progress=log.append)
    except OSError as exc:
        return {"ok": False, "error": str(exc), "log": log, **_snapshot()}
    return {"ok": True, "log": log, **_snapshot()}
