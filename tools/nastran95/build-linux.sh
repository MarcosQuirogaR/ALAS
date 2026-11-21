#!/bin/sh
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez
#
# Reproducible Linux build and staging of NASTRAN-95 for ALAS.
#
#   tools/nastran95/build-linux.sh [--out DIR] [--license FILE] [--jobs N]
#
# Output (default `external tools/NASTRAN-95` under the repository root) has
# exactly the layout `cargo xtask dist` checks (xtask/src/dist_nastran95.rs):
#
#   build/bin/nastran, nastran95-build.txt, nastran95-open-core.txt
#   rf/                        rigid formats incl. NASINFO
#   runtime/DEPENDENCIES.txt   inventory of runtime libraries ("none": static)
#   MODIFICATIONS.md, SOURCE-REVISION.txt, source/NASTRAN-95-source.zip
#   LICENSE                    only when --license FILE (the filled-in NOSA 1.3
#                              agreement) is given; without it `dist` records
#                              NASTRAN-95 as not_bundled, as on Windows.
#
# Inputs are pinned below: the upstream fork commit, ALAS's local patch
# (tools/nastran95/alas-local.patch: open-core parameterisation and the build
# options recorded in MODIFICATIONS.md) and the GNU Fortran version, which is
# recorded (and, when NASTRAN95_REQUIRE_GFORTRAN_MAJOR is set, enforced).
# libgfortran and libgcc are linked statically (GCC Runtime Library Exception);
# any other shared object left over (libquadmath on gfortran 11) is bundled in
# runtime/ with its licence notice and declared in DEPENDENCIES.txt.
set -eu

UPSTREAM_URL="https://github.com/Foadsf/NASTRAN-95.git"
UPSTREAM_COMMIT="060ed900db822745353fcce96bddc1565027e7db"
OPEN_CORE_WORDS="64000000"

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
out="$repo/external tools/NASTRAN-95"
license=""
jobs=$(nproc 2>/dev/null || echo 1)
while [ $# -gt 0 ]; do
    case "$1" in
        --out) out="$2"; shift 2 ;;
        --license) license="$2"; shift 2 ;;
        --jobs) jobs="$2"; shift 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done

# The script deletes files under "$out/runtime" and replaces one archive under
# "$out/source". Refuse an output folder that is empty-named or a root, and an
# existing runtime folder that this script did not write (it always leaves a
# DEPENDENCIES.txt there), so a wrong --out cannot lose unrelated files.
case "$out" in
    ""|"/"|"$HOME"|"$HOME/") echo "refusing --out '$out'" >&2; exit 2 ;;
esac
if [ -d "$out/runtime" ] && [ -n "$(ls -A "$out/runtime" 2>/dev/null)" ] &&
    [ ! -f "$out/runtime/DEPENDENCIES.txt" ]; then
    echo "refusing --out '$out': runtime/ is not empty and holds no DEPENDENCIES.txt from this script" >&2
    exit 2
fi

for tool in git gfortran cmake ninja zip sha256sum; do
    command -v "$tool" >/dev/null 2>&1 || { echo "missing required tool: $tool" >&2; exit 1; }
done
gfortran_version=$(gfortran -dumpfullversion)
if [ -n "${NASTRAN95_REQUIRE_GFORTRAN_MAJOR:-}" ] &&
    [ "${gfortran_version%%.*}" != "$NASTRAN95_REQUIRE_GFORTRAN_MAJOR" ]; then
    echo "gfortran $gfortran_version, need major $NASTRAN95_REQUIRE_GFORTRAN_MAJOR" >&2
    exit 1
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
src="$work/src"
git init -q "$src"
git -C "$src" remote add origin "$UPSTREAM_URL"
git -C "$src" fetch -q --depth 1 origin "$UPSTREAM_COMMIT"
git -C "$src" checkout -q FETCH_HEAD
[ "$(git -C "$src" rev-parse HEAD)" = "$UPSTREAM_COMMIT" ] || { echo "upstream commit mismatch" >&2; exit 1; }
git -C "$src" apply --whitespace=nowarn "$here/alas-local.patch"

# The corresponding source: patched tree + this recipe, as a deterministic zip.
cp "$here/alas-local.patch" "$here/build-linux.sh" "$src/"
git -C "$src" add -A
GIT_AUTHOR_NAME=alas GIT_AUTHOR_EMAIL=alas@invalid GIT_AUTHOR_DATE="2026-01-01T00:00:00Z" \
GIT_COMMITTER_NAME=alas GIT_COMMITTER_EMAIL=alas@invalid GIT_COMMITTER_DATE="2026-01-01T00:00:00Z" \
    git -C "$src" commit -q -m "NASTRAN-95 as built by ALAS (upstream $UPSTREAM_COMMIT + alas-local.patch)"
