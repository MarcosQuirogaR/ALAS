// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Publication hygiene: nothing shipped or published may carry internal
//! programme documents or name an AI assistant or agent tool.
//!
//! The needles are assembled from fragments so that this file, which ships in
//! the source snapshot, does not trip the scan it implements.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicUsize, Ordering},
        LazyLock, Mutex, PoisonError,
    },
};

/// Repository-relative directories that never leave the private repository.
fn private_prefixes() -> Vec<String> {
    vec![
        ["docs/", "internal"].concat(),
        ["docs/", "research"].concat(),
    ]
}

/// Whole-word needles (ASCII, case-insensitive). A word character is an ASCII
/// letter or digit, so an underscore does not hide a word in an identifier.
fn word_needles() -> Vec<String> {
    [
        ["clau", "de"],
        ["anthro", "pic"],
        ["co", "dex"],
        ["co", "pilot"],
        ["chat", "gpt"],
        ["notebook", "lm"],
        ["gem", "ini"],
        ["open", "ai"],
    ]
    .iter()
    .map(|parts| parts.concat())
    .collect()
}

/// Phrase needles (ASCII, case-insensitive substring): trailers, banners and
/// the paths of private working trees.
fn phrase_needles(include_paths: bool) -> Vec<String> {
    let mut needles = vec![
        ["co-authored", "-by:"].concat(),
        ["generated ", "with ["].concat(),
    ];
    if include_paths {
        needles.extend([
            [".age", "nt/"].concat(),
            ["docs/", "internal"].concat(),
            ["docs/", "research"].concat(),
            // Personal absolute paths of the author's machine.
            [":/proy", "ectos"].concat(),
            ["/users/", "marcos"].concat(),
            ["/home/", "marcos"].concat(),
            // Build-target folders of the author's machine.
            [":/alas", "-targets"].concat(),
            [":/alas", "-scratch"].concat(),
            [":/alas", "-baselines"].concat(),
            // Agent-workflow vocabulary: worktree names, hand-off files, task briefs.
            ["wt", "-v15"].concat(),
            ["wt", "-audit"].concat(),
            ["hand", "off.md"].concat(),
            ["folder", "-rules"].concat(),
            ["sub", "agent"].concat(),
            ["worker", " brief"].concat(),
            ["fix ", "lane"].concat(),
            ["programme", "-20"].concat(),
        ]);
    }
    needles
}

/// Programme bookkeeping that must not ship: a numbered design decision, an
/// audit finding id, the unpublished UAV plan and private branch names.
fn programme_reference(lower: &str) -> Option<String> {
    let decision = ["decision", " d"].concat();
    let mut from = 0;
    while let Some(offset) = lower[from..].find(&decision) {
        let end = from + offset + decision.len();
        if lower.as_bytes().get(end).is_some_and(u8::is_ascii_digit) {
            return Some(decision.trim().to_owned() + " Dn");
        }
        from = end;
    }
    let audit_id = ["siz", "-0"].concat();
    if let Some(offset) = lower.find(&audit_id) {
        let before_ok = offset == 0 || !is_word_byte(lower.as_bytes()[offset - 1]);
        if before_ok {
            return Some(audit_id);
        }
    }
    [["uav", "-plan"].concat(), ["integration", "-2026"].concat()]
        .into_iter()
        .find(|needle| lower.contains(needle.as_str()))
}

/// Author or committer addresses that must not appear on a published commit
/// (an older secondary identity of the author). Returns the matching fragment.
fn secondary_identity(identities: &str) -> Option<String> {
    let lower = identities.to_ascii_lowercase();
    let needle = ["shat", "men4@"].concat();
    lower.contains(&needle).then_some(needle)
}

/// A reviewed, line-level exemption: the word is ordinary data in this file.
struct Exemption {
    /// Repository-relative path suffix of the file.
    path_suffix: String,
    needle: String,
    /// The exempt line starts with this text.
    line_prefix: String,
}

