#!/usr/bin/env python3
"""
Unit tests for ALAS Terminal Agent Control Panel.
Standard library only (unittest).
Covers:
  - Report parser: caps (Claude budget cap vs quota), empty SUCCESS, error, partial JSON streaming
  - Provider detection via JSON semantics with generic filenames
  - Model truthfulness: requested model vs reported actual model (Opus vs sonnet-5, historical Gemini)
  - Timezone formatting: explicit UTC and local timezone
  - Review report loader: Markdown and HTML stripping for review detail view
  - Registry roundtrip: spaces in titles/paths, row preservation, atomic update
  - Sanitization: ANSI colors, cursor codes, OSC titles, control characters, utf-8
  - PID-reuse guard: mocked Windows ctypes & Linux /proc start identity verification
  - Prompt security: blocked credentials/auth transcripts, historical missing prompt
  - Real OS PID identity verification on host platform
  - PTY feedback: 80-column responsive table layout (never exceeds terminal width)
  - PTY feedback: 2000-character single-line prompt full wrapping & scrollability
  - PTY feedback: Bounded idle redraw (no busy repaint loop, 3s refresh or input/resize only)
"""

import json
import os
import pathlib
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch

# Ensure tools directory is on path
tools_dir = pathlib.Path(__file__).parent.resolve()
sys.path.insert(0, str(tools_dir))

import agent_control  # noqa: E402


class TestReportParser(unittest.TestCase):
    """Tests for report parsing, Claude budget caps, Gemini empty SUCCESS/denied actions, and partial JSON."""

    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.temp_dir.name)

    def tearDown(self):
        self.temp_dir.cleanup()

    def test_claude_budget_cap_reached(self):
        """Claude error_max_budget_usd is invocation cap NOT quota."""
        report_data = {
            "duration_api_ms": 910637,
            "stop_reason": "tool_use",
            "session_id": "test-session-123",
            "total_cost_usd": 4.378,
            "terminal_reason": "budget_exhausted",
            "is_error": True,
            "num_turns": 37,
            "subtype": "error_max_budget_usd",
            "errors": ["Reached maximum budget ($4)"],
        }
        res = agent_control.evaluate_report_data(report_data, "claude-formula-review.json")
        self.assertEqual(res["status"], "CAP_REACHED")
        detail = res["detail"].lower()
        self.assertIn("invocation", detail)
        self.assertIn("cap reached", detail)
        self.assertIn("per-job", detail)
        self.assertIn("not account quota", detail)
        self.assertNotIn("quota", detail.replace("not account quota", ""))
        self.assertEqual(res["turns"], 37)
        self.assertAlmostEqual(res["cost_usd"], 4.378, places=2)

    def test_claude_generic_error(self):
        """Claude API or tool error should be FAILED."""
        report_data = {
            "session_id": "test-sess-456",
            "is_error": True,
            "subtype": "api_error",
            "errors": ["Network timeout during tool call"],
            "num_turns": 5,
        }
        res = agent_control.evaluate_report_data(report_data, "claude-test.json")
        self.assertEqual(res["status"], "FAILED")
        self.assertIn("Network timeout", res["detail"])

    def test_claude_completed_success(self):
        """Claude normal completion."""
        report_data = {
            "session_id": "test-sess-789",
            "is_error": False,
            "subtype": "success",
            "terminal_reason": "completed",
            "result": "Review completed successfully. Findings: None.",
            "num_turns": 46,
            "duration_ms": 454027,
            "total_cost_usd": 1.35,
        }
        res = agent_control.evaluate_report_data(report_data, "claude-a380-review.json")
        self.assertEqual(res["status"], "COMPLETED")
        self.assertIn("Review completed", res["content_text"])
        self.assertAlmostEqual(res["cost_usd"], 1.35, places=2)
        self.assertAlmostEqual(res["duration_sec"], 454.027, places=2)

    def test_gemini_empty_success_denied_actions(self):
        """is_error/denied_actions/empty Gemini SUCCESS is NOT completed work."""
        report_data = {
            "conversation_id": "test-gemini-conv",
            "status": "SUCCESS",
            "response": "",
            "duration_seconds": 4.5,
            "num_turns": 1,
            "denied_actions": [{"action": "read_file", "display_name": "ListDir"}],
        }
        res = agent_control.evaluate_report_data(report_data, "gemini-a380-gear-audit.json")
        self.assertEqual(res["status"], "DENIED")
        self.assertIn("ListDir", res["detail"])

    def test_gemini_empty_success_without_denials(self):
        """Gemini report with SUCCESS status but empty response text is INCOMPLETE."""
        report_data = {
            "conversation_id": "test-empty-gemini",
            "status": "SUCCESS",
            "response": "   ",
            "duration_seconds": 2.1,
            "num_turns": 1,
            "denied_actions": [],
        }
        res = agent_control.evaluate_report_data(report_data, "gemini-empty.json")
        self.assertEqual(res["status"], "INCOMPLETE")
        self.assertIn("Empty SUCCESS", res["detail"])

    def test_gemini_completed_with_response(self):
        """Gemini report with SUCCESS status and non-empty response is COMPLETED."""
        report_data = {
            "conversation_id": "test-good-gemini",
            "status": "SUCCESS",
            "response": "### Implementation Summary\nAll tasks verified.",
            "duration_seconds": 78.9,
            "num_turns": 1,
            "usage": {"total_tokens": 142316},
            "denied_actions": [],
        }
        res = agent_control.evaluate_report_data(report_data, "gemini-c1-lints.json")
        self.assertEqual(res["status"], "COMPLETED")
        self.assertEqual(res["tokens"], 142316)
        self.assertIn("Implementation Summary", res["content_text"])

    def test_provider_recognition_with_generic_filenames(self):
        """Provider is accurately recognized by JSON content semantics even with generic filenames."""
        claude_generic = {
            "session_id": "sess-generic-123",
            "terminal_reason": "completed",
            "result": "Claude generic output text",
            "total_cost_usd": 0.50,
        }
        res_claude = agent_control.evaluate_report_data(claude_generic, "report.json")
        self.assertEqual(res_claude["status"], "COMPLETED")
        self.assertIn("Claude generic output", res_claude["content_text"])

        gemini_generic = {
            "conversation_id": "conv-generic-456",
            "status": "SUCCESS",
            "response": "Gemini generic output text",
            "duration_seconds": 10.0,
        }
        res_gemini = agent_control.evaluate_report_data(gemini_generic, "output.json")
        self.assertEqual(res_gemini["status"], "COMPLETED")
        self.assertIn("Gemini generic output", res_gemini["content_text"])

    def test_partial_json_recovery_during_active_write(self):
        """Streaming or truncated JSON file does not crash and parses available fields."""
        partial_json_str = (
            '{"conversation_id": "stream-conv-99", '
            '"status": "SUCCESS", '
            '"duration_seconds": 12.5, '
            '"response": "Partial output in progress... '
        )
        report_file = self.root / "gemini-stream.json"
        report_file.write_text(partial_json_str, encoding="utf-8")

        parsed = agent_control.parse_report_file(report_file)
        self.assertTrue(parsed.get("_is_partial"))
        self.assertEqual(parsed.get("status"), "SUCCESS")
        self.assertEqual(parsed.get("conversation_id"), "stream-conv-99")
        self.assertAlmostEqual(parsed.get("duration_seconds"), 12.5)

        eval_res = agent_control.evaluate_report_data(parsed, "gemini-stream.json")
        self.assertEqual(eval_res["status"], "WRITING")
        self.assertIn("Partial output in progress", eval_res["content_text"])

    def test_empty_zero_byte_report(self):
        """Zero-byte file is safely handled as empty/pending."""
        empty_file = self.root / "gemini-agent-control-build.json"
        empty_file.write_text("", encoding="utf-8")

        parsed = agent_control.parse_report_file(empty_file)
        self.assertTrue(parsed.get("is_empty"))

        eval_res = agent_control.evaluate_report_data(parsed, empty_file.name)
        self.assertEqual(eval_res["status"], "PENDING")


