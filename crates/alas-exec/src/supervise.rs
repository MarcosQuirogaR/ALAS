// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Lifetime supervision for the external solvers ALAS launches.
//!
//! [`kill_process_tree`](crate::process::kill_process_tree) covers the case
//! where ALAS is still alive to issue the kill: a timeout, a cancellation. It
//! cannot cover the case where ALAS itself is gone first. A user who ends the
//! application from Task Manager, a crash, or a forced close from the shell
//! all leave whatever `avl`, `vspaero`, `mses` or `nastran` was running as an
//! orphan that keeps burning CPU and holding its working directory, because
//! nothing is left to run the cleanup.
//!
//! On Windows the kernel can own that cleanup instead. ALAS creates one
//! anonymous Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` and assigns
//! every process it launches through [`SupervisedSpawn::spawn_supervised`] to
//! it. Processes those children start (the NASTRAN launcher's real solver, a
//! `cmd` wrapper's payload) join the job automatically. The job handle lives
//! for the life of the ALAS process and is never closed by code; when the
//! process ends, however it ends, the kernel closes its last handle and
//! terminates every process still in the job. Microsoft documents this
//! behaviour for the flag and for nested jobs on Windows 8 and later.
//!
//! What this does *not* do, so nobody reads more into it:
//!
//! * It does not change how Task Manager groups rows. Task Manager groups a
//!   windowless child under the application that is its parent; that grouping
//!   is by parent process, and these children already qualify because they
//!   are direct children spawned without a console. The job is invisible to
//!   Task Manager's UI.
//! * It does not rename third-party binaries. `vspaero.exe` keeps its own
//!   file description; ALAS records which task launched it in the ledger
//!   [`recent_launches`] returns, keyed by PID.
//! * It does not cover a process launched for the user to keep, such as a
//!   viewer opened on a result. Those go through plain `spawn` and are not
//!   ALAS-owned.
//! * There is a window between `CreateProcess` returning and the assignment
//!   call. A child that forks in those microseconds leaves a grandchild
//!   outside the job; the tree kill at timeout still reaches it, the
//!   kill-on-close does not. Narrow, and recorded here rather than hidden.
//!
//! On other platforms there is no job object; [`supervision_status`] says so,
//! and the process-group kill in `process` remains the whole story.

use std::collections::VecDeque;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;

/// Whether owned processes are placed under kernel-enforced supervision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisionStatus {
    /// The job object exists; every supervised spawn is assigned to it.
    Active,
    /// This platform has no job objects. Process groups and tree kills still
    /// apply, but nothing outlives ALAS to clean up after a forced exit.
    Unsupported,
    /// The job object could not be created; the OS error text is kept so a
    /// diagnostic can show it rather than a bare "no".
    Unavailable(String),
}

/// One launch ALAS owns: the PID and the task that started it.
///
/// This is what ties a row in Task Manager, which shows the vendor's name for
/// the executable, to the ALAS analysis that is actually running it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchRecord {
    /// Position in the launch order, starting at 1; a reader that has shown
    /// the ledger up to some sequence asks for what came after it.
    pub sequence: u64,
    /// Process identifier the OS assigned to the child.
    pub pid: u32,
    /// The ALAS task that launched it, such as `AVL sweep` or `MSES point`.
    pub role: String,
    /// The program as the spawn named it.
    pub program: String,
    /// When the spawn returned.
    pub started: SystemTime,
    /// True when the child was assigned to the ALAS job object.
    pub supervised: bool,
}

/// How many launches the ledger remembers; an airfoil sweep spawns hundreds,
/// and the point of the ledger is the recent ones a user is looking at.
const LEDGER_CAPACITY: usize = 256;

static LEDGER: Mutex<VecDeque<LaunchRecord>> = Mutex::new(VecDeque::new());
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn record(mut entry: LaunchRecord) {
    let mut ledger = LEDGER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    entry.sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed) + 1;
    if ledger.len() >= LEDGER_CAPACITY {
        ledger.pop_front();
    }
    ledger.push_back(entry);
}

