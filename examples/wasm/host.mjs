// examples/wasm/host.mjs — the whole of the "wasm host glue".
//
// `spec/ROADMAP-2.0.md` files this next to the Android NDK and iOS signing, post-1.0 and
// unbuilt. It is this file. Measured 2026-08-16 by writing it: eleven libc symbols across
// every Burxt program tried, and of those only six need real behaviour, one needs to tell a
// single truth, and three exist to end the program.
//
// Run it with node (>= 18, for `WebAssembly.instantiate` on a Buffer and top-level await):
//
//     node host.mjs island.wasm 'Ada Lovelace' 'One <script> compiler' '$1,299.00'
//
// The same file is what a browser needs, minus the `readFileSync` — pass the bytes from
// `fetch` instead. Nothing here is node-specific except reading the file.

import { readFileSync } from 'node:fs';

const bytes = readFileSync(process.argv[2]);

// **Must match the `-z stack-size=` the module was linked with** — see `getrlimit` below,
// where this number is the one truth the host owes the runtime.
const STACK_SIZE = 1 << 20;

let mem = null;
// Re-read `mem.buffer` on every access: `memory.grow` DETACHES the old ArrayBuffer, so a
// view cached across an allocation is a use-after-free with a friendly name.
const u8 = () => new Uint8Array(mem.buffer);
const dv = () => new DataView(mem.buffer);

const cstr = (p) => {
  const b = u8();
  let e = p;
  while (b[e] !== 0) e++;
  return new TextDecoder().decode(b.subarray(p, e));
};

let pending = '';
const write = (s) => {
  pending += s;
  let i;
  while ((i = pending.indexOf('\n')) >= 0) {
    console.log(pending.slice(0, i));
    pending = pending.slice(i + 1);
  }
};

// ── malloc ───────────────────────────────────────────────────────────────────────────────
// Burxt asks for its region ONCE and then serves itself from `burxt.alloc`, so a bump
// allocator over the end of linear memory is all the host owes it. `free` is never called
// and never imported — the region allocator is the whole memory story.
//
// **Returning 0 on failure is load-bearing.** The compiler asks for 4 GiB, then 256 MB, then
// 16 MB, and keeps whichever it gets; a host that throws instead of returning NULL gives the
// ladder nothing to fall down, and the program dies on a 4 GiB request that no wasm module
// can ever satisfy — a wasm32 address space is 4 GiB in total.
let brk = 0;
const malloc = (n) => {
  n = Number(n); // Burxt's Int is i64, so sizes arrive as BigInt
  if (brk === 0) brk = mem.buffer.byteLength;
  const need = brk + n;
  const have = mem.buffer.byteLength;
  if (need > have) {
    try {
      mem.grow(Math.ceil((need - have) / 65536));
    } catch {
      return 0;
    }
  }
  const p = brk;
  brk = (brk + n + 15) & ~15;
  return p;
};

// ── printf ───────────────────────────────────────────────────────────────────────────────
// On wasm32 a variadic call arrives as `(fmt, va)`, where `va` points at a buffer of promoted
// arguments rather than being spread across registers. Burxt has no float, so this only ever
// sees integers and strings. Eight-byte arguments are aligned to eight.
//
// **ZERO-PADDING IS NOT OPTIONAL, and getting this wrong is worse than not implementing it.**
// A `Decimal<2>` renders through `"%s%llu.%02llu"` — sign, whole part, point, fraction — so a
// host that ignores the `02` turns $1299.05 into `1299.5`. It does not crash, it does not warn,
// and the money is wrong by a factor of ten. That was this file's first version, caught only by
// comparing a wasm render against the native one for the same value. **Compare the two outputs
// for anything carrying a Decimal; a shim that is nearly right about printf is a shim that
// silently corrupts prices.**
const format = (f, va) => {
  const d = dv();
  let a = va;
  let s = '';
  for (let i = 0; i < f.length; i++) {
    if (f[i] !== '%') { s += f[i]; continue; }
    i++;
    let flags = '';
    while ('-+ #0'.includes(f[i])) flags += f[i++];
    let width = '';
    while (f[i] >= '0' && f[i] <= '9') width += f[i++];
    let length = '';
    while ('hlLzjt'.includes(f[i])) length += f[i++];
    const c = f[i];

    if (c === '%') { s += '%'; continue; }
    let text;
    if (c === 's') { text = cstr(d.getUint32(a, true)); a += 4; }
    else if (c === 'c') { text = String.fromCharCode(d.getInt32(a, true)); a += 4; }
    else {
      let v;
      if (length.includes('l') || length.includes('z') || length.includes('j')) {
        a = (a + 7) & ~7;
        v = c === 'u' ? d.getBigUint64(a, true) : d.getBigInt64(a, true);
        a += 8;
      } else {
        v = c === 'u' ? BigInt(d.getUint32(a, true)) : BigInt(d.getInt32(a, true));
        a += 4;
      }
      text = c === 'x' ? v.toString(16) : c === 'X' ? v.toString(16).toUpperCase() : v.toString();
    }

    const n = width ? parseInt(width, 10) : 0;
    if (text.length < n) {
      if (flags.includes('-')) text = text.padEnd(n, ' ');
      else if (flags.includes('0') && c !== 's' && c !== 'c') {
        // A zero-padded negative keeps its sign in front of the zeros.
        const neg = text.startsWith('-');
        const body = neg ? text.slice(1) : text;
        text = (neg ? '-' : '') + body.padStart(n - (neg ? 1 : 0), '0');
      } else text = text.padStart(n, ' ');
    }
    s += text;
  }
  return s;
};

