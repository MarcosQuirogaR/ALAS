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
//! On Unix there is no job object and the workspace forbids `unsafe`, so
//! `prctl(PR_SET_PDEATHSIG)` (which needs `pre_exec`) is not available. Each
//! supervised child instead gets a small watchdog process (`sh`, in its own
//! process group, one per child) that polls once a second and, when ALAS is
//! gone while the child is still running, kills the child's process group.
//! It also exits by itself when the child ends. Residual behaviour: the kill
//! comes up to about a second after ALAS dies, so a child may run that long; a
//! SIGKILL aimed at the watchdog as well (for example `pkill sh`, or killing
//! the whole session) leaves the child running, and so does a host without
//! `/bin/sh`, which [`supervision_status`] reports as `Unavailable`.

use std::collections::VecDeque;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;

/// Whether owned processes are placed under kernel-enforced supervision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisionStatus {
    /// Parent-exit supervision is in force: the Windows job object, or on Unix the per-child watchdog.
    Active,
    /// Parent-exit supervision is unavailable here (no job object, or no `/bin/sh` for the Unix watchdog). Process groups and tree kills still
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
    /// True when the child is under parent-exit supervision (the ALAS job object, or the Unix watchdog).
    pub supervised: bool,
    /// The run stage the launching thread was working for (see
    /// [`with_launch_stage`]), such as `downstream/mses`; `None` when the
    /// launch came from outside any declared stage.
    pub stage: Option<String>,
}

thread_local! {
    static LAUNCH_STAGE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Runs `work` with launches from this thread recorded under `stage`.
///
/// Stages run concurrently (the downstream analyses each run on their own
/// thread), so the stage of a launch is a property of the thread that makes
/// it, not of whichever stage last reported. The previous stage is restored
/// afterwards, so declarations nest. A thread `work` spawns does not inherit
/// the stage; it re-declares it with [`launch_stage`] read on this thread.
pub fn with_launch_stage<T>(stage: &str, work: impl FnOnce() -> T) -> T {
    struct Restore(Option<String>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take();
            LAUNCH_STAGE.with(|current| *current.borrow_mut() = previous);
        }
    }
    let previous = LAUNCH_STAGE.with(|current| current.replace(Some(stage.to_owned())));
    let _restore = Restore(previous);
    work()
}

/// The stage launches from this thread are recorded under, if declared.
pub fn launch_stage() -> Option<String> {
    LAUNCH_STAGE.with(|current| current.borrow().clone())
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

static LAUNCHES_BLOCKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Refuse (or allow again) every supervised launch.
///
/// A user cancellation kills the running solvers, but the stage that was
/// waiting on one would go on to start the next (the next load case, the next
/// Patran render) before it reaches a cancellation check. While launches are
/// blocked each of those spawns fails at once, so the stage unwinds without
/// starting new work. The caller lifts the block when the run has ended.
pub fn block_launches(blocked: bool) {
    LAUNCHES_BLOCKED.store(blocked, Ordering::SeqCst);
}

/// The error a launch gets while [`block_launches`] is in force.
fn refuse_when_blocked(blocked: bool) -> std::io::Result<()> {
    if blocked {
        Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "external tool launch refused: the run was cancelled",
        ))
    } else {
        Ok(())
    }
}

/// Kill, by PID, the process tree of every launch ALAS owns that is still
/// alive and started at or after `since`. Returns the root PIDs it signalled.
///
/// This is the prompt half of a user cancellation: the pipeline's cooperative
/// cancel flag is only read between stages, while a solver (NASTRAN, Patran,
/// VSPAERO) may run for many minutes. Killing the children makes each blocked
/// wait return at once, so the stage ends and the flag is then seen.
///
/// On Windows the ALAS job object lists the live members, so a PID the OS has
/// reused for an unrelated process is never touched. On Unix the process group
/// of each recent ledger entry is signalled; `since` bounds the PID-reuse
/// window to the run being cancelled.
pub fn terminate_launched_since(since: SystemTime) -> Vec<u32> {
    let roots: Vec<u32> = recent_launches()
        .into_iter()
        .filter(|entry| entry.started >= since)
        .map(|entry| entry.pid)
        .filter(|pid| platform::is_live(*pid))
        .collect();
    for pid in &roots {
        crate::process::kill_process_tree(*pid);
    }
    roots
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
    /// Spawn, then place the child under parent-exit supervision and record the
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
        refuse_when_blocked(LAUNCHES_BLOCKED.load(Ordering::SeqCst))?;
        let program = self.get_program().to_string_lossy().into_owned();
        let child = spawn_retrying_busy_executable(self)?;
        let supervised = platform::assign(&child).is_ok();
        record(LaunchRecord {
            sequence: 0,
            pid: child.id(),
            role: role.to_owned(),
            program,
            started: SystemTime::now(),
            supervised,
            stage: launch_stage(),
        });
        Ok(child)
    }
}

