// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Global search over settings and concepts (Ctrl+K).
//!
//! The index has two sources. Every configuration field the guided pages
//! render (name, label, unit, help from the live schema, so it never drifts
//! from the forms) maps to the flow page and Advanced section that hosts it.
//! A short glossary of concepts, with synonyms, covers terms a user thinks in
//! that are not a field label (MTOW, static margin, TODA). Choosing a result
//! opens the page, expands the section and highlights the field.
//!
//! Every entry also carries where and at which detail level it is drawn
//! (`advanced`, `section`) and the `visible_if` conditions that gate it, so a
//! result is never a target the current configuration hides: [`search_in`]
//! drops entries whose condition is false for the live configuration, and
//! `advanced` is true whenever the target is drawn only at the Advanced level
//! (a field flagged advanced, a page whose rows are Advanced only, or a group
//! the Basic level folds away).

use crate::nav;
use crate::search_concepts::CONCEPTS;
use crate::views::form_page::placement;

/// One searchable target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchEntry {
    /// The shown title: a field label or a concept name (English).
    pub title: String,
    /// The unit, when the target is a field with one.
    pub unit: String,
    /// The page id that hosts the target.
    pub page: &'static str,
    /// The Advanced section of that page, when the target sits in one.
    pub section: Option<&'static str>,
    /// Dotted path of the field to highlight, when the target is one field.
    pub field: Option<String>,
    /// The registered preset the entry loads, by key, when it is one.
    pub preset: Option<String>,
    /// Whether the entry is a page of the navigation tree.
    pub is_page: bool,
    /// Whether the target is only shown at the Advanced detail level.
    pub advanced: bool,
    /// The `visible_if` conditions of the target and its groups, as the
    /// dotted path of the controlling value and the values that show it.
    visible_if: Vec<Condition>,
    haystack: String,
    /// The field's one-sentence help, lower case; a match here ranks above a
    /// match that is only in the long help.
    summary: String,
}

/// A condition on the configuration: the dotted path of the controlling value,
/// the values it is compared with and whether the target shows while the
/// value is one of them (`false`) or while it is none of them (`true`).
type Condition = (&'static str, &'static [&'static str], bool);

/// Conditions the forms apply without a `visible_if` in the schema: the
/// freighter-only cargo objective and the engine designer's cycle rows, which
/// follow the engine class (see `placement::propulsion_cycle_field_applies`).
fn implied_condition(path: &str) -> Option<Condition> {
    const TECHNOLOGY: &str = "geometry.engine.propulsion_technology";
    if path == "requirements.cargo_objective_kg" {
        return Some(("requirements.aircraft_type", &["cargo"], false));
    }
    // The shaft-power mass inputs belong to a turboprop only.
    if path == "mass_model.flops_turboprop" || path.starts_with("mass_model.flops_turboprop.") {
        return Some((TECHNOLOGY, &["turboprop"], false));
    }
    let name = path.strip_prefix("propulsion_cycle.")?;
    match (
        placement::propulsion_cycle_field_applies(name, false),
        placement::propulsion_cycle_field_applies(name, true),
    ) {
        (true, false) => Some((TECHNOLOGY, &["turboprop"], true)),
        (false, true) => Some((TECHNOLOGY, &["turboprop"], false)),
        _ => None,
    }
}

/// What the shell highlights after a result is chosen.
#[derive(Debug, Clone, PartialEq)]
pub struct Focus {
    /// Dotted path of the field, with the group as the first segment.
    pub field: Option<String>,
    /// The Advanced section to force open.
    pub section: Option<&'static str>,
    /// The egui time (s) at which the highlight started; `None` until the
    /// first frame after the jump stamps it.
    pub started: Option<f64>,
    /// Seconds since the highlight started.
    pub elapsed: f32,
}

impl Focus {
    /// A highlight that starts on the next frame.
    pub fn new(field: Option<String>, section: Option<&'static str>) -> Self {
        Self {
            field,
            section,
            started: None,
            elapsed: 0.0,
        }
    }
}

/// Seconds the highlight and forced expansion last after a jump.
pub const FOCUS_SECONDS: f32 = 1.8;

/// Seconds, from the jump, during which the target keeps being scrolled into
/// view. Longer than a frame or two because a collapsed section or a lazily
/// laid out card may only reach its place a few frames after the jump.
const SCROLL_SECONDS: f32 = 0.8;

/// The strength (0 to 1) of the pulse `elapsed` seconds into the highlight:
/// full at the start, two soft beats, gone at [`FOCUS_SECONDS`]. It never
/// drops below a quarter of its start before the last tenth, so a screenshot
/// or a glance half a second after the jump still sees the target marked.
pub fn pulse_strength(elapsed: f32) -> f32 {
    let t = (elapsed / FOCUS_SECONDS).clamp(0.0, 1.0);
    let beats = 0.75 + 0.25 * (t * std::f32::consts::TAU * 2.0).cos();
    let fade = if t > 0.9 {
        (1.0 - t) * 10.0
    } else {
        1.0 - 0.6 * t
    };
    fade * beats
}

/// The search box, its results and the pending highlight.
#[derive(Debug, Default)]
pub struct SearchState {
    /// Whether the overlay is open.
    pub open: bool,
    /// The query text.
    pub query: String,
    /// The highlighted result row.
    pub selected: usize,
    /// The pending jump highlight.
    pub focus: Option<Focus>,
    /// The index and the language it was built for.
    pub index: Option<(String, Vec<SearchEntry>)>,
    /// Whether the query box still needs keyboard focus.
    pub wants_focus: bool,
    /// Enter was pressed this frame while the palette was open. The key is
    /// taken out of the input at the start of the frame so no widget drawn
    /// before the palette (a focused Run button) can see it.
    pub enter: bool,
    /// Arrow-key steps taken this frame, down positive.
    pub step: i32,
}

#[path = "search_index.rs"]
mod index;
pub use index::{build_index, build_index_with_presets};

/// The entries matching `query`, best first. Every whitespace-separated token
/// must occur in the entry; a title that starts with the first token, then a
/// title that contains it, rank above a match elsewhere in the entry; concepts
/// rank above fields on ties, and an entry drawn at Basic above an equal one
/// drawn only at Advanced.
pub fn search<'a>(index: &'a [SearchEntry], query: &str, limit: usize) -> Vec<&'a SearchEntry> {
    search_in(index, query, limit, None)
}

