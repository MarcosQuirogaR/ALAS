#!/usr/bin/env python3
"""
ALAS Terminal Agent Control Panel
Standard library only. Cross-platform (Windows Git Bash / CMD / PowerShell / Linux).
Monitors running agents, exact prompts, outputs, progress, and review status.
Zero false-positives on PID reuse via OS process start identity verification.
"""

from __future__ import annotations

import argparse
import ctypes
import datetime
import json
import os
import pathlib
import re
import shutil
import sys
import tempfile
import textwrap
import time
from typing import Any, Dict, List, Optional, Tuple

# -----------------------------------------------------------------------------
# Constants & Configuration
# -----------------------------------------------------------------------------

DEFAULT_DISCOVERY_LIMIT = 80
AUTO_REFRESH_INTERVAL = 3.0
PID_TOLERANCE_SECONDS = 15.0
REGISTRY_LOCK_TIMEOUT = 10.0
REGISTRY_LOCK_STALE_SECONDS = 60.0

REGISTRY_REL_PATH = pathlib.Path(".agent/control/jobs.json")
REPORTS_DIR_REL = pathlib.Path(".agent/reports")

FORBIDDEN_PROMPT_PATTERNS = [
    r"auth",
    r"cookie",
    r"credential",
    r"secret",
    r"token",
    r"\.env",
    r"id_rsa",
    r"password",
    r"session_store",
]

# Regex for stripping ANSI escape sequences and non-printable control characters
ANSI_CSI_REGEX = re.compile(
    r"(?:\x1B[@-Z\\-_]|[\x80-\x9A\x9C-\x9F]|(?:\x1B\[|\x9B)[0-?]*[ -/]*[@-~])"
)
ANSI_OSC_REGEX = re.compile(r"\x1B\][^\x07\x1B]*(\x07|\x1B\\)")
CONTROL_CHAR_REGEX = re.compile(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f-\x9f]")
HTML_TAG_REGEX = re.compile(r"<[^>]+>")


def sanitize_text(text: Optional[str]) -> str:
    """
    Remove ANSI escape sequences, terminal control characters, and OSC sequences
    from untrusted text while preserving normal whitespace, newlines, and unicode.
    """
    if not text:
        return ""
    # Strip OSC sequences (e.g. title changes, hyperlinks)
    cleaned = ANSI_OSC_REGEX.sub("", text)
    # Strip ANSI CSI / color / cursor escapes
    cleaned = ANSI_CSI_REGEX.sub("", cleaned)
    # Strip control characters except newline (\n), carriage return (\r), and tab (\t)
    cleaned = CONTROL_CHAR_REGEX.sub("", cleaned)
    return cleaned


def strip_html_tags(text: str) -> str:
    """Strip HTML markup tags for display in plain text terminal."""
    if not text:
        return ""
    return HTML_TAG_REGEX.sub("", text)


def normalize_error_list(errors: Any) -> List[Any]:
    """
    Normalize a report's "errors" field to a list.
    Agent CLIs emit it as a list, a bare string, or omit it; iterating a bare
    string character-by-character would defeat the budget-cap classification.
    """
    if errors is None:
        return []
    if isinstance(errors, list):
        return errors
    if isinstance(errors, str):
        return [errors] if errors.strip() else []
    return [errors]


def format_timestamp_tz(iso_str: Optional[str]) -> Tuple[str, str, str]:
    """
    Convert an ISO timestamp string into:
      (compact_utc, full_utc, full_local)
    Example:
      compact_utc: "15:09:25 UTC"
      full_utc:    "2026-09-09 15:09:25 UTC"
      full_local:  "2026-09-09 17:09:25 CEST"
    """
    if not iso_str:
        return ("-", "-", "-")
    try:
        clean_ts = iso_str.replace("Z", "+00:00")
        dt = datetime.datetime.fromisoformat(clean_ts)
        if dt.tzinfo is None:
            dt_utc = dt.replace(tzinfo=datetime.timezone.utc)
        else:
            dt_utc = dt.astimezone(datetime.timezone.utc)

        dt_local = dt_utc.astimezone()
        tz_name = dt_local.tzname() or "local"

        compact_utc = dt_utc.strftime("%H:%M:%S UTC")
        full_utc = dt_utc.strftime("%Y-%m-%d %H:%M:%S UTC")
        full_local = dt_local.strftime(f"%Y-%m-%d %H:%M:%S {tz_name}")
        return (compact_utc, full_utc, full_local)
    except Exception:
        short = str(iso_str)[:12]
        return (short, str(iso_str), str(iso_str))


# -----------------------------------------------------------------------------
# OS Process Liveness & Start Identity Verification (Anti-PID-Reuse)
# -----------------------------------------------------------------------------


def parse_start_to_epoch(recorded: Any) -> Optional[float]:
    """
    Convert recorded start identity (ISO timestamp string, FILETIME int, or epoch float)
    into a Unix epoch timestamp (seconds).
    """
    if recorded is None:
        return None
    if isinstance(recorded, (int, float)):
        val = float(recorded)
        if val > 1e16:
            # Windows FILETIME (100-ns intervals since 1601-01-01)
            return (val - 116444736000000000.0) / 10000000.0
        elif val > 1e11:
            # Epoch in milliseconds
            return val / 1000.0
        return val
    if isinstance(recorded, str):
        s = recorded.strip()
        if not s:
            return None
        # Try numeric string
        try:
            val = float(s)
            if val > 1e16:
                return (val - 116444736000000000.0) / 10000000.0
            elif val > 1e11:
                return val / 1000.0
            return val
        except ValueError:
            pass
        # Try ISO format
        try:
            clean_ts = s.replace("Z", "+00:00")
            dt = datetime.datetime.fromisoformat(clean_ts)
            return dt.timestamp()
        except Exception:
            pass
    return None


def get_os_process_identity(pid: int) -> Tuple[bool, Optional[float], Optional[str]]:
    """
    Query the operating system for process active state and creation epoch.
    Returns: (is_alive, creation_epoch_seconds, error_reason)
    """
    if sys.platform == "win32":
        try:
            from ctypes import wintypes

            kernel32 = ctypes.windll.kernel32

            PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
            PROCESS_QUERY_INFORMATION = 0x0400
            STILL_ACTIVE = 259

            class FILETIME(ctypes.Structure):
                _fields_ = [
                    ("dwLowDateTime", wintypes.DWORD),
                    ("dwHighDateTime", wintypes.DWORD),
                ]

            handle = kernel32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, pid)
            if not handle:
                handle = kernel32.OpenProcess(PROCESS_QUERY_INFORMATION, False, pid)
            if not handle:
                # Process does not exist or access denied
                return False, None, "Process cannot be opened or does not exist"

            try:
                exit_code = wintypes.DWORD()
                if not kernel32.GetExitCodeProcess(handle, ctypes.byref(exit_code)):
                    return False, None, "GetExitCodeProcess failed"
                if exit_code.value != STILL_ACTIVE:
                    return False, None, f"Process exited with code {exit_code.value}"

                creation_time = FILETIME()
                exit_time = FILETIME()
                kernel_time = FILETIME()
                user_time = FILETIME()
                if not kernel32.GetProcessTimes(
                    handle,
                    ctypes.byref(creation_time),
                    ctypes.byref(exit_time),
                    ctypes.byref(kernel_time),
                    ctypes.byref(user_time),
                ):
                    return True, None, "GetProcessTimes failed"

                ft_val = (creation_time.dwHighDateTime << 32) | creation_time.dwLowDateTime
                epoch_sec = (ft_val - 116444736000000000.0) / 10000000.0
                return True, epoch_sec, None
            finally:
                kernel32.CloseHandle(handle)
        except Exception as ex:
            return False, None, f"Windows ctypes error: {ex}"
    else:
        # Linux / Unix / macOS
        proc_dir = pathlib.Path(f"/proc/{pid}")
        if not proc_dir.exists():
            return False, None, "Process not in /proc"
        try:
            # /proc/<pid> directory ctime represents the process creation time in Unix epoch
            epoch_sec = proc_dir.stat().st_ctime
            return True, epoch_sec, None
        except Exception as ex:
            return False, None, f"Unix /proc error: {ex}"


