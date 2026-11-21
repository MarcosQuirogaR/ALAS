// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Plan, consent, install and uninstall.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use super::build::{build_in_place, SelfChecks};
use super::extract::{extract_archive, install_root};
use super::health::{run_health_check, HealthReport};
use super::manifest::{Artifact, Manifest, ToolEntry};
use super::plan::{install_plan, ConsentReceipt, InstallPlan, PlanItemState};
use super::registry::{InstalledRecord, Registry};
use super::transfer::{fetch_verified, Job, TransferOptions};
use super::InstallError;

/// Where the installer works.
#[derive(Debug, Clone)]
pub struct InstallContext {
    /// ALAS user data root (the locator's `user_data_root`).
    pub data_root: PathBuf,
    /// Platform key, normally [`super::current_platform`].
    pub platform: String,
    /// Transport switches.
    pub options: TransferOptions,
    /// Self-checks the application provides to `build_from_source` tools.
    pub checks: SelfChecks,
}

impl InstallContext {
    /// Context for the running platform with production transport rules.
    pub fn new(data_root: impl Into<PathBuf>) -> Self {
        Self {
            data_root: data_root.into(),
            platform: super::current_platform().to_owned(),
            options: TransferOptions::default(),
            checks: SelfChecks::default(),
        }
    }

    /// Register the self-check a `build_from_source` tool names in its manifest entry.
    #[must_use]
    pub fn with_check(
        mut self,
        name: &str,
        check: impl Fn(&Path) -> Result<String, String> + Send + Sync + 'static,
    ) -> Self {
        self.checks.insert(name, std::sync::Arc::new(check));
        self
    }

    /// `<data root>/external tools`, searched by the tool locator.
    pub fn tools_dir(&self) -> PathBuf {
        self.data_root.join("external tools")
    }

    fn cache_dir(&self) -> PathBuf {
        self.data_root.join("downloads").join("tools")
    }

    fn staging_dir(&self) -> PathBuf {
        self.data_root.join("tools-staging")
    }
}

/// Progress of one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgressEvent {
    /// Work on a tool began.
    Started(String),
    /// Download progress for a tool.
    Downloading {
        /// Tool id.
        id: String,
        /// Bytes on disk so far.
        done: u64,
        /// Expected total.
        total: u64,
    },
    /// Hash verified; extracting.
    Extracting(String),
    /// Patching and compiling the source on this computer.
    Building(String),
    /// Running the tiny case.
    HealthCheck(String),
    /// Work on a tool ended.
    Finished(String),
}

/// What happened to one tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolResult {
    /// Installed and healthy.
    Installed {
        /// Absolute executable path.
        executable: PathBuf,
        /// Health check outcome.
        health: HealthReport,
    },
    /// Nothing to do.
    AlreadyInstalled,
    /// Not attempted; the text says why.
    Skipped(String),
    /// Guided steps to follow by hand.
    Guided(Vec<String>),
    /// Failed; previous state was restored.
    Failed(String),
    /// Stopped by the user; the partial download is kept for resume.
    Cancelled,
}

/// Per-tool result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolReport {
    /// Tool id.
    pub id: String,
    /// Outcome.
    pub result: ToolResult,
}

/// Run the plan. `manifest` and `ctx` must be the ones the plan was built from,
/// and `consent` must come from [`ConsentReceipt::for_plan`] on that plan.
pub fn run_install(
    manifest: &Manifest,
    ctx: &InstallContext,
    plan: &InstallPlan,
    consent: &ConsentReceipt,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(&ProgressEvent),
) -> Result<Vec<ToolReport>, InstallError> {
    let selection: Vec<String> = plan.items.iter().map(|item| item.id.clone()).collect();
    let fresh = install_plan(manifest, ctx, Some(&selection));
    if consent.fingerprint != plan.fingerprint || fresh.fingerprint != plan.fingerprint {
        return Err(InstallError::Unavailable(
            "the consent does not match what would be installed; review the plan again".into(),
        ));
    }
    let mut reports = Vec::new();
    for item in &plan.items {
        let result = match &item.state {
            PlanItemState::AlreadyInstalled => ToolResult::AlreadyInstalled,
            PlanItemState::Guided | PlanItemState::MissingSystemTools => {
                ToolResult::Guided(item.steps.clone())
            }
            PlanItemState::PendingReview => ToolResult::Skipped(
                "no reviewed archive is pinned for this tool yet; install it from the publisher page".into(),
            ),
            PlanItemState::MissingPin => {
                ToolResult::Skipped("the manifest has no SHA-256 for this platform".into())
            }
            PlanItemState::UnsupportedPlatform => {
                ToolResult::Skipped("no download is published for this platform".into())
            }
            PlanItemState::Install => {
                if cancel.load(Ordering::Relaxed) {
                    ToolResult::Cancelled
                } else {
                    progress(&ProgressEvent::Started(item.id.clone()));
                    let result = install_one(manifest, ctx, &item.id, cancel, progress);
                    progress(&ProgressEvent::Finished(item.id.clone()));
                    match result {
                        Ok(done) => done,
                        Err(InstallError::Cancelled) => ToolResult::Cancelled,
                        Err(error) => ToolResult::Failed(error.to_string()),
                    }
                }
            }
        };
        reports.push(ToolReport {
            id: item.id.clone(),
            result,
        });
    }
    Ok(reports)
}

fn io_error(what: &str, path: &Path, error: std::io::Error) -> InstallError {
    InstallError::Io(format!("{what} {}: {error}", path.display()))
}