impl SearchEntry {
    /// Whether the target exists for the configuration `config`: every
    /// `visible_if` condition of the field and its groups holds. Without a
    /// configuration every entry is available.
    pub fn available(&self, config: Option<&serde_json::Value>) -> bool {
        let Some(config) = config else { return true };
        self.visible_if.iter().all(|(path, values, negate)| {
            let listed = path
                .split('.')
                .try_fold(config, |value, segment| value.get(segment))
                .and_then(|value| {
                    // A switch compares as "true" or "false", as in the forms.
                    value
                        .as_str()
                        .map(str::to_owned)
                        .or_else(|| value.as_bool().map(|on| on.to_string()))
                })
                .is_some_and(|current| values.contains(&current.as_str()));
            listed != *negate
        })
    }
}

/// The score of an entry that matches the query only outside its title.
const WEAK_MATCH_SCORE: u32 = 10;

/// [`search`] over the entries available for the live configuration `config`.
pub fn search_in<'a>(
    index: &'a [SearchEntry],
    query: &str,
    limit: usize,
    config: Option<&serde_json::Value>,
) -> Vec<&'a SearchEntry> {
    let tokens: Vec<String> = query
        .to_lowercase()
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    if tokens.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(u32, &SearchEntry)> = index
        .iter()
        .filter(|entry| entry.available(config))
        .filter(|entry| {
            tokens
                .iter()
                .all(|token| entry.haystack.contains(token.as_str()))
        })
        .map(|entry| {
            let title = entry.title.to_lowercase();
            let translated = alas_i18n::t(Some(entry.title.as_str()), None).to_lowercase();
            let query = tokens.join(" ");
            let words = |text: &str| -> Vec<String> {
                text.split(|c: char| !c.is_alphanumeric())
                    .filter(|w| !w.is_empty())
                    .map(str::to_owned)
                    .collect()
            };
            let title_words = words(&title);
            let translated_words = words(&translated);
            let whole_words =
                |words: &[String]| tokens.iter().all(|t| words.iter().any(|w| w == t));
            // 0: the whole title; 1: every token is a whole word of the
            // title; 2: the title starts with the first token; 3: the title
            // contains the tokens; 5: the field's own name (the dotted path)
            // contains them; 6: a concept's synonym; 10: help text only.
            let mut score = WEAK_MATCH_SCORE;
            if title == query || translated == query {
                score = 0;
            } else if whole_words(&title_words) || whole_words(&translated_words) {
                score = 1;
            } else if title.starts_with(&tokens[0]) || translated.starts_with(&tokens[0]) {
                score = 2;
            } else if tokens
                .iter()
                .all(|t| title.contains(t.as_str()) || translated.contains(t.as_str()))
            {
                score = 3;
            } else if let Some(path) = &entry.field {
                let path = path.to_lowercase().replace(['_', '.'], " ");
                if tokens.iter().all(|t| path.contains(t.as_str())) {
                    score = 5;
                }
            }
            if score == WEAK_MATCH_SCORE
                && tokens.iter().all(|t| entry.summary.contains(t.as_str()))
            {
                score = 8;
            }
            // A named concept is matched on purpose through its synonyms, so
            // a synonym hit is not a weak match.
            if score == WEAK_MATCH_SCORE && CONCEPTS.iter().any(|c| c.title == entry.title) {
                score = 6;
            }
            // A page or a preset is named by its words (its title, id and
            // aliases): a whole-word hit ranks with a title word match.
            if score == WEAK_MATCH_SCORE && (entry.is_page || entry.preset.is_some()) {
                let haystack_words = words(&entry.haystack);
                if whole_words(&haystack_words) {
                    score = 1;
                }
            }
            if entry.field.is_some() && score == 3 {
                score += 1;
            }
            (score, entry)
        })
        .collect();
    // A match that is only in the help text or a field name ("propeller" in
    // the help of an unrelated speed factor) is noise whenever some title
    // matches: those weaker matches are dropped, a concept's synonyms stay.
    if scored.iter().any(|(score, _)| *score < 5) {
        scored
            .retain(|(score, entry)| *score < 5 || CONCEPTS.iter().any(|c| c.title == entry.title));
    } else if scored.iter().any(|(score, _)| *score < WEAK_MATCH_SCORE) {
        scored.retain(|(score, _)| *score < WEAK_MATCH_SCORE);
    }
    scored.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.advanced.cmp(&b.1.advanced))
            .then_with(|| a.1.title.cmp(&b.1.title))
    });
    scored
        .into_iter()
        .take(limit)
        .map(|(_, entry)| entry)
        .collect()
}

