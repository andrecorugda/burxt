# The half of a language that lives outside the compiler

*Milestone log, v0.0.31 – v0.0.37. The design these versions serve is in [DESIGN.md](../../DESIGN.md); the whole log is indexed [here](README.md).*

Source spans, carets, a language server, live diagnostics in VS Code with no dependencies at all, hover, and every mistake reported at once instead of the first one.

### v0.0.31: editor support — the half of a language that lives outside the compiler

A language is not real to the people using it until their editor knows it. This
version is that half, and it is deliberately the *declarative* half first.

**A TextMate grammar** (`editors/vscode/syntaxes/burxt.tmLanguage.json`) plus a
language configuration, packaged as a VS Code extension with **no JavaScript and
no build step** — it installs by being symlinked into place. The grammar knows
what makes Burxt Burxt, not just generic C-family shapes:

- `$19.99` and `8.25%` are numeric literals in their own right.
- `Decimal<2, RoundHalfEven>` highlights the scale and the rounding contract
  distinctly, because the contract is part of the type.
- `{interpolation}` inside a string is embedded code, `\{` is an escape, and a
  bare `}` is flagged **invalid** — the same thing the lexer does.
- `return tail f(...)`, `region name`, `dyn Trait`, and
  `amount: Decimal<2> as scaled` each read as what they are.

The same grammar is the artifact GitHub's Linguist consumes, so this is also step
one of `.bx` files being coloured on github.com.

**Verified, not assumed.** The grammar was run through the real TextMate engine
(`vscode-textmate` + Oniguruma) over a program exercising every construct, and
the token scopes were read back. A dependency-free test then locks the invariant
permanently: **every keyword and builtin the compiler knows must appear in the
grammar's patterns** — extracted from `src/lexer.rs` and `src/typeck.rs` at test
time rather than duplicated, because a duplicated list is the thing that drifts.
The test searches the grammar's *patterns* and not its prose, which was found by
mutation: the looser first version passed after the `tail` rule was deleted,
because the word survived in a comment.

**`burxt check`** — parse and typecheck only, no LLVM context and no linker. This
is what an editor or a CI gate calls, so it has to stay the cheapest way to ask
"is this program legal?".

**Two things this exposed, both fixed here:**

- **Nothing was checking the examples.** They are the first thing a newcomer
  reads, and they could rot silently while the suite stayed green. Every
  `examples/*.bx` now has to typecheck. Data files that other examples *read*
  moved to `examples/inputs/` — a directory rather than an exception list,
  because exception lists rot too.
- **The README described a version of Burxt that no longer existed** (no enums,
  no regions, no tail calls, no boundary exactness). Refreshed, since it is the
  front door.

**What is honestly NOT here:** diagnostics in the editor. Every compiler error
today is a precise sentence with **no position attached** — fine in a terminal,
useless to an editor, which needs a line, a column and a length to underline. So
source spans are the next piece of work, and an LSP after that. Building an LSP
first would be a shell with nothing inside it. A tree-sitter grammar (Neovim,
Helix) and a formatter are also recorded as not-built rather than implied;
`editors/README.md` holds the dependency order and the Linguist checklist,
including why `.bx` is not mislabelled as another language to fake detection.

### v0.0.32: errors that know where they are

