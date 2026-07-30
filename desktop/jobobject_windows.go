// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

package main

import (
	"fmt"
	"os/exec"
	"unsafe"

	"golang.org/x/sys/windows"
)

// assignToKillOnCloseJob ties cmd's process lifetime to this one via a
// Windows Job Object with JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE.
//
// Without this, a child process spawned with exec.Command survives its
// parent's death on Windows -- there is no POSIX-style process-group signal
// propagation. That isn't just a theoretical concern: it's exactly what
// leaked several orphaned Python sidecar processes (each holding its own
// loopback port forever) during `wails dev`'s own hot-reload cycle, which
// kills and restarts the compiled app binary directly rather than going
// through App.shutdown/OnShutdown. Binding the sidecar to a
// kill-on-job-close Job Object means Windows itself terminates it the
// instant this process dies, gracefully or not -- the same guarantee
// SidecarManager.Stop() already provides for the graceful-exit path.
func assignToKillOnCloseJob(cmd *exec.Cmd) error {
	job, err := windows.CreateJobObject(nil, nil)
	if err != nil {
		return fmt.Errorf("CreateJobObject: %w", err)
	}

	info := windows.JOBOBJECT_EXTENDED_LIMIT_INFORMATION{
		BasicLimitInformation: windows.JOBOBJECT_BASIC_LIMIT_INFORMATION{
			LimitFlags: windows.JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
		},
	}
	if _, err := windows.SetInformationJobObject(
		job,
		windows.JobObjectExtendedLimitInformation,
		uintptr(unsafe.Pointer(&info)),
		uint32(unsafe.Sizeof(info)),
	); err != nil {
		windows.CloseHandle(job)
		return fmt.Errorf("SetInformationJobObject: %w", err)
	}

	procHandle, err := windows.OpenProcess(
		windows.PROCESS_SET_QUOTA|windows.PROCESS_TERMINATE,
		false,
		uint32(cmd.Process.Pid),
	)
	if err != nil {
		windows.CloseHandle(job)
		return fmt.Errorf("OpenProcess: %w", err)
	}
	defer windows.CloseHandle(procHandle)

	if err := windows.AssignProcessToJobObject(job, procHandle); err != nil {
		windows.CloseHandle(job)
		return fmt.Errorf("AssignProcessToJobObject: %w", err)
	}

	// Intentionally leak `job`: it must stay open for this process's entire
	// lifetime for JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE to fire on exit --
	// Windows closes it (and kills everything assigned to it) automatically
	// when this process terminates.
	return nil
}