def verify_pid_liveness(pid: Optional[int], process_start: Optional[Any]) -> Dict[str, Any]:
    """
    Verify process liveness against recorded process_start to prevent PID-reuse false running.
    """
    if pid is None:
        return {
            "status": "NO_PID",
            "detail": "No PID registered",
            "is_verified_running": False,
        }

    is_alive, actual_epoch, err = get_os_process_identity(pid)
    if not is_alive:
        return {
            "status": "DEAD",
            "detail": err or f"Process {pid} is inactive",
            "is_verified_running": False,
        }

    # Process is alive in OS. Now verify start identity!
    recorded_epoch = parse_start_to_epoch(process_start)
    if recorded_epoch is None:
        return {
            "status": "UNKNOWN",
            "detail": f"Process {pid} active but start identity unverified (no recorded process_start)",
            "is_verified_running": False,
        }

    if actual_epoch is None:
        return {
            "status": "UNKNOWN",
            "detail": f"Process {pid} active but OS creation time could not be queried",
            "is_verified_running": False,
        }

    diff = abs(actual_epoch - recorded_epoch)
    if diff <= PID_TOLERANCE_SECONDS:
        return {
            "status": "RUNNING",
            "detail": f"Process {pid} verified running (start match: {diff:.1f}s)",
            "is_verified_running": True,
        }
    else:
        return {
            "status": "REUSED_PID",
            "detail": f"PID {pid} reused by a different process (start diff: {diff:.1f}s > {PID_TOLERANCE_SECONDS}s)",
            "is_verified_running": False,
        }


# -----------------------------------------------------------------------------
# Report Parser (Partial JSON, Claude Cap, Gemini Success/Denial)
# -----------------------------------------------------------------------------


def extract_partial_json_fields(text: str) -> Dict[str, Any]:
    """
    Fallback regex parser for incomplete/streaming JSON files during active writing.
    """
    data: Dict[str, Any] = {"_is_partial": True}

    patterns = {
        "status": r'"status"\s*:\s*"([^"]+)"',
        "subtype": r'"subtype"\s*:\s*"([^"]+)"',
        "terminal_reason": r'"terminal_reason"\s*:\s*"([^"]+)"',
        "is_error": r'"is_error"\s*:\s*(true|false)',
        "conversation_id": r'"conversation_id"\s*:\s*"([^"]+)"',
        "session_id": r'"session_id"\s*:\s*"([^"]+)"',
        "duration_ms": r'"duration_ms"\s*:\s*([0-9.]+)',
        "duration_seconds": r'"duration_seconds"\s*:\s*([0-9.]+)',
        "total_cost_usd": r'"total_cost_usd"\s*:\s*([0-9.]+)',
        "num_turns": r'"num_turns"\s*:\s*([0-9]+)',
        "result": r'"result"\s*:\s*"([^"\\]*(?:\\.[^"\\]*)*)',
        "response": r'"response"\s*:\s*"([^"\\]*(?:\\.[^"\\]*)*)',
    }

    for key, pat in patterns.items():
        m = re.search(pat, text)
        if m:
            val = m.group(1)
            if key == "is_error":
                data[key] = val.lower() == "true"
            elif key in ("duration_ms", "duration_seconds", "total_cost_usd"):
                try:
                    data[key] = float(val)
                except ValueError:
                    data[key] = val
            elif key == "num_turns":
                try:
                    data[key] = int(val)
                except ValueError:
                    data[key] = val
            else:
                data[key] = val

    if "denied_actions" in text:
        data["has_denied_actions"] = True

    return data


def parse_report_file(file_path: pathlib.Path) -> Dict[str, Any]:
    """
    Robustly read and parse agent report files (Claude & Gemini).
    Handles 0-byte files, streaming partial JSON, and completed JSON.
    """
    if not file_path.exists():
        return {"exists": False, "status": "MISSING"}

    try:
        size = file_path.stat().st_size
    except Exception:
        return {"exists": False, "status": "UNREADABLE"}

    if size == 0:
        return {
            "exists": True,
            "is_empty": True,
            "status": "EMPTY",
            "_is_partial": True,
            "raw_text": "",
        }

    try:
        content = file_path.read_text(encoding="utf-8", errors="replace")
    except Exception as ex:
        return {
            "exists": True,
            "is_error": True,
            "status": "READ_ERROR",
            "error_detail": str(ex),
        }

    try:
        parsed = json.loads(content)
        parsed["exists"] = True
        parsed["is_empty"] = False
        parsed["_is_partial"] = False
        parsed["raw_text"] = content
        return parsed
    except json.JSONDecodeError:
        partial = extract_partial_json_fields(content)
        partial["exists"] = True
        partial["is_empty"] = False
        partial["raw_text"] = content
        return partial


def extract_actual_model_from_report(report: Dict[str, Any]) -> Optional[str]:
    """
    Extract the dominant/reported actual model from Claude's modelUsage.
    Gemini CLI reports currently do not include model metadata.
    """
    model_usage = report.get("modelUsage", {})
    if isinstance(model_usage, dict) and model_usage:
        def model_priority(item):
            name, info = item
            if not isinstance(info, dict):
                return (0.0, 0)
            cost = float(info.get("costUSD", 0.0) or 0.0)
            tokens = int(info.get("outputTokens", 0) or 0)
            return (cost, tokens)

        sorted_models = sorted(model_usage.items(), key=model_priority, reverse=True)
        if sorted_models:
            top_name, top_info = sorted_models[0]
            if isinstance(top_info, dict) and top_info.get("canonicalModel"):
                return str(top_info["canonicalModel"])
            return str(top_name)
    return None


def evaluate_report_data(report: Dict[str, Any], filename: str = "") -> Dict[str, Any]:
    """
    Analyze parsed report data according to specific domain rules:
    - Claude error_max_budget_usd is invocation cap NOT quota.
    - Gemini empty SUCCESS response or denied_actions is NOT completed work.
    Robustly identifies provider based on JSON structure/semantics even with generic filenames.
    """
    res: Dict[str, Any] = {
        "status": "PENDING",
        "detail": "",
        "turns": None,
        "tokens": None,
        "cost_usd": None,
        "duration_sec": None,
        "content_text": "",
        "model_actual": None,
    }

    if report.get("exists") is False:
        res["status"] = "PENDING"
        res["detail"] = "Output file does not exist"
        return res

    if report.get("is_empty") is True:
        res["status"] = "PENDING"
        res["detail"] = "Output file is empty (0 bytes)"
        return res

    is_partial = report.get("_is_partial", False)

    # 1. Content-based Provider Recognition (with filename fallback)
    has_claude_keys = any(
        k in report
        for k in (
            "session_id",
            "modelUsage",
            "terminal_reason",
            "total_cost_usd",
            "duration_api_ms",
            "subagent_stats",
            "permission_denials",
        )
    ) or ("result" in report and "subtype" in report)

    has_gemini_keys = any(
        k in report
        for k in (
            "conversation_id",
            "denied_actions",
            "duration_seconds",
        )
    ) or ("response" in report and "status" in report)

    fn_lower = filename.lower()
    has_claude_fn = "claude" in fn_lower
    has_gemini_fn = "gemini" in fn_lower

    if has_claude_keys or (has_claude_fn and not has_gemini_keys):
        is_claude = True
        is_gemini = False
    elif has_gemini_keys or (has_gemini_fn and not has_claude_keys):
        is_claude = False
        is_gemini = True
    elif "result" in report:
        is_claude = True
        is_gemini = False
    elif "response" in report:
        is_claude = False
        is_gemini = True
    else:
        is_claude = False
        is_gemini = False

    # Extract turns
    if "num_turns" in report and report["num_turns"] is not None:
        try:
            res["turns"] = int(report["num_turns"])
        except (ValueError, TypeError):
            pass

    # Extract duration
    if "duration_ms" in report and isinstance(report["duration_ms"], (int, float)):
        res["duration_sec"] = report["duration_ms"] / 1000.0
    elif "duration_seconds" in report and isinstance(report["duration_seconds"], (int, float)):
        res["duration_sec"] = float(report["duration_seconds"])

    # Extract cost
    if "total_cost_usd" in report and isinstance(report["total_cost_usd"], (int, float)):
        res["cost_usd"] = float(report["total_cost_usd"])

    # Extract tokens
    usage = report.get("usage", {})
    if isinstance(usage, dict):
        if "total_tokens" in usage:
            res["tokens"] = usage.get("total_tokens")
        elif "output_tokens" in usage:
            out_t = usage.get("output_tokens", 0) or 0
            in_t = usage.get("input_tokens", 0) or 0
            res["tokens"] = in_t + out_t

    # Extract reported actual model
    actual_model = extract_actual_model_from_report(report)
    if actual_model:
        res["model_actual"] = sanitize_text(actual_model)

    # Content text
    content_text = report.get("result") or report.get("response") or ""
    res["content_text"] = sanitize_text(content_text)

    # 2. Specific Claude Evaluation Logic
    if is_claude:
        subtype = str(report.get("subtype", "")).lower()
        term_reason = str(report.get("terminal_reason", "")).lower()
        errors = normalize_error_list(report.get("errors"))
        is_error = report.get("is_error", False)

        has_budget_cap = (
            subtype == "error_max_budget_usd"
            or term_reason == "budget_exhausted"
            or any(
                "maximum budget" in str(e).lower() or "reached maximum budget" in str(e).lower()
                for e in errors
            )
        )

        if has_budget_cap:
            res["status"] = "CAP_REACHED"
            res["detail"] = "Invocation budget cap reached (per-job spend cap, not account quota)"
        elif is_error or errors:
            res["status"] = "FAILED"
            raw_err = errors[0] if errors else report.get("api_error_status", "Execution error")
            err_msg = sanitize_text(str(raw_err))
            res["detail"] = f"Error: {err_msg}"
        elif subtype == "success" or term_reason == "completed":
            res["status"] = "COMPLETED"
            res["detail"] = "Claude completed execution successfully"
        elif is_partial:
            res["status"] = "WRITING"
            res["detail"] = "Claude report is currently being written"
        else:
            res["status"] = "COMPLETED"
            res["detail"] = "Claude task finished"

    # 3. Specific Gemini Evaluation Logic
    elif is_gemini:
        gemini_status = sanitize_text(str(report.get("status", "")).upper())
        denied_actions = report.get("denied_actions", [])
        has_denied = bool(denied_actions) or report.get("has_denied_actions", False)
        is_error = report.get("is_error", False)
        raw_response = report.get("response")
        resp_text = (raw_response or "").strip()

        if has_denied:
            res["status"] = "DENIED"
            action_name = "action"
            if denied_actions and isinstance(denied_actions, list) and isinstance(denied_actions[0], dict):
                raw_name = denied_actions[0].get(
                    "display_name", denied_actions[0].get("action", "action")
                )
                action_name = sanitize_text(str(raw_name))
            res["detail"] = f"Work blocked: denied action ({action_name})"
        elif is_error or gemini_status in ("ERROR", "FAILED"):
            res["status"] = "FAILED"
            res["detail"] = "Gemini execution failed"
        elif gemini_status == "SUCCESS":
            if not resp_text and not is_partial:
                res["status"] = "INCOMPLETE"
                res["detail"] = "Empty SUCCESS response: work incomplete / stopped"
            elif is_partial:
                res["status"] = "WRITING"
                res["detail"] = "Gemini output is currently being written"
            else:
                res["status"] = "COMPLETED"
                res["detail"] = "Gemini completed successfully"
        elif is_partial:
            res["status"] = "WRITING"
            res["detail"] = "Gemini output is currently being written"
        else:
            res["status"] = gemini_status or "UNKNOWN"
            res["detail"] = f"Report status: {gemini_status}"

    else:
        # Generic fallback
        if is_partial:
            res["status"] = "WRITING"
            res["detail"] = "Report is currently being written"
        elif report.get("is_error"):
            res["status"] = "FAILED"
            res["detail"] = "Error reported"
        else:
            res["status"] = sanitize_text(str(report.get("status", "COMPLETED")))
            res["detail"] = "Execution finished"

    return res


