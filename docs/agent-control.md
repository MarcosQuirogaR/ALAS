# ALAS Terminal Agent Control Panel

> Maintainer-only tool documentation. This describes an internal developer
> dashboard for supervising AI-assisted coding jobs during development; it is
> not part of ALAS the aircraft-design application and is not needed to
> build, run, or use it.

A lightweight, terminal-based control panel to monitor running agents, inspect exact prompts, stream outputs and metrics, and track process completion separately from review acceptance.

Built entirely using the **Python standard library** (zero third-party dependencies) with a cross-platform Bash launcher supporting **Git Bash on Windows**, **Linux**, and **macOS**.

---

## 1. Quick Start

### Bash Launcher (Git Bash / Linux / macOS)
```bash
# Launch interactive TUI (responsive layout, auto-refresh 3s)
./tools/agent-control.sh

# One-shot readable table output
./tools/agent-control.sh --once

# Machine-readable JSON output (ideal for pipes / non-TTY)
./tools/agent-control.sh --json

# Show all historical reports (bypassing the default newest-80 cap)
./tools/agent-control.sh --all
```

The launcher derives the repository root, handles path conversion under Git Bash/MSYS via `cygpath`, and probes candidate interpreters (`uv run python`, `python3`, `python`, `py -3`) with `-c "pass"` to skip dead Microsoft Store stubs on Windows.

### Windows PowerShell / CMD Alternative
```powershell
# Using uv
uv run python tools/agent_control.py

# Or using standard python
python tools/agent_control.py

# One-shot readable table
uv run python tools/agent_control.py --once

# Non-TTY JSON dump
uv run python tools/agent_control.py --json
```

---

## 2. Interactive TUI Interface

When launched inside a terminal, the dashboard provides a high-density, real-time control interface:

### Keybindings
| Key | Action |
| :--- | :--- |
| `Up` / `Down` or `k` / `j` | Navigate jobs in list view, or scroll lines in detail view |
| `PageUp` / `PageDown` | Jump 10 rows in list view, or scroll page in detail view |
| `p` or `Enter` | Open **Prompt Detail View** for selected job |
| `o` | Open **Output Detail View** for selected job |
| `v` | Open **Review Detail View** for selected job (renders review markdown/HTML reports) |
| `r` | Manually refresh status immediately from disk |
| `q` | Quit control panel (in list view) or return to list (in detail view) |
| `b` / `Esc` / `Backspace` | Return to list view from any detail view |

### Responsive 80-Column Layout
- **No Line Wrapping**: The table dynamically calibrates column allocations based on terminal width. At 80 columns (`80x24` standard PTY), every line is guaranteed to be $\le 80$ characters.
- **Adaptive Column Priorities**: At 80 columns, the secondary `START (UTC)` column is hidden to preserve maximum space for `TITLE / ID`, `PROVIDER`, `MODEL`, `STATUS`, `ELAPSED`, `OUTPUT`, and `REVIEW`. Full timestamp metadata is always accessible in detail views (`p`, `o`, `v`).
- **List Viewport Scrolling**: When jobs exceed available vertical screen space, the list view scrolls smoothly with the cursor selection and displays a `Rows n-m` indicator in the title bar.

### Full Text Wrapping & Scrolling
- Long single-line paragraphs (e.g. 2000-character prompts without line breaks) are wrapped into viewport-width lines while preserving logical line breaks and paragraph spacing.
- Text never truncates into a single line ending in `...` with `Scroll (0/0)`. Users can scroll through the entire text line-by-line across all detail views (`p`, `o`, `v`).

### Bounded Idle Redraw
- Screen repainting is strictly event-driven: redraws occur **only** on initial render, user input, terminal resize, or when the 3.0-second auto-refresh interval expires.
- Zero CPU or terminal token emission when idle. Cursor is hidden during execution and cleanly restored upon exit (`\x1b[?25h`).

---

## 3. Core Capabilities & Domain Integrity

### 1. Zero False-Positives on PID Reuse
Operating systems recycle Process IDs (PIDs) quickly. A stale PID from an old job must **never** be mistakenly reported as `RUNNING`.
- **Windows**: Uses standard library `ctypes` (`kernel32.OpenProcess`, `GetExitCodeProcess`, and `GetProcessTimes`). Converts process creation `FILETIME` to epoch seconds and verifies it matches the recorded `process_start` timestamp within a 15-second tolerance. If exit code is not `STILL_ACTIVE` or creation times differ, it is classified as dead or `REUSED_PID`.
- **Linux**: Queries `/proc/{pid}` and verifies the process creation time (`ctime`) against recorded `process_start`.
- **Unverified PIDs**: If the process start identity cannot be verified or was not recorded, status defaults to `UNKNOWN`, never a false-positive `RUNNING`.

