// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Building a `build_from_source` tool on the user's machine.
//!
//! The source archive is fetched and extracted like any other artifact; this
//! module then fetches the platform's toolchain (once, reused), applies the
//! embedded patch, runs the manifest's build commands under the supervision
//! rules of the rest of the crate, prunes the tree to the files the tool needs
//! and runs the application's registered self-check on the result. Nothing is
//! left in the final directory that was not built here from the pinned source.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::process::{drain, drained_text, kill_process_tree, NewProcessGroup, NoConsoleWindow};
use crate::supervise::SupervisedSpawn;

use super::engine::{InstallContext, ProgressEvent};
use super::extract::{extract_archive, install_root};
use super::manifest::{Artifact, BuildSpec, BuildStep, ToolEntry};
use super::patch::apply_unified;
use super::transfer::{fetch_verified, Job};
use super::InstallError;

/// A self-check the application registers by name (the installer cannot depend
/// on the crates that know how to run a solver). It receives the staged install
/// directory and returns a one-line result.
pub type SelfCheck = Arc<dyn Fn(&Path) -> Result<String, String> + Send + Sync>;

/// The registered self-checks, by name.
#[derive(Clone, Default)]
pub struct SelfChecks(BTreeMap<String, SelfCheck>);

impl std::fmt::Debug for SelfChecks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.0.keys()).finish()
    }
}

impl SelfChecks {
    /// Register `check` under `name`.
    pub fn insert(&mut self, name: &str, check: SelfCheck) {
        self.0.insert(name.to_owned(), check);
    }

    fn get(&self, name: &str) -> Option<&SelfCheck> {
        self.0.get(name)
    }
}

/// Patches compiled into the application, by manifest name.
fn embedded_patch(name: &str) -> Option<&'static [u8]> {
    match name {
        "nastran95-alas-local" => Some(include_bytes!(
            "../../../../tools/nastran95/alas-local.patch"
        )),
        _ => None,
    }
}

/// Whether `program` is found on `PATH` (with the Windows executable suffixes).
pub(super) fn command_on_path(program: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    let suffixes: &[&str] = if cfg!(windows) {
        &["", ".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    std::env::split_paths(&path).any(|dir| {
        suffixes
            .iter()
            .any(|suffix| dir.join(format!("{program}{suffix}")).is_file())
    })
}

/// The programs a `build_from_source` tool still needs from the system on this
/// platform: none when a toolchain archive is pinned for it.
pub(super) fn missing_system_commands(build: &BuildSpec, platform: &str) -> Vec<String> {
    if build
        .toolchain
        .get(platform)
        .is_some_and(|parts| !parts.is_empty())
    {
        return Vec::new();
    }
    build
        .required_commands
        .get(platform)
        .into_iter()
        .flatten()
        .filter(|program| !command_on_path(program))
        .cloned()
        .collect()
}

fn toolchain_root(ctx: &InstallContext) -> PathBuf {
    ctx.data_root.join("toolchains")
}

const MARKER: &str = ".alas-toolchain";

/// True when `part` was extracted earlier from the pinned archive: the marker holds
/// its SHA-256 and its executable is there. Nothing is fetched or re-hashed.
pub(super) fn toolchain_reusable(ctx: &InstallContext, part: &Artifact) -> bool {
    let Some(sha256) = part.sha256.as_deref() else {
        return false;
    };
    let target = toolchain_root(ctx).join(&part.directory);
    fs::read_to_string(target.join(MARKER)).is_ok_and(|text| text.trim() == sha256)
        && target.join(&part.executable).is_file()
}