# -----------------------------------------------------------------------------
# Job Model & State Aggregator
# -----------------------------------------------------------------------------


class Job:
    def __init__(
        self,
        job_id: str,
        title: str,
        provider: str,
        model: str,
        prompt_file: Optional[str] = None,
        prompt_kind: str = "exact",
        output_file: Optional[str] = None,
        review_file: Optional[str] = None,
        pid: Optional[int] = None,
        process_start: Optional[Any] = None,
        started_at: Optional[str] = None,
        verification: str = "pending",
        is_registered: bool = True,
        started_at_source: str = "recorded",
    ):
        self.id = job_id
        self.title = title
        self.provider = provider

        # Requested model (from registry)
        self.model_requested = model
        # Reported actual model (from output JSON modelUsage)
        self.model_actual: Optional[str] = None
        # Formatted composite model label for display
        self.model_display: str = model

        self.prompt_file = prompt_file
        self.prompt_kind = prompt_kind
        self.output_file = output_file
        self.review_file = review_file
        self.pid = pid
        self.process_start = process_start
        self.started_at = started_at
        self.started_at_source = started_at_source
        self.verification = verification
        self.is_registered = is_registered

        # Evaluated runtime state
        self.status: str = "UNKNOWN"
        self.status_detail: str = ""
        self.elapsed_str: str = "-"
        self.output_summary: str = "-"
        self.review_summary: str = "-"
        self.content_text: str = ""
        self.turns: Optional[int] = None
        self.tokens: Optional[int] = None
        self.cost_usd: Optional[float] = None
        self.duration_sec: Optional[float] = None

    def evaluate(self, repo_root: pathlib.Path) -> None:
        """
        Evaluate full status by verifying PID liveness, parsing output report,
        and checking review artifacts without fake progress percentages.
        Makes requested model vs reported actual model explicit.
        """
        # 1. Verify PID Liveness
        pid_info = verify_pid_liveness(self.pid, self.process_start)
        is_running = pid_info["is_verified_running"]

        # 2. Check Output File
        report_data = {}
        eval_data = {
            "status": "PENDING",
            "detail": "No output",
            "turns": None,
            "tokens": None,
            "cost_usd": None,
            "duration_sec": None,
            "content_text": "",
            "model_actual": None,
        }

        has_output = False
        if self.output_file:
            out_path = repo_root / self.output_file
            if out_path.exists():
                has_output = True
                report_data = parse_report_file(out_path)
                eval_data = evaluate_report_data(report_data, out_path.name)
                self.content_text = eval_data["content_text"]
                self.turns = eval_data["turns"]
                self.tokens = eval_data["tokens"]
                self.cost_usd = eval_data["cost_usd"]
                self.duration_sec = eval_data["duration_sec"]
                if eval_data.get("model_actual"):
                    self.model_actual = eval_data["model_actual"]

        # 3. Model Labeling: Requested vs Reported Actual Model
        if self.is_registered:
            if self.model_actual:
                req_norm = self.model_requested.lower().replace("-", "").replace(" ", "")
                act_norm = self.model_actual.lower().replace("-", "").replace(" ", "")
                if req_norm not in act_norm and act_norm not in req_norm:
                    short_act = self.model_actual
                    if short_act.startswith("claude-"):
                        short_act = short_act[7:]
                    self.model_display = f"{self.model_requested} -> {short_act}"
                else:
                    self.model_display = self.model_requested
            else:
                self.model_display = self.model_requested
        else:
            if "gemini" in self.provider.lower():
                self.model_requested = "unregistered (registry only)"
                self.model_actual = "not reported in Gemini report"
                self.model_display = "unrecorded"
            else:
                self.model_requested = "unregistered"
                if self.model_actual:
                    short_act = self.model_actual
                    if short_act.startswith("claude-"):
                        short_act = short_act[7:]
                    self.model_display = f"unregistered (act: {short_act})"
                else:
                    self.model_display = "unregistered"

        # 4. Determine Overall Process Status
        if is_running:
            self.status = "RUNNING"
            self.status_detail = pid_info["detail"]
        else:
            if has_output and not report_data.get("is_empty"):
                self.status = eval_data["status"]
                self.status_detail = eval_data["detail"]
            else:
                if self.pid is not None:
                    if pid_info["status"] == "REUSED_PID":
                        self.status = "UNKNOWN"
                        self.status_detail = pid_info["detail"]
                    elif pid_info["status"] == "DEAD":
                        self.status = "UNKNOWN"
                        self.status_detail = f"Process {self.pid} terminated without output"
                    else:
                        self.status = "UNKNOWN"
                        self.status_detail = pid_info["detail"]
                else:
                    self.status = "PENDING"
                    self.status_detail = "Pending agent launch (no PID registered)"

        # 5. Format Elapsed Time
        start_epoch = parse_start_to_epoch(self.started_at)
        now_epoch = time.time()

        if self.status == "RUNNING" and start_epoch:
            diff = max(0.0, now_epoch - start_epoch)
            self.elapsed_str = format_duration(diff)
        elif self.duration_sec is not None:
            self.elapsed_str = format_duration(self.duration_sec)
        elif start_epoch and self.output_file:
            out_path = repo_root / self.output_file
            if out_path.exists():
                try:
                    mtime = out_path.stat().st_mtime
                    diff = max(0.0, mtime - start_epoch)
                    self.elapsed_str = format_duration(diff)
                except Exception:
                    self.elapsed_str = "-"
            else:
                self.elapsed_str = "-"
        else:
            self.elapsed_str = "-"

        # 6. Format Output Summary
        metrics = []
        if self.cost_usd is not None:
            metrics.append(f"${self.cost_usd:.2f}")
        if self.tokens is not None:
            if self.tokens >= 1000:
                metrics.append(f"{self.tokens/1000:.1f}k tok")
            else:
                metrics.append(f"{self.tokens} tok")
        if self.turns is not None:
            metrics.append(f"{self.turns}t")

        if metrics:
            self.output_summary = " ".join(metrics)
        elif self.status == "RUNNING":
            self.output_summary = "[running]"
        elif self.status == "PENDING":
            self.output_summary = "[pending]"
        elif has_output:
            self.output_summary = "[ready]"
        else:
            self.output_summary = "-"

        # 7. Format Review / Verification Status (Separate from process completion!)
        if self.verification and self.verification.strip():
            self.review_summary = self.verification.strip()
        elif self.review_file:
            rev_path = repo_root / self.review_file
            if rev_path.exists():
                self.review_summary = extract_review_verdict(rev_path)
            else:
                self.review_summary = "pending"
        else:
            self.review_summary = "-"

    def to_dict(self) -> Dict[str, Any]:
        compact_utc, full_utc, full_local = format_timestamp_tz(self.started_at)
        return {
            "id": self.id,
            "title": self.title,
            "provider": self.provider,
            "model_requested": self.model_requested,
            "model_actual": self.model_actual,
            "model": self.model_display,
            "status": self.status,
            "status_detail": self.status_detail,
            "started_at": self.started_at,
            "started_at_source": self.started_at_source,
            "started_at_utc": full_utc,
            "started_at_local": full_local,
            "process_start": self.process_start,
            "elapsed": self.elapsed_str,
            "duration_seconds": self.duration_sec,
            "prompt_file": self.prompt_file,
            "prompt_kind": self.prompt_kind,
            "output_file": self.output_file,
            "output_summary": self.output_summary,
            "review_file": self.review_file,
            "review_summary": self.review_summary,
            "verification": self.verification,
            "pid": self.pid,
            "turns": self.turns,
            "tokens": self.tokens,
            "cost_usd": self.cost_usd,
            "is_registered": self.is_registered,
        }


