// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! What an install run would do, and the consent that binds a user to exactly that.

use crate::sha256::Sha256;

use super::build::{missing_system_commands, toolchain_reusable};
use super::engine::InstallContext;
use super::identity::installed_version;
use super::manifest::{Artifact, InstallKind, Manifest, ToolEntry};
use super::registry::Registry;

/// What will happen to one tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanItemState {
    /// Will be downloaded and installed.
    Install,
    /// Installed with this version and healthy; nothing to do.
    AlreadyInstalled,
    /// Redistributable, but no reviewed archive is pinned yet.
    PendingReview,
    /// The manifest has an artifact without a SHA-256 for this platform.
    MissingPin,
    /// No artifact for this platform.
    UnsupportedPlatform,
    /// Licence-gated or needs administrator rights: steps only.
    Guided,
    /// A source build whose compiler the system must provide first (steps say how).
    MissingSystemTools,
}

/// One row of the consent screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanItem {
    /// Tool id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Licence summary.
    pub licence: String,
    /// Publisher page.
    pub publisher_url: String,
    /// What will happen.
    pub state: PlanItemState,
    /// Version to install, when known.
    pub version: Option<String>,
    /// Source URL, when the state is [`PlanItemState::Install`].
    pub url: Option<String>,
    /// Expected download size in bytes.
    pub download_bytes: Option<u64>,
    /// Steps for guided tools.
    pub steps: Vec<String>,
    /// True when the install compiles the tool on this computer.
    pub builds_from_source: bool,
    /// Every archive the install fetches (the tool itself and, when it is built, its
    /// toolchain), with where it comes from and what it costs.
    pub downloads: Vec<PlanDownload>,
    /// Disk the build tree needs while compiling, when the manifest says.
    pub build_disk_bytes: Option<u64>,
}

/// One archive of an install, shown to the user before consent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanDownload {
    /// What it is for ("Source", "Toolchain").
    pub role: String,
    /// Archive name or version.
    pub name: String,
    /// HTTPS source.
    pub url: String,
    /// Licence of this archive.
    pub licence: String,
    /// Expected size in bytes.
    pub bytes: u64,
    /// True when an earlier verified extraction is reused and nothing is fetched.
    pub reused: bool,
}

impl PlanDownload {
    /// One line for the consent screen (English).
    pub fn describe(&self) -> String {
        self.describe_with(&|text| text.to_owned())
    }

    /// As [`Self::describe`], with the fixed words passed through `translate`.
    pub fn describe_with(&self, translate: &dyn Fn(&str) -> String) -> String {
        let licence = if self.licence.is_empty() {
            String::new()
        } else {
            format!(" ({})", self.licence)
        };
        if self.reused {
            format!(
                "{} {}{licence}: {}",
                translate(&self.role),
                self.name,
                translate("already on this computer, not downloaded again")
            )
        } else {
            format!(
                "{} {}{licence}: {}, {} {} MB",
                translate(&self.role),
                self.name,
                self.url,
                translate("about"),
                self.bytes / 1_000_000
            )
        }
    }
}

/// Everything an install run would do, shown to the user before consent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
    /// One row per requested tool.
    pub items: Vec<PlanItem>,
    pub(super) fingerprint: String,
}

impl InstallPlan {
    /// Rows that will download something.
    pub fn downloads(&self) -> impl Iterator<Item = &PlanItem> {
        self.items
            .iter()
            .filter(|item| item.state == PlanItemState::Install)
    }

    /// Total expected download size in bytes.
    pub fn total_download_bytes(&self) -> u64 {
        self.downloads()
            .filter_map(|item| item.download_bytes)
            .sum()
    }
}

/// Proof that the user agreed to exactly this plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentReceipt {
    pub(super) fingerprint: String,
}

impl ConsentReceipt {
    /// Record consent to `plan`. Call this only from the code path that handles
    /// the user's explicit confirmation (button or `--yes`).
    pub fn for_plan(plan: &InstallPlan) -> Self {
        Self {
            fingerprint: plan.fingerprint.clone(),
        }
    }
}

/// Work out what installing `selection` (all tools when `None`) would do.
pub fn install_plan(
    manifest: &Manifest,
    ctx: &InstallContext,
    selection: Option<&[String]>,
) -> InstallPlan {
    let registry = Registry::load(&ctx.data_root);
    let mut items = Vec::new();
    let mut hasher = Sha256::new();
    for tool in &manifest.tools {
        if selection.is_some_and(|ids| !ids.iter().any(|id| id == &tool.id)) {
            continue;
        }
        let item = plan_item(tool, ctx, &registry);
        if item.state == PlanItemState::Install {
            for field in [
                &item.id,
                item.url.as_deref().unwrap_or(""),
                &artifact_hash(tool, ctx),
            ] {
                hasher.update_field(field.as_bytes());
            }
        }
        items.push(item);
    }
    InstallPlan {
        items,
        fingerprint: hasher.finalize_hex(),
    }
}