/// The needle tables are fixed text; build each once rather than per scanned line.
static WORD_NEEDLES: LazyLock<Vec<String>> = LazyLock::new(word_needles);
static PHRASES_WITH_PATHS: LazyLock<Vec<String>> = LazyLock::new(|| phrase_needles(true));
static PHRASES_WITHOUT_PATHS: LazyLock<Vec<String>> = LazyLock::new(|| phrase_needles(false));
static EXEMPTIONS: LazyLock<Vec<Exemption>> = LazyLock::new(exemptions);

fn exemptions() -> Vec<Exemption> {
    vec![Exemption {
        // A Selig airfoil database entry whose name is an ordinary word.
        path_suffix: "crates/alas-geom/data/selig.txt".to_owned(),
        needle: ["gem", "ini"].concat(),
        line_prefix: ["GEM", "INI (smoothed)"].concat(),
    }]
}

/// True for a repository-relative path inside a private documentation tree.
pub(crate) fn is_internal_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    let normalized = normalized.trim_start_matches("./");
    private_prefixes().iter().any(|prefix| {
        normalized == prefix
            || normalized.starts_with(&format!("{prefix}/"))
            || normalized.contains(&format!("/{prefix}/"))
    })
}

/// True for a file that only exists in the private working history: a path in
/// a private documentation tree, or a hand-off note wherever it sits.
pub(crate) fn is_private_work_file(path: &str) -> bool {
    if is_internal_path(path) {
        return true;
    }
    let normalized = path.replace('\\', "/");
    let name = normalized.rsplit('/').next().unwrap_or("");
    let lower = name.to_ascii_lowercase();
    let stem = lower.strip_suffix(".md").unwrap_or(&lower);
    stem.starts_with(&["hand", "off"].concat())
}

/// Where the checked tree is going. Publication is the default: only an
/// explicit `--private-branch` lets private working files stay tracked.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Context {
    /// A public ref, a release archive or a pull request into main.
    Public,
    /// A private work branch that never reaches the public remote.
    PrivateBranch,
}

fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
}

/// True when `gpt` at `start..end` of `lower` is a numbered model name.
fn is_gpt_model(bytes: &[u8], start: usize, end: usize) -> bool {
    let before_ok = start == 0 || !is_word_byte(bytes[start - 1]);
    let after = &bytes[end..];
    let numbered = after.first().is_some_and(u8::is_ascii_digit)
        || (after.first() == Some(&b'-') && after.get(1).is_some_and(u8::is_ascii_digit));
    before_ok && numbered
}

/// The needle that `line` references, if any.
fn reference_in(label: &str, line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    // JSON and Rust strings escape a backslash; compare paths in one spelling.
    let slashed = lower.replace("\\\\", "/").replace('\\', "/");
    let bytes = lower.as_bytes();
    let normalized_label = label.replace('\\', "/");
    let exempt = |needle: &str| {
        EXEMPTIONS.iter().any(|e| {
            e.needle == needle
                && normalized_label.ends_with(&e.path_suffix)
                && line.starts_with(&e.line_prefix)
        })
    };
    for needle in WORD_NEEDLES.iter() {
        let mut from = 0;
        while let Some(offset) = lower[from..].find(needle.as_str()) {
            let start = from + offset;
            let end = start + needle.len();
            let before_ok = start == 0 || !is_word_byte(bytes[start - 1]);
            let after_ok = end >= bytes.len() || !is_word_byte(bytes[end]);
            if before_ok && after_ok && !exempt(needle) {
                return Some(needle.clone());
            }
            from = end;
        }
    }
    let gpt = ["g", "pt"].concat();
    let mut from = 0;
    while let Some(offset) = lower[from..].find(&gpt) {
        let start = from + offset;
        let end = start + gpt.len();
        if is_gpt_model(bytes, start, end) {
            return Some(gpt);
        }
        from = end;
    }
    // Ignore and attributes files legitimately name the private trees they keep out of git.
    let is_ignore_file =
        normalized_label.ends_with(".gitignore") || normalized_label.ends_with(".gitattributes");
    let phrases = if is_ignore_file {
        &PHRASES_WITHOUT_PATHS
    } else {
        &PHRASES_WITH_PATHS
    };
    phrases
        .iter()
        .find(|needle| slashed.contains(needle.as_str()))
        .cloned()
        .or_else(|| programme_reference(&lower))
}