class TestModelTruthfulness(unittest.TestCase):
    """Tests making requested model vs reported actual model explicit."""

    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.temp_dir.name)
        (self.root / ".agent/reports").mkdir(parents=True, exist_ok=True)

    def tearDown(self):
        self.temp_dir.cleanup()

    def test_registered_opus_does_not_mask_actual_sonnet(self):
        """Registered 'Opus' should not mask modelUsage 'claude-sonnet-5'."""
        report_data = {
            "session_id": "opus-test-id",
            "terminal_reason": "completed",
            "subtype": "success",
            "result": "Opus requested audit completed.",
            "modelUsage": {
                "claude-haiku-4-5-20251001": {"outputTokens": 100, "costUSD": 0.001},
                "claude-sonnet-5": {"outputTokens": 40000, "costUSD": 1.35},
            },
        }
        report_path = self.root / ".agent/reports/claude-opus-test.json"
        report_path.write_text(json.dumps(report_data), encoding="utf-8")

        job = agent_control.Job(
            job_id="opus-audit",
            title="Audit",
            provider="Claude Code",
            model="Opus",
            output_file=".agent/reports/claude-opus-test.json",
            is_registered=True,
        )
        job.evaluate(self.root)

        self.assertEqual(job.model_requested, "Opus")
        self.assertEqual(job.model_actual, "claude-sonnet-5")
        self.assertIn("Opus", job.model_display)
        self.assertIn("sonnet-5", job.model_display)

    def test_historical_gemini_model_labeled_truthfully(self):
        """Unknown historical Gemini reports must be labeled truthfully as unrecorded/registry only."""
        report_data = {
            "conversation_id": "gem-hist-id",
            "status": "SUCCESS",
            "response": "Historical Gemini result.",
        }
        report_path = self.root / ".agent/reports/gemini-hist.json"
        report_path.write_text(json.dumps(report_data), encoding="utf-8")

        job = agent_control.Job(
            job_id="gemini-hist",
            title="Historical Gemini",
            provider="Gemini CLI",
            model="unregistered",
            output_file=".agent/reports/gemini-hist.json",
            is_registered=False,
        )
        job.evaluate(self.root)

        self.assertEqual(job.model_requested, "unregistered (registry only)")
        self.assertEqual(job.model_actual, "not reported in Gemini report")
        self.assertEqual(job.model_display, "unrecorded")


