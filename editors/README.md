# Editor and platform support for Burxt

A language is not real to the people using it until their editor knows it. This
directory holds that half of the project.

> **The rule, learned the hard way in v0.0.99.** A change to the language is not finished
> until the grammar, the language server and the **packaged** `.vsix` have changed with it.
> A reader's first contact with Burxt is an editor, not `cargo test`, and a language that
> compiles correctly while looking broken *is* broken.
>
> Tests enforce it, and each exists because the thing it checks actually went wrong. **The list is
> not counted here**, because a count restated beside the thing it counts is a number that goes stale
> in one of the two places — as this sentence did, saying *three* above a table of four:
>
> | Test | What it caught |
> |---|---|
> | `editor_grammar_knows_every_keyword_the_compiler_does` | a keyword the grammar had never heard of |
> | `editor_grammar_highlights_every_declaration_the_examples_write` | `function (self)` — shipped v0.0.95, uncoloured until v0.0.99, because **a keyword list is not a grammar** |
> | `the_packaged_extension_matches_the_grammar_in_the_repository` | a `.vsix` built before a rename, so the editor coloured yesterday's language |
> | `the_language_server_checks_the_program_a_file_belongs_to` | five compiler modules squiggled as broken, and `main.bx` failing on its own `use` lines |
> | `an_unresolvable_import_is_reported_as_one` | an import that would not resolve, reported as a **syntax error on a valid line** — the server fell back to checking the raw buffer, where a `use` parses as an assignment |
> | `the_documented_install_command_names_the_file_pack_py_writes` | the install command in `README.md` naming version 0.1.3 while `package.json` said 0.1.4 — so the front door pointed at a file the packer does not write |
> | `the_manifest_grammars_cover_the_whole_vocabulary` | `burxt.package` and `burxt.lock` opening as plain text, and a grammar registered but not packaged |
>
> After touching the grammar, run `python3 vscode/pack.py` and bump the version in
> `vscode/package.json` — an installed extension does not upgrade to the same version number, and
> **the filename no longer tells you anything**: it is `burxt.vsix` at every version, deliberately, so
> the version lives only where a tool reads it.

| Piece | State | Where |
|---|---|---|
| TextMate grammar (highlighting) | **DONE** (v0.0.31) | `vscode/syntaxes/burxt.tmLanguage.json` |
| Language configuration (comments, brackets, indent) | **DONE** (v0.0.31) | `vscode/language-configuration.json` |
| `burxt.package` and `burxt.lock` highlighting | **DONE** (v1.4.0) — two languages, matched by filename | `vscode/syntaxes/burxt-package.tmLanguage.json`, `vscode/syntaxes/burxt-lock.tmLanguage.json` |
| VS Code extension | **DONE** (v0.0.31), live diagnostics (v0.0.34) — still no build step | `vscode/` |
| `burxt check` — front end only, for editors and CI | **DONE** (v0.0.31) | `src/rust-compiler/main.rs` |
| Diagnostics with line/column | **DONE** (v0.0.32) | `src/rust-compiler/diag.rs`, `burxt check --json` |
| Language server (`burxt lsp`) | **DONE** (v0.0.33) — diagnostics on change, hover (v0.0.35) | `src/rust-compiler/lsp.rs` |
| VS Code diagnostics + hover | **DONE** (v0.0.36) — hand-written LSP client, still no npm | `vscode/extension.js` |
| VS Code problem matcher (for tasks and CI) | **DONE** (v0.0.33) | `vscode/package.json`, `.vscode/tasks.json` |
| Neovim / Helix configs | **DONE** (v0.0.33) — diagnostics, no highlighting yet | `nvim/`, `helix/` |
| `.bx` file icon in the explorer | **DONE** (v0.0.42) — works under the default theme | `vscode/package.json` |
| Tree-sitter grammar (Neovim/Helix colour) | not written | see below |
| Hover, go-to-definition | not written | see below |
| GitHub language detection | blocked on a popularity gate, not on us | see below |

## Installing the VS Code extension

Package it and install it — no npm, no `vsce`, no bundler:

```bash
python3 editors/vscode/pack.py                                  # writes burxt.vsix
code --install-extension editors/vscode/burxt.vsix
```

`pack.py` is a .vsix writer in the standard library: a .vsix is a ZIP holding an OPC
content-types map, a VSIX manifest, and the extension under `extension/`. `vsce`
does more — linting, dependency bundling, marketplace checks — and all of it is for
publishing rather than installing, so none of it is needed here.

