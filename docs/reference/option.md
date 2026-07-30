---
layout: doc
title: lib/option.bx
section: reference
description: Absence, made explicit.
---


# `lib/option.bx`

Absence, made explicit.

```burxt
use "lib/option.bx";
```

Burxt has no null. Nothing is implicitly absent, no value silently means "missing", and no dereference can fail — because absence is a type, and the compiler makes you say what happens when there is nothing there.

This is a LIBRARY file. `Option<T>` is four lines of Burxt with no compiler support beyond generics, which is the test M7 set for whether the generics are real: if `Option` had needed a keyword, they were not.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`Option`](#option) | enum | One of two things: nothing, or a value. `match` forces both cases to be written, so "I forgot to handle missing" is a co |
| [`option_or`](#option-or) | function | The value, or the fallback. The one everybody writes first. |
| [`option_is_some`](#option-is-some) | function | Whether there is anything there. Useful in a condition; useless for getting at the value, which is the point — asking is |
| [`option_is_none`](#option-is-none) | function | — |

## Types
{: #types}

### `Option`
{: #option}

```burxt
enum Option<T>
```

One of two things: nothing, or a value. `match` forces both cases to be written, so "I forgot to handle missing" is a compile error rather than a crash at three in the morning.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/option.bx#L18)

## Functions
{: #functions}

### `option_or`
{: #option-or}

```burxt
function option_or<T>(o: Option<T>, fallback: T) -> T
```

The value, or the fallback. The one everybody writes first.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/option.bx#L37)

### `option_is_some`
{: #option-is-some}

```burxt
function option_is_some<T>(o: Option<T>) -> Bool
```

Whether there is anything there. Useful in a condition; useless for getting at the value, which is the point — asking is not the same as having.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/option.bx#L46)

### `option_is_none`
{: #option-is-none}

```burxt
function option_is_none<T>(o: Option<T>) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/option.bx#L53)

