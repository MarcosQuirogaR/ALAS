// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Consent before a run sends the configured SimBrief account name to the
//! dispatch service.
//!
//! A configuration file can name an account, so a loaded file must not make a
//! run contact the network. The first run for a loaded file asks, naming the
//! account and the host; the answer holds for the session and for that file
//! and account only. It is never saved.

use alas_config::AlasConfig;
use egui::Context;

use crate::shell_confirm::{confirm_dialog, ConfirmAction, ConfirmAnswer};
use crate::state::AppState;
use crate::views::{tr, tr_fields, tr_label};

/// The host a live dispatch fetch contacts.
pub const DISPATCH_HOST: &str = "www.simbrief.com";

/// What the user answered, per loaded file and account, in this session.
#[derive(Debug, Default)]
pub struct LiveDispatchConsent {
    /// The file-and-account key of the answer, and whether fetching is allowed.
    decided: Option<(String, bool)>,
    /// A run waiting for the answer.
    prompt: Option<LiveDispatchPrompt>,
}

/// The run that waits for the consent question.
#[derive(Debug, Clone)]
struct LiveDispatchPrompt {
    /// Whether the waiting run is the baseline-only run.
    baseline_only: bool,
    /// The account the question names.
    account: String,
    /// The key the answer is recorded under.
    key: String,
}

impl LiveDispatchConsent {
    /// Forget the answer: a different file was loaded.
    pub fn reset(&mut self) {
        self.decided = None;
        self.prompt = None;
    }

    /// Whether the question is on screen.
    pub fn is_asking(&self) -> bool {
        self.prompt.is_some()
    }

    fn key(config_path: &str, account: &str) -> String {
        format!("{config_path}\n{account}")
    }

    fn record(&mut self, key: String, allowed: bool) {
        self.decided = Some((key, allowed));
        self.prompt = None;
    }
}

impl AppState {
    /// Whether the run about to start may fetch its route from the dispatch
    /// service.
    ///
    /// `Some(false)` when no account is configured, the mission is off, or the
    /// user declined; `Some(true)` after consent for this file and account;
    /// `None` while the question is open (the run starts again once it is
    /// answered).
    pub(crate) fn live_dispatch_allowed(
        &mut self,
        config: &AlasConfig,
        baseline_only: bool,
    ) -> Option<bool> {
        let account = config.mission.simbrief_username.trim();
        if !config.mission.enabled || account.is_empty() {
            return Some(false);
        }
        let key = LiveDispatchConsent::key(&self.config_path, account);
        if let Some((decided_key, allowed)) = &self.shell.live_dispatch.decided {
            if *decided_key == key {
                return Some(*allowed);
            }
        }
        self.shell.live_dispatch.prompt = Some(LiveDispatchPrompt {
            baseline_only,
            account: account.to_owned(),
            key,
        });
        None
    }
}

/// Draw the question, when a run waits for it.
pub fn show(state: &mut AppState, ctx: &Context) {
    let Some(prompt) = state.shell.live_dispatch.prompt.clone() else {
        return;
    };
    let body = tr_fields(
        "This run will send the SimBrief account name \"{account}\" to {host} to fetch the route. Nothing is sent unless you allow it. The answer applies to this file until another is loaded.",
        &[
            ("account", prompt.account.clone()),
            ("host", DISPATCH_HOST.to_owned()),
        ],
    );
    let answer = confirm_dialog(
        ctx,
        "alas_live_dispatch_block",
        &tr("Fetch the route from SimBrief?"),
        Some(&body),
        &[
            ConfirmAction::primary(tr_label("Fetch the route")),
            ConfirmAction::plain(tr_label("Run without fetching")),
        ],
    );
    let allowed = match answer {
        Some(ConfirmAnswer::Action(0)) => true,
        Some(ConfirmAnswer::Action(_)) => false,
        Some(ConfirmAnswer::Cancel) => {
            state.shell.live_dispatch.prompt = None;
            return;
        }
        None => return,
    };
    state.shell.live_dispatch.record(prompt.key, allowed);
    state.start_pipeline(prompt.baseline_only);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured(account: &str) -> AlasConfig {
        let mut config = AlasConfig::default();
        config.mission.enabled = true;
        config.mission.simbrief_username = account.to_owned();
        config
    }

    #[test]
    fn no_account_means_no_question_and_no_fetch() {
        let mut state = AppState::default();
        assert_eq!(
            state.live_dispatch_allowed(&configured("  "), false),
            Some(false)
        );
        assert!(!state.shell.live_dispatch.is_asking());
    }

    #[test]
    fn a_configured_account_asks_before_the_first_fetch_and_remembers_the_answer() {
        let mut state = AppState::default();
        let config = configured("pilot42");
        assert_eq!(state.live_dispatch_allowed(&config, true), None);
        assert!(state.shell.live_dispatch.is_asking());

        let key = LiveDispatchConsent::key(&state.config_path, "pilot42");
        state.shell.live_dispatch.record(key, true);
        assert_eq!(state.live_dispatch_allowed(&config, true), Some(true));

        // Another account is a new question; loading a file forgets the answer.
        assert_eq!(
            state.live_dispatch_allowed(&configured("other"), true),
            None
        );
        state.shell.live_dispatch.reset();
        assert_eq!(state.live_dispatch_allowed(&config, true), None);
    }

    #[test]
    fn a_disabled_mission_never_fetches() {
        let mut state = AppState::default();
        let mut config = configured("pilot42");
        config.mission.enabled = false;
        assert_eq!(state.live_dispatch_allowed(&config, false), Some(false));
    }
}