**Install rather than symlink.** A symlink into the extensions directory does work,
until something reads the extension registry and does not find you. An installed
extension is registered, versioned, upgradable and uninstallable through the normal
UI, and it is the shape every other extension has.

On a remote — WSL, SSH, a container — the manifest declares
`"extensionKind": ["workspace"]`, because the extension spawns the compiler and the
language server and therefore has to run where the code is rather than on the UI
side. **`pack.py` refuses to build a package whose manifest does not say so** rather than
supplying a default: the wrong value loads the extension on the UI side of a remote, where
the compiler it spawns does not exist, and a default is invisible exactly when it is wrong.

## `burxt.package` and `burxt.lock`

Both opened as plain text until v1.4.0, though every Burxt package on disk has them. They are
**two languages matched by filename**, not by extension — neither file has a suffix of its own,
and claiming `.package` would both miss these and claim someone else's.

**They do not reuse `source.burxt`, and that is the point.** `manifest.rs` comments its own
choice of comment syntax: `#` and not `//`, *because this is not Burxt source and reading it as
though it were is how someone ends up expecting interpolation in it*. Painting a manifest with
the Burxt grammar would teach exactly the confusion that decision exists to prevent. They are
also two languages rather than one with two filenames, because the vocabularies do not overlap —
`dependency` in a lockfile is refused, and one grammar for both would colour each file's mistakes
as though they were correct.

**The readability that was actually asked for is in the lockfile.** `write_lock` emits

```
package  bmx  https://github.com/andrecorugda/bmx  burxt-0.12.1  d9940651a8207986f2a5a3b7a9673e3245bf78ca
```

and the tag and the 40-character commit sit adjacent, reading as one blur. Those are the two
fields a person compares by eye when a fetch surprises them, so the commit gets a scope of its own
— but only when it really is 40 hex characters. A looser rule still colours a five-field line the
parser accepts, because the parser counts words and never checks the shape of the fifth: **a
grammar stricter than its parser reports a refusal that does not exist.**

**Both grammars do mark an unknown key invalid**, which colouring is normally the wrong tool for.
The exception holds here because the vocabulary is closed and the compiler says so in the refusal
itself — *"A manifest has `name`, `version` and `dependency` — and that is the whole grammar"*.
There is no list a grammar could be wrong about. `the_manifest_grammars_cover_the_whole_vocabulary`
reads that sentence out of `manifest.rs` and fails if a grammar does not know a word it names.

**No language server and no highlighting outside VS Code.** `helix/` and `nvim/` attach `burxt lsp`
and have no grammar at all — highlighting there needs tree-sitter, which does not exist yet — so
there is nothing for them to gain here. A manifest has no server because it has nothing a server
would add: every refusal already names the line.

Then reload the window. A `.bx` file should light up: money literals as numbers,
`Decimal<2, RoundHalfEven>` with the scale and the rounding contract distinct,
`{interpolation}` as embedded code, `\{` as an escape, and a bare `}` in a string
flagged as invalid — because the compiler refuses it too.

To package it as a `.vsix` instead (requires `npm i -g @vscode/vsce`):

```bash
cd editors/vscode && vsce package
```

## Getting the Burxt icon on `.bx` files

`contributes.languages[].icon` is the whole mechanism — the same one
`apex-stack.apex-alpine` uses — and it works under the **default Seti theme**. No
icon theme to install, nothing to switch, nothing lost.

**A correction worth keeping, because v0.0.41 got this wrong.** I claimed Seti
ignores language-contributed icons and shipped a file icon theme to work around it.
That was an assumption, and it was false. VS Code's own logic is:

```js
n = true                     // set when a theme defines languageIds
showLanguageModeIcons === true || (n && showLanguageModeIcons !== false)
```

Seti defines 83 `languageIds` and never sets the flag to `false`, so the second
clause is true and language icons apply to any language Seti does not itself cover —
which includes Burxt. The icon theme has been removed; it solved a problem that did
not exist, at the cost of every other file's icon.

The lesson is the one this project applies to the compiler: **check the mechanism,
do not reason about it from memory.** The answer was in the shipped bundle the whole
time.

## Other editors
## Other editors

The grammar is a standard TextMate grammar, which is the same artifact most
editors want:

- **Sublime Text / TextMate**: point at `burxt.tmLanguage.json` directly.
- **Zed**: TextMate grammars are supported via a language extension.
- **Neovim / Helix**: these want a **tree-sitter** grammar instead, which is a
  separate piece of work and not yet written. Recorded, not pretended.
- **JetBrains**: TextMate bundles are supported under
  *Settings → Editor → TextMate Bundles*.

