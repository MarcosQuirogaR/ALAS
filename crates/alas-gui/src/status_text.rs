// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! File-operation status messages as the status bars show them.

use crate::views::tr_fields;

/// The text a status bar shows for `message`, and the full original text for
/// a tooltip. A file-operation failure of the form `cannot <verb> <path>:
/// <system text>` is shown translated and with the file name only; the path
/// and the system's own wording stay in the tooltip. Any other message is
/// shown as it is.
pub(crate) fn present_status(message: &str) -> (String, Option<String>) {
    let parsed = message.strip_prefix("cannot ").and_then(|rest| {
        let (verb, rest) = rest.split_once(' ')?;
        let (path, _) = rest.split_once(": ")?;
        let template = match verb {
            "read" => "Cannot read {name}.",
            "write" => "Cannot write {name}.",
            "create" => "Cannot create {name}.",
            "remove" | "delete" => "Cannot delete {name}.",
            _ => return None,
        };
        let name = path
            .rsplit(['/', '\\'])
            .find(|part| !part.is_empty())
            .unwrap_or(path);
        Some(tr_fields(template, &[("name", name.to_owned())]))
    });
    match parsed {
        Some(shown) => (shown, Some(message.to_owned())),
        None => (message.to_owned(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::present_status;

    #[test]
    fn a_failed_write_shows_the_file_name_and_keeps_the_path_for_the_tooltip() {
        let message = "cannot write C:\\data\\store\\wing.json: access denied (os error 5)";
        let (shown, detail) = present_status(message);
        assert_eq!(shown, "Cannot write wing.json.");
        assert_eq!(detail.as_deref(), Some(message));
    }

    #[test]
    fn other_messages_pass_through() {
        assert_eq!(present_status("Saved."), ("Saved.".to_owned(), None));
    }
}
