# Burxt 1.0.0 — the road to a language people can ship on

> Status: **the plan of record.** Written 2026-07-31 at v0.0.205, from a full read of all 26 specs plus
> `DESIGN.md` and three systematic scans of the compiler and standard library against Rust, Python,
> PHP, Java and Go. Every claim here was verified by **running** the compiler, not by reading it.
>
> This supersedes `FAR-HORIZON-ROADMAP.md`'s §4 ranking for near-term work. That document remains the
> **audit** — the row-by-row comparison and the record of which absences are decisions. This one is the
> **order**.

## The goal

> **Burxt 1.0.0 — a language someone outside this repository can ship on.**
>
> They can write a real program, **test** it, **debug** it, **depend** on other people's code, and hand
> it to a shell that understands what it says. Nothing basic is missing compared to Rust, Python, PHP,
> Java or Go — and everything deliberately absent is **named, with its reason**, in one place.

Four things were chosen as must-ship, and they define the bar: **dependency management**,
**DWARF/debugger**, **time + date + randomness**, **integer widths + Unicode**. Everything not on that
list becomes a **documented limitation in the release notes**, never a silent gap.

The standard-library bar was set separately and it is the harder one: **full parity with Rust's `str`
and `Vec`**, not the common 80%. That choice is what promotes closures, tuples and an iterator protocol
from "nice later" into 1.0 — see §A.

## The ordering rule

> *"I would rather do compiler fixes to unblock a lot first, second to bugs that is urgent."*

So: **A** compiler leverage → **B** urgent bugs → **C** the rest of the bar → **D** the library floor →
**E** security → **F** papercuts → **G** post-1.0 → **H** the release gate.

**Why A before D, concretely.** If the ~120 library items are written before A2 (`const`), A3 (generic
`Option`) and A5 (`.chars()`), then `lib/math.bx`, `array_pop` and `string_reverse` get built *around*
those limits and rebuilt afterwards. A-first means each is written once. And two B items already depend
on A items — B5 needs A1, B9 needs A5 — so A-first is the correct dependency order, not merely the
cheaper one.

## Three process rules that govern all of it

Each was learned expensively and each has a version number behind it.

1. **A status line saying DONE is not evidence. The suite is.** M13 was marked DONE for fourteen
   versions with its core decision (`it`) never implemented and a cited fixture that had never existed.
2. **A pass fixture cannot tell "supported" from "not examined."** A new type going green in stage-1 on
   the first try is a red flag — check the fail fixtures and measure the ratchet. This caught `CPointer`
   (v0.0.196) and a shift-distance node kind (v0.0.199).
3. **Where stage-0 refuses something, stage-1's handling is untested by construction.** So relaxing any
   stage-0 rule is itself the reason to go read stage-1's version of it. That is how v0.0.202 found
   stage-1 answering `<` on Strings with `strcmp(a,b) == 0`, and generic comparisons sorting by
   allocation address.

---

## A — Compiler fixes, ranked by leverage ÷ cost

