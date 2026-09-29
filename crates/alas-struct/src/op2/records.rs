// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Splitting the byte stream into records and pairing IDENT with DATA.

use super::*;

/// Split the byte stream into Fortran record bodies.
///
/// Each record is `[len][body][len]` with 32-bit little-endian length words.
/// Reading stops cleanly at the first length word that cannot begin a record
/// (non-positive, or overrunning the file): that is how the trailing
/// end-of-file marker and any pad are reached. A record whose two length words
/// disagree is corruption, and is reported.
pub(super) fn split_records(bytes: &[u8]) -> Result<Vec<&[u8]>, Op2Error> {
    let mut records = Vec::new();
    let mut p = 0usize;
    while p + 8 <= bytes.len() {
        let len = read_i32(bytes, p)?;
        if len <= 0 {
            break;
        }
        let len = len as usize;
        let end = match p.checked_add(8).and_then(|v| v.checked_add(len)) {
            Some(end) if end <= bytes.len() => end,
            _ => break,
        };
        let trailing = read_i32(bytes, p + 4 + len)?;
        if trailing != len as i32 {
            return Err(Op2Error::RecordLengthMismatch { offset: p });
        }
        records.push(&bytes[p + 4..p + 4 + len]);
        p = end;
    }
    Ok(records)
}

/// The datablock name records this reader routes on: eigenvectors arrive in
/// `OPHIG`, everything else in a name starting `OUG` (displacement family) or
/// `OES` (element stress).
pub(super) fn record_is_name(record: &[u8]) -> Option<String> {
    if record.len() != 8 {
        return None;
    }
    if !record
        .iter()
        .all(|&c| c == b' ' || c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return None;
    }
    if !record[0].is_ascii_uppercase() {
        return None;
    }
    Some(String::from_utf8_lossy(record).trim_end().to_string())
}

/// Walk the records, joining every DATA chunk belonging to one 146-word IDENT
/// and routing the completed table by its datablock name and analysis code.
pub(super) fn parse(records: &[&[u8]]) -> Result<Op2, Op2Error> {
    let mut op2 = Op2::default();
    let mut i = skip_header(records);
    let mut name = String::new();
    let mut pending: Option<PendingTable<'_>> = None;
    let mut declared_words: Option<usize> = None;
    while i < records.len() {
        let record = records[i];
        i += 1;
        if let Some(found) = record_is_name(record) {
            flush_pending(&mut op2, &mut pending)?;
            name = found;
            declared_words = None;
            continue;
        }
        if let Some(word) = as_i32(record) {
            if word < 0 && pending.as_ref().is_some_and(|table| !table.data.is_empty()) {
                flush_pending(&mut op2, &mut pending)?;
            }
            declared_words = (word > 1).then_some(word as usize);
            continue;
        }
        let words = record.len() / 4;
        if declared_words == Some(words) {
            declared_words = None;
            if let Some(table) = pending.as_mut() {
                table.data.extend_from_slice(record);
            } else if words == IDENT_WORDS {
                pending = Some(PendingTable {
                    name: name.clone(),
                    ident: record,
                    data: Vec::new(),
                });
            }
        }
    }
    flush_pending(&mut op2, &mut pending)?;
    Ok(op2)
}

/// One table-3 and all table-4 record bodies that follow it.
struct PendingTable<'a> {
    name: String,
    ident: &'a [u8],
    data: Vec<u8>,
}

fn flush_pending(op2: &mut Op2, pending: &mut Option<PendingTable<'_>>) -> Result<(), Op2Error> {
    let Some(table) = pending.take() else {
        return Ok(());
    };
    if table.data.is_empty() {
        return Ok(());
    }
    store(op2, &table.name, table.ident, &table.data)
}

/// The number of 4-byte words in a table-3 IDENT record.
pub(super) const IDENT_WORDS: usize = 146;

/// Skip the file header: the records up to and including the first `(-1, 0)`
/// marker pair, after which the first datablock begins.
fn skip_header(records: &[&[u8]]) -> usize {
    for i in 0..records.len().saturating_sub(1) {
        if as_i32(records[i]) == Some(-1) && as_i32(records[i + 1]) == Some(0) {
            return i + 2;
        }
    }
    0
}
