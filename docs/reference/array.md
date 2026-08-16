---
layout: doc
title: lib/array.bx
section: reference
description: "The operations on a growable array that every program reaches for."
---

{% raw %}

# `lib/array.bx`

The operations on a growable array that every program reaches for.

```burxt
use "lib/array.bx";
```

This file could not be written until v0.0.201, and the reason is worth stating because it was misdiagnosed for a long time: **no function could modify an array it was passed.** Not in place, not by `push`. My own notes blamed generic `xs[i] = v`; that was wrong, and generics were never involved. A parameter was simply always a copy, and `mutable` on one was a parse error.

So `sort`, `reverse`, `swap` and `fill` — every operation whose whole job is to change what it was given — were unwritable, and this module was a line in a plan for six versions.

---- how to read a signature here ------------------------------------------------------

**`mutable xs: [T]` means the call changes YOUR array.** That is the only thing you need to check, and it is in the signature rather than at the call site on purpose: the same rule `mutable self` follows, and the reason both follow it is that a promise belongs where it is made once, not at every place it is used. A function without `mutable` cannot touch what you passed, and the compiler enforces that rather than the documentation asking nicely.

```burxt
 sort(mutable xs)      changes xs
 index_of(xs, of)      cannot
```

---- what is NOT here, and why ---------------------------------------------------------

**Sorting Strings works** (v0.0.202), by BYTE order — so "Zebra" comes before "apple", because `Z` is 90 and `a` is 97. That is not alphabetical order in any language, and it is deliberate: locale collation means picking a language and one of its several orders, which is a decision nobody wrote down. Byte order needs no decision and is identical on every machine, which is what a reproducible sort is built on.

~~**No `map` or `filter`.** Those need function values, which the language does not have.**~~ **Both exist as of v0.0.279**, along with `fold`, `any`, `all`, `position`, `retain`, `partition` and `sort_by` — roadmap D2d, and see the `// ---- higher order ----` section below.

The claim was true when written and the diagnosis was right: they DO need function values. What changed is that the language got them without getting closures. `dynamic Trait` had been a function value since v0.0.14; A9 made it generic, which is the whole of what was missing, and A10 (closures) was then closed without being built. The vocabulary lives in `lib/fn.bx` and the reasoning for its shape lives there too.

The old advice — *"write the loop; it is three lines and a reader can see what it does"* — is still right for a one-off, and nothing here deprecates a `for` loop. What a named function buys is the case where the loop is NOT three lines: `array_sort_by` is a stable merge sort, and nobody should write that inline.

~~**`array_index_of` is not `pure`, and it should be.**~~ **Fixed in v0.0.248, and this comment had the diagnosis exactly right.** It said `Option.Some(i)` "reads as a method call on the enum" and that the `pure` checker "cannot see through enum construction". That was the mechanism: a variant constructor PARSES as a method call and is told apart inside the method-call branch, but the blanket *"a pure function may not call a method"* refusal sat at the TOP of that branch, before anything looked at whether the receiver was an enum. So a constructor was refused for being SHAPED like a method call.

One removal fixed both halves of roadmap A4 — `pure` on a method, and `pure` returning an Option — which had been listed as two related items and were one branch. **A comment that names a mechanism rather than a symptom is worth more than a bug report**, and this one was load-bearing two years after it was written.

~~**An Option-returning GENERIC is not writable at all.**~~ **Fixed in v0.0.241.** `Option.None` and `Option.Some` now resolve their type argument from the enclosing signature, so `array_pop<T> -> Option<T>` compiles and runs in both compilers — measured.