| # | Fix | Size | Unblocks |
|---|---|---|---|
| A1 | **`c_bytes_at(p, n)`** — one builtin, mirroring `c_string_at` | **S** | **Every key, token, session and UUID v4** — `/dev/urandom` is a character device, so `read_file` sizes it with `ftell` and gets 0 · `file_read_bytes` · streaming reads · `mmap` → N9 row 6 · buffer-filling syscalls (`getrandom`, `clock_gettime`) |
| A2 | **`const` / named constants** — none exist at all | **S** | All of `lib/math.bx` (`INT_MAX` cannot be *named* today) · CRC and hash polynomials · every magic number in the new modules |
| A3 | **`Option.None` in a free generic function.** ⚠ **Verify first.** `map.find` already returns `Option<V>` from a METHOD, so the limit is narrower than three library headers claim | **S–M** | `array_pop<T>` · a generic `Set` · `map.take` · `option_ok_or` · retires a limitation cited in `array.bx`, `option.bx` and the audit |
| A4 | **`pure` on a method / `pure` returning an Option** | M | ~15 new stdlib functions get the right signature rather than the wrong one by accident. `pure` is also what a **contract clause** may call |
| A5 | **`.chars()` / codepoint iteration** — A4.4's one remaining gap | M | The whole UTF-8 layer: correct case handling · a `string_reverse` that does not corrupt · char indexing · `\uXXXX` in JSON · `is_valid_utf8` |
| A6 | **`for i in 0..n`** — ranges | M | The most-repeated four-line idiom in the codebase; it appears in every `lib/` file |
| A7 | **Integer widths** `i32`/`u8`/`u32`/`u64` | M | C structs (`dirent.d_name`) · fixed-width records → N9 row 6 · `clock_gettime` → **monotonic and sub-second time**, so benchmarking and timeouts · binary formats · A4.4's deferred **Bytes type** |
| A8 | **Tuples** | M | `zip` · `enumerate` · `char_indices` · `split_at` · `divmod` · `split_once` without inventing a record |
| A9 | **Generic interfaces** — the cheap alternative to closures. `dynamic Trait` is already a function value in all but name; interfaces simply cannot take type parameters. **On no roadmap — needs an explicit yes/no, because YES may replace A10** | M | `sort_by` · predicates · visitors · most of `map`/`filter`, in a form consistent with the no-closures decision |
| A10 | **Closures / function values** — or A9 instead of it | **L** | `map`/`filter`/`fold`/`any`/`all`/`retain`/`partition`/`position` across four libraries **at once** · `signal()`, so a server can shut down cleanly |
| A11 | **An iterator protocol.** A4.4 deferred it with *"trigger: after growable collections make iteration general."* **That trigger has fired.** | L | Lazy chains · `for` over a Map without allocating an array and re-hashing every key |
| **A12** | **M14 slice 3 — per-block release** (+ ~~`allocates nothing`~~ **DONE v0.0.209**, `burxt explain memory`). ⚠ **IN PROGRESS — the forcing function fired at v0.0.207** | L | Bounded memory in a loop (**5,280 → 1,408 KB** per 100k Strings) · the compiler's own ceiling, which **went red in CI at 544 MB against 540** while passing locally at 537 · **prerequisite for the freestanding/IoT target** |

### A1 in detail — why the smallest item is first

It was filed in `FAR-HORIZON-ROADMAP.md` M2 as one of four unopened doors, listed as an `mmap`
prerequisite. It is much more than that: **without it a Burxt process cannot generate a cryptographic
key at all.** `/dev/urandom` is a character device, `read_file` measures with `ftell` and gets zero, and
the only workaround — `os_capture("head -c32 /dev/urandom …")` — puts the key in the process table where
any other user on the machine can read it.

- Signature `c_bytes_at(p: CPointer, n: Int) -> [Int]`, copying `n` bytes into a region-allocated array.
- **The one real decision: what happens when `n` lies.** The length is the caller's claim, not a fact
  the type carries. **Resolved: trust it, name it in the documentation as the pointer wall's one soft
  edge, and check the half that can be checked** — a null pointer and a negative count, refused as a
  literal and trapped at runtime. Consistent with `as scaled` and `external function`: the boundary is
  declared, not inferred.
- Same shape as `c_string_at`, which already does the copy-at-the-boundary work. One extra argument.