/// Fetch and extract the platform's toolchain archives (reusing a verified
/// earlier extraction) and return their directories.
fn ensure_toolchain(
    ctx: &InstallContext,
    tool: &ToolEntry,
    parts: &[Artifact],
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(&ProgressEvent),
) -> Result<Vec<PathBuf>, InstallError> {
    let mut dirs = Vec::new();
    for part in parts {
        let sha256 = part.sha256.as_deref().ok_or_else(|| {
            InstallError::Unavailable(format!(
                "{}: a toolchain archive has no pinned SHA-256",
                tool.name
            ))
        })?;
        let target = toolchain_root(ctx).join(&part.directory);
        let marker = target.join(MARKER);
        if toolchain_reusable(ctx, part) {
            dirs.push(target);
            continue;
        }
        let cache = ctx.data_root.join("downloads").join("tools");
        fs::create_dir_all(&cache)
            .map_err(|e| InstallError::Io(format!("cannot create {}: {e}", cache.display())))?;
        let archive = cache.join(format!("{sha256}.download"));
        let total = part.size_bytes.unwrap_or(part.min_bytes);
        let id = tool.id.clone();
        let mut report = |done: u64| {
            progress(&ProgressEvent::Downloading {
                id: id.clone(),
                done,
                total,
            });
        };
        fetch_verified(
            &Job {
                name: &part.directory,
                url: &part.url,
                sha256,
                min_bytes: part.min_bytes,
                part: &archive,
                options: ctx.options,
            },
            cancel,
            &mut report,
        )?;
        progress(&ProgressEvent::Extracting(tool.id.clone()));
        let staging =
            toolchain_root(ctx).join(format!(".{}-{}", part.directory, std::process::id()));
        let _ = fs::remove_dir_all(&staging);
        extract_archive(&archive, part.format, &staging)?;
        let root = install_root(&staging, part.strip_single_root)?;
        if !root.join(&part.executable).is_file() {
            let _ = fs::remove_dir_all(&staging);
            return Err(InstallError::Archive(format!(
                "{} is missing from the toolchain archive",
                part.executable
            )));
        }
        let _ = fs::remove_dir_all(&target);
        fs::rename(&root, &target)
            .map_err(|e| InstallError::Io(format!("cannot install {}: {e}", target.display())))?;
        let _ = fs::remove_dir_all(&staging);
        fs::write(&marker, sha256)
            .map_err(|e| InstallError::Io(format!("cannot write {}: {e}", marker.display())))?;
        let _ = fs::remove_file(&archive);
        dirs.push(target);
    }
    Ok(dirs)
}

fn job_count() -> usize {
    thread::available_parallelism().map_or(1, |n| n.get().min(4))
}

fn tail(text: &str, max: usize) -> String {
    let text = text.trim_end();
    if text.len() <= max {
        return text.to_owned();
    }
    let start = text.len() - max;
    let start = (start..text.len())
        .find(|&i| text.is_char_boundary(i))
        .unwrap_or(text.len());
    format!("...{}", &text[start..])
}

/// Run one build command to completion, killing the tree on cancel or deadline.
fn run_step(
    step: &BuildStep,
    cwd: &Path,
    source_root: &Path,
    path: &OsString,
    cancel: &AtomicBool,
) -> Result<(), InstallError> {
    let src = source_root.to_string_lossy().replace('\\', "/");
    let jobs = job_count().to_string();
    let args: Vec<String> = step
        .args
        .iter()
        .map(|arg| arg.replace("{src}", &src).replace("{jobs}", &jobs))
        .collect();
    let label = format!(
        "{} {}",
        step.program,
        args.first().map_or("", String::as_str)
    );
    let mut command = Command::new(&step.program);
    command
        .args(&args)
        .current_dir(cwd)
        .env("PATH", path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .no_window()
        .new_process_group();
    let mut child = command
        .spawn_supervised("tool installer source build")
        .map_err(|error| InstallError::Build(format!("cannot start {}: {error}", step.program)))?;
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let deadline = Instant::now() + Duration::from_secs(step.timeout_seconds.max(1));
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                kill_process_tree(child.id());
                return Err(InstallError::Build(format!("{label}: {error}")));
            }
        }
        if cancel.load(Ordering::Relaxed) {
            kill_process_tree(child.id());
            let _ = child.wait();
            return Err(InstallError::Cancelled);
        }
        if Instant::now() >= deadline {
            kill_process_tree(child.id());
            let _ = child.wait();
            return Err(InstallError::Build(format!(
                "{label} timed out after {} s",
                step.timeout_seconds
            )));
        }
        thread::sleep(Duration::from_millis(200));
    };
    let text = format!("{}{}", drained_text(out), drained_text(err));
    if status.success() {
        Ok(())
    } else {
        Err(InstallError::Build(format!(
            "{label} failed with {status}: {}",
            tail(&text, 2000)
        )))
    }
}

/// Remove everything below `root` except the `keep` paths.
fn prune(root: &Path, keep: &[String]) -> Result<(), InstallError> {
    let keep: Vec<PathBuf> = keep.iter().map(PathBuf::from).collect();
    prune_dir(root, root, &keep)
}