class TestTimezoneAndReviewPanel(unittest.TestCase):
    """Tests explicit UTC/local timezone formatting and review panel loader."""

    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.temp_dir.name)
        (self.root / ".agent/reports").mkdir(parents=True, exist_ok=True)

    def tearDown(self):
        self.temp_dir.cleanup()

    def test_explicit_utc_and_local_timezone(self):
        """Timestamp formatting provides both UTC and local timezone representations."""
        iso_str = "2026-09-09T15:09:25.117475Z"
        compact_utc, full_utc, full_local = agent_control.format_timestamp_tz(iso_str)
        self.assertIn("UTC", compact_utc)
        self.assertIn("15:09:25", compact_utc)
        self.assertIn("2026-09-09", full_utc)
        self.assertIn("UTC", full_utc)
        self.assertIn("2026-09-09", full_local)

    def test_review_report_loader_markdown(self):
        """Review report loader loads Markdown review files cleanly."""
        rev_path = self.root / ".agent/reports/test-rev.md"
        rev_path.write_text(
            "# Review Verdict\n\n**Approved:** All acceptance criteria pass.", encoding="utf-8"
        )

        job = agent_control.Job(
            job_id="test-job",
            title="Job",
            provider="Gemini CLI",
            model="flash",
            review_file=".agent/reports/test-rev.md",
        )
        text, status = agent_control.get_job_review(job, self.root)
        self.assertEqual(status, "reviewed")
        self.assertIn("Review Verdict", text)
        self.assertIn("Approved", text)

    def test_review_report_loader_html_stripped(self):
        """Review report loader strips HTML tags for clean terminal viewing."""
        rev_path = self.root / ".agent/reports/test-rev.html"
        rev_path.write_text(
            "<html><body><h1>Audit Report</h1><p>Verdict: <b>Rejected</b></p></body></html>",
            encoding="utf-8",
        )

        job = agent_control.Job(
            job_id="test-html-job",
            title="HTML Job",
            provider="Claude Code",
            model="Opus",
            review_file=".agent/reports/test-rev.html",
        )
        text, status = agent_control.get_job_review(job, self.root)
        self.assertEqual(status, "reviewed")
        self.assertNotIn("<html>", text)
        self.assertNotIn("<b>", text)
        self.assertIn("Audit Report", text)
        self.assertIn("Verdict: Rejected", text)

    def test_review_report_missing_file(self):
        """Missing review file returns descriptive status without exception."""
        job = agent_control.Job(
            job_id="test-missing-rev",
            title="Missing Rev Job",
            provider="Gemini CLI",
            model="flash",
            review_file=".agent/reports/nonexistent.md",
            verification="rejected",
        )
        text, status = agent_control.get_job_review(job, self.root)
        self.assertEqual(status, "missing_file")
        self.assertIn("not found on disk", text)
        self.assertIn("rejected", text)