/// One finding per offending line of `text`, labelled with `label`.
pub(crate) fn scan_text(label: &str, text: &str) -> Vec<String> {
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            reference_in(label, line).map(|needle| {
                format!(
                    "{label}:{}: references an AI assistant, agent tool or private path ({needle})",
                    index + 1
                )
            })
        })
        .collect()
}

/// Scan raw file content. Files with NUL bytes are binary and skipped; any
/// other file is read as lossy UTF-8 so a stray byte cannot hide a match.
pub(crate) fn scan_bytes(label: &str, bytes: &[u8]) -> Vec<String> {
    // UTF-16 text (byte-order mark) is full of NUL bytes but is not binary.
    let utf16 = |little_endian: bool, body: &[u8]| -> String {
        let units: Vec<u16> = body
            .chunks_exact(2)
            .map(|pair| {
                if little_endian {
                    u16::from_le_bytes([pair[0], pair[1]])
                } else {
                    u16::from_be_bytes([pair[0], pair[1]])
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    if let Some(body) = bytes.strip_prefix(&[0xff, 0xfe]) {
        return scan_text(label, &utf16(true, body));
    }
    if let Some(body) = bytes.strip_prefix(&[0xfe, 0xff]) {
        return scan_text(label, &utf16(false, body));
    }
    if bytes.contains(&0) {
        return Vec::new();
    }
    scan_text(label, &String::from_utf8_lossy(bytes))
}
fn collect_files(directory: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(directory)
        .map_err(|e| format!("failed to enumerate {}: {e}", directory.display()))?
    {
        let entry = entry.map_err(|e| format!("failed to read package entry: {e}"))?;
        let path = entry.path();
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            collect_files(&path, output)?;
        } else {
            output.push(path);
        }
    }
    Ok(())
}

/// Findings for an assembled package directory: internal documents present,
/// or AI references in any shipped text file.
pub(crate) fn package_findings(package_dir: &Path) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    collect_files(package_dir, &mut files)?;
    files.sort();
    let mut findings = Vec::new();
    for file in files {
        let relative = file
            .strip_prefix(package_dir)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");
        if is_private_work_file(&relative) {
            findings.push(format!("{relative}: internal document in the package"));
            continue;
        }
        let bytes =
            fs::read(&file).map_err(|e| format!("failed to read {}: {e}", file.display()))?;
        findings.extend(scan_bytes(&relative, &bytes));
    }
    Ok(findings)
}

/// Refuse to continue packaging when the assembled package is not clean.
pub(crate) fn verify_package(package_dir: &Path) -> Result<(), String> {
    let findings = package_findings(package_dir)?;
    if findings.is_empty() {
        println!("Publication hygiene: package contains no internal documents or AI references");
        return Ok(());
    }
    Err(format!(
        "publication hygiene failed for the package:\n{}",
        findings.join("\n")
    ))
}

fn git_stdout(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|e| format!("failed to run git {}: {e}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!("git {} failed ({})", args.join(" "), output.status));
    }
    Ok(output.stdout)
}

/// `cargo xtask hygiene [--private-branch] [--range A..B] [--text FILE]...`
///
/// Scans every tracked file, the commit messages and author/committer
/// identities of the range, and any extra text files (pull request bodies).
/// By default the tree is checked as a public ref: a tracked path in a private
/// documentation tree or a hand-off note is a finding. `--private-branch`
/// names the private work-branch case, where those files are allowed to be
/// tracked (their content is still not scanned).
/// What scanning one tracked path produced.
#[derive(Default)]
struct ScanOutcome {
    findings: Vec<String>,
    scanned: usize,
    deleted: usize,
}

fn scan_listed(root: &Path, item: &[u8], context: Context) -> ScanOutcome {
    let mut outcome = ScanOutcome::default();
    let Ok(relative) = std::str::from_utf8(item) else {
        outcome.findings.push(format!(
            "{}: tracked path is not valid UTF-8 and was not scanned",
            String::from_utf8_lossy(item)
        ));
        return outcome;
    };
    if is_private_work_file(relative) {
        if context == Context::Public {
            outcome.findings.push(format!(
                "{relative}: private working file tracked on a public ref"
            ));
        }
        return outcome;
    }
    let bytes = match fs::read(root.join(relative)) {
        Ok(bytes) => bytes,
        Err(_) if !root.join(relative).exists() => {
            outcome.deleted = 1;
            return outcome;
        }
        Err(error) => {
            outcome
                .findings
                .push(format!("{relative}: cannot be read ({error})"));
            return outcome;
        }
    };
    outcome.scanned = 1;
    outcome.findings = scan_bytes(relative, &bytes);
    outcome
}

/// Worker threads for the file scan: `XTASK_THREADS` when set, otherwise the
/// available parallelism, at least one.
fn scan_threads() -> usize {
    std::env::var("XTASK_THREADS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
        })
}

/// `work` over every item, results in item order regardless of the thread count.
fn map_in_order<T: Sync, R: Send>(items: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = scan_threads().min(items.len()).max(1);
    if threads == 1 {
        return items.iter().map(&work).collect();
    }
    let slots: Vec<Mutex<Option<R>>> = items.iter().map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(item) = items.get(index) else { break };
                let value = work(item);
                *slots[index].lock().unwrap_or_else(PoisonError::into_inner) = Some(value);
            });
        }
    });
    slots
        .into_iter()
        .filter_map(|slot| slot.into_inner().unwrap_or_else(PoisonError::into_inner))
        .collect()
}

