# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Translation of user-facing text (Spanish, extensible to further languages).

**Design: English stays canonical in the source.** Config field labels/help,
figure titles and axis labels remain written inline in English exactly as
before; translation happens at the boundary where text is handed to the UI
(``sidecar/schema.py`` for forms, the figure factories for charts), by looking
the English string up in a catalog. Nothing in the physics or config modules has
to know a second language exists, there is no duplicated prose to keep in sync,
and an untranslated string degrades to English rather than to a missing-key
placeholder.

The active language is a :class:`~contextvars.ContextVar`, set per request from
``?lang=`` (see ``sidecar/server.py``'s middleware). A ContextVar rather than a
plain global because the sidecar serves requests concurrently and renders
figures on worker threads: each request gets its own value, and threads spawned
from it inherit the value they were created with instead of racing over one
shared setting.

Catalogs live in :mod:`alas.translations` (one module per language) so this
module stays logic-only.
"""

from __future__ import annotations

import contextvars
from typing import Dict, Optional

DEFAULT_LANGUAGE = "en"
SUPPORTED_LANGUAGES = ("en", "es")

_LANGUAGE: contextvars.ContextVar[str] = contextvars.ContextVar(
    "alas_language", default=DEFAULT_LANGUAGE
)

# Loaded lazily: the Spanish catalog is a large dict and headless/CLI use never
# needs it.
_CATALOGS: Dict[str, Dict[str, str]] = {}


def normalize_language(lang: Optional[str]) -> str:
    """Map anything the UI might send ("es-ES", "ES", None) onto a supported
    code, falling back to English rather than raising."""
    if not lang:
        return DEFAULT_LANGUAGE
    base = str(lang).strip().lower().replace("_", "-").split("-")[0]
    return base if base in SUPPORTED_LANGUAGES else DEFAULT_LANGUAGE


def set_language(lang: Optional[str]) -> str:
    normalized = normalize_language(lang)
    _LANGUAGE.set(normalized)
    return normalized


def get_language() -> str:
    return _LANGUAGE.get()


def _catalog(lang: str) -> Dict[str, str]:
    if lang in _CATALOGS:
        return _CATALOGS[lang]
    catalog: Dict[str, str] = {}
    if lang == "es":
        try:
            from .translations.es import CATALOG as es_catalog

            catalog = es_catalog
        except Exception:
            catalog = {}  # a broken/absent catalog must never break the app
    _CATALOGS[lang] = catalog
    return catalog


def t(text: Optional[str], lang: Optional[str] = None) -> str:
    """Translate ``text`` into the active (or given) language.

    Falls back to the original string whenever there's no entry -- partial
    catalogs are therefore always safe to ship, and a newly-added English
    string shows up untranslated instead of blank.
    """
    if not text:
        return text or ""
    language = normalize_language(lang) if lang is not None else get_language()
    if language == DEFAULT_LANGUAGE:
        return text
    return _catalog(language).get(text, text)
