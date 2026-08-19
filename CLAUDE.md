# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and test

LLVM 18 development libraries are required, and the bindings need to be pointed at them:

```sh
export LLVM_SYS_181_PREFIX=/usr/lib/llvm-18     # or $(brew --prefix llvm@18) on macOS
cargo build                                     # produces ./target/debug/burxt
cargo test                                      # the gate: ~140 tests, several minutes
```

```sh
cargo test <substring>                          # one test by name
cargo test --release <substring>                # much faster to run, slower to build
BURXT_VERDICTS=1 cargo test -- --nocapture      # one TAP-style line per fixture
./target/debug/burxt check tests/fail/x.bx      # exercise a single fixture by hand
```

The suite also runs **on Burxt**, over the same fixtures, and a Rust test asserts the two runners
agree fixture by fixture:

```sh
./target/debug/burxt build tests/runner.bx -o /tmp/burxt-runner
/tmp/burxt-runner "$(realpath ./target/debug/burxt)" /tmp/work     # must print `all green`
```

`cargo test` measures the **working tree**, not the commit. Before reporting a green run as evidence
about a branch: `git diff --quiet HEAD -- . && cargo test`.

Any pipeline in CI or a script must `set -o pipefail` — GitHub Actions runs bash with `-e` and
without it, and a `| tee` swallowed every cargo failure here for thirty versions, hiding a broken
self-hosting fixpoint under a green tick.

## Two compilers, and which one is the product

| | | |
|---|---|---|
| `src/burxt-compiler/` | Burxt | **The product.** Compiles itself to a byte-identical fixpoint. |
| `src/rust-compiler/` | Rust | The **bootstrap** and the **differential oracle**. Not a peer. |

`Cargo.toml` points `[[bin]]` at `src/rust-compiler/main.rs`, because on a machine with no Burxt
binary `cargo build` is the only way to get one. That is why the Rust compiler is not under `tests/`
despite being the cross-check — see `src/README.md`.

**The gate: Rust may BUILD Burxt; Burxt may not USE Rust.** A bootstrap is a one-time debt; a
dependency is permanent.

"The two agree" means the same *result*, not identical text. A transliteration would be wrong: three
times the Burxt implementation has audited the Rust one precisely because it did the job differently
(`diag.bx` is total where `diag.rs` panicked; `lsp.bx` answers hover on files with `use` lines where
`lsp.rs` answered nothing).

## Front end / back end

`lexer.rs` → `parser.rs` → `ast.rs` → `typeck.rs` → `codegen.rs`, driven by `main.rs`.

**Only `codegen.rs` may know about LLVM or any target detail.** Preserve this — it is what makes
cross-platform support a configuration problem rather than a rewrite. `typeck.rs` is where the
language's correctness thesis is enforced.

Two passes-related rules that have each been violated twice:

- **A declaration pass cannot judge anything that depends on what it is still collecting.** A type
  may name any type the program declares, including itself, so nothing can be judged until every
  declaration is in. That is why `check_return_storage` is a separate pass.
- **Imports never reach the parser.** `strip_imports` (`main.rs`) blanks `use` lines with spaces —
  same length, so every span downstream is unaffected. A code path that parses raw buffer text will
  report a `use` line as a syntax error.

## The test suite is data-driven

Fixtures live under `tests/`, one directory per verdict. Adding a test means dropping two files in a
directory — no registration:

| | |
|---|---|
| `tests/pass/x.bx` + `x.stdout` | must compile and produce exactly that output |
| `tests/fail/x.bx` + `x.stderr` | must be **rejected** with exactly that error |
| `tests/panic/x.bx` + `x.stdout` | must compile, then die at runtime with that message |
| `tests/limitations/x.bx` | a documented limit; header comment carries `CLAIM:` / `HOLDS:` / `REFUSED-BECAUSE:` / `WHY:` |
| `tests/review/x.old.bx` + `x.new.bx` + `x.expect` | what `burxt review` says changed about a promise |
| `tests/support/` | Burxt harnesses driving the LSP, diagnostics, layout, review, the runner |

Rejection tests matter as much as acceptance tests: the language's identity is that the compiler
refuses dangerous things, so a feature PR needs both. Error messages read as **advice** — name the
rule and show the syntax that fixes it.

A fixture in `tests/pass/` cannot distinguish "supported" from "not examined". That is why CI runs a
mutation check (`tests/support/runner_agreement.bx`) that breaks one fixture in a scratch tree and
requires the runner to *name* it.

