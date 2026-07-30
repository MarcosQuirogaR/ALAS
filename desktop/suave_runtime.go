// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

package main

import (
	"fmt"
	"log"
	"os"
	"path"
	"path/filepath"
	"runtime"
	"strings"
)

// suaveCacheRoot holds one subdirectory per fingerprinted SUAVE-runtime
// build, reused across launches so the ~150-300MB extraction only happens
// once per build instead of on every startup -- same pattern (and same
// os.UserCacheDir()-not-os.TempDir() rationale -- avoiding the "unpack an
// executable into Temp, then run it" behavioural-AV trigger, see that
// comment) as sidecarCacheRoot (sidecar.go).
func suaveCacheRoot() string {
	base, err := os.UserCacheDir()
	if err != nil {
		base = os.TempDir()
	}
	return filepath.Join(base, "ALAS", "suave-cache")
}

const (
	suaveArchiveName     = "alas-suave.zip"
	suaveFingerprintName = "fingerprint.txt"
)

// extractEmbeddedSuaveIfPresent returns (venvDir, runnerDir, err) for the
// bundled SUAVE mission-analysis runtime staged by scripts/build_suave_env.py
// (isolated Python 3.10 venv + SUAVE-2.5.2 source + our suave_runner
// wrapper), unpacking the embedded archive into suaveCacheRoot() first if
// this build hasn't been extracted before. The archive is flat at the root
// (suave-venv/, suave_runner/, SUAVE-2.5.2/ as siblings) so suave_runner/
// _compat.py's own sibling-relative sys.path lookup keeps working unmodified
// after extraction -- see build_suave_env.py's module docstring.
//
// Returns ("", "", nil) -- no error -- if no bundle was embedded for this
// platform (the ordinary case in dev mode, where suave_dist/ holds only
// PLACEHOLDER.md); callers fall back to alas/paths.py's own resolution
// in that case, exactly like a missing embedded sidecar falls back to `uv run`.
func extractEmbeddedSuaveIfPresent() (string, string, error) {
	platformDir := path.Join(suaveDistRoot, runtime.GOOS+"-"+runtime.GOARCH)

	fingerprintBytes, err := embeddedSuave.ReadFile(path.Join(platformDir, suaveFingerprintName))
	if err != nil {
		return "", "", nil // no bundle for this platform (dev mode)
	}
	fingerprint := strings.TrimSpace(string(fingerprintBytes))
	if fingerprint == "" {
		return "", "", fmt.Errorf("embedded SUAVE fingerprint file is empty")
	}

	cacheRoot := suaveCacheRoot()
	cacheDir := filepath.Join(cacheRoot, fingerprint)
	venvDir := filepath.Join(cacheDir, "suave-venv")
	runnerDir := filepath.Join(cacheDir, "suave_runner")

	if _, err := os.Stat(venvDir); err == nil {
		return venvDir, runnerDir, nil
	}

	archiveBytes, err := embeddedSuave.ReadFile(path.Join(platformDir, suaveArchiveName))
	if err != nil {
		return "", "", fmt.Errorf("embedded SUAVE archive missing (fingerprint present): %w", err)
	}

	// No cache hit: extract to a staging directory and rename it into place
	// atomically -- same crash-mid-extraction safety as the sidecar's own
	// extraction (see sidecar.go's extractEmbeddedSidecarIfPresent).
	if err := os.MkdirAll(cacheRoot, 0o755); err != nil {
		return "", "", fmt.Errorf("creating SUAVE cache root: %w", err)
	}
	stagingDir, err := os.MkdirTemp(cacheRoot, "staging-*")
	if err != nil {
		return "", "", fmt.Errorf("creating SUAVE extraction dir: %w", err)
	}

	// extractZip is defined in sidecar.go (same package) -- reused verbatim.
	if err := extractZip(archiveBytes, stagingDir); err != nil {
		_ = os.RemoveAll(stagingDir)
		return "", "", fmt.Errorf("extracting embedded SUAVE runtime: %w", err)
	}

	if runtime.GOOS != "windows" {
		pythonBin := filepath.Join(stagingDir, "suave-venv", "bin", "python")
		if _, statErr := os.Stat(pythonBin); statErr == nil {
			if err := os.Chmod(pythonBin, 0o755); err != nil {
				_ = os.RemoveAll(stagingDir)
				return "", "", fmt.Errorf("marking extracted SUAVE interpreter executable: %w", err)
			}
		}
	}

	if err := os.Rename(stagingDir, cacheDir); err != nil {
		// Losing race against a concurrent launch that already populated
		// cacheDir is fine -- use what's there and discard our staging copy.
		if _, statErr := os.Stat(venvDir); statErr == nil {
			_ = os.RemoveAll(stagingDir)
			return venvDir, runnerDir, nil
		}
		_ = os.RemoveAll(stagingDir)
		return "", "", fmt.Errorf("placing extracted SUAVE runtime into cache: %w", err)
	}

	pruneStaleSuaveCaches(cacheRoot, fingerprint)

	return venvDir, runnerDir, nil
}

// pruneStaleSuaveCaches mirrors pruneStaleSidecarCaches (sidecar.go): keeps
// only the current build's extracted SUAVE runtime so app updates don't
// accumulate multiple copies in the temp directory forever. Best-effort.
func pruneStaleSuaveCaches(cacheRoot, keepFingerprint string) {
	entries, err := os.ReadDir(cacheRoot)
	if err != nil {
		return
	}
	for _, entry := range entries {
		name := entry.Name()
		if name == keepFingerprint || strings.HasPrefix(name, "staging-") {
			continue
		}
		if err := os.RemoveAll(filepath.Join(cacheRoot, name)); err != nil {
			log.Println("suave: could not prune stale cache entry:", name, err)
		}
	}
}
