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
| [`string.bx`](string.bx) | **bytes:** `string_find`, `string_contains`, `string_starts_with`, `string_ends_with`, `string_trim`, `string_split`, `string_lines`, `string_join`, `string_repeat`, `string_to_int`, `string_parse_int` · **codepoints:** `char_count`, `char_at`, `next_char`, `char_index`, `codepoint_at`, `is_continuation`, `is_valid_utf8`, `to_bytes`, `string_reverse` · **ASCII-only, and the name says so:** `string_to_upper_ascii`, `string_to_lower_ascii`, `is_ascii`, `all_digits`, `is_alpha`. **`from_codepoint`/`from_bytes` are absent** — no Int→String path exists; see the gap at the foot of the file |
| [`option.bx`](option.bx) | `Option<T>` — absence as a type. `option_or`, `option_is_some`, `option_is_none`. No `unwrap`, on purpose. |
| [`result.bx`](result.bx) | `Result<T, E>` — failure as a type. `result_or`, `result_is_ok`. Both `match` arms always required. |
| [`array.bx`](array.bx) | `array_contains`, `array_index_of`, `array_is_sorted`, `array_min`/`max`/`first`/`last`, `array_sum_int`/`sum_money`, and the changing half: `array_swap`, `array_reverse`, `array_sort`, `array_fill`, `array_extend`, `array_remove_at`. **`mutable xs: [T]` in the signature means the call changes YOUR array** |
| [`math.bx`](math.bx) | `INT_MAX`/`INT_MIN`, `math_min`/`max`/`clamp` (generic over `Ordered`), `math_abs`, `math_sign`, `math_pow`, `math_isqrt`, `math_gcd`, `math_lcm`, and four families for overflow: `+` traps, `math_checked_*` answers `None`, `math_saturating_*` clamps to the bound, `math_wrapping_*` discards the carry |
| [`set.bx`](set.bx) | `Set<T: Equatable>` over `Map<T, Bool>` — `set_new`, then methods: `add`, `add_all`, `has`, `remove`, `count`, `items`, `take`, `union`, `intersect`, `difference`, `is_subset_of`, `equals`. **Insertion order, and a re-added element goes to the end.** `union`/`intersect`/`difference` answer a NEW set |
| [`time.bx`](time.bx) | `DateTime` and `Duration`, **UTC only and whole seconds — both said in the header, not guessed**. `time_from_unix`/`time_to_unix` (Hinnant's exact-integer calendar, no table, no float), `time_format_iso`/`time_parse_iso` (RFC 3339; an offset is **refused**, never read as UTC), `time_weekday`, `time_day_of_year`, `time_is_leap_year`, `time_days_in_month`, `duration_seconds`/`minutes`/`hours`/`days`, `time_add`, `time_between`. Dates before 1970 work. **No leap seconds** — unix time has none |
| [`map.bx`](map.bx) | `Map<K, V>` in insertion order, always. `map_new`, then methods: `set`, `get`, `find`, `has`, `remove`, `count`, `keys` |
| [`json.bx`](json.bx) | `Json` and `Field`, `json_render`, `json_parse`, and the typed readers. **A JSON number is its digits, not a float** — see the header |
| [`vector.bx`](vector.bx) | `vector_dot`, `vector_magnitude`, `vector_is_unit`, `vector_normalise`, `vector_top_dot`, and a store: `vector_store_render`/`parse`/`read`/`write`/`append`. Exact, never through a float |
| [`test.bx`](test.bx) | `Tests`, `test_begin`, `check_int`/`money`/`decimal`/`string`/`bool`/`that`, `test_end`. Testing Burxt, in Burxt — no registration, because there are no function values |
| [`files.bx`](files.bx) | `file_read`, `file_write`, `file_append`, `file_exists`, `file_delete`, `file_move`, `file_make_directory`, `file_list_directory` |
| [`os.bx`](os.bx) | `os_args`, `os_arg`, `os_arg_count`, `os_now`, `os_run`, `os_capture`, `os_read_byte`, `os_read_all`, `os_byte_as_string` |

The first two rows used to read `str.bx` and `fs.bx`, which have never been the filenames — so both
links 404'd, and `map.bx` and `json.bx` were missing from a table whose whole job is saying what is
here. **`array.bx`, `vector.bx` and `test.bx` were missing from it too**, and for longer — found the
same way, by listing the directory instead of reading the table.

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
