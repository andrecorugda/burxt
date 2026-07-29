# The Burxt standard library

```burxt
use "lib/string.bx";
use "lib/files.bx";
use "lib/os.bx";
```

Written in Burxt, from the same builtins any program has. Nothing here is privileged: every
function could have been written by the program that uses it, and the point is that it was
written **once, carefully**, instead of forty times.

| Module | What it holds |
|---|---|
| [`str.bx`](str.bx) | `string_find`, `string_contains`, `string_starts_with`, `string_ends_with`, `string_trim`, `string_split`, `string_lines`, `string_join`, `string_repeat`, `string_to_int` |
| [`option.bx`](option.bx) | `Option<T>` — absence as a type. `option_or`, `option_is_some`, `option_is_none`. No `unwrap`, on purpose. |
| [`result.bx`](result.bx) | `Result<T, E>` — failure as a type. `result_or`, `result_is_ok`. Both `match` arms always required. |
| [`fs.bx`](fs.bx) | `file_read`, `file_write`, `file_append`, `file_exists`, `file_delete`, `file_move`, `file_make_directory`, `file_list_directory` |
| [`os.bx`](os.bx) | `os_args`, `os_arg`, `os_arg_count`, `os_now`, `os_run`, `os_capture`, `os_read_byte`, `os_read_all` |

## Why it exists

Two reasons, and the second is the real one.

**Convenience**: splitting a string, appending to a file and listing a directory are things
every program needs and none should re-derive.

**Safety at the boundary**: Burxt refuses a C return whose ownership it cannot describe, so
`opendir`, `getenv` and `fopen` are out of reach directly. The way through is the shell or an
`Int` standing in for a pointer — and **every program that writes that itself is a program
making a promise the compiler cannot check.** Here the promise is made once, in the open,
with the reasoning next to it. `file_list_directory(path)` is safe because `fs.bx` did the unsafe part
carefully, one time.

## What it does not hide

- **Failure has no type yet.** `file_read` of a missing file answers `""`, which is
  indistinguishable from an empty file. `string_to_int("abc")` answers 0. The honest fix is
  `Option<T>` and `Result<T, E>`, which the language does not have — so the limitation is
  stated in each function's comment rather than papered over.
- **`file_append` reads the whole file and writes it back**, because the builtin replaces
  rather than appends. Fine for a log line, wrong for a gigabyte, and worth knowing which
  you have.
- **`file_quote` refuses a path containing a single quote** rather than mishandling it. Shell
  quoting has exactly one hole and that is it.
- **Building a large String means chunks**, never an append per byte or per line — this
  project has paid for that three times (v0.0.68, v0.0.77, v0.0.82). `os_read_all` batches
  in 4 KB pieces for that reason.

## Naming

Every function is prefixed with its module — `string_find`, not `find`. There is no `pub` and
no namespacing yet, so a short name would collide with a program's own, and a collision is a
compile error rather than a silent shadow. When modules get namespaces the prefix becomes
redundant and can go.
