#!/bin/sh
# Build a release tarball: one binary, the standard library, and the guide.
#
#     sh scripts/release.sh            # writes dist/burxt-<version>-linux-x86_64.tar.gz
#
# **This is an entry point and nothing else. `scripts/release.bx` is the release script.**
# Everything the tarball IS — what goes in it, the strip, the archive, and every assertion about
# what came out — lives there, in the language this repository ships.
#
# What stays here is the part that cannot be written in Burxt: on a fresh clone there is no Burxt
# binary, so cargo has to produce one before anything else can run. The same chicken-and-egg as
# `scripts/install.sh`, and the same answer — keep the cold-start path in shell, keep the logic out.
#
# No `exec`, because the trap above has to run: the compiled release script is 30 MB of scratch.
#
# `run -o` and not `run`, because `burxt run` writes its executable into the working directory and a
# stray binary in the repository root is something the test suite refuses.
set -e
cd "$(dirname "$0")/.."

export LLVM_SYS_181_PREFIX="${LLVM_SYS_181_PREFIX:-/usr/lib/llvm-18}"
cargo build --release >&2      # cargo's progress is a build log, not this script's output

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
./target/release/burxt run scripts/release.bx -o "$WORK/release"
