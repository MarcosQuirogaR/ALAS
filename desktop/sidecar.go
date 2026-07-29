// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

package main

import (
	"archive/zip"
	"bufio"
	"bytes"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"os/exec"
	"path"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"time"
)

// The frozen sidecar bundle is ~800MB of scientific-Python (scipy/VTK/
// casadi/pyNastran); cold interpreter startup plus antivirus scanning of a
// freshly-extracted binary can legitimately take well over a naive
// short timeout on an otherwise-successful launch. An unsigned,
// never-before-seen-on-this-machine binary of this size (e.g. a fresh
// from-the-site download) can trigger much slower cloud-lookup AV scanning
// than a locally-built one the AV already trusts, so the budget needs to be
// generous -- but still bounded so a truly broken sidecar fails loudly
// rather than hanging forever. If this window ever needs to change, change
// desktop/frontend/src/lib/sidecarClient.ts's READY_TIMEOUT_MS together with
// it (it must stay larger than sidecarPortTimeout+sidecarHealthTimeout with
// real margin -- extraction happens before either timeout starts counting,
// so it's additional unaccounted time on top).
const (
	sidecarPortTimeout   = 180 * time.Second
	sidecarHealthTimeout = 180 * time.Second
)

// SidecarManager owns the lifecycle of the Python sidecar process
// (alas/sidecar/server.py, see docs/architecture.md).
//
// Start prefers a frozen, PyInstaller-built alas-core(.exe) staged by
// scripts/build_sidecar.py and embedded via embed_sidecar.go -- the shape a
// `wails build` produces (wails.json's "*/*" preBuildHooks entry runs that
// script automatically). When no such build exists for the current
// GOOS/GOARCH (a plain `go build`, or `wails dev`, whose whole point is
// live-editing Python without a separate freeze step), it falls back to
// launching the sidecar straight from the repo's uv-managed venv via
// `uv run`. Both paths announce their port and pass a /healthz check the
// same way (alas/sidecar/server.py's stdout handshake), so nothing
// downstream of launch needs to know which mode is active.
type SidecarManager struct {
	mu   sync.Mutex
	cmd  *exec.Cmd
	port int

	// procDone is closed by the watcher goroutine Start() spawns once the
	// sidecar process has exited and its exit state is recorded in
	// procState. Startup waits select on it so a sidecar that dies
	// immediately (bad DLL, missing data file, crash in server.py's imports)
	// fails fast with the real exit code instead of burning the whole
	// port/health timeout budget. cmd.Wait() is called exactly once, by that
	// watcher -- Stop() must wait on procDone rather than calling Wait()
	// itself.
	procDone  chan struct{}
	procState *os.ProcessState
}

// repoRoot resolves the ALAS repo root from the desktop/ subdirectory
// this Go module lives in. Dev-mode-only: a frozen build never calls this,
// since it has no repo checkout to find.
func repoRoot() (string, error) {
	exeDir, err := os.Getwd()
	if err != nil {
		return "", err
	}
	// `wails dev`/`wails build` run with cwd == desktop/, so the repo root is
	// one level up. Fall back to walking upward looking for pyproject.toml in
	// case cwd is ever something else (e.g. invoked from repo root directly).
	candidate := filepath.Join(exeDir, "..")
	if _, err := os.Stat(filepath.Join(candidate, "pyproject.toml")); err == nil {
		return filepath.Abs(candidate)
	}
	dir := exeDir
	for i := 0; i < 6; i++ {
		if _, err := os.Stat(filepath.Join(dir, "pyproject.toml")); err == nil {
			return filepath.Abs(dir)
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			break
		}
		dir = parent
	}
	return "", fmt.Errorf("could not locate repo root (pyproject.toml) above %s", exeDir)
}

// sidecarBinaryName is the entry executable scripts/packaging/
// alas_sidecar.spec produces, per platform.
func sidecarBinaryName() string {
	if runtime.GOOS == "windows" {
		return "alas-core.exe"
	}
	return "alas-core"
}