### 2. Truthful Model Labeling: Requested vs. Reported Actual
- **Opus Masking Guard**: If a job was registered requesting `Opus`, but Claude's execution report records in `modelUsage` that `claude-sonnet-5` did the actual work, the dashboard does **not** mask the actual model. The column and detail view explicitly report `Opus -> sonnet-5`.
- **Historical Gemini Reports**: Gemini CLI report JSON does not record model metadata. For discovered historical Gemini reports without a registry entry, the dashboard truthfully labels the model as `unrecorded` / `unregistered (registry only)` rather than guessing.

### 3. Explicit UTC & Local Timezones
- In wide views ($\ge 90$ cols), the table displays **`START (UTC)`** (e.g. `15:09:25 UTC`).
- In all detail panels (`p`, `o`, `v`), timestamps explicitly show both representations:
  `Start Time: 2026-09-09 15:09:25 UTC (Local: 2026-09-09 17:09:25 CEST)`
- In `--json`, discovered jobs report start-time provenance via `started_at_source`: `recorded`, `inferred (report mtime - duration)`, or `inferred (report mtime = finish time)`.

### 4. Review Report Panel (`v` key)
- Pressing `v` opens the dedicated **Review Detail View**, displaying the associated review artifact (`.md` or `.html`).
- HTML tags in audit reports are stripped cleanly for plain-text terminal viewing.
- Displays review verdict and verification state side-by-side with execution metrics.

### 5. Accurate Agent Status Classification
Process completion is evaluated strictly from actual artifacts, distinguishing technical execution from domain review:
- **Claude Invocation Budget Cap**: Claude reports with `subtype: "error_max_budget_usd"` or `terminal_reason: "budget_exhausted"` are classified as **`CAP_REACHED`** with detail `"Invocation budget cap reached (per-job spend cap, not account quota)"`.
- **Gemini Incomplete & Denied Actions**: Gemini reports with `"status": "SUCCESS"` but an empty response string or non-empty `denied_actions` (e.g. attempted directory listing blocked) are classified as **`DENIED`** or **`INCOMPLETE`**, never as completed work.
- **Robust Provider Recognition**: Providers are recognized based on JSON structure/semantics (`session_id`, `modelUsage`, `terminal_reason`, `conversation_id`, `denied_actions`), independent of generic filenames.
- **Partial JSON Streaming Recovery**: When agents are actively writing reports or write is interrupted, the parser extracts available fields via a resilient fallback parser and marks status as `WRITING`, avoiding unhandled JSON syntax crashes.
- **Empty Output Without PID**: If an output file is 0 bytes and no active PID is verified, status is **`PENDING`** or **`UNKNOWN`**.
- **No Fake Progress Percentages**: Progress is reported through concrete turn counts, tokens, costs, and elapsed time rather than arbitrary percentage estimates.

### 6. Strict Prompt Security Policy
- Prompts are read **only** from explicitly registered prompt files (`job.prompt_file`).
- Files containing credentials, tokens, session cookies, `.env`, or `id_rsa` keys are strictly blocked by the security filter.
- Historical discovered reports without registered prompt files explicitly display: `Prompt not captured for this historical job`.
- Read-only control panel: **Never** automatically kills, launches, or transmits data to tasks.

### 7. Text Sanitization & Windows UTF-8
- All displayed text is sanitized using regex filters that strip ANSI color codes (`\x1b[...m`), cursor controls, OSC sequences (`\x1b]...`), and non-printable control characters while preserving formatting, newlines, tabs, and Unicode characters.
- Standard input/output encoding is configured gracefully for UTF-8 on Windows consoles (`ENABLE_VIRTUAL_TERMINAL_PROCESSING` enabled via `ctypes`).

---

## 4. Jobs Registry (`.agent/control/jobs.json`)

Registered jobs are stored in `.agent/control/jobs.json`. Auto-discovery also surfaces historical reports matching `.agent/reports/claude-*.json` and `.agent/reports/gemini-*.json` (newest 80 by default).

### Registry Schema
```json
{
  "version": 1,
  "jobs": [
    {
      "id": "agent-control-build",
      "title": "Build terminal agent control panel",
      "provider": "Gemini CLI",
      "model": "gemini-3.8-flash-high",
      "prompt_file": ".agent/control/prompts/agent-control-build.txt",
      "prompt_kind": "exact",
      "output_file": ".agent/reports/gemini-agent-control-build.json",
      "review_file": ".agent/reports/agent-control-review.md",
      "verification": "pending",
      "pid": 51616,
      "started_at": "2026-09-09T15:09:25.117475Z",
      "process_start": "2026-09-09T15:09:25.117475Z"
    }
  ]
}
```

