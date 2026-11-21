// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Background state for the "Install all" external-tools flow.
//!
//! The engine lives in [`alas_exec::installer`]; this module only owns the
//! worker thread, the plan shown for consent, and the per-tool progress the
//! card draws. Nothing is downloaded until [`ToolInstaller::confirm`] is called
//! from the consent button, and the plan the engine runs is the plan shown.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::thread;

use alas_exec::installer::{
    install_plan, run_install, uninstall_tool, ConsentReceipt, InstallContext, InstallPlan,
    Manifest, ProgressEvent, Registry, ToolReport, ToolResult,
};

/// Where one tool is in the current run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolProgress {
    /// Short stage text.
    pub stage: String,
    /// Bytes downloaded so far.
    pub done: u64,
    /// Expected bytes.
    pub total: u64,
}

enum Message {
    Progress(ProgressEvent),
    Finished(Result<Vec<ToolReport>, String>),
}

/// State of the installer card.
pub struct ToolInstaller {
    /// Plan awaiting the user's consent.
    pub plan: Option<InstallPlan>,
    /// A run is in progress.
    pub running: bool,
    /// Progress by tool id for the current or last run.
    pub progress: BTreeMap<String, ToolProgress>,
    /// Results of the last finished run.
    pub reports: Vec<ToolReport>,
    /// Error that stopped the last run before any tool was tried.
    pub error: Option<String>,
    context: Option<InstallContext>,
    rx: Option<Receiver<Message>>,
    cancel: Arc<AtomicBool>,
}

impl Default for ToolInstaller {
    fn default() -> Self {
        Self {
            plan: None,
            running: false,
            progress: BTreeMap::new(),
            reports: Vec::new(),
            error: None,
            context: None,
            rx: None,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl ToolInstaller {
    /// Build the plan for every tool and show it for consent.
    pub fn prepare(&mut self, data_root: PathBuf) {
        if self.running {
            return;
        }
        let context = InstallContext::new(data_root).with_check(
            alas_struct::nastran95::INSTALL_CHECK_NAME,
            alas_struct::nastran95::installation_self_check,
        );
        match Manifest::bundled() {
            Ok(manifest) => {
                self.plan = Some(install_plan(&manifest, &context, None));
                self.error = None;
                self.reports.clear();
                self.progress.clear();
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        self.context = Some(context);
    }

    /// Drop the consent screen without downloading anything.
    pub fn dismiss(&mut self) {
        if !self.running {
            self.plan = None;
        }
    }

    /// The user agreed to the plan on screen: start the worker.
    pub fn confirm(&mut self) {
        let (Some(plan), Some(context)) = (self.plan.take(), self.context.clone()) else {
            return;
        };
        if self.running {
            return;
        }
        let consent = ConsentReceipt::for_plan(&plan);
        self.running = true;
        self.cancel.store(false, Ordering::Relaxed);
        self.error = None;
        self.reports.clear();
        let (sender, receiver) = channel();
        self.rx = Some(receiver);
        let cancel = Arc::clone(&self.cancel);
        thread::spawn(move || {
            let outcome = Manifest::bundled()
                .map_err(|error| error.to_string())
                .and_then(|manifest| {
                    let progress_sender = sender.clone();
                    let mut progress = move |event: &ProgressEvent| {
                        let _ = progress_sender.send(Message::Progress(event.clone()));
                    };
                    run_install(&manifest, &context, &plan, &consent, &cancel, &mut progress)
                        .map_err(|error| error.to_string())
                });
            let _ = sender.send(Message::Finished(outcome));
        });
    }

    /// Ask the running install to stop; a partial download is kept for resume.
    pub fn cancel(&mut self) {
        if self.running {
            self.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Remove one installed tool.
    pub fn uninstall(&mut self, data_root: PathBuf, id: &str) -> Result<bool, String> {
        uninstall_tool(&InstallContext::new(data_root), id).map_err(|error| error.to_string())
    }

    /// Absolute executable of an installed tool, if still on disk.
    pub fn installed_executable(data_root: &std::path::Path, id: &str) -> Option<PathBuf> {
        Registry::load(data_root).executable(data_root, id)
    }

    /// Drain worker messages. Returns the reports on the frame the run ended.
    pub fn poll(&mut self) -> Option<Vec<ToolReport>> {
        let mut finished = None;
        let messages: Vec<Message> = self
            .rx
            .as_ref()
            .map(|rx| rx.try_iter().collect())
            .unwrap_or_default();
        for message in messages {
            match message {
                Message::Progress(event) => self.apply(event),
                Message::Finished(outcome) => finished = Some(outcome),
            }
        }
        let outcome = finished?;
        self.running = false;
        self.rx = None;
        match outcome {
            Ok(reports) => {
                self.reports = reports.clone();
                Some(reports)
            }
            Err(error) => {
                self.error = Some(error);
                None
            }
        }
    }

    fn apply(&mut self, event: ProgressEvent) {
        match event {
            ProgressEvent::Started(id) => {
                self.progress.insert(
                    id,
                    ToolProgress {
                        stage: "Downloading".into(),
                        ..ToolProgress::default()
                    },
                );
            }
            ProgressEvent::Downloading { id, done, total } => {
                let entry = self.progress.entry(id).or_default();
                entry.done = done;
                entry.total = total;
            }
            ProgressEvent::Extracting(id) => self.stage(id, "Extracting"),
            ProgressEvent::Building(id) => self.stage(id, "Building"),
            ProgressEvent::HealthCheck(id) => self.stage(id, "Health Check"),
            ProgressEvent::Finished(id) => self.stage(id, "Done"),
        }
    }

    fn stage(&mut self, id: String, text: &str) {
        self.progress.entry(id).or_default().stage = text.into();
    }
}

/// Short label and optional detail for a finished tool.
pub fn describe_result(result: &ToolResult) -> (&'static str, Option<String>) {
    match result {
        ToolResult::Installed { .. } => ("Installed", None),
        ToolResult::AlreadyInstalled => ("Up to Date", None),
        ToolResult::Skipped(_) => ("Not Offered", None),
        ToolResult::Guided(_) => ("Manual Install", None),
        ToolResult::Failed(message) => ("Failed", Some(message.clone())),
        ToolResult::Cancelled => ("Cancelled", None),
    }
}
