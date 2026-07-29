// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

package main

import (
	"os/exec"
	"syscall"

	"golang.org/x/sys/windows"
)

// configureSidecarProcAttr hides the child's console window. A production
// Wails build has no console of its own (it's a windowsgui-subsystem
// binary), so without this flag Windows allocates and flashes a new
// console window the instant a console-subsystem child (uv.exe, or the
// frozen alas-core.exe -- see sidecar.go) is spawned. Go still
// captures the child's stdout/stderr through anonymous pipes regardless of
// whether a console is attached, so hiding it costs nothing.
func configureSidecarProcAttr(cmd *exec.Cmd) {
	cmd.SysProcAttr = &syscall.SysProcAttr{CreationFlags: windows.CREATE_NO_WINDOW}
}

// bindSidecarLifetime ties the sidecar's lifetime to this process via the
// existing kill-on-job-close Job Object (jobobject_windows.go) -- see that
// file's comment for why a plain parent/child relationship isn't enough on
// Windows.
func bindSidecarLifetime(cmd *exec.Cmd) error {
	return assignToKillOnCloseJob(cmd)
}

// killSidecarProcessTree kills the sidecar. The Job Object bound in
// bindSidecarLifetime already guarantees every descendant (e.g. an MSES or
// SUAVE subprocess the sidecar itself spawned) dies alongside it, so
// killing just the immediate child is sufficient here.
func killSidecarProcessTree(cmd *exec.Cmd) error {
	return cmd.Process.Kill()
}
