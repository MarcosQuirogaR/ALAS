// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The pretty `Debug` text of a derived type, read back as JSON.
//!
//! Several result types of the partial run have no `Serialize` and live in
//! crates this one does not own. Their derived `Debug` output is a regular
//! grammar (structs, tuples, lists, maps, strings, numbers), so it is parsed
//! into a [`Value`] instead of being shipped as one opaque string.
//!
//! Mapping: a struct is an object of its fields plus `"_type"` (the name; for
//! an enum struct variant the variant name); `Some(x)` is `x` and `None` is
//! `null`; a unit variant is its name as a string; any other tuple is
//! `{"Name": [..]}` and a bare tuple a list; a map is an object (non-string
//! keys are written as their text); `inf`, `-inf` and `NaN` are `null` (JSON
//! has no non-finite number). Text this reader does not understand, such as
//! a hand-written `Debug`, is kept as the original string.

use serde_json::{Map, Number, Value};

/// `value`'s `Debug` text as JSON, or the text itself when it does not parse.
pub(super) fn debug_json<T: std::fmt::Debug>(value: &T) -> Value {
    let text = format!("{value:#?}");
    parse(&text).unwrap_or(Value::String(text))
}

fn parse(text: &str) -> Option<Value> {
    let mut reader = Reader {
        bytes: text.as_bytes(),
        at: 0,
    };
    let value = reader.value(0)?;
    reader.skip_space();
    (reader.at == reader.bytes.len()).then_some(value)
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

const MAX_DEPTH: usize = 200;

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(|b| b.is_ascii_whitespace()) {
            self.at += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> bool {
        self.skip_space();
        if self.peek() == Some(byte) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn value(&mut self, depth: usize) -> Option<Value> {
        if depth > MAX_DEPTH {
            return None;
        }
        self.skip_space();
        match self.peek()? {
            b'"' => self.string().map(Value::String),
            b'\'' => self.char_literal().map(Value::String),
            b'[' => {
                self.at += 1;
                self.sequence(b']', depth).map(Value::Array)
            }
            b'(' => {
                self.at += 1;
                self.sequence(b')', depth).map(Value::Array)
            }
            b'{' => {
                self.at += 1;
                self.braced(None, depth)
            }
            b'-' | b'0'..=b'9' => self.number(),
            b if b.is_ascii_alphabetic() || b == b'_' => self.named(depth),
            _ => None,
        }
    }

    fn sequence(&mut self, close: u8, depth: usize) -> Option<Vec<Value>> {
        let mut items = Vec::new();
        loop {
            if self.eat(close) {
                return Some(items);
            }
            items.push(self.value(depth + 1)?);
            if !self.eat(b',') {
                return self.eat(close).then_some(items);
            }
        }
    }

    /// The inside of `{ ... }` after the opening brace: fields, map entries or
    /// set members.
    fn braced(&mut self, name: Option<&str>, depth: usize) -> Option<Value> {
        let mut object = Map::new();
        if let Some(name) = name {
            object.insert("_type".to_owned(), Value::String(name.to_owned()));
        }
        let mut members = Vec::new();
        let mut is_set = false;
        loop {
            if self.eat(b'}') {
                break;
            }
            let first = self.value(depth + 1)?;
            if self.eat(b':') {
                let value = self.value(depth + 1)?;
                let key = match first {
                    Value::String(text) => text,
                    other => other.to_string(),
                };
                object.insert(key, value);
            } else {
                is_set = true;
                members.push(first);
            }
            if !self.eat(b',') {
                if self.eat(b'}') {
                    break;
                }
                return None;
            }
        }
        if is_set && object.is_empty() {
            Some(Value::Array(members))
        } else if is_set {
            None
        } else {
            Some(Value::Object(object))
        }
    }

    fn named(&mut self, depth: usize) -> Option<Value> {
        let start = self.at;
        loop {
            match self.peek() {
                Some(b) if b.is_ascii_alphanumeric() || b == b'_' => self.at += 1,
                // A path separator `::` belongs to the name; a lone `:` ends it.
                Some(b':') if self.bytes.get(self.at + 1) == Some(&b':') => self.at += 2,
                _ => break,
            }
        }
        let name = std::str::from_utf8(&self.bytes[start..self.at]).ok()?;
        match name {
            "true" => return Some(Value::Bool(true)),
            "false" => return Some(Value::Bool(false)),
            "None" | "inf" | "NaN" => return Some(Value::Null),
            _ => {}
        }
        self.skip_space();
        match self.peek() {
            Some(b'{') => {
                self.at += 1;
                self.braced(Some(name), depth)
            }
            Some(b'(') => {
                self.at += 1;
                let mut items = self.sequence(b')', depth)?;
                if name == "Some" && items.len() == 1 {
                    return items.pop();
                }
                let mut object = Map::new();
                object.insert(name.to_owned(), Value::Array(items));
                Some(Value::Object(object))
            }
            _ => Some(Value::String(name.to_owned())),
        }
    }

    fn number(&mut self) -> Option<Value> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
            if self.bytes[self.at..].starts_with(b"inf") {
                self.at += 3;
                return Some(Value::Null);
            }
        }
        while self
            .peek()
            .is_some_and(|b| b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'E' | b'+' | b'-'))
        {
            self.at += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at]).ok()?;
        if let Ok(integer) = text.parse::<i64>() {
            return Some(Value::Number(integer.into()));
        }
        if let Ok(integer) = text.parse::<u64>() {
            return Some(Value::Number(integer.into()));
        }
        let float: f64 = text.parse().ok()?;
        Some(Number::from_f64(float).map_or(Value::Null, Value::Number))
    }

    fn string(&mut self) -> Option<String> {
        self.at += 1;
        let mut out = Vec::new();
        loop {
            let byte = self.peek()?;
            self.at += 1;
            match byte {
                b'"' => return String::from_utf8(out).ok(),
                b'\\' => {
                    let escaped = self.peek()?;
                    self.at += 1;
                    match escaped {
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'0' => out.push(0),
                        b'\\' | b'"' | b'\'' => out.push(escaped),
                        b'u' => {
                            if self.peek()? != b'{' {
                                return None;
                            }
                            let end = self.bytes[self.at..].iter().position(|&b| b == b'}')?;
                            let hex = std::str::from_utf8(&self.bytes[self.at + 1..self.at + end])
                                .ok()?;
                            let ch = char::from_u32(u32::from_str_radix(hex, 16).ok()?)?;
                            self.at += end + 1;
                            let mut buffer = [0_u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
                        }
                        _ => return None,
                    }
                }
                other => out.push(other),
            }
        }
    }

    fn char_literal(&mut self) -> Option<String> {
        // 'x' or an escape such as '\n'; read up to the closing quote.
        self.at += 1;
        let start = self.at;
        while self.peek()? != b'\'' || self.bytes.get(self.at.wrapping_sub(1)) == Some(&b'\\') {
            self.at += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at])
            .ok()?
            .to_owned();
        self.at += 1;
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // The fields exist to be rendered through `Debug`; nothing reads them.
    #[derive(Debug)]
    #[allow(dead_code)]
    struct Inner {
        a: f64,
        b: Option<f64>,
    }

    #[derive(Debug)]
    #[allow(dead_code)]
    enum Kind {
        Unit,
        Pair(i32, String),
        Named { x: bool },
    }

    // The fields exist to be rendered through `Debug`; nothing reads them.
    #[derive(Debug)]
    #[allow(dead_code)]
    struct Outer {
        name: String,
        inner: Inner,
        list: Vec<Inner>,
        kinds: Vec<Kind>,
        speed: f64,
        low: f64,
        none: Option<i32>,
        empty: Vec<i32>,
        map: std::collections::BTreeMap<String, u32>,
        tuple: (i32, f64),
    }

    #[test]
    fn derived_debug_text_becomes_json_with_non_finite_numbers_as_null() {
        let outer = Outer {
            name: "quote \" and \\ and\nnewline".to_owned(),
            inner: Inner {
                a: 1.5e-7,
                b: Some(2.0),
            },
            list: vec![
                Inner {
                    a: f64::INFINITY,
                    b: None,
                },
                Inner {
                    a: f64::NAN,
                    b: Some(f64::NEG_INFINITY),
                },
            ],
            kinds: vec![
                Kind::Unit,
                Kind::Pair(-3, "p".to_owned()),
                Kind::Named { x: true },
            ],
            speed: 238.5,
            low: -0.25,
            none: None,
            empty: vec![],
            map: [("k".to_owned(), 7)].into_iter().collect(),
            tuple: (1, 2.5),
        };
        let value = debug_json(&outer);
        assert_eq!(value["_type"], "Outer");
        assert_eq!(value["name"], "quote \" and \\ and\nnewline");
        assert_eq!(value["inner"]["a"], json!(1.5e-7));
        assert_eq!(value["inner"]["b"], json!(2.0));
        assert_eq!(value["list"][0]["a"], Value::Null);
        assert_eq!(value["list"][0]["b"], Value::Null);
        assert_eq!(value["list"][1]["a"], Value::Null);
        assert_eq!(value["list"][1]["b"], Value::Null);
        assert_eq!(value["kinds"][0], "Unit");
        assert_eq!(value["kinds"][1], json!({"Pair": [-3, "p"]}));
        assert_eq!(value["kinds"][2]["x"], true);
        assert_eq!(value["speed"], json!(238.5));
        assert_eq!(value["low"], json!(-0.25));
        assert_eq!(value["none"], Value::Null);
        assert_eq!(value["empty"], json!([]));
        assert_eq!(value["map"]["k"], 7);
        assert_eq!(value["tuple"], json!([1, 2.5]));
    }

    #[test]
    fn text_that_is_not_derived_debug_is_kept_as_a_string() {
        struct Custom;
        impl std::fmt::Debug for Custom {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("1.5s elapsed @ here")
            }
        }
        assert_eq!(debug_json(&Custom), json!("1.5s elapsed @ here"));
    }
}
