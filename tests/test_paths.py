# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""Path resolution for bundled data and externally-provisioned tools.

These are the cheapest high-value tests in the project: every function under
test behaves differently inside a PyInstaller bundle than in a dev checkout,
so its failure mode is "the packaged app silently reports a tool as
unconfigured" -- reproducible only by building and launching the real
executable, which takes minutes. Simulating ``sys.frozen``/``sys._MEIPASS``
and the launcher's environment variables costs milliseconds instead.
"""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

from alas import paths
from alas.paths import (
    APP_DIR_ENV,
    app_root,
    bundle_root,
    find_tool_dir,
    is_frozen,
    resolve_tool_dir,
    resolve_tool_exe,
)


@pytest.fixture
def app_dir(tmp_path, monkeypatch):
    """A writable stand-in for the real install directory."""
    monkeypatch.setenv(APP_DIR_ENV, str(tmp_path))
    return tmp_path


# --------------------------------------------------------------------------
# app_root / bundle_root / is_frozen
# --------------------------------------------------------------------------


def test_app_root_uses_env_var_when_the_directory_exists(app_dir):
    assert app_root() == app_dir


def test_app_root_ignores_env_var_pointing_at_a_missing_directory(
    tmp_path, monkeypatch
):
    monkeypatch.setenv(APP_DIR_ENV, str(tmp_path / "does-not-exist"))
    monkeypatch.delattr(sys, "frozen", raising=False)
    # Falls back to the repo root rather than returning a path nothing can read.
    assert app_root() == paths._REPO_ROOT


def test_app_root_never_raises_without_the_env_var(monkeypatch):
    monkeypatch.delenv(APP_DIR_ENV, raising=False)
    monkeypatch.delattr(sys, "frozen", raising=False)
    assert app_root() == paths._REPO_ROOT


def test_is_frozen_follows_sys_frozen(monkeypatch):
    monkeypatch.delattr(sys, "frozen", raising=False)
    assert is_frozen() is False
    monkeypatch.setattr(sys, "frozen", True, raising=False)
    assert is_frozen() is True


def test_bundle_root_prefers_meipass_when_frozen(tmp_path, monkeypatch):
    monkeypatch.setattr(sys, "frozen", True, raising=False)
    monkeypatch.setattr(sys, "_MEIPASS", str(tmp_path), raising=False)
    assert bundle_root() == tmp_path


def test_bundle_root_is_the_repo_root_in_a_dev_checkout(monkeypatch):
    monkeypatch.delattr(sys, "_MEIPASS", raising=False)
    assert bundle_root() == paths._REPO_ROOT


def test_bundle_root_and_app_root_diverge_only_when_frozen(app_dir, monkeypatch):
    """The distinction the whole module exists for: read-only bundled data and
    the user's install directory are the same place in a dev checkout and two
    different places once frozen."""
    monkeypatch.delattr(sys, "_MEIPASS", raising=False)
    monkeypatch.delenv(APP_DIR_ENV, raising=False)
    assert bundle_root() == app_root()

    meipass = app_dir / "extraction-cache"
    meipass.mkdir()
    monkeypatch.setattr(sys, "frozen", True, raising=False)
    monkeypatch.setattr(sys, "_MEIPASS", str(meipass), raising=False)
    monkeypatch.setenv(APP_DIR_ENV, str(app_dir))
    assert bundle_root() == meipass
    assert app_root() == app_dir
    assert bundle_root() != app_root()


# --------------------------------------------------------------------------
# resolve_tool_dir -- always returns a path, for error messages
# --------------------------------------------------------------------------


def test_resolve_tool_dir_honours_an_absolute_path_verbatim(tmp_path, app_dir):
    override = tmp_path / "somewhere" / "else"
    assert resolve_tool_dir(override) == override


def test_resolve_tool_dir_finds_a_tool_beside_the_app(app_dir):
    tool = app_dir / "external tools" / "MSES"
    tool.mkdir(parents=True)
    assert resolve_tool_dir("external tools/MSES") == tool


def test_resolve_tool_dir_searches_the_bin_subdirectory(app_dir):
    tool = app_dir / "bin" / "external tools" / "MSES"
    tool.mkdir(parents=True)
    assert resolve_tool_dir("external tools/MSES") == tool


def test_resolve_tool_dir_prefers_beside_the_app_over_bin(app_dir):
    """_candidate_roots() is ordered; the first hit must win."""
    beside = app_dir / "external tools" / "MSES"
    beside.mkdir(parents=True)
    (app_dir / "bin" / "external tools" / "MSES").mkdir(parents=True)
    assert resolve_tool_dir("external tools/MSES") == beside


def test_resolve_tool_dir_returns_a_recognisable_guess_when_nothing_exists(app_dir):
    """Downstream reports "not found under {dir}", so the guess must name a
    place the user recognises -- not a temp extraction cache.

    The tool name must not exist in the real repo: _REPO_ROOT is always the
    last candidate root, so a name like "external tools/MSES" would be found
    in the developer's own checkout and never reach the fallback.
    """
    assert resolve_tool_dir("external tools/NOT_A_REAL_TOOL") == (
        app_dir / "external tools" / "NOT_A_REAL_TOOL"
    )


@pytest.mark.parametrize("blank", ["", "   ", "\t"])
def test_resolve_tool_dir_treats_blank_as_unset(blank, app_dir):
    """Path("") normalises to ".", which would resolve every candidate root to
    itself. Blank means unset, so return the app root explicitly."""
    assert resolve_tool_dir(blank) == app_dir


# --------------------------------------------------------------------------
# find_tool_dir -- returns None instead of guessing
# --------------------------------------------------------------------------


@pytest.mark.parametrize("blank", ["", "   ", "\t"])
def test_find_tool_dir_returns_none_for_blank(blank, app_dir):
    """Without this guard Path("") makes every candidate root "exist", so this
    returned the app root itself and beat the bundled-runtime env-var
    fallback -- surfacing as mission "not_configured" on a build that had
    SUAVE bundled correctly."""
    assert find_tool_dir(blank) is None


def test_find_tool_dir_returns_none_when_nothing_exists(app_dir):
    assert find_tool_dir("external tools/NOT_A_REAL_TOOL") is None


def test_repo_root_is_always_a_fallback_candidate(app_dir):
    """A dev checkout keeps working even when APP_DIR_ENV points elsewhere:
    the repo root is searched last, so tools committed to the tree are found
    without any configuration. This is why tests must use tool names that do
    not exist in the repo."""
    assert (paths._REPO_ROOT / "external tools" / "suave_runner").exists()
    assert find_tool_dir("external tools/suave_runner") == (
        paths._REPO_ROOT / "external tools" / "suave_runner"
    )


def test_find_tool_dir_finds_a_provisioned_directory(app_dir):
    tool = app_dir / "external tools" / "suave_runner"
    tool.mkdir(parents=True)
    assert find_tool_dir("external tools/suave_runner") == tool


def test_find_tool_dir_absolute_path_must_exist(tmp_path, app_dir):
    missing = tmp_path / "nope"
    assert find_tool_dir(missing) is None
    missing.mkdir()
    assert find_tool_dir(missing) == missing


# --------------------------------------------------------------------------
# resolve_tool_exe -- must never hand a directory to subprocess
# --------------------------------------------------------------------------


@pytest.mark.parametrize("blank", ["", "   ", "\t"])
def test_resolve_tool_exe_returns_none_for_blank(blank, tmp_path):
    """A blank path must not become `root / ""` == root, which exists and was
    then launched as a program -- producing a permission error naming the
    repository root instead of "not configured"."""
    assert resolve_tool_exe(blank, tmp_path) is None


def test_resolve_tool_exe_rejects_a_directory(tmp_path):
    """exists() is true for directories; only a regular file is launchable."""
    (tmp_path / "nastran").mkdir()
    assert resolve_tool_exe("nastran", tmp_path) is None


def test_resolve_tool_exe_resolves_a_relative_file_against_root(tmp_path):
    exe = tmp_path / "bin" / "nastran.exe"
    exe.parent.mkdir()
    exe.write_text("")
    assert resolve_tool_exe("bin/nastran.exe", tmp_path) == exe


def test_resolve_tool_exe_honours_an_absolute_file(tmp_path):
    exe = tmp_path / "nastran.exe"
    exe.write_text("")
    assert (
        resolve_tool_exe(
            exe, Path("C:/unused") if sys.platform == "win32" else Path("/unused")
        )
        == exe
    )


def test_resolve_tool_exe_returns_none_for_a_missing_file(tmp_path):
    assert resolve_tool_exe("bin/nastran.exe", tmp_path) is None