> **✅ DONE (v0.0.207) — and one claim above needed correcting.** This section said `c_bytes_at`
> unblocks the CSPRNG. It does not do it alone. The chain is **`malloc` → `getrandom` → `c_bytes_at` →
> `free`**, and it works only because `malloc` returning a `CPointer` was already legal (v0.0.196) and a
> `CPointer` was already an allowed extern *parameter*. `c_bytes_at` is the last missing link, not the
> only one — so A1 completed something three versions in the making rather than opening it single
> handed. `tests/pass/os_random_bytes.bx` is the proof: 32 bytes of real OS entropy, in-process, in
> both compilers.
>
> Also landed with it, because they were the same hazard: **ten builtins that were implemented and
> never reserved** (roadmap B6) — `bit_*`, `shift_*`, `c_is_null`, `c_string_at`, `c_bytes_at` — so a
> program could declare a function with the same name and collide. And **eleven that were never
> documented** (B14), which is the same omission showing up twice, because
> `docs/reference/builtins.md` claims to be generated from that list. Each new entry carries a probe
> the generator **compiles**, so the page is verified rather than remembered.

---

## B — Urgent bugs, silent wrong answers first

| # | Bug | Size |
|---|---|---|
| B1 | **`file_read` of a missing file answers `""`** — indistinguishable from an empty file. The silent wrong answer the thesis exists to refuse, in the standard library. Its own comment says the fix needs `Option`, *"which the language does not have yet"* — **it does** | S |
| B2 | **`os_byte_as_string` is lossy** — every byte ≥ 127 becomes `"?"`. The only int→character path in the library, and it silently destroys data | S |
| B3 | **Hardcoded temp paths** `/tmp/burxt-fs-list`, `/tmp/burxt-os-capture` — two processes clobber each other, and both are a symlink-attack surface on a shared machine | S |
| B4 | **No constant-time compare.** `==` on Strings is `strcmp`, which short-circuits and **leaks the answer through timing** — every token and HMAC comparison | S |
| B5 | **The UTF-8 invariant is declared and unenforced** at all four entry points (`read_file`, `argument`, `os_env`, `c_string_at`). **Decided: validate.** Consequence: binary through `read_file` breaks → needs `file_read_bytes` (A1) | M |
| B6 | **9 builtins are not in `is_reserved_name`** (`bit_*`, `shift_*`, `c_is_null`, `c_string_at`) → a user program can shadow them | S |
| B7 | **Stack overflow is the only failure Burxt does not name** — raw SIGSEGV (exit 139), not exit 70 | S–M |
| B8 | A bare **`it` inside a string literal** in a bracket clause is wrong | S |
| B9 | **`lib/json.bx` rejects valid JSON** (`\b`, `\f`, `\uXXXX`). Refuses rather than corrupts, but Burxt cannot read real-world JSON. Needs A5 | S–M |
| B10 | **Iterative AST walkers** — 512 MB stack, ~30k-node ceiling | M |
| B11 | **M7: stage-1 compiles 101 of 102**; generic records in stage-1 | M |
| B12 | **stage-1 backend gaps** — Decimals, `match`, `musttail`, contracts, FFI. Refused by name, never miscompiled, but it bounds what the differential can cover | L |
| B13 | M11's **1.67× compile-time growth is unattributed**; the ratchet tightening is pending | S |
| B15 | **stage-0 accepts a trailing `;` on an interface method signature and stage-1 refuses it.** Found by writing a fixture in v0.0.209 — no existing fixture used the `;` form, so the differential could not see it. A divergence in what is ACCEPTED, which is the direction that matters | S |
| B14 | **Doc rot** — `lib/README.md` claims `Option`/`Result` do not exist · `map.bx` claims no bit ops · the module table omits 3 modules · `docs/reference/builtins.md` omits 9 builtins while claiming to be generated from that list | XS |

---

## C — The rest of the 1.0 bar

| # | Item | Size |
|---|---|---|
| C1 | **DWARF debug info + an `-O0` flag.** Stage-0 only for 1.0, stated as such. Matters because *an agent that cannot debug inserts `print`, which moves the stack and changes the answer* — the v0.0.141 trap | M |
| C2 | **Dependency management** — manifest (git URL + tag), lockfile, local cache, `pub`/visibility. **No registry for 1.0.** `burxt review` becomes the semver rule: a major bump is *mechanically detectable*, which nothing else can do | L |

---

## D — The standard-library floor: full Rust `str` + `Vec` parity