class TestRegistryRoundtrip(unittest.TestCase):
    """Tests for jobs.json registration, preservation of rows, and space handling."""

    def setUp(self):
        # ignore_cleanup_errors: on Windows an unlinked lock file stays "delete
        # pending" until the last handle closes, so rmtree can transiently see a
        # non-empty directory. This tolerates that teardown race only; every
        # product assertion below is unchanged.
        self.temp_dir = tempfile.TemporaryDirectory(ignore_cleanup_errors=True)
        self.root = pathlib.Path(self.temp_dir.name)

    def tearDown(self):
        self.temp_dir.cleanup()

    def test_atomic_register_roundtrip_with_spaces(self):
        """Register a job with spaces in arguments, titles, and paths without corruption."""
        agent_control.atomic_register_job(
            repo_root=self.root,
            job_id="job with spaces",
            title="Title With Multiple Spaces & Special Chars (v1.0)",
            provider="Gemini CLI Extended",
            model="gemini-3.8-flash-high (preview)",
            prompt_file=".agent/control/prompts/prompt with spaces.txt",
            output_file=".agent/reports/output with spaces.json",
            pid=99999,
            started_at="2026-09-09T15:00:00.000Z",
            review_file=".agent/reports/review file with spaces.md",
            verification="audit_completed_release_blocked",
        )

        reg_file = self.root / agent_control.REGISTRY_REL_PATH
        self.assertTrue(reg_file.exists())

        jobs = agent_control.load_registry(self.root)
        self.assertEqual(len(jobs), 1)
        job = jobs[0]
        self.assertEqual(job["id"], "job with spaces")
        self.assertEqual(job["title"], "Title With Multiple Spaces & Special Chars (v1.0)")
        self.assertEqual(job["provider"], "Gemini CLI Extended")
        self.assertEqual(job["model"], "gemini-3.8-flash-high (preview)")
        self.assertEqual(job["prompt_file"], ".agent/control/prompts/prompt with spaces.txt")
        self.assertEqual(job["output_file"], ".agent/reports/output with spaces.json")
        self.assertEqual(job["review_file"], ".agent/reports/review file with spaces.md")
        self.assertEqual(job["verification"], "audit_completed_release_blocked")
        self.assertEqual(job["pid"], 99999)

    def test_register_preserves_existing_jobs(self):
        """Registering a new job must preserve all existing jobs and rows."""
        agent_control.atomic_register_job(
            repo_root=self.root,
            job_id="job-1",
            title="First Job",
            provider="Claude Code",
            model="Opus",
            prompt_file=".agent/control/prompts/job1.txt",
            output_file=".agent/reports/claude-job1.json",
        )

        agent_control.atomic_register_job(
            repo_root=self.root,
            job_id="job-2",
            title="Second Job",
            provider="Gemini CLI",
            model="gemini-3.8-flash-high",
            prompt_file=".agent/control/prompts/job2.txt",
            output_file=".agent/reports/gemini-job2.json",
        )

        jobs = agent_control.load_registry(self.root)
        self.assertEqual(len(jobs), 2)
        self.assertEqual(jobs[0]["id"], "job-1")
        self.assertEqual(jobs[1]["id"], "job-2")

    def test_register_updates_existing_job_in_place(self):
        """Registering an existing ID updates fields while preserving unoverridden fields."""
        agent_control.atomic_register_job(
            repo_root=self.root,
            job_id="update-job",
            title="Original Title",
            provider="Claude Code",
            model="Opus",
            prompt_file="prompt.txt",
            output_file="out.json",
            verification="pending",
        )

        agent_control.atomic_register_job(
            repo_root=self.root,
            job_id="update-job",
            title="Updated Title",
            provider="Claude Code",
            model="Opus",
            prompt_file="prompt.txt",
            output_file="out.json",
            verification="accepted",
        )

        jobs = agent_control.load_registry(self.root)
        self.assertEqual(len(jobs), 1)
        self.assertEqual(jobs[0]["title"], "Updated Title")
        self.assertEqual(jobs[0]["verification"], "accepted")

    def test_update_does_not_rewrite_started_at_or_process_start(self):
        """Metadata-only updates must preserve recorded start identity for still-running PIDs."""
        agent_control.atomic_register_job(
            repo_root=self.root,
            job_id="run-job",
            title="Running Job",
            provider="Claude Code",
            model="Opus",
            prompt_file="p.txt",
            output_file="o.json",
            pid=12345,
            started_at="2026-09-09T10:00:00.000Z",
            verification="pending",
        )
        before = agent_control.load_registry(self.root)[0]
        self.assertEqual(before["started_at"], "2026-09-09T10:00:00.000Z")
        self.assertEqual(before["process_start"], "2026-09-09T10:00:00.000Z")

        agent_control.atomic_register_job(
            repo_root=self.root,
            job_id="run-job",
            title="Running Job (reviewed)",
            provider="Claude Code",
            model="Opus",
            prompt_file="p.txt",
            output_file="o.json",
            pid=12345,
            verification="accepted",
        )
        after = agent_control.load_registry(self.root)[0]
        self.assertEqual(after["verification"], "accepted")
        self.assertEqual(after["started_at"], "2026-09-09T10:00:00.000Z")
        self.assertEqual(after["process_start"], "2026-09-09T10:00:00.000Z")

    def test_update_with_different_pid_refreshes_identity(self):
        """Registering the same ID with a different PID indicates a relaunch: refresh identity."""
        agent_control.atomic_register_job(
            repo_root=self.root,
            job_id="relaunch-job",
            title="Relaunch Job",
            provider="Claude Code",
            model="Opus",
            prompt_file="p.txt",
            output_file="o.json",
            pid=11111,
            started_at="2026-09-09T10:00:00.000Z",
        )
        agent_control.atomic_register_job(
            repo_root=self.root,
            job_id="relaunch-job",
            title="Relaunch Job",
            provider="Claude Code",
            model="Opus",
            prompt_file="p.txt",
            output_file="o.json",
            pid=22222,
        )
        after = agent_control.load_registry(self.root)[0]
        self.assertEqual(after["pid"], 22222)
        self.assertNotEqual(after["started_at"], "2026-09-09T10:00:00.000Z")

    def test_concurrent_registration_does_not_lose_rows(self):
        """Concurrent registration via multiple threads must not lose rows."""
        n_workers = 12
        barrier = threading.Barrier(n_workers)

        def worker(idx: int):
            barrier.wait()
            agent_control.atomic_register_job(
                repo_root=self.root,
                job_id=f"thread-job-{idx}",
                title=f"Worker {idx}",
                provider="Test",
                model="test",
                prompt_file="p.txt",
                output_file=f"o_{idx}.json",
            )

        threads = [threading.Thread(target=worker, args=(i,)) for i in range(n_workers)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        jobs = agent_control.load_registry(self.root)
        self.assertEqual(len(jobs), n_workers)
        registered_ids = {j["id"] for j in jobs}
        self.assertEqual(registered_ids, {f"thread-job-{i}" for i in range(n_workers)})

    def test_corrupt_registry_raises_runtime_error_and_preserves_file(self):
        """Unparseable jobs.json must NOT be silently replaced with an empty template."""
        reg_path = self.root / agent_control.REGISTRY_REL_PATH
        reg_path.parent.mkdir(parents=True, exist_ok=True)
        reg_path.write_text("{corrupt json bytes...", encoding="utf-8")

        with self.assertRaises(RuntimeError):
            agent_control.atomic_register_job(
                repo_root=self.root,
                job_id="new-job",
                title="New",
                provider="P",
                model="M",
                prompt_file="p.txt",
                output_file="o.json",
            )
        self.assertEqual(reg_path.read_text(encoding="utf-8"), "{corrupt json bytes...")


class TestSanitization(unittest.TestCase):
    """Tests for stripping ANSI escapes, OSC sequences, and control codes."""

    def test_strip_ansi_color_and_formatting(self):
        raw = "\x1b[31;1mError: Severe Failure\x1b[0m normal text \x1b[32mOK\x1b[m"
        clean = agent_control.sanitize_text(raw)
        self.assertEqual(clean, "Error: Severe Failure normal text OK")

    def test_strip_ansi_cursor_and_screen_clear(self):
        raw = "\x1b[2J\x1b[HWelcome to the terminal\x1b[1;20H"
        clean = agent_control.sanitize_text(raw)
        self.assertEqual(clean, "Welcome to the terminal")

    def test_strip_osc_window_titles_and_hyperlinks(self):
        raw = (
            "\x1b]0;Malicious Window Title\x07Body text \x1b]8;;http://evil.com\x1b\\Link\x1b]8;;\x1b\\"
        )
        clean = agent_control.sanitize_text(raw)
        self.assertEqual(clean, "Body text Link")

    def test_strip_dangerous_control_characters(self):
        raw = "Hello\x00\x07\x08 World\x7f!"
        clean = agent_control.sanitize_text(raw)
        self.assertEqual(clean, "Hello World!")

    def test_preserves_tabs_and_newlines_and_utf8(self):
        raw = "Line 1\n\tIndented with accents: avión, diseño, 35.5°\r\nLine 2"
        clean = agent_control.sanitize_text(raw)
        self.assertEqual(clean, raw)


class TestPIDReuseGuard(unittest.TestCase):
    """Tests for PID liveness and start identity verification to prevent PID reuse false running."""

    @patch("agent_control.get_os_process_identity")
    def test_pid_verified_running(self, mock_get_ident):
        now_epoch = 1757430000.0
        mock_get_ident.return_value = (True, now_epoch, None)

        recorded_start = now_epoch - 2.0
        res = agent_control.verify_pid_liveness(12345, recorded_start)
        self.assertTrue(res["is_verified_running"])
        self.assertEqual(res["status"], "RUNNING")
        self.assertIn("verified running", res["detail"])

    @patch("agent_control.get_os_process_identity")
    def test_pid_reuse_detected_different_start_time(self, mock_get_ident):
        mock_get_ident.return_value = (True, 1757437200.0, None)

        recorded_start = 1757430000.0
        res = agent_control.verify_pid_liveness(12345, recorded_start)
        self.assertFalse(res["is_verified_running"])
        self.assertEqual(res["status"], "REUSED_PID")
        self.assertIn("reused by a different process", res["detail"])

    @patch("agent_control.get_os_process_identity")
    def test_pid_dead_inactive(self, mock_get_ident):
        mock_get_ident.return_value = (False, None, "Process cannot be opened")
        res = agent_control.verify_pid_liveness(12345, 1757430000.0)
        self.assertFalse(res["is_verified_running"])
        self.assertEqual(res["status"], "DEAD")

    @patch("agent_control.get_os_process_identity")
    def test_pid_active_without_recorded_start_is_unknown(self, mock_get_ident):
        mock_get_ident.return_value = (True, 1757430000.0, None)
        res = agent_control.verify_pid_liveness(12345, None)
        self.assertFalse(res["is_verified_running"])
        self.assertEqual(res["status"], "UNKNOWN")
        self.assertIn("unverified", res["detail"])

    def test_no_pid_returns_no_pid(self):
        res = agent_control.verify_pid_liveness(None, None)
        self.assertFalse(res["is_verified_running"])
        self.assertEqual(res["status"], "NO_PID")


class TestRealOSProcessIdentity(unittest.TestCase):
    """Directly test get_os_process_identity without patching."""

    def test_current_process_is_reported_alive_with_start_time(self):
        current_pid = os.getpid()
        is_alive, epoch, err = agent_control.get_os_process_identity(current_pid)
        self.assertTrue(is_alive)
        self.assertIsNone(err)
        self.assertIsNotNone(epoch)
        self.assertGreater(epoch, 1700000000.0)

    def test_current_process_verifies_running_with_own_start(self):
        current_pid = os.getpid()
        _, epoch, _ = agent_control.get_os_process_identity(current_pid)
        res = agent_control.verify_pid_liveness(current_pid, epoch)
        self.assertTrue(res["is_verified_running"])
        self.assertEqual(res["status"], "RUNNING")

    def test_current_process_catches_shifted_start_identity(self):
        current_pid = os.getpid()
        _, epoch, _ = agent_control.get_os_process_identity(current_pid)
        res = agent_control.verify_pid_liveness(current_pid, epoch - 3600.0)
        self.assertFalse(res["is_verified_running"])
        self.assertEqual(res["status"], "REUSED_PID")


class TestPromptSecurity(unittest.TestCase):
    """Tests for prompt security guard: explicit registry files only, no credential reads."""

    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.temp_dir.name)

    def tearDown(self):
        self.temp_dir.cleanup()

    def test_historical_missing_prompt(self):
        job = agent_control.Job(
            job_id="historical-job",
            title="Historical Job",
            provider="Claude Code",
            model="Opus",
            prompt_file=None,
        )
        text, kind = agent_control.get_job_prompt(job, self.root)
        self.assertEqual(kind, "not captured")
        self.assertIn("not captured", text)

    def test_security_blocks_auth_and_credential_transcripts(self):
        sensitive_paths = [
            ".agent/auth.json",
            "configs/.env",
            "tokens/session_token.txt",
            ".agent/credentials.txt",
            "id_rsa",
        ]
        for p in sensitive_paths:
            job = agent_control.Job(
                job_id="evil-job",
                title="Evil Job",
                provider="Claude Code",
                model="Opus",
                prompt_file=p,
            )
            text, kind = agent_control.get_job_prompt(job, self.root)
            self.assertEqual(kind, "security_blocked")
            self.assertIn("SECURITY BLOCKED", text)

    def test_valid_explicit_prompt_file(self):
        prompt_dir = self.root / ".agent/control/prompts"
        prompt_dir.mkdir(parents=True, exist_ok=True)
        pfile = prompt_dir / "valid-prompt.txt"
        pfile.write_text(
            "Audit aircraft parity with strict physics.\x1b[32mGreen\x1b[0m", encoding="utf-8"
        )

        job = agent_control.Job(
            job_id="valid-job",
            title="Valid Job",
            provider="Gemini CLI",
            model="gemini-3.8-flash-high",
            prompt_file=".agent/control/prompts/valid-prompt.txt",
            prompt_kind="exact",
        )
        text, kind = agent_control.get_job_prompt(job, self.root)
        self.assertEqual(kind, "exact")
        self.assertEqual(text, "Audit aircraft parity with strict physics.Green")


