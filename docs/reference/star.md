---
layout: doc
title: lib/star.bx
section: reference
description: "Star-burxt: what gives a BMX document life."
---

{% raw %}

# `lib/star.bx`

Star-burxt: what gives a BMX document life.

```burxt
use "lib/star.bx";
```

BMX describes where an expression goes. It does not know what `for` means, or `if`, or `on:click` — `BOUNDARY.md` is the document that keeps it that way. This file is the host that decides, and `SPEC.md` §4a.5 is its specification:

- refuse an unknown block name, never render it and never skip it silently - refuse an unknown attribute on a block it declares - decide what a head means, including whether it binds names inside the body - **refuse an event attribute it cannot wire.** A host with no runtime must

```burxt
 refuse `on:*` rather than emit an inline handler — emitting one puts
 unchecked script on the page, which is the hole escaping exists to close
```

That last one is the reason this file exists rather than a runtime `button()` function. `lib/bmx.bx` hands a component block's head over as a **runtime String**, so `on:click=count + 1` would arrive as text that nothing compiles: no typecheck, no unknown-name error, no `burxt review` surface. A handler that is a string is a handler the language cannot judge, and judging it is the whole case for writing a framework in this language rather than another one.

---- the shape, and why it is the one Burxt was going to force anyway --------

```burxt
 props count: Int          the state
 on:click=count + 1        an event yields the NEXT state
 view                      a pure function of the state
 dispatch                  a pure function of (handler, state) -> state
```

A view is `pure`, so `burxt effects --allow ""` confirms it reaches nothing — by construction rather than by inspection. A handler is an expression producing the next state, which is the same architecture Elm arrived at, and it is not a coincidence: **Burxt has no closures** (`lib/fn.bx` — "a closure needs an owner for its captured state, which is a memory question in a language whose whole memory model is regions"). With no closures there is nothing to capture a mutable cell in, so state cannot hide inside a handler. It has to be threaded, and threading it is what makes the update inspectable.

The consequence worth stating: **an event handler's effect on the program is a value you can print.** `dispatch(0, 41)` is `42`. No framework whose handlers are closures can say that, and `burxt review` can diff what a handler promises between versions.

---- what reaches the page ---------------------------------------------------

