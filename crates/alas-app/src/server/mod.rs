// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The headless server: ALAS as Model Context Protocol tools over stdio.
//!
//! Transport: JSON-RPC 2.0, one compact JSON message per line on stdin and
//! stdout, UTF-8 (the MCP stdio transport). Requests are answered in order.
//! Methods: `initialize`, `ping`, `tools/list` and `tools/call`; a plain
//! JSON-RPC client may call the tool methods without `initialize`.
//! Notifications get no reply. Batches (JSON arrays) are refused: MCP
//! 2025-06-18 removed them. stdout carries nothing but protocol messages;
//! the pipeline runs quiet and on its own thread, so a run never blocks a
//! request.
//!
//! The tools are listed in [`tools::catalogue`]. Every path they take is
//! relative to the [`Workspace`] folder and cannot leave it, and a
//! configuration cannot name a program or folder of its own
//! ([`workspace::check_locations`]); no tool reaches the network.

use std::io::{self, BufRead, Write};

use serde_json::{json, Value};

mod runs;
#[cfg(test)]
mod tests;
mod tools;
mod workspace;

pub use workspace::Workspace;

/// MCP protocol revisions this server speaks, newest first. The tool subset
/// used here is the same in all three.
pub const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

/// JSON-RPC 2.0 error codes.
mod code {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
}

/// A JSON-RPC error object.
#[derive(Debug)]
struct RpcError {
    code: i64,
    message: String,
}

impl RpcError {
    fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// One server session: the workspace and the runs it started.
pub struct Server {
    workspace: Workspace,
    runs: runs::RunManager,
}

impl Server {
    /// A session on `workspace`. The user's persisted custom airports are
    /// registered at once, as the desktop application does at start-up.
    pub fn new(workspace: Workspace) -> Self {
        // A damaged store is reported by the tools that need it; the session
        // starts with the curated table.
        let _ = crate::custom_airports::install(&workspace.airport_store(), None);
        Self {
            workspace,
            runs: runs::RunManager::default(),
        }
    }

    /// Handle one line of input; the reply line, if the message wants one.
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let message: Value = match serde_json::from_str(line) {
            Ok(message) => message,
            Err(error) => {
                return Some(error_reply(
                    Value::Null,
                    RpcError::new(code::PARSE_ERROR, format!("not JSON: {error}")),
                ))
            }
        };
        let reply = self.handle_message(message)?;
        // Serializing a serde_json::Value cannot fail.
        Some(serde_json::to_string(&reply).unwrap_or_default())
    }

    fn handle_message(&mut self, message: Value) -> Option<Value> {
        let invalid =
            |id: Value, why: &str| Some(error_value(id, RpcError::new(code::INVALID_REQUEST, why)));
        let Value::Object(object) = message else {
            return invalid(
                Value::Null,
                "a message is one JSON object; batches are not supported",
            );
        };
        // A message without an id is a notification (`notifications/initialized`,
        // `notifications/cancelled`, ...) and is never answered, well formed or
        // not. None needs action: every request is answered before the next
        // is read, and a run is stopped with `cancel_run`.
        let id = object.get("id").cloned()?;
        if !(id.is_string() || id.is_number()) {
            return invalid(Value::Null, "id must be a string or a number");
        }
        if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return invalid(id, "jsonrpc must be \"2.0\"");
        }
        let method = match object.get("method") {
            Some(Value::String(method)) => method.as_str(),
            // A response from the client: this server sends no requests.
            None if object.contains_key("result") || object.contains_key("error") => return None,
            _ => return invalid(id, "method must be a string"),
        };
        let params = object.get("params").cloned().unwrap_or(Value::Null);
        Some(match self.request(method, &params) {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err(error) => error_value(id, error),
        })
    }

    fn request(&mut self, method: &str, params: &Value) -> Result<Value, RpcError> {
        match method {
            "initialize" => Ok(initialize(params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools::catalogue() })),
            "tools/call" => {
                let name = params
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| RpcError::new(code::INVALID_PARAMS, "params.name is missing"))?;
                let arguments = match params.get("arguments") {
                    None | Some(Value::Null) => json!({}),
                    Some(value @ Value::Object(_)) => value.clone(),
                    Some(_) => {
                        return Err(RpcError::new(
                            code::INVALID_PARAMS,
                            "params.arguments must be an object",
                        ))
                    }
                };
                match tools::call(self, name, &arguments) {
                    tools::Outcome::Unknown => Err(RpcError::new(
                        code::INVALID_PARAMS,
                        format!("unknown tool: {name}"),
                    )),
                    tools::Outcome::Done(value) => Ok(tool_result(&value, false)),
                    tools::Outcome::Failed(message) => {
                        Ok(tool_result(&json!({ "error": message }), true))
                    }
                }
            }
            other => Err(RpcError::new(
                code::METHOD_NOT_FOUND,
                format!("method not found: {other}"),
            )),
        }
    }

    /// Stop: cancel a run in progress and wait for its thread, so no solver
    /// outlives the session it belongs to.
    pub fn shutdown(&mut self) {
        self.runs.shutdown();
    }
}

