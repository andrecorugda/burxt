# `allocates`, `pure`, and the mark

*Milestone log, v0.0.38 – v0.0.42. The design these versions serve is in [DESIGN.md](../../DESIGN.md); the whole log is indexed [here](README.md).*

Functions that allocate in the caller's region, `pure` as a checked claim, and Andre's artwork wired into the repository and the editor — including a version spent chasing an icon that had been declared correctly all along.

### v0.0.38: functions that allocate in the caller's region

```text
fn describe(line: Int, byte: Int) -> String allocates {
    return "line " + to_string(line) + ": unexpected byte " + to_string(byte);
}

region source { print(describe(3, 108)); }
```

**A helper could not build a String and return it, and that blocked the
self-hosted compiler more than anything else.** Every error message, every rendered
type name, every `Int`-to-text conversion in a library function needs exactly this
shape — and both routes were closed. A plain function body has no region, so the
allocation was refused; opening one inside the function meant the result could not
be returned, because that region ends at the closing brace.

**The fix rests on something that was already true.** A function called from inside
a region *already* allocates in that region — the allocator is a bump pointer, and
the mark belongs to the caller. So the value never outlives its region and M1's rule
was satisfied all along. The compiler simply had no way to know a function intended
this, and refused conservatively.

`allocates` on the signature says it: **this function builds values in its caller's
region.** It may allocate without opening one, it may return what it built, and
every call site must have a region open.

**Declared, not inferred, and the reason matters.** Inference is entirely possible —
walk the call graph, propagate. It was rejected because it would be the only
invisible contract in the language. Every other guarantee Burxt makes is written
where it applies: a rounding contract in the type, `dyn` at the dispatch site,
`tail` at the call, `as scaled` at the boundary. A function that quietly acquires a
requirement on all its callers because someone added a `+` deep inside it is the
action-at-a-distance the rest of the language refuses. Being declared also makes it
decidable in one pass, since signatures are hoisted before any body is checked.

It is **not a lifetime** — no name, no scope relation, nothing to unify. One bit,
which is why it can be a keyword rather than a parameter, and why M1's "no lifetimes
in signatures" still holds.

**What still fails, and must:** a value built inside a `region` block the function
itself opened cannot be returned (that region really does end); and a caller cannot
return an `allocates` call's result out of its own region, because such a call now
*counts* as allocating at the call site, so the caller's escape rules govern it
exactly as if it had built the value itself.

**Codegen did not change at all.** If it had needed to, the reasoning above would
have been wrong.

**The payoff, in the self-hosted lexer:** `examples/lexer.bx` now reports
`byte 64 at offset 177 starts no token` — a message *built* by Burxt code rather
than printed piecemeal. The requirement is visible up the whole chain: `unexpected`,
`show` and `tokenize` each say `allocates`, because each calls something that does.

**Two things this shook out:**

- **Every "needs a region" message is now written once**, in one helper, and offers
  both fixes. They had drifted into four slightly different sentences.
- **A `match` arm's pattern error pointed at the previous arm.** Checking the
  arm above had moved the recorded position. Found the honest way: by shadowing a
  name in `examples/lexer.bx` and being sent to the wrong line.

Spec: `spec/M1a-CALLER-REGION-FUNCTIONS.md`, with its own must-NOT list — no
inference, no region names in signatures, no implicit region at a call site, no
`allocates` on `extern fn`, and no codegen change.

### v0.0.39: `pure` — reproducibility the compiler checks (NOVELTY §2, slice 1)

```text
pure fn interest(balance: Decimal<2, RoundHalfEven>, rate: Decimal<4>)
    -> Decimal<2, RoundHalfEven>
{
    return balance * rate;
}
```

> **This function's result depends only on its arguments. The compiler checked.**

Auditors and regulators care intensely whether a calculation is reproducible, and
today the honest answer in every language is *"we believe so."* A hidden
`DateTime.Now`, a locale-dependent parse, or a config lookup three calls down
silently makes a computation irreproducible, and nothing catches it. Burxt already
guarantees the arithmetic is exact and byte-identical across targets; `pure` extends
that to the **inputs** — nothing may enter the calculation except through a
parameter.

**The register listed this as needing an effect system first. It needed less than
that**, because v0.0.38 introduced the first declared effect marker (`allocates`).
`pure` is the same shape pointed the other way: a marker that **forbids** rather than
permits. A `pure fn` may not print, may not read a file, may not call into C, and may
not call a function that is not itself `pure` — which makes the property transitive
without inferring anything.

**What it may do, deliberately: allocate.** A bump allocator observes nothing about
the outside world and returns the same layout for the same sequence of calls, so
`pure fn render(...) -> String allocates` is legal and useful — a pure function that
builds a string. The two markers compose because they describe different things: one
says *where memory comes from*, the other says *what may influence the result*.

**Purity constrains the callee, never the caller.** Any function may call a pure one,
nothing propagates upward, and the marker can be adopted one function at a time.

**Honest about today's teeth, because overselling this would be worse than not
shipping it.** Burxt has no clock, no random, no locale, no environment access and no
ambient configuration. So the rules bite on **I/O and the FFI** — which is where
nondeterminism actually enters a Burxt program today — and are otherwise a **forward
guarantee**: when a clock is added it will be added *behind* this rule rather than in
front of it.

