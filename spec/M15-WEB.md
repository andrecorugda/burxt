# Burxt — the web stack (M15)

> Status: **W0 DONE (2026-08-16); the rest SPECIFIED, scheduled for 2.0.** Its socket
> foundation (W2) shipped early, in 1.1.0 — see [`1.1/M16-NETWORK.md`](1.1/M16-NETWORK.md).
> 2026-08-01: 1.0 is the real core and comes first.
>
> **`lib/html.bx` and `lib/cgi.bx` both exist** and meet W0's bar: both compile under
> `the_standard_library_compiles_and_works`, stage-1 compiles them and agrees with stage-0, and
> the escaping test that §W0's own trap-note calls *the test that matters* is
> `tests/pass/html_library.bx`.
>
> **Four refusals came with them that this document did not anticipate**, each argued at the
> function rather than assumed from a rule: a tag or attribute **name** that is not a name, and a
> **void element carrying children** (both holes escaping does not cover); **malformed
> percent-encoding**, refused rather than repaired; and a link target whose **scheme** is not
> allowed. One defect was found the same way — `Content-Length` was `len(body)`, but `print` is
> the only way out of a Burxt program and it appends a newline, so every response was one byte
> short of what it declared.
>
> **And one finding outran this whole document: a contract-first view needs no compiler work.**
> A view is an ordinary `pure function ... -> Html` carrying `requires`; money keeps its scale
> and rounding contract into the tag; re-rounding inside a view is a compile error; and
> `burxt review` already diffs a view's promise. `tests/pass/html_view_contract.bx` pins it.
> That is the seventh time in this repository a wall dissolved under a probe rather than an
> argument.
>
> Written now rather than later because exploring it sent someone back to
> [`FAR-HORIZON-ROADMAP.md`](FAR-HORIZON-ROADMAP.md) §1 and found **one row genuinely stale** — C
> structs, whose blocker A7 removed in v0.0.261 without anyone returning to the row — and one
> merely vague. A stale limitation is worse than a stale DONE, because nobody re-tests what a
> document says does not work. Both are fixed there; the socket row keeps its **Blocking** verdict,
> because it was right.
>
> | Claim | State |
> |---|---|
> | The `Html` tree below compiles and runs on **today's** compiler | **MEASURED**, 2026-08-01 — see §W0 |
> | `socket(2, 1, 0)` answers a file descriptor and `close` works | **MEASURED** — [`NOVELTY.md`](NOVELTY.md) §8 |
> | Integer widths, the prerequisite for describing `sockaddr_in` | **DONE v0.0.261**, both stages |
> | Everything else here | **specified, unbuilt** |

---

## 0. The rule that governs this file

**Burxt ships primitives, not a framework.**

`html.bx` renders, `cgi.bx` reads a request, `net.bx` moves bytes, `http.bx` speaks the protocol.
Routing, templating conventions, ORMs, middleware, sessions and asset pipelines belong in other
people's repositories.

That is not modesty, it is the mechanism. PHP put the web in the language and spent twenty years
unable to remove any of it; Rust put nothing in the language and got Axum, Actix, Askama and Maud
from people who were not on the compiler team. **The second one is how a language nobody has heard
of acquires an ecosystem** — and an ecosystem is the stated goal here, not a first-party framework.

So every decision below is judged by one question: *does this let someone else build something we
did not think of?* A surface that answers no is the wrong surface, however convenient.

**Must NOT do:** ship a router, a template file format, or anything named `burxt new --web`. The
day one of those exists, every framework author is competing with the compiler team instead of
building on it.

---

## 1. The split nobody expects

"Front-end" is two unrelated engineering problems wearing one word, and only one of them is hard.

| | What it needs | State |
|---|---|---|
| **Producing HTML** | string handling, an escape table, a recursive render | **Needs nothing. It runs today** |
| **Serving it over a socket** | C struct layouts, sockets, a concurrency model | The rest of this file |

**Anyone reading "the web stack waits on threads" has it wrong for half the stack.** `lib/html.bx`
is `lib/json.bx`'s shape applied to a different grammar, and `lib/cgi.bx` needs only what
`lib/os.bx` already exports. Both are writable the day 1.0 ships, with no language change at all.

That split is why W0 is numbered zero: it does not sit in the dependency chain.

---

## 2. The slices, in dependency order

