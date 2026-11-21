// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The add-on's own folder: a file tree, a read-only text loader for the code
//! viewer, and opening a file in the system's editor or default program.

use std::path::{Path, PathBuf};

/// Deepest folder level listed.
const MAX_DEPTH: usize = 5;
/// Most entries listed for one add-on.
const MAX_ENTRIES: usize = 400;
/// Largest file loaded into the viewer, bytes.
const MAX_VIEW_BYTES: u64 = 128 * 1024;
/// Most lines shown in the viewer.
const MAX_VIEW_LINES: usize = 1500;

/// One folder or file of the add-on.
#[derive(Debug, Clone)]
pub(crate) struct TreeNode {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) children: Option<Vec<TreeNode>>,
}

/// A file loaded into the read-only viewer.
#[derive(Debug, Clone)]
pub(crate) struct ViewedFile {
    pub(crate) path: PathBuf,
    pub(crate) text: String,
    /// Why the content is not (fully) shown, when it is not.
    pub(crate) note: Option<String>,
}

/// The file tree and viewer of the selected add-on.
#[derive(Debug, Default)]
pub(crate) struct FileBrowser {
    /// Add-on the tree was scanned for.
    pub(crate) addon_id: String,
    pub(crate) tree: Vec<TreeNode>,
    pub(crate) viewed: Option<ViewedFile>,
}

impl FileBrowser {
    /// Scan `dir` again for `addon_id` and close the viewer.
    pub(crate) fn rescan(&mut self, addon_id: &str, dir: &Path) {
        self.addon_id = addon_id.to_owned();
        let mut budget = MAX_ENTRIES;
        self.tree = scan_dir(dir, 0, &mut budget);
        self.viewed = None;
    }
}

fn hidden(name: &str) -> bool {
    name.starts_with('.') || name == "__pycache__" || name == "target"
}

/// Folders first, then files, each alphabetical; bounded in depth and count.
pub(crate) fn scan_dir(dir: &Path, depth: usize, budget: &mut usize) -> Vec<TreeNode> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries: Vec<_> = read
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let kind = entry.file_type().ok()?;
            // Symbolic links are not followed: the tree stays inside the folder.
            (!hidden(&name) && !kind.is_symlink()).then(|| (name, entry.path(), kind.is_dir()))
        })
        .collect();
    entries.sort_by(|a, b| {
        b.2.cmp(&a.2)
            .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
    });
    let mut nodes = Vec::new();
    for (name, path, is_dir) in entries {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let children = is_dir.then(|| {
            if depth + 1 < MAX_DEPTH {
                scan_dir(&path, depth + 1, budget)
            } else {
                Vec::new()
            }
        });
        nodes.push(TreeNode {
            name,
            path,
            children,
        });
    }
    nodes
}

/// Load `path` for the read-only viewer: text only, bounded in size and lines.
pub(crate) fn load_viewed(path: &Path) -> ViewedFile {
    let shown = |text: String, note: Option<String>| ViewedFile {
        path: path.to_path_buf(),
        text,
        note,
    };
    let size = match std::fs::metadata(path) {
        Ok(meta) => meta.len(),
        Err(error) => return shown(String::new(), Some(error.to_string())),
    };
    let take = size.min(MAX_VIEW_BYTES);
    let mut bytes = Vec::new();
    let read = std::fs::File::open(path).and_then(|file| {
        use std::io::Read;
        file.take(take).read_to_end(&mut bytes)
    });
    if let Err(error) = read {
        return shown(String::new(), Some(error.to_string()));
    }
    if bytes.contains(&0) {
        return shown(
            String::new(),
            Some("Binary file: not shown. Use Open in editor.".to_owned()),
        );
    }
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    let mut note = (size > MAX_VIEW_BYTES).then(|| {
        format!(
            "Showing the first {} KiB of {} KiB.",
            MAX_VIEW_BYTES / 1024,
            size / 1024
        )
    });
    if text.lines().count() > MAX_VIEW_LINES {
        text = text
            .lines()
            .take(MAX_VIEW_LINES)
            .collect::<Vec<_>>()
            .join("\n");
        note = Some(format!("Showing the first {MAX_VIEW_LINES} lines."));
    }
    shown(text, note)
}

