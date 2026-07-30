// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

package main

import (
	"context"
	"log"
	"sync"
)

// App struct
type App struct {
	ctx     context.Context
	sidecar *SidecarManager

	// startMu serializes startSidecar() calls -- both the initial OnStartup
	// call and any user-triggered RetrySidecar() go through it, so a slow
	// first attempt that's still extracting/booting can never race a retry
	// into starting a second sidecar process concurrently.
	startMu sync.Mutex
	lastErr string
}

// NewApp creates a new App application struct
func NewApp() *App {
	return &App{sidecar: &SidecarManager{}}
}

// startup is called when the app starts. The context is saved so we can call
// the runtime methods, and the Python sidecar (alas/sidecar/server.py)
// is started here so it's already warm by the time the frontend makes its
// first request.
func (a *App) startup(ctx context.Context) {
	a.ctx = ctx
	a.startSidecar()
}

// startSidecar runs (or re-runs) SidecarManager.Start() and records the
// outcome. log.Println alone is invisible on a double-clicked, console-less
// .exe, so the failure is also stashed in lastErr; GetSidecarError() below
// lets the Splash screen show the real reason instead of just "timed out
// after 240s".
func (a *App) startSidecar() {
	a.startMu.Lock()
	defer a.startMu.Unlock()
	if err := a.sidecar.Start(); err != nil {
		log.Println("sidecar failed to start:", err)
		a.lastErr = err.Error()
	} else {
		a.lastErr = ""
	}
}

// shutdown is called when the app is closing, so the sidecar process never
// outlives the window it was serving.
func (a *App) shutdown(ctx context.Context) {
	a.sidecar.Stop()
}

// GetSidecarPort returns the loopback port the Python sidecar is listening
// on, or 0 if it failed to start. The frontend talks to
// http://127.0.0.1:<port> directly (see docs/architecture.md's migration
// plan) rather than proxying every request through Go.
func (a *App) GetSidecarPort() int {
	return a.sidecar.Port()
}

// GetSidecarError returns the message from the most recent failed start
// attempt, or "" if the sidecar is up (or hasn't finished its first attempt
// yet). Polled by the frontend alongside GetSidecarPort so a real failure
// (bad DLL, missing embedded bundle, port never announced) surfaces
// immediately instead of only after the frontend's own poll budget expires.
func (a *App) GetSidecarError() string {
	a.startMu.Lock()
	defer a.startMu.Unlock()
	return a.lastErr
}

// RetrySidecar re-attempts the sidecar start from scratch (stopping any
// half-started process first), so the Splash screen's "Retry" button can
// recover from a failed startup() attempt without the user relaunching the
// whole app.
func (a *App) RetrySidecar() {
	a.sidecar.Stop()
	a.startSidecar()
}

// ReportBridgeCheckResult lets the frontend's Phase 1 verification page
// (App.tsx) surface its end-to-end bridge-check outcome into this process's
// own stdout, since a Wails webview window has no console visible to
// whoever launched `wails dev`. Temporary scaffolding for this phase, not a
// permanent part of the app's API surface.
func (a *App) ReportBridgeCheckResult(passed bool, details string) {
	log.Println("=== BRIDGE CHECK RESULT ===")
	log.Println("passed:", passed)
	log.Println(details)
	log.Println("=== END BRIDGE CHECK RESULT ===")
}

// LogFrontendEvent is a general-purpose escape hatch for the verification
// page to surface arbitrary checkpoints (e.g. Phase 3's schema/preset/
// design-space load, validation results) into this process's own stdout,
// the same reasoning as ReportBridgeCheckResult above but not scoped to
// pass/fail bridge semantics. Temporary scaffolding for this phase, not a
// permanent part of the app's API surface.
func (a *App) LogFrontendEvent(label string, details string) {
	log.Printf("=== FRONTEND EVENT: %s ===\n", label)
	log.Println(details)
	log.Printf("=== END FRONTEND EVENT: %s ===\n", label)
}
