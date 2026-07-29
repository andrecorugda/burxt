# A point-of-sale, four times

The same small app — a fixed catalogue, one sale, two tax rules, one receipt — written four times,
split the same four ways every time:

```
examples/pos/          items.bx    tax.bx    receipt.bx    till.bx      burxt run till.bx
examples/pos-python/   items.py    tax.py    receipt.py    till.py      python3 till.py
examples/pos-php/      items.php   tax.php   receipt.php   till.php     php till.php
examples/pos-rust/     items.rs    tax.rs    receipt.rs    till.rs      rustc -O till.rs -o /tmp/till && /tmp/till
```

Same module boundaries, same function names, same order inside each file. It exists so the language
can be judged the way a developer judges one: by reading a real program in it beside the same program
in something they already know.

| | items | tax | receipt | till | **code** |
|---|---|---|---|---|---|
| **Burxt** | 13 | 28 | 24 | 52 | **117** |
| Python | 17 | 31 | 13 | 39 | **100** |
| PHP | 32 | 46 | 18 | 45 | **141** |
| Rust | 40 | 44 | 21 | 61 | **166** |

Lines of code — blanks and comments excluded, because the three ports carry a lot of commentary
explaining what they had to do that Burxt does not.

The three reference implementations agree **byte for byte**:

```
--- flat tax ---
Rice 5kg  x3    158.25
Milk 1L  x2     36.80
Newspaper  x1     12.00
subtotal    207.05
tax          23.41
due         230.46

--- split tax ---
Rice 5kg  x3    158.25
Milk 1L  x2     36.80
Newspaper  x1     12.00
subtotal    207.05
tax           3.91
due         210.96
```

## How each one makes money exact

This is the whole reason the app is a till and not a to-do list.

| | mechanism | what it costs |
|---|---|---|
| **Burxt** | `Decimal<2, RoundHalfUp>` — a **type**. `$52.75` is a literal | The rounding rule is viral: it appears in four signatures across three files |
| **Python** | `decimal.Decimal` + `.quantize(CENTS, ROUND_HALF_UP)` | Written at every rounding site. Drop one and the total is silently six-decimal |
| **PHP** | bcmath **strings**; `bcmul`/`bcadd` per operation | `+=` is a float bug. Half-up is hand-rolled, because bcmath truncates |
| **Rust** | `i64` of cents in a newtype | No decimal in std — the real answer is a crate. `(x + 5_000) / 10_000` by hand |

Two of those three get the rounding wrong the *first* time someone adds a feature, and nothing
catches it. That is the argument for the type, and it survives being written out.

But the cost is real too, and it is visible: `Decimal<2, RoundHalfUp>` had to be spelled into
`line_tax`, `money_column`, `totals_text` and two locals in `ring_up`, because a function returning a
contracted decimal forces every caller to name the contract. There is no `type` alias to shorten it
yet. The three ports pay nothing there — and record nothing either.

## This directory found a wrong answer in money, and fixed it

**All four now agree.** They did not when this comparison was written: Burxt printed `tax 0.00` on
both receipts while Python, PHP and Rust all printed `23.41` and `3.91`. The subtotals were right and
the tax accumulated to nothing — no crash, no warning. Exactly the failure this language exists to
refuse.

Three independent implementations agreeing with each other and not with Burxt is what made it
undeniable, and that is the argument for writing the same program four times.

**The cause was an ABI mismatch on vtable calls.** A trait method taking a record declares its
parameter `byval(T)` — on x86-64 the record travels in the stack argument area. The indirect call
passed a bare pointer, which travels in a register. So the method read its `Item` from whatever
happened to be on the stack, and `if !item.taxable` answered from garbage: a taxable item taxed at
zero.

Two things about it are worth keeping:

- **It was layout-dependent.** Adding a `print` to the failing program moved the frame and it started
  answering correctly, which is why six earlier reductions all "passed". A wrong answer that
  disappears when you look at it is the worst kind.
- **The coverage gap was shaped like the intersection of two well-tested things.** `tests/pass/abi_*`
  covers records crossing call boundaries. `tests/pass/trait_dyn_*` covers dispatch through a trait
  object. Seven fixtures pass a record to a method, and **not one** did it dynamically.

Fixed in v0.0.141. The regression fixture is `tests/pass/abi_dyn_record_params.bx`, and because a
layout-dependent bug can only be caught by luck, there is also a structural invariant —
`every_call_site_mirrors_the_declared_abi` — asserting that every call site passing a record attaches
the attributes its callee declares. Three sites do; two of them always did.

## What reading four versions side by side actually showed

Not speed. Three things about how the code is shaped:

1. **The other three put data and behaviour in one block.** `class Item` / `impl Line` — the fields
   and the thing you do with them, in one place, findable by one jump. In `items.bx`, `record Item`
   and `line_subtotal(line)` are adjacent by *convention*; nothing connects them, and nothing stops
   the next function about Items landing in another file. This is the cost of having records and no
   grouping construct, and it is the largest readability gap of the four.
2. **`region sale { ... }` is bookkeeping no other version has.** `till.bx` wraps the entire program
   in it, and the name `sale` is never mentioned again. Python needs `if __name__ == "__main__"` and
   Rust needs `fn main`, so Burxt is not alone in having ceremony — but those two names *do* something,
   and `sale` does not.
3. **Modules: Burxt's `use` is one idea where Rust has two.** Every `.bx` file that needs `items.bx`
   says so. Rust declares `mod items;` once in the crate root and then every file writes
   `use crate::items::…` — declaring a module and importing from it are separate. Burxt's is simpler
   and, for a four-file app, better. Python's is the same shape as Burxt's; PHP's `require_once`
   is the same shape and also the one that fails at run time rather than compile time.

One smaller thing, found by writing them: `label()` on the tax rule was dead in all four versions
until the header line started using it. A trait with a method nobody calls is a bad example of a
trait, and only the Rust compiler said so.