/// The most recent launches ALAS owns, oldest first.
pub fn recent_launches() -> Vec<LaunchRecord> {
    LEDGER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .cloned()
        .collect()
}

/// The launches recorded after `sequence`, oldest first.
///
/// A run log that has shown everything up to a record passes that record's
/// `sequence` back and receives only what is new; `0` asks for everything the
/// ledger still holds.
pub fn launches_after(sequence: u64) -> Vec<LaunchRecord> {
    LEDGER
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .filter(|entry| entry.sequence > sequence)
        .cloned()
        .collect()
}

/// Whether the ALAS job object is in force for this process.
///
/// Creating the job is lazy and shared with the first supervised spawn, so
/// asking is the same as arming it.
pub fn supervision_status() -> SupervisionStatus {
    platform::status()
}

/// Spawns a [`Command`] as a process ALAS owns for its whole life.
pub trait SupervisedSpawn {
    /// Spawn, then place the child under the ALAS job object and record the
    /// launch in the ledger under `role`.
    ///
    /// Spawning is the only step that can fail: a child that is running but
    /// could not be assigned is still a running child, so the assignment is
    /// best effort and its outcome is recorded rather than returned. The
    /// timeout and cancellation kills at the call sites do not depend on it.
    fn spawn_supervised(&mut self, role: &str) -> std::io::Result<Child>;
}

impl SupervisedSpawn for Command {
    fn spawn_supervised(&mut self, role: &str) -> std::io::Result<Child> {
        let program = self.get_program().to_string_lossy().into_owned();
        let child = self.spawn()?;
        let supervised = platform::assign(&child).is_ok();
        record(LaunchRecord {
            sequence: 0,
            pid: child.id(),
            role: role.to_owned(),
            program,
            started: SystemTime::now(),
            supervised,
        });
        Ok(child)
    }
}

#[cfg(windows)]
mod platform {
    use super::SupervisionStatus;
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;
    use std::sync::OnceLock;
    use win32job::{ExtendedLimitInfo, Job};

    /// The one job for the life of the process. Never dropped by code: closing
    /// its handle is what kills the members, and the kernel does that when
    /// the process ends.
    static JOB: OnceLock<Result<Job, String>> = OnceLock::new();

    fn job() -> Result<&'static Job, String> {
        JOB.get_or_init(create).as_ref().map_err(Clone::clone)
    }

    fn create() -> Result<Job, String> {
        let mut limits = ExtendedLimitInfo::new();
        limits.limit_kill_on_job_close();
        Job::create_with_limit_info(&limits).map_err(|error| error.to_string())
    }

    pub(super) fn status() -> SupervisionStatus {
        match job() {
            Ok(_) => SupervisionStatus::Active,
            Err(reason) => SupervisionStatus::Unavailable(reason),
        }
    }

    pub(super) fn assign(child: &Child) -> Result<(), String> {
        // The raw handle is the pointer-sized value the OS knows the process
        // by; `win32job` takes it as an integer. Nothing changes hands: `std`
        // still owns the handle and closes it when `child` is dropped.
        let handle = child.as_raw_handle() as isize;
        job()?
            .assign_process(handle)
            .map_err(|error| error.to_string())
    }

    /// Whether `pid` is a member of the ALAS job right now.
    #[cfg(test)]
    pub(super) fn contains(pid: u32) -> bool {
        job()
            .ok()
            .and_then(|job| job.query_process_id_list().ok())
            .is_some_and(|members| members.contains(&(pid as usize)))
    }
}

#[cfg(not(windows))]
mod platform {
    use super::SupervisionStatus;
    use std::process::Child;

    pub(super) fn status() -> SupervisionStatus {
        SupervisionStatus::Unsupported
    }

    pub(super) fn assign(_child: &Child) -> Result<(), String> {
        Err("job objects exist only on Windows".to_owned())
    }
}