// sidecarCacheRoot holds one subdirectory per fingerprinted sidecar build,
// reused across launches so a ~800MB extraction only happens once per build
// instead of on every startup.
//
// Deliberately os.UserCacheDir() (%LocalAppData% on Windows, ~/Library/Caches
// on macOS, $XDG_CACHE_HOME/~/.cache on Linux), NOT os.TempDir(): unpacking a
// large unsigned executable into the raw system temp directory and then
// running it from there is a well-known "dropper" behavioural pattern --
// Kaspersky's System Watcher (and most other behavioural AV engines) watch
// exactly this "process writes an .exe under Temp, then executes it, which
// spawns further child processes" chain, and reported flagging ALAS for
// it directly. A per-app subfolder under the OS's normal persistent
// per-user cache location is both a smaller/less suspicious behavioural
// footprint AND simply more correct: Temp is documented as transient
// (cleanable by the OS/disk-cleanup tools at any time), which was already a
// poor fit for a cache meant to persist across launches. Falls back to
// os.TempDir() only if the OS cache dir genuinely can't be resolved.
func sidecarCacheRoot() string {
	base, err := os.UserCacheDir()
	if err != nil {
		base = os.TempDir()
	}
	return filepath.Join(base, "ALAS", "sidecar-cache")
}

const (
	sidecarArchiveName     = "alas-core.zip"
	sidecarFingerprintName = "fingerprint.txt"
)

// extractEmbeddedSidecarIfPresent returns the path to the frozen sidecar's
// entry executable for the current GOOS/GOARCH, unpacking the embedded
// deflate-compressed archive (staged by scripts/build_sidecar.py) into
// sidecarCacheRoot() first if this build hasn't been extracted before. The
// build's identity comes from the fingerprint file computed at build time
// -- nothing large is hashed or walked at launch. Returns "" (no error) if
// no bundle was embedded -- the ordinary case in dev mode, where
// sidecar_dist/ holds only PLACEHOLDER.md (see embed_sidecar.go).
func extractEmbeddedSidecarIfPresent() (string, error) {
	binaryName := sidecarBinaryName()
	platformDir := path.Join(sidecarDistRoot, runtime.GOOS+"-"+runtime.GOARCH)

	fingerprintBytes, err := embeddedSidecar.ReadFile(path.Join(platformDir, sidecarFingerprintName))
	if err != nil {
		return "", nil // no bundle for this platform (dev mode)
	}
	fingerprint := strings.TrimSpace(string(fingerprintBytes))
	if fingerprint == "" {
		return "", fmt.Errorf("embedded sidecar fingerprint file is empty")
	}

	cacheRoot := sidecarCacheRoot()
	cacheDir := filepath.Join(cacheRoot, fingerprint)
	binaryPath := filepath.Join(cacheDir, binaryName)

	if _, err := os.Stat(binaryPath); err == nil {
		return binaryPath, nil
	}

	archiveBytes, err := embeddedSidecar.ReadFile(path.Join(platformDir, sidecarArchiveName))
	if err != nil {
		return "", fmt.Errorf("embedded sidecar archive missing (fingerprint present): %w", err)
	}

	// No cache hit: extract to a staging directory and rename it into place
	// atomically, so a launch that crashes or is killed mid-extraction never
	// leaves a partial cacheDir that a later launch would mistake for a
	// complete one.
	if err := os.MkdirAll(cacheRoot, 0o755); err != nil {
		return "", fmt.Errorf("creating sidecar cache root: %w", err)
	}
	stagingDir, err := os.MkdirTemp(cacheRoot, "staging-*")
	if err != nil {
		return "", fmt.Errorf("creating sidecar extraction dir: %w", err)
	}

	if err := extractZip(archiveBytes, stagingDir); err != nil {
		_ = os.RemoveAll(stagingDir)
		return "", fmt.Errorf("extracting embedded sidecar: %w", err)
	}

	if runtime.GOOS != "windows" {
		// The zip (written with forward-slash paths, no mode bits relied on)
		// loses Unix executable bits; PyInstaller's other bundled files
		// (shared libs, data) are only ever dlopen'd or read, never exec'd
		// directly, so only the launcher needs this.
		if err := os.Chmod(filepath.Join(stagingDir, binaryName), 0o755); err != nil {
			_ = os.RemoveAll(stagingDir)
			return "", fmt.Errorf("marking extracted sidecar executable: %w", err)
		}
	}

	if err := os.Rename(stagingDir, cacheDir); err != nil {
		// Losing race against a concurrent launch that already populated
		// cacheDir is fine -- use what's there and discard our staging copy.
		if _, statErr := os.Stat(binaryPath); statErr == nil {
			_ = os.RemoveAll(stagingDir)
			return binaryPath, nil
		}
		_ = os.RemoveAll(stagingDir)
		return "", fmt.Errorf("placing extracted sidecar into cache: %w", err)
	}

	pruneStaleSidecarCaches(cacheRoot, fingerprint)

	return binaryPath, nil
}