**Deliberately not done:** no inference (`pure` is written where it applies, like
every other guarantee in the language), no opt-out inside a pure function, and **no
purity-driven optimisation**. Memoisation and common-subexpression elimination are
things this guarantee enables, and doing them now would mean the marker changes
behaviour as well as legality. In the version that introduces it, it must only ever
change what compiles.

Methods cannot carry the marker yet, so a pure function cannot call one — refused
with that reason stated, and `pure fn (self: T) ...` is refused at the parser with
the same explanation rather than a confusing message about tokens.

Spec: `spec/N2-PURE-FUNCTIONS.md`.

### v0.0.40: the brand, in place

Andre's artwork, organised and wired in. The mark is `><` — two chevrons converging
— and the wordmark is `Burxt` with that mark **as** its `x`. Reading the name and
seeing the logo are one act, and what the mark means is *exact*: two things meeting
at a position that is fixed rather than approximate.

- `assets/` holds the kit: the icon at favicon sizes, transparent and on an obsidian
  tile, a multi-size `.ico`, the wordmark, and lockups on transparent, light and
  dark grounds.
- The VS Code extension uses the **tile** for its marketplace icon (the extensions
  list shows it on its own background, so a filled tile is right) and the
  transparent 48px icon for `.bx` files in the explorer. Copper reads on both light
  and dark themes, so one file serves both.
- The README banner switches on `prefers-color-scheme`.

> **Superseded in v0.0.69**, when Andre replaced the artwork with the `b` mark. The
> method below is unchanged — sample, do not eyeball — but the copper is now ember
> `#E8502A`. The paragraph is kept as written because the reasoning still holds.

**The palette was sampled from the artwork, not eyeballed**: copper `#b26436`,
obsidian `#232320`, read pixel by pixel out of `burxt-favicon-512.png`. Anything
that needs the brand colour as *text* — the GitHub Linguist entry, a future
stylesheet — now uses the value the artwork actually contains, instead of the
placeholder green I had guessed earlier.

The extension keeps its own copies of two files, because VS Code resolves
contributed paths relative to the extension directory rather than the repository;
`assets/README.md` records the two `cp` commands to re-run if the artwork changes,
next to Andre's original notes rather than edited into them.

### v0.0.41: the mark on `.bx` files

The extension declared an icon for the `burxt` language in v0.0.40 and nothing
appeared, which is worth recording because it looks like a bug and is not.

**VS Code has no supported way to add one icon on top of another icon theme.** A
file icon theme is monolithic. The default **Seti** theme ignores
language-contributed icons entirely, and the built-in **Minimal** theme does too —
its `languageIds` map is literally empty, which I checked in the shipped theme file
rather than assuming. So the declaration alone can never show anything.

So the extension now ships a **file icon theme**: the copper mark for `.bx`, and a
plain document, folder and open-folder for everything else. It sets
`showLanguageModeIcons: true`, so any language that contributes its own icon gets
it — and the reason a default document glyph is needed at all is that **zero
built-in languages contribute one** (also measured, by scanning every built-in
extension's manifest). A `showLanguageModeIcons` theme with no fallback would leave
every other file blank.

Deliberately minimal, and it says so in the docs rather than pretending: this is not
an attempt at a four-hundred-glyph icon set, it is three utility shapes and the
brand mark. `editors/README.md` records the two alternatives that keep rich icons
for everything else — `vscode-icons`' custom icon folder, or any theme that already
opts into language icons.

This repository turns the theme on in `.vscode/settings.json`, which is
workspace-scoped: it applies here, nowhere else, and one deleted line reverts it.

### v0.0.42: a real extension, and a correction

**v0.0.41 was wrong, and the way it was wrong is worth more than the fix.** I claimed
the default Seti theme ignores language-contributed icons, and shipped a whole file
icon theme to work around it — at the cost of every other file's icon. That claim was
an assumption. VS Code's own logic, read out of the shipped bundle:

```js
n = true                     // set when a theme defines languageIds
showLanguageModeIcons === true || (n && showLanguageModeIcons !== false)
```

Seti defines 83 `languageIds` and never sets the flag to `false`, so language icons
**do** apply to any language Seti does not itself cover. `contributes.languages[].icon`
was correct all along — the same mechanism `apex-stack.apex-alpine` uses, which is
what Andre pointed at. The icon theme is removed, along with the workspace setting
that turned it on. No theme to switch, nothing lost.

The lesson is the one this project already applies to the compiler: **check the
mechanism instead of reasoning about it from memory.** The answer was in a file on
disk the whole time, and finding it took one grep.

**What was actually missing was installation.** The extension had been *symlinked*
into the extensions directory, which works until something reads the extension
registry and does not find you. So it is now packaged and installed properly:

```sh
python3 editors/vscode/pack.py                            # no npm, no vsce
code --install-extension editors/vscode/burxt-0.1.0.vsix
```

`pack.py` is a .vsix writer in the standard library — a .vsix is a ZIP holding an OPC
content-types map, a VSIX manifest and the extension under `extension/`. `vsce` does
more (linting, dependency bundling, marketplace checks), all of it for *publishing*
rather than installing, so none of it is needed. The extension keeps its promise of
needing no toolchain.

**One manifest property that matters on a remote:** `"extensionKind": ["workspace"]`.
Without it, a WSL or SSH session runs the extension on the **UI** side, where there
is no compiler and no language server to talk to. Now asserted by a test, along with
the language icon declaration and the existence of every file `pack.py` ships —
three things whose loss is silent.
