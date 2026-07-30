# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodríguez

"""
Authenticode-sign the built Windows executable.

Run after `wails build`:

    uv run python scripts/sign_windows.py

**A code-signing certificate cannot be generated, only obtained.** Windows
trusts a signature because a Certificate Authority verified the publisher's
legal identity; that check is the entire value of signing, and it is why no
script can produce a trusted certificate for you. An OV certificate signs
the binary; an EV certificate additionally clears the SmartScreen
reputation prompt that new publishers otherwise face.

This script therefore does the part that *can* be automated: locate signtool,
locate the certificate from whichever source is configured, sign with a
timestamp, and verify the result. It never invents a certificate and never
silently skips signing -- an unsigned build should fail loudly here rather than
be published looking signed.

Configuration (environment variables, so a certificate never lands in the repo):

    ALAS_SIGN_PFX        path to a .pfx/.p12 certificate file
    ALAS_SIGN_PASSWORD   its password
  or
    ALAS_SIGN_THUMBPRINT SHA-1 thumbprint of a certificate already in the
                              Windows certificate store (the usual setup for a
                              hardware token / EV certificate, whose private key
                              cannot be exported to a file at all)

    ALAS_SIGN_TIMESTAMP  RFC-3161 timestamp server
                              (default: http://timestamp.digicert.com)
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
DEFAULT_TARGET = REPO_ROOT / "desktop" / "build" / "bin" / "ALAS.exe"
DEFAULT_TIMESTAMP = "http://timestamp.digicert.com"


def find_signtool() -> Path | None:
    """Locate signtool.exe: PATH first, then the Windows SDK's versioned dirs."""
    on_path = shutil.which("signtool")
    if on_path:
        return Path(on_path)
    roots = [
        Path(os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)"))
        / "Windows Kits"
        / "10"
        / "bin",
        Path(os.environ.get("ProgramFiles", r"C:\Program Files"))
        / "Windows Kits"
        / "10"
        / "bin",
    ]
    candidates: list[Path] = []
    for root in roots:
        if root.is_dir():
            # Newest SDK last -> pick the highest version available.
            candidates.extend(sorted(root.glob("*/x64/signtool.exe")))
    return candidates[-1] if candidates else None


def build_sign_command(signtool: Path, target: Path) -> list[str]:
    pfx = os.environ.get("ALAS_SIGN_PFX")
    thumbprint = os.environ.get("ALAS_SIGN_THUMBPRINT")
    timestamp = os.environ.get("ALAS_SIGN_TIMESTAMP", DEFAULT_TIMESTAMP)

    # /fd sha256 : file digest. /tr + /td : RFC-3161 timestamp, so the signature
    # stays valid after the certificate itself expires. Without a timestamp
    # every build silently stops being trusted on the certificate's expiry date.
    base = [
        str(signtool),
        "sign",
        "/fd",
        "sha256",
        "/tr",
        timestamp,
        "/td",
        "sha256",
        "/v",
    ]

    if pfx:
        password = os.environ.get("ALAS_SIGN_PASSWORD")
        cmd = base + ["/f", pfx]
        if password:
            cmd += ["/p", password]
        return cmd + [str(target)]
    if thumbprint:
        # /sha1 selects by thumbprint from the certificate store; /a would let
        # signtool guess, which is how the wrong certificate gets used.
        return base + ["/sha1", thumbprint, str(target)]

    raise SystemExit(
        "No signing certificate configured.\n"
        "  Set ALAS_SIGN_PFX (+ ALAS_SIGN_PASSWORD) for a .pfx file, or\n"
        "  set ALAS_SIGN_THUMBPRINT for a certificate in the Windows store.\n"
        "A certificate must be obtained from a CA; it cannot be "
        "generated locally in any form Windows will trust."
    )


def main() -> int:
    if sys.platform != "win32":
        print("sign_windows.py only runs on Windows (signtool is a Windows SDK tool).")
        return 1

    target = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_TARGET
    if not target.is_file():
        print(f"Nothing to sign: {target} does not exist. Run `wails build` first.")
        return 1

    signtool = find_signtool()
    if signtool is None:
        print(
            "signtool.exe not found. Install the Windows SDK "
            "(https://developer.microsoft.com/windows/downloads/windows-sdk/) "
            "or add signtool to PATH."
        )
        return 1

    print(f"signtool : {signtool}")
    print(f"target   : {target}")

    cmd = build_sign_command(signtool, target)
    # Never print the command: it can contain the certificate password.
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        print("Signing FAILED:\n" + (result.stderr or result.stdout))
        return result.returncode
    print("Signed.")

    verify = subprocess.run(
        [str(signtool), "verify", "/pa", "/v", str(target)],
        capture_output=True,
        text=True,
    )
    if verify.returncode != 0:
        # A signature that doesn't verify is worse than none: it looks signed
        # while still tripping SmartScreen, so fail rather than report success.
        print("Verification FAILED:\n" + (verify.stdout or verify.stderr))
        return verify.returncode

    print("Verified. Signature chains to a trusted root.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