**D0 comes first, before a single function is written:** decide the **`Builder`** shape. `out = out + b`
is O(n²) and this project has paid for that three times (v0.0.68, v0.0.77, v0.0.82). Every one of the
~100 functions below must be run-based, or the library ships 80 quadratic appends.

### D1 — writable today, no compiler change

| # | Module | Functions |
|---|---|---|
| D1a | **`lib/string.bx`** transform | `to_upper_ascii` · `to_lower_ascii` · `capitalise` · `title_case` · `equals_ignore_case` · `find_ignore_case` · `compare_ignore_case` · **`replace`** · `replace_first` · `reverse` *(needs A5)* · `pad_start` · `pad_end` · `pad_centre` · `trim_start` · `trim_end` · `trim_bytes` · `strip_prefix` · `strip_suffix` · `slice` *(end-exclusive)* · `insert` · `remove` · `shorten` · `squeeze_space` · `indent` · `dedent` |
| D1b | **`lib/string.bx`** search & split | `rfind` · `find_from` · `count` · `find_any` · `rfind_any` · `contains_any` · `find_at -> Option<Int>` · `split_space` · `split_any` · `split_times` · `rsplit` · `split_once` · `split_inclusive` · `split_no_empty` |
| D1c | **`lib/string.bx`** classify & compare | `is_empty` · `is_blank` · `is_digit` · `is_alpha` · `is_alnum` · `is_upper` · `is_lower` · `is_punct` · `is_hex_digit` · `is_ascii` · `all_digits` · `all_alpha` · `compare` *(3-way)* · `compare_natural` · `common_prefix_len` · `edit_distance` |
| D1d | **`lib/string.bx`** parse & format | `parse_int_base` · `parse_hex` · `int_to_base` · `int_to_hex` · `int_to_binary` · `int_padded` · `int_grouped` · **`parse_decimal`** *(per scale)* · `decimal_padded` |
| D1e | **`lib/array.bx`** | **`slice`** · `copy` · `concat` · `insert_at` · `remove_value` · `count_of` · `last_index_of` · `binary_search` · `equals` · `dedup` · `product_int` · `repeat` · `take` · `drop` · `pop` *(precondition form)* · `rotate` · **a faster stable sort** |
| D1f | **`lib/map.bx`** | **`values()`** · **`entries()`** · `clear()` · `merge()` · `take()` · `is_empty()` · `map_increment` · `map_from` |
| D1g | **`lib/set.bx`** *(new)* | `Set<T: Equatable>` over `Map<T, Bool>` — `add` · `has` · `remove` · `count` · `items` · `union` · `intersect` · `difference`. **Every comparison language ships one; Burxt has none** |
| D1h | **`lib/math.bx`** *(new)* | `abs` · `min` · `max` · `sign` · `clamp` · `pow` · `isqrt` · `gcd` · `lcm` · `checked_add/sub/mul` · `saturating_*` · `wrapping_*` · `INT_MAX`/`INT_MIN` *(needs A2)* |
| D1i | **Decimal helpers** | per-scale `abs` · `min` · `max` · `is_zero` · `percent_of` · `round_to` · **`money_split`** — largest-remainder penny allocation, *the* canonical exact-money problem, and absent |
| D1j | **`lib/time.bx`** *(new)* | civil date ↔ unix seconds · ISO-8601 format + parse · `Duration` · day/second arithmetic · `weekday` · `is_leap_year` · `days_in_month`. **UTC only, said so.** Monotonic and sub-second need A7 |
| D1k | **`lib/random.bx`** *(new)* | seeded xorshift/PCG · `next_below` · `next_between` · `shuffle` · `choice`. **Named `random_from(seed)`, never a bare `random()`** — reproducible on purpose, wrong for keys |
| D1l | **`lib/path.bx`** *(new)* | `join` · `basename` · `dirname` · `extension` · `stem` · `is_absolute` · `normalise`. **None exist at all** |
| D1m | **`lib/files.bx`** | `read_maybe -> Option<String>` · `is_directory` · `is_file` · `size` · `copy` · `remove_directory` · `walk` · `read_bytes` *(A1)* · `temp_file` |
| D1n | **`lib/log.bx`** *(new)* | `debug`/`info`/`warn`/`error` · `BURXT_LOG` threshold · stderr · timestamps. Closes the audit's `structured logging: Blocking` |
| D1o | **Errors** | `assert_that(held, why)` · `panic(why)` · `todo()` · `unreachable(why)` · `result_is_error` · `result_context` · `option_ok_or` |
| D1p | **UTF-8 layer** *(A5)* | `next_char` + `CharAt` · `char_count` · `char_at` · `char_index` · `from_codepoint` · `codepoint_at` · `is_valid_utf8` · `is_continuation` · `from_byte` *(retires the lossy `os_byte_as_string`)* · `to_bytes` · `from_bytes` |
| D1q | **Process / env** | `os_capture_status` — stdout, stderr and exit code separately; today they are merged with `2>&1` · `os_set_env` · `os_pid` · `os_cwd` · `os_platform` |
| D1r | **`sleep(ms)`** | A five-line extern. **Blocks every retry and poll loop today** |
| D1s | **`range(n) -> [Int]`** | Stopgap until A6 |
| D1t | **`lib/csv.bx`** *(new)* | read + write. JSON is covered thoroughly; CSV is the other universal interchange format, and the one a money language is handed most |