fn prune_dir(root: &Path, dir: &Path, keep: &[PathBuf]) -> Result<(), InstallError> {
    let entries = fs::read_dir(dir)
        .map_err(|e| InstallError::Io(format!("cannot read {}: {e}", dir.display())))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        if keep.iter().any(|kept| relative.starts_with(kept)) {
            continue;
        }
        if keep.iter().any(|kept| kept.starts_with(&relative)) {
            prune_dir(root, &path, keep)?;
            continue;
        }
        let result = if path.is_dir() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        result.map_err(|e| InstallError::Io(format!("cannot remove {}: {e}", path.display())))?;
    }
    Ok(())
}

/// Patch, build, prune and check the extracted source tree at `root`.
pub(super) fn build_in_place(
    ctx: &InstallContext,
    tool: &ToolEntry,
    artifact: &Artifact,
    build: &BuildSpec,
    root: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(&ProgressEvent),
) -> Result<String, InstallError> {
    let platform = ctx.platform.as_str();
    let toolchain_dirs = match build.toolchain.get(platform) {
        Some(parts) if !parts.is_empty() => ensure_toolchain(ctx, tool, parts, cancel, progress)?,
        _ => Vec::new(),
    };
    let missing = missing_system_commands(build, platform);
    if !missing.is_empty() {
        return Err(InstallError::Unavailable(format!(
            "{} needs {} on this computer. {}",
            tool.name,
            missing.join(", "),
            build
                .missing_commands_hint
                .get(platform)
                .map_or("", String::as_str)
        )));
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    for toolchain in &toolchain_dirs {
        for relative in build.path_dirs.get(platform).into_iter().flatten() {
            dirs.push(toolchain.join(relative));
        }
    }
    dirs.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(dirs)
        .map_err(|error| InstallError::Build(format!("cannot build PATH: {error}")))?;

    progress(&ProgressEvent::Building(tool.id.clone()));
    if let Some(name) = &build.patch {
        let patch = embedded_patch(name)
            .ok_or_else(|| InstallError::Manifest(format!("{}: unknown patch {name}", tool.id)))?;
        apply_unified(root, patch)?;
    }
    let steps = build.steps.get(platform).ok_or_else(|| {
        InstallError::Unavailable(format!(
            "{} has no build steps for this platform",
            tool.name
        ))
    })?;
    for step in steps {
        if cancel.load(Ordering::Relaxed) {
            return Err(InstallError::Cancelled);
        }
        run_step(step, root, root, &path, cancel)?;
    }
    prune(root, &build.keep)?;

    let result = match &build.check {
        None => "built; no self-check defined".to_owned(),
        Some(check_name) => {
            progress(&ProgressEvent::HealthCheck(tool.id.clone()));
            let check = ctx.checks.get(check_name).ok_or_else(|| {
                InstallError::Health(format!(
                    "the self-check {check_name:?} is not available in this application; the build was discarded"
                ))
            })?;
            check(root).map_err(InstallError::Health)?
        }
    };
    let record = format!(
        "Built on this computer by the ALAS tool installer; not redistributable.
source: {}
source SHA-256: {}
revision: {}
recipe identity: {}
patch SHA-256: {}
toolchain: {}
self-check: {}
",
        artifact.url,
        artifact.sha256.as_deref().unwrap_or(""),
        artifact.version,
        super::identity::installed_version(tool, artifact, platform),
        patch_sha256(build).unwrap_or_default(),
        build
            .toolchain
            .get(platform)
            .into_iter()
            .flatten()
            .map(|part| {
                format!(
                    "{} {} SHA-256 {}",
                    part.version,
                    part.url,
                    part.sha256.as_deref().unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("; "),
        result
    );
    fs::write(root.join("ALAS-BUILD.txt"), record)
        .map_err(|e| InstallError::Io(format!("cannot write the build record: {e}")))?;
    Ok(result)
}

/// SHA-256 of the embedded patch, recorded beside the build for provenance.
pub(super) fn patch_sha256(build: &BuildSpec) -> Option<String> {
    let name = build.patch.as_deref()?;
    let bytes = embedded_patch(name)?;
    let mut hasher = crate::sha256::Sha256::new();
    hasher.update(bytes);
    Some(hasher.finalize_hex())
}
