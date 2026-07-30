---
layout: doc
title: lib/result.bx
section: reference
description: Failure, made explicit.
---


# `lib/result.bx`

Failure, made explicit.

```burxt
use "lib/result.bx";
```

A function that can fail says so in its type. There is no exception to catch and no error code to forget, because the compiler will not let a `Result` be read without saying what happens when it is an error.

A LIBRARY file, like lib/option.bx: `Result<T, E>` is three lines of Burxt.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`Result`](#result) | enum | Either an answer or a reason there is none. `E` is usually a String while a program is young and an enum once its failur |
| [`result_or`](#result-or) | function | The answer, or the fallback. For when a failure has an obvious substitute. |
| [`result_is_ok`](#result-is-ok) | function | — |

## Types
{: #types}

### `Result`
{: #result}

```burxt
enum Result<T, E>
```

Either an answer or a reason there is none. `E` is usually a String while a program is young and an enum once its failures are worth naming.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/result.bx#L15)

## Functions
{: #functions}

### `result_or`
{: #result-or}

```burxt
function result_or<T, E>(r: Result<T, E>, fallback: T) -> T
```

The answer, or the fallback. For when a failure has an obvious substitute.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/result.bx#L39)

### `result_is_ok`
{: #result-is-ok}

```burxt
function result_is_ok<T, E>(r: Result<T, E>) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/result.bx#L46)

