# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Golden generator for `alas.paths` (path resolution).

Generates golden fixtures recording data-root, tool-directory, and tool-executable
resolution behavior across candidate path layouts.
"""

from __future__ import annotations

import os
from pathlib import Path
import sys
import tempfile

import _framework

_framework.add_alas_to_path()

import alas.paths as paths  # noqa: E402


def main():
    with tempfile.TemporaryDirectory() as td:
        root = Path(td).resolve()
        (root / "bin").mkdir()
        (root / "data").mkdir()
        (root / "data/airports.json").write_text("{}", encoding="utf-8")
        (root / "external tools/MSES").mkdir(parents=True)
        (root / "external tools/MSES/mses.exe").write_text("bin", encoding="utf-8")
        (root / "external tools/suave_runner").mkdir(parents=True)

        paths._REPO_ROOT = root
        os.environ["ALAS_APP_DIR"] = str(root)

        test_paths = [
            "external tools/MSES",
            "external tools/suave_runner",
            "data/airports.json",
            "nonexistent/folder/here",
            "",
            "   ",
            str(root / "data"),
        ]

        def norm(p: Path | None) -> str | None:
            if p is None:
                return None
            try:
                rel = str(p.relative_to(root)).replace("\\", "/")
                return "." if rel == "" else rel
            except ValueError:
                return str(p).replace("\\", "/")

        tool_dir_results = {p: norm(paths.resolve_tool_dir(p)) for p in test_paths}
        find_tool_results = {p: norm(paths.find_tool_dir(p)) for p in test_paths}
        data_path_results = {p: norm(paths.resolve_data_path(p)) for p in test_paths}

        exe_tests = [
            "external tools/MSES/mses.exe",
            "external tools/MSES",
            "nonexistent/file.exe",
            "",
            "   ",
        ]
        exe_results = {p: norm(paths.resolve_tool_exe(p, root)) for p in exe_tests}

        payload = {
            "resolve_tool_dir": tool_dir_results,
            "find_tool_dir": find_tool_results,
            "resolve_data_path": data_path_results,
            "resolve_tool_exe": exe_results,
        }

        _framework.write(
            "app",
            "paths",
            payload,
            description="Path resolution outputs across standard candidate tools, data directories, and edge cases.",
        )


if __name__ == "__main__":
    main()