```text
error: `*` on Decimal<2> needs an explicit rounding contract, because the exact
       result can have more than 2 decimal places. Declare one in the type, e.g.
       Decimal<2, RoundHalfEven> or Decimal<2, RoundHalfUp>.
 --> invoice.bx:3:1
  |
3 | let total: Decimal<2> = price * rate;
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

Burxt's errors were always sentences a person could act on. What they lacked was
a **position** — fine in a terminal, useless to an editor, which needs a line, a
column and a length to underline. This is that missing half, and it is the
prerequisite the previous version named for everything editor-facing.

**Spans are byte ranges, and lines are a presentation concern.** The lexer knows
offsets for free; `LineIndex` converts to line/column once, at the edge. Storing
line/column everywhere instead would mean every layer agreeing on how to count a
tab. Columns count **characters**, so a `café` earlier on the line does not push
the caret one place right of what the reader sees.

**The interesting part is how little the error sites changed.** There are roughly
200 `Err(format!(...))` sites across the parser and typechecker, and not one of
them threads a span. Instead each stage attaches the position **once, at its
boundary**:

- The parser fails fast, so the token under the cursor when the error surfaces
  *is* the token the message is about.
- The typechecker records where it is on entering a statement or a top-level
  item, and attaches that on the way out. A nested statement naturally yields the
  **most precise** position, because it was the last thing entered.

That is why this landed as a refactor rather than a rewrite: the position was
recoverable from control flow that already existed.

**`--json` diagnostics.** `burxt check file.bx --json` emits one JSON object per
diagnostic, carrying 1-based line/column for humans *and* 0-based LSP positions,
because converting between them in the consumer is where off-by-ones live. Any
editor with a problem matcher can show squiggles today, without an LSP.

**A test that found five real bugs the moment it was written.** Every program in
`tests/fail/` is now required to report a position that points at *code* — not at
a comment or a blank line, which is the tell for a span that was never set.
Five of 226 failed: four validation paths (array returns, recursive structs,
incomplete impls, `dyn` returns) reported line 1 because they check *items*
rather than statements, and one pointed at the empty line after a file ending in
a newline. All five fixed — item passes now record the item's span, and an error
at end-of-file is reported on the last line with content, because "unexpected end
of file" pointing at a blank line is technically true and useless.

**A self-inflicted bug worth recording**, because the class recurs: adding
offset tracking meant routing every `self.chars.next()` through a `bump()`
helper — and the mechanical replacement rewrote the call *inside `bump` itself*,
so it called itself forever. Every program stack-overflowed instantly. The lesson
is the same one the codegen match-arm edits taught: **a global replace whose
pattern also matches the replacement's own body is a trap**, and the fix is to
check the helper after replacing, not to trust the sed.

**Deferred honestly:** expression-level spans. A type error underlines the whole
statement rather than the offending sub-expression, which is right about the line
and coarse within it. Also, a diagnostic inside a `{interpolation}` carries the
message but points at the string literal, because the interpolated fragment is
re-lexed on its own and its offsets are relative to the fragment. Both are
refinements of a working position, not missing positions.

### v0.0.33: a language server

```bash
burxt lsp      # diagnostics as you type, in any LSP-speaking editor
```

Positions existed as of v0.0.32, so the server has something to serve. It
typechecks the **buffer**, not the file on disk — which is the entire point of an
editor integration — and publishes one diagnostic or none.

**One diagnostic, honestly.** The compiler stops at the first error, so the server
does not pretend to a list. Reporting several is a *compiler* change (error
recovery), not a server change, and it is recorded that way so the limitation is
not mistaken for a server bug. **Publishing the empty list matters as much as
publishing an error**: it is what clears the squiggle when the code becomes valid,
and a server that only ever reports problems looks correct in a unit test while
leaving stale underlines in a real editor. The end-to-end test asserts exactly
that sequence — open valid (empty), break it (one error at the right line), fix it
(empty again).

**A JSON reader, written rather than depended on.** The compiler has exactly one
dependency (LLVM) and that restraint is worth keeping. The alternative people
reach for at this size — finding fields with string search — is wrong the moment a
document contains a quote or a backslash, which Burxt source does constantly. A
language server that mangles the buffer it was sent is worse than none. So
`src/json.rs` is a small, correct reader and writer, including surrogate pairs
(that is how an emoji in a document arrives) and integers that do not serialize as
`1.0` (some clients are strict). Its tests cover the malformed inputs too, because
a server that panics takes the editor's language support down with it.

**Details that are easy to get wrong and were tested instead of assumed:**

- `Content-Length` counts **bytes**, not characters — a message with a non-ASCII
  identifier would otherwise be truncated at the client.
- An unknown **request** must be answered (`-32601`), or a real client waits
  forever. An unknown **notification** must be ignored. The `id` field is the
  only difference.
- Full-document sync is requested deliberately: applying incremental text edits
  correctly is fiddly, and a server that corrupts its own copy of the buffer
  reports errors about code nobody wrote.

**Reaching editors.** Neovim (`editors/nvim/burxt.lua`, no plugin manager) and
Helix (`editors/helix/languages.toml`) attach the server directly; Zed, Emacs,
Sublime LSP and Kate need only the command. VS Code is the awkward one: launching
a server requires `vscode-languageclient`, which means npm and bundling — a real
cost against the extension's current property of being copyable with no
toolchain. Until that is paid, VS Code gets squiggles from a **problem matcher**
(`$burxt`) plus a task, which is declarative and needs no build step. The matcher
was verified against real compiler output rather than by reading the regex.

**Honest gaps, recorded rather than implied:** hover (the first thing worth adding
— `Decimal<2, RoundHalfEven>` on hover is worth more in Burxt than in most
languages), go-to-definition, and a tree-sitter grammar so Neovim and Helix get
colour and not only errors.

### v0.0.34: live diagnostics in VS Code, with no dependencies at all

Errors appear as you type, and the extension is still a directory you can copy
into place — no `npm install`, no `node_modules`, no bundler.

**How, given that an LSP client normally means npm.** It does not use the LSP. The
extension is plain CommonJS against the `vscode` API, which the editor injects at
runtime, and it runs `burxt check - --json`, feeding the buffer on **stdin**. Same
squiggles, no toolchain. `burxt lsp` remains the real server for every other
editor; switching VS Code to it buys hover and go-to-definition *when those exist*,
at the cost of a build step — worth paying then, not now.

**`burxt check -` reads the program from stdin**, which is the piece that made
this possible. What an editor has in its buffer is not what is on disk, and
checking the file would report errors the user already fixed. `run` and `build`
refuse `-`, because there would be no name for the executable.

**A wire format has consumers, so it is now tested as one.** The `--json`
diagnostic is read by the extension and will be read by CI gates. Renaming a field
would break them *silently* — the extension would simply stop showing squiggles,
with no error anywhere. So one test asserts the field names on **both sides at
once**: that the compiler emits them, and that `extension.js` reads the same ones.
It also asserts the positions stay 0-based, and that the extension invokes the
stdin form rather than checking the file on disk.

**Verified before shipping**, by driving the extension's exact pipeline from node:
spawn the compiler, feed a buffer, convert the JSON to a range, and print what
that range underlines. It underlines `let total: Decimal<2> = price * rate;` —
the offending statement, not the file. And a valid buffer yields zero diagnostics,
which is what clears the squiggle.

**Not fixed here, and not hidden:** one error at a time (a compiler change, error
recovery), and statement-level rather than expression-level underlining. Both
apply to every editor path, so they are recorded once rather than per client.

### v0.0.35: expression spans, sharper carets, and hover

```text
error: in the call to `tax`, argument 1 must be Decimal<2>, but it has type Int
 --> invoice.bx:3:11
  |