fn install_one(
    manifest: &Manifest,
    ctx: &InstallContext,
    id: &str,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(&ProgressEvent),
) -> Result<ToolResult, InstallError> {
    let tool = manifest
        .tool(id)
        .ok_or_else(|| InstallError::Unavailable(format!("unknown tool {id}")))?;
    let artifact = tool.artifact_for(&ctx.platform).ok_or_else(|| {
        InstallError::Unavailable(format!("{} has no download for this platform", tool.name))
    })?;
    let sha256 = artifact
        .sha256
        .as_deref()
        .ok_or_else(|| InstallError::Unavailable(format!("{} has no pinned SHA-256", tool.name)))?;

    let cache = ctx.cache_dir();
    fs::create_dir_all(&cache).map_err(|e| io_error("cannot create", &cache, e))?;
    let part = cache.join(format!("{sha256}.download"));
    let total = artifact.size_bytes.unwrap_or(artifact.min_bytes);
    let mut report = |done: u64| {
        progress(&ProgressEvent::Downloading {
            id: id.to_owned(),
            done,
            total,
        })
    };
    fetch_verified(
        &Job {
            name: &tool.name,
            url: &artifact.url,
            sha256,
            min_bytes: artifact.min_bytes,
            part: &part,
            options: ctx.options,
        },
        cancel,
        &mut report,
    )?;

    progress(&ProgressEvent::Extracting(id.to_owned()));
    let staging = ctx
        .staging_dir()
        .join(format!("{id}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    let outcome = place(ctx, tool, artifact, &part, &staging, cancel, progress);
    let _ = fs::remove_dir_all(&staging);
    let (executable, health) = outcome?;
    let _ = fs::remove_file(&part);

    let mut registry = Registry::load(&ctx.data_root);
    registry.tools.insert(
        id.to_owned(),
        InstalledRecord {
            version: super::identity::installed_version(tool, artifact, &ctx.platform),
            directory: format!("external tools/{}", artifact.directory),
            executable: artifact.executable.clone(),
            archive_sha256: sha256.to_owned(),
            url: artifact.url.clone(),
            installed_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_secs()),
            healthy: health.passed,
        },
    );
    registry.save(&ctx.data_root)?;
    Ok(ToolResult::Installed { executable, health })
}

/// Extract, swap into place, health-check; undo everything on failure.
fn place(
    ctx: &InstallContext,
    tool: &ToolEntry,
    artifact: &Artifact,
    archive: &Path,
    staging: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(&ProgressEvent),
) -> Result<(PathBuf, HealthReport), InstallError> {
    extract_archive(archive, artifact.format, staging)?;
    let root = install_root(staging, artifact.strip_single_root)?;
    let build_detail = match &tool.build {
        Some(build) => Some(build_in_place(
            ctx, tool, artifact, build, &root, cancel, progress,
        )?),
        None => None,
    };
    if !root.join(&artifact.executable).is_file() {
        return Err(InstallError::Archive(format!(
            "{} is missing from the archive",
            artifact.executable
        )));
    }
    let tools_dir = ctx.tools_dir();
    fs::create_dir_all(&tools_dir).map_err(|e| io_error("cannot create", &tools_dir, e))?;
    let target = tools_dir.join(&artifact.directory);
    let backup = tools_dir.join(format!(
        ".{}.old-{}",
        artifact.directory,
        std::process::id()
    ));
    let had_previous = target.exists();
    if had_previous {
        fs::rename(&target, &backup).map_err(|e| io_error("cannot move aside", &target, e))?;
    }
    let restore = |reason: InstallError| {
        let _ = fs::remove_dir_all(&target);
        if had_previous {
            let _ = fs::rename(&backup, &target);
        }
        reason
    };
    if let Err(error) = fs::rename(&root, &target) {
        return Err(restore(io_error("cannot install into", &target, error)));
    }
    let executable = target.join(&artifact.executable);
    let health_target = target.join(
        artifact
            .health_executable
            .as_deref()
            .unwrap_or(&artifact.executable),
    );
    let health = match &artifact.health {
        None => HealthReport {
            passed: true,
            detail: build_detail
                .unwrap_or_else(|| "no tiny case defined; executable present".into()),
        },
        Some(spec) => {
            progress(&ProgressEvent::HealthCheck(tool.id.clone()));
            match run_health_check(&health_target, spec, &ctx.data_root) {
                Ok(report) if report.passed => report,
                Ok(report) => return Err(restore(InstallError::Health(report.detail))),
                Err(error) => return Err(restore(error)),
            }
        }
    };
    if had_previous {
        let _ = fs::remove_dir_all(&backup);
    }
    Ok((executable, health))
}

/// Remove an installed tool's directory and registry record.
pub fn uninstall_tool(ctx: &InstallContext, id: &str) -> Result<bool, InstallError> {
    let mut registry = Registry::load(&ctx.data_root);
    let Some(record) = registry.tools.get(id).cloned() else {
        return Ok(false);
    };
    let target = ctx.data_root.join(&record.directory);
    let tools_dir = ctx.tools_dir();
    let inside = target.parent() == Some(tools_dir.as_path())
        && target.file_name().is_some_and(|name| !name.is_empty());
    if !inside {
        return Err(InstallError::Io(format!(
            "refusing to remove {}: not directly below the installer's tools directory",
            target.display()
        )));
    }
    if let Ok(meta) = fs::symlink_metadata(&target) {
        if meta.file_type().is_symlink() {
            return Err(InstallError::Io(format!(
                "refusing to remove {}: it is a link",
                target.display()
            )));
        }
        fs::remove_dir_all(&target).map_err(|e| io_error("cannot remove", &target, e))?;
    }
    registry.tools.remove(id);
    registry.save(&ctx.data_root)?;
    Ok(true)
}