pub(crate) fn run(root: &Path, args: &[String]) -> Result<(), String> {
    let context = if args.iter().any(|a| a == "--private-branch") {
        Context::PrivateBranch
    } else {
        Context::Public
    };
    let mut findings = Vec::new();

    let listing = git_stdout(root, &["ls-files", "-z"])?;
    let mut scanned = 0usize;
    let mut deleted = 0usize;
    let items: Vec<&[u8]> = listing
        .split(|byte| *byte == 0)
        .filter(|i| !i.is_empty())
        .collect();
    // Files are independent; the results are merged in listing order, so the
    // findings are the same at any thread count.
    for outcome in map_in_order(&items, |item| scan_listed(root, item, context)) {
        scanned += outcome.scanned;
        deleted += outcome.deleted;
        findings.extend(outcome.findings);
    }

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--private-branch" => index += 1,
            "--range" => {
                let range = args.get(index + 1).ok_or("--range needs A..B")?;
                let log = git_stdout(
                    root,
                    &["log", "--format=%H%n%an <%ae>%n%cn <%ce>%n%B%x00", range],
                )?;
                let text = String::from_utf8_lossy(&log).into_owned();
                for entry in text.split('\0').filter(|e| !e.trim().is_empty()) {
                    let entry = entry.trim_start();
                    let label = format!("commit {}", entry.lines().next().unwrap_or("?"));
                    findings.extend(scan_text(&label, entry));
                    // Lines 2 and 3 are the author and committer identities.
                    let identities = entry.lines().skip(1).take(2).collect::<Vec<_>>().join(" ");
                    if let Some(address) = secondary_identity(&identities) {
                        findings.push(format!(
                            "{label}: author or committer uses a secondary identity ({address})"
                        ));
                    }
                }
                index += 2;
            }
            "--text" => {
                let file = args.get(index + 1).ok_or("--text needs a file")?;
                let bytes = fs::read(file).map_err(|e| format!("{file}: {e}"))?;
                let text = String::from_utf8_lossy(&bytes).into_owned();
                findings.extend(scan_text(file, &text));
                index += 2;
            }
            other => return Err(format!("hygiene: unknown argument {other}")),
        }
    }

    println!(
        "hygiene: {context:?} context, scanned {scanned} tracked files ({deleted} deleted in the worktree, not scanned)"
    );
    if findings.is_empty() {
        println!("hygiene: pass");
        return Ok(());
    }
    for finding in &findings {
        println!("{finding}");
    }
    Err(format!("hygiene: {} finding(s)", findings.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frag(parts: &[&str]) -> String {
        parts.concat()
    }

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(dir)
            .args([
                "-c",
                "user.name=Test Author",
                "-c",
                "user.email=test@example.org",
            ])
            .args(args)
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?}");
    }

    /// A throw-away repository with a clean first commit.
    fn temp_repo(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("alas-hygiene-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]);
        fs::write(dir.join("README.md"), "Aircraft design.\n").unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-q", "-m", "first"]);
        dir
    }

    #[test]
    fn private_documentation_paths_are_recognised() {
        for tree in ["internal", "research"] {
            let path = frag(&["docs/", tree, "/v15/RULES.md"]);
            assert!(is_internal_path(&path), "{path}");
            assert!(is_internal_path(&path.replace('/', "\\")), "{path}");
            assert!(is_internal_path(&frag(&["source/alas/", &path])));
        }
        assert!(!is_internal_path(&frag(&["docs/", "internal-notes.md"])));
        assert!(!is_internal_path("docs/release-packaging.md"));
    }

    #[test]
    fn a_package_containing_private_documents_is_refused() {
        for tree in ["internal", "research"] {
            let dir = std::env::temp_dir().join(format!(
                "alas-hygiene-private-{tree}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&dir);
            let private = dir.join("source/alas/docs").join(tree).join("v15");
            fs::create_dir_all(&private).unwrap();
            fs::write(private.join("RULES.md"), "# rules\n").unwrap();
            let result = verify_package(&dir);
            let _ = fs::remove_dir_all(&dir);
            let message = result.unwrap_err();
            assert!(
                message.contains(&frag(&["docs/", tree, "/v15/RULES.md"])),
                "{message}"
            );
        }
    }

    #[test]
    fn a_package_with_an_ai_reference_is_refused_and_a_clean_one_passes() {
        let dir = std::env::temp_dir().join(format!("alas-hygiene-ai-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("README.md"), "Aircraft design.\n").unwrap();
        assert!(verify_package(&dir).is_ok());
        let name = frag(&["Clau", "de"]);
        fs::write(dir.join("NOTES.md"), format!("written by {name}\n")).unwrap();
        // A file with a stray invalid byte is still scanned.
        let mut bytes = format!("{name} wrote this\n").into_bytes();
        bytes.push(0xff);
        fs::write(dir.join("LATIN1.txt"), bytes).unwrap();
        let result = verify_package(&dir);
        let _ = fs::remove_dir_all(&dir);
        let message = result.unwrap_err();
        assert!(message.contains("NOTES.md:1"), "{message}");
        assert!(message.contains("LATIN1.txt:1"), "{message}");
    }

    #[test]
    fn every_pattern_is_detected_as_a_whole_word_or_phrase() {
        for parts in [
            ["Anthro", "pic"],
            ["Co", "dex"],
            ["Co", "Pilot"],
            ["Chat", "GPT"],
            ["Notebook", "LM"],
            ["Gem", "ini"],
            ["Open", "AI"],
        ] {
            let word = frag(&parts);
            assert!(
                !scan_text("t", &format!("uses {word}.")).is_empty(),
                "{word}"
            );
        }
        for prefix in ["G", "g"] {
            for suffix in ["PT-4", "pt-4o", "PT4"] {
                let text = format!("model {prefix}{suffix}");
                assert!(!scan_text("t", &text).is_empty(), "{text}");
            }
        }
        let trailer = frag(&["Co-Authored", "-By: Someone"]);
        assert!(!scan_text("t", &trailer).is_empty());
        let banner = frag(&["Generated ", "with [Tool](https://example.org)"]);
        assert!(!scan_text("t", &banner).is_empty());
        let path = frag(&["see ", ".age", "nt/reports/x.md"]);
        assert!(!scan_text("t", &path).is_empty());
        let doc = frag(&["see ", "docs/", "internal/v15"]);
        assert!(!scan_text("t", &doc).is_empty());
    }

    #[test]
    fn programme_bookkeeping_is_detected() {
        for text in [
            frag(&["(", "decision", " D6)"]),
            frag(&["audit ", "SIZ", "-06"]),
            frag(&["(", "UAV", "-PLAN 3.1)"]),
            frag(&["attrib-", "integration", "-2026-10-14"]),
        ] {
            assert!(!scan_text("t.rs", &text).is_empty(), "{text}");
        }
        assert!(scan_text("t.rs", "the decision to descend; resize-0").is_empty());
    }

    #[test]
    fn ignore_files_may_name_the_private_trees_but_not_assistants() {
        let path = frag(&["/.age", "nt/"]);
        assert!(scan_text("/.gitignore", &path).is_empty());
        assert!(!scan_text("docs/x.md", &path).is_empty());
        let name = frag(&["/CLAU", "DE.md"]);
        assert!(!scan_text(".gitignore", &name).is_empty());
    }

    #[test]
    fn escaped_and_backslash_spellings_of_private_paths_are_detected() {
        let drive = frag(&["C", ":"]);
        let agent = frag(&[".age", "nt"]);
        let single = "\\";
        let double = "\\\\";
        let json =
            format!("\"path\": \"{drive}{double}Work{double}{agent}{double}reports{double}x.pdf\"");
        let raw = format!("{drive}{single}Work{single}{agent}{single}reports{single}x.pdf");
        assert!(!scan_text("a.json", &json).is_empty(), "{json}");
        assert!(!scan_text("a.txt", &raw).is_empty(), "{raw}");
    }

    #[test]
    fn personal_absolute_paths_are_detected() {
        let forward = frag(&["C", ":/Proy", "ectos/OpenFOAM/bin"]);
        let json = frag(&["C", ":\\\\Users\\\\Mar", "cos\\\\x"]);
        assert!(!scan_text("t", &forward).is_empty());
        assert!(!scan_text("t", &json).is_empty());
        assert!(scan_text("t", "C:\\Users\\Example\\ALAS\\runs").is_empty());
    }

    #[test]
    fn underscores_do_not_hide_a_word() {
        for text in [
            frag(&["CLAU", "DE_CODE_USE"]),
            frag(&["anthro", "pic_api_key"]),
            frag(&["co", "dex_runtime"]),
            frag(&[".clau", "de_settings"]),
        ] {
            assert!(!scan_text("t", &text).is_empty(), "{text}");
        }
    }

    #[test]
    fn ordinary_words_and_fixture_provenance_prose_are_not_flagged() {
        assert!(scan_text("t", "the fixture was generated with one panel").is_empty());
        assert!(scan_text("t", "co-pilot seat, codexes, Co-authored the program").is_empty());
        assert!(scan_text("t", "supercopilots, gptx, the GPT disk").is_empty());
    }

    #[test]
    fn the_selig_database_entry_is_the_only_reviewed_exemption() {
        let entry = frag(&["GEM", "INI (smoothed)"]);
        assert!(scan_text("crates/alas-geom/data/selig.txt", &entry).is_empty());
        assert!(scan_text("source/alas/crates/alas-geom/data/selig.txt", &entry).is_empty());
        assert!(!scan_text("crates/other/data.txt", &entry).is_empty());
        let other = frag(&["the GEM", "INI model"]);
        assert!(!scan_text("crates/alas-geom/data/selig.txt", &other).is_empty());
    }

    #[test]
    fn run_is_clean_on_a_clean_repository() {
        let dir = temp_repo("run-clean");
        let result = run(&dir, &[]);
        let _ = fs::remove_dir_all(&dir);
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn run_rejects_private_trees_on_a_public_ref_and_allows_them_on_a_private_branch() {
        let dir = temp_repo("run-files");
        let private = dir.join(frag(&["docs/", "internal"]));
        fs::create_dir_all(&private).unwrap();
        fs::write(private.join("n.md"), format!("{}\n", frag(&["Clau", "de"]))).unwrap();
        git(&dir, &["add", "."]);
        let private_args = vec!["--private-branch".to_owned()];
        assert!(
            run(&dir, &private_args).is_ok(),
            "private trees are not scanned"
        );
        let public = run(&dir, &[]).unwrap_err();
        assert_eq!(public, "hygiene: 1 finding(s)", "tracked on a public ref");
        fs::write(dir.join("HANDOFF-NEXT.md"), "notes\n").unwrap();
        git(&dir, &["add", "."]);
        assert_eq!(run(&dir, &[]).unwrap_err(), "hygiene: 2 finding(s)");
        fs::write(
            dir.join("bad.md"),
            format!("{} wrote\n", frag(&["Clau", "de"])),
        )
        .unwrap();
        git(&dir, &["add", "."]);
        let result = run(&dir, &private_args);
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(result.unwrap_err(), "hygiene: 1 finding(s)");
    }

    #[test]
    fn utf16_text_is_scanned_and_secondary_identities_are_found() {
        let name = frag(&["Clau", "de"]);
        let mut bytes = vec![0xff, 0xfe];
        for unit in format!("{name} wrote\n").encode_utf16() {
            bytes.extend(unit.to_le_bytes());
        }
        assert!(!scan_bytes("x.txt", &bytes).is_empty());
        assert!(secondary_identity(&frag(&["A <shat", "men4@example.org>"])).is_some());
        assert!(secondary_identity("A <a@example.org>").is_none());
    }

    #[test]
    fn workflow_vocabulary_and_profile_paths_are_detected() {
        for text in [
            frag(&["see the ", "sub", "agent brief"]),
            frag(&["C", ":\\Users\\Mar", "cos\\AppData\\Roaming\\x"]),
            frag(&["D", ":\\ALAS", "-targets\\x"]),
            frag(&["open ", "HAND", "OFF.md"]),
        ] {
            assert!(!scan_text("t.txt", &text).is_empty(), "{text}");
        }
    }

    #[test]
    fn run_range_scans_messages_and_identities_of_the_published_commits() {
        let dir = temp_repo("run-range");
        fs::write(dir.join("a.txt"), "a\n").unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-q", "-m", "plain change"]);
        let args = vec!["--range".to_owned(), "HEAD~1..HEAD".to_owned()];
        assert!(run(&dir, &args).is_ok());

        let trailer = frag(&["Co-Authored", "-By: Someone <s@example.org>"]);
        fs::write(dir.join("b.txt"), "b\n").unwrap();
        git(&dir, &["add", "."]);
        git(
            &dir,
            &["commit", "-q", "-m", &format!("change\n\n{trailer}")],
        );
        assert!(run(&dir, &args).is_err(), "trailer in the message");

        git(&dir, &["reset", "-q", "--hard", "HEAD~1"]);
        fs::write(dir.join("c.txt"), "c\n").unwrap();
        git(&dir, &["add", "."]);
        let identity = frag(&["Clau", "de <noreply@example.org>"]);
        git(
            &dir,
            &["commit", "-q", "--author", &identity, "-m", "clean message"],
        );
        assert!(run(&dir, &args).is_err(), "assistant identity as author");
        let result = run(&dir, &["--range".to_owned(), "HEAD..HEAD".to_owned()]);
        let _ = fs::remove_dir_all(&dir);
        assert!(result.is_ok(), "an empty range has nothing to scan");
    }

    #[test]
    fn run_text_scans_extra_files_such_as_a_pull_request_body() {
        let dir = temp_repo("run-text");
        let body = dir.join("pr-body.txt");
        fs::write(&body, "Plain description.\n").unwrap();
        let args = vec!["--text".to_owned(), body.to_string_lossy().into_owned()];
        assert!(run(&dir, &args).is_ok());
        let banner = frag(&["Generated ", "with [Tool](https://example.org)"]);
        fs::write(&body, format!("Description\n\n{banner}\n")).unwrap();
        let result = run(&dir, &args);
        let _ = fs::remove_dir_all(&dir);
        assert!(result.is_err());
    }

    #[test]
    fn unknown_arguments_are_rejected() {
        let dir = temp_repo("run-args");
        let result = run(&dir, &["--bogus".to_owned()]);
        let _ = fs::remove_dir_all(&dir);
        assert!(result.unwrap_err().contains("unknown argument"));
    }
}