// pruneStaleSidecarCaches removes every cached extraction under cacheRoot
// except the current build's, so app updates don't accumulate multiple
// ~800MB copies in the temp directory forever. Best-effort: a failure here
// (e.g. a file still locked from a just-exited sidecar) is logged, not
// fatal -- it just means cleanup is retried on the next launch.
func pruneStaleSidecarCaches(cacheRoot, keepFingerprint string) {
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
			log.Println("sidecar: could not prune stale cache entry:", name, err)
		}
	}
}

// extractZip unpacks a zip archive held in memory into destDir. Entry paths
// are validated to stay inside destDir (zip-slip); the archive is written by
// our own build script, but the check is cheap insurance.
func extractZip(archive []byte, destDir string) error {
	zr, err := zip.NewReader(bytes.NewReader(archive), int64(len(archive)))
	if err != nil {
		return err
	}
	destRoot := filepath.Clean(destDir) + string(os.PathSeparator)
	for _, entry := range zr.File {
		target := filepath.Join(destDir, filepath.FromSlash(entry.Name))
		if !strings.HasPrefix(target, destRoot) {
			return fmt.Errorf("zip entry escapes extraction dir: %s", entry.Name)
		}
		if entry.FileInfo().IsDir() {
			if err := os.MkdirAll(target, 0o755); err != nil {
				return err
			}
			continue
		}
		if err := os.MkdirAll(filepath.Dir(target), 0o755); err != nil {
			return err
		}
		src, err := entry.Open()
		if err != nil {
			return err
		}
		dst, err := os.OpenFile(target, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0o644)
		if err != nil {
			src.Close()
			return err
		}
		_, copyErr := io.Copy(dst, src)
		src.Close()
		if closeErr := dst.Close(); copyErr == nil {
			copyErr = closeErr
		}
		if copyErr != nil {
			return copyErr
		}
	}
	return nil
}

// buildCommand picks the frozen embedded sidecar if one was staged for this
// platform, otherwise falls back to the dev `uv run` invocation.
func (s *SidecarManager) buildCommand() (*exec.Cmd, error) {
	binaryPath, err := extractEmbeddedSidecarIfPresent()
	if err != nil {
		return nil, err
	}
	if binaryPath != "" {
		cmd := exec.Command(binaryPath)
		// Set the working directory to the bundle root so the PyInstaller
		// bootloader's LoadLibrary("python313.dll") call can find it and its
		// _internal/ sibling DLLs. Without this, the child inherits the
		// Wails exe's working directory (build/bin/), which is not on the
		// DLL search path, causing [PYI-XXXXX:ERROR] Failed to load Python DLL.
		cmd.Dir = filepath.Dir(binaryPath)
		// The frozen sidecar runs from a temp extraction cache, so it cannot
		// derive the real install directory from its own location. Hand it the
		// Wails app-exe directory explicitly: alas/paths.py reads
		// ALAS_APP_DIR to locate externally-provisioned tools ("external
		// tools/MSES", the SUAVE venv/runner) the user drops beside the app.
		cmd.Env = sidecarEnv(appInstallDir())
		addSuaveEnv(&cmd.Env)
		return cmd, nil
	}

	root, err := repoRoot()
	if err != nil {
		return nil, err
	}
	cmd := exec.Command("uv", "run", "--project", root, "python", "-m", "alas.sidecar.server")
	cmd.Dir = root
	// Dev mode: the repo root IS the app root (tools live in the checkout).
	cmd.Env = sidecarEnv(root)
	addSuaveEnv(&cmd.Env)
	return cmd, nil
}

// addSuaveEnv extends env with ALAS_SUAVE_VENV_DIR/ALAS_SUAVE_RUNNER_DIR
// if a bundled SUAVE runtime was embedded and successfully extracted (see
// suave_runtime.go). A no-op (env left untouched) in dev mode or on a
// platform with no bundled runtime -- alas/pipeline.py falls back to its
// normal alas/paths.py-based resolution in that case, exactly like
// today. Extraction failures are logged, not fatal to sidecar startup: SUAVE
// mission analysis degrading to "not_configured" is far better than the whole
// app failing to launch over a corrupted embedded archive.
func addSuaveEnv(env *[]string) {
	venvDir, runnerDir, err := extractEmbeddedSuaveIfPresent()
	if err != nil {
		// Also hand the reason to the sidecar. Logging alone made this failure
		// effectively invisible: Go's log goes nowhere the user can see, so a
		// failed extraction surfaced only as a bare "mission: not_configured"
		// with no way to tell it apart from "SUAVE was never set up". The
		// sidecar folds this into the mission error message instead.
		log.Println("suave: could not extract bundled runtime:", err)
		*env = append(*env, "ALAS_SUAVE_EXTRACT_ERROR="+err.Error())
		return
	}
	if venvDir == "" {
		return // no bundle embedded for this platform (dev mode)
	}
	*env = append(*env, "ALAS_SUAVE_VENV_DIR="+venvDir, "ALAS_SUAVE_RUNNER_DIR="+runnerDir)
}

