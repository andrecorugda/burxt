---
layout: doc
title: lib/net.bx
section: reference
description: "TCP, over the pointer wall."
---


# `lib/net.bx`

TCP, over the pointer wall.

```burxt
use "lib/net.bx";
```

A server that binds a chosen port, accepts a connection, reads a request and answers it; and a client that connects to an IPv4 address. Every function here says `touches network` in its signature, so a caller that never wrote those two words cannot reach any of it — which is the point of an effect, and the reason this module needed no new one.

**This module is younger than the wall it crosses, and the wall was smaller than it read.** `spec/1.0/ROADMAP-1.0.md` §G2 put sockets behind "the pointer wall's remaining doors" — four of them, callbacks and struct-returns included — and read as a milestone. Measured instead of reasoned about, a Burxt program accepted a connection and answered an HTTP request with **no compiler change at all**: `socket`, `listen`, `accept`, `read`, `write` and `close` are plain `external function` declarations, a Burxt String reaches C as a `char *` so sending needs nothing, and `listen()` auto-binds to an ephemeral port so a server ran before `bind` did.

One thing was genuinely missing, and it was one builtin: **`bind()` wants sixteen bytes of `struct sockaddr_in` handed over by pointer**, and Burxt could read C's memory and never write it. `c_bytes_to` is that, and it is the exact mirror of `c_bytes_at`. Seven doors were imagined; one was locked. See [[the wall pattern]] — the wall had been written down once, in prose, as the reason for a workaround, and then re-read as a fact by everyone who came after.

**What is here is TCP and nothing above it.**

- **No DNS.** `getaddrinfo` answers a *chain of structs* and hands back a pointer INSIDE one of them, and reading a pointer out of C's memory is a different door from writing bytes into it. So an address is four octets, and `net_connect_ipv4` says so in its name rather than promising a hostname it cannot resolve — the same bargain `string_to_upper_ascii` makes. - **No TLS.** Binding one is the recorded decision (§E5): there is no control over instruction timing here, and a hand-rolled handshake that *looks* fine is the exact failure this language exists to refuse. `https` is not one function away and this module does not pretend it is. - **No HTTP.** A request is bytes and a response is bytes. Parsing one is a library above this. - **Blocking, and no timeouts.** `net_accept` waits until somebody connects. A server that must not block forever needs `poll`, which wants a struct — reachable now, and not yet written.

**Errors answer `Option`, not a bare -1.** A socket call returns -1 and leaves the reason in `errno`, and -1 is a perfectly good file descriptor number as far as the type is concerned. The same argument `file_read_maybe` settled: absence goes in the type, or it gets discovered later wearing a disguise.

