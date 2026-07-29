# The Burxt standard library

```burxt
use "lib/str.bx";
use "lib/fs.bx";
use "lib/os.bx";
```

Written in Burxt, from the same builtins any program has. Nothing here is privileged: every
function could have been written by the program that uses it, and the point is that it was
written **once, carefully**, instead of forty times.

| Module | What it holds |
|---|---|
| [`str.bx`](str.bx) | `str_find`, `str_contains`, `str_starts_with`, `str_ends_with`, `str_trim`, `str_split`, `str_lines`, `str_join`, `str_repeat`, `str_to_int` |
| [`option.bx`](option.bx) | `Option<T>` — absence as a type. `option_or`, `option_is_some`, `option_is_none`. No `unwrap`, on purpose. |
| [`result.bx`](result.bx) | `Result<T, E>` — failure as a type. `result_or`, `result_is_ok`. Both `match` arms always required. |
| [`fs.bx`](fs.bx) | `fs_read`, `fs_write`, `fs_append`, `fs_exists`, `fs_delete`, `fs_move`, `fs_make_dir`, `fs_list` |
| [`os.bx`](os.bx) | `os_args`, `os_arg`, `os_arg_count`, `os_now`, `os_run`, `os_capture`, `os_read_byte`, `os_read_all` |

## Why it exists

Two reasons, and the second is the real one.

**Convenience**: splitting a string, appending to a file and listing a directory are things
every program needs and none should re-derive.

**Safety at the boundary**: Burxt refuses a C return whose ownership it cannot describe, so
`opendir`, `getenv` and `fopen` are out of reach directly. The way through is the shell or an
`Int` standing in for a pointer — and **every program that writes that itself is a program
making a promise the compiler cannot check.** Here the promise is made once, in the open,
with the reasoning next to it. `fs_list(path)` is safe because `fs.bx` did the unsafe part
carefully, one time.

## What it does not hide

- **Failure has no type yet.** `fs_read` of a missing file answers `""`, which is
  indistinguishable from an empty file. `str_to_int("abc")` answers 0. The honest fix is
  `Option<T>` and `Result<T, E>`, which the language does not have — so the limitation is
  stated in each function's comment rather than papered over.
- **`fs_append` reads the whole file and writes it back**, because the builtin replaces
  rather than appends. Fine for a log line, wrong for a gigabyte, and worth knowing which
  you have.
- **`fs_quote` refuses a path containing a single quote** rather than mishandling it. Shell
  quoting has exactly one hole and that is it.
- **Building a large String means chunks**, never an append per byte or per line — this
  project has paid for that three times (v0.0.68, v0.0.77, v0.0.82). `os_read_all` batches
  in 4 KB pieces for that reason.

## Naming

Every function is prefixed with its module — `str_find`, not `find`. There is no `pub` and
no namespacing yet, so a short name would collide with a program's own, and a collision is a
compile error rather than a silent shadow. When modules get namespaces the prefix becomes
redundant and can go.
