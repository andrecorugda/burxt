---
layout: doc
title: lib/html.bx
section: reference
description: "HTML as a typed tree, escaped at the one point it leaves."
---

{% raw %}

# `lib/html.bx`

HTML as a typed tree, escaped at the one point it leaves.

```burxt
use "lib/html.bx";
```

This is M15's W0, and it needed no compiler feature: the `enum` + `class` mutual recursion is the same shape `lib/json.bx` already proved, and the escape loop is `json_escape` with a different table.

---- The one position this library takes ------------------------------------------------

**An `Html` value cannot carry an unescaped String by mistake.**

`Text` and `Raw` are different constructors and neither is the default, so embedding unescaped bytes is a thing a reviewer sees on the line that does it. Escaping then happens on RENDER, not at construction, because the alternative cannot be checked: if `html_text` escaped eagerly, `Html.Text` would hold already-escaped text and nothing in the type would say so — one function that forgets, or one value built by hand, and the page has a hole. Escaping where a String leaves the tree means there is exactly one place to be right.

---- The two holes escaping does not cover, and both are refused here --------------------

Escaping a VALUE does not save you from a bad NAME. `html_element(tag, ...)` with a tag of `"div onclick=steal()"` writes that text into the markup as syntax, and no amount of escaping the children helps — so a tag or attribute name that is not a name is refused by contract rather than rendered. `html_is_name` is the whole rule and it is deliberately narrow: a letter first, then letters, digits, `-` and `_`.

A **void element cannot carry children**, also by contract. `<br>Rice</br>` does not mean what it looks like — a browser reads `</br>` as a SECOND `<br>` and the text lands outside the element you wrote. Rendering it would be a silent wrong answer, which `DESIGN.md` calls worse than a crash, so it is a refusal at the point of construction instead.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`Attr`](#attr) | class | One attribute. A class rather than a two-payload variant, for the reason `Field` is one at lib/json.bx:49 — a name and a |
| [`Element`](#element) | class | — |
| [`Html`](#html) | enum | A node. `Element` holds `[Html]` and `Html` holds an `Element` — the two halves of one recursive shape, and the slice is |
| [`html_is_name`](#html-is-name) | function | A tag or attribute name: a letter, then letters, digits, `-` and `_`. Narrow on purpose — this is the predicate that sta |
| [`html_is_void`](#html-is-void) | function | The HTML void elements, in full. A closing tag for one of these is not merely redundant — see the header. The list is th |
| [`html_text`](#html-text) | function | — |
| [`html_raw`](#html-raw) | function | The escape hatch, and it is spelled out. There is no convenience wrapper on this and there will not be one: one way to s |
| [`html_attr`](#html-attr) | function | — |
| [`html_element`](#html-element) | function | Answers `Html` rather than `Element` so that nesting is one call deep: the tree is the surface, and `Html.Node(html_elem |
| [`html_escape`](#html-escape) | function | The five entities, and only those. `'` goes out as `&#39;` rather than `&apos;`, which is XML's spelling and not in HTML |
| [`html_render`](#html-render) | function | One node as text. Recursive, because the shape is. |
| [`html_document`](#html-document) | function | A whole document, with the doctype every browser needs to stay out of quirks mode. |

## Types
{: #types}

### `Attr`
{: #attr}

```burxt
class Attr { name: String, value: String }
```

One attribute. A class rather than a two-payload variant, for the reason `Field` is one at lib/json.bx:49 — a name and a value travelling together is a thing worth naming.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/html.bx#L37)

### `Element`
{: #element}

```burxt
class Element { tag: String, attrs: [Attr], children: [Html] }
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/html.bx#L39)

### `Html`
{: #html}

```burxt
enum Html
```

A node. `Element` holds `[Html]` and `Html` holds an `Element` — the two halves of one recursive shape, and the slice is what keeps neither side infinitely wide.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/html.bx#L43)

## Functions
{: #functions}

### `html_is_name`
{: #html-is-name}

```burxt
pure function html_is_name(name: String) -> Bool
```

A tag or attribute name: a letter, then letters, digits, `-` and `_`. Narrow on purpose — this is the predicate that stands between a computed name and markup injection, and the safe answer to "should this byte be allowed" is no.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/html.bx#L55)

### `html_is_void`
{: #html-is-void}

```burxt
pure function html_is_void(tag: String) -> Bool
```

The HTML void elements, in full. A closing tag for one of these is not merely redundant — see the header. The list is the standard's and does not grow.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/html.bx#L77)

### `html_text`
{: #html-text}

```burxt
pure function html_text(value: String) -> Html
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/html.bx#L107)

### `html_raw`
{: #html-raw}

```burxt
pure function html_raw(trusted: String) -> Html
```

The escape hatch, and it is spelled out. There is no convenience wrapper on this and there will not be one: one way to say "unescaped", so a reviewer greps one word.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/html.bx#L113)

### `html_attr`
{: #html-attr}

```burxt
pure function html_attr(name: String, value: String) -> Attr
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/html.bx#L117)

### `html_element`
{: #html-element}

```burxt
pure function html_element(tag: String, attrs: [Attr], children: [Html]) -> Html
```

Answers `Html` rather than `Element` so that nesting is one call deep: the tree is the surface, and `Html.Node(html_element(...))` at every level would be ceremony carrying no promise.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/html.bx#L126)

### `html_escape`
{: #html-escape}

```burxt
pure function html_escape(text: String) -> String
```

The five entities, and only those. `'` goes out as `&#39;` rather than `&apos;`, which is XML's spelling and not in HTML 4 — the numeric form is read correctly by everything.

Built in RUNS rather than a byte at a time, copied from `json_escape` at lib/json.bx:98 for the reason recorded there: `out = out + one_byte` copies the whole String on every byte, and this project has paid for that shape three times.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/html.bx#L141)

### `html_render`
{: #html-render}

```burxt
pure function html_render(node: Html) -> String
```

One node as text. Recursive, because the shape is.

Attribute values are escaped and always double-quoted. An unquoted attribute is where a value with a space becomes two attributes, so there is no option to omit them.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/html.bx#L167)

### `html_document`
{: #html-document}

```burxt
pure function html_document(root: Html) -> String
```

A whole document, with the doctype every browser needs to stay out of quirks mode.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/html.bx#L194)


{% endraw %}