// appInstallDir is the directory containing the running Wails app executable --
// where a packaged install keeps its "external tools/" folder beside the exe.
// Falls back to the process working directory if the exe path can't be resolved.
func appInstallDir() string {
	exe, err := os.Executable()
	if err != nil {
		if wd, wdErr := os.Getwd(); wdErr == nil {
			return wd
		}
		return ""
	}
	return filepath.Dir(exe)
}

// sidecarEnv returns the parent environment with ALAS_APP_DIR set to
// appDir (see alas/paths.py). An empty appDir leaves the env untouched so
// the Python side falls back to its own detection.
func sidecarEnv(appDir string) []string {
	env := os.Environ()
	if appDir != "" {
		env = append(env, "ALAS_APP_DIR="+appDir)
	}
	return env
}

// Start launches the sidecar and blocks until it has announced its port and
// responded to a /healthz check, or the timeout elapses.
func (s *SidecarManager) Start() error {
	cmd, err := s.buildCommand()
	if err != nil {
		return err
	}

	// Must be set before Start(): on Windows this suppresses the console
	// window a console-subsystem child would otherwise flash; on
	// Linux/Unix it groups the sidecar (and anything it itself spawns, e.g.
	// an MSES or SUAVE run) so it can be torn down as a unit. See
	// procattrs_windows.go / procattrs_linux.go / procattrs_other.go.
	configureSidecarProcAttr(cmd)

	if cmd.Dir != "" {
		log.Printf("sidecar: launching %s\n", cmd.Dir)
	}

	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return err
	}
	stderr, err := cmd.StderrPipe()
	if err != nil {
		return err
	}

	if err := cmd.Start(); err != nil {
		return fmt.Errorf("starting sidecar: %w", err)
	}

	// Best-effort: if this fails (e.g. running as a user without the rights
	// to open the process handle on Windows), the sidecar still works, it
	// just loses the guarantee that an ungraceful parent death also kills
	// it -- so a failure here is logged, not fatal to startup.
	if err := bindSidecarLifetime(cmd); err != nil {
		log.Println("sidecar: could not bind lifetime to parent process:", err)
	}

	procDone := make(chan struct{})
	s.mu.Lock()
	s.cmd = cmd
	s.procDone = procDone
	s.mu.Unlock()

	portCh := make(chan int, 1)
	var pipesDrained sync.WaitGroup
	pipesDrained.Add(2)
	go func() {
		defer pipesDrained.Done()
		s.scanPort(stdout, portCh)
	}()
	go func() {
		defer pipesDrained.Done()
		s.drainLog("sidecar[stderr]", stderr)
	}()

	// Single Wait() owner: fires only after both pipe readers hit EOF (Wait
	// closes the pipes, so calling it earlier could truncate their output).
	go func() {
		pipesDrained.Wait()
		err := cmd.Wait()
		s.mu.Lock()
		s.procState = cmd.ProcessState
		s.mu.Unlock()
		if err != nil {
			log.Println("sidecar: process exited:", err)
		}
		close(procDone)
	}()

	select {
	case port := <-portCh:
		s.mu.Lock()
		s.port = port
		s.mu.Unlock()
	case <-procDone:
		return fmt.Errorf("sidecar exited before announcing a port (%s)", s.exitStateString())
	case <-time.After(sidecarPortTimeout):
		return fmt.Errorf("sidecar did not announce a port within %s", sidecarPortTimeout)
	}

	return s.waitHealthy(sidecarHealthTimeout)
}

// exitStateString renders the recorded exit state, if the watcher has stored
// one yet.
func (s *SidecarManager) exitStateString() string {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.procState == nil {
		return "exit state unknown"
	}
	return s.procState.String()
}