### D2 — needs A first, listed so nothing is written twice

| # | Item | Needs |
|---|---|---|
| D2a | `array_pop<T> -> Option<T>` · generic `Set` · `map.take` · `option_ok_or` | A3 |
| D2b | Codepoint-correct `string_reverse`, case handling, char indexing, JSON `\u` | A5 |
| D2c | `zip` · `enumerate` · `char_indices` · `split_at` · `divmod` | A8 |
| D2d | `map` · `filter` · `fold` · `any` · `all` · `sort_by` · `retain` · `partition` · `position` | A9 or A10 |
| D2e | Monotonic clock · sub-second time · benchmarking · timeouts | A7 |
| D2f | `chunks` · `windows` · borrowed sub-slices | a slicing decision |

### The two omissions most embarrassing for the pitch

Both are in D1, and both deserve naming here so they are not deprioritised as "just more functions":

1. **You cannot read a `Decimal` out of a file.** There is no `string_parse_decimal`, in a language
   whose headline feature is exact money. Reconstruction via `tick * count` works but must be written
   once per scale, because a scale cannot be a type parameter.
2. **`money_split` does not exist.** Splitting $100.00 three ways so the parts sum *exactly* to the
   total is the canonical exact-money problem, it appears in no library or example, and it is the first
   function a reader of the pitch would look for.

---

## E — Security and cryptography: build vs bind

Verified: **zero** occurrences of sha256, hmac, aes, encrypt, bcrypt, argon, base64, jwt, constant-time
or urandom anywhere in `lib/`, `src/` or `examples/`. The only checksum in the project is CRC-32, and it
lives in a **test fixture**.

**Why this needs its own section.** A language whose stated purpose is *"an agent writes the code, a
senior developer reviews it"* has a specific exposure: **if the primitives are absent, an agent will
hand-roll them** — and a subtly wrong AES produces ciphertext that looks perfectly fine. The absence is
not neutral; it invites the exact outcome this language exists to prevent.

**So the line is drawn by testability, not by difficulty.**

