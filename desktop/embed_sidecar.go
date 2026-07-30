// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

package main

import "embed"

// embeddedSidecar holds whatever scripts/build_sidecar.py staged into
// sidecar_dist/<goos>-<goarch>/ before this binary was compiled -- a real
// frozen alas-core(.exe) bundle after a `wails build` (see
// wails.json's "*/*" preBuildHooks entry), or just sidecar_dist/
// PLACEHOLDER.md during `wails dev`/a plain `go build`, since go:embed
// requires the pattern to match at least one file at compile time either
// way. sidecar.go checks which case it got at startup.
//
//go:embed all:sidecar_dist
var embeddedSidecar embed.FS

const sidecarDistRoot = "sidecar_dist"