impl SearchEntry {
    /// An entry that does not come from the settings schema: a target the
    /// caller indexes itself (the Sandbox parameters). `page` and `section`
    /// are only shown as the location, `field` is the path to highlight and
    /// `haystack` the text the query is matched against.
    pub fn scoped(
        title: &str,
        unit: &str,
        page: &'static str,
        section: Option<&'static str>,
        field: &str,
        haystack: &str,
    ) -> Self {
        Self {
            title: title.to_owned(),
            unit: unit.to_owned(),
            page,
            section,
            field: Some(field.to_owned()),
            preset: None,
            is_page: false,
            advanced: false,
            visible_if: Vec::new(),
            haystack: haystack.to_lowercase(),
            summary: String::new(),
        }
    }

    /// The entry with `summary` as its one-sentence help, which ranks a hit
    /// above one found only in the long haystack (the same tier the schema
    /// index gives a field's summary).
    pub(crate) fn with_summary(mut self, summary: &str) -> Self {
        self.summary = summary.to_lowercase();
        self
    }

    /// The page path shown under the title, for example `Aircraft > Geometry`.
    pub fn location(&self) -> String {
        let page = nav::page(self.page).map_or(self.page, |page| page.title);
        match self.section {
            Some(section) => format!(
                "{} > {}",
                alas_i18n::t(Some(page), None),
                alas_i18n::t(Some(section), None)
            ),
            None => alas_i18n::t(Some(page), None).into_owned(),
        }
    }
}

/// The focus as the form renderers read it: the field path below its group in
/// the renderers' own `::` form, and whether the jump just happened.
#[derive(Debug, Clone, PartialEq)]
pub struct PublishedFocus {
    /// For `geometry.wing.root_airfoil`, `::wing::root_airfoil`.
    pub tail: String,
    /// The full dotted path, `geometry.wing.root_airfoil`; what
    /// [`focus_anchor`] matches.
    pub path: String,
    /// Whether the page should still scroll the field into view.
    pub fresh: bool,
    /// The pulse strength (0 to 1) of this frame, see [`pulse_strength`].
    pub pulse: f32,
}

fn focus_key() -> egui::Id {
    egui::Id::new("alas_search_focus")
}