### Concurrent Registration & Start Identity Preservation
- **`RegistryLock`**: Write operations are serialized by a cross-platform sentinel lock (`.agent/control/jobs.json.lock`) with a 10-second acquisition timeout and 60-second stale reclamation. A writer releases only a lock it owns, so a writer that timed out and proceeded cannot delete the sentinel of the writer that holds it. On Windows, a lock file pending deletion reports `ERROR_ACCESS_DENIED` rather than `EEXIST`; this is treated as ordinary contention and retried, never propagated as a failed registration.
- **Unique staging files**: Each writer stages its update through its own `tempfile.mkstemp` file (threads in one process share a PID, so a PID-derived name is not unique) and then performs an atomic `replace`.
- **Identity Preservation**: Metadata-only updates (e.g. updating `--verification` on a completed or running job) preserve the original `started_at` and `process_start` timestamps so that running jobs continue to pass PID-reuse verification. A different `--pid` indicates a genuine relaunch and updates the start identity.
- **Corrupt Registry Refusal**: If `jobs.json` contains invalid JSON or is not an object, the tool refuses to overwrite it, exiting with code 2 to prevent data loss.

### Register Subcommand
```bash
# Register a newly launched agent job
./tools/agent-control.sh register \
  --id "mses-airfoil-audit" \
  --title "Audit MSES transonic convergence and Mach contours" \
  --provider "Claude Code" \
  --model "Opus" \
  --prompt-file ".agent/control/prompts/mses-prompt.txt" \
  --output-file ".agent/reports/claude-mses-audit.json" \
  --review-file ".agent/reports/mses-review.md" \
  --pid 12345 \
  --verification "pending"
```

If `--pid` is provided and the process is currently alive, its exact OS start identity is automatically captured and recorded into `process_start`.

---

## 5. Verification & Testing

Unit tests are located at `tools/test_agent_control.py` and require only the standard library:

```bash
# Execute unit tests using Python
python tools/test_agent_control.py

# Or via uv
uv run python tools/test_agent_control.py
```

### Test Suite Coverage
1. **Report Parser**:
   - Claude budget cap mapping (`error_max_budget_usd` -> `CAP_REACHED`, not quota).
   - Claude generic error (`is_error: true` -> `FAILED`).
   - Claude completed success (`terminal_reason: completed` -> `COMPLETED`).
   - Gemini empty `SUCCESS` and blocked `denied_actions` -> `DENIED` / `INCOMPLETE`.
   - Streaming partial JSON recovery during active writes.
   - 0-byte output file handling.
   - Provider recognition via JSON semantics with generic filenames (`report.json`).
   - Bare string `errors` field budget-cap parsing.
2. **Model Truthfulness**:
   - Requested `Opus` does not mask reported `claude-sonnet-5` (`Opus -> sonnet-5`).
   - Discovered historical Gemini labeled truthfully (`unrecorded`).
3. **Timezones & Review Panel**:
   - Explicit UTC and local timezone representations.
   - Markdown review file loading.
   - HTML review file tag stripping.
   - Missing review file handling.
4. **Registry Concurrency & Durability**:
   - Multi-threaded concurrent registration preserves all rows without loss.
   - Metadata-only updates preserve `started_at` and `process_start`.
   - Relaunch with new PID refreshes identity.
   - Corrupt registry refusal prevents wiping data.
   - Argument preservation with spaces.
5. **Text Sanitization & Escapes**:
   - Stripping ANSI colors, screen clears, OSC hyperlinks/titles, and control codes from reports, status details, and table cells.
   - Preserving Unicode accents and whitespace.
6. **PID-Reuse Guard**:
   - Mocked identity results (the `get_os_process_identity` boundary is patched, so the tests are platform-independent; the Linux `/proc` branch itself is only executed when the suite runs on Linux).
   - Direct verification on host platform using current process.
   - Reused PID detection when creation timestamps differ.
   - Fallback to `UNKNOWN` when identity cannot be verified.
7. **Prompt Security**:
   - Blocking sensitive paths (`.env`, `auth.json`, `tokens`).
   - "Prompt not captured" for historical discovered jobs.
8. **Responsive 80-Column Layout**:
   - Header, separator, and data rows strictly fit within 80 columns without line wrapping.
   - Narrow viewport (60 cols) adaptation.
9. **Full Text Wrapping & Scrolling**:
   - 2000-character single-line prompts wrap into viewport-width lines, avoiding `Scroll (0/0)` truncation.
   - Logical paragraph breaks and empty lines are preserved.
10. **Bounded Idle Redraw**:
    - Idle polling ticks emit zero frames.
    - Exactly 4 frames rendered over 10 seconds of idle time.
    - Immediate frame rendering on keypress or terminal resize.