3 | print(tax(n) + $1.00);
  |           ^
```

Statement spans put the caret on the right line (v0.0.32). Expression spans put it
under the thing that is actually wrong — and they are what makes **hover** possible
at all, since answering "what is the type here?" means knowing which expression
*here* is.

**How the caret finds the smallest wrong thing.** `check_expr` became a thin
wrapper that, on failure, claims the position **unless something further in has
already claimed it**. A child's wrapper runs before its parent's as the error
propagates outward, so the innermost failing expression wins automatically — no
error site had to be touched. Where a parent's own check fails over children that
were each individually fine (a wrong argument, a value that disagrees with its
declared type), the parent says so explicitly with `blame(span)`, because there the
rule would be wrong: `let bad: Int = it.price;` should underline `it.price`, not
the whole line.

The bookkeeping lives in a `Cell`, not behind `&mut self`. Expression checking is
`&self`, and threading mutability through every checker method to carry a
diagnostic detail would claim it was part of the checking. It is not.

**Hover, and why it is worth more in Burxt than elsewhere.**

```text
Decimal<2, RoundHalfEven>

Exact decimal, 2 decimal places. A result that needs rounding rounds half to even
(banker's rounding).
```

The type names the scale; the sentence names what happens when a result does not
fit that scale, which is the whole question this language exists to make visible.
`CDouble` says a Decimal may not cross as one. A bare `Decimal<2>` says any
operation that could round is a compile error until a contract is declared.

The checker now records `(span, type)` for every expression it gets through, and
hover picks the **smallest** span containing the cursor — because expressions nest,
and the cursor on `qty` in `price * qty` should say `Int`, not the product's type.

**Two honest limits, both tested rather than footnoted:**

- Hover knows types **up to the first error and nothing past it**, because the
  compiler stops there. So hover goes quiet below a mistake and returns when it is
  fixed. That is error recovery's job, not the server's.
- The `let`-mismatch caret moved from the whole statement to the value, which
  broke two tests that had recorded the old, coarser positions. Both were updated
  to the sharper expectation — worth noting because a test that encodes a position
  is exactly the test that should fail when positions improve.

**And one test caught its own premise being wrong:** the end-to-end session used
`textDocument/hover` as its "unsupported method" probe. Hover is supported now, so
the probe moved to `textDocument/definition` and the test gained an assertion that
hover actually answers with a type.

### v0.0.36: VS Code speaks to the language server

Hover shipped for every LSP-speaking editor in v0.0.35 — except VS Code, which was
on a private `burxt check --json` path. Now it uses the same server as everyone
else, and still needs **no `npm install`**.

**A hand-written LSP client, about a hundred lines**, instead of
`vscode-languageclient`. That package would bring npm, a lock file and a bundling
step, and the property worth protecting is that `editors/vscode/` is a directory
you copy into place and use. What the client has to get right is small and
well-defined: frame messages out, unframe them in, match responses to requests by
id, and pass notifications along.

**The one detail that decides whether it works: buffer BYTES, not a string.**
`Content-Length` counts bytes, so accumulating stdout as a string and slicing on
that count corrupts every message containing a non-ASCII character — and Burxt
programs contain `café` and `€` in string literals routinely. The test asserts
`Buffer.concat` is used, with the reason written next to it.

**Why using the server matters more than the line count:** there is now exactly one
place where "what does the compiler know about this buffer" is answered, and every
editor asks it the same way. The `--json` path stays supported for tasks and CI —
`.vscode/tasks.json` and the `$burxt` problem matcher both use it — but it is no
longer a second implementation of the editor experience.

**The client is tested rather than inspected.** VS Code cannot be scripted here;
the client can. `editors/vscode/test/harness.js` stubs the `vscode` module, drives
the real `extension.js` against a real `burxt lsp`, and checks the whole loop:
a valid buffer publishes an empty list, a broken one publishes exactly one
diagnostic positioned at the offending value, fixing it clears the squiggle, hover
returns `Decimal<2, RoundHalfEven>` with its contract explained, and hover on
whitespace returns null rather than a guess. `cargo test` runs it when node is
available and **says loudly when it skips** — the Rust suite must not require a
JavaScript toolchain, but a check this valuable should not quietly not run.

These are exactly the failures that look fine on inspection: a message split across
chunks, a byte length applied to a string, a promise that never resolves.

### v0.0.37: every mistake at once

```text
error: type mismatch in `let wrong`: declared Bool, but expression has type Int
 --> many.bx:3:19
  |
3 | let wrong: Bool = qty;
  |                   ^^^

error: type mismatch in `let bad`: declared String, but expression has type Decimal<2>
 --> many.bx:5:19
  ...

3 errors
```

The typechecker no longer stops at the first problem. It records it, recovers, and
carries on — so a file with three mistakes reports three, in source order, instead
of making the reader fix one, recompile, and discover the next five times over.

**Burxt turns out to be unusually good at this, for a reason worth recording:
every `let` declares its type.** The hard part of error recovery elsewhere is that
a failed initializer leaves a binding with no type, so every later use of it
produces a second, invented error — the cascade that makes recovery worse than
useless. Here the annotation was mandatory all along, so a statement that fails
still contributes a **correctly typed name**, and the rest of the function checks
against the type the author asked for. The test asserts both halves: all three
real errors, and *nothing else* — no "unknown name" noise from the two later
statements that use the failed bindings.

**Two things deliberately still report alone:**

- **Lexer and parser errors.** Recovering a token stream means guessing where a
  malformed statement ends, and a wrong guess *invents* errors rather than finding
  them. Asserted by its own test so the distinction stays a decision.
- **Declaration errors** — a bad struct field, an unknown type in a signature.
  Continuing past those means checking a function whose types are unknown, which
  produces confident nonsense.

**Two follow-on effects, one of which reversed an earlier test:**

- **Hover now works below a mistake**, not just above it. The v0.0.35 test asserted
  the opposite ("hover goes quiet below a mistake, and comes back when it is
  fixed") and was correct at the time; recovery is what changed it. The test now
  asserts hover answers on *both* sides of an error.
- **The return-path proof had to become conditional.** A body with a failed
  statement produces no `TypedStmt` for it, so "must end by returning" would fire
  as a second complaint about the same mistake. It now runs only when the body
  checked cleanly.

The language server publishes all of them, so an editor underlines every place at
once; `--json` emits one object per line, already in source order, each error only
once.