// The tests spawn real processes; a failed expect is the test host lacking a
// shell or a job object, not a library invariant being broken.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::{kill_process_tree, NoConsoleWindow};
    use std::time::{Duration, Instant};

    /// A child that would run for a minute if nothing stopped it.
    fn long_running_command() -> Command {
        let mut command = if cfg!(windows) {
            let mut command = Command::new("cmd");
            command.args(["/C", "ping -n 60 127.0.0.1 > NUL"]);
            command
        } else {
            let mut command = Command::new("sh");
            command.args(["-c", "sleep 60"]);
            command
        };
        command.no_window();
        command
    }

    #[test]
    fn supervision_status_matches_the_platform() {
        let expected = if cfg!(windows) {
            SupervisionStatus::Active
        } else {
            SupervisionStatus::Unsupported
        };
        assert_eq!(supervision_status(), expected);
    }

    #[test]
    fn a_supervised_spawn_is_recorded_under_its_role() {
        let mut child = long_running_command()
            .spawn_supervised("ledger probe")
            .expect("the OS shell is present in any environment that runs these tests");
        let pid = child.id();
        let entry = recent_launches()
            .into_iter()
            .find(|entry| entry.pid == pid)
            .expect("a supervised spawn is always recorded");
        assert_eq!(entry.role, "ledger probe");
        assert_eq!(entry.supervised, cfg!(windows));
        assert!(entry
            .program
            .contains(if cfg!(windows) { "cmd" } else { "sh" }));
        assert!(entry.sequence >= 1);
        // A reader that has seen this record is not shown it again; anything
        // it is shown came later.
        assert!(launches_after(entry.sequence)
            .iter()
            .all(|later| later.sequence > entry.sequence));
        assert!(launches_after(entry.sequence - 1)
            .iter()
            .any(|shown| shown.pid == pid));
        kill_process_tree(pid);
        let _ = child.wait();
    }

    #[cfg(windows)]
    #[test]
    fn a_supervised_child_is_a_member_of_the_alas_job() {
        let mut child = long_running_command()
            .spawn_supervised("membership probe")
            .expect("the OS shell is present in any environment that runs these tests");
        assert!(platform::contains(child.id()));
        kill_process_tree(child.id());
        let _ = child.wait();
    }

    #[cfg(windows)]
    fn is_running(pid: usize) -> bool {
        // `tasklist` lists the process, or explains that nothing matched.
        let output = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .no_window()
            .output()
            .expect("tasklist ships with every supported Windows");
        String::from_utf8_lossy(&output.stdout).contains(&format!(" {pid} "))
    }

    #[cfg(windows)]
    #[test]
    fn closing_the_last_job_handle_ends_the_child_and_its_descendants() {
        // The contract the process-wide job relies on, exercised on a job of
        // its own so the test can close the handle: a wrapper and the solver
        // it started are both gone once the job is.
        use std::os::windows::io::AsRawHandle;
        use win32job::{ExtendedLimitInfo, Job};

        let mut limits = ExtendedLimitInfo::new();
        limits.limit_kill_on_job_close();
        let job = Job::create_with_limit_info(&limits).expect("a job object can be created");
        let mut child = long_running_command()
            .spawn()
            .expect("the OS shell is present in any environment that runs these tests");
        job.assign_process(child.as_raw_handle() as isize)
            .expect("a freshly spawned child can be assigned");

        // `cmd` starts `ping` a moment later; it joins the job by inheritance.
        let deadline = Instant::now() + Duration::from_secs(10);
        let members = loop {
            let members = job
                .query_process_id_list()
                .expect("a job can always list its members");
            if members.len() >= 2 || Instant::now() > deadline {
                break members;
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let wrapper = child.id() as usize;
        let solver = members
            .iter()
            .copied()
            .find(|&member| member != wrapper)
            .expect("the wrapper's payload joins the job without being assigned");
        assert!(is_running(solver));

        drop(job);

        let deadline = Instant::now() + Duration::from_secs(10);
        let wrapper_ended = loop {
            match child.try_wait() {
                Ok(Some(_)) => break true,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                _ => break false,
            }
        };
        assert!(wrapper_ended, "closing the job ends the wrapper");
        let deadline = Instant::now() + Duration::from_secs(10);
        while is_running(solver) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!is_running(solver), "closing the job ends the payload too");
    }
}
