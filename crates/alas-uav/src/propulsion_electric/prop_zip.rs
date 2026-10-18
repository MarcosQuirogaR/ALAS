// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Minimal zip reader for importing a user-provided archive of tables.
//!
//! Supports stored and deflated entries of plain (non-zip64, unencrypted)
//! archives, which is what the propeller data downloads use.  Anything else is
//! reported per entry.  Entries are size-limited so an archive bomb cannot
//! exhaust memory.

/// Largest accepted uncompressed entry; a PER3 table is about 0.1 to 0.2 MB.
const MAX_ENTRY_BYTES: usize = 8 * 1024 * 1024;
const MAX_ENTRIES: usize = 20_000;

/// One archive entry: its stored name and its decoded bytes (or why not).
pub(super) struct ZipEntry {
    pub(super) name: String,
    pub(super) data: Result<Vec<u8>, String>,
}

fn u16_at(bytes: &[u8], at: usize) -> Option<usize> {
    let b = bytes.get(at..at + 2)?;
    Some(usize::from(u16::from_le_bytes([b[0], b[1]])))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<usize> {
    let b = bytes.get(at..at + 4)?;
    usize::try_from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok()
}

pub(super) fn read_entries(bytes: &[u8]) -> Result<Vec<ZipEntry>, String> {
    let bad = || "not a readable zip archive".to_owned();
    let tail_start = bytes.len().saturating_sub(22 + 65_535);
    let eocd = (tail_start..bytes.len().saturating_sub(21))
        .rev()
        .find(|&i| bytes[i..].starts_with(b"PK\x05\x06"))
        .ok_or_else(bad)?;
    let count = u16_at(bytes, eocd + 10).ok_or_else(bad)?;
    let mut at = u32_at(bytes, eocd + 16).ok_or_else(bad)?;
    if count > MAX_ENTRIES || count == 0xFFFF {
        return Err("zip archive has too many entries (or uses zip64)".to_owned());
    }
    let mut entries = Vec::new();
    for _ in 0..count {
        if !bytes.get(at..).ok_or_else(bad)?.starts_with(b"PK\x01\x02") {
            return Err(bad());
        }
        let flags = u16_at(bytes, at + 8).ok_or_else(bad)?;
        let method = u16_at(bytes, at + 10).ok_or_else(bad)?;
        let compressed = u32_at(bytes, at + 20).ok_or_else(bad)?;
        let name_len = u16_at(bytes, at + 28).ok_or_else(bad)?;
        let extra_len = u16_at(bytes, at + 30).ok_or_else(bad)?;
        let comment_len = u16_at(bytes, at + 32).ok_or_else(bad)?;
        let local = u32_at(bytes, at + 42).ok_or_else(bad)?;
        let name_bytes = bytes.get(at + 46..at + 46 + name_len).ok_or_else(bad)?;
        let name = String::from_utf8_lossy(name_bytes).into_owned();
        at += 46 + name_len + extra_len + comment_len;
        let data = if name.ends_with('/') {
            continue;
        } else if flags & 1 != 0 {
            Err("encrypted entry".to_owned())
        } else {
            entry_data(bytes, local, method, compressed)
        };
        entries.push(ZipEntry { name, data });
    }
    Ok(entries)
}

fn entry_data(
    bytes: &[u8],
    local: usize,
    method: usize,
    compressed: usize,
) -> Result<Vec<u8>, String> {
    let bad = || "corrupt zip entry".to_owned();
    if !bytes
        .get(local..)
        .ok_or_else(bad)?
        .starts_with(b"PK\x03\x04")
    {
        return Err(bad());
    }
    let name_len = u16_at(bytes, local + 26).ok_or_else(bad)?;
    let extra_len = u16_at(bytes, local + 28).ok_or_else(bad)?;
    let start = local + 30 + name_len + extra_len;
    let raw = bytes.get(start..start + compressed).ok_or_else(bad)?;
    match method {
        0 if raw.len() <= MAX_ENTRY_BYTES => Ok(raw.to_vec()),
        0 => Err("entry is larger than the 8 MB limit".to_owned()),
        8 => miniz_oxide::inflate::decompress_to_vec_with_limit(raw, MAX_ENTRY_BYTES)
            .map_err(|_| "entry is corrupt or larger than the 8 MB limit".to_owned()),
        other => Err(format!("unsupported zip compression method {other}")),
    }
}
