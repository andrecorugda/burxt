---
layout: doc
title: lib/log.bx
section: reference
description: "Four levels, a threshold from the environment, and stderr."
---

{% raw %}

# `lib/log.bx`

Four levels, a threshold from the environment, and stderr.

```burxt
use "lib/log.bx";
```

```burxt
 log_info("listening on 8080");
 log_error("could not open " + path);
```

```burxt
 $ BURXT_LOG=debug ./server
 2026-08-02T09:14:03Z DEBUG parsed 41 rows
 2026-08-02T09:14:03Z INFO  listening on 8080
```

Before this file a Burxt program that wanted to say something to its operator called `print`, which is the same channel its ANSWER goes out on. `spec/1.0/ROADMAP-1.0.md` §D1n calls the gap *"structured logging: Blocking"*, and this closes it.

---- stderr, and it is not a preference ------------------------------------------------

**Every line goes to stderr, through `print_error`.** A log line on stdout corrupts the output of every program that is piped: `myprog | wc -l` counts the log, `myprog > data.csv` writes the log into the CSV, and `$(myprog)` captures it into a variable. The two streams exist so that a program can say what it FOUND and what it was DOING at the same time without one destroying the other, and a logging library on the wrong stream takes that away from every caller at once.

This repository has a runner invariant about exactly this, which is why it is worth restating: `run_capturing_stdout` in `tests/runner.bx` keeps the streams apart *because* it once merged them with `2>&1` and reported `tests/pass/print_error.bx` as failing. The merge was invisible for as long as nothing could write to stderr on purpose. `tests/pass/log_library.bx` asserts the same property from the other side: it logs at every level and its `.stdout` does not contain a single log line.

---- the default threshold is WARN, and why it is neither of the obvious two ------------

With `BURXT_LOG` unset a program logs **warnings and errors, and nothing else.**

Silent-by-default and info-by-default are both defensible and this is neither, so the argument has to be made rather than asserted. Info-by-default means that linking this file changes what every program prints, and a program that suddenly narrates itself has had its stderr taken over without asking. Silent-by-default means `log_error("the database is gone")` produces nothing at all unless an operator knew in advance to set a variable — an error that vanishes is worse than no logging, because the code reads as though it reported the problem.

WARN is the only default that is both. `log_debug` and `log_info` are the noisy levels and they are the ones you opt into; `log_warn` and `log_error` are, by construction, things that are not supposed to happen, so a well-behaved program is quiet by default without ever losing a problem.

**And it can be silenced completely**: `BURXT_LOG=off`. A library that cannot be quiet is not one you can put in a pipeline, so the off switch is a named level rather than an afterthought.

---- an unknown value must not be able to turn the log off -----------------------------

`BURXT_LOG=banana` sets the threshold to **DEBUG** — the most verbose level, not the default and certainly not off.

The reasoning is about which mistake is recoverable. The only reason anyone sets `BURXT_LOG` is to see MORE, so a typo should err toward showing too much: too much output is visible the instant you look, and it explains itself. Too little is silent, and "why is my logging not working" is an hour that ends in someone discovering they typed `DEBUGG`. Falling back to the default would also satisfy "does not silence the log", and it is the quieter choice — it is rejected because it is indistinguishable from having set nothing at all, which is exactly the state the operator was trying to leave.