## Repository invariants enforced by the suite

These fail `cargo test` and surprise people:

- **`.gitignore` is a whitelist.** A new `.bx`/`.md`/`.rs`/`.json`/`.sh`/`.py`/`.css`/`.js`/`.html`/
  `.yml` file must be `git add`ed or `every_source_and_document_is_in_version_control` fails.
- **The layout is declared in two places.** `the_repository_layout_is_declared` holds directories;
  `the_repository_root_holds_only_what_belongs_there` holds an allowlist of root *files*. Adding a
  root file and checking only the first is the "whitelist of places to check is not a check" failure
  below — it happened while this file was being added. A `git worktree` must live *outside* the
  repository.
- **`tests/runner.bx` re-implements the invariants, so several rules live in two runners.** Change one
  and `the_suite_also_runs_on_burxt` fails with `not ok invariant/…`. That is the parity discipline
  working on the suite rather than the compiler; grep `runner.bx` for the rule you just edited.
- **No stray binary in the repository root** — `burxt run` writes the executable into the working
  directory, so run one-off programs from `/tmp`.
- **The packaged `.vsix` must match the grammar in the tree**, and every documented install command
  must name the file `pack.py` actually writes.

## Standard library and `std/` resolution

`lib/` is the standard library, written in Burxt from the same builtins any program has. `use
"std/x.bx"` resolves against **exactly two roots**: `BURXT_LIB`, then `$PREFIX/lib/burxt` beside the
running binary. There is deliberately no `/usr/local/lib/burxt` fallback — it was removed because a
compiler built in this repository silently compiled against the *installed* library.

A build never touches the network. `burxt fetch` is the only place, and only when asked; with a
`burxt.lock` present the locked commit is checked out rather than the tag. `burxt.package` grammar is
closed — `name`, `version`, `dependency`, and nothing else.

## Editor extension

`editors/vscode/`, packaged by `python3 editors/vscode/pack.py` — a `.vsix` writer in the Python
standard library, no npm and no `vsce`. It writes `burxt.vsix`, with **no version in the filename**
deliberately; the version lives in `package.json` where VS Code reads it. After touching the grammar,
repack and bump that version — an installed extension will not upgrade to the same number.

The extension spawns the compiler, so `extensionKind` must be `["workspace"]`; `pack.py` refuses to
build a package whose manifest omits it rather than defaulting. Its `FILES` list is authored rather
than globbed, so a new asset must be added there *and* to `package.json`.

The compiler it spawns defaults to whichever of `./target/release/burxt` and `./target/debug/burxt`
is newer **in the workspace**, otherwise `burxt` from PATH — so a repository that consumes Burxt
without building it silently inherits whatever is installed.

## Documentation

- `docs/guide/`, `docs/reference/`, `docs/install/` — the live Jekyll site.
- `docs/1.0/` — **frozen.** It documents 1.0.0 and carries a notice saying it will not change again.
  Do not edit it to fix drift.
- `docs/log/` — the milestone log, a historical record. Do not rewrite past entries.
- `spec/` — the design record grouped by the version each decision shipped in; `spec/README.md` is
  the status index. `DESIGN.md` is the standing design; superseded decisions are marked, not
  silently swapped.

Several scripts generate site content (`scripts/site-*.py`); tests assert the generated output is
current, so regenerate rather than hand-editing what they produce.

## Working discipline

The feature bar, from `CONTRIBUTING.md`: **is there a concrete, required program that cannot be
written without it?** Merely convenient features are deferred into the ledger with the trigger that
would earn them a later milestone. Design changes to the type system, grammar, or object model get
discussed before code.

`CONTRIBUTING.md` §5–§7 are long for a reason — they record failures that actually happened here, and
they are worth reading before any commit that touches a shared file:

- `git commit` takes the whole index; `git commit --amend` does too, regardless of any pathspec.
  Use `git commit -- <paths>`, then read `git show --stat HEAD` — the line counts, not just the names.
- **A whitelist of places to check is not a check.** Two people cleared the same commit by confirming
  it held nothing under `src/`, `examples/` or a named test directory. Both were right; the sweep was
  in `tests/runner.rs`, which neither thought of as anyone's.
- **When a guard refuses, never make the expected value true.** A compare-and-swap whose expected
  value is read from the thing being compared against is a guard with an off switch.
- **A name asserting work is not evidence the work happens.** Ask what a check would say if the thing
  it names were absent entirely; if the answer is "nothing", it is not checking. For any
  correspondence check, write the reverse direction and run it once.
