---
title: Burxt compared
---

# Burxt compared

Every row is checked against **Burxt 1.0.0** by running the compiler. Where another language's cell
is arguable, it says so rather than being generous to us.

**Read the last section first if you are deciding whether to try it.** Most of what is below, other
languages also have. Four rows do not exist anywhere else, and they are the reason this language was
built.

---

## Numbers and money

| | Burxt | Rust | Go | Java | Python | C# |
|---|---|---|---|---|---|---|
| exact decimals | **built in, `Decimal<2>`** | crate | crate | `BigDecimal` | `Decimal` | `decimal` |
| floating point | **none, at all** | yes | yes | yes | yes | yes |
| a float can be introduced by accident | **impossible** | yes | yes | yes | yes | yes |
| scale in the type | **yes — `Decimal<2>` ≠ `Decimal<4>`** | no | no | no | no | no |
| rounding must be named | **yes, or it will not compile** | no | no | no | no | no |
| same answer on every machine | **yes, byte-identical** | for integers | for integers | `BigDecimal` | `Decimal` | `decimal` |
| integer overflow | **named error, exit 70** | panics in debug | **wraps silently** | **wraps silently** | unbounded | wraps or throws |

The row that surprises people is the third. Java has `BigDecimal` *and* `double`, and the bug is
always someone reaching for the second. Here there is no second.

## Correctness

| | Burxt | Rust | Go | Java | Python | C# |
|---|---|---|---|---|---|---|
| null | **none** | none | **`nil`** | `null` | `None` everywhere | `null` (opt-out) |
| absence in the type | `Option<T>` | `Option<T>` | no | `Optional` | no | nullable refs |
| truthiness | **none — conditions are `Bool`** | none | none | none | yes | none |
| **preconditions in the signature** | **`requires`, always checked** | no | no | no | no | `Debug.Assert` |
| **postconditions in the signature** | **`ensures`, always checked** | no | no | no | no | no |
| contracts stripped in release | **never, in any build mode** | — | — | — | — | yes |
| exhaustive `match` | **yes, no wildcard arm** | yes | no | since 21 | 3.10, unchecked | yes |
| a new enum variant breaks the code that ignored it | **yes, on purpose** | no (`_` allowed) | no | no | no | no |
| every failure is named | **yes — exit 70, including a full stack** | mostly | mostly | yes | yes | yes |

Eiffel had contracts in 1986 and Ada/SPARK has stronger ones. What is unusual here is not that
contracts exist — it is that they are **machine-readable and never stripped**, which is what the
last section is about.

## Memory and runtime

| | Burxt | Rust | Go | Java | Python | C# |
|---|---|---|---|---|---|---|
| garbage collector | **none** | none | yes | yes | refcount + GC | yes |
| reference counting | **none** | opt-in | no | no | yes | no |
| runtime | **none** | none | yes | JVM | interpreter | CLR |
| pauses | **none** | none | sub-ms | tunable | GIL | tunable |
| how memory is freed | **per block, one pointer assignment** | ownership | GC | GC | refcount | GC |
| learning cost | **low — no lifetimes to write** | high | low | low | none | low |
| the cost, stated | **coarser than a borrow checker: a value it cannot prove safe is not released** | — | — | — | — | — |

The failure direction is memory, never a dangling pointer. That is the trade, and it is written into
[the memory guide](guide/04-memory.html) rather than discovered.

## Building and shipping

| | Burxt | Rust | Go | Java | Python | C# |
|---|---|---|---|---|---|---|
| compiles to a native binary | **yes** | yes | yes | with work | no | AOT |
| self-hosting | **yes, byte-identical fixpoint** | yes | yes | yes | no | yes |
| **two independent compilers that must agree** | **yes** | no | no | several JVMs | several | no |
| debugger | **DWARF, `-O0 -g`** | yes | yes | yes | yes | yes |
| break on a failing precondition and read the arguments | **yes** | n/a | n/a | n/a | n/a | n/a |
| dependency manifest + lockfile | **yes, commits pinned** | yes | yes | yes | yes | yes |
| a build may reach the network | **never — `burxt fetch` is explicit** | yes | yes | yes | yes | yes |
| cross-target IR identical | **yes, all eight** | no | no | — | — | no |
| runs with neither compiler nor toolchain installed | **yes** | yes | yes | needs a JVM | needs Python | needs a runtime |

## What Burxt does not have

Stated as plainly as the rest, because a comparison that only lists strengths is an advertisement.

| | Burxt | the others |
|---|---|---|
| threads | **none** — processes via `os_fork` | all of them |
| TCP sockets | **yes**, server and client | all of them |
| TLS | **no library — bound, and that works today** | all of them |
| HTTP, DNS | **none** | all of them |
| closures | **declined** — `dynamic Trait` instead | all of them |
| inheritance | **declined** — composition only | most |
| generics | yes | yes |
| a formatter | **yes — `burxt fmt`, in both compilers** | all of them |
| regex | **none** | all of them |
| package registry | **none — git URL and tag** | all of them |
| full Unicode case mapping | **ASCII, and the name says so** | most |
| ecosystem | **new** | decades |

The full list, with a reason for every entry, is [what Burxt does not do](limitations.html).

---

## The four rows that exist nowhere else

Everything above this line, some other language also has. These four do not exist, and they are why
the rest was built the way it was.

### 1. The compiler can tell you a change made your program promise LESS

```
$ burxt review before.bx after.bx
WEAKENED  withdraw    lost `requires amount <= balance`

1 weakened promise(s). A weakened contract is the one change that passes every
test — the tests were failing BECAUSE of it.
```

An agent was told to make a failing test pass and deleted the precondition. **Every test now passes.
Coverage is unchanged.** A reviewer skimming the diff sees one removed line.

Nothing else can answer this, because nothing else puts the precondition where a machine can diff it.

### 2. A semver rule a machine applies

```
$ burxt review --semver before.bx after.bx
major   `withdraw` gained `requires amount <= balance`
major   `read_config` now touches files — effects propagate, so every caller
        must declare it too or stop compiling
minor   `extra` is new and public

minimum bump: major
```

A **stricter** precondition is a breaking change — it promises more and breaks callers — and so is a
public function that starts touching the filesystem, because effects propagate through signatures.
`cargo-semver-checks` is the nearest relative and it compares types; it cannot see either of these.

It sets the minimum and says so: it reads the interface, not the behaviour, and can never prove an
upgrade is safe.

### 3. A tool schema derived from the preconditions

`burxt mcp-schema` turns a function's `requires` clauses into an MCP tool definition. The validation
an agent is given and the validation the code enforces are **the same sentence**, so they cannot
drift.

### 4. Exact money that is byte-identical everywhere

Not "we have a decimal type" — **no float exists to reach for**, the scale is in the type, rounding
must be named, and the answer is identical on every machine, target and run. The flagship case
nobody thought was reachable without floats — vector similarity search — turned out reachable *and
better*: exact `Decimal<7>` cosine similarity carries a claim no float-based store can make.

---

*Checked against 1.0.0. If a row here is wrong about another language, that is a bug and worth
reporting — a comparison that flatters us is worth nothing.*