`log_env_problem()` answers the complaint in words, for a program that wants to print it at startup. It is a function a caller calls rather than something this file prints on its own, because a module cannot hold state (`lib/math.bx`'s header has the long version) and so it has nowhere to remember that it already complained — the alternative is the same nag on every line.

Matching is **case-insensitive and trims surrounding space**, so `WARN`, `warn` and `" warn "` are one value. The names, and every alias:

```burxt
 off | none | silent      nothing is logged
 debug                    everything
 info
 warn | warning
 error
 (unset, or empty)        the default: warn
 (anything else)          debug, and `log_env_problem` says so
```

---- what this deliberately does not do ------------------------------------------------

**No JSON, and no key/value fields.** The audit's phrase is "structured logging" and the row's own spec is levels, a threshold, stderr and timestamps — which is what a human reading a terminal needs. `log_format` is a plain function returning the line, so a program that wants JSON records composes `lib/json.bx` over the same levels in a few lines of its own. Making that the default would optimise for a log aggregator nobody here has, at the cost of the reader who does.

**No file output and no rotation.** A log goes to stderr; where it ends up after that is the shell's business, and `./prog 2>> app.log` already exists.

**Whole seconds, UTC.** `lib/time.bx`'s limits, inherited rather than worked around: sub-second timestamps need a monotonic clock, which needs `clock_gettime`, which needs A7. Two lines logged in the same second carry the same stamp and their ORDER is the only thing distinguishing them.

---- where D0 applies here, and where it does not --------------------------------------

§D0 requires a chunk list joined pairwise for anything that builds a String in a loop. **`log_one_line` is the only function here that loops**, and it uses one. Everything else is a fixed handful of `+` on short pieces — a timestamp, a five-byte level and two spaces — where a chunk list would be more machinery than the thing it protects. That judgement is recorded rather than left implicit, because a reader should not have to check five functions to find out whether the rule was considered.

`log_one_line` also returns its argument UNTOUCHED when there is nothing to escape, which is the common case: the scan is one pass and costs no allocation at all. A message long enough for the chunking to matter is a message someone built, and building it is where their own §D0 lives.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`log_level_from_name`](#log-level-from-name) | function | The level a `BURXT_LOG` value names. |
| [`log_level_name`](#log-level-name) | function | The five-letter tag a line carries. Padded by `log_format`, not here, so this answers a name rather than a column. |
| [`log_level_passes`](#log-level-passes) | function | Would a line at `level` be written, given `threshold`? |
| [`log_threshold`](#log-threshold) | function | The threshold this process is running with. |
| [`log_enabled`](#log-enabled) | function | Would a line at this level be written? The threshold is read from the environment on each call, which is a `getenv` — ch |
| [`log_env_problem`](#log-env-problem) | function | A complaint about `BURXT_LOG`, in words, or None when there is nothing to complain about. |
| [`log_pad_level`](#log-pad-level) | function | `name` widened to five bytes, so the messages line up in a column. |
| [`log_has_break`](#log-has-break) | function | Is there anything in this message that would break the one-record-per-line shape? |
| [`log_one_line`](#log-one-line) | function | The message with newlines and carriage returns turned into the two-character escapes `\n` and `\r`, so a record is exact |
| [`log_merge`](#log-merge) | function | The chunk list, joined PAIRWISE. `join_chunks` in `src/burxt-compiler/emit.bx` is the reference; a left fold here would  |
| [`log_format`](#log-format) | function | One log line, complete, with no clock and no environment in it. |
| [`log_at`](#log-at) | function | Write a line at `level`, if the threshold allows it. Answers whether it was written. |
| [`log_debug`](#log-debug) | function | The noisy one. Off unless `BURXT_LOG=debug`. |
| [`log_info`](#log-info) | function | What the program is doing. Off by default — see the header's argument about the threshold. |
| [`log_warn`](#log-warn) | function | Something is wrong and the program is carrying on. On by default. |
| [`log_error`](#log-error) | function | Something is wrong and the program is not carrying on with it. On by default, and the level that must never be silently  |

## Functions
{: #functions}

### `log_level_from_name`
{: #log-level-from-name}

```burxt
function log_level_from_name(text: String) -> Int
```

The level a `BURXT_LOG` value names.

Total: every String answers something, and nothing here can fail or crash. The three behaviours worth pinning, all of them tested in `tests/pass/log_library.bx`:

```burxt
 "WARN", "warn", " warn "   all LOG_WARN — case-insensitive, and space is trimmed
 ""                         LOG_DEFAULT — `BURXT_LOG=` is set-to-nothing, which is not a level
 "banana"                   LOG_DEBUG — a typo shows too much, never too little
```

`BURXT_LOG=` answering the default rather than `off` follows the convention every other tool uses: setting a variable to the empty string is how a shell script says "leave it alone", and `lib/os.bx`'s `os_env` keeps unset and empty distinguishable precisely so that a library can decide this deliberately instead of by accident. Here they are decided to be the same.

Not `pure`, and only because `string_trim` in `lib/string.bx` is not marked `pure` — it reads its argument, allocates, and touches nothing else. Duplicating a five-line trim into this file to win a marker would be the wrong trade; the marker belongs on `string_trim`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/log.bx#L149)

### `log_level_name`
{: #log-level-name}

```burxt
pure function log_level_name(level: Int) -> String allocates
```

The five-letter tag a line carries. Padded by `log_format`, not here, so this answers a name rather than a column.

Exact matches rather than ranges, and an unnamed level renders as `LEVEL<n>` instead of being rounded to a neighbour. A caller passing 15 to `log_at` gets a line saying 15 — the alternative prints `INFO` for something that is not INFO, which is a small lie in the one place a reader is relying on the text.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/log.bx#L181)

### `log_level_passes`
{: #log-level-passes}

```burxt
pure function log_level_passes(level: Int, threshold: Int) -> Bool
```

Would a line at `level` be written, given `threshold`?

A separate function from the emitters because it is the whole of the policy, and because a program with an expensive message wants to ask before building it:

```burxt
 if log_enabled(LOG_DEBUG) {
     log_debug("state: " + expensive_render(world));
 }
```

`>=` and not `>`: a threshold of WARN logs warnings.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/log.bx#L210)

### `log_threshold`
{: #log-threshold}

```burxt
function log_threshold() -> Int touches input
```

The threshold this process is running with.

`os_env_or` with an EMPTY fallback, and `log_level_from_name("")` answers `LOG_DEFAULT` — so "unset" and "set to nothing" arrive at the same place without this function deciding it twice. The whole of the environment handling is this one line; everything above it is testable without an environment, which matters because a Burxt program cannot yet SET one (that is §D1q, `os_set_env`). `tests/pass/log_library.bx` covers this line by re-running itself under `env`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/log.bx#L223)

### `log_enabled`
{: #log-enabled}

```burxt
function log_enabled(level: Int) -> Bool touches input
```

Would a line at this level be written? The threshold is read from the environment on each call, which is a `getenv` — cheap, and it means a caller never holds a stale copy.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/log.bx#L229)

### `log_env_problem`
{: #log-env-problem}

```burxt
function log_env_problem() -> Option<String> touches input
```

A complaint about `BURXT_LOG`, in words, or None when there is nothing to complain about.

For a program that wants to tell its operator about a typo:

```burxt
 match log_env_problem() {
     None => { }
     Some(why) => { print_error(why); }
 }
```

This file will not print it unprompted. A module holds no state, so it has nowhere to remember that it already complained, and the alternative is the same sentence attached to every line — which is how a warning becomes something people filter out.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/log.bx#L245)

### `log_pad_level`
{: #log-pad-level}

```burxt
pure function log_pad_level(name: String) -> String allocates
```

`name` widened to five bytes, so the messages line up in a column.

Five because `ERROR` and `DEBUG` are the longest of the four. `LEVEL15` overflows the column and is left to: an unnamed level is already unusual, and truncating the one field that says what happened to protect an alignment would be the wrong way round.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/log.bx#L272)

### `log_has_break`
{: #log-has-break}

```burxt
pure function log_has_break(message: String) -> Bool
```

Is there anything in this message that would break the one-record-per-line shape?

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/log.bx#L283)

### `log_one_line`
{: #log-one-line}

```burxt
pure function log_one_line(message: String) -> String allocates
```

The message with newlines and carriage returns turned into the two-character escapes `\n` and `\r`, so a record is exactly one line.

**Why escape at all**: a log line's whole value is that it starts with a timestamp and a level. A message containing a newline produces a second line with neither, which `grep`, `tail -f` and every log reader will attribute to the wrong record — a wrong answer that looks like data. So the message is altered rather than the format broken, and the alteration is visible in the output instead of silent.

**This is not a reversible encoding, and it is not meant to be.** A backslash is left alone, so a message that literally contained the two characters `\` and `n` renders the same as one that contained a newline. Escaping backslashes too would fix that and would double every backslash in every path and pattern anyone ever logs, for the benefit of a parser that would be better off reading JSON. The ambiguity is stated rather than removed: `log_format` builds text to be READ.

The chunk list is §D0's shape — a message is unbounded, so `out = out + byte` here is the quadratic this project has paid for four times. The scan first means the ordinary message, which contains no break at all, is returned as-is with nothing allocated.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/log.bx#L314)

### `log_merge`
{: #log-merge}

```burxt
pure function log_merge(chunks: [String]) -> String allocates
```

The chunk list, joined PAIRWISE. `join_chunks` in `src/burxt-compiler/emit.bx` is the reference; a left fold here would rebuild the whole prefix at every step, which is the same quadratic one level up.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/log.bx#L348)

### `log_format`
{: #log-format}

```burxt
pure function log_format(level: Int, message: String, unix_seconds: Int) -> String allocates
```

One log line, complete, with no clock and no environment in it.

```burxt
 log_format(LOG_WARN, "disk almost full", 1785312896)
     == "2026-07-29T08:14:56Z WARN  disk almost full"
```

The timestamp is a PARAMETER, which is the whole reason this function exists separately from `log_at`: a formatter that read the clock itself could not be tested, and "the output looks about right" is how a date bug ships. Every expectation in `tests/pass/log_library.bx` is an exact String because of this.

No trailing newline — `print_error` adds one, the same as `print`.

`time_format_iso` `requires` a year in 0..9999, so a `unix_seconds` outside that range stops the program on the contract rather than printing a wrong date. `os_now()` cannot produce one this side of the year 10000.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/log.bx#L384)

### `log_at`
{: #log-at}

```burxt
function log_at(level: Int, message: String) -> Bool touches input, clock
```

Write a line at `level`, if the threshold allows it. Answers whether it was written.

The Bool is what makes the four below useful as expressions and costs nothing: Burxt has no void function — every function returns a value — so this was going to answer SOMETHING, and "did the operator see this" is the only fact worth handing back. A caller who does not care writes `log_info("started");` as a statement and discards it.

`touches input` for the environment read and `clock` for the timestamp. Neither is hidden: a region calling `log_debug` declares both, which is the language making "this function looks at the world" visible at the call site rather than in a comment.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/log.bx#L402)

### `log_debug`
{: #log-debug}

```burxt
function log_debug(message: String) -> Bool touches input, clock
```

The noisy one. Off unless `BURXT_LOG=debug`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/log.bx#L411)

### `log_info`
{: #log-info}

```burxt
function log_info(message: String) -> Bool touches input, clock
```

What the program is doing. Off by default — see the header's argument about the threshold.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/log.bx#L416)

### `log_warn`
{: #log-warn}

```burxt
function log_warn(message: String) -> Bool touches input, clock
```

Something is wrong and the program is carrying on. On by default.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/log.bx#L421)

### `log_error`
{: #log-error}

```burxt
function log_error(message: String) -> Bool touches input, clock
```

Something is wrong and the program is not carrying on with it. On by default, and the level that must never be silently lost — which is the whole reason the default threshold is not `off`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/log.bx#L427)


{% endraw %}