/// Spawn, retrying briefly while the kernel reports the executable as busy.
///
/// On Linux `ETXTBSY` is returned when any process still holds a write
/// descriptor on the file, which includes a sibling thread's child that was
/// forked while the file was being written and has not yet reached `exec`.
/// The condition clears within milliseconds, so a short bounded retry turns a
/// spurious launch failure into a normal launch; any other error, or a file
/// that stays busy, is returned unchanged.
fn spawn_retrying_busy_executable(command: &mut Command) -> std::io::Result<Child> {
    let mut delay = std::time::Duration::from_millis(10);
    for _ in 0..6 {
        match command.spawn() {
            Err(error) if error.kind() == std::io::ErrorKind::ExecutableFileBusy => {
                std::thread::sleep(delay);
                delay *= 2;
            }
            result => return result,
        }
    }
    command.spawn()
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

    /// Whether `pid` is a live member of the ALAS job.
    pub(super) fn is_live(pid: u32) -> bool {
        contains(pid)
    }

    /// Whether `pid` is a member of the ALAS job right now.
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
    use crate::process::{NewProcessGroup, NoConsoleWindow};
    use std::path::Path;
    use std::process::{Child, Command, Stdio};

    /// Poll interval is the script's `sleep 1`. Exits quietly when the child
    /// is gone; kills the child's group (and the child itself, in case it is
    /// not a group leader) when the parent is gone first.
    const WATCHDOG: &str =
        "while kill -0 \"$1\" 2>/dev/null && kill -0 \"$2\" 2>/dev/null; do sleep 1; done; \
        kill -0 \"$2\" 2>/dev/null || exit 0; \
        kill -s KILL -- \"-$2\" 2>/dev/null; kill -s KILL \"$2\" 2>/dev/null; exit 0";

    /// Without a job there is no authoritative live list; the group signal is
    /// harmless when the group is already gone.
    pub(super) fn is_live(_pid: u32) -> bool {
        true
    }

    pub(super) fn status() -> SupervisionStatus {
        if Path::new("/bin/sh").is_file() {
            SupervisionStatus::Active
        } else {
            SupervisionStatus::Unavailable("no /bin/sh for the parent-exit watchdog".to_owned())
        }
    }

    /// The watchdog for `child`, which should be watched on behalf of the
    /// process `parent`.
    pub(super) fn watchdog_command(parent: u32, child: u32) -> Command {
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", WATCHDOG, "alas-watchdog"])
            .arg(parent.to_string())
            .arg(child.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .no_window()
            // Its own group, so a signal to ALAS's group does not take the
            // watchdog down with the process it is watching.
            .new_process_group();
        command
    }

    pub(super) fn assign(child: &Child) -> Result<(), String> {
        let watchdog = watchdog_command(std::process::id(), child.id())
            .spawn()
            .map_err(|error| format!("cannot start the parent-exit watchdog: {error}"))?;
        // The watchdog ends within a second of the child; reap it then.
        std::thread::spawn(move || {
            let mut watchdog = watchdog;
            let _ = watchdog.wait();
        });
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn watchdog_for_test(parent: u32, child: u32) -> std::io::Result<Child> {
        watchdog_command(parent, child).spawn()
    }
}

// The tests spawn real processes; a failed expect is the test host lacking a
// shell or a job object, not a library invariant being broken.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::{kill_process_tree, NoConsoleWindow};
    #[cfg(windows)]
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
        // Windows: the job object. Unix: the parent-exit watchdog (needs /bin/sh).
        assert_eq!(supervision_status(), SupervisionStatus::Active);
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
        assert!(entry.supervised);
        assert_eq!(entry.stage, None, "no stage was declared");
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

    #[test]
    fn a_blocked_launch_is_refused_as_interrupted() {
        let error = refuse_when_blocked(true).expect_err("a blocked launch fails");
        assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
        assert!(refuse_when_blocked(false).is_ok());
    }

    #[test]
    fn terminating_launched_children_ends_a_long_running_solver_promptly() {
        let since = SystemTime::now();
        let mut child = long_running_command()
            .spawn_supervised("terminate probe")
            .expect("the OS shell is present in any environment that runs these tests");
        let pid = child.id();
        let started = std::time::Instant::now();
        let killed = terminate_launched_since(since);
        assert!(killed.contains(&pid), "the live child is signalled by PID");
        let _ = child.wait();
        assert!(
            started.elapsed() < std::time::Duration::from_secs(20),
            "the kill returns far sooner than the child's own 60 s"
        );
    }

    #[test]
    fn a_launch_is_recorded_under_the_stage_its_thread_declared() {
        let mut child = with_launch_stage("downstream/mses", || {
            assert_eq!(
                with_launch_stage("inner", launch_stage).as_deref(),
                Some("inner")
            );
            assert_eq!(launch_stage().as_deref(), Some("downstream/mses"));
            long_running_command()
                .spawn_supervised("stage probe")
                .expect("the OS shell is present in any environment that runs these tests")
        });
        assert_eq!(launch_stage(), None, "the declaration ends with its scope");
        let pid = child.id();
        let entry = recent_launches()
            .into_iter()
            .find(|entry| entry.pid == pid)
            .expect("a supervised spawn is always recorded");
        assert_eq!(entry.stage.as_deref(), Some("downstream/mses"));
        kill_process_tree(pid);
        let _ = child.wait();
    }

    /// Wait until `child` has ended, up to `limit`.
    #[cfg(unix)]
    fn ends_within(child: &mut std::process::Child, limit: std::time::Duration) -> bool {
        let deadline = std::time::Instant::now() + limit;
        loop {
            if child.try_wait().expect("a child can be polled").is_some() {
                return true;
            }
            if std::time::Instant::now() > deadline {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_watchdog_kills_the_child_group_when_its_parent_is_gone() {
        use crate::process::NewProcessGroup;
        // A stand-in for ALAS: the watchdog only needs the parent's pid to
        // disappear while the child runs on.
        let mut parent = long_running_command().spawn().expect("sh is present");
        let mut child = long_running_command()
            .new_process_group()
            .spawn()
            .expect("sh is present");
        let mut watchdog =
            platform::watchdog_for_test(parent.id(), child.id()).expect("the watchdog starts");
        std::thread::sleep(std::time::Duration::from_millis(1500));
        assert!(
            child.try_wait().expect("polls").is_none(),
            "the child must survive while its parent is alive"
        );
        parent.kill().expect("the stand-in parent can be killed");
        let _ = parent.wait();
        assert!(
            ends_within(&mut child, std::time::Duration::from_secs(10)),
            "the child outlived its parent"
        );
        let _ = watchdog.wait();
    }

    #[cfg(unix)]
    #[test]
    fn the_watchdog_leaves_the_parent_alone_when_the_child_ends_first() {
        let mut parent = long_running_command().spawn().expect("sh is present");
        let mut child = long_running_command().spawn().expect("sh is present");
        let mut watchdog =
            platform::watchdog_for_test(parent.id(), child.id()).expect("the watchdog starts");
        child.kill().expect("the child can be killed");
        let _ = child.wait();
        assert!(
            ends_within(&mut watchdog, std::time::Duration::from_secs(10)),
            "the watchdog must end once its child has"
        );
        assert!(parent.try_wait().expect("polls").is_none());
        let _ = parent.kill();
        let _ = parent.wait();
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
