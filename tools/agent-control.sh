#!/usr/bin/env bash
# =============================================================================
# ALAS Terminal Agent Control Launcher
# Works across Git Bash on Windows, Linux, and macOS.
# Automatically derives repository root and discovers Python / uv.
# =============================================================================

set -euo pipefail

# Derive repository path from script location
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
PYTHON_SCRIPT="${SCRIPT_DIR}/agent_control.py"

# Handle Windows path conversions under Git Bash / MSYS / Cygwin
if command -v cygpath >/dev/null 2>&1; then
    TARGET_SCRIPT="$(cygpath -w "${PYTHON_SCRIPT}")"
    TARGET_REPO="$(cygpath -w "${REPO_ROOT}")"
else
    TARGET_SCRIPT="${PYTHON_SCRIPT}"
    TARGET_REPO="${REPO_ROOT}"
fi

# Probe whether an interpreter actually executes code (rejects Windows Store stubs)
probe_python() {
    "$@" -c "pass" >/dev/null 2>&1
}

# Discover available Python environment
if command -v uv >/dev/null 2>&1 && probe_python uv run python; then
    exec uv run python "${TARGET_SCRIPT}" --repo "${TARGET_REPO}" "$@"
elif command -v python3 >/dev/null 2>&1 && probe_python python3; then
    exec python3 "${TARGET_SCRIPT}" --repo "${TARGET_REPO}" "$@"
elif command -v python >/dev/null 2>&1 && probe_python python; then
    exec python "${TARGET_SCRIPT}" --repo "${TARGET_REPO}" "$@"
elif command -v py >/dev/null 2>&1 && probe_python py -3; then
    exec py -3 "${TARGET_SCRIPT}" --repo "${TARGET_REPO}" "$@"
else
    echo "ERROR: No functional Python interpreter found ('uv', 'python3', 'python', 'py -3')." >&2
    echo "Please install Python 3 or uv to run the agent control panel." >&2
    exit 1
fi