| Slice | What | Depends on | Language change |
|---|---|---|---|
| **W0** | `lib/html.bx` + `lib/cgi.bx` | **nothing** | none |
| **W1** | C struct layouts at the FFI boundary — enough to describe `sockaddr_in` | A7 widths (**DONE** v0.0.261) | compiler |
| **W2** | `lib/net.bx` — `socket`/`bind`/`listen`/`accept`/`send`/`recv`/`close` | W1 | none |
| **W3** | Concurrency — this is **[`ROADMAP-1.0.md`](ROADMAP-1.0.md) §G1**, not a new item | M1's re-decision | compiler, large |
| **W4** | `lib/http.bx` — request parse, response build, the listener | W2, W3 | none |
| **W5** | TLS / HTTPS | W4, §E build-vs-bind | undecided |

Three of the six need no language change. The compiler work is W1 (small, unblocked) and W3
(large, and already on the roadmap for reasons that have nothing to do with the web).

---

## W0 — `lib/html.bx` and `lib/cgi.bx`, which need nothing

### The shape, and it is MEASURED

This ran on the release binary at v0.0.264, unmodified, and printed
`<div><p class="sku">Rice</p></div>`:

```burxt
class Attr { name: String, value: String }

class Element { tag: String, attrs: [Attr], children: [Html] }

enum Html {
    Text(String),   // escaped on render
    Raw(String),    // trusted, and you had to type it
    Node(Element),
}
```

Two things were unknown before it was run rather than reasoned about, and both are the kind of
question this project has learned not to answer from the armchair:

1. **A variant payload may be a class BY VALUE.** `lib/json.bx:49` records that *"a variant may
   not carry an enum, while a class field may"*, but `Json` only ever carries **slices**
   (`[Json]`, `[Field]`) — so whether `Node(Element)` was legal was genuinely open. It is.
2. **`Html -> Element -> [Html]` typechecks**, the same mutual recursion `Json -> [Field] -> Json`
   already proved. The slice is what keeps neither side infinitely wide.

`allocates` does not appear on the renderer — M14's inference (v0.0.142/144) covers it. Also
measured; the same program compiles with the marker deleted.

### Why escaping happens on render, not at construction

Because the alternative cannot be checked. If `html_text` escaped eagerly, `Html.Text` would hold
*already-escaped* text and nothing in the type would say so — then one function that forgets, or
one round-trip through a value someone built by hand, and the page has a hole. Escaping at the
single point where a `String` leaves the tree means **there is exactly one place to be right**.

`Raw` is the escape hatch and it is a variant, not a flag: embedding unescaped bytes is something a
reviewer sees on the line that does it. That is `DESIGN.md`'s premise in miniature: strict enough
that an agent cannot get it wrong by accident, plain enough that a reviewer sees the one line where
the rule was waived. An `Html` value cannot carry an unescaped string by mistake, because `Text` and
`Raw` are different constructors and neither is the default.

### Reuse, not reinvention

- The escape loop is `json_escape` at `lib/json.bx:98` — scan bytes, splice on a hit, one table
  lookup. Change the table to the five HTML entities (`&`, `<`, `>`, `"`, `'`); the loop is the same
  loop and should be copied rather than re-derived.
- The recursive renderer is `json_render` at `lib/json.bx:123`.
- Names carry the module prefix per `lib/README.md:65`: `html_element`, `html_text`, `html_raw`,
  `html_attr`, `html_render`. No namespaces exist, so the prefix is what prevents a collision with
  a user's own `render`.

### `lib/cgi.bx`

CGI is the interface every web server has spoken since 1993: the request arrives in environment
variables and on stdin, the response leaves on stdout. Burxt already has all three.

| Needs | Already exists |
|---|---|
| `REQUEST_METHOD`, `PATH_INFO`, `QUERY_STRING`, `CONTENT_LENGTH` | `os_env` — `lib/os.bx:140` |
| The request body | `os_read_all` — `lib/os.bx:112` |
| Status, headers, body out | `print` |

So a Burxt program behind nginx or Apache serves dynamic pages **with no listener, no sockets and
no concurrency** — the web server owns the crowd. This is exactly how PHP started, and it is worth
saying plainly that it is not a lesser option: it is a deployment model that outlived most of its
successors.

`examples/mcp/server.bx` is the working precedent for a Burxt program speaking a protocol over
stdio, and should be read before this is written.

The parsing that `cgi.bx` owns and `os.bx` does not: percent-decoding, `&`/`=` splitting for query
and form bodies, and refusing rather than guessing on malformed input. Note that `string_split`
takes a single **byte** separator (`M12-STRINGS.md`), which is enough here — `&`, `=` and `;` are
all one byte — but it is the constraint to design against.