/// Publish the pending highlight for this frame (or clear it).
pub fn publish_focus(ctx: &egui::Context, focus: Option<&Focus>) {
    let published = focus.and_then(|focus| {
        let path = focus.field.as_ref()?;
        // A top-level key (`departure_airport`) has no group below which the
        // generic forms could find it: only bespoke anchors match its path.
        let tail = path.split_once('.').map_or_else(String::new, |(_, rest)| {
            format!("::{}", rest.replace('.', "::"))
        });
        Some(PublishedFocus {
            tail,
            path: path.clone(),
            fresh: focus.elapsed < SCROLL_SECONDS,
            pulse: pulse_strength(focus.elapsed),
        })
    });
    ctx.data_mut(|data| match published {
        Some(value) => data.insert_temp(focus_key(), value),
        None => {
            data.remove::<bool>(anchored_key());
            data.remove::<PublishedFocus>(focus_key())
        }
    });
}

/// The highlight published this frame, if any.
pub fn published_focus(ctx: &egui::Context) -> Option<PublishedFocus> {
    ctx.data(|data| data.get_temp::<PublishedFocus>(focus_key()))
}

/// Whether the search is highlighting `path` (a dotted configuration path)
/// this frame. A bespoke editor uses it to open the card that holds the
/// control before it draws it.
pub fn focus_wants(ctx: &egui::Context, path: &str) -> bool {
    published_focus(ctx)
        .is_some_and(|focus| focus.path == path || focus.path.starts_with(&format!("{path}.")))
}

/// Whether the search is highlighting any of `paths` or a field below one:
/// a bespoke card opens when this holds.
pub fn focus_wants_any(ctx: &egui::Context, paths: &[&str]) -> bool {
    paths.iter().any(|path| focus_wants(ctx, path))
}

/// For editors that draw their own controls: call it with the dotted
/// configuration `path` the control edits and the `rect` the control occupies.
/// When the search is highlighting that path it scrolls the rect into view
/// (while the jump is fresh) and draws the one-second pulse around it; any
/// other time it does nothing.
pub fn focus_anchor(ui: &mut egui::Ui, path: &str, rect: egui::Rect) {
    let Some(focus) = published_focus(ui.ctx()).filter(|focus| focus.path == path) else {
        return;
    };
    pulse(ui, &focus, rect, rect);
}

/// [`focus_anchor`] for the schema form renderers, which know a control by its
/// `::` path below the group (see [`PublishedFocus::tail`]). `scroll_to` is
/// the part of a tall target (a group card) that is centred on screen.
pub(crate) fn focus_anchor_tail(
    ui: &mut egui::Ui,
    tail: &str,
    rect: egui::Rect,
    scroll_to: egui::Rect,
) {
    let Some(focus) = published_focus(ui.ctx()).filter(|focus| focus.tail == tail) else {
        return;
    };
    pulse(ui, &focus, rect, scroll_to);
}

/// Scroll `rect` into the centre of the page while a jump to `section` is fresh: for a target
/// that is no schema field (a figure card), found by the section the result names.
pub(crate) fn scroll_to_section(
    ui: &egui::Ui,
    section: &str,
    rect: egui::Rect,
    focus: Option<&Focus>,
) {
    let Some(focus) = focus.filter(|focus| focus.section == Some(section)) else {
        return;
    };
    if focus.elapsed < SCROLL_SECONDS {
        let frame = ui.ctx().cumulative_pass_nr();
        ui.ctx()
            .data_mut(|data| data.insert_temp(scroll_request_key(), (frame, rect)));
    }
}

fn anchored_key() -> egui::Id {
    egui::Id::new("alas_search_focus_anchored")
}

/// Whether some control drew the pulse of the published highlight this frame
/// or an earlier one since [`publish_focus`] last cleared it. The jump tests
/// use it to prove a result lands on something drawn.
#[cfg(test)]
pub(crate) fn focus_was_anchored(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<bool>(anchored_key()).unwrap_or(false))
}