class TestFormattingAndEvaluation(unittest.TestCase):
    """Tests for table formatting and elapsed calculations."""

    def test_format_duration(self):
        self.assertEqual(agent_control.format_duration(5.2), "5.2s")
        self.assertEqual(agent_control.format_duration(65.0), "1m 05s")
        self.assertEqual(agent_control.format_duration(3665.0), "1h 01m")
        self.assertEqual(agent_control.format_duration(-1.0), "-")

    def test_format_table_renders_cleanly(self):
        job = agent_control.Job(
            job_id="test-job",
            title="Test Aircraft Mission",
            provider="Gemini CLI",
            model="gemini-3.8-flash-high",
        )
        job.status = "COMPLETED"
        job.elapsed_str = "4m 12s"
        job.output_summary = "142k tok"
        job.review_summary = "accepted"

        lines = agent_control.format_table([job], term_width=100, selected_index=0)
        self.assertGreater(len(lines), 2)
        self.assertIn("TITLE / ID", lines[0])
        self.assertIn("STATUS", lines[0])
        self.assertIn("> ", lines[2])
        self.assertIn("test-job", lines[2])
        self.assertIn("COMPLETED", lines[2])


class TestEscapeSanitization(unittest.TestCase):
    """Untrusted agent-emitted text must be sanitized everywhere."""

    def test_claude_error_detail_is_sanitized(self):
        report = {
            "session_id": "s",
            "is_error": True,
            "errors": ["\x1b[2J\x1b]0;PWNED\x07boom\x1b[31m"],
        }
        res = agent_control.evaluate_report_data(report, "claude-x.json")
        self.assertEqual(res["status"], "FAILED")
        self.assertNotIn("\x1b", res["detail"])
        self.assertIn("boom", res["detail"])

    def test_gemini_denied_action_name_is_sanitized(self):
        report = {
            "conversation_id": "c",
            "status": "SUCCESS",
            "response": "ok",
            "denied_actions": [{"display_name": "\x1b[5mBLINK\x1b[0m"}],
        }
        res = agent_control.evaluate_report_data(report, "gemini-x.json")
        self.assertEqual(res["status"], "DENIED")
        self.assertNotIn("\x1b", res["detail"])
        self.assertIn("BLINK", res["detail"])

    def test_errors_field_as_bare_string_still_detects_budget_cap(self):
        report = {"session_id": "s", "is_error": True, "errors": "Reached maximum budget ($4)"}
        res = agent_control.evaluate_report_data(report, "claude-x.json")
        self.assertEqual(res["status"], "CAP_REACHED")

    def test_table_cell_strips_escapes_and_keeps_column_width(self):
        cell = agent_control.truncate_str("\x1b[31mRED\x1b[0m", 10)
        self.assertEqual(cell, "RED".ljust(10))
        multiline = agent_control.truncate_str("line1\nline2", 12)
        self.assertNotIn("\n", multiline)