A handler becomes `data-star-h="0"` — an INDEX, never an inline handler. The driver installs one delegated listener and calls the exported `dispatch`. So a star-burxt page carries no executable markup at all, which is §4a.5 satisfied literally rather than approximately.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`StarHandler`](#starhandler) | class | A handler found while walking the document: its index, and the expression the author wrote. The expression is emitted in |
| [`StarComponent`](#starcomponent) | class | What a generated component is: the view, the dispatch, and the handler table that ties an index on the page to an expres |
| [`star_event_part`](#star-event-part) | function | `button on:click=count + 1` -> Some("click=count + 1"). The format captured the head as opaque text; splitting it is our |
| [`star_event_name`](#star-event-name) | function | `click=count + 1` -> the event name, and the expression after the FIRST `=`. The expression runs to the end of the head, |
| [`star_event_expression`](#star-event-expression) | function | — |
| [`star_head_without_event`](#star-head-without-event) | function | The head with its `on:` binding removed — what is left is the element's own business. Trailing space trimmed, so `button |
| [`star_event_is_wired`](#star-event-is-wired) | function | §4a.5: refuse an event attribute we cannot wire. The list is short on purpose — every entry is one the driver installs a |
| [`star_takes_phrasing`](#star-takes-phrasing) | function | The block names this host declares, and the refusal for the rest. |
| [`star_is_element`](#star-is-element) | function | — |
| [`star_key_part`](#star-key-part) | function | `line in order.lines key line.id` -> Some("line.id"). The keyword must stand at a token boundary, so a collection named  |
| [`star_head_without_key`](#star-head-without-key) | function | The head with the `key` clause removed — what `for` itself gets. |
| [`star_emit_stmts`](#star-emit-stmts) | function | The document's blocks become statements pushing into `target`. This mirrors `bmx_emit_stmts` and diverges in exactly one |
| [`star_props`](#star-props) | function | The `props` head is the component's signature — declared by the component, so an invoker never needs an out-of-band list |
| [`star_generate`](#star-generate) | function | The whole of it: a document becomes a `pure function ... -> Html` and a `pure function ..._dispatch(handler: Int, <props |
| [`star_first_prop_name`](#star-first-prop-name) | function | — |
| [`star_argument_list`](#star-argument-list) | function | `count: Int, label: String` -> `count, label`. What the entry point passes on. |

## Types
{: #types}

### `StarHandler`
{: #starhandler}

```burxt
class StarHandler { event: String, expression: String }
```

A handler found while walking the document: its index, and the expression the author wrote. The expression is emitted into `dispatch` verbatim, where the compiler judges it — a typo is `unknown variable`, a wrong type is a type error, and both name the thing the author wrote.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/star.bx#L59)

### `StarComponent`
{: #starcomponent}

```burxt
class StarComponent
```

What a generated component is: the view, the dispatch, and the handler table that ties an index on the page to an expression in the source.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/star.bx#L63)

## Functions
{: #functions}

### `star_event_part`
{: #star-event-part}

```burxt
pure function star_event_part(head: String) -> Option<String>
```

`button on:click=count + 1` -> Some("click=count + 1"). The format captured the head as opaque text; splitting it is ours, and so is refusing what we cannot wire.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/star.bx#L84)

### `star_event_name`
{: #star-event-name}

```burxt
pure function star_event_name(part: String) -> String
```

`click=count + 1` -> the event name, and the expression after the FIRST `=`. The expression runs to the end of the head, so an `on:` binding is written last: expressions contain spaces and equals signs, and a format that does not parse heads cannot tell us where one ends.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/star.bx#L103)

### `star_event_expression`
{: #star-event-expression}

```burxt
pure function star_event_expression(part: String) -> String
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/star.bx#L112)

### `star_head_without_event`
{: #star-head-without-event}

```burxt
pure function star_head_without_event(head: String) -> String
```

The head with its `on:` binding removed — what is left is the element's own business. Trailing space trimmed, so `button on:click=x` leaves `button`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/star.bx#L123)

### `star_event_is_wired`
{: #star-event-is-wired}

```burxt
pure function star_event_is_wired(name: String) -> Bool
```

§4a.5: refuse an event attribute we cannot wire. The list is short on purpose — every entry is one the driver installs a delegated listener for, and adding to it means adding to the driver. A host that accepts an event it does not deliver has told the author a lie the compiler cannot catch.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/star.bx#L142)

### `star_takes_phrasing`
{: #star-takes-phrasing}

```burxt
pure function star_takes_phrasing(name: String) -> Bool
```

The block names this host declares, and the refusal for the rest.

§4a.5 first line: *refuse an unknown block name, never render it and never skip it silently.* So star-burxt says what it knows: `for` and `if` are control flow, `props` is the signature, and these are elements. Anything else is a component, and until cross-file resolution exists there is nothing to resolve it against — so it is refused BY NAME rather than rendered as something plausible. Elements whose content model is PHRASING rather than flow. A `<p>` inside one of these is invalid HTML, not merely unwanted, which is why the unwrapping above is required rather than a preference. The list is short because it is the set this host declares — adding to it is a decision, not a convenience.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/star.bx#L157)

### `star_is_element`
{: #star-is-element}

```burxt
pure function star_is_element(name: String) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/star.bx#L163)

### `star_key_part`
{: #star-key-part}

```burxt
pure function star_key_part(head: String) -> Option<String>
```

`line in order.lines key line.id` -> Some("line.id"). The keyword must stand at a token boundary, so a collection named `monkey` does not match.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/star.bx#L195)

### `star_head_without_key`
{: #star-head-without-key}

```burxt
pure function star_head_without_key(head: String) -> String
```

The head with the `key` clause removed — what `for` itself gets.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/star.bx#L210)

### `star_emit_stmts`
{: #star-emit-stmts}

```burxt
function star_emit_stmts(blocks: [Block], target: String, tag: String, indent: String,
```

The document's blocks become statements pushing into `target`. This mirrors `bmx_emit_stmts` and diverges in exactly one place: a block carrying an `on:*` binding becomes an element with `data-star-h="N"` and its expression is put aside for `dispatch`, where the COMPILER judges it. That divergence is the whole file. `key_here` is the key expression that applies to elements emitted at THIS level — the immediate body of a keyed `for`. It is not passed down: a key identifies the row, and a grandchild of the row is identified by being inside it.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/star.bx#L234)

### `star_props`
{: #star-props}

```burxt
pure function star_props(blocks: [Block]) -> String
```

The `props` head is the component's signature — declared by the component, so an invoker never needs an out-of-band list. BMX captured it opaquely; reading it is ours, and it goes through verbatim into the function's parameter list, where the compiler judges every name and type in it.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/star.bx#L490)

### `star_generate`
{: #star-generate}

```burxt
function star_generate(source: String, name: String) -> Result<StarComponent, String>
```

The whole of it: a document becomes a `pure function ... -> Html` and a `pure function ..._dispatch(handler: Int, <props>) -> <state>`.

**`state` is the FIRST prop's type**, which is the v1 rule and is stated here rather than inferred: a handler yields the next state, and with one prop that is unambiguous. Structured state is the open v2 question — regions are LIFO and nested regions do not exist, so nothing built inside a frame can outlive it.

Both functions are `pure`, so `burxt effects --allow ""` confirms a component reaches nothing — a confirmation by construction rather than a discovery.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/star.bx#L516)

### `star_first_prop_name`
{: #star-first-prop-name}

```burxt
pure function star_first_prop_name(props: String) -> String
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/star.bx#L576)

### `star_argument_list`
{: #star-argument-list}

```burxt
pure function star_argument_list(props: String) -> String
```

`count: Int, label: String` -> `count, label`. What the entry point passes on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/star.bx#L586)


{% endraw %}
