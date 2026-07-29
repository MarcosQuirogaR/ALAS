# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Cross-field validation endpoint.

Thin wrapper over :func:`alas.validation.validate` -- a UI-agnostic rule
engine (see that module's docstring) shared with the CLI.
"""

from __future__ import annotations

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

from ..config.settings import ALASConfig
from ..validation import validate

router = APIRouter()


class ValidateRequest(BaseModel):
    config: dict = {}


@router.post("/validate")
def validate_config(req: ValidateRequest) -> dict:
    try:
        config = ALASConfig.from_dict(req.config)
    except (KeyError, TypeError, ValueError) as exc:
        raise HTTPException(status_code=422, detail=str(exc))

    issues = validate(config)
    return {
        "issues": [
            {"field_path": i.field_path, "message": i.message, "severity": i.severity}
            for i in issues
        ]
    }
