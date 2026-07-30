// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

//go:build !windows && !linux

package main

import (
	"os/exec"
	"syscall"
)

// configureSidecarProcAttr puts the sidecar in its own process group so
// killSidecarProcessTree can signal it and any descendants together.
// Pdeathsig (used on Linux, see procattrs_linux.go) is a Linux-only field
// in the syscall package, so this fallback build -- macOS/BSD -- doesn't
// get the "auto-die if this process crashes without calling Stop()"
// guarantee, only the group-kill on a clean Stop().
func configureSidecarProcAttr(cmd *exec.Cmd) {
	cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true}
}

func bindSidecarLifetime(cmd *exec.Cmd) error {
	return nil
}

func killSidecarProcessTree(cmd *exec.Cmd) error {
	return syscall.Kill(-cmd.Process.Pid, syscall.SIGKILL)
}
