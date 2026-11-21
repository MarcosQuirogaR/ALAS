// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! `--install-tools`, `--yes` and `--uninstall-tool`: the headless face of the
//! external-tool installer in [`alas_exec::installer`].

use std::sync::atomic::AtomicBool;

use alas_exec::installer::{
    install_plan, run_install, uninstall_tool, ConsentReceipt, InstallContext, InstallPlan,
    Manifest, PlanItemState, ProgressEvent, ToolResult,
};
use alas_exec::ToolLocator;

use super::args::CliArgs;

/// Handle the installer flags. `None` means the arguments did not ask for it.
pub(super) fn run_tools_command(cli: &CliArgs, locator: &ToolLocator) -> Option<i32> {
    if cli.uninstall_tool.is_none() && !cli.install_tools {
        return None;
    }
    let ctx = InstallContext::new(locator.user_data_root()).with_check(
        alas_struct::nastran95::INSTALL_CHECK_NAME,
        alas_struct::nastran95::installation_self_check,
    );
    if let Some(id) = &cli.uninstall_tool {
        return Some(match uninstall_tool(&ctx, id) {
            Ok(true) => {
                println!("Removed {id}.");
                if let Some(note) =
                    alas_exec::installer::toolchain_leftover_note(locator.user_data_root())
                {
                    println!("{note}");
                }
                0
            }
            Ok(false) => {
                println!("{id} is not installed by ALAS.");
                0
            }
            Err(error) => {
                eprintln!("Cannot remove {id}: {error}");
                1
            }
        });
    }
    let manifest = match Manifest::bundled() {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!("{error}");
            return Some(1);
        }
    };
    let plan = install_plan(&manifest, &ctx, None);
    print_plan(&plan, &ctx);
    if !cli.assume_yes {
        println!(
            "\nNothing was downloaded. Re-run with --yes to install the tools marked [install]."
        );
        return Some(0);
    }
    if plan.downloads().count() == 0 {
        return Some(0);
    }
    let consent = ConsentReceipt::for_plan(&plan);
    let cancel = AtomicBool::new(false);
    let mut last_percent = u64::MAX;
    let mut progress = |event: &ProgressEvent| match event {
        ProgressEvent::Started(id) => println!("{id}: starting"),
        ProgressEvent::Downloading { id, done, total } => {
            let percent = (done * 100).checked_div(*total).unwrap_or(0).min(99) / 10 * 10;
            if percent != last_percent {
                last_percent = percent;
                println!("{id}: downloaded {} MB ({percent}%)", done / 1_000_000);
            }
        }
        ProgressEvent::Extracting(id) => println!("{id}: verified, extracting"),
        ProgressEvent::Building(id) => {
            println!("{id}: patching and compiling on this computer (several minutes)")
        }
        ProgressEvent::HealthCheck(id) => println!("{id}: running health check"),
        ProgressEvent::Finished(_) => last_percent = u64::MAX,
    };
    let reports = match run_install(&manifest, &ctx, &plan, &consent, &cancel, &mut progress) {
        Ok(reports) => reports,
        Err(error) => {
            eprintln!("{error}");
            return Some(1);
        }
    };
    let mut failed = false;
    for report in &reports {
        match &report.result {
            ToolResult::Installed { executable, health } => {
                println!(
                    "{}: installed at {} ({})",
                    report.id,
                    executable.display(),
                    health.detail
                )
            }
            ToolResult::Failed(message) => {
                failed = true;
                eprintln!("{}: FAILED: {message}", report.id);
            }
            ToolResult::Cancelled => failed = true,
            _ => {}
        }
    }
    Some(i32::from(failed))
}

fn print_plan(plan: &InstallPlan, ctx: &InstallContext) {
    println!("External tools (user data: {})", ctx.data_root.display());
    for item in &plan.items {
        let tag = match item.state {
            PlanItemState::Install if item.builds_from_source => "download and compile",
            PlanItemState::Install => "install",
            PlanItemState::AlreadyInstalled => "installed",
            PlanItemState::PendingReview => "pending review",
            PlanItemState::MissingPin => "no pinned hash",
            PlanItemState::UnsupportedPlatform => "not for this platform",
            PlanItemState::Guided => "manual",
            PlanItemState::MissingSystemTools => "needs system tools",
        };
        println!(
            "  [{tag}] {} - {} ({})",
            item.name, item.licence, item.publisher_url
        );
        if item.downloads.is_empty() {
            if let (Some(url), Some(bytes)) = (&item.url, item.download_bytes) {
                println!("      from {url}, about {} MB", bytes / 1_000_000);
            }
        } else {
            for download in &item.downloads {
                println!("      {}", download.describe());
            }
            if let Some(bytes) = item.build_disk_bytes {
                println!(
                    "      the build needs about {} MB of disk while compiling",
                    bytes / 1_000_000
                );
            }
        }
        for (number, step) in item.steps.iter().enumerate() {
            println!("      {}. {step}", number + 1);
        }
    }
}
