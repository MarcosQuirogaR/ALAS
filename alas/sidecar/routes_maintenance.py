# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Installation maintenance: inspect and reclaim the disk this install uses.

A packaged ALAS spreads sizeable data across three places, none of which
the user can easily reason about from Explorer:

* ``%LocalAppData%/ALAS/sidecar-cache/<fingerprint>`` -- the extracted
  Python sidecar bundle (hundreds of MB), kept across launches so the
  extraction cost is paid once per build.
* ``%LocalAppData%/ALAS/suave-cache/<fingerprint>`` -- the extracted SUAVE
  mission-analysis runtime, same idea.
* the OS temp dir -- scratch directories from MSES/SUAVE runs
  (``alas_mses_*``, ``alas_suave_*``). These are cleaned up by their
  own code paths, but a hard kill (or a crash mid-solve) can strand one.

plus ``outputs/`` next to the app, where exported designs/figures accumulate.

**The currently-running build's caches are never offered for deletion.** The
sidecar answering this request is itself executing out of its own
fingerprinted cache directory; removing it would pull the floor out from under
the live process. Stale fingerprints (previous builds) are what actually waste
space, and those are safe -- the launcher already prunes them opportunistically
(``pruneStaleSidecarCaches``), this just makes it explicit and on-demand.
"""

from __future__ import annotations

import os
import shutil
import tempfile
from pathlib import Path
from typing import Dict, List, Optional

from fastapi import APIRouter
from pydantic import BaseModel

from ..paths import SUAVE_RUNNER_DIR_ENV, SUAVE_VENV_DIR_ENV, app_root

router = APIRouter()


def _cache_root() -> Optional[Path]:
    """``%LocalAppData%/ALAS`` (or the XDG/macOS equivalent) -- must match
    ``desktop/sidecar.go``'s ``sidecarCacheRoot()``."""
    if os.name == "nt":
        base = os.environ.get("LOCALAPPDATA")
        return Path(base) / "ALAS" if base else None
    xdg = os.environ.get("XDG_CACHE_HOME")
    if xdg:
        return Path(xdg) / "ALAS"
    home = Path.home()
    if os.sys.platform == "darwin":  # type: ignore[attr-defined]
        return home / "Library" / "Caches" / "ALAS"
    return home / ".cache" / "ALAS"


def _dir_size(path: Path) -> int:
    total = 0
    try:
        for entry in path.rglob("*"):
            try:
                if entry.is_file():
                    total += entry.stat().st_size
            except OSError:
                continue  # vanished mid-walk / permission denied -- just skip
    except OSError:
        pass
    return total


def _active_cache_dirs() -> set:
    """Cache directories the RUNNING process depends on, which must never be
    deleted: the sidecar bundle this interpreter was extracted from, and the
    SUAVE runtime the launcher pointed us at."""
    active = set()
    # A frozen sidecar runs from <cache>/<fingerprint>/alas-sidecar/...
    try:
        exe_dir = Path(os.sys.executable).resolve().parent  # type: ignore[attr-defined]
        active.update({exe_dir, exe_dir.parent})
    except Exception:
        pass
    for env in (SUAVE_VENV_DIR_ENV, SUAVE_RUNNER_DIR_ENV):
        value = os.environ.get(env)
        if value:
            p = Path(value)
            active.update({p, p.parent})
    return active


def _is_active(path: Path, active: set) -> bool:
    resolved = path.resolve()
    for a in active:
        try:
            if resolved == a or resolved in a.parents:
                return True
        except Exception:
            continue
    return False


def _collect() -> List[Dict]:
    """Every reclaimable item, each tagged with whether it's safe to remove."""
    items: List[Dict] = []
    active = _active_cache_dirs()

    root = _cache_root()
    if root and root.exists():
        for group, label in (
            ("sidecar-cache", "Sidecar runtime"),
            ("suave-cache", "SUAVE runtime"),
        ):
            group_dir = root / group
            if not group_dir.exists():
                continue
            for entry in sorted(group_dir.iterdir()):
                if not entry.is_dir():
                    continue
                in_use = _is_active(entry, active)
                # Only ever assert "in use" -- the opposite can't be proven from
                # here (a dev-mode sidecar legitimately matches no cache dir),
                # and labelling a perfectly current runtime "stale" would invite
                # the user to delete something they'll just re-extract.
                items.append(
                    {
                        "id": f"{group}/{entry.name}",
                        "path": str(entry),
                        "label": f"{label} ({entry.name})"
                        + (" — in use" if in_use else ""),
                        "bytes": _dir_size(entry),
                        "removable": not in_use,
                        "category": "cache",
                    }
                )

    tmp = Path(tempfile.gettempdir())
    for pattern, label in (
        ("alas_mses_*", "MSES scratch"),
        ("alas_suave_*", "SUAVE scratch"),
        ("alas-sidecar-build*", "Build scratch"),
    ):
        for entry in sorted(tmp.glob(pattern)):
            items.append(
                {
                    "id": f"temp/{entry.name}",
                    "path": str(entry),
                    "label": f"{label} ({entry.name})",
                    "bytes": _dir_size(entry)
                    if entry.is_dir()
                    else entry.stat().st_size,
                    "removable": True,
                    "category": "temp",
                }
            )

    outputs = app_root() / "outputs"
    if outputs.exists():
        items.append(
            {
                "id": "outputs",
                "path": str(outputs),
                "label": "Exported outputs (designs, figures, mission CSVs)",
                "bytes": _dir_size(outputs),
                "removable": True,
                "category": "outputs",
            }
        )
    return items


@router.get("/maintenance/storage")
def get_storage() -> dict:
    items = _collect()
    return {
        "items": items,
        "total_bytes": sum(i["bytes"] for i in items),
        "reclaimable_bytes": sum(i["bytes"] for i in items if i["removable"]),
    }


class ClearRequest(BaseModel):
    # Item ids from GET /maintenance/storage. Empty = every removable item.
    ids: List[str] = []


@router.post("/maintenance/clear")
def clear_storage(req: ClearRequest) -> dict:
    """Delete the requested items. Never deletes an item marked non-removable,
    even if explicitly named -- the in-use runtime caches are load-bearing for
    the process serving this very request."""
    wanted = set(req.ids)
    freed = 0
    removed: List[str] = []
    failed: List[Dict[str, str]] = []
    for item in _collect():
        if not item["removable"]:
            continue
        if wanted and item["id"] not in wanted:
            continue
        path = Path(item["path"])
        try:
            if path.is_dir():
                shutil.rmtree(path, ignore_errors=False)
            else:
                path.unlink()
            freed += item["bytes"]
            removed.append(item["id"])
        except OSError as exc:
            # Windows file locks (OneDrive, an antivirus scan, a still-open
            # handle) are routine here -- report per item instead of failing
            # the whole sweep.
            failed.append({"id": item["id"], "error": str(exc)})
    return {"freed_bytes": freed, "removed": removed, "failed": failed}