| # | Item | Verdict |
|---|---|---|
| E1 | **SHA-256 / SHA-512 · HMAC · PBKDF2** | **BUILD** — published test vectors, no secret-dependent branching. Verifiable exactly as CRC-32 already is, so "it compiles" and "it is correct" become one statement |
| E2 | **hex · base64 · base64url** encode/decode | **BUILD** |
| E3 | **CSPRNG + `uuid_v4`** | **BUILD, after A1.** Impossible today |
| E4 | **Promote CRC-32** out of `tests/pass/bits.bx` into `lib/hash.bx`; add `fnv1a` for a version-stable hash | **BUILD — already written and verified. Cheapest win on the board** |
| E5 | **AES · ChaCha20 · RSA · Ed25519 · X25519 · TLS · Argon2/scrypt/bcrypt** | **BIND — do not hand-roll.** Two reasons: no control over instruction timing, and RSA and the curves need **arbitrary-precision integers**, which do not exist — `Decimal` is a scaled i64 capped at scale 18 |
| E6 | **Secrets cannot be zeroed** — a String lives until its region closes; there is no `zeroise` | **document as a 1.0 limitation** |

**Two naming decisions, to make with the modules rather than after:** `random_from(seed)` never a bare
`random()`, and `string_equals_constant_time` spelled out in full — for the same reason `divide_floor`
and `shift_right_zeros` are, because the *behaviour* is the point.

---

## F — Papercuts

Stack trace on failure · reach a Decimal's unscaled integer · `to_string` of a record / a display trait ·
default parameter values and named arguments · `burxt fmt` · `==` on records and enums · nested match
patterns *(trigger fired v0.0.118)* · `old(...)` of an aggregate and `ensures` on an aggregate return ·
`pure` methods · `decreases` on methods · mutual recursion and lexicographic measures · `if` as an
expression · `allocates` on methods · `[0; N]` literals · unit literals (`5.km`) · pipelines ·
attributes `#[...]` · regex · editor go-to-definition and a tree-sitter grammar · `List<T>` as a library
type · no warnings only errors · parser errors arrive alone · SOLID lints · stage-0 AST renames · the
`region` naming sweep · profiler · compound `Map` keys.

---

## G — Post-1.0, by gate

The grouping is the useful part: these are not independent items, they are five gates.

| # | Gate |
|---|---|
| G1 | **Concurrency** — threads, shared regions, **derived mutual exclusion from a declared invariant** (*"the genuinely novel step"*), data races as compile errors, `map_seeded`. Regions were chosen partly to make this right; effect handlers are the intended mechanism |
| G2 | **The pointer wall's remaining doors** — callbacks into Burxt (→ `sqlite3_exec`, `signal`), C→Burxt strings, an environment effect → then sockets → TLS → HTTPS → a model client |
| G3 | **M3 packaging** — per-target linking, desktop matrix, Android NDK/JNI, iOS signing, wasm host glue. *Objects already emit for 8 triples with byte-identical IR; what remains is a sysroot per platform* |
| G4 | **Freestanding runtime (IoT)** — configurable region, no-libc mode, `print` routed out. Xtensa (ESP32), AVR, MSP430, ARM/Thumb and RISC-V 32 backends are already registered and `armv7` emits real ELF. **Needs A12.** The pitch is unusually strong here: exact decimals, no float, no GC, no runtime, bounded memory and byte-identical IR is what embedded control code wants, and it is the one domain where "no floating point" reads as a feature |
| G5 | **An encoder to guard** — N1 / NOVELTY §1's serialization and database boundary exactness. `lib/json.bx` fired this trigger |
| G6 | N9 rows 6–9 + the *"money may not reach a model"* rule and its fixtures · borrow and mutability tracking for `dynamic` · M4 phases 4b–6 · static contract proving (SMT) · A4.6's deferred rows |
| G7 | **burxtQL** — a query language whose **contract IS its schema**, the same trick `burxt mcp-schema` already does one layer up. Specced nowhere; after N9 rows 6–9 |

---

## H — Forcing functions and the release gate