## What is in it
{: #what-is-in-it}

| Name | Kind | What it answers |
|---|---|---|
| [`net_uses_bsd_sockaddr`](#net-uses-bsd-sockaddr) | function | The null pointer these calls want comes from `lib/os.bx`, which needed the same one for `fork` and already had the trick |
| [`write_sockaddr_in`](#write-sockaddr-in) | function | `struct sockaddr_in`, the sixteen bytes every one of these calls is really about. |
| [`net_listen`](#net-listen) | function | A socket bound to `port` on every interface, listening, ready for `net_accept`. |
| [`net_accept`](#net-accept) | function | Waits for a connection and answers the socket that talks to it. **Blocks until one arrives.** |
| [`net_connect_ipv4`](#net-connect-ipv4) | function | Connects to an IPv4 address given as four octets. **No hostname**, and the name says so. |
| [`net_read`](#net-read) | function | Reads up to `limit` bytes. Answers `None` on error and `Some("")` when the peer hung up. |
| [`net_write`](#net-write) | function | Writes every byte of `text`, or answers `None`. |
| [`net_close`](#net-close) | function | Closes a socket. Answers whether the close itself succeeded — worth checking on a socket, where a failing close can mean |
| [`net_listen_any_port`](#net-listen-any-port) | function | A socket on a port the KERNEL picks, and the port it picked. |
| [`net_port_of`](#net-port-of) | function | The port a socket is actually bound to. `None` if it is not bound, or is not an IPv4 socket. |

## Functions
{: #functions}

### `net_uses_bsd_sockaddr`
{: #net-uses-bsd-sockaddr}

```burxt
function net_uses_bsd_sockaddr() -> Bool touches network
```

The null pointer these calls want comes from `lib/os.bx`, which needed the same one for `fork` and already had the trick: `getenv` of a name nothing sets answers NULL, guaranteed by POSIX. Spelling it twice would be a fact stored twice, and a fact stored twice disagrees with itself eventually. **Linux and the BSDs do not lay out `sockaddr_in` the same way, and this is measured rather than assumed.**

```burxt
 Linux:      sa_family_t sin_family;   // TWO bytes, little-endian  -> [2, 0, ...]
 macOS/BSD:  uint8_t     sin_len;      // ONE byte, the struct size -> [16, 2, ...]
             sa_family_t sin_family;   // ONE byte
```

There is no byte pair that satisfies both: Linux needs `[2, 0]` and macOS needs the `2` in the second slot, and `[2, 2]` reads on Linux as family 514. So the layout has to be decided at runtime, and Burxt has no conditional compilation — a recorded decision, and the right one.

**This cost a CI runner an hour.** `lib/net.bx` shipped writing the Linux layout unconditionally. On macOS `bind` then saw family 0, `net_port_of` answered `None`, the fixture connected to port 0, and the parent waited in `accept()` until GitHub cancelled the job at sixty minutes. The library was announced as portable and was tested on one kernel.

The probe is a throwaway UDP socket bound to `127.0.0.1:0` — the kernel picks the port, nothing listens, nothing conflicts, and it is three syscalls rather than the subprocess `os_platform` would cost. `SOCK_DGRAM` is 2 and `AF_INET` is 2 on both families, which is why this can ask the question in the first place.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/net.bx#L88)

### `write_sockaddr_in`
{: #write-sockaddr-in}

```burxt
function write_sockaddr_in(where: CPointer, port: Int, a: Int, b: Int, c: Int, d: Int) -> Int touches network
```

`struct sockaddr_in`, the sixteen bytes every one of these calls is really about.

```burxt
 struct sockaddr_in {
     sa_family_t    sin_family;   // 2 bytes, HOST order — AF_INET is 2
     in_port_t      sin_port;     // 2 bytes, NETWORK order — big-endian, always
     struct in_addr sin_addr;     // 4 bytes, network order
     char           sin_zero[8];  // padding, and it must be zero
 };
```

**The port is big-endian and the family is not**, which is the single most-fumbled detail in this struct and the reason it is written out here once rather than at three call sites. A port written little-endian binds successfully to a completely different number: 8080 becomes 36895, nothing fails, and the mistake is found by a client that cannot connect. That is a silent wrong answer, so it gets a function and a comment instead of four inline subtractions.

`requires` rather than a `Result`: a port outside 1..=65535 is not a condition a caller handles, it is a caller that has made a mistake — and this is the load-bearing/partition line the guide draws. Port 0 is excluded deliberately even though the kernel accepts it as "pick one for me", because a caller who wanted that wanted `net_listen_any_port`, which does not exist yet and would say so in its name.

**It writes the struct rather than answering it**, and that shape was chosen by the region model rather than by taste — the same way `sha256_k` fills an array instead of returning one. A function whose parameters are all `Int` carries no caller region, so it may not return `[Int]`: "its storage lives in a region and would not outlive it". Taking the destination pointer is the better API anyway. There is exactly one place that knows this layout, which is the point.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/net.bx#L129)

### `net_listen`
{: #net-listen}

```burxt
function net_listen(port: Int, backlog: Int) -> Option<Int> touches network, input
```

A socket bound to `port` on every interface, listening, ready for `net_accept`.

`SO_REUSEADDR` is set, and that is a decision rather than a copied incantation. Without it a server that has just exited leaves the port in TIME_WAIT for up to two minutes and the next start fails with EADDRINUSE — which reads, to whoever is restarting it, as "the port is taken" by something that is not there. The cost is the documented one: two servers may bind the same port on different addresses. For a server on `0.0.0.0` that is not reachable.

Answers `None` when the socket cannot be made, bound or listened on. The commonest reason by a long way is that something else already has the port.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/net.bx#L162)

### `net_accept`
{: #net-accept}

```burxt
function net_accept(server: Int) -> Option<Int> touches network, input
```

Waits for a connection and answers the socket that talks to it. **Blocks until one arrives.**

The peer's address is discarded — `accept` will fill a struct with it if given one, and there is nothing in this module that reads a `sockaddr` back into an address yet. `None` is a real failure (the listening socket was closed, or a signal interrupted the wait), not "nobody came".

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/net.bx#L199)

### `net_connect_ipv4`
{: #net-connect-ipv4}

```burxt
function net_connect_ipv4(a: Int, b: Int, c: Int, d: Int, port: Int) -> Option<Int> touches network, input
```

Connects to an IPv4 address given as four octets. **No hostname**, and the name says so.

`net_connect_ipv4(93, 184, 216, 34, 80)` is `93.184.216.34:80`. Resolving a name needs `getaddrinfo`, which answers a chain of structs and hands back a pointer buried in one of them; reading a pointer out of C's memory is a door that is still shut, and a function called `net_connect` that only worked for addresses would be a promise this cannot keep.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/net.bx#L213)

### `net_read`
{: #net-read}

```burxt
function net_read(fd: Int, limit: Int) -> Option<String> touches network
```

Reads up to `limit` bytes. Answers `None` on error and `Some("")` when the peer hung up.

**Those two are different facts and the type keeps them apart**, which is the whole reason this does not answer a bare String. `recv` says 0 for a clean close and -1 for a broken one, and a library that folded both into `""` would be `file_read`'s missing-file bug again, in a module written after that bug was fixed.

**One `recv` is not a message.** TCP is a stream: a 4 KB request can arrive as three reads, and a caller that treats one `net_read` as the whole request works perfectly on localhost and fails against a real client. Reading until a terminator is the caller's job, and it is a real job.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/net.bx#L250)

### `net_write`
{: #net-write}

```burxt
function net_write(fd: Int, text: String) -> Option<Int> touches network
```

Writes every byte of `text`, or answers `None`.

**Loops, because `send` is allowed to write less than it was given** and routinely does on a slow peer. A single `send` whose result nobody checks is the oldest bug in network code and it only shows up under load, which is the worst time to find it.

`MSG_NOSIGNAL` (0x4000) is passed for a reason worth naming: writing to a socket the peer has closed raises SIGPIPE, whose default action **kills the process**. A server that dies because a browser closed a tab is not a server, and Burxt has no signal handlers to install instead. With this flag the call answers EPIPE like any other error and the program stays alive.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/net.bx#L278)

### `net_close`
{: #net-close}

```burxt
function net_close(fd: Int) -> Bool touches network
```

Closes a socket. Answers whether the close itself succeeded — worth checking on a socket, where a failing close can mean data the kernel never managed to send.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/net.bx#L297)

### `net_listen_any_port`
{: #net-listen-any-port}

```burxt
function net_listen_any_port(backlog: Int) -> Option<Int> touches network, input
```

A socket on a port the KERNEL picks, and the port it picked.

`net_listen` takes the port a program has decided on. These two are for the program that has not: a test that must not collide with another test, a client that only needs somewhere to be reached, a service that publishes its address rather than agreeing on it in advance.

**This pair is the reason `c_bytes_to` is more than a `bind` helper.** `getsockname` fills a `sockaddr_in` AND takes its size by pointer — in on the way in, the real size on the way out — so it needs C's memory written *and* read, in one call. Before `c_bytes_to` the port a program had been given was simply unreachable from inside it.

The fixture that made this necessary is the honest story: `tests/pass/net_loopback.bx` bound a fixed 18099, passed alone, and failed the moment two of the suite's tests ran it at once. The comment in that fixture predicted it and the fixed port was kept anyway, because the alternative looked bigger than the test. It was about twenty lines.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/net.bx#L316)

### `net_port_of`
{: #net-port-of}

```burxt
function net_port_of(fd: Int) -> Option<Int> touches network
```

The port a socket is actually bound to. `None` if it is not bound, or is not an IPv4 socket.

The size argument is in-out: it must say how much room the struct has before the call, and the kernel writes back how much it used. Sixteen goes in as four little-endian bytes; a smaller number coming back would mean this is not the address family assumed here.

[Source](https://github.com/andrecorugda/burxt/blob/main/lib/net.bx#L350)

