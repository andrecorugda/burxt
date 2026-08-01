# Burxt in a container: the artifact that reaches Linux, Kubernetes and Windows at once.
#
#     docker build -f scripts/Dockerfile -t burxt .
#     docker run --rm -v "$PWD:/work" burxt run hello.bx
#
# On Windows 11 the same image runs under WSL container (`wslc run ...`) with no Docker
# Desktop and no third-party runtime — which is why this file exists instead of a native
# MSVC port of the compiler. One artifact, three platforms.
#
# ---------------------------------------------------------------------------------------
# This image does NOT build Burxt. It copies binaries that `release.sh` already built and
# smoke-tested natively on each architecture.
#
# The alternative — compiling inside the image — means building a statically-linked LLVM 18
# under QEMU for the non-host architecture. That is hours per arch for a result identical to
# the one the native runner produced in minutes, and a build that slow stops being run.
# `release.yml` builds amd64 on `ubuntu-latest` and arm64 on `ubuntu-24.04-arm`, both native.
#
# The build context is **`dist/oci`**, not the repository root:
#
#     docker buildx build -f scripts/Dockerfile dist/oci
#
#     dist/oci/linux-amd64/burxt   dist/oci/linux-amd64/lib/
#     dist/oci/linux-arm64/burxt   dist/oci/linux-arm64/lib/
#
# A root context would ship `target/` — gigabytes of build tree — to the daemon on every
# build. The usual answer is a `.dockerignore`, but the repository root is an allowlist
# enforced by `the_repository_root_holds_only_what_belongs_there` in BOTH runners, and a new
# root file means editing that rule in two places to save a directory that should not have
# been in the context anyway. Narrowing the context is the smaller and more honest fix.
#
# `TARGETARCH` is set by buildx per platform and selects between the two subdirectories.
# ---------------------------------------------------------------------------------------

FROM debian:bookworm-slim

# `gcc` is not optional and it is not a convenience. `burxt build` hands its object file to
# the system linker by calling `cc` — the compiler owns the triple and the object, and
# delegates linking to the toolchain that already knows the platform (roadmap M3).
#
# An image carrying only the binary would pass `burxt check` and fail every single `burxt
# build`, which is precisely the failure `scripts/install.sh` warns about before installing.
# Shipping an image that reproduces a failure we already wrote a warning for would be worse
# than shipping no image.
#
# This layer comes before the COPY so that a new Burxt version does not re-fetch apt.
RUN apt-get update \
    && apt-get install -y --no-install-recommends gcc libc6-dev \
    && rm -rf /var/lib/apt/lists/*

ARG TARGETARCH
COPY linux-${TARGETARCH}/burxt /usr/local/bin/burxt
COPY linux-${TARGETARCH}/lib/  /usr/local/lib/burxt/

# The standard library lives here, and inside the image it must be named by ABSOLUTE path:
#
#     use "/usr/local/lib/burxt/string.bx";
#
# There is deliberately no `BURXT_LIB` here, because there is no such thing. `use` resolves
# relative to the importing FILE's directory and nothing else — no search path, no
# environment variable (`load_into` in src/rust-compiler/main.rs). An `ENV BURXT_LIB` would
# be decoration that changes nothing, and a container that advertises a variable the
# compiler never reads is worse than one that says the real path.
#
# That this is awkward in a container is a genuine finding, not a packaging detail: the
# image gives the library ONE fixed home, which is the first time a search path would have
# an unambiguous answer. Recorded as a compiler question in spec/ROADMAP-1.1.md, not
# papered over here.

WORKDIR /work

# No ENTRYPOINT wrapper script. `docker run burxt run hello.bx` should be the same words as
# `burxt run hello.bx`, because a container that renames the commands is a second CLI to
# learn. Bare `docker run burxt` prints the compiler's own usage.
ENTRYPOINT ["/usr/local/bin/burxt"]
CMD []
