# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""The Selig airfoil coordinate corpus, read out of ``coord_seligFmt.zip``.

``AirfoilLibrary._init_zip``/``get`` (``alas/geometry/airfoils.py``) reads
1,665 ``.dat`` coordinate files out of ``alas/data/coord_seligFmt.zip``
through Python's ``zipfile``. ``alas-geom::selig`` reproduces that without a
zip dependency: the archive is unpacked once into a single embedded text
corpus, ``crates/alas-geom/data/selig.txt``, delimited by lines ``@<stem>``.
Each entry stores its ``.dat`` file's bytes verbatim, including the
name/header line the parser skips over.

That delimiter scheme is safe only if two things are true of the archive:
every entry is ASCII, and no line in any entry begins with ``@``. This
generator checks both against the live archive rather than assuming
``docs/PORTING.md``'s claim still holds, and refuses to write anything if
either fails. It also checks the archive has no two stems that collide once
lowercased, which is what lets the corpus be looked up by a single
case-folded map with no ordering to reproduce.

This generator does two jobs from one reading of the archive, deliberately:
it writes the embedded corpus the crate ships (``crates/alas-geom/data/
selig.txt``), and it writes the parity fixture (``golden/geom/selig.json``)
that proves that corpus against the archive. Splitting the extraction logic
across two scripts would risk the corpus and the fixture disagreeing about
what "verbatim" means; written once here, they cannot.

One entry each in the archive lacks a byte the delimiter scheme relies on: 5
of 1,665 ``.dat`` files (``e374``, ``eiffel10``, ``k3311``,
``legionair140_sm``, ``psu94097``) have no trailing newline on their last
line. Concatenated directly, that last line would run into the following
``@stem`` line and neither would parse correctly. Every stored entry is
therefore terminated with a single ``\\n`` if it does not already end in one
-- a no-op for the other 1,660 entries, which already end ``\\r\\n`` -- and the
per-entry digest is taken after that normalization, over exactly the bytes
the corpus stores, so the fixture proves what is actually shipped rather than
the archive's raw bytes.