`min` and `max` still take a precondition, and that stays: *"the largest of nothing"* is not a question with a wrong answer, it is a question that should not have been asked, which is what `requires` says. **The limitation was never the reason for that design and the note claimed it was** — a real reason and a convenient excuse for it had been written down together, and only one of them was load-bearing.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`array_contains`](#array-contains) | function | Is this value in the array? `T: Equatable` because that is exactly the promise `==` needs. |
| [`array_index_of`](#array-index-of) | function | Where it first appears, or None. |
| [`array_is_sorted`](#array-is-sorted) | function | — |
| [`array_min`](#array-min) | function | The smallest and largest. A PRECONDITION rather than an Option, and the reason is the better one rather than the conveni |
| [`array_max`](#array-max) | function | — |
| [`array_first`](#array-first) | function | — |
| [`array_last`](#array-last) | function | — |
| [`array_sum_int`](#array-sum-int) | function | A total, and it TRAPS on overflow rather than wrapping — which is the same promise `+` makes, kept here rather than quie |
| [`array_sum_money`](#array-sum-money) | function | — |
| [`array_swap`](#array-swap) | function | Exchange two elements. The one operation `sort` and `reverse` are both built from, so it is worth having by name — and t |
| [`array_reverse`](#array-reverse) | function | — |
| [`array_sort`](#array-sort) | function | Insertion sort, and the choice is deliberate rather than lazy. |
| [`array_fill`](#array-fill) | function | Every element set to one value. Keeps the length: this fills what is there rather than growing. |
| [`array_extend`](#array-extend) | function | Append every element of `from` to `xs`. `from` is NOT mutable, and the asymmetry is the point: one of these two arrays c |
| [`array_remove_at`](#array-remove-at) | function | Remove the element at `at`, closing the gap. Order is preserved, which is why this shifts rather than swapping the last  |
| [`array_copy`](#array-copy) | function | A NEW array with the same elements. |
| [`array_slice`](#array-slice) | function | The elements from `from` up to but NOT including `to`, as a new array. |
| [`array_concat`](#array-concat) | function | Both arrays' elements, in order, as a NEW one. Neither argument changes — which is the difference between this and `arra |
| [`array_insert_at`](#array-insert-at) | function | Put `value` at `at`, shifting everything from there along. `at == len(xs)` appends, which is why the precondition is `<= |
| [`array_count_of`](#array-count-of) | function | How many times a value appears. `array_contains` asks whether it appears at all; this is the count, and having both mean |
| [`array_equals`](#array-equals) | function | Same length and equal elements in the same order. |
| [`array_binary_search`](#array-binary-search) | function | The index of `of`, found in O(log n) — or None. |
| [`array_dedup`](#array-dedup) | function | Drop ADJACENT duplicates, in place, answering the new length. |
| [`array_map`](#array-map) | function | Every element through `f`, as a new array. `[T]` in, `[U]` out — so this is the type-changing form, and `Mapper<T, T>` c |
| [`array_filter`](#array-filter) | function | The elements that pass, as a new array, in their original order. |
| [`array_fold`](#array-fold) | function | Left fold: `start`, then one `step` per element, in order. |
| [`array_any`](#array-any) | function | Does any element pass? False for an empty array, and it stops at the first one that does. |
| [`array_all`](#array-all) | function | Do all of them pass? **True for an empty array** — vacuously, because there is no element that fails. That answer surpri |
| [`array_position`](#array-position) | function | The index of the first element that passes, or None. The predicate counterpart of `array_index_of`, and `Option<Int>` fo |
| [`array_retain`](#array-retain) | function | Keep the elements that pass, IN PLACE, answering the new length. |
| [`array_partition`](#array-partition) | function | Split into those that pass and those that do not: `(passing, failing)`, both new arrays, each in original order. |
| [`array_sort_by`](#array-sort-by) | function | **Bottom-up merge sort**, where `array_sort` is insertion sort. Two different answers to two different questions, and th |
| [`array_zip`](#array-zip) | function | The two arrays walked together, stopping at the shorter. `zip([1,2,3], ["a","b"])` has two pairs. |
| [`array_enumerate`](#array-enumerate) | function | Every element with its index. `enumerate(["a","b"])` is `[(0,"a"), (1,"b")]`. |
| [`array_split_at`](#array-split-at) | function | Two arrays: everything before `at`, and everything from `at`. A negative `at` is 0 and one past the end is the whole arr |

## Functions
{: #functions}

### `array_contains`
{: #array-contains}

```burxt
pure function array_contains<T: Equatable>(xs: [T], of: T) -> Bool
```

Is this value in the array? `T: Equatable` because that is exactly the promise `==` needs.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L80)

### `array_index_of`
{: #array-index-of}

```burxt
pure function array_index_of<T: Equatable>(xs: [T], of: T) -> Option<Int>
```

Where it first appears, or None.

`Option<Int>` rather than -1, and the difference is the whole habit this language is built on: -1 is a valid Int, so a caller who forgets to check gets an index rather than a mistake. None cannot be used as an index by accident.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L96)

### `array_is_sorted`
{: #array-is-sorted}

```burxt
pure function array_is_sorted<T: Ordered>(xs: [T]) -> Bool
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L107)

### `array_min`
{: #array-min}

```burxt
function array_min<T: Ordered>(xs: [T]) -> T
```

The smallest and largest. A PRECONDITION rather than an Option, and the reason is the better one rather than the convenient one: "the largest of nothing" is not a question with a wrong answer, it is a question that should not have been asked — which is exactly what `requires` says. The same call `vector_normalise` makes about the zero vector.

(There is also a language reason, and it is worth recording because it is a real gap rather than a preference: `Option.None` could not be built where `T` was a type PARAMETER, and even `let nothing: Option<T> = Option.None;` was refused. **Both work as of v0.0.241** — the claim about the `let` was TRUE when written and was verified before being retired rather than assumed stale.

The cause turned out to be inverted from how it read: it was not that methods had this inference and free functions lacked it. **A free generic function's body is checked ABSTRACTLY, and a generic method's body never is** — `lib/map.bx`'s `find -> Option<V>` has shipped since v0.0.118 not because methods resolve it correctly but because they never enter the state that broke free functions. There was no working path to copy.)

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L133)

### `array_max`
{: #array-max}

```burxt
function array_max<T: Ordered>(xs: [T]) -> T
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L147)

### `array_first`
{: #array-first}

```burxt
function array_first<T>(xs: [T]) -> T
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L161)

### `array_last`
{: #array-last}

```burxt
function array_last<T>(xs: [T]) -> T
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L167)

### `array_sum_int`
{: #array-sum-int}

```burxt
pure function array_sum_int(xs: [Int]) -> Int
```

A total, and it TRAPS on overflow rather than wrapping — which is the same promise `+` makes, kept here rather than quietly relaxed because this is a loop. A total that silently wrapped would be the exact wrong answer this language exists to refuse, and a sum is where money lives.

Two of them rather than one generic, because `0` and `$0.00` are different values and a generic has no way to name "the zero of T" — there is no `Default`, deliberately.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L179)

### `array_sum_money`
{: #array-sum-money}

```burxt
pure function array_sum_money(xs: [Decimal<2>]) -> Decimal<2>
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L189)

### `array_swap`
{: #array-swap}

```burxt
function array_swap<T>(mutable xs: [T], i: Int, j: Int) -> Int
```

Exchange two elements. The one operation `sort` and `reverse` are both built from, so it is worth having by name — and the precondition is not defensive: an index outside the array has no element to exchange, and answering anyway is how a swap silently corrupts a neighbour.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L207)

### `array_reverse`
{: #array-reverse}

```burxt
function array_reverse<T>(mutable xs: [T]) -> Int
```

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L219)

### `array_sort`
{: #array-sort}

```burxt
function array_sort<T: Ordered>(mutable xs: [T]) -> Int
```

Insertion sort, and the choice is deliberate rather than lazy.

It is **stable** — equal elements keep the order they were written in — which for records sorted by one field is the difference between a reproducible listing and one that reshuffles between runs. It needs **no extra storage**, so it does not allocate and can be called from a function that declares `allocates nothing`. And it is **O(n) on an already-sorted array**, which is what a program that sorts after every append actually does.

It is O(n²) on reversed input, and that is the honest cost. A merge sort would need a scratch array and a way to allocate one; a quicksort would lose stability and need a pivot choice nobody wrote down. When a corpus arrives that this is too slow for, the answer is a second named function rather than quietly changing this one, because a caller may be depending on the stability.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L244)

### `array_fill`
{: #array-fill}

```burxt
function array_fill<T>(mutable xs: [T], with: T) -> Int
```

Every element set to one value. Keeps the length: this fills what is there rather than growing.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L262)

### `array_extend`
{: #array-extend}

```burxt
function array_extend<T>(mutable xs: [T], from: [T]) -> Int
```

Append every element of `from` to `xs`. `from` is NOT mutable, and the asymmetry is the point: one of these two arrays changes, and the signature says which.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L273)

### `array_remove_at`
{: #array-remove-at}

```burxt
function array_remove_at<T>(mutable xs: [T], at: Int) -> Int
```

Remove the element at `at`, closing the gap. Order is preserved, which is why this shifts rather than swapping the last element into the hole — a faster removal that reorders is a different function and would need a different name to say so.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L285)

### `array_copy`
{: #array-copy}

```burxt
function array_copy<T>(xs: [T]) -> [T]
```

A NEW array with the same elements.

**Read this one before you assume you do not need it.** Assigning an array does NOT copy it:

```burxt
 let mutable b: [Int] = a;
 b[0] = 99;                    // a[0] is now 99
```

That is measured, in both compilers, not inferred from the memory model. An array value carries a pointer to its buffer, and assignment copies the value, so both names see one buffer. Every function in this file that answers a new array therefore builds it with `push` rather than starting from an assignment — and a caller who wants an independent array has to say so, which is what this is for.

It is not a deep copy. If `T` is a class, the elements are still shared; there is no way to write a deep copy generically, because there is no interface saying how to clone a `T`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L315)

### `array_slice`
{: #array-slice}

```burxt
function array_slice<T>(xs: [T], from: Int, to: Int) -> [T]
```

The elements from `from` up to but NOT including `to`, as a new array.

End-EXCLUSIVE, matching `string_slice` and the reason it chose the same: `to - from` is the length, `array_slice(xs, 0, len(xs))` is the whole thing, and two adjacent slices meet without an off-by-one at the seam. Inclusive ends read more naturally in one call and wrongly in every composition of two.

An empty slice is legal — `from == to` answers `[]` — because a loop that narrows a range down to nothing should not have to special-case its last step. `from > to` is not legal, because that is a reversed range rather than an empty one, and answering `[]` would hide the caller's arithmetic bug.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L335)

### `array_concat`
{: #array-concat}

```burxt
function array_concat<T>(first: [T], second: [T]) -> [T]
```

Both arrays' elements, in order, as a NEW one. Neither argument changes — which is the difference between this and `array_extend`, and the reason both exist.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L351)

### `array_insert_at`
{: #array-insert-at}

```burxt
function array_insert_at<T>(mutable xs: [T], at: Int, value: T) -> Int
```

Put `value` at `at`, shifting everything from there along. `at == len(xs)` appends, which is why the precondition is `<=` and not `<` — inserting at the end is the ordinary case of a loop that inserts in order, not an edge to be refused.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L369)

### `array_count_of`
{: #array-count-of}

```burxt
pure function array_count_of<T: Equatable>(xs: [T], of: T) -> Int
```

How many times a value appears. `array_contains` asks whether it appears at all; this is the count, and having both means neither caller writes the loop.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L385)

### `array_equals`
{: #array-equals}

```burxt
pure function array_equals<T: Equatable>(first: [T], second: [T]) -> Bool
```

Same length and equal elements in the same order.

Explicit rather than `a == b`, because `==` on two arrays is not defined and this says what the comparison IS: order-sensitive, so `[1, 2]` and `[2, 1]` differ. A set comparison is `lib/set.bx`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L401)

### `array_binary_search`
{: #array-binary-search}

```burxt
pure function array_binary_search<T: Ordered>(xs: [T], of: T) -> Option<Int>
```

The index of `of`, found in O(log n) — or None.

**The array must be sorted ascending, and that is a requirement on the CALLER rather than a `requires` clause.** The reason is cost and it is worth being explicit about, because omitting a precondition is normally the wrong call in this language: `requires array_is_sorted(xs)` is O(n), which would make an O(log n) function linear and leave a caller no faster than `array_index_of`. A contract that costs more than the function it guards is a contract nobody can afford to keep.

On an unsorted array this answers *something* — possibly None for a value that is present. That is the honest cost of the above. `array_index_of` is the one to reach for when sortedness is not already known, and `array_is_sorted` exists if you want to check once outside a loop.

**On duplicates it answers the FIRST match**, not an arbitrary one. A plain binary search may stop on any of them, which makes the result depend on the length of the array rather than its contents — reproducible in the sense that it is deterministic, useless in the sense that inserting an unrelated element changes it. So this is a lower-bound search followed by one equality test: same O(log n), one answer.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L432)

### `array_dedup`
{: #array-dedup}

```burxt
function array_dedup<T: Equatable>(mutable xs: [T]) -> Int
```

Drop ADJACENT duplicates, in place, answering the new length.

**Adjacent, which means this only removes ALL duplicates from a SORTED array.** On `[1, 2, 1]` nothing is removed, and that is not a bug — it is `uniq`, and it is the version that runs in O(n) with no allocation and no `Ordered` bound. It is also exactly the shape of function this project keeps getting wrong: right about the sorted case somebody wrote, silent about the unsorted case nobody did. So it is said here, and `tests/pass/array_higher_order.bx` pins `[1, 2, 1]` specifically so the behaviour cannot drift into the other one unnoticed.

For all-distinct regardless of order: `array_sort` then this, or `lib/set.bx`, which keeps insertion order and costs a hash per element.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L467)

### `array_map`
{: #array-map}

```burxt
function array_map<T, U>(xs: [T], f: dynamic Mapper<T, U>) -> [U]
```

Every element through `f`, as a new array. `[T]` in, `[U]` out — so this is the type-changing form, and `Mapper<T, T>` covers the case where it does not change.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L519)

### `array_filter`
{: #array-filter}

```burxt
function array_filter<T>(xs: [T], keep: dynamic Predicate<T>) -> [T]
```

The elements that pass, as a new array, in their original order.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L530)

### `array_fold`
{: #array-fold}

```burxt
function array_fold<T, A>(xs: [T], start: A, f: dynamic Folder<T, A>) -> A
```

Left fold: `start`, then one `step` per element, in order.

**`start` is a parameter and there is no version without one**, which is not an omission. A `fold` that used "the zero of T" would need a `Default` interface, and this language deliberately has none — `array_sum_int` and `array_sum_money` are two functions rather than one generic for exactly that reason (`0` and `$0.00` are different values and nothing can name both). Passing the start also makes the empty case total: the fold of nothing is `start`, no precondition needed.

Left rather than right, and only left. A right fold over an array is a left fold over its reverse, and `array_reverse` is right there; shipping both would mean a reader checking which one they have.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L554)

### `array_any`
{: #array-any}

```burxt
function array_any<T>(xs: [T], holds: dynamic Predicate<T>) -> Bool
```

Does any element pass? False for an empty array, and it stops at the first one that does.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L565)

### `array_all`
{: #array-all}

```burxt
function array_all<T>(xs: [T], holds: dynamic Predicate<T>) -> Bool
```

Do all of them pass? **True for an empty array** — vacuously, because there is no element that fails. That answer surprises people often enough to be worth a line: it is what makes `array_all(a, p) && array_all(b, p)` equal `array_all(concat(a, b), p)` for every a and b, and any other choice breaks that. Pinned in the fixture.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L580)

### `array_position`
{: #array-position}

```burxt
function array_position<T>(xs: [T], holds: dynamic Predicate<T>) -> Option<Int>
```

The index of the first element that passes, or None. The predicate counterpart of `array_index_of`, and `Option<Int>` for the same reason that one gives: -1 is a usable index and None is not.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L594)

### `array_retain`
{: #array-retain}

```burxt
function array_retain<T>(mutable xs: [T], keep: dynamic Predicate<T>) -> Int
```

Keep the elements that pass, IN PLACE, answering the new length.

`array_filter`'s mutating twin, and the signature is the whole difference: `mutable xs` says the call changes your array, where `array_filter` cannot touch it. Having both is the point — a filter into a new array is what you want inside an expression, and a retain is what you want when the array is large and the copy is the expensive part.

Compacting rather than repeated removal: one pass, each survivor moved down to the next open slot, then one truncate. Removing in place with `array_remove_at` would be O(n²) — it shifts the whole tail per removal — which for a predicate that rejects most elements is the difference between a scan and a stall.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L616)

### `array_partition`
{: #array-partition}

```burxt
function array_partition<T>(xs: [T], keep: dynamic Predicate<T>) -> ([T], [T])
```

Split into those that pass and those that do not: `(passing, failing)`, both new arrays, each in original order.

A TUPLE return, which A8 landed the same day as A9 — before that this needed a two-field class declared somewhere a caller could reach, which is precisely the record nobody wants to invent. `.0` is the passing half; the order is the same as `filter` first, which is the reading order of the name.

One pass rather than two calls to `array_filter` with opposite predicates: a caller cannot write the negation of an arbitrary `Predicate` without declaring another class, and calling the predicate twice per element would double the vtable cost for an answer already in hand.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L647)

### `array_sort_by`
{: #array-sort-by}

```burxt
function array_sort_by<T>(mutable xs: [T], order: dynamic Comparer<T>) -> Int
```

**Bottom-up merge sort**, where `array_sort` is insertion sort. Two different answers to two different questions, and the deciding factor is one this file did not have before today:

**Every comparison here is a vtable call**, where `array_sort` compares with `<` on an `Ordered` type and the compiler emits it inline. So the COUNT of comparisons stopped being a secondary concern. **Measured**, reversed input, this merge sort against insertion sort with the same comparer — the version this function would be if it had simply copied `array_sort`'s algorithm:

```burxt
 n           this (merge)      insertion sort with a comparer
 20,000      0.00 s              0.28 s
 40,000      0.00 s              1.15 s
 80,000      0.00 s              4.99 s
 2,000,000   0.14 s              not attempted
```

Insertion sort quadruples for every doubling, which is the O(n²) showing up exactly where it should. The last row is the one that settles it: this sorts **25 times as many elements in a thirtieth of the time** that insertion sort needs for 80,000. This is the *"a faster stable sort"* roadmap D1e asked for, arriving with the feature that made a comparison expensive enough to need it.

**`array_sort` stays and is still the right call** for an `Ordered` type: its `<` is inline, so none of the above applies to it, and it allocates nothing — so it can be called where a scratch array cannot be had. Two functions, and the choice is visible in which one you typed.

**Why merge and not quicksort**, given quicksort also gets O(n log n) with no allocation: quicksort is not stable, and stability is the promise in this function's first line. It also needs a pivot rule, and every simple pivot rule has an input shape that makes it quadratic — which for a language arguing about reproducibility is a worse failure than the scratch array.

**The cost, stated:** one scratch array of `n` elements, and `n` writes back per pass — so `n log n` copies on top of `n log n` comparisons. Ping-ponging between the two buffers would halve the copying, but the result has to END in `xs`, so it would depend on the number of passes being even and need a final conditional copy. Not worth the reasoning for a constant factor on the cheap half of the loop; the expensive half is the comparisons, and those are already minimal.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L702)

### `array_zip`
{: #array-zip}

```burxt
function array_zip<A, B>(left: [A], right: [B]) -> [(A, B)]
```

The two arrays walked together, stopping at the shorter. `zip([1,2,3], ["a","b"])` has two pairs.

Stopping at the shorter rather than refusing unequal lengths: the caller who wants them equal has `array_equals` on the lengths and a clearer error than this could give, and the caller who does not want them equal is the common one — walking a list against its first N labels.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L798)

### `array_enumerate`
{: #array-enumerate}

```burxt
function array_enumerate<T>(xs: [T]) -> [(Int, T)]
```

Every element with its index. `enumerate(["a","b"])` is `[(0,"a"), (1,"b")]`.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L810)

### `array_split_at`
{: #array-split-at}

```burxt
function array_split_at<T>(xs: [T], at: Int) -> ([T], [T])
```

Two arrays: everything before `at`, and everything from `at`. A negative `at` is 0 and one past the end is the whole array, so the answer is always two valid arrays.

Clamped rather than refused, unlike indexing. `xs[i]` out of range is a mistake about ONE element with no sensible answer; `split_at` past the end has an obvious one — everything, then nothing — and refusing it would make every caller write the clamp this line already contains.

**§B54 lived in this function**, and it was nothing to do with the function: `math_clamp` is generic and calls its type parameter `T`, this one is generic and calls its type parameter `T`, and stage-1's emitter let the caller's binding shadow the callee's — so a call whose arguments are two Ints was emitted as the String copy and the program segfaulted. Fixed in `emit.bx`, not here. The function was taken out for one version rather than shipped with a warning.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/array.bx#L832)


{% endraw %}