/// Extensions that run code when opened: never opened from a result link.
const EXECUTABLE_EXTENSIONS: &[&str] = &[
    "exe", "bat", "cmd", "com", "msi", "ps1", "vbs", "vbe", "js", "jse", "wsf", "scr", "lnk",
    "reg", "sh", "bash", "app", "command", "jar", "dll", "pif", "cpl", "hta",
];

/// Whether a result file named by an add-on may be opened from its link: it
/// must exist, be a file, and not be a program or script.
pub(crate) fn is_openable_result(path: &Path) -> bool {
    path.is_file()
        && !path.extension().is_some_and(|ext| {
            let ext = ext.to_string_lossy().to_ascii_lowercase();
            EXECUTABLE_EXTENSIONS.contains(&ext.as_str())
        })
}

/// Open `path` for editing: the registered editor on Windows (the `Edit` verb,
/// falling back to Notepad), the default program elsewhere. Never runs it.
pub(crate) fn open_in_editor(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use alas_exec::process::NoConsoleWindow;
        let script = "$p=$env:ALAS_OPEN_PATH; try { Start-Process -Verb Edit -FilePath $p -ErrorAction Stop } catch { Start-Process -FilePath notepad.exe -ArgumentList ('\"'+$p+'\"') }";
        std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("ALAS_OPEN_PATH", path)
            .no_window()
            .spawn()
            .map(alas_exec::process::reap_in_background)
            .map_err(|error| error.to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        open_with_default(path)
    }
}

/// Open a result file with the system's default program.
pub(crate) fn open_result(path: &Path) -> Result<(), String> {
    if !is_openable_result(path) {
        return Err(format!("{} cannot be opened from here.", path.display()));
    }
    open_with_default(path)
}

fn open_with_default(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut command = std::process::Command::new("explorer.exe");
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");
    use alas_exec::process::NoConsoleWindow;
    command
        .no_window()
        .arg(path)
        .spawn()
        .map(alas_exec::process::reap_in_background)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(label: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("alas-addon-files-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_tree_lists_folders_first_and_skips_hidden_and_cache_entries() {
        let dir = temp("tree");
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        std::fs::create_dir_all(dir.join("__pycache__")).unwrap();
        std::fs::write(dir.join("main.py"), "print(1)").unwrap();
        std::fs::write(dir.join("addon.toml"), "x").unwrap();
        std::fs::write(dir.join(".hidden"), "x").unwrap();
        std::fs::write(dir.join("lib").join("util.py"), "x").unwrap();
        let mut browser = FileBrowser::default();
        browser.rescan("demo", &dir);
        let names: Vec<_> = browser.tree.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["lib", "addon.toml", "main.py"]);
        assert_eq!(browser.tree[0].children.as_ref().map(Vec::len), Some(1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_viewer_refuses_binary_files_and_caps_long_text() {
        let dir = temp("view");
        std::fs::write(dir.join("a.bin"), [1_u8, 0, 2]).unwrap();
        assert!(load_viewed(&dir.join("a.bin")).note.is_some());
        let long: String = (0..2000).map(|i| format!("line {i}\n")).collect();
        std::fs::write(dir.join("long.txt"), long).unwrap();
        let viewed = load_viewed(&dir.join("long.txt"));
        assert_eq!(viewed.text.lines().count(), MAX_VIEW_LINES);
        assert!(viewed.note.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_result_link_never_opens_a_program_or_script() {
        let dir = temp("open");
        for (name, ok) in [
            ("report.html", true),
            ("run.bat", false),
            ("tool.EXE", false),
            ("a.py", true),
        ] {
            std::fs::write(dir.join(name), "x").unwrap();
            assert_eq!(is_openable_result(&dir.join(name)), ok, "{name}");
        }
        assert!(!is_openable_result(&dir.join("missing.txt")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
