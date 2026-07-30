// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

package main

import (
	"os/exec"
	"syscall"
)

// configureSidecarProcAttr puts the sidecar in its own process group
// (Setpgid) so killSidecarProcessTree can signal it and every descendant it
// spawns (an MSES or SUAVE subprocess, say) in one call instead of only the
// immediate child, and sets Pdeathsig so the kernel kills it automatically
// if this process dies without ever running Stop() (a crash, `kill -9`,
// etc.) -- the closest Linux equivalent of the Windows Job Object's
// kill-on-job-close guarantee (jobobject_windows.go).
func configureSidecarProcAttr(cmd *exec.Cmd) {
	cmd.SysProcAttr = &syscall.SysProcAttr{
		Setpgid:   true,
		Pdeathsig: syscall.SIGKILL,
	}
}

// bindSidecarLifetime has nothing left to do here: configureSidecarProcAttr
// already set Pdeathsig before Start(), the only hook point Linux offers
// for this guarantee.
func bindSidecarLifetime(cmd *exec.Cmd) error {
	return nil
}

// killSidecarProcessTree signals the whole process group (negative pid),
// not just the sidecar's own pid, so a subprocess it spawned doesn't
// survive it as an orphan.
func killSidecarProcessTree(cmd *exec.Cmd) error {
	return syscall.Kill(-cmd.Process.Pid, syscall.SIGKILL)
}