class TestDiscoveredJobStartTime(unittest.TestCase):
    """Discovered reports have no recorded start; mtime is the finish time."""

    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.temp_dir.name)
        (self.root / ".agent/reports").mkdir(parents=True, exist_ok=True)

    def tearDown(self):
        self.temp_dir.cleanup()

    def test_start_time_is_finish_minus_duration(self):
        report = {
            "conversation_id": "c",
            "status": "SUCCESS",
            "response": "done",
            "duration_seconds": 660.0,
        }
        path = self.root / ".agent/reports/gemini-disc.json"
        path.write_text(json.dumps(report), encoding="utf-8")

        jobs = agent_control.build_job_list(self.root)
        job = next(j for j in jobs if j.output_file.endswith("gemini-disc.json"))

        finish = path.stat().st_mtime
        start = agent_control.parse_start_to_epoch(job.started_at)
        self.assertAlmostEqual(start, finish - 660.0, delta=2.0)
        self.assertIn("mtime - duration", job.started_at_source)
        self.assertIn("started_at_source", job.to_dict())

    def test_start_time_without_duration_is_labeled_as_finish_time(self):
        path = self.root / ".agent/reports/gemini-nodur.json"
        path.write_text(
            json.dumps({"conversation_id": "c", "status": "SUCCESS", "response": "x"}),
            encoding="utf-8",
        )

        jobs = agent_control.build_job_list(self.root)
        job = next(j for j in jobs if j.output_file.endswith("gemini-nodur.json"))
        self.assertIn("finish time", job.started_at_source)