/// The one highlight every target shares: a tinted fill and a stroke around
/// `rect` that fade over [`FOCUS_SECONDS`], with `scroll_to` centred while the
/// jump is fresh.
fn pulse(ui: &mut egui::Ui, focus: &PublishedFocus, rect: egui::Rect, scroll_to: egui::Rect) {
    ui.ctx()
        .data_mut(|data| data.insert_temp(anchored_key(), true));
    let accent = ui.visuals().hyperlink_color;
    let strength = focus.pulse.clamp(0.0, 1.0);
    let alpha = (255.0 * strength) as u8;
    let color = egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), alpha);
    ui.painter().rect_filled(
        rect.expand(3.0),
        6.0,
        egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), alpha / 4),
    );
    ui.painter()
        .rect_stroke(rect.expand(3.0), 6.0, egui::Stroke::new(2.5_f32, color));
    if focus.fresh {
        // Not `ui.scroll_to_rect` here: egui hands that request to the first
        // scroll area that ENDS after it, and every scroll area drawn later on
        // the page (a table, a nested list) ends before the page's own one and
        // swallows it. The page re-issues it from the end of its content,
        // see [`flush_scroll`].
        let frame = ui.ctx().cumulative_pass_nr();
        ui.ctx()
            .data_mut(|data| data.insert_temp(scroll_request_key(), (frame, scroll_to)));
    }
    #[cfg(test)]
    ui.ctx()
        .data_mut(|data| data.insert_temp(scrolled_key(), (scroll_to, ui.clip_rect())));
}

fn scroll_request_key() -> egui::Id {
    egui::Id::new("alas_search_focus_scroll_request")
}

/// Call as the LAST statement inside the content of a page's scroll area: it
/// scrolls the area so the jump target (drawn above in this frame) sits in
/// the centre of the visible part. The request repeats every frame while the
/// jump is fresh, so a section the jump just opened, which grows the layout
/// over a few frames, is still followed until it is in view.
pub fn flush_scroll(ui: &mut egui::Ui) {
    let frame = ui.ctx().cumulative_pass_nr();
    let request = ui
        .ctx()
        .data(|data| data.get_temp::<(u64, egui::Rect)>(scroll_request_key()));
    if let Some((_, rect)) = request.filter(|(requested, _)| *requested == frame) {
        ui.scroll_to_rect(rect, Some(egui::Align::Center));
    }
}

#[cfg(test)]
fn scrolled_key() -> egui::Id {
    egui::Id::new("alas_search_focus_scrolled")
}

/// The rect the last pulse scrolled to and the clip rect it was drawn in.
#[cfg(test)]
pub(crate) fn last_scroll_target(ctx: &egui::Context) -> Option<(egui::Rect, egui::Rect)> {
    ctx.data(|data| data.get_temp(scrolled_key()))
}

