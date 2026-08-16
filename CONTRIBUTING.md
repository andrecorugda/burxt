# Contributing to Burxt

Thank you for your interest in Burxt. This project is built with a deliberate discipline, and contributions that understand it are far more likely to be merged. Please read this before opening a large PR.

## The philosophy (read this first)

Burxt's guiding rule is from its design north star: **pick a few signature features, resist the kitchen sink.** A language novel on every line gets admired and unused. Almost every feature request is individually reasonable and collectively a swamp. So the bar for a new feature is not "is this nice?" — it is:

> **Is there a concrete, required program that cannot be written without it?**

If a feature is merely convenient, it is deferred and written into the deferred-features ledger with the trigger that would earn it a future milestone — not added now. This is how the design phases stay honest.

Two consequences for contributors:
- **Small, focused PRs win.** A PR that adds one well-scoped capability with tests is easy to review and merge. A PR that adds five is not.
- **Design changes need discussion first.** For anything touching the type system, grammar, or object model, open an issue describing the problem before writing code. The design is recorded in `DESIGN.md`, the milestone log in `docs/log/`, and the milestone specs in `spec/`; changes to it are deliberate and get documented (superseded decisions are marked, not silently swapped).

## How the code is structured

The compiler is written in Rust with a strict front-end / back-end split:

- `src/rust-compiler/lexer.rs` — source text → tokens
- `src/rust-compiler/parser.rs` — tokens → AST
- `src/rust-compiler/ast.rs` — the AST and type definitions
- `src/rust-compiler/typeck.rs` — typechecking; **this is where the language's correctness thesis is enforced**
- `src/rust-compiler/codegen.rs` — typed AST → LLVM IR → native object. **The only file that touches LLVM.**
- `src/rust-compiler/main.rs` — the `burxt` CLI driver

**Keep the front end platform-independent.** Only `codegen.rs` may know about LLVM or any target detail. This separation is what makes cross-platform support a configuration problem rather than a rewrite; please preserve it.

## The working discipline

Burxt is developed in small, verified increments. Please follow the same rhythm:

1. **Never leave the tree not compiling.** Build and run the tests after each change.
2. **Tests are the product.** Because Burxt's identity is "the compiler refuses to let dangerous things happen," the *rejection* tests matter as much as the acceptance tests. A feature PR should include (a) programs that must compile and produce expected output, and (b) programs that must be *rejected* with a clear, English error message.
3. **Error messages read as advice.** A rejection should tell the user what to do, not just what went wrong. Match the existing style.
4. **Match the existing code style** and the design recorded in `DESIGN.md`.
5. **Commit what you changed, not what your tree holds.** If more than one piece of work is in
   flight — yours and yours, or yours and a collaborator's — git will quietly commit all of it
   under whichever message you happened to write. It has four faces, and knowing three of them
   is not enough:

   | | |
   |---|---|
   | `git commit` | takes the **whole index**, whatever is in it |
   | `git commit --amend` | takes the whole index too — **regardless of any pathspec you used before it.** Fixing a typo in a message can undo a careful commit |
   | `git commit -- <path>` | takes the **working-tree** version of that path, not the staged one |
   | `git add <path>` | stages whatever that path currently holds, including edits that are not yours |

   Use `git commit -- <paths>`, then **read `git show --stat HEAD` before moving on** — the line
   counts, not just the names. A filename can look plausible in a commit about something else;
   `188 +++` beside it cannot, and that is the difference between the two sweeps that were caught
   and the one that went unnoticed for hours. If it lists a file you did not touch, stop:
   `git reset --soft HEAD~1` puts everything back in the index, losing nothing, provided you have
   not pushed.

   **And a diff is against the base you are standing on.** If your checkout is not on the branch
   you are landing to, `git diff` renders somebody else's later commits as *your* deletions — so
   the `--stat` looks like your own work and reads as plausible. This is the only one of these
   the `--stat` habit cannot catch by itself, because nothing in it looks foreign. Compare
   against the target instead: `git show <target-branch>:<path>` and diff your version against
   that. It happened here while this very paragraph was being written.

   **Read the whole `--stat`, not a list of places you expected trouble.** The sweep that went
   unnoticed for hours had already been checked — by confirming the commit held nothing under
   `src/`, `examples/` or one named test directory. All true, and it measured the wrong dimension:
   the file that carried somebody else's work was `tests/runner.rs`, which was not on the list
   because nobody thought of it as theirs. **A whitelist of places to check is not a check.** It
   can only ever find what its author already suspected.

   **Two people cleared that commit independently, by the same method, and both were right and
   both were wrong.** Each confirmed it held nothing under `src/`, `examples/` or the named test
   directory; each was correct; neither thought of `tests/runner.rs` as somewhere another person's
   work could be. Two independent auditors passing the same commit is not confirmation when they
   share the blind spot — it is the same check run twice.

   **And when a guard refuses, never make the expected value true.** The safe way to move a
   shared branch is the three-argument form, which fails rather than clobbers:

       git update-ref refs/heads/develop <new> <expected-old>

   It refused, correctly, because someone else had landed. The response was to add a line that
   set the local branch to whatever the remote said *first* — a compare-and-swap whose expected
   value was read from the thing being compared against — and then run the real one on a premise
   the first line had manufactured. That discarded a colleague's landing on the way past, and the
   second command reported success.

   **The refusal is the information.** A guard firing looks like the branch being wrong; it
   almost always means somebody else was right. Fetch, merge or rebase onto what is actually
   there, re-run the suite on that base, and announce again — the base has changed, so the
   approvals you were holding were given for a different tree.

   This one differs from the six above in a way worth naming. Those are things a tool does that
   you did not intend. This is a thing you do to a tool that is working: **a guard that can be
   satisfied by assignment is a guard with an off switch**, and the moment it fires is exactly
   when reaching for that switch feels like tidying up. It was reached for twice, the second time
   out of habit built the first time.

   **And do not un-stage another contributor's work either.** If a file holds hunks that are not
   all yours, leave the index alone and say so. Removing someone's work from the index without
   asking is no better than committing it without asking — the same act, in the other direction,
   and it is the one nobody thinks to check for.

   This is not hypothetical. It fired three times in one day here — twice unnoticed — and each
   time it attached one author's work to another author's commit message, so `git log -- <file>`
   answered a question about a subject the file has nothing to do with.

   **And when the work cannot be separated even in the file** — two authors' hunks interleaved in
   one `runner.rs`, say — every route through the index or the working tree takes both. That is
   true of porcelain and not of git. You can build a commit from a *known* tree plus one change,
   touching neither the index nor the checkout:

   ```sh
   git show HEAD:path/to/file > /tmp/base          # the committed version, nobody else's hunks
   #   ... apply only your change to /tmp/base ...
   blob=$(git hash-object -w /tmp/base)
   GIT_INDEX_FILE=/tmp/ix git read-tree HEAD
   GIT_INDEX_FILE=/tmp/ix git update-index --cacheinfo 100644,"$blob",path/to/file
   tree=$(GIT_INDEX_FILE=/tmp/ix git write-tree)
   commit=$(git commit-tree "$tree" -p HEAD -m "...")
   git update-ref refs/heads/<branch> "$commit"
   ```

   The real index is never opened, so other people's staged work is untouched — check it with
   `git status --porcelain` before and after and compare. **Then verify the result somewhere the
   contamination is not**: build and test in a throwaway `git worktree add --detach <tmp> <commit>`,
   because the shared tree cannot tell you whether what you *committed* compiles, only whether what
   you *have* does. Remove the worktree afterwards.

   This is worth knowing because *"nobody can commit this file"* sounds like a fact and is a
   property of the tools people reach for first. It was said here about a file that then merged
   between three authors with no conflict at all — the file was never unsplittable, it was
   **unattributable**, and those are different problems with different fixes.

   **When two pieces of work genuinely cannot be separated by path**, they are usually in the
   same file, and the answer is `git worktree add ../burxt-<topic> <branch>`: a second checkout
   with its own HEAD and index, sharing the object store. Put it **outside** the repository —
   a worktree inside it is an undeclared directory and fails
   `the_repository_layout_is_declared`. It also does something a shared tree cannot:
   **it builds against the branch you are actually targeting**, which is how a dependency on
   somebody else's uncommitted work gets caught rather than shipped.

6. **A passing suite is not evidence of the commit unless the tree IS the commit.** `cargo test`
   measures your working directory. If anything is uncommitted — a fix you made after staging, a
   file you meant to delete, a generated artifact — the result describes a tree that exists on
   your disk and nowhere else, and you will report it as though it described the branch.

   ```sh
   git diff --quiet HEAD -- . && cargo test --release
   ```

   This is the same rule the repository already has about binaries — *a binary is not evidence of
   the source it came from* — one level up, and it is a different category from the four faces
   above. Those are about what a **commit contains**; this is about what a **test result
   describes**. Both are an artifact attributed to a source it did not come from.

   It is not hypothetical either. A green run of 98 was published here, from a tree holding one
   uncommitted deletion; the same suite against the commit gave 97 and a failure. The number was
   honest, carefully measured, and about the wrong thing.

## Getting set up

See the build instructions in [README.md](README.md). In short: Rust via rustup, LLVM 18 dev libraries, `LLVM_SYS_181_PREFIX` pointed at your LLVM 18 install, then `cargo build` and `cargo test`.

## Submitting

1. Open an issue for anything beyond a small fix, so the approach can be agreed before you invest time.
2. Keep the PR focused on one thing.
3. Include tests, including rejection tests where the change affects what the compiler accepts.
4. Make sure `cargo build` is warning-free and `cargo test` is green.

## Licensing of contributions

Burxt is dual-licensed under MIT and Apache-2.0. By submitting a contribution, you agree it will be licensed under both, without additional terms. You retain copyright to your contributions.

## Code of conduct

Participation in this project is governed by the [Code of Conduct](CODE_OF_CONDUCT.md). Please be respectful and constructive.
