# Security policy

## Supported versions

Security fixes are made on the latest release (currently 1.3.x) and on `main`.
Older releases are not patched; upgrade to the latest release.

## Reporting a vulnerability

Please do not open a public issue for a security problem.

Use GitHub private vulnerability reporting: on the repository page choose
Security, then "Report a vulnerability". If that is unavailable, email
marcos.quiroga.rodriguez@gmail.com with "ALAS security" in the subject.

Include the affected version, how to reproduce the problem, and what an
attacker could do with it. You should receive an acknowledgement within seven
days. Fixes are published in a new release, and the report is credited unless
you ask otherwise.

## Scope

In scope: the ALAS application and its release packages, including handling of
files the program opens (aircraft configuration, CPACS, airport and route
data), the way it launches external tools, and the build and release workflows.

Out of scope: the external programs ALAS can drive (AVL, XFOIL, MSES, OpenVSP,
NASTRAN, OpenFOAM) and the dependencies listed in `Cargo.lock`; report those to
their own projects. Wrong engineering results are not security issues; report
them as bugs with the aircraft configuration that reproduces them.