## What is deliberately NOT here yet

**A tree-sitter grammar.** Neovim and Helix want one for *highlighting* — they
get diagnostics from the LSP today but no colour. It is the largest remaining gap
in editor support; writing two grammars that can disagree is also a real cost,
which is why the TextMate one came first (it serves VS Code, Sublime, Zed,
JetBrains, and Linguist from a single file).

**Formatting (`burxt fmt`).** A formatter is a design decision about the
language's canonical shape, not a tooling detail — it deserves its own spec
rather than being improvised.

## The language server

`burxt lsp` speaks the Language Server Protocol over stdio. It typechecks the
buffer you are editing — not the file on disk — and publishes one diagnostic or
none, because the compiler stops at the first error and pretending to a list would
be a lie. Publishing the empty list matters as much as publishing an error: it is
what clears the squiggle when the code becomes valid again.

```bash
cargo build && sudo ln -sf "$PWD/target/debug/burxt" /usr/local/bin/burxt
```

**Neovim** — `editors/nvim/burxt.lua`, no plugin manager and no `nvim-lspconfig`
needed:

```lua
vim.cmd('source /path/to/burxt/editors/nvim/burxt.lua')
```

**Helix** — append `editors/helix/languages.toml` to
`~/.config/helix/languages.toml`.

**Zed, Emacs (eglot/lsp-mode), Sublime LSP, Kate** — any LSP client works; the
command is `burxt lsp` and the file type is `.bx`.

**VS Code** — errors as you type *and* hover, with **no `npm install`**. The
extension is a hand-written LSP client (about a hundred lines of framing and
request bookkeeping) rather than `vscode-languageclient`, because that package
would bring npm, a lock file and a bundling step, and the property worth
protecting is that this directory can be copied into place and used.

It talks to the same `burxt lsp` every other editor uses. That matters more than
the line count: there is one place where "what does the compiler know about this
buffer" is answered, and every editor asks it the same way.

Set `burxt.path` in settings if the compiler is not on `PATH` and not at
`./target/debug/burxt` in the workspace. *Burxt: Restart Language Server* is in the
command palette.

A `$burxt` problem matcher is also contributed, for tasks and CI: `Ctrl+Shift+B`
runs `burxt check` on the current file and fills the Problems panel;
`.vscode/tasks.json` here is the working example. `burxt check <file> --json`
remains supported for exactly that.

**The client is tested, not just inspected.** VS Code cannot be scripted here, but
the client can: `node editors/vscode/test/harness.js` stubs the `vscode` module,
drives the real extension against a real server, and checks the whole loop —
diagnostics appearing, clearing when the code is fixed, and hover returning the
type with its rounding contract. `cargo test` runs it when node is present and says
so loudly when it does not.

### What the server does NOT do yet

- ~~**Hover**~~ **shipped in v0.0.35**: the exact type, plus a sentence on what it
  guarantees — a `Decimal<2, RoundHalfEven>` hover names the scale *and* says
  results round half to even. It reports the SMALLEST expression under the cursor,
  and knows types only up to the first error, because the compiler stops there.
- **Go-to-definition**, which needs the compiler to keep name resolution rather
  than only its result.
- ~~**More than one error at a time**~~ **shipped in v0.0.37**: the typechecker
  recovers per statement, so every type error is published at once. Lexer, parser
  and declaration errors still arrive alone — guessing where a malformed statement
  ends would invent errors instead of finding them.
- **Incremental sync.** The server asks for full-document sync deliberately:
  applying incremental text edits correctly is fiddly, and a server that corrupts
  its own copy of the buffer reports errors about code you never wrote.

## GitHub language detection

