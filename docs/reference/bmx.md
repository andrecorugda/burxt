---
layout: doc
title: lib/bmx.bx
section: reference
description: "BMX 0.1, parsed into a typed tree."
---

{% raw %}

# `lib/bmx.bx`

BMX 0.1, parsed into a typed tree.

```burxt
use "lib/bmx.bx";
```

> **[Read the BMX documentation at bmx.burxt-lang.org](https://bmx.burxt-lang.org/)** — BMX is a format with its own specification, guide and conformance suite.

BMX is markdown with one unambiguous reading and a typed hole in it. The format is specified in its own repository, independently of this file and of this language: structure, error codes and a conformance suite as data. This is one implementation of it.

---- The one property that separates BMX from markdown -----------------------------------

**It always fails loudly.** `*bold` with no closing star is an error here, not the characters `*bold`. An unterminated fence is an error, not a document that silently became one code block. `{{ x` is an error, not text. Markdown is designed so that nothing is ever a syntax error, which is the opposite of what a language whose compiler refuses things can build on.

Every refusal carries a **code** — `BMX-E001` and its siblings — because a code is what a conformance suite can assert across implementations, while a message is this file's own words. The codes are the format's; the wording after them is ours.

---- Where this implementation stands ----------------------------------------------------

The format defines two conformance levels. **Level 1 is rendering**: parse, then substitute slot values with escaping applied. **Level 2 is checking**: every slot expression verified against a declared interface before the document renders, so a slot naming a field that does not exist is a build error rather than a blank on a page.

**This file carries both, and the split is in the file.** `bmx_to_html` is level 1: it refuses a slot with no binding and a link target with a dangerous scheme, but it looks values up by their expression TEXT and nothing there checks a type.

**`bmx_emit_burxt` is level 2, and it is a code GENERATOR rather than a renderer — which is not a limitation but the design.** A `.bmx` document becomes a `pure function ... -> Html` whose slots are ordinary Burxt expressions, and then the COMPILER does the checking. Measured, all four: a slot naming a field that does not exist is a type error at the expression; a slot holding a `Decimal` rather than a `String` is a type error, so the conversion is written in the document where a reviewer sees it; money that would narrow without a rounding contract is refused *inside the view*; and `burxt review` diffs the generated signature between versions. `tests/runner.rs`'s `the_bmx_generator_hands_the_compiler_a_view_it_can_check` runs every one.

**None of that is implemented here.** It is what the language already does to any function, which is `BOUNDARY.md` paying off: the format stays dumb enough for anyone to implement, and the checking lives where it can be enforced.

One consequence of the same boundary, worth knowing before it is met: **a BMX document in a Burxt string literal has to escape every brace** — `"\{\{ user.name \}\}"` — because `{` opens a `{expr}` interpolation and a bare `}` closes one. It compiles and it is correct; it is merely unreadable, and `tests/pass/bmx_library.bx` is written that way on purpose to show what it costs.

*(This paragraph first said a document "does not go in a string literal" at all. That was over-strong, and the fixture two directories away already disproved it. Corrected 2026-08-16 — the weaker claim is the true one, and the habit of reaching for the stronger one is what put two wrong rows in `FAR-HORIZON-ROADMAP.md`.)*

So BMX lives in `.bmx` files: read at runtime here, read at build time by the generator. Not because a literal is refused, but because a template inside one is a template no editor highlights, no formatter touches and no conformance suite can reach. The language decision behind this is `DESIGN.md`'s *"The delimiters do not move for an embedded format"* (2026-08-16) — the delimiters stay, and the format is the one with a choice left.

---- What it is built on -----------------------------------------------------------------

`lib/html.bx` for output, so escaping has exactly one place to be right in this language and BMX does not get a second one. `lib/json.bx` for the AST, which is how the format's conformance suite compares implementations.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`BmxSlot`](#bmxslot) | class | A slot carries its expression AND where it started, because a host must be able to point at the source the author wrote  |
| [`BmxWrap`](#bmxwrap) | class | — |
| [`BmxLink`](#bmxlink) | class | — |
| [`BmxInline`](#bmxinline) | class | Inline content. An inline block — `::key[Ctrl+S]::`. NOT a slot: a slot's value is escaped and this is a call to somethi |
| [`Bmx`](#bmx) | enum | — |
| [`BmxItem`](#bmxitem) | class | — |
| [`BmxHeading`](#bmxheading) | class | — |
| [`BmxList`](#bmxlist) | class | — |
| [`BmxCode`](#bmxcode) | class | — |
| [`BmxWords`](#bmxwords) | class | — |
| [`BmxBlock`](#bmxblock) | class | Block content. `Paragraph` and `Quote` carry the same shape and are still two variants: they mean different things and r |
| [`Block`](#block) | enum | — |
| [`BmxLine`](#bmxline) | class | — |
| [`Binding`](#binding) | class | — |
| [`bmx_error`](#bmx-error) | function | `BMX-E001 at 6: unterminated slot`. The code leads so a conformance harness can compare on a prefix without parsing our  |
| [`bmx_is_space`](#bmx-is-space) | function | — |
| [`bmx_strip_end`](#bmx-strip-end) | function | A line's content with trailing spaces removed. The format strips them because the two-space line break is an invisible c |
| [`bmx_is_blank`](#bmx-is-blank) | function | — |
| [`bmx_starts_with`](#bmx-starts-with) | function | — |
| [`bmx_ordered_marker`](#bmx-ordered-marker) | function | The digits at the start of a line, or -1 if there are none. `12. ` is an ordered marker. |
| [`bmx_fence`](#bmx-fence) | function | How many colons a line opens or closes with. |
| [`bmx_parse_inline`](#bmx-parse-inline) | function | `base` is where `text` starts in the whole document, so a slot's offset is a real position in the file the author opened |
| [`bmx_merge_text`](#bmx-merge-text) | function | Adjacent `Text` nodes are always merged. The format requires it — two implementations that disagree about whether `a` `b |
| [`bmx_lines`](#bmx-lines) | function | Split into lines, keeping each line's byte offset. `\r\n` ends a line and a lone `\r` does not — a stray carriage return |
| [`bmx_parse`](#bmx-parse) | function | A document, or the first error in it. A conforming parser stops at the first error: recovery is a real want, but recover |
| [`bmx_parse_blocks`](#bmx-parse-blocks) | function | Blocks nest by fence LENGTH — a longer fence contains a shorter one, the rule code fences already use, so there is nothi |
| [`bmx_inline_json`](#bmx-inline-json) | function | — |
| [`bmx_json_children`](#bmx-json-children) | function | The children array on its own, because a block carries one and the document carries one. |
| [`bmx_json`](#bmx-json) | function | — |
| [`bmx_bind`](#bmx-bind) | function | — |
| [`bmx_lookup`](#bmx-lookup) | function | — |
| [`bmx_target_allowed`](#bmx-target-allowed) | function | A target with no scheme is relative and allowed. A target with a scheme is allowed only from a named set. The check is " |
| [`bmx_inline_html`](#bmx-inline-html) | function | — |
| [`bmx_heading_tag`](#bmx-heading-tag) | function | — |
| [`bmx_html`](#bmx-html) | function | — |
| [`bmx_to_html`](#bmx-to-html) | function | Source in, page out. The whole path, for the caller who does not need the tree. |
| [`bmx_burxt_string`](#bmx-burxt-string) | function | A Burxt string literal, escaped. `\{` and `\}` are the two a reader will not expect: a bare brace opens or closes an int |
| [`bmx_emit_inline`](#bmx-emit-inline) | function | — |
| [`bmx_emit_one`](#bmx-emit-one) | function | The expression for one block that is NOT a fence. Fences are statements and are handled by `bmx_emit_stmts`, which is th |
| [`bmx_emit_stmts`](#bmx-emit-stmts) | function | Statements that push each block into `target`, because a fence is a statement: `for` and `if` bind names, and a binding  |
| [`bmx_props`](#bmx-props) | function | A `props` block declares the view's own signature, so a component can be invoked by another document without an out-of-b |
| [`bmx_without_props`](#bmx-without-props) | function | The document without its `props` block, because a declaration is not content and must not become a call to a component n |
| [`bmx_emit_burxt`](#bmx-emit-burxt) | function | A document and a signature in, a Burxt source file out. |

## Types
{: #types}

### `BmxSlot`
{: #bmxslot}

```burxt
public class BmxSlot { expression: String, offset: Int }
```

A slot carries its expression AND where it started, because a host must be able to point at the source the author wrote rather than at whatever it generated. The format makes the offset mandatory for that reason.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L80)

### `BmxWrap`
{: #bmxwrap}

```burxt
public class BmxWrap { children: [Bmx] }
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L82)

### `BmxLink`
{: #bmxlink}

```burxt
public class BmxLink { target: String, children: [Bmx] }
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L84)

### `BmxInline`
{: #bmxinline}

```burxt
public class BmxInline { name: String, head: String, offset: Int }
```

Inline content. An inline block — `::key[Ctrl+S]::`. NOT a slot: a slot's value is escaped and this is a call to something the host declared, so the two look different because they ARE different.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L89)

### `Bmx`
{: #bmx}

```burxt
public enum Bmx
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L91)

### `BmxItem`
{: #bmxitem}

```burxt
public class BmxItem { children: [Bmx] }
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L101)

### `BmxHeading`
{: #bmxheading}

```burxt
public class BmxHeading { level: Int, children: [Bmx] }
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L103)

### `BmxList`
{: #bmxlist}

```burxt
public class BmxList { ordered: Bool, items: [BmxItem] }
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L104)

### `BmxCode`
{: #bmxcode}

```burxt
public class BmxCode { info: String, value: String }
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L105)

### `BmxWords`
{: #bmxwords}

```burxt
public class BmxWords { children: [Bmx] }
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L106)

### `BmxBlock`
{: #bmxblock}

```burxt
public class BmxBlock { name: String, head: String, offset: Int, children: [Block] }
```

Block content. `Paragraph` and `Quote` carry the same shape and are still two variants: they mean different things and render differently, and a `kind` field would put the distinction somewhere a `match` cannot see it. A block — the format's one extension point for structure. `head` is captured and NEVER parsed, exactly like a slot's expression: what `for line in order.lines` means is the host's business.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L113)

### `Block`
{: #block}

```burxt
public enum Block
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L115)

### `BmxLine`
{: #bmxline}

```burxt
class BmxLine { text: String, offset: Int }
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L412)

### `Binding`
{: #binding}

```burxt
class Binding { name: String, value: String }
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L830)

## Functions
{: #functions}

### `bmx_error`
{: #bmx-error}

```burxt
pure function bmx_error(code: String, offset: Int, message: String) -> String
```

`BMX-E001 at 6: unterminated slot`. The code leads so a conformance harness can compare on a prefix without parsing our prose, which is the half of an error the format owns.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L128)

### `bmx_is_space`
{: #bmx-is-space}

```burxt
pure function bmx_is_space(b: Int) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L134)

### `bmx_strip_end`
{: #bmx-strip-end}

```burxt
public pure function bmx_strip_end(text: String) -> String
```

A line's content with trailing spaces removed. The format strips them because the two-space line break is an invisible character that changes output, which is unreviewable by construction.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L141)

### `bmx_is_blank`
{: #bmx-is-blank}

```burxt
pure function bmx_is_blank(text: String) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L149)

### `bmx_starts_with`
{: #bmx-starts-with}

```burxt
pure function bmx_starts_with(text: String, prefix: String) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L158)

### `bmx_ordered_marker`
{: #bmx-ordered-marker}

```burxt
pure function bmx_ordered_marker(text: String) -> Int
```

The digits at the start of a line, or -1 if there are none. `12. ` is an ordered marker.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L163)

### `bmx_fence`
{: #bmx-fence}

```burxt
pure function bmx_fence(text: String) -> Int
```

How many colons a line opens or closes with.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L176)

### `bmx_parse_inline`
{: #bmx-parse-inline}

```burxt
function bmx_parse_inline(text: String, base: Int) -> Result<[Bmx], String>
```

`base` is where `text` starts in the whole document, so a slot's offset is a real position in the file the author opened rather than an index into a fragment.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L188)

### `bmx_merge_text`
{: #bmx-merge-text}

```burxt
function bmx_merge_text(nodes: [Bmx]) -> [Bmx]
```

Adjacent `Text` nodes are always merged. The format requires it — two implementations that disagree about whether `a` `b` is one node or two disagree about the document, and the conformance suite would rightly fail one of them.

This exists because inline content is parsed **one line at a time** rather than over a joined buffer. That is what keeps a slot's offset pointing at the author's source: joining first and parsing after put the offset off by exactly the trailing spaces stripped from every earlier line, measured at 11 where the byte was at 14. It is also what the spec already implied — every inline construct must close on its own line, so there was never a reason to parse across one.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L368)

### `bmx_lines`
{: #bmx-lines}

```burxt
function bmx_lines(source: String) -> [BmxLine]
```

Split into lines, keeping each line's byte offset. `\r\n` ends a line and a lone `\r` does not — a stray carriage return in the middle of a line is far more likely to be data than intent, and the format says so rather than leaving it to each parser.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L417)

### `bmx_parse`
{: #bmx-parse}

```burxt
public function bmx_parse(source: String) -> Result<[Block], String>
```

A document, or the first error in it. A conforming parser stops at the first error: recovery is a real want, but recovery that differs between implementations is worse than none.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L445)

### `bmx_parse_blocks`
{: #bmx-parse-blocks}

```burxt
function bmx_parse_blocks(lines: [BmxLine], from: Int, depth: Int, mutable out: [Block]) -> Result<Int, String>
```

Blocks nest by fence LENGTH — a longer fence contains a shorter one, the rule code fences already use, so there is nothing new to learn and nothing to count. `depth` is the enclosing block's fence length, or 0 at the top. Answers the index it stopped at.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L464)

### `bmx_inline_json`
{: #bmx-inline-json}

```burxt
function bmx_inline_json(nodes: [Bmx]) -> Json
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L687)

### `bmx_json_children`
{: #bmx-json-children}

```burxt
function bmx_json_children(blocks: [Block]) -> Json
```

The children array on its own, because a block carries one and the document carries one.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L745)

### `bmx_json`
{: #bmx-json}

```burxt
function bmx_json(blocks: [Block]) -> Json
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L804)

### `bmx_bind`
{: #bmx-bind}

```burxt
pure function bmx_bind(name: String, value: String) -> Binding
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L832)

### `bmx_lookup`
{: #bmx-lookup}

```burxt
pure function bmx_lookup(bindings: [Binding], name: String) -> Option<String>
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L836)

### `bmx_target_allowed`
{: #bmx-target-allowed}

```burxt
pure function bmx_target_allowed(target: String) -> Bool
```

A target with no scheme is relative and allowed. A target with a scheme is allowed only from a named set. The check is "is there a `:` before the first `/`", which is what distinguishes `mailto:a@b` and `javascript:x` from `/page` and `a/b:c`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L850)

### `bmx_inline_html`
{: #bmx-inline-html}

```burxt
function bmx_inline_html(nodes: [Bmx], bindings: [Binding]) -> Result<[Html], String>
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L868)

### `bmx_heading_tag`
{: #bmx-heading-tag}

```burxt
pure function bmx_heading_tag(level: Int) -> String
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L927)

### `bmx_html`
{: #bmx-html}

```burxt
function bmx_html(blocks: [Block], bindings: [Binding]) -> Result<Html, String>
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L931)

### `bmx_to_html`
{: #bmx-to-html}

```burxt
function bmx_to_html(source: String, bindings: [Binding]) -> Result<String, String>
```

Source in, page out. The whole path, for the caller who does not need the tree.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L992)

### `bmx_burxt_string`
{: #bmx-burxt-string}

```burxt
pure function bmx_burxt_string(text: String) -> String
```

A Burxt string literal, escaped. `\{` and `\}` are the two a reader will not expect: a bare brace opens or closes an interpolation, so a document's own braces have to survive the trip into a literal.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L1029)

### `bmx_emit_inline`
{: #bmx-emit-inline}

```burxt
public function bmx_emit_inline(nodes: [Bmx]) -> Result<String, String>
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L1046)

### `bmx_emit_one`
{: #bmx-emit-one}

```burxt
public function bmx_emit_one(block: Block) -> Result<String, String>
```

The expression for one block that is NOT a fence. Fences are statements and are handled by `bmx_emit_stmts`, which is the only caller.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L1097)

### `bmx_emit_stmts`
{: #bmx-emit-stmts}

```burxt
public function bmx_emit_stmts(blocks: [Block], target: String, tag: String, indent: String) -> Result<String, String>
```

Statements that push each block into `target`, because a fence is a statement: `for` and `if` bind names, and a binding cannot live inside an expression.

**This is where the whole design pays off.** `::: for line in order.lines` becomes a real Burxt `for`, so inside the body `line` is a real `Line` — a slot saying `line.sk` is a type error naming the field, and money that would round without a contract is refused. No other template language checks the body of a loop, because no other one hands that body to a compiler which already knows the types.

`tag` makes generated names unique without threading a counter: children of block `i` under `tag` are built in `k_<tag>_<i>`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L1169)

### `bmx_props`
{: #bmx-props}

```burxt
function bmx_props(blocks: [Block]) -> String
```

A `props` block declares the view's own signature, so a component can be invoked by another document without an out-of-band agreement about its parameters:

```burxt
 ::: props title: String, featured: Bool
 :::
```

It is not a new construct — `props` is a block name like any other, and its head is captured opaquely exactly like `for`'s. **That is why §7's front-matter row closed as NOT NEEDED rather than as built**: the format already had the general shape, and the answer was to use it on itself.

Answers the head of the first `props` block, or "" if the document declares none.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L1249)

### `bmx_without_props`
{: #bmx-without-props}

```burxt
function bmx_without_props(blocks: [Block]) -> [Block]
```

The document without its `props` block, because a declaration is not content and must not become a call to a component nobody wrote.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L1269)

### `bmx_emit_burxt`
{: #bmx-emit-burxt}

```burxt
function bmx_emit_burxt(blocks: [Block], source_name: String, name: String, parameters: String, clauses: [String]) -> Result<String, String>
```

A document and a signature in, a Burxt source file out.

`parameters` is written verbatim into the signature (`"order: Order"`) and `requires` is one clause per entry. Both come from the caller because the FORMAT does not carry them — see the note above about front matter.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/bmx.bx#L1297)


{% endraw %}
