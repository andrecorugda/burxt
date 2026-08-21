---
layout: doc
title: lib/time.bx
section: reference
description: "Dates and durations, in whole seconds, in UTC."
---

{% raw %}

# `lib/time.bx`

Dates and durations, in whole seconds, in UTC.

```burxt
use "lib/time.bx";
```

`os_now()` answers seconds since 1970 and nothing else: no formatting, no arithmetic, no calendar. So a Burxt program could tell you the time was 1785312000 and could not tell you it was a Tuesday. §D1j is that gap, and *"time + date + randomness"* is one of the four must-ships for 1.0.

---- the three limits, and all three are in the names or right here ---------------------

**1. UTC ONLY.** There is no timezone parameter, no local time, and no `%Z`. `DESIGN.md` committed to it: *"dates/timezones, when they come, arrive timezone-explicit or not at all."* A function called `to_local_time` that read `TZ` would give a different answer on a developer's laptop than on the server, and both would look right — the silent-wrong-answer shape this language exists to refuse. The same call `string_to_upper_ascii` records by naming ASCII in its own name.

So `time_format_iso` always ends in `Z`, and `time_parse_iso` **refuses an offset**: `"+05:00"` is not read as UTC, it is answered with `None`. Reading it as UTC would be wrong by five hours while looking like it worked, and converting it would need the timezone arithmetic that is not here.

**2. WHOLE SECONDS FROM `os_now`, MICROSECONDS FROM `time_wall_micros`, AND NO MONOTONIC CLOCK.**

**This paragraph said "no milliseconds, no monotonic clock … blocked on A7" and shipped that way in 1.6.0 and 1.7.0, five hundred lines above `time_wall_micros`, which answers microseconds.** A7 integer widths turned out not to be needed: `c_bytes_at` hands over the sixteen bytes of a `struct timespec` and `time_i64_at` reassembles them. Found 2026-08-21 when the comet session asked for a sub-second clock and reported measuring elapsed time with `date +%s%3N` **from a Burxt program** — they read this paragraph, believed it, and reached for the shell. The capability was in the tarball they already had.

That is the most expensive stale claim this repository has produced: a present-tense sentence in SHIPPED documentation, contradicted by code in the same file, that talked a consumer out of the language. The reference page carried both halves and a reader who starts at the top stops here.

**What is still true is the monotonic half, and the reason has not changed.** `CLOCK_MONOTONIC` is **1 on Linux and 6 on macOS**, Burxt has no conditional compilation — a recorded decision, not a gap — so no single program can name both. `CLOCK_REALTIME` is 0 on both, which is why `time_wall_micros` is a WALL clock and says so: it can step backwards when the machine's time is corrected, so a duration measured across an NTP correction can be negative. Fine for timing a compile or a request. Not fine for a timeout that must never go backwards.

**3. NO LEAP SECONDS**, because Unix time has none. **A day here is always exactly 86400 seconds.** That is not an approximation this file chose, it is the definition of the scale `time()` answers on: 2016-12-31T23:59:60Z, a real UTC second, has no unix representation at all. Modelling leap seconds would mean shipping a table that expires — the IERS announces them six months ahead — and a date library whose answers change when you update a table is not what a reproducible language should hand you. Durations here are counts of 86400-second days, and the two places that matters are stated where they arise.

**No floats anywhere.** Every function below is `divide_floor`, `remainder`, `+`, `-` and `*` on Ints. That is the language's identity rather than a preference, and for dates it is also just correct: a float cannot hold a second-precise instant past 2^53 seconds, and rounding a date is how a timestamp lands on the wrong day at midnight.

---- the calendar conversion, and why it is exact ---------------------------------------

`time_days_from_civil` and `time_civil_from_days` are **Howard Hinnant's algorithms** from *chrono-Compatible Low-Level Date Algorithms*, which are the standard exact-integer pair and the basis of C++20's `<chrono>`. No lookup table, no loop over years, no floating point — closed-form integer arithmetic in both directions, over the **proleptic Gregorian calendar** (the Gregorian rules extended backwards forever, which is what ISO-8601 specifies and what unix time assumes).