# =============================================================================
# PTY Smoke Tests: Responsive 80-Column Layout, Full Wrapping, Bounded Redraw
# =============================================================================


class TestResponsiveTable80Columns(unittest.TestCase):
    """
    PTY Feedback 1:
    The table must adapt cleanly to an 80-column viewport.
    Every line (header, separator, data rows) must strictly satisfy len(line) <= term_width.
    Never force a minimum width that causes line wrapping on an 80-column terminal.
    """

    def setUp(self):
        self.jobs = [
            agent_control.Job(
                job_id="agent-control-build",
                title="Build terminal agent control panel with responsive layout",
                provider="Gemini CLI",
                model="gemini-3.8-flash-high",
                verification="pending",
                started_at="2026-09-09T15:09:25.117475Z",
            ),
            agent_control.Job(
                job_id="opus-overhaul-audit",
                title="Full release audit against original objectives and physics",
                provider="Claude Code",
                model="Opus",
                verification="audit_completed_release_blocked",
                started_at="2026-09-09T15:18:17.794469Z",
            ),
            agent_control.Job(
                job_id="materials-integration-review",
                title="Audit material binding and eight-preset physics in ALAS",
                provider="Claude Code",
                model="Opus",
                verification="rejected",
                started_at="2026-09-09T14:30:00.000Z",
            ),
        ]
        for j in self.jobs:
            j.status = "COMPLETED"
            j.elapsed_str = "12m 34s"
            j.output_summary = "142k tok"
            j.review_summary = "accepted"

    def test_table_strictly_fits_80_columns(self):
        """At 80 columns, every line must be <= 80 chars, eliminating PTY line wraps."""
        lines = agent_control.format_table(self.jobs, term_width=80, selected_index=0)
        self.assertGreater(len(lines), 3)
        for i, line in enumerate(lines):
            self.assertLessEqual(
                len(line),
                80,
                f"Line {i} length ({len(line)}) exceeds 80 columns: '{line}'",
            )
        # Verify headers are clean and separator matches
        self.assertEqual(len(lines[0]), len(lines[1]))
        self.assertIn("TITLE / ID", lines[0])
        self.assertIn("STATUS", lines[0])
        self.assertIn("REVIEW", lines[0])

    def test_table_strictly_fits_narrow_viewport(self):
        """At 60 columns (narrow terminal), every line must be <= 60 chars."""
        lines = agent_control.format_table(self.jobs, term_width=60, selected_index=0)
        self.assertGreater(len(lines), 3)
        for i, line in enumerate(lines):
            self.assertLessEqual(
                len(line),
                60,
                f"Line {i} length ({len(line)}) exceeds 60 columns: '{line}'",
            )

    def test_table_expands_with_wide_viewport(self):
        """At 120 columns (wide terminal), full columns including START (UTC) are shown."""
        lines = agent_control.format_table(self.jobs, term_width=120, selected_index=0)
        self.assertGreater(len(lines), 3)
        for i, line in enumerate(lines):
            self.assertLessEqual(len(line), 120)
        self.assertIn("START (UTC)", lines[0])