fn artifact_hash(tool: &ToolEntry, ctx: &InstallContext) -> String {
    let mut text = tool
        .artifact_for(&ctx.platform)
        .and_then(|artifact| artifact.sha256.clone())
        .unwrap_or_default();
    for part in toolchain_parts(tool, &ctx.platform) {
        text.push_str(&part.url);
        text.push_str(part.sha256.as_deref().unwrap_or(""));
        // What the user saw (fetch or reuse) is part of what they agreed to.
        text.push_str(if toolchain_reusable(ctx, part) {
            "reused"
        } else {
            "fetch"
        });
    }
    text
}

fn toolchain_parts<'a>(tool: &'a ToolEntry, platform: &str) -> &'a [Artifact] {
    tool.build
        .as_ref()
        .and_then(|build| build.toolchain.get(platform))
        .map_or(&[], Vec::as_slice)
}

fn plan_item(tool: &ToolEntry, ctx: &InstallContext, registry: &Registry) -> PlanItem {
    let mut item = PlanItem {
        id: tool.id.clone(),
        name: tool.name.clone(),
        licence: tool.licence.clone(),
        publisher_url: tool.publisher_url.clone(),
        state: PlanItemState::Guided,
        version: None,
        url: None,
        download_bytes: None,
        steps: tool.guided_steps.clone(),
        builds_from_source: tool.kind == InstallKind::BuildFromSource,
        downloads: Vec::new(),
        build_disk_bytes: None,
    };
    item.state = match tool.kind {
        InstallKind::Guided => PlanItemState::Guided,
        InstallKind::PendingReview => PlanItemState::PendingReview,
        InstallKind::Automatic | InstallKind::BuildFromSource => {
            match tool.artifact_for(&ctx.platform) {
                None => PlanItemState::UnsupportedPlatform,
                Some(artifact) if artifact.sha256.is_none() => PlanItemState::MissingPin,
                Some(_)
                    if toolchain_parts(tool, &ctx.platform)
                        .iter()
                        .any(|p| p.sha256.is_none()) =>
                {
                    PlanItemState::MissingPin
                }
                Some(artifact) => {
                    item.version = Some(artifact.version.clone());
                    item.url = Some(artifact.url.clone());
                    let mut bytes = artifact.size_bytes.unwrap_or(artifact.min_bytes);
                    item.downloads.push(PlanDownload {
                        role: if tool.kind == InstallKind::BuildFromSource {
                            "Source"
                        } else {
                            "Download"
                        }
                        .to_owned(),
                        name: artifact.version.clone(),
                        url: artifact.url.clone(),
                        licence: tool.licence.clone(),
                        bytes,
                        reused: false,
                    });
                    for part in toolchain_parts(tool, &ctx.platform) {
                        let reused = toolchain_reusable(ctx, part);
                        let size = part.size_bytes.unwrap_or(part.min_bytes);
                        if !reused {
                            bytes += size;
                        }
                        item.downloads.push(PlanDownload {
                            role: "Toolchain".to_owned(),
                            name: part.version.clone(),
                            url: part.url.clone(),
                            licence: part.licence.clone().unwrap_or_default(),
                            bytes: size,
                            reused,
                        });
                    }
                    item.download_bytes = Some(bytes);
                    item.build_disk_bytes = tool.build.as_ref().and_then(|build| {
                        build.disk_bytes.map(|base| {
                            let toolchain: u64 = toolchain_parts(tool, &ctx.platform)
                                .iter()
                                .filter(|part| !toolchain_reusable(ctx, part))
                                .map(|part| part.extracted_bytes.unwrap_or(0))
                                .sum();
                            base + toolchain
                        })
                    });
                    let wanted = installed_version(tool, artifact, &ctx.platform);
                    let current = registry.tools.get(&tool.id).is_some_and(|record| {
                        record.healthy
                            && record.version == wanted
                            && registry.executable(&ctx.data_root, &tool.id).is_some()
                    });
                    let missing = tool
                        .build
                        .as_ref()
                        .map(|build| missing_system_commands(build, &ctx.platform))
                        .unwrap_or_default();
                    if current {
                        PlanItemState::AlreadyInstalled
                    } else if !missing.is_empty() {
                        let hint = tool
                            .build
                            .as_ref()
                            .and_then(|build| build.missing_commands_hint.get(&ctx.platform))
                            .cloned()
                            .unwrap_or_default();
                        item.steps = vec![
                            format!("Missing on this computer: {}.", missing.join(", ")),
                            hint,
                            "Then run the installer again.".to_owned(),
                        ];
                        PlanItemState::MissingSystemTools
                    } else {
                        PlanItemState::Install
                    }
                }
            }
        }
    };
    item
}