The fixture does not carry a second copy of the corpus (1,665 entries, 2.52 MB
raw). It carries a digest per entry, keyed by stem, and full resolved
coordinates for a representative sample: the smallest and largest entries by
point count, two entries that hit the 5-file no-trailing-newline case, two
entries whose remaining lines include ones that fail the parser's float check
(a multi-element flap deck's comment lines, and a licence-notice comment), and
a handful of ordinary entries, one of which -- ``naca2410`` -- is the default
aircraft's wing tip section (``docs/PORTING.md``, Geometry).

The digest is FNV-1a, 64-bit, not a cryptographic hash: this fixture is
proving the corpus has not silently drifted from the archive it was
extracted from, not defending against someone crafting a collision, and
``alas-testkit`` (this port's only shared test dependency) has no hash
function to reach for -- adding one to compute a digest of static airfoil
data would be a dependency this repository's layering rules do not grant a
generator the authority to add. FNV-1a is specified precisely enough
(http://www.isthe.com/chongo/tech/comp/fnv/) to implement identically on
both sides with the standard library alone, which is what the port's Rust
side does too.
"""

from __future__ import annotations

import zipfile
from pathlib import Path

import _framework

ZIP_PATH = _framework.ALAS_ROOT / "alas" / "data" / "coord_seligFmt.zip"
REPO_ROOT = Path(__file__).resolve().parent.parent.parent
CORPUS_PATH = REPO_ROOT / "crates" / "alas-geom" / "data" / "selig.txt"

# Spread across the corpus rather than picked at random: see the module
# docstring for why each one is here.
SAMPLE_STEMS = [
    "naca2410",  # the default aircraft's wing tip section
    "e49",  # fewest resolved points (24)
    "30p-30n",  # most resolved points (664); also has comment lines to skip
    "s9104",  # a licence-notice comment line the parser must skip
    "e374",  # no trailing newline on the last coordinate line
    "legionair140_sm",  # no trailing newline on the last coordinate line
    "clarky",
    "rae2822",
    "sd7037",
]


def _normalize(raw: bytes) -> bytes:
    """Terminate an entry with exactly one trailing newline.

    The delimiter scheme needs every entry to end at a line boundary so the
    next ``@stem`` line starts a fresh line; 1,660 of 1,665 entries already
    do (they end ``\\r\\n``), and this is a no-op for them. The other 5 do
    not end in any newline at all, and gain a bare ``\\n`` -- not part of the
    original file, but harmless: line splitting does not distinguish a final
    line with or without its own terminator, on either side of this port.
    """
    return raw if raw.endswith(b"\n") else raw + b"\n"


def _read_entries() -> list[tuple[str, bytes]]:
    """Every ``.dat`` entry, in archive order, normalized as stored."""
    entries = []
    with zipfile.ZipFile(ZIP_PATH) as archive:
        for name in archive.namelist():
            if not (name.startswith("coord_seligFmt/") and name.endswith(".dat")):
                continue
            stem = Path(name).stem
            entries.append((stem, _normalize(archive.read(name))))
    return entries


def _verify_corpus_invariants(entries: list[tuple[str, bytes]]) -> None:
    """The two properties the delimiter scheme assumes, plus the one that
    makes a case-folded name index unambiguous. All three are asserted here
    against the archive rather than taken on faith."""
    seen_lower: dict[str, str] = {}
    for stem, raw in entries:
        try:
            text = raw.decode("ascii")
        except UnicodeDecodeError as error:
            raise SystemExit(f"{stem}: entry is not ASCII ({error})") from error
        for line in text.splitlines():
            if line.startswith("@"):
                raise SystemExit(f"{stem}: a content line starts with '@': {line!r}")
        lower = stem.lower()
        if lower in seen_lower and seen_lower[lower] != stem:
            raise SystemExit(
                f"stems collide under lower(): {seen_lower[lower]!r} and {stem!r}"
            )
        seen_lower[lower] = stem


def _write_corpus(entries: list[tuple[str, bytes]]) -> None:
    """Write the embedded corpus ``alas-geom::selig`` includes at build time.

    Opened in binary mode and given each entry's bytes exactly as read from
    the archive (plus the normalization above) -- a text-mode write on
    Windows would rewrite every bare ``\\n`` this function adds into
    ``\\r\\n``, which is a byte this function did not put there.
    """
    CORPUS_PATH.parent.mkdir(parents=True, exist_ok=True)
    with CORPUS_PATH.open("wb") as handle:
        for stem, raw in entries:
            handle.write(b"@" + stem.encode("ascii") + b"\n")
            handle.write(raw)
    print(f"wrote {CORPUS_PATH}")


_FNV_OFFSET_BASIS_64 = 0xCBF29CE484222325
_FNV_PRIME_64 = 0x100000001B3
_MASK_64 = (1 << 64) - 1


def _fnv1a64(data: bytes) -> str:
    """FNV-1a, 64-bit, as 16 lowercase hex digits.

    See the module docstring for why this and not a cryptographic hash. The
    algorithm: start from the offset basis, then for every byte, XOR it in
    and multiply by the prime, wrapping to 64 bits.
    """
    digest = _FNV_OFFSET_BASIS_64
    for byte in data:
        digest ^= byte
        digest = (digest * _FNV_PRIME_64) & _MASK_64
    return f"{digest:016x}"


def _parse_coordinates(raw: bytes) -> list[list[float]]:
    """Reproduce the zip-branch of ``AirfoilLibrary.get``: skip the header
    line, then parse each remaining line's first two whitespace-separated
    fields as floats, silently dropping any line that does not parse."""
    lines = raw.decode("ascii").splitlines()
    coords = []
    for line in lines[1:]:
        parts = line.strip().split()
        if len(parts) >= 2:
            try:
                coords.append([float(parts[0]), float(parts[1])])
            except ValueError:
                continue
    return coords


def main() -> None:
    entries = _read_entries()
    if len(entries) != 1665:
        raise SystemExit(f"expected 1,665 airfoil entries in the archive, found {len(entries)}")

    _verify_corpus_invariants(entries)
    _write_corpus(entries)

    by_stem = dict(entries)
    digests = {stem: _fnv1a64(raw) for stem, raw in entries}

    samples = {}
    for stem in SAMPLE_STEMS:
        if stem not in by_stem:
            raise SystemExit(f"sample stem {stem!r} is not in the archive")
        samples[stem] = _parse_coordinates(by_stem[stem])

    _framework.write(
        "geom",
        "selig",
        {"digests": digests, "samples": samples},
        description=(
            "FNV-1a 64-bit digest of every Selig corpus entry's stored "
            "bytes, keyed by stem (proves crates/alas-geom/data/selig.txt "
            "against the archive), plus resolved (x, y) coordinates for a "
            "representative sample of entries"
        ),
    )


if __name__ == "__main__":
    main()