They shift the year to start in MARCH, which is the trick that makes them table-free: with February last, the leap day is the final day of the year and every other month's length follows the repeating 31/30 pattern that `(153*m + 2)/5` generates. That is why `m <= 2` borrows a year.

**Burxt makes them shorter than the C++.** Hinnant's version computes the era as `(y >= 0 ? y : y-399) / 400`, and that conditional exists only to make C++'s truncating `/` behave like floor division on negative years. Burxt refuses a bare `/` on two Ints and makes you say which you meant, so `divide_floor(y, 400)` **is** the intended operation and both sign-correction hacks disappear — one in each direction. A language that made an operator's meaning explicit made the algorithm that depended on it plainer, which is the argument `divide_floor` was added for, arriving somewhere nobody planned it.

Verified rather than trusted: a round trip over 2,000,001 consecutive days including every negative one, and 200,000 random dates in years 1..9999 checked field-by-field against a known-good calendar. Both are in `tests/pass/time_library.bx` in the form a fixture can hold.

**Dates before 1970 work.** Unix seconds go negative and so does everything here; there is no floor at the epoch, because a date of birth is the first thing anybody asks a date library for.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`DateTime`](#datetime) | class | A date and a time of day, in UTC. Six Ints and nothing else — no timezone field, because there is one timezone; no fract |
| [`Duration`](#duration) | class | An elapsed count of seconds — **not** an instant. |
| [`duration_seconds`](#duration-seconds) | function | — |
| [`duration_minutes`](#duration-minutes) | function | — |
| [`duration_hours`](#duration-hours) | function | — |
| [`duration_days`](#duration-days) | function | A day is 86400 seconds here, always — see limit 3. For calendar-day arithmetic across a date that a human would call "th |
| [`duration_total_seconds`](#duration-total-seconds) | function | — |
| [`duration_total_minutes`](#duration-total-minutes) | function | The whole units contained, rounded toward NEGATIVE INFINITY rather than toward zero — because `divide_floor` is the oper |
| [`duration_total_hours`](#duration-total-hours) | function | — |
| [`duration_total_days`](#duration-total-days) | function | — |
| [`time_add`](#time-add) | function | A unix timestamp `span` later. Refuses rather than wraps: a date library that answered the year -292277022657 for "a cen |
| [`time_between`](#time-between) | function | How long from `from_unix` to `to_unix`. **Negative when the second is earlier**, which is the honest answer rather than  |
| [`time_floor_mod`](#time-floor-mod) | function | Floor modulo — the remainder with the sign of the DIVISOR, so it is never negative for a positive divisor. Burxt's `rema |
| [`time_is_leap_year`](#time-is-leap-year) | function | Proleptic Gregorian: divisible by 4, except centuries, except every fourth century. Correct for negative years too, beca |
| [`time_days_in_month`](#time-days-in-month) | function | 28, 29, 30 or 31. A table would be four lines shorter and would need `from_bytes` to be indexed by anything but a chain  |
| [`time_days_from_civil`](#time-days-from-civil) | function | Days since 1970-01-01, which is day 0. Hinnant's `days_from_civil`. |
| [`time_civil_from_days`](#time-civil-from-days) | function | The inverse, as a DateTime at midnight. Hinnant's `civil_from_days`. |
| [`time_from_unix`](#time-from-unix) | function | The civil instant at that many seconds since 1970. **Total** — every `Int` is a valid instant, including every negative  |
| [`time_is_valid`](#time-is-valid) | function | Is this a real instant? Every field in range, and the day within the month's actual length — so 2023-02-29 is false and  |
| [`time_to_unix`](#time-to-unix) | function | Seconds since 1970. A `requires` rather than an `Option`, because an invalid DateTime is a program that built a date wro |
| [`time_weekday`](#time-weekday) | function | ISO-8601 weekday: **Monday is 1 and Sunday is 7.** |
| [`time_day_of_year`](#time-day-of-year) | function | Day of the year, 1..366. 1 January is 1. |
| [`time_pad`](#time-pad) | function | Zero-padded to `width`. Longer values are NOT truncated — a year of 12345 formats as five digits rather than silently be |
| [`time_format_iso`](#time-format-iso) | function | `2026-08-01T12:34:56Z` — RFC 3339, which is the ISO-8601 profile everything actually speaks. |
| [`time_format_date`](#time-format-date) | function | Just the date: `2026-08-01`. The other ISO-8601 form a program is handed, and the one a CSV column usually holds. |
| [`time_parse_field`](#time-parse-field) | function | A fixed-width run of digits as a number, or None. The building block the parser needs and `string_parse_int` is not: `st |
| [`time_parse_iso`](#time-parse-iso) | function | * **An offset.** `"2026-08-01T12:34:56+05:00"` answers `None`. Treating it as UTC would be |
| [`time_parse_unix`](#time-parse-unix) | function | Seconds since 1970 straight from ISO-8601 text, or None. The composition a caller reaching for this module usually wants |
| [`time_i64_at`](#time-i64-at) | function | One little-endian i64 out of a byte array, starting at `from`. |
| [`time_wall_micros`](#time-wall-micros) | function | The wall clock in microseconds since the epoch, or `None` if the clock could not be read. |
| [`time_since_micros`](#time-since-micros) | function | Microseconds between two readings. Answers `None` when either reading failed, and the caller is expected to notice — a d |

## Types
{: #types}

### `DateTime`
{: #datetime}

```burxt
class DateTime { year: Int, month: Int, day: Int, hour: Int, minute: Int, second: Int }
```

A date and a time of day, in UTC. Six Ints and nothing else — no timezone field, because there is one timezone; no fractional field, because there is no sub-second time.

The fields are public and a program may build one directly. Whether the result is a real date is a separate question, answered by `time_is_valid` and demanded by `time_to_unix`'s contract, which is the split this language usually makes: construction is cheap, and the promise is checked where it is relied on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L122)

### `Duration`
{: #duration}

```burxt
class Duration { seconds: Int }
```

An elapsed count of seconds — **not** an instant.

A class rather than a bare `Int`, and this is the one design decision here that could have gone either way. `lib/math.bx` is free functions over `Int`; `lib/set.bx` is a class. What decides it is that **a timestamp and a duration are both seconds and adding two timestamps is meaningless.** `1785312000 + 1785312000` compiles and is nonsense. `time_add(now, duration_hours(3))` cannot be given a second timestamp, because a timestamp is not a `Duration` — the wrapper is what makes the mistake a compile error instead of a date in the year 2083.

That is `DESIGN.md`'s own instinct applied one field over: it says cross-currency arithmetic will need *"the currencies to be distinct types (nominal records already give this shape)"*. Seconds elapsed and seconds since the epoch are the same argument with a smaller stake.

The cost is real, measured, and worth naming in full — two things, not one:

* **No `+`.** Operators do not apply to a class, so adding two durations is

```burxt
 `duration_seconds(duration_total_seconds(a) + duration_total_seconds(b))`.
```

* **No `print`.** `print(span)` is refused with *"print does not know how to show a Duration —

```burxt
 print its fields"*, so showing one means `print(duration_total_seconds(span))`.
```

The constructors below are what makes that trade pay: `duration_hours(3)` says its unit at the call site, where `10800` needs a comment and `10800 / 60 / 60` needs checking.

**A duration may be negative**, which is what `time_between` answers when its arguments are in the other order. That is a real answer, not an error: "three hours earlier" is a duration.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L149)

## Functions
{: #functions}

### `duration_seconds`
{: #duration-seconds}

```burxt
pure function duration_seconds(count: Int) -> Duration
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L158)

### `duration_minutes`
{: #duration-minutes}

```burxt
pure function duration_minutes(count: Int) -> Duration
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L162)

### `duration_hours`
{: #duration-hours}

```burxt
pure function duration_hours(count: Int) -> Duration
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L168)

### `duration_days`
{: #duration-days}

```burxt
pure function duration_days(count: Int) -> Duration
```

A day is 86400 seconds here, always — see limit 3. For calendar-day arithmetic across a date that a human would call "the same time tomorrow", that is the same thing, because UTC has no daylight saving. It is only different from a civil day in a timezone that shifts, and there are none here.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L177)

### `duration_total_seconds`
{: #duration-total-seconds}

```burxt
pure function duration_total_seconds(span: Duration) -> Int
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L183)

### `duration_total_minutes`
{: #duration-total-minutes}

```burxt
pure function duration_total_minutes(span: Duration) -> Int
```

The whole units contained, rounded toward NEGATIVE INFINITY rather than toward zero — because `divide_floor` is the operation whose answer is monotonic, so a duration one second longer never answers fewer minutes. `divide_toward_zero` would break that across zero: -90 seconds is -2 minutes here and would be -1 there, sitting between -60 and -120 which both answer differently. The same reason `array_sort` is stable: the surprising case is the one that has to behave.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L192)

### `duration_total_hours`
{: #duration-total-hours}

```burxt
pure function duration_total_hours(span: Duration) -> Int
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L196)

### `duration_total_days`
{: #duration-total-days}

```burxt
pure function duration_total_days(span: Duration) -> Int
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L200)

### `time_add`
{: #time-add}

```burxt
pure function time_add(unix_seconds: Int, span: Duration) -> Int
```

A unix timestamp `span` later. Refuses rather than wraps: a date library that answered the year -292277022657 for "a century after now" would be worse than one that stopped.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L208)

### `time_between`
{: #time-between}

```burxt
pure function time_between(from_unix: Int, to_unix: Int) -> Duration
```

How long from `from_unix` to `to_unix`. **Negative when the second is earlier**, which is the honest answer rather than an absolute value: the caller asked a directed question.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L216)

### `time_floor_mod`
{: #time-floor-mod}

```burxt
pure function time_floor_mod(value: Int, by: Int) -> Int
```

Floor modulo — the remainder with the sign of the DIVISOR, so it is never negative for a positive divisor. Burxt's `remainder` takes the sign of the dividend (`remainder(-7, 3)` is -1), which is the wrong end for a calendar: a weekday index or a leap-year test must not go negative in 1969.

**Built by CORRECTING the remainder, not by `value - divide_floor(value, by) * by`** — and that is not a style choice, it is a bug the fixture caught. The subtract-the-product form overflows at `INT_MIN`: `divide_floor(INT_MIN, 86400)` is -106751991167301, and multiplying that back by 86400 gives -9223372036854806400, which is below `INT_MIN` by 30592 — exactly the answer being sought. So the arithmetic that computes the remainder trapped on the one input where the remainder was most interesting, and `time_from_unix` was documented TOTAL while dying on `INT_MIN`.

This form cannot overflow anywhere: `remainder` is bounded by `by` in magnitude, and adding `by` to a value in `(-by, 0]` lands in `(0, by]`. Total for every `Int`, which is what the two callers below promise.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L238)

### `time_is_leap_year`
{: #time-is-leap-year}

```burxt
pure function time_is_leap_year(year: Int) -> Bool
```

Proleptic Gregorian: divisible by 4, except centuries, except every fourth century. Correct for negative years too, because `time_floor_mod` never answers negative — year -4 is a leap year and year -1 is not, which a truncating remainder would get backwards.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L251)

### `time_days_in_month`
{: #time-days-in-month}

```burxt
pure function time_days_in_month(year: Int, month: Int) -> Int
```

28, 29, 30 or 31. A table would be four lines shorter and would need `from_bytes` to be indexed by anything but a chain of comparisons, so the comparisons are written out.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L263)

### `time_days_from_civil`
{: #time-days-from-civil}

```burxt
pure function time_days_from_civil(year: Int, month: Int, day: Int) -> Int
```

Days since 1970-01-01, which is day 0. Hinnant's `days_from_civil`.

Every division here is `divide_floor`, and only the first one needs to be: `yoe`, `doe` and `doy` are all non-negative by construction, so floor and truncation agree on them. Using one name throughout means a reader checks the sign question once instead of at every division.

The March shift is the whole trick — see the file header. `m <= 2` borrows a year so that February is last, which puts the leap day at the end where it disturbs nothing.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L287)

### `time_civil_from_days`
{: #time-civil-from-days}

```burxt
pure function time_civil_from_days(days: Int) -> DateTime
```

The inverse, as a DateTime at midnight. Hinnant's `civil_from_days`.

146097 is the days in a 400-year era, which is exact — the Gregorian cycle repeats every 400 years with no remainder, and that is the fact the whole closed form rests on.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L314)

### `time_from_unix`
{: #time-from-unix}

```burxt
pure function time_from_unix(unix_seconds: Int) -> DateTime
```

The civil instant at that many seconds since 1970. **Total** — every `Int` is a valid instant, including every negative one, so there is no precondition and nothing to check.

`divide_floor` and `time_floor_mod` rather than truncation, and this is exactly the bug that makes a naive implementation wrong for one day in every negative timestamp: -1 second is 1969-12-31T23:59:59Z, so the day index is -1 and the second-of-day is 86399. Truncating division answers day 0 and -1 seconds, which is 1970-01-01 at minus one second — the wrong date and an impossible time.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L345)

### `time_is_valid`
{: #time-is-valid}

```burxt
pure function time_is_valid(when: DateTime) -> Bool
```

Is this a real instant? Every field in range, and the day within the month's actual length — so 2023-02-29 is false and 2024-02-29 is true.

`pure`, so `time_to_unix`'s contract can be one clause that says the whole thing instead of nine that say it piecewise. The year bound is what keeps the multiplication below from overflowing.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L364)

### `time_to_unix`
{: #time-to-unix}

```burxt
pure function time_to_unix(when: DateTime) -> Int
```

Seconds since 1970. A `requires` rather than an `Option`, because an invalid DateTime is a program that built a date wrong, not input that happened to be bad — and `time_parse_iso` is the one that takes input and it answers `Option`. Same split `char_at` and `string_parse_int` make.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L387)

### `time_weekday`
{: #time-weekday}

```burxt
pure function time_weekday(when: DateTime) -> Int
```

ISO-8601 weekday: **Monday is 1 and Sunday is 7.**

ISO's numbering rather than C's `tm_wday` (Sunday 0), because this module formats ISO-8601 and one convention per file beats two. The epoch was a Thursday, so day 0 must answer 4, and `+ 3` before the modulo is what places it — checked in the fixture against four known dates rather than reasoned about, because an off-by-one here is invisible until it is a Monday report run on Sunday.

`time_floor_mod` and not `remainder`: for any date before 1970 the day index is negative, and a remainder that kept that sign would answer a weekday of -2.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L406)

### `time_day_of_year`
{: #time-day-of-year}

```burxt
pure function time_day_of_year(when: DateTime) -> Int
```

Day of the year, 1..366. 1 January is 1.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L414)

### `time_pad`
{: #time-pad}

```burxt
pure function time_pad(value: Int, width: Int) -> String allocates
```

Zero-padded to `width`. Longer values are NOT truncated — a year of 12345 formats as five digits rather than silently becoming 2345, because dropping a digit from a date is the kind of quiet corruption this file is written to avoid. `time_format_iso`'s contract is what keeps that from arising there.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L427)

### `time_format_iso`
{: #time-format-iso}

```burxt
pure function time_format_iso(when: DateTime) -> String allocates
```

`2026-08-01T12:34:56Z` — RFC 3339, which is the ISO-8601 profile everything actually speaks.

Always `Z`, never an offset, because there is one timezone here. See limit 1.

**`requires` a year in 0..9999**, and that is the FORMAT's limit rather than this library's: the four-digit field cannot hold a year outside it, and ISO-8601's expanded form (`+0012026-…`) requires the sender and receiver to have agreed on how many digits, which is a negotiation and not a default. `time_to_unix` and `time_from_unix` have the full range; only the text form is narrow, and the contract says where the narrowing is.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L447)

### `time_format_date`
{: #time-format-date}

```burxt
pure function time_format_date(when: DateTime) -> String allocates
```

Just the date: `2026-08-01`. The other ISO-8601 form a program is handed, and the one a CSV column usually holds.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L459)

### `time_parse_field`
{: #time-parse-field}

```burxt
pure function time_parse_field(text: String, at: Int, width: Int) -> Option<Int>
```

A fixed-width run of digits as a number, or None. The building block the parser needs and `string_parse_int` is not: `string_parse_int` accepts a leading `-`, so it would read `"-1"` out of a month field, and it accepts `"7"` where the format demands `"07"`.

`all_digits` from `lib/string.bx` is what makes this two lines — every byte a digit, then the ordinary parse. Written after that function existed rather than around its absence.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L473)

### `time_parse_iso`
{: #time-parse-iso}

```burxt
pure function time_parse_iso(text: String) -> Option<DateTime>
```

* **An offset.** `"2026-08-01T12:34:56+05:00"` answers `None`. Treating it as UTC would be

```burxt
 wrong by five hours while looking right, and converting it needs timezone arithmetic that is
 not here — see limit 1. This is the single most important `None` in the file.
```

* **A lower-case `t` or `z`.** RFC 3339 permits them; this does not, because one spelling per

```burxt
 concept is cheaper to keep right than two, and a rejected timestamp is a fixable bug where a
 silently-accepted variant is a divergence between two readers.
```

* **A space instead of `T`.** Same reason. * **A fractional part.** `".500"` has no representation here — see limit 2 — and dropping it

```burxt
 would silently lose precision the sender thought it was sending.
```

* **An unpadded field.** `"2026-8-1"` is not ISO-8601; the widths are fixed and this reads them

```burxt
 by position, which is why `time_parse_field` demands exactly `width` digits.
```

* **An impossible date.** `"2023-02-29"` parses as digits and then fails `time_is_valid`, so it

```burxt
 answers `None` rather than the first of March. A parser that normalises is a parser that
 accepts a typo.
```

* **A year outside 0..9999** cannot arise: the four-digit field cannot express one.

A leading `+` or `-` for an expanded year is also refused, which is the same negotiation `time_format_iso` declines to guess at.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L507)

### `time_parse_unix`
{: #time-parse-unix}

```burxt
pure function time_parse_unix(text: String) -> Option<Int>
```

Seconds since 1970 straight from ISO-8601 text, or None. The composition a caller reaching for this module usually wants, and the reason `time_parse_iso` returns the DateTime rather than the seconds: one of the two is derivable and the other is not.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L547)

### `time_i64_at`
{: #time-i64-at}

```burxt
pure function time_i64_at(bytes: [Int], from: Int) -> Int
```

One little-endian i64 out of a byte array, starting at `from`.

Eight bytes reassembled by hand because Burxt reads a C buffer as bytes and has no way to say "the i64 at this offset" — the pointer wall hands over bytes and nothing else, which is what makes it a wall.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L584)

### `time_wall_micros`
{: #time-wall-micros}

```burxt
function time_wall_micros() -> Option<Int> touches clock
```

The wall clock in microseconds since the epoch, or `None` if the clock could not be read.

`None` rather than 0: a clock that failed and a clock reading zero are different facts, and zero is a real instant (1970) that a duration calculation would silently accept.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L601)

### `time_since_micros`
{: #time-since-micros}

```burxt
function time_since_micros(started: Int) -> Option<Int> touches clock
```

Microseconds between two readings. Answers `None` when either reading failed, and the caller is expected to notice — a duration is exactly the place a missing measurement must not become zero.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/time.bx#L620)


{% endraw %}