/// Apply a chosen result: show the page at the level that reveals the target
/// and start the highlight.
pub fn jump_to(state: &mut crate::state::AppState, entry: &SearchEntry) {
    // The Sandbox indexes its own parameters and has no guided page to open.
    if state.sandbox.active() {
        crate::sandbox::search::jump(state, entry);
        return;
    }
    if let Some(preset) = &entry.preset {
        // A preset replaces the aircraft: it goes through the same
        // replace-unsaved-changes confirmation as the Aircraft page's list,
        // and a clean sheet is confirmed too.
        crate::app::go_to_page(state, entry.page);
        let action = crate::shell_confirm::Pending::Preset(preset.clone());
        if state.starting_design() != crate::sandbox::StartingDesign::PresetAircraft {
            state.shell.pending = Some(action);
        } else {
            state.guarded(action);
        }
        state.shell.search.open = false;
        state.shell.search.query.clear();
        state.shell.search.selected = 0;
        return;
    }
    if entry.advanced {
        state.shell.detail = crate::detail::Detail::Advanced;
    }
    crate::app::go_to_page(state, entry.page);
    state.shell.search.focus = Some(Focus::new(entry.field.clone(), entry.section));
    state.shell.search.open = false;
    state.shell.search.query.clear();
    state.shell.search.selected = 0;
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod acceptance_tests;

#[cfg(test)]
#[path = "search_focus_tests.rs"]
mod focus_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> Vec<SearchEntry> {
        let state = crate::state::AppState::default();
        build_index(&state.schema)
    }

    fn index_with_presets(state: &crate::state::AppState) -> Vec<SearchEntry> {
        build_index_with_presets(&state.schema, &state.preset_names)
    }

    #[test]
    fn typing_inputs_finds_the_inputs_page_first() {
        let state = crate::state::AppState::default();
        let index = index_with_presets(&state);
        let hits = search(&index, "Inputs", 5);
        let first = hits.first().expect("a hit");
        assert!(first.is_page && first.page == "inputs", "{first:?}");
    }

    #[test]
    fn typing_a_preset_name_finds_the_preset_before_anything_else() {
        let state = crate::state::AppState::default();
        let index = index_with_presets(&state);
        for (query, key) in [("ATR", "ATR72-600"), ("a320", "A320-200"), ("ave", "AVE")] {
            let hits = search(&index, query, 5);
            let first = hits.first().expect("a hit");
            assert_eq!(first.preset.as_deref(), Some(key), "{query}: {first:?}");
        }
    }

    #[test]
    fn choosing_a_preset_entry_asks_before_replacing_unsaved_changes() {
        let mut state = crate::state::AppState::default();
        state.perform(crate::shell_confirm::Pending::Preset("A320-200".to_owned()));
        assert!(!state.has_unsaved_edits());
        let index = index_with_presets(&state);
        let entry = search(&index, "ATR", 1)[0].clone();
        // Clean case: the preset loads at once.
        jump_to(&mut state, &entry);
        assert_eq!(state.active_preset, "ATR72-600");
        assert!(state.shell.pending.is_none());
        // Edited case: the same confirmation as the Aircraft page's list.
        state.shell.undo.mark_unsaved();
        let a320 = search(&index, "A320", 1)[0].clone();
        jump_to(&mut state, &a320);
        assert_eq!(state.active_preset, "ATR72-600", "nothing replaced yet");
        assert!(state.shell.pending.is_some());
    }

    #[test]
    fn a_field_label_finds_its_page_section_and_path() {
        let index = index();
        let hits = search(&index, "root airfoil", 5);
        let hit = hits
            .iter()
            .find(|entry| entry.field.as_deref() == Some("geometry.wing.root_airfoil"))
            .expect("root airfoil is searchable");
        assert_eq!(hit.page, "inputs");
        assert_eq!(hit.section, Some("Aerodynamics"));
    }

    #[test]
    fn a_unit_or_synonym_finds_a_concept() {
        let index = index();
        let hits = search(&index, "toda", 5);
        assert!(hits.iter().any(|entry| entry.title == "Field Length"));
    }

    #[test]
    fn every_token_must_match_and_no_query_matches_nothing() {
        let index = index();
        assert!(search(&index, "", 5).is_empty());
        assert!(search(&index, "zzzz-no-such-setting", 5).is_empty());
        assert!(!search(&index, "cruise mach", 5).is_empty());
    }

    #[test]
    fn the_published_focus_uses_the_renderers_path_form() {
        let ctx = egui::Context::default();
        let focus = Focus::new(Some("geometry.wing.root_airfoil".to_owned()), None);
        publish_focus(&ctx, Some(&focus));
        let published = published_focus(&ctx).expect("published");
        assert_eq!(published.tail, "::wing::root_airfoil");
        assert_eq!(published.path, "geometry.wing.root_airfoil");
        assert!(published.fresh);
        assert!(focus_wants(&ctx, "geometry.wing"));
        publish_focus(&ctx, None);
        assert_eq!(published_focus(&ctx), None);
    }

    #[test]
    fn the_pulse_stays_visible_through_the_jump_and_the_scroll_outlives_the_first_frames() {
        assert!(pulse_strength(0.0) > 0.9);
        // Still clearly drawn half a second and most of the way through.
        assert!(pulse_strength(0.5) > 0.3, "{}", pulse_strength(0.5));
        assert!(pulse_strength(FOCUS_SECONDS * 0.8) > 0.2);
        assert_eq!(pulse_strength(FOCUS_SECONDS), 0.0);
        let ctx = egui::Context::default();
        let mut focus = Focus::new(Some("geometry.wing.root_airfoil".to_owned()), None);
        focus.elapsed = 0.5;
        publish_focus(&ctx, Some(&focus));
        assert!(published_focus(&ctx).expect("published").fresh);
    }

    #[test]
    fn every_concept_field_is_an_indexed_field_of_the_schema() {
        let index = index();
        let missing: Vec<String> = CONCEPTS
            .iter()
            .filter_map(|concept| Some((concept.title, concept.field?)))
            .filter(|(title, path)| {
                !index
                    .iter()
                    .any(|entry| entry.field.as_deref() == Some(*path) && entry.title != *title)
            })
            .map(|(title, path)| format!("{title} -> {path}"))
            .collect();
        assert!(missing.is_empty(), "not indexed fields: {missing:?}");
    }

    #[test]
    fn every_indexed_page_exists() {
        for entry in index() {
            assert!(
                nav::page(entry.page).is_some(),
                "{} -> {}",
                entry.title,
                entry.page
            );
        }
    }
}
