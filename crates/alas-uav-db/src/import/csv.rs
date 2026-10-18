// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! A small RFC 4180 reader (quoted fields, doubled quotes, CRLF, BOM).
//!
//! Vendor and team spreadsheets are exported with commas or semicolons, with
//! or without quotes. A dedicated parser keeps the dependency graph unchanged
//! and reports the line of an unterminated quote, which is the one structural
//! error a spreadsheet export can contain.

use crate::error::{DbError, DbResult};

/// Parse `text` into rows of fields, skipping fully blank rows.
pub fn parse(text: &str, delimiter: char) -> DbResult<Vec<Vec<String>>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut line = 1usize;
    let mut quote_line = 1usize;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_quotes {
            match ch {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => in_quotes = false,
                '\n' => {
                    line += 1;
                    field.push('\n');
                }
                other => field.push(other),
            }
            continue;
        }
        match ch {
            '"' if field.is_empty() => {
                in_quotes = true;
                quote_line = line;
            }
            c if c == delimiter => row.push(std::mem::take(&mut field)),
            '\r' => {}
            '\n' => {
                line += 1;
                finish_row(&mut rows, &mut row, &mut field);
            }
            other => field.push(other),
        }
    }
    if in_quotes {
        return Err(DbError::new(format!(
            "unterminated quoted field starting on line {quote_line}"
        )));
    }
    finish_row(&mut rows, &mut row, &mut field);
    Ok(rows)
}

fn finish_row(rows: &mut Vec<Vec<String>>, row: &mut Vec<String>, field: &mut String) {
    row.push(std::mem::take(field));
    let blank = row.iter().all(|f| f.trim().is_empty());
    let finished = std::mem::take(row);
    if !blank {
        rows.push(finished);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_quoted_and_escaped_fields() -> DbResult<()> {
        let rows = parse("a,b,c\r\n1,\"x,y\",\"say \"\"hi\"\"\"\r\n\r\n3,,\n", ',')?;
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1], vec!["1", "x,y", "say \"hi\""]);
        assert_eq!(rows[2], vec!["3", "", ""]);
        Ok(())
    }

    #[test]
    fn bom_and_semicolon_delimiter() -> DbResult<()> {
        let rows = parse("\u{feff}a;b\n1;2", ';')?;
        assert_eq!(rows[0], vec!["a", "b"]);
        assert_eq!(rows[1], vec!["1", "2"]);
        Ok(())
    }

    #[test]
    fn unterminated_quote_reports_its_line() {
        let error = parse("a,b\n1,\"oops\n2,3", ',')
            .err()
            .map(|e| e.to_string());
        assert_eq!(
            error.as_deref(),
            Some("unterminated quoted field starting on line 2")
        );
    }
}