mkdir -p "$out/source"
rm -f "$out/source/NASTRAN-95-source.zip"
git -C "$src" archive --format=zip -o "$out/source/NASTRAN-95-source.zip" HEAD
archive_sha=$(sha256sum "$out/source/NASTRAN-95-source.zip" | cut -d' ' -f1)

build="$work/build"
cmake -S "$src" -B "$build" -G Ninja \
    -DCMAKE_Fortran_COMPILER=gfortran \
    -DNASTRAN95_OPEN_CORE_WORDS="$OPEN_CORE_WORDS" \
    -DCMAKE_EXE_LINKER_FLAGS="-static-libgfortran -static-libgcc"
cmake --build "$build" --target nastran -j "$jobs"

mkdir -p "$out/build/bin" "$out/rf" "$out/runtime"
cp "$build/bin/nastran" "$build/bin/nastran95-open-core.txt" "$out/build/bin/"
chmod 755 "$out/build/bin/nastran"
cp -R "$src"/rf/. "$out/rf/"

# Runtime inventory. -static-libgfortran -static-libgcc leave only what the GNU
# driver does not fold in (on gfortran 11, libquadmath may stay dynamic; the
# driver option -static-libquadmath is not relied on). Every shared object that
# is not part of the base system (libc, libm, libdl, libpthread, the loader,
# vdso) is copied beside the solver and declared in DEPENDENCIES.txt, with its
# distribution licence notice; the dist contract checks the two agree.
mkdir -p "$out/runtime"
find "$out/runtime" -type f -delete
: > "$work/deps.txt"
ldd "$out/build/bin/nastran" | while read -r name arrow path rest; do
    case "$name" in
        linux-vdso.so.*|libc.so.*|libm.so.*|libdl.so.*|libpthread.so.*|librt.so.*|ld-linux*|/lib*/ld-linux*) ;;
        *.so|*.so.*)
            [ "$arrow" = "=>" ] && [ -f "$path" ] || { echo "cannot resolve $name" >&2; exit 1; }
            cp -L "$path" "$out/runtime/$name"
            echo "$name" >> "$work/deps.txt"
            pkg=$(dpkg -S "$path" 2>/dev/null | head -1 | cut -d: -f1 || true)
            if [ -n "$pkg" ] && [ -f "/usr/share/doc/$pkg/copyright" ]; then
                cp "/usr/share/doc/$pkg/copyright" "$out/runtime/LICENSE-$name.txt"
            else
                echo "no licence notice found for $name ($path)" >&2
                exit 1
            fi ;;
    esac
done
{
    echo "# Runtime libraries beside the solver (libc, libm and the loader come from the host)."
    echo "# libgfortran and libgcc are linked statically; whatever else the driver left dynamic is listed here."
    cat "$work/deps.txt"
} > "$out/runtime/DEPENDENCIES.txt"

# The flags actually used, read from the build system rather than restated.
compile_line=$(ninja -C "$build" -t commands nastran | grep -m1 'gfortran.*mis/' | cut -c1-2000)
link_line=$(ninja -C "$build" -t commands nastran | tail -1 | cut -c1-2000)

cat > "$out/build/bin/nastran95-build.txt" <<BUILD
NASTRAN-95 Linux build recorded by tools/nastran95/build-linux.sh
upstream: $UPSTREAM_URL @ $UPSTREAM_COMMIT
local patch SHA-256: $(sha256sum "$here/alas-local.patch" | cut -d' ' -f1)
gfortran: $gfortran_version
cmake: $(cmake --version | head -1)
open core words: $OPEN_CORE_WORDS
compile: $compile_line
link: $link_line
executable SHA-256: $(sha256sum "$out/build/bin/nastran" | cut -d' ' -f1)
BUILD

cp "$here/MODIFICATIONS-linux.md" "$out/MODIFICATIONS.md"
cat > "$out/SOURCE-REVISION.txt" <<REVISION
Upstream repository: $UPSTREAM_URL
Upstream commit: $UPSTREAM_COMMIT
Local patch: tools/nastran95/alas-local.patch (see MODIFICATIONS.md)
Build recipe: tools/nastran95/build-linux.sh
Source archive SHA-256: $archive_sha
REVISION

if [ -n "$license" ]; then
    cp "$license" "$out/LICENSE"
else
    echo "[INFO] no --license given: LICENSE absent, cargo xtask dist will record NASTRAN-95 as not_bundled"
fi
echo "staged NASTRAN-95 ($gfortran_version) in $out"