`.bx` files currently show as plain text on github.com. Burxt is not in
[github/linguist](https://github.com/github-linguist/linguist) yet, and that is a
**popularity gate rather than a technical one**.

### The bar, quoted rather than remembered

An earlier version of this section said "a few hundred public repositories". That
was wrong, and wrong in the flattering direction. Linguist's
[`CONTRIBUTING.md`][linguist-contrib] states the requirement as *files*, not repos:

> - at least **2000 files** per extension […] indexed in the last year […] excluding
>   forks, for extensions […] expected to occur more than once per repo, like Ruby's
>   `.rb` extension.
> - at least 200 files […] for extensions or filenames expected to only occur **once**
>   per repo, like a `Makefile`.
> - the results should show a reasonable distribution across unique `:user/:repo`
>   combinations […] If particular users are showing a high proportion of the results,
>   **for example the primary language owner, we will filter out those users** using
>   `-user:<username>`.

`.bx` is a per-file extension, so the number is 2000, not 200. And the last clause is
the one that actually bites: this repository is the primary language owner's, so every
`.bx` file in it counts for **zero**. The evidence has to come from repositories that
are not ours. Linguist also states plainly that it does "not accept PRs for very new or
hobby languages, and will close any such PRs" — so filing early is not a neutral act.

The query the PR template asks for wants keywords unique to the language, so that
`.bx` files belonging to something else do not pad the count. `RoundHalfEven` is the
best single token we have — it is a Burxt rounding contract, it appears in 8 of the 30
`.bx` files here, and nothing else spells it:

```
https://github.com/search?type=code&q=NOT+is%3Afork+path%3A*.bx+RoundHalfEven
```

Broader, if the count above is short: `Decimal<` reaches 18 of 30 files, and the
quoted-path import form `use "option.bx"` is unlike any other language's `use`.

Merges land only shortly before a Linguist release, every few months.

### Verified clear, 2026-08-01

Two things that would have been rejection causes are checked and clean:

- **`.bx` is unclaimed.** No language in Linguist's `languages.yml` on `main` uses the
  extension, so no `heuristics.yml` entry and no `test_heuristics.rb` case is needed —
  a missing heuristic test is the single most common reason these PRs bounce.
  *Caveat with a clock on it:* BoxLang (Ortus Solutions) also uses `.bx`. It has no
  Linguist issue or PR today. If it lands first, Burxt inherits the heuristic work.

  **Decided, so it stays decided: `.bx` is not being given up.** `.bxt` is unclaimed
  and a rename is cheap *today*, which makes it a tempting suggestion — but the
  trigger for it would only fire once Burxt has outside adoption, and by then the
  `.bx` files in strangers' repositories *are* the Linguist evidence. Renaming at
  that point discards the evidence and splits the ecosystem across two extensions
  permanently. Supporting both is worse still: Linguist's bar is **per extension**,
  so two extensions halve the count on each. If we end up sharing `.bx`, the answer
  is a heuristic, not a retreat.
- **The grammar survives PCRE.** Linguist compiles grammars with PCRE where TextMate
  uses Oniguruma, and rejects grammars whose patterns fail. All 57 regexes in
  `burxt.tmLanguage.json` were scanned for the Oniguruma-only constructs (`\h`, `\y`,
  `(?~`, `\K`, variable-length lookbehind): **zero hits**.

### The submission kit

- [x] An open-source TextMate grammar (`vscode/syntaxes/burxt.tmLanguage.json`,
      scope `source.burxt`) — the artifact Linguist actually consumes.
- [x] Unambiguous extension: `.bx`.
- [x] A licence Linguist accepts (MIT OR Apache-2.0).
- [x] A brand colour taken from the artwork: `#E8502A` (see `assets/README.md`).
      The PR template requires a *rationale* for the colour, not just the hex; ours is
      that it is sampled pixel-wise from `burxt-b-favicon-512.png`.
- [ ] **Two or three** real-world sample programs — not the 230+ under `tests/`, which
      an earlier version of this list offered. `bundle exec rake samples` feeds a
      Bayesian classifier, and burying it in near-identical fixtures teaches it the
      shape of our test suite rather than the shape of the language. `examples/tour.bx`,
      `examples/invoice.bx` and `examples/parser.bx` are the intended three.
- [ ] **A standalone grammar repository.** `script/add-grammar <url>` adds the grammar
      repo to Linguist *as a git submodule* and copies its licence file. Pointing it at
      this repository would vendor the whole compiler into Linguist, and our licence
      files are named `LICENSE-MIT` / `LICENSE-APACHE` rather than a plain `LICENSE`,
      which the script's detection may not find. The grammar needs its own small repo
      with a conventional licence file before any PR is opened.
- [ ] Usage across enough public repositories — 2000 `.bx` files, ours excluded.
      **This remains the only item that cannot be bought with work here.**
- [ ] The `languages.yml` entry, submitted as a PR:

```yaml
Burxt:
  type: programming
  color: "#E8502A"
  extensions:
    - ".bx"
  tm_scope: source.burxt
  ace_mode: text
  language_id: <assigned by script/update-ids>
```

**Deliberately not worked around.** `.gitattributes` could force `.bx` to be
counted as some other language and the repo would look "detected" — but it would
be *wrongly* detected, and a wrong label is worse than no label. That is the same
standard the language itself applies to a silently wrong number.

[linguist-contrib]: https://github.com/github-linguist/linguist/blob/main/CONTRIBUTING.md#language-extension-and-filename-usage-requirements