### The brace papercut, and why HTML gets off lightly

`NOVELTY.md:349` records that `"{"` must be written `"\{"` because `{` opens an interpolation, and
that for JSON work this is *every brace*. HTML's syntax is `<>`, so the tax is near zero for markup
— but **an inline `<style>` or `<script>` block is all braces**, and anyone embedding CSS will meet
it immediately. Not a blocker; worth knowing before someone reports it as a bug in `html.bx`.

### Must NOT do

- ~~**No template file format.**~~ **SUPERSEDED 2026-08-16, by Andre, and the reasoning changed
  rather than being waived.** The rule as written was aimed at a `.bxhtml` dialect — *"an
  unoriginal idea"*, in his words — and against that it still holds: a second HTML dialect is a
  second language inside the first, buying a syntax and nothing else.

  What supersedes it is **BMX**, which is a different proposition on all three of the original
  objections. It is markdown-based rather than an HTML dialect; it lives in its **own repository
  with its own CI and version**, so it is not the compiler team's forever; and its point is not
  syntax but that a view becomes **a function with a contract** — `requires`, `touches`, `pure`,
  checked against the template body, with the rounding contract surviving into the view. That last
  part is the thing no other stack has, and it cannot be had from the typed tree alone.

  **What survives from the old rule, and must:** the typed tree stays the surface underneath — BMX
  renders *through* `Html`, so escaping still has exactly one place to be right, and W0 is BMX's
  foundation rather than its competitor. And Burxt ships primitives: BMX is not a framework, and
  §0 still governs.

- **No template file format inside the compiler.** The rule above moved, it did not vanish. A
  parser for BMX written in Burxt in `lib/` is fine; a `.bmx` mode wired into the compiler is the
  thing the original objection was actually about.
- **No `html_raw` convenience wrappers** that quietly widen the trusted set. One way to say
  "unescaped", and it is spelled out.

---

## W1 — C struct layouts

**Compiler work, small, and unblocked.**

`bind`, `connect` and `accept` take a `struct sockaddr *`. Burxt can describe an `Int`, a pointer
and now a fixed-width integer, but not a **layout** — so these three calls are the only thing
standing between the language and a socket.

A7 (integer widths, **DONE v0.0.261**, both stages) was the prerequisite, and that row's own
*unblocks* column already names C structs alongside `dirent.d_name`, `clock_gettime` and binary
formats. **This is not web-specific work**; the web is simply its first caller.

### Why the compiler and not a 40-line C shim

`NOVELTY.md:370` costs the shim at ~40 lines of C, for `connect`'s `sockaddr*` only. It would work,
and it was **considered and refused** (Andre, 2026-08-01).

The reason is `lib/README.md:9`: *"Nothing here is privileged: every function could have been
written by the program that uses it."* That sentence is load-bearing — it is the claim that the
standard library is not a place where the rules are different. A `.c` file sitting in `lib/` would
be the first thing in the directory that is untrue of, and it would be untrue of it permanently,
because nothing forces a shim to be revisited once it works.

The cost is honest and stated: **the whole socket half of this file waits on a compiler feature.**
That is the trade, made deliberately, and `lib/README.md` is not edited as a result.

---

## W2 — `lib/net.bx`

**No language change once W1 lands.** Most of it already crosses the boundary today.

`NOVELTY.md` §8 measured this: `socket(2, 1, 0)` answered fd **3**, and `close` worked — because a
file descriptor is an `int`, not a pointer. **A socket clears the pointer wall.** `socket`, `send`,
`recv`, `listen` and `close` all cross with the FFI Burxt already has; only the `sockaddr` three
wait on W1.

The effect vocabulary needs one addition. `lib/os.bx` writes `touches input`, `touches commands,
files` — there is no `network` effect yet, and there should be, because it is what lets
`NOVELTY.md` §2 forbid a network call inside a function that computes money. **Add the effect in
this slice**, not later: an effect retro-fitted after callers exist is a breaking change to every
signature.

---

## W3 — concurrency, which is §G1 and not a new item

**This is the large one, and this file must not invent it.** The position is already on record in
three places, and W3's job is to be their first real consumer:

- **[`NOVELTY.md`](NOVELTY.md) §6 — effect handlers, not `async`.** Effects inferred rather than
  written, no function coloring, no mandatory executor; the caller's **handler** decides how an
  effect is discharged — blocking, pooled, event-loop, or mocked in a test. One body serves all.
  Its honest costs are listed there and are not softened here: stack capture, `wasm32` needing
  Asyncify until stack-switching lands, and effect *inference* being exactly where Koka and OCaml
  earn their reputation for impenetrable errors.
- **[`FAR-HORIZON-ROADMAP.md`](FAR-HORIZON-ROADMAP.md):69** — the pitch is *"two threads cannot
  corrupt a balance,"* not *"you can await many sockets."* Throughput is not the goal; a money
  language's concurrency story is a correctness story.
- **`FAR-HORIZON-ROADMAP.md`:55, the M1 amendment** — memory ownership and concurrency scheduling
  are **different axes**, and M1 must be decided knowing effect handlers are the intended
  mechanism, or it will be decided in a way that fights them.

The groundwork is further along than it looks: effects are already written and checked (`touches`),
and `allocates` inference (v0.0.142–144) is the least-fixpoint walk over the call graph that effect
inference needs. Handlers are the next step, not the first one.

### What was explicitly refused

Two shortcuts were offered and turned down (Andre, 2026-08-01 — *"we need burxt to solve this, do
not worry, we will build slow but concrete"*):

- **A serial accept loop** — answer one request, then the next. Honest, but it is not a server, and
  calling it one in a document is how a stale DONE gets born.
- **`fork()` per connection** — reachable today, since `fork` returns an `int`. Refused because it
  buys concurrency without answering the question the language exists to answer: it makes sharing
  impossible rather than safe, and every child re-runs the region allocator.

**Neither is a fallback to reach for if W3 runs long.** If W3 is not ready, the answer is W0 —
CGI behind a real web server — which is a complete, honest deployment story that needs none of this.

---

## W4 — `lib/http.bx`

**No language change.** Request line and header parsing, a response builder, and the listener that
W2 and W3 make possible. Chunked encoding, keep-alive and the header-injection refusals (a header
value containing CR or LF is refused, never sanitised) belong here.

Deliberately unspecified until W3 settles, because the shape of a listener is decided by the
concurrency model and writing it twice is the waste this file exists to avoid.

---

## W5 — TLS / HTTPS

**Named, undesigned.** It depends on `ROADMAP-1.0.md` §E's build-vs-bind call, which is unmade.
Binding OpenSSL means the pointer wall's remaining doors (§G2); building it is a multi-year
proposition nobody should start by accident. No estimate is offered here because there is no
honest one.

---

## W6 — the browser, which turned out not to be a different document

**This section replaces the row below that said running Burxt in a browser was out of scope.** It
was written when host glue was filed as a post-1.0 subsystem beside the Android NDK. That was an
estimate, and measuring it produced a different number: the host is **seven libc symbols** for a
pure component, of which **two do real work** — `malloc` and `memcpy`. `wasm32-unknown-unknown`, so
no WASI, and `rust-lld -flavor wasm` IS `wasm-ld`, so nothing needs installing. It is a ~150-line
JavaScript driver, not a subsystem.

**star-burxt** is the front end built on it: a `.bmx` document is a component, and it renders in a
browser with the compiler having judged its event handlers. **It is not in this repository** —
`github.com/andrecorugda/star-burxt`, documented at `star.burxt-lang.org`, versioned on its own
cadence and depended on through `burxt.package` like anybody else's package. It lived in `lib/` for
exactly as long as `use` had no way to name the standard library from outside the compiler's tree;
`std/` ended that, and it left in the next commit.

**The property this file should record, because it will otherwise be rediscovered as a design
choice somebody made:**

> **The refusal that looked like a limitation is what produces the diffable component.**

Burxt has no closures. They were declined in `DESIGN.md` for a memory reason — a closure needs an
owner for its captured state, which is a question about regions rather than about ergonomics — and
that decision predates every line of web code here.

With no closures, **state cannot hide inside an event handler.** A handler cannot capture a mutable
cell and change it later; it can only be given the state and produce the next one. So a handler is
an expression, threaded explicitly, and three things follow that nobody designed:

1. **The compiler judges it.** `on:click=total * 1.5` on a `Decimal<2>` is a compile error about
   rounding — the ordinary rule, reaching an event handler. No framework whose handlers are
   closures can see inside one, and not through carelessness: **a closure's captured state is
   invisible to the signature**, which is the same sentence that declined closures in the first
   place.
2. **`burxt review` can diff what a handler promises** between versions, because the promise is a
   signature rather than a capture.
3. **The page is resumable.** A handler reaches the browser as `data-star-h="0"` — a static symbol
   plus serialisable state, never an inline handler — so a server can emit the wiring and the
   client need not run a render to attach it. Measured: a server-rendered page, interactive, with
   the wasm module not even fetched until the first click.

**And the memory architecture was forced in the same way.** A long-lived page re-renders
indefinitely and `burxt.alloc` reclaims only on region close, so the frame must be the region —
1,000,000 frames flat in 16 MB, against 50,000 before exhaustion without it. The compiler refuses to
let a String built in a region outlive the block, which means the host must take the bytes *inside*
the frame. There was exactly one shape available.

**What is still true of the old row:** a DOM binding is not specified anywhere, and the driver is
JavaScript because a browser reaches the DOM no other way. What is not true is that any of it is
far off.

## 3. What this file does NOT cover
- **Databases.** An encoder to guard is §G5; a driver needs W2 first.
- **A separate repository.** Considered and rejected: `use` names a path relative to the importing
  file with no way to name a **dependency** (`M6-MODULES.md` §1), so a second repo would need a git
  submodule today. Revisit when packaging exists — and note `ROADMAP-2.0.md` §L1, where the
  container raised the search-path question from the other side.

---

## 4. Verification — what would have to be true

Per `ROADMAP-2.0.md`'s rule: every slice states what would have to be **executed**, not written.

| Slice | The claim is earned when |
|---|---|
| W0 | `lib/html.bx` and `lib/cgi.bx` compile under `the_standard_library_compiles_and_works` (`tests/runner.rs:2489`), and a **fail fixture** proves an unescaped `<script>` cannot reach output through `Text` |
| W1 | A `.bx` program reads a field out of a real `struct sockaddr_in` it did not construct, and both stages agree |
| W2 | A Burxt program opens a socket, sends bytes to a listener and reads the reply — **executed**, not compiled |
| W3 | Two units of work interleave, and a data race on a shared balance is a **compile error** with a fixture proving it |
| W4 | `curl` gets a correct response from a Burxt binary, and a second concurrent `curl` is not made to wait |
| W5 | — |
| W6a | A handler expression that narrows money is a **compile error**, proved by a fail fixture in THIS suite: `tests/fail/an_event_handler_may_not_round_money_silently.bx`. It is hand-written and framework-free, because the claim is about the language — the generated shape, not a paraphrase of it |
| W6b † | A `.bmx` component **runs in a real browser** — clicked, state changed, page updated. Measured in Chrome rather than reasoned about, and earned by `star-burxt@bc60edcbb6c05ba4aa4d5a69b202145ebe99675d`'s `test.py`: fifteen assertions, accepting case first |

**† is not a lesser mark, it is a different one: this repository cannot re-run it.** Every other row
above is falsified by `cargo test` on some future day; W6b is falsified only by somebody going and
looking. star-burxt can go red, or quietly rewrite `test.py`, and this file would keep saying the
claim holds — so the row carries **a commit rather than a repository**, for the same reason
`burxt.lock` pins commits rather than tags: a tag moves and a sha is a fact. Re-pin it when
star-burxt earns more, and treat the old sha as what was actually checked rather than as history.

**W6 was ONE row and splitting it is the point of the mark.** The two halves have different
falsifiability, and one row carrying both meant the verifiable half was hostage to the one nobody
here can run. The half about the LANGUAGE — that a click handler is judged like any other
expression — never needed star-burxt to exist, and now does not need it to keep existing.

**The trap this table exists to avoid:** W0 will be tempting to mark DONE the moment `html.bx`
compiles. Compiling proves nothing about escaping — and a new type going green on the first try is
a red flag rather than a result, because an unknown type silences every rule that would have
refused it. **The fail fixture is the test that matters**, not the pass one.

**W6 has the same trap in a new costume.** It will be tempting to mark DONE the moment a component
renders, because a rendering component is visible and satisfying. Rendering proves nothing about the
guarantees: the claim is that the compiler *judges the handler*, and the test for that is a program
that must be **refused**. A generator that refused everything would pass every refusal test, which
is why the accepting case is asserted first in the same test.

And one measurement of W6 is not a measurement of anything: `0.038 ms` per frame is a four-node
counter, which is the workload where nothing is hard. A number quoted for this slice must say what
size tree it came from.
