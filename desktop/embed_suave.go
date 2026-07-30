// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

package main

import "embed"

// embeddedSuave holds whatever scripts/build_suave_env.py staged into
// suave_dist/<goos>-<goarch>/ before this binary was compiled -- the
// self-contained SUAVE mission-analysis runtime (isolated Python 3.10 venv +
// SUAVE-2.5.2 source + our suave_runner wrapper), or just suave_dist/
// PLACEHOLDER.md during `wails dev`/a plain `go build`, since go:embed
// requires the pattern to match at least one file at compile time either
// way. suave_runtime.go checks which case it got at startup.
//
//go:embed all:suave_dist
var embeddedSuave embed.FS

const suaveDistRoot = "suave_dist"