class TestViewportTextWrapping(unittest.TestCase):
    """
    PTY Feedback 2:
    A 2000-character single-line prompt must not truncate to a single ellipsis line.
    Must wrap into viewport-width lines, preserve logical line breaks, and allow full scrolling.
    """

    def test_wrap_2000_char_single_line_prompt(self):
        """2000-char single-line prompt wraps into multiple lines of length <= 80."""
        # Construct a 2000-character single-line text without newlines
        words = [f"word{i:04d}" for i in range(250)]
        long_prompt = " ".join(words)  # ~2000 chars
        self.assertGreaterEqual(len(long_prompt), 2000)
        self.assertNotIn("\n", long_prompt)

        wrapped = agent_control.wrap_text_to_viewport(long_prompt, width=80)

        # Must have wrapped into at least 25 lines
        self.assertGreaterEqual(len(wrapped), 25)
        # Every line must be <= 80 characters
        for idx, line in enumerate(wrapped):
            self.assertLessEqual(
                len(line), 80, f"Wrapped line {idx} exceeds 80 chars: '{line}'"
            )

        # Content must be completely preserved without data loss
        rejoined = " ".join(line.strip() for line in wrapped if line.strip())
        self.assertEqual(rejoined, long_prompt)

        # Scroll calculation in an 80x24 terminal: avail_height ~ 13
        avail_height = 13
        max_scroll = max(0, len(wrapped) - avail_height)
        self.assertGreater(
            max_scroll,
            10,
            f"Expected max_scroll > 10 for 2000-char text, got {max_scroll}. Cannot be 0/0!",
        )

    def test_wrap_preserves_logical_breaks_and_blank_lines(self):
        """Existing logical line breaks and paragraph separations are preserved."""
        text = "Paragraph 1 line.\n\nParagraph 2 line.\n\nParagraph 3 line."
        wrapped = agent_control.wrap_text_to_viewport(text, width=80)
        self.assertEqual(len(wrapped), 5)
        self.assertEqual(wrapped[0], "Paragraph 1 line.")
        self.assertEqual(wrapped[1], "")
        self.assertEqual(wrapped[2], "Paragraph 2 line.")
        self.assertEqual(wrapped[3], "")
        self.assertEqual(wrapped[4], "Paragraph 3 line.")


class TestBoundedIdleRedraw(unittest.TestCase):
    """
    PTY Feedback 3:
    Screen must NOT repaint continuously while idle.
    Redraw only when dirty, on input, on resize, or on the 3.0s auto-refresh interval.
    In 10 seconds of idle polling, exactly 4 frames are rendered (t=0, t=3, t=6, t=9).
    """

    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.temp_dir.name)
        (self.root / ".agent/control").mkdir(parents=True, exist_ok=True)
        jobs_json = self.root / ".agent/control/jobs.json"
        jobs_json.write_text('{"version": 1, "jobs": []}', encoding="utf-8")

    def tearDown(self):
        self.temp_dir.cleanup()

    def test_idle_polling_does_not_redraw(self):
        """Between refresh intervals, idle polling ticks return None (zero stdout bytes)."""
        controller = agent_control.TUIController(self.root)

        # First frame: dirty is True, renders initial frame
        frame0 = controller.render_if_dirty(now=1000.0, term_size=(80, 24))
        self.assertIsNotNone(frame0)
        self.assertEqual(controller.redraw_count, 1)

        # Next 29 polling ticks over 2.9 seconds: completely idle, must return None!
        for tick in range(1, 30):
            t = 1000.0 + (tick * 0.1)
            frame = controller.render_if_dirty(now=t, term_size=(80, 24), key=None)
            self.assertIsNone(
                frame,
                f"Idle polling tick {tick} at t={t:.1f} emitted an unneeded screen redraw!",
            )
            self.assertEqual(controller.redraw_count, 1)

    def test_auto_refresh_redraws_every_3_seconds(self):
        """Over 10 seconds of simulated idle time, exactly 4 redraws occur (t=0, 3, 6, 9)."""
        controller = agent_control.TUIController(self.root)

        # Simulate 10 seconds with 0.1s input polling ticks (100 ticks total)
        emitted_frames = []
        for tick in range(101):
            t = 1000.0 + (tick * 0.1)
            frame = controller.render_if_dirty(now=t, term_size=(80, 24), key=None)
            if frame is not None:
                emitted_frames.append(t)

        # Expected triggers at t=1000.0 (start), 1003.0 (+3s), 1006.0 (+6s), 1009.0 (+9s)
        self.assertEqual(
            len(emitted_frames),
            4,
            f"Expected exactly 4 redraws in 10s idle time, got {len(emitted_frames)} at times: {emitted_frames}",
        )
        self.assertEqual(controller.redraw_count, 4)

    def test_keypress_triggers_immediate_redraw(self):
        """User input sets dirty=True and triggers an immediate redraw."""
        controller = agent_control.TUIController(self.root)
        controller.render_if_dirty(now=1000.0, term_size=(80, 24))
        self.assertEqual(controller.redraw_count, 1)

        # Press 'j' at t=1000.5 (idle time)
        frame = controller.render_if_dirty(now=1000.5, term_size=(80, 24), key="j")
        self.assertIsNotNone(frame)
        self.assertEqual(controller.redraw_count, 2)

    def test_terminal_resize_triggers_immediate_redraw(self):
        """Terminal resize sets dirty=True and triggers an immediate redraw."""
        controller = agent_control.TUIController(self.root)
        controller.render_if_dirty(now=1000.0, term_size=(80, 24))
        self.assertEqual(controller.redraw_count, 1)

        # Resize to (100, 30) at t=1000.5
        frame = controller.render_if_dirty(now=1000.5, term_size=(100, 30), key=None)
        self.assertIsNotNone(frame)
        self.assertEqual(controller.redraw_count, 2)


if __name__ == "__main__":
    unittest.main()