// scanPort reads the sidecar's stdout looking for the one-line
// "ALAS_PORT=<n>" handshake (see alas/sidecar/server.py), then
// keeps draining the rest of stdout so the child process's pipe never fills
// and blocks it. Identical for the frozen binary and the dev `uv run`
// process -- both ultimately run the same server.py entrypoint.
func (s *SidecarManager) scanPort(r io.Reader, portCh chan<- int) {
	// Startup-debugging aid, left commented out for normal use: mirrors the
	// sidecar's stdout to a file next to the app exe, so a double-clicked
	// (console-less) launch still leaves evidence. Re-enable by uncommenting
	// here and in drainLog/waitHealthy if startup ever needs diagnosing.
	// var logFile *os.File
	// exePath, err := os.Executable()
	// if err == nil {
	// 	logPath := filepath.Join(filepath.Dir(exePath), "sidecar-stdout.log")
	// 	logFile, _ = os.OpenFile(logPath, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0666)
	// }
	// if logFile != nil {
	// 	defer logFile.Close()
	// }

	scanner := bufio.NewScanner(r)
	found := false
	for scanner.Scan() {
		line := scanner.Text()
		// if logFile != nil {
		// 	fmt.Fprintln(logFile, line)
		// }
		if !found {
			if after, ok := strings.CutPrefix(line, "ALAS_PORT="); ok {
				if port, err := strconv.Atoi(strings.TrimSpace(after)); err == nil {
					portCh <- port
					found = true
					continue
				}
			}
		}
		log.Println("sidecar[stdout]:", line)
	}
}

func (s *SidecarManager) drainLog(prefix string, r io.Reader) {
	// Startup-debugging aid, disabled -- see scanPort for the rationale and
	// how to re-enable.
	// var logFile *os.File
	// exePath, err := os.Executable()
	// if err == nil {
	// 	logPath := filepath.Join(filepath.Dir(exePath), "sidecar-stderr.log")
	// 	logFile, _ = os.OpenFile(logPath, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0666)
	// }
	// if logFile != nil {
	// 	defer logFile.Close()
	// }

	scanner := bufio.NewScanner(r)
	for scanner.Scan() {
		line := scanner.Text()
		// if logFile != nil {
		// 	fmt.Fprintln(logFile, line)
		// }
		log.Println(prefix+":", line)
	}
}

func (s *SidecarManager) waitHealthy(timeout time.Duration) error {
	deadline := time.Now().Add(timeout)
	url := fmt.Sprintf("http://127.0.0.1:%d/healthz", s.Port())
	client := &http.Client{Timeout: 2 * time.Second}

	// Startup-debugging aid, disabled -- see scanPort for the rationale and
	// how to re-enable.
	// var healthLog *os.File
	// if exePath, err := os.Executable(); err == nil {
	// 	logPath := filepath.Join(filepath.Dir(exePath), "sidecar-health.log")
	// 	healthLog, _ = os.OpenFile(logPath, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0666)
	// }
	// if healthLog != nil {
	// 	defer healthLog.Close()
	// 	fmt.Fprintf(healthLog, "waitHealthy: polling %s\n", url)
	// }

	for time.Now().Before(deadline) {
		resp, err := client.Get(url)
		if err == nil {
			resp.Body.Close()
			// if healthLog != nil {
			// 	fmt.Fprintf(healthLog, "attempt: HTTP %d\n", resp.StatusCode)
			// }
			if resp.StatusCode == http.StatusOK {
				return nil
			}
		}
		// } else if healthLog != nil {
		// 	fmt.Fprintf(healthLog, "attempt error: %v\n", err)
		// }

		// Fail fast if the process died instead of polling out the clock.
		// (The pre-procDone version of this check read cmd.ProcessState
		// directly, which nothing had populated -- Wait() hadn't been
		// called -- so it could never fire.)
		s.mu.Lock()
		procDone := s.procDone
		s.mu.Unlock()
		if procDone != nil {
			select {
			case <-procDone:
				return fmt.Errorf("sidecar process exited (%s) before becoming healthy", s.exitStateString())
			default:
			}
		}

		time.Sleep(200 * time.Millisecond)
	}
	return fmt.Errorf("sidecar did not become healthy within %s", timeout)
}

// Port returns the loopback port the sidecar is listening on. Zero before
// Start() completes.
func (s *SidecarManager) Port() int {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.port
}

// Stop terminates the sidecar process. Safe to call multiple times. The
// extracted frozen binary (see sidecarCacheRoot) is deliberately left in
// place -- it's reused on the next launch, and pruneStaleSidecarCaches
// keeps it from accumulating across app updates.
func (s *SidecarManager) Stop() {
	s.mu.Lock()
	cmd := s.cmd
	s.cmd = nil
	procDone := s.procDone
	s.procDone = nil
	s.mu.Unlock()

	if cmd != nil && cmd.Process != nil {
		if err := killSidecarProcessTree(cmd); err != nil {
			log.Println("sidecar: kill failed:", err)
		}
		// The watcher goroutine owns cmd.Wait() (calling it here too would
		// race it); wait for it to observe the death, bounded so a wedged
		// child can never block app shutdown.
		if procDone != nil {
			select {
			case <-procDone:
			case <-time.After(10 * time.Second):
				log.Println("sidecar: process did not exit within 10s of kill")
			}
		}
	}
}