/// The `initialize` result: the client's protocol revision when this server
/// speaks it, else the newest one it does (the client then decides).
fn initialize(params: &Value) -> Value {
    let requested = params.get("protocolVersion").and_then(Value::as_str);
    let version = requested
        .and_then(|requested| PROTOCOL_VERSIONS.iter().find(|v| **v == requested))
        .unwrap_or(&PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "ALAS", "version": env!("CARGO_PKG_VERSION") },
        "instructions": "ALAS aircraft conceptual design. Start with list_presets, create_config \
            and validate_config; start_run returns at once, poll run_status, then get_results. \
            Paths are relative to the workspace folder. Values are SI unless a field name says \
            otherwise. Report results with their findings; do not tune inputs to match a target.",
    })
}

/// A `tools/call` result: the value as text (every client reads that) and as
/// structured content (clients of 2025-06-18 and later).
fn tool_result(value: &Value, is_error: bool) -> Value {
    let text = serde_json::to_string_pretty(value).unwrap_or_default();
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": value,
        "isError": is_error,
    })
}

fn error_value(id: Value, error: RpcError) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": error.code, "message": error.message },
    })
}

fn error_reply(id: Value, error: RpcError) -> String {
    serde_json::to_string(&error_value(id, error)).unwrap_or_default()
}

/// Serve `workspace` on this process's stdin and stdout until stdin closes.
/// Returns the process exit code.
pub fn serve(workspace: Workspace) -> i32 {
    let mut server = Server::new(workspace);
    let code = serve_streams(&mut server, io::stdin().lock(), io::stdout().lock());
    server.shutdown();
    code
}

/// Discard input up to and including the next newline without holding it,
/// so the message after an over-long one is read normally.
fn skip_line(input: &mut impl BufRead) -> io::Result<()> {
    loop {
        let available = input.fill_buf()?;
        if available.is_empty() {
            return Ok(());
        }
        if let Some(end) = available.iter().position(|byte| *byte == b'\n') {
            input.consume(end + 1);
            return Ok(());
        }
        let length = available.len();
        input.consume(length);
    }
}

/// Longest message accepted, in bytes with its newline. A configuration
/// document is a few tens of kilobytes; the cap keeps a runaway client from
/// exhausting memory.
pub const MAX_MESSAGE_BYTES: u64 = 4 * 1024 * 1024;

/// The loop of [`serve`] over any streams.
fn serve_streams(server: &mut Server, mut input: impl BufRead, mut output: impl Write) -> i32 {
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        match io::Read::take(&mut input, MAX_MESSAGE_BYTES + 1).read_until(b'\n', &mut buffer) {
            Ok(0) => return 0,
            Ok(_) => {}
            Err(_) => return 1,
        }
        let too_long = buffer.len() as u64 > MAX_MESSAGE_BYTES;
        if too_long && buffer.last() != Some(&b'\n') && skip_line(&mut input).is_err() {
            return 1;
        }
        let reply = match std::str::from_utf8(&buffer) {
            _ if too_long => Some(error_reply(
                Value::Null,
                RpcError::new(
                    code::INVALID_REQUEST,
                    format!("message longer than {MAX_MESSAGE_BYTES} bytes"),
                ),
            )),
            Ok(line) if line.trim().is_empty() => continue,
            // A byte-order mark (a request file saved by a Windows editor)
            // is not part of the message.
            Ok(line) => server.handle_line(line.trim_start_matches('\u{feff}').trim_end()),
            Err(_) => Some(error_reply(
                Value::Null,
                RpcError::new(code::PARSE_ERROR, "message is not UTF-8"),
            )),
        };
        if let Some(reply) = reply {
            if writeln!(output, "{reply}")
                .and_then(|()| output.flush())
                .is_err()
            {
                return 1;
            }
        }
    }
}