| # | Item |
|---|---|
| H1 | **A12's forcing function FIRED at v0.0.207**, and the promise was broken once. The ceiling went red in CI at **544 MB against 540**, while passing locally at **537** — the growth cumulative over v0.0.200–207, which added 143 lines to `emit.bx` alone with nothing re-measuring. Raised to **600 against the CI number**, because the 540 was set against a *local* 497 and CI runs ~7 MB higher, so the real margin was 3 MB rather than 43 — the exact mistake the comment above it warns about. **A ceiling must be set against CI, not the laptop.** The raise was taken because a red tree is the failure this project spent thirteen versions learning to avoid and slice 3 is not a hotfix; the cost is that A12 is now **next** rather than queued |
| H2 | **Doc hygiene** — six spec headers still say `spec, to implement` for shipped work; `DESIGN.md` is stamped v0.0.152 and its *"Open tradeoff — Memory management"* was decided by M1; `spec/README.md` says *"as of v0.0.58"*; four audit rows are stale; **there is no effects spec in `spec/` at all**. **Fix each in whichever version touches it**, never as a separate cleanup — that is how they rotted |
| H3 | CI green **on the commit being tagged** — a tag on a red commit must be withdrawn, which happened with v0.0.171 |
| H4 | `cargo test --release the_release_tarball_works_without_rust_or_llvm -- --ignored` passes |
| H5 | **The 1.0 limitations document** — every `Decision` and every unpicked `Blocking` row, so nothing surprises anyone. This is what makes a high bar honest instead of optimistic |
| H6 | A stated **compatibility promise**, with `burxt review` as its mechanical enforcer |

---

## Not work — decisions on record

Changing these breaks the language. Each has its reason written down, and **all of them belong in H5.**

**Identity:** no floating point — upheld *and strengthened* by N9, where the flagship use nobody thought
was reachable without floats turned out reachable and better without them · no char type, no bare `s[i]`
· no reflection · no inheritance (dropped v0.0.46, *"composition-only is final"*) · no null · no GC, no
refcounting, no runtime · no `unsafe` escape hatch · no truthiness · **no removing Rust** — stage-0 is
the trust anchor and the differential · no file-level privacy.

**Shape:** bitwise as seven named builtins, not operators · String order is BYTE order, never locale
collation · no operator overloading · no C-style ternary · no `%` for modulo — it is the percent literal
· no format-spec mini-language in interpolation · no implicit prelude, glob imports or conditional
compilation · no undefined `Map` iteration order, ever · no wildcard `_` match arm · no stripping
contracts in any build mode · no `unwrap`.

**Stated-and-accepted costs:** contracts always checked · region granularity is coarser than Rust
(*"the honest limit of this model, accepted deliberately"*) · scale ceiling 18 · always-sret ·
declaration-order layout · **no catch/recover** — a failure exits 70 and there is no handler.

---

## Verification — every version, without exception

```sh
RUSTFLAGS="-D warnings" cargo build --release && RUSTFLAGS="-D warnings" cargo test --release
python3 scripts/site-examples.py --check
python3 scripts/site-reference.py     # when the compiler's surface changed
python3 editors/vscode/pack.py        # when a keyword was added
gh run list --limit 2                 # CI was red for 13 versions unwatched
```

The `-D warnings` is not optional: CI passes it and a plain `cargo build` does not, which is the third
distinct way CI has gone red while every local run was green.

- **Both compilers, or it is not done.** Stage-1 parity in the same version. The three process rules say
  why a green stage-1 is not evidence on its own.
- **A fixture per behaviour and a fail fixture per refusal**, with the reason in the fixture's comment.
  Raise the fail ratchet to the **measured** value with no cushion — a cushion *is* the drift.
- **Two streams, an exit status, or a cross-target claim needs a runner invariant**, not a `tests/pass`
  fixture — that harness compares stdout and requires success. `print_error_writes_to_stderr`,
  `a_program_reports_its_status_to_the_shell` and `the_ir_is_the_same_for_every_target` are the patterns.
- **Every claim added to a spec is verified by running the compiler.** M13 is why.
- **A change to the language is not finished until the highlighter, the language server and the packaged
  extension have changed with it** (M10 §2e).