const imports = {
  env: {
    malloc,
    memcpy: (d, s, n) => { u8().copyWithin(d, s, s + Number(n)); return d; },
    printf: (fmt, va) => { const s = format(cstr(fmt), va); write(s); return s.length; },
    putchar: (c) => (write(String.fromCharCode(c)), c),
    fwrite: (p, sz, n, _stream) => {
      write(new TextDecoder().decode(u8().subarray(p, p + Number(sz) * Number(n))));
      return n;
    },
    snprintf: (buf, n, fmt, va) => {
      n = Number(n);
      const b = new TextEncoder().encode(format(cstr(fmt), va));
      const k = Math.min(b.length, n - 1);
      u8().set(b.subarray(0, k), buf);
      u8()[buf + k] = 0;
      return b.length; // what it WOULD have taken — Burxt calls it twice, to measure then render
    },

    // The three that only end the program. `fputs` and `fprintf` are reached exclusively from
    // the panic path, so neither needs to be right about formatting — by the time either runs,
    // the program is already over.
    fputs: (p, _stream) => (write(cstr(p)), 1),
    fprintf: (_stream, fmt, va) => { const s = format(cstr(fmt), va); write(s); return s.length; },
    exit: (code) => { throw { burxtExit: code }; },

    // `stderr` is a DATA symbol, not a function. It is also why the link line below needs
    // `--allow-undefined` and not `--import-undefined`: the latter only turns undefined
    // FUNCTIONS into imports, and leaves this one an unresolved symbol.
    stderr: 0,

    // ── the one truth the host owes the runtime ──────────────────────────────────────────
    // A browser has no rlimits, but Burxt reads RLIMIT_STACK to place its stack-overflow
    // floor, and on wasm the answer is known exactly: it is the `-z stack-size` the module was
    // linked with. Reporting failure is NOT a safe fallback — the runtime's fallback is 8 MB,
    // which is larger than the whole linear-memory stack, and before the saturating fix in
    // `codegen.rs` that made every call look like an overflow.
    getrlimit: (_resource, p) => {
      const d = dv();
      d.setBigUint64(p, BigInt(STACK_SIZE), true);     // rlim_cur
      d.setBigUint64(p + 8, BigInt(STACK_SIZE), true); // rlim_max
      return 0;
    },
  },
};

const { instance } = await WebAssembly.instantiate(bytes, imports);
mem = instance.exports.memory;

// ── the String ABI ───────────────────────────────────────────────────────────────────────
// A Burxt String is a pointer to its BYTES, with the i64 length in the eight bytes BEFORE
// them and a NUL after — see spec/1.0/M12-STRINGS.md §3. So it is a length-prefixed string
// and a valid `char*` at the same time, and a host reads one with a single load.
const fromBx = (p) => {
  const len = Number(dv().getBigUint64(p - 8, true));
  return new TextDecoder().decode(u8().subarray(p, p + len));
};

const toBx = (s) => {
  const b = new TextEncoder().encode(s);
  const base = malloc(b.length + 9);
  dv().setBigUint64(base, BigInt(b.length), true);
  u8().set(b, base + 8);
  u8()[base + 8 + b.length] = 0;
  return base + 8;
};

// `main` first, so the module prints exactly what the native build prints.
try {
  instance.exports.main(0, 0);
} catch (e) {
  if (e && e.burxtExit !== undefined) process.exitCode = e.burxtExit;
  else throw e;
}
if (pending) console.log(pending);

// Then the island itself: JavaScript calling a compiled Burxt `pure function` with JavaScript
// strings, and getting HTML back. Note what the host does NOT have to do — the `<` in an
// argument comes back as `&lt;`, because `html_text` escaped it before the bytes left linear
// memory, and there is no raw path for the host to reach for.
// An argument spelled `123n` is passed as a Burxt Int — which is how a `Decimal<2>` crosses,
// as its exact scaled integer. Everything else is passed as a String.
if (instance.exports['bx.island']) {
  const args = process.argv.slice(3);
  const marshalled = args.map((a) => (/^-?\d+n$/.test(a) ? BigInt(a.slice(0, -1)) : toBx(a)));
  console.log(fromBx(instance.exports['bx.island'](...marshalled)));
}