def format_duration(seconds: float) -> str:
    """Format seconds into readable compact elapsed string (e.g. 1h 23m, 4m 12s, 5.2s)."""
    if seconds < 0:
        return "-"
    if seconds < 60:
        return f"{seconds:.1f}s"
    minutes = int(seconds // 60)
    rem_sec = int(seconds % 60)
    if minutes < 60:
        return f"{minutes}m {rem_sec:02d}s"
    hours = int(minutes // 60)
    rem_min = int(minutes % 60)
    return f"{hours}h {rem_min:02d}m"


def extract_review_verdict(path: pathlib.Path) -> str:
    """Read review file snippet to extract status without fabricated claims."""
    try:
        sample = path.read_text(encoding="utf-8", errors="replace")[:2000].lower()
        if "not approved" in sample or "verdict: not approved" in sample or "rejected" in sample:
            return "rejected"
        if "blocked" in sample:
            return "blocked"
        if "pass" in sample or "approved" in sample or "accepted" in sample:
            return "accepted"
        return "reviewed"
    except Exception:
        return "exists"


# -----------------------------------------------------------------------------
# Prompt & Review Security Policy
# -----------------------------------------------------------------------------


def get_job_prompt(job: Job, repo_root: pathlib.Path) -> Tuple[str, str]:
    """
    Load exact prompt content only from explicit registry files.
    Enforces security policy: NEVER reads auth, cookies, or raw credential transcripts.
    Missing historical prompts display 'not captured'.
    """
    if not job.prompt_file:
        return (
            "Prompt not captured for this historical job (no explicit prompt file registered)",
            "not captured",
        )

    norm_path = job.prompt_file.replace("\\", "/").lower()
    for pat in FORBIDDEN_PROMPT_PATTERNS:
        if re.search(pat, norm_path):
            return (
                f"[SECURITY BLOCKED] Access to sensitive or credential file rejected: {job.prompt_file}",
                "security_blocked",
            )

    full_path = repo_root / job.prompt_file
    if not full_path.exists() or not full_path.is_file():
        return (
            f"Prompt file not found on disk: {job.prompt_file}",
            job.prompt_kind or "missing",
        )

    try:
        content = full_path.read_text(encoding="utf-8", errors="replace")
        sanitized = sanitize_text(content)
        return sanitized, job.prompt_kind or "exact"
    except Exception as ex:
        return f"Error reading prompt file: {ex}", "error"


def get_job_review(job: Job, repo_root: pathlib.Path) -> Tuple[str, str]:
    """
    Load review report content from job.review_file.
    Returns (review_text, status_label).
    """
    if not job.review_file:
        return (
            f"No review report file specified for this job.\nVerification status: {job.verification}",
            "no_review_file",
        )

    full_path = repo_root / job.review_file
    if not full_path.exists() or not full_path.is_file():
        return (
            f"Review report file not found on disk: {job.review_file}\nVerification status: {job.verification}",
            "missing_file",
        )

    try:
        raw = full_path.read_text(encoding="utf-8", errors="replace")
        if full_path.suffix.lower() in (".html", ".htm"):
            clean = strip_html_tags(raw)
        else:
            clean = raw
        sanitized = sanitize_text(clean)
        return sanitized, "reviewed"
    except Exception as ex:
        return f"Error reading review file: {ex}", "read_error"


def wrap_text_to_viewport(text: str, width: int) -> List[str]:
    """
    Wrap text to fit within width while preserving logical line breaks.
    Long single-line paragraphs are broken into multiple lines of <= width
    so that all content is scrollable and readable in the terminal viewport.
    """
    if not text:
        return [""]
    w = max(10, width)
    wrapped_lines: List[str] = []

    for raw_line in text.splitlines():
        if not raw_line.strip():
            wrapped_lines.append("")
            continue
        parts = textwrap.wrap(
            raw_line,
            width=w,
            expand_tabs=False,
            replace_whitespace=False,
            drop_whitespace=True,
            break_long_words=True,
            break_on_hyphens=True,
        )
        if parts:
            wrapped_lines.extend(parts)
        else:
            wrapped_lines.append("")
    return wrapped_lines if wrapped_lines else [""]


# -----------------------------------------------------------------------------
# Registry Management & Auto-Discovery
# -----------------------------------------------------------------------------


class RegistryLock:
    """
    Cross-platform sentinel-file lock protecting .agent/control/jobs.json
    against concurrent writers (e.g. parallel background agents registering
    themselves on start/completion). Uses standard library O_CREAT|O_EXCL.
    Reclaims locks older than REGISTRY_LOCK_STALE_SECONDS to survive crashed writers.
    """

    def __init__(
        self,
        lock_path: pathlib.Path,
        timeout: float = REGISTRY_LOCK_TIMEOUT,
        stale_seconds: float = REGISTRY_LOCK_STALE_SECONDS,
    ):
        self.lock_path = lock_path
        self.timeout = timeout
        self.stale_seconds = stale_seconds
        self.fd: Optional[int] = None

    def __enter__(self) -> "RegistryLock":
        deadline = time.time() + self.timeout
        while True:
            try:
                flags = os.O_CREAT | os.O_EXCL | os.O_RDWR
                self.fd = os.open(str(self.lock_path), flags, 0o644)
                os.write(self.fd, f"{os.getpid()}\n{time.time():.3f}\n".encode("utf-8"))
                return self
            except FileExistsError:
                pass  # Another writer holds the lock
            except PermissionError:
                # Windows only: unlinking a file leaves it "delete pending" until
                # the last handle closes, and os.open on it reports
                # ERROR_ACCESS_DENIED instead of EEXIST. This is ordinary
                # contention. Letting it propagate aborted the caller's
                # registration outright and silently dropped that job's row.
                pass

            # Reclaim a lock orphaned by a crashed or killed writer
            try:
                mtime = self.lock_path.stat().st_mtime
                if time.time() - mtime > self.stale_seconds:
                    try:
                        self.lock_path.unlink()
                        continue
                    except OSError:
                        pass
            except OSError:
                pass

            if time.time() >= deadline:
                break
            time.sleep(0.02)
        return self

    @property
    def acquired(self) -> bool:
        """True only when this instance actually owns the sentinel file."""
        return self.fd is not None

    def __exit__(self, exc_type, exc_val, exc_tb):
        # Release ONLY a lock this instance owns. A writer that timed out and
        # proceeded unlocked must never delete the sentinel of the writer that
        # does hold it: that would admit a third writer mid-write and reintroduce
        # the lost update the lock exists to prevent.
        if self.fd is None:
            return False
        try:
            os.close(self.fd)
        except OSError:
            pass
        self.fd = None
        # Releasing must not leave a sentinel behind: a surviving lock file blocks
        # every later writer until the stale timeout elapses.
        for _ in range(5):
            try:
                self.lock_path.unlink()
                break
            except FileNotFoundError:
                break
            except OSError:
                time.sleep(0.01)
        return False


def load_registry(repo_root: pathlib.Path) -> List[Dict[str, Any]]:
    """Load jobs registry from .agent/control/jobs.json."""
    reg_path = repo_root / REGISTRY_REL_PATH
    if not reg_path.exists():
        return []
    try:
        text = reg_path.read_text(encoding="utf-8", errors="replace")
        data = json.loads(text)
        return data.get("jobs", [])
    except Exception:
        return []


def discover_report_files(repo_root: pathlib.Path) -> List[pathlib.Path]:
    """
    Auto-discover .agent/reports/claude-*.json and gemini-*.json.
    Sorted by modification time descending (newest first).
    """
    reports_dir = repo_root / REPORTS_DIR_REL
    if not reports_dir.exists() or not reports_dir.is_dir():
        return []

    found: List[pathlib.Path] = []
    try:
        for entry in reports_dir.iterdir():
            if not entry.is_file():
                continue
            name = entry.name.lower()
            if (name.startswith("claude-") or name.startswith("gemini-")) and name.endswith(".json"):
                found.append(entry)
    except Exception:
        return []

    found.sort(key=lambda p: p.stat().st_mtime if p.exists() else 0, reverse=True)
    return found


def humanize_title_from_filename(filename: str) -> str:
    """Convert filename like claude-mses-strict-review.json to readable title."""
    stem = filename
    if stem.endswith(".json"):
        stem = stem[:-5]
    if stem.startswith("claude-"):
        stem = stem[7:]
    elif stem.startswith("gemini-"):
        stem = stem[7:]
    parts = stem.replace("_", "-").split("-")
    words = [p.capitalize() for p in parts if p]
    return " ".join(words) or stem


def build_job_list(repo_root: pathlib.Path, include_all: bool = False) -> List[Job]:
    """
    Merge explicit registered jobs with auto-discovered report files.
    Default discovery limit is newest 80 files unless include_all is True.
    Registered jobs always take precedence.
    """
    registered_raw = load_registry(repo_root)
    jobs: List[Job] = []

    seen_output_files = set()
    seen_ids = set()

    for item in registered_raw:
        job_id = str(item.get("id", "")).strip()
        if not job_id:
            continue
        out_file = item.get("output_file")
        if out_file:
            norm_out = pathlib.Path(out_file).as_posix().lower()
            seen_output_files.add(norm_out)
        seen_ids.add(job_id)

        job = Job(
            job_id=job_id,
            title=str(item.get("title", job_id)),
            provider=str(item.get("provider", "Unknown")),
            model=str(item.get("model", "unknown")),
            prompt_file=item.get("prompt_file"),
            prompt_kind=str(item.get("prompt_kind", "exact")),
            output_file=out_file,
            review_file=item.get("review_file"),
            pid=item.get("pid"),
            process_start=item.get("process_start"),
            started_at=item.get("started_at"),
            started_at_source="recorded",
            verification=str(item.get("verification", "pending")),
            is_registered=True,
        )
        job.evaluate(repo_root)
        jobs.append(job)

    # Discovered files
    discovered_paths = discover_report_files(repo_root)
    limit = None if include_all else DEFAULT_DISCOVERY_LIMIT
    if limit is not None:
        discovered_paths = discovered_paths[:limit]

    for p in discovered_paths:
        rel_posix = p.relative_to(repo_root).as_posix()
        if rel_posix.lower() in seen_output_files:
            continue

        file_stem = p.stem
        candidate_id = file_stem
        if candidate_id in seen_ids:
            candidate_id = f"disc-{file_stem}"

        seen_output_files.add(rel_posix.lower())
        seen_ids.add(candidate_id)

        is_claude = file_stem.lower().startswith("claude")
        provider = "Claude Code" if is_claude else "Gemini CLI"
        title = humanize_title_from_filename(p.name)

        review_file = None
        review_stem = file_stem
        if review_stem.startswith("claude-"):
            review_stem = review_stem[7:]
        elif review_stem.startswith("gemini-"):
            review_stem = review_stem[7:]

        potential_reviews = [
            REPORTS_DIR_REL / f"{review_stem}.md",
            REPORTS_DIR_REL / f"{file_stem}.md",
            REPORTS_DIR_REL / f"{review_stem}.html",
            REPORTS_DIR_REL / f"{file_stem}.html",
        ]
        for pr in potential_reviews:
            if (repo_root / pr).exists():
                review_file = pr.as_posix()
                break

        # A report file's mtime is when the agent *finished* writing, not when it started.
        # Recover start time as (mtime - duration) when the duration is recorded.
        mtime = p.stat().st_mtime
        parsed_preview = parse_report_file(p)
        eval_preview = evaluate_report_data(parsed_preview, p.name)
        duration_sec = eval_preview.get("duration_sec")
        if duration_sec is not None and duration_sec > 0:
            inferred_start_epoch = max(0.0, mtime - duration_sec)
            started_at_source = "inferred (report mtime - duration)"
        else:
            inferred_start_epoch = mtime
            started_at_source = "inferred (report mtime = finish time)"
        iso_start = datetime.datetime.fromtimestamp(
            inferred_start_epoch, datetime.timezone.utc
        ).isoformat()

        job = Job(
            job_id=candidate_id,
            title=title,
            provider=provider,
            model="unregistered",
            prompt_file=None,
            prompt_kind="not captured",
            output_file=rel_posix,
            review_file=review_file,
            pid=None,
            process_start=None,
            started_at=iso_start,
            started_at_source=started_at_source,
            verification="pending",
            is_registered=False,
        )
        job.evaluate(repo_root)
        jobs.append(job)

    def sort_key(j: Job) -> Tuple[int, float]:
        reg_rank = 0 if j.is_registered else 1
        ep = parse_start_to_epoch(j.started_at) or 0.0
        return (reg_rank, -ep)

    jobs.sort(key=sort_key)
    return jobs


def atomic_register_job(
    repo_root: pathlib.Path,
    job_id: str,
    title: str,
    provider: str,
    model: str,
    prompt_file: str,
    output_file: str,
    pid: Optional[int] = None,
    started_at: Optional[str] = None,
    review_file: Optional[str] = None,
    prompt_kind: str = "exact",
    verification: str = "pending",
) -> None:
    """
    Atomically register or update a job in .agent/control/jobs.json.
    Preserves all existing rows and fields. Captures process_start identity when a
    live PID is supplied. Metadata-only updates (verification, review_file, title)
    deliberately do NOT rewrite started_at/process_start: overwriting the recorded
    OS start identity of a still-running process makes it fail the PID-reuse check
    and be misreported as REUSED_PID. Serialized against concurrent writers by
    RegistryLock; raises RuntimeError rather than discarding an unreadable registry.
    """
    reg_path = repo_root / REGISTRY_REL_PATH
    reg_dir = reg_path.parent
    reg_dir.mkdir(parents=True, exist_ok=True)

    with RegistryLock(reg_dir / "jobs.json.lock"):
        data: Dict[str, Any] = {"version": 1, "jobs": []}
        if reg_path.exists():
            raw_text = reg_path.read_text(encoding="utf-8", errors="replace")
            if raw_text.strip():
                try:
                    loaded = json.loads(raw_text)
                except json.JSONDecodeError as ex:
                    raise RuntimeError(
                        f"Registry {reg_path} exists but is not valid JSON ({ex}). "
                        "Refusing to overwrite; repair or move the file first."
                    ) from ex
                if not isinstance(loaded, dict):
                    raise RuntimeError(
                        f"Registry {reg_path} is not a JSON object. Refusing to overwrite."
                    )
                data = loaded
                if not isinstance(data.get("jobs"), list):
                    data["jobs"] = []

        now_iso = datetime.datetime.now(datetime.timezone.utc).isoformat()

        captured_process_start: Optional[str] = None
        if pid is not None:
            is_alive, creation_epoch, _ = get_os_process_identity(pid)
            if is_alive and creation_epoch:
                captured_process_start = datetime.datetime.fromtimestamp(
                    creation_epoch, datetime.timezone.utc
                ).isoformat()

        new_row: Dict[str, Any] = {
            "id": job_id,
            "title": title,
            "provider": provider,
            "model": model,
            "prompt_file": prompt_file,
            "prompt_kind": prompt_kind,
            "output_file": output_file,
            "review_file": review_file,
            "verification": verification,
            "pid": pid,
        }

        new_row = {k: v for k, v in new_row.items() if v is not None}

        updated = False
        new_jobs = []
        for existing in data["jobs"]:
            if str(existing.get("id")) == job_id:
                merged = dict(existing)
                merged.update(new_row)
                is_relaunch = pid is not None and existing.get("pid") != pid
                if started_at:
                    merged["started_at"] = started_at
                elif is_relaunch or not merged.get("started_at"):
                    merged["started_at"] = now_iso
                if captured_process_start:
                    merged["process_start"] = captured_process_start
                elif is_relaunch or not merged.get("process_start"):
                    merged["process_start"] = merged["started_at"]
                new_jobs.append(merged)
                updated = True
            else:
                new_jobs.append(existing)

        if not updated:
            new_row["started_at"] = started_at or now_iso
            new_row["process_start"] = captured_process_start or new_row["started_at"]
            new_jobs.append(new_row)

        data["jobs"] = new_jobs

        # The staging name must be unique per writer, not per process: threads in
        # one interpreter share a PID, so a pid-only name let two writers collide
        # on the same file and the loser's replace() raced against a vanished path.
        serialized = json.dumps(data, indent=2, ensure_ascii=False)
        fd, temp_name = tempfile.mkstemp(
            dir=str(reg_dir), prefix="jobs.json.tmp.", suffix=".partial"
        )
        temp_file = pathlib.Path(temp_name)
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as handle:
                handle.write(serialized + "\n")
                handle.flush()
                os.fsync(handle.fileno())
            temp_file.replace(reg_path)
        finally:
            if temp_file.exists():
                try:
                    temp_file.unlink()
                except OSError:
                    pass


# -----------------------------------------------------------------------------
# Terminal UI & Formatting
# -----------------------------------------------------------------------------


def truncate_str(text: str, width: int) -> str:
    """
    Truncate text to width with ellipsis if needed.
    Sanitizes first so that escape sequences smuggled in through registry fields,
    report filenames, or agent-authored status text can neither reach the terminal
    nor consume invisible column width and break table alignment.
    """
    text = sanitize_text(text)
    if "\n" in text or "\r" in text or "\t" in text:
        text = text.replace("\r\n", " ").replace("\r", " ").replace("\n", " ").replace("\t", " ")
    if width <= 0:
        return ""
    if len(text) <= width:
        return text.ljust(width)
    if width <= 3:
        return text[:width]
    return text[: width - 3] + "..."


def format_table(jobs: List[Job], term_width: int, selected_index: int = -1) -> List[str]:
    """
    Format readable table of jobs strictly fitting term_width.
    Adapts responsive layout to terminal width:
    - At >= 115 cols: Full columns with START (UTC).
    - At 90-114 cols: Compact columns with START (UTC).
    - At 75-89 cols (standard 80x24): Drops START (UTC) to prioritize Title,
      Provider, Model, Status, Elapsed, Output, Review. Line width <= term_width.
    - At < 75 cols: Compact minimal layout.
    Never exceeds term_width or forces wrapping on narrow viewports.
    """
    lines = []
    width = max(35, term_width)

    col_sel = 2

    if width >= 115:
        # Full 8-column layout
        show_start = True
        show_prov = True
        col_prov = 11
        col_model = 16
        col_status = 12
        col_start = 12
        col_elapsed = 9
        col_out = 12
        col_rev = 14
        inter_spaces = 8
        fixed_sum = (
            col_sel
            + col_prov
            + col_model
            + col_status
            + col_start
            + col_elapsed
            + col_out
            + col_rev
            + inter_spaces
        )
        col_title = max(16, width - fixed_sum)

        header = (
            f"{'':2} "
            f"{'TITLE / ID'.ljust(col_title)} "
            f"{'PROVIDER'.ljust(col_prov)} "
            f"{'MODEL'.ljust(col_model)} "
            f"{'STATUS'.ljust(col_status)} "
            f"{'START (UTC)'.ljust(col_start)} "
            f"{'ELAPSED'.ljust(col_elapsed)} "
            f"{'OUTPUT'.ljust(col_out)} "
            f"{'REVIEW'.ljust(col_rev)}"
        )

    elif width >= 90:
        # Compact 8-column layout
        show_start = True
        show_prov = True
        col_prov = 8
        col_model = 13
        col_status = 10
        col_start = 9
        col_elapsed = 8
        col_out = 10
        col_rev = 11
        inter_spaces = 8
        fixed_sum = (
            col_sel
            + col_prov
            + col_model
            + col_status
            + col_start
            + col_elapsed
            + col_out
            + col_rev
            + inter_spaces
        )
        col_title = max(14, width - fixed_sum)

        header = (
            f"{'':2} "
            f"{'TITLE / ID'.ljust(col_title)} "
            f"{'PROVIDER'.ljust(col_prov)} "
            f"{'MODEL'.ljust(col_model)} "
            f"{'STATUS'.ljust(col_status)} "
            f"{'START (UTC)'.ljust(col_start)} "
            f"{'ELAPSED'.ljust(col_elapsed)} "
            f"{'OUTPUT'.ljust(col_out)} "
            f"{'REVIEW'.ljust(col_rev)}"
        )

    elif width >= 75:
        # Responsive 7-column layout (calibrated for standard 80-column terminal)
        # Drops START (UTC) to keep Title readable; full start time is in detail views.
        show_start = False
        show_prov = True
        col_prov = 8
        col_model = 11
        col_status = 10
        col_elapsed = 7
        col_out = 9
        col_rev = 9
        inter_spaces = 7
        fixed_sum = (
            col_sel
            + col_prov
            + col_model
            + col_status
            + col_elapsed
            + col_out
            + col_rev
            + inter_spaces
        )
        col_title = max(14, width - fixed_sum)

        header = (
            f"{'':2} "
            f"{'TITLE / ID'.ljust(col_title)} "
            f"{'PROVIDER'.ljust(col_prov)} "
            f"{'MODEL'.ljust(col_model)} "
            f"{'STATUS'.ljust(col_status)} "
            f"{'ELAPSED'.ljust(col_elapsed)} "
            f"{'OUTPUT'.ljust(col_out)} "
            f"{'REVIEW'.ljust(col_rev)}"
        )

    else:
        # Narrow layout (<75 cols)
        show_start = False
        show_prov = False
        col_model = 10
        col_status = 9
        col_elapsed = 6
        col_out = 7
        col_rev = 8
        inter_spaces = 6
        fixed_sum = col_sel + col_model + col_status + col_elapsed + col_out + col_rev + inter_spaces
        col_title = max(10, width - fixed_sum)

        header = (
            f"{'':2} "
            f"{'TITLE / ID'.ljust(col_title)} "
            f"{'MODEL'.ljust(col_model)} "
            f"{'STATUS'.ljust(col_status)} "
            f"{'ELAPSED'.ljust(col_elapsed)} "
            f"{'OUTPUT'.ljust(col_out)} "
            f"{'REVIEW'.ljust(col_rev)}"
        )

    header = header[:width]
    sep = ("-" * len(header))[:width]
    lines.append(header)
    lines.append(sep)

    for idx, job in enumerate(jobs):
        sel_prefix = "> " if idx == selected_index else "  "
        title_display = f"{job.id}: {job.title}" if job.id != job.title else job.title
        title_str = truncate_str(title_display, col_title)
        model_str = truncate_str(job.model_display, col_model)
        status_str = truncate_str(job.status, col_status)
        elapsed_str = truncate_str(job.elapsed_str, col_elapsed)
        out_str = truncate_str(job.output_summary, col_out)
        rev_str = truncate_str(job.review_summary, col_rev)

        if show_start and show_prov:
            prov_str = truncate_str(job.provider, col_prov)
            compact_utc, _, _ = format_timestamp_tz(job.started_at)
            start_str = truncate_str(compact_utc, col_start)
            row = (
                f"{sel_prefix}"
                f"{title_str} "
                f"{prov_str} "
                f"{model_str} "
                f"{status_str} "
                f"{start_str} "
                f"{elapsed_str} "
                f"{out_str} "
                f"{rev_str}"
            )
        elif show_prov:
            prov_str = truncate_str(job.provider, col_prov)
            row = (
                f"{sel_prefix}"
                f"{title_str} "
                f"{prov_str} "
                f"{model_str} "
                f"{status_str} "
                f"{elapsed_str} "
                f"{out_str} "
                f"{rev_str}"
            )
        else:
            row = (
                f"{sel_prefix}"
                f"{title_str} "
                f"{model_str} "
                f"{status_str} "
                f"{elapsed_str} "
                f"{out_str} "
                f"{rev_str}"
            )

        lines.append(row[:width])

    return lines


# -----------------------------------------------------------------------------
# Cross-Platform Keyboard Reader
# -----------------------------------------------------------------------------


class KeyReader:
    def __init__(self):
        self.is_windows = sys.platform == "win32"
        self.old_settings = None
        if not self.is_windows and sys.stdin.isatty():
            try:
                import termios

                self.old_settings = termios.tcgetattr(sys.stdin)
            except Exception:
                pass

    def enable_raw(self):
        if not self.is_windows and sys.stdin.isatty():
            try:
                import tty

                tty.setcbreak(sys.stdin.fileno())
            except Exception:
                pass

    def restore(self):
        if not self.is_windows and self.old_settings:
            try:
                import termios

                termios.tcsetattr(sys.stdin, termios.TCSADRAIN, self.old_settings)
            except Exception:
                pass

    def read_key(self, timeout: float = 0.1) -> Optional[str]:
        """
        Read single normalized keypress. Returns:
        'up', 'down', 'pageup', 'pagedown', 'enter', 'esc', 'backspace', or single char.
        """
        if self.is_windows:
            import msvcrt

            start = time.time()
            while time.time() - start < timeout:
                if msvcrt.kbhit():
                    ch = msvcrt.getwch()
                    if ch in ("\x00", "\xe0"):
                        sc = msvcrt.getwch()
                        if sc == "H":
                            return "up"
                        elif sc == "P":
                            return "down"
                        elif sc == "I":
                            return "pageup"
                        elif sc == "Q":
                            return "pagedown"
                        elif sc == "K":
                            return "left"
                        elif sc == "M":
                            return "right"
                    elif ch in ("\r", "\n"):
                        return "enter"
                    elif ch == "\x1b":
                        return "esc"
                    elif ch in ("\x08", "\x7f"):
                        return "backspace"
                    else:
                        return ch.lower()
                time.sleep(0.02)
            return None
        else:
            import select

            rlist, _, _ = select.select([sys.stdin], [], [], timeout)
            if not rlist:
                return None
            try:
                ch = sys.stdin.read(1)
            except Exception:
                return None

            if ch == "\x1b":
                rlist2, _, _ = select.select([sys.stdin], [], [], 0.05)
                if not rlist2:
                    return "esc"
                ch2 = sys.stdin.read(1)
                if ch2 == "[":
                    ch3 = sys.stdin.read(1)
                    if ch3 == "A":
                        return "up"
                    elif ch3 == "B":
                        return "down"
                    elif ch3 == "5":
                        sys.stdin.read(1)  # ~
                        return "pageup"
                    elif ch3 == "6":
                        sys.stdin.read(1)  # ~
                        return "pagedown"
                return "esc"
            elif ch in ("\r", "\n"):
                return "enter"
            elif ch in ("\x08", "\x7f"):
                return "backspace"
            else:
                return ch.lower()


# -----------------------------------------------------------------------------
# Interactive TUI Controller & State Machine
# -----------------------------------------------------------------------------


def enable_windows_ansi():
    """Enable virtual terminal processing on Windows consoles for ANSI codes."""
    if sys.platform == "win32":
        try:
            kernel32 = ctypes.windll.kernel32
            hOut = kernel32.GetStdHandle(-11)  # STD_OUTPUT_HANDLE
            mode = ctypes.c_ulong()
            if kernel32.GetConsoleMode(hOut, ctypes.byref(mode)):
                kernel32.SetConsoleMode(hOut, mode.value | 0x0004)
        except Exception:
            pass
        try:
            sys.stdout.reconfigure(encoding="utf-8", errors="replace")
            sys.stderr.reconfigure(encoding="utf-8", errors="replace")
        except Exception:
            pass


class TUIController:
    """
    Manages TUI state, dirty tracking, and screen rendering.
    Enforces bounded idle redraw: redraws ONLY on initial frame, user input,
    terminal resize, or 3-second auto-refresh timer expiration.
    """

    def __init__(self, repo_root: pathlib.Path, include_all: bool = False):
        self.repo_root = repo_root
        self.include_all = include_all
        self.mode = "LIST"  # "LIST", "PROMPT", "OUTPUT", "REVIEW"
        self.selected_idx = 0
        self.list_offset = 0
        self.scroll_offset = 0
        # None = never refreshed; distinguishes "first frame" from "timer expired"
        # without falling back to an empty-job-list probe.
        self.last_refresh_time: Optional[float] = None
        self.prev_term_size: Tuple[int, int] = (0, 0)
        self.dirty = True
        self.jobs: List[Job] = []
        self.redraw_count = 0

    def refresh_jobs(self, now: float) -> None:
        self.jobs = build_job_list(self.repo_root, include_all=self.include_all)
        self.last_refresh_time = now
        if self.selected_idx >= len(self.jobs):
            self.selected_idx = max(0, len(self.jobs) - 1)
        self.dirty = True

    def handle_key(self, key: str, now: float) -> bool:
        """
        Process a key press. Returns True to continue, False to quit.
        Sets dirty=True on any state-changing key.
        """
        if self.mode == "LIST":
            if key in ("q", "\x03"):
                return False
            elif key in ("up", "k"):
                if self.selected_idx > 0:
                    self.selected_idx -= 1
                    self.dirty = True
            elif key in ("down", "j"):
                if self.selected_idx < len(self.jobs) - 1:
                    self.selected_idx += 1
                    self.dirty = True
            elif key == "pageup":
                new_idx = max(0, self.selected_idx - 10)
                if new_idx != self.selected_idx:
                    self.selected_idx = new_idx
                    self.dirty = True
            elif key == "pagedown":
                new_idx = min(max(0, len(self.jobs) - 1), self.selected_idx + 10)
                if new_idx != self.selected_idx:
                    self.selected_idx = new_idx
                    self.dirty = True
            elif key in ("p", "enter"):
                self.mode = "PROMPT"
                self.scroll_offset = 0
                self.dirty = True
            elif key == "o":
                self.mode = "OUTPUT"
                self.scroll_offset = 0
                self.dirty = True
            elif key == "v":
                self.mode = "REVIEW"
                self.scroll_offset = 0
                self.dirty = True
            elif key == "r":
                self.refresh_jobs(now)

        elif self.mode in ("PROMPT", "OUTPUT", "REVIEW"):
            if key in ("b", "q", "esc", "backspace"):
                self.mode = "LIST"
                self.scroll_offset = 0
                self.dirty = True
            elif key in ("up", "k"):
                if self.scroll_offset > 0:
                    self.scroll_offset -= 1
                    self.dirty = True
            elif key in ("down", "j"):
                self.scroll_offset += 1
                self.dirty = True
            elif key == "pageup":
                self.scroll_offset = max(0, self.scroll_offset - 10)
                self.dirty = True
            elif key == "pagedown":
                self.scroll_offset += 10
                self.dirty = True
            elif key == "r":
                self.refresh_jobs(now)

        return True

    def render_if_dirty(
        self, now: float, term_size: Tuple[int, int], key: Optional[str] = None
    ) -> Optional[str]:
        """
        Check timers, resize, and input. If dirty, render and return screen buffer.
        If not dirty, return None (NO repainting / NO token emission while idle).
        """
        # 1. Check timer expiration. The trigger is the timer alone: an extra
        # "or not self.jobs" condition made an empty registry refresh on every
        # poll tick, repainting continuously at the polling rate (~10 Hz) exactly
        # when the panel is most idle.
        if self.last_refresh_time is None or now - self.last_refresh_time >= AUTO_REFRESH_INTERVAL:
            self.refresh_jobs(now)

        # 2. Check terminal resize
        if term_size != self.prev_term_size:
            self.prev_term_size = term_size
            self.dirty = True

        # 3. Handle key if present. Any consumed keypress repaints so the user
        # gets immediate feedback; idle ticks pass key=None and stay bounded.
        if key:
            if not self.handle_key(key, now):
                return None
            self.dirty = True

        # 4. If not dirty, do NOT redraw
        if not self.dirty:
            return None

        # 5. Render buffer
        term_cols, term_lines = term_size
        buffer = self._render_buffer(term_cols, term_lines)
        self.dirty = False
        self.redraw_count += 1
        return "\n".join(buffer)

    def _render_buffer(self, term_cols: int, term_lines: int) -> List[str]:
        buffer = ["\x1b[2J\x1b[H"]

        if self.mode == "LIST":
            reg_count = sum(1 for j in self.jobs if j.is_registered)
            disc_count = len(self.jobs) - reg_count
            title_bar = (
                f"ALAS AGENT CONTROL PANEL | Jobs: {len(self.jobs)} ({reg_count} reg, {disc_count} disc) | "
                f"Auto-refresh: {AUTO_REFRESH_INTERVAL:.0f}s"
            )
            help_bar = "[Up/Down or j/k] Navigate | [p] Prompt | [o] Output | [v] Review | [r] Refresh | [q] Quit"

            table_lines = format_table(self.jobs, term_cols, selected_index=self.selected_idx)
            head, rows = table_lines[:2], table_lines[2:]
            avail_rows = max(1, term_lines - 5)
            if self.selected_idx < self.list_offset:
                self.list_offset = self.selected_idx
            elif self.selected_idx >= self.list_offset + avail_rows:
                self.list_offset = self.selected_idx - avail_rows + 1
            self.list_offset = max(0, min(self.list_offset, max(0, len(rows) - avail_rows)))
            if len(rows) > avail_rows:
                title_bar += f" | Rows {self.list_offset + 1}-{min(self.list_offset + avail_rows, len(rows))}"
            buffer.append(f"\x1b[7m{truncate_str(title_bar, term_cols)}\x1b[0m")
            buffer.extend(head)
            buffer.extend(rows[self.list_offset : self.list_offset + avail_rows])
            buffer.append("\n" + f"\x1b[2m{truncate_str(help_bar, term_cols)}\x1b[0m")

        elif self.mode in ("PROMPT", "OUTPUT", "REVIEW"):
            current_job = (
                self.jobs[self.selected_idx]
                if (0 <= self.selected_idx < len(self.jobs))
                else None
            )
            if not current_job:
                self.mode = "LIST"
                return buffer

            _, full_utc, full_local = format_timestamp_tz(current_job.started_at)

            if self.mode == "PROMPT":
                view_title = f"PROMPT DETAIL: {current_job.id}"
                prompt_text, prompt_kind = get_job_prompt(current_job, self.repo_root)
                meta_lines = [
                    f"Job:        {current_job.id} - {current_job.title}",
                    f"Provider:   {current_job.provider}",
                    f"Model:      Req: {current_job.model_requested} | Act: {current_job.model_actual or 'n/a'}",
                    f"Start Time: {full_utc} (Local: {full_local})",
                    f"Prompt:     {current_job.prompt_file or '[none]'} (kind: {prompt_kind})",
                ]
                body_text = prompt_text

            elif self.mode == "OUTPUT":
                view_title = f"OUTPUT DETAIL: {current_job.id}"
                meta_lines = [
                    f"Job:        {current_job.id} - {current_job.title}",
                    f"Status:     {current_job.status} ({current_job.status_detail})",
                    f"Provider:   {current_job.provider}",
                    f"Model:      Req: {current_job.model_requested} | Act: {current_job.model_actual or 'n/a'}",
                    f"Start Time: {full_utc} (Local: {full_local})",
                    f"Output:     {current_job.output_file or '[none]'}",
                    f"Metrics:    Duration: {current_job.elapsed_str} | Turns: {current_job.turns or '-'} | "
                    f"Tokens: {current_job.tokens or '-'} | Cost: {f'${current_job.cost_usd:.2f}' if current_job.cost_usd else '-'}",
                    f"Review:     {current_job.review_summary} (File: {current_job.review_file or '-'})",
                ]
                body_text = (
                    current_job.content_text or f"[No output content in {current_job.output_file}]"
                )

            else:  # REVIEW mode
                view_title = f"REVIEW DETAIL: {current_job.id}"
                review_text, _ = get_job_review(current_job, self.repo_root)
                meta_lines = [
                    f"Job:          {current_job.id} - {current_job.title}",
                    f"Status:       {current_job.status} ({current_job.status_detail})",
                    f"Verification: {current_job.verification} (Summary: {current_job.review_summary})",
                    f"Review File:  {current_job.review_file or '[none]'}",
                    f"Model:        Req: {current_job.model_requested} | Act: {current_job.model_actual or 'n/a'}",
                    f"Start Time:   {full_utc} (Local: {full_local})",
                ]
                body_text = review_text

            buffer.append(f"\x1b[7m{truncate_str(view_title, term_cols)}\x1b[0m")
            for ml in meta_lines:
                buffer.append(truncate_str(ml, term_cols))
            buffer.append("-" * min(term_cols, 100))

            # Full text wrapping: long paragraphs wrap into viewport lines without loss
            content_lines = wrap_text_to_viewport(body_text, term_cols)

            avail_height = max(5, term_lines - len(meta_lines) - 5)
            max_scroll = max(0, len(content_lines) - avail_height)
            self.scroll_offset = max(0, min(self.scroll_offset, max_scroll))

            visible = content_lines[self.scroll_offset : self.scroll_offset + avail_height]
            for vl in visible:
                buffer.append(truncate_str(vl, term_cols))

            footer = f"[j/k or Up/Down] Scroll ({self.scroll_offset}/{max_scroll}) | [b or Esc or q] Back to list"
            buffer.append("\n" + f"\x1b[2m{truncate_str(footer, term_cols)}\x1b[0m")

        return buffer


def run_tui(repo_root: pathlib.Path, include_all: bool = False):
    """Run interactive TUI dashboard with bounded idle redraw and cursor restoration."""
    enable_windows_ansi()
    key_reader = KeyReader()
    key_reader.enable_raw()

    controller = TUIController(repo_root, include_all=include_all)

    # Hide cursor while in TUI
    sys.stdout.write("\x1b[?25l")
    sys.stdout.flush()

    try:
        while True:
            now = time.time()
            term_size = shutil.get_terminal_size((80, 24))

            frame = controller.render_if_dirty(now, term_size)
            if frame is not None:
                sys.stdout.write(frame)
                sys.stdout.flush()

            # Poll for input: calculate time until next 3s auto-refresh
            last_refresh = controller.last_refresh_time or time.time()
            remaining = max(0.02, AUTO_REFRESH_INTERVAL - (time.time() - last_refresh))
            poll_timeout = min(0.1, remaining)

            key = key_reader.read_key(timeout=poll_timeout)
            if key:
                if key in ("q", "\x03") and controller.mode == "LIST":
                    break
                now = time.time()
                frame = controller.render_if_dirty(now, term_size, key=key)
                if frame is not None:
                    sys.stdout.write(frame)
                    sys.stdout.flush()
    except KeyboardInterrupt:
        pass
    finally:
        key_reader.restore()
        # Restore cursor and normal text attributes on exit
        sys.stdout.write("\x1b[?25h\x1b[0m\n")
        sys.stdout.flush()


# -----------------------------------------------------------------------------
# Main Entry Point & CLI
# -----------------------------------------------------------------------------


def find_repo_root() -> pathlib.Path:
    """Derive repo root from script location or current directory."""
    script_path = pathlib.Path(__file__).resolve()
    candidate = script_path.parent.parent
    if (candidate / ".agent").exists() or (candidate / "Cargo.toml").exists():
        return candidate
    cwd = pathlib.Path.cwd()
    if (cwd / ".agent").exists():
        return cwd
    return candidate


def main():
    enable_windows_ansi()
    repo_root = find_repo_root()

    parser = argparse.ArgumentParser(
        description="ALAS Terminal Agent Control Panel",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )

    subparsers = parser.add_subparsers(dest="subcommand", help="Subcommand to run")

    reg_parser = subparsers.add_parser("register", help="Register or update an agent job")
    reg_parser.add_argument("--id", required=True, help="Job unique identifier")
    reg_parser.add_argument("--title", required=True, help="Descriptive job title")
    reg_parser.add_argument("--provider", required=True, help="Agent provider (e.g. Gemini CLI, Claude Code)")
    reg_parser.add_argument("--model", required=True, help="Agent model name")
    reg_parser.add_argument("--prompt-file", required=True, help="Explicit prompt file path")
    reg_parser.add_argument("--output-file", required=True, help="Report output file path")
    reg_parser.add_argument("--pid", type=int, default=None, help="Process ID of running agent")
    reg_parser.add_argument("--started-at", default=None, help="Start ISO timestamp")
    reg_parser.add_argument("--review-file", default=None, help="Review or verification file path")
    reg_parser.add_argument("--prompt-kind", default="exact", help="Prompt kind (default: exact)")
    reg_parser.add_argument("--verification", default="pending", help="Verification status")

    parser.add_argument("--once", action="store_true", help="Print table once and exit")
    parser.add_argument("--json", action="store_true", help="Output all jobs as JSON and exit")
    parser.add_argument("--all", action="store_true", help="Show all discovered reports instead of newest 80")
    parser.add_argument("--repo", default=None, help="Explicit repository root directory")

    args = parser.parse_args()

    if args.repo:
        repo_root = pathlib.Path(args.repo).resolve()

    if args.subcommand == "register":
        try:
            atomic_register_job(
                repo_root=repo_root,
                job_id=args.id,
                title=args.title,
                provider=args.provider,
                model=args.model,
                prompt_file=args.prompt_file,
                output_file=args.output_file,
                pid=args.pid,
                started_at=args.started_at,
                review_file=args.review_file,
                prompt_kind=args.prompt_kind,
                verification=args.verification,
            )
        except RuntimeError as ex:
            sys.stderr.write(f"ERROR: {ex}\n")
            sys.exit(2)
        print(f"Successfully registered job '{args.id}' in {REGISTRY_REL_PATH.as_posix()}")
        return

    if args.json:
        jobs = build_job_list(repo_root, include_all=args.all)
        out = [j.to_dict() for j in jobs]
        print(json.dumps(out, indent=2, ensure_ascii=False))
        return

    if args.once or not sys.stdin.isatty():
        term_cols, _ = shutil.get_terminal_size((80, 24))
        jobs = build_job_list(repo_root, include_all=args.all)
        table_lines = format_table(jobs, term_cols, selected_index=-1)
        for line in table_lines:
            print(line)
        return

    run_tui(repo_root, include_all=args.all)


if __name__ == "__main__":
    main()
