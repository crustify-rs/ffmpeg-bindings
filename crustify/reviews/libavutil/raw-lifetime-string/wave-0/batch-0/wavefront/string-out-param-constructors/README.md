# Wavefront: out-parameter string constructors were never analyzed

- Store: `crustify/wavefront/ownership-store.json`
- Affected commit: `a95b9e3220` ("wrap libavutil's string raw-lifetime primitives")
- Records: `av_escape`, `av_dict_get_string`, `av_opt_serialize`, `av_opt_get`,
  `av_opt_get_key_value`, `av_bprint_finalize` (no record before); `av_strdup`
  (`ptr_ret` note)

## Finding

The previous run catalogued the string tier's constructors from the return
form only: `av_asprintf`, `av_get_token`, `av_append_path_component` and
`av_strireplace` (all `for: "return"`). libavutil's public API also hands out
fresh `av_free`-released strings through a `char **` out-parameter, and none
of those had a record (`_analysis.submitted: false`, every `ptr` null), even
`av_escape`, which sits in the same `avstring.c` as the catalogued ones. The
previous run's own `av_free` note names `av_bprint_finalize`'s out-string as
one of the strings it releases.

| symbol | out slot | what C stores there |
|---|---|---|
| `av_escape` (`avstring.c`) | `char **dst` | `av_bprint_finalize`'s string |
| `av_dict_get_string` (`dict.c`) | `char **buffer` | `av_strdup("")` or `av_bprint_finalize`'s string |
| `av_opt_serialize` (`opt.c`) | `char **buffer` | `av_bprint_finalize`'s string |
| `av_opt_get` (`opt.c`) | `uint8_t **out_val` | `av_strdup` of the formatted value, an `av_malloc` hex dump, `av_dict_get_string`'s string, or an `av_reallocp`-grown buffer for array options |
| `av_opt_get_key_value` (`opt.c`) | `char **rkey`, `char **rval` | `get_key`'s `av_malloc` key, `av_get_token`'s value |
| `av_bprint_finalize` (`bprint.c`) | `char **ret_str` | the `av_realloc` heap buffer, or `av_memdup(str, min(len + 1, size))` |

`opt.h` says it outright for `av_opt_get_key_value`: "`rkey` parsed key; must
be freed using av_free()".

### `av_bprint_finalize` returns an unterminated string for a count-only buffer

While analyzing `ret_str`: for an `AV_BPRINT_SIZE_COUNT_ONLY` buffer (or
`av_bprint_init_for_buffer(.., 0)`), `size` is 0, so `real_size =
FFMIN(len + 1, size)` is 0 and the result is `av_memdup(str, 0)`, a 1-byte
`av_malloc` block that is never written. It is not NUL-terminated. On the
shared ASan build:

```c
AVBPrint bp; char *s = NULL;
av_bprint_init(&bp, 0, AV_BPRINT_SIZE_COUNT_ONLY);
av_bprintf(&bp, "hello");
av_bprint_finalize(&bp, &s);   /* returns 0 */
strlen(s);
```

```console
==592606==ERROR: AddressSanitizer: heap-buffer-overflow on address 0x6dcc93fe0041
READ of size 2 at 0x6dcc93fe0041 thread T0
    #0 in strlen
    #1 in main t.c:12:28
0x6dcc93fe0041 is located 0 bytes after 1-byte region [0x6dcc93fe0040,0x6dcc93fe0041)
allocated by thread T0 here:
    #0 in posix_memalign
    #1 in av_malloc        libavutil/mem.c:107:9
    #3 in av_memdup        libavutil/mem.c:306:15
    #4 in av_bprint_finalize libavutil/bprint.c:248:19
```

This is a C defect outside this tier's Rust surface. It matters to whichever
batch wraps `av_bprint_finalize`: that batch must not adopt `ret_str` as a
`HeapString` (`CStrBox::from_raw` requires a NUL-terminated string) for a
count-only buffer. The record's `ret_str` note now says so. `av_escape`,
`av_dict_get_string` and `av_opt_serialize` init their buffers with a
non-zero size, so they never hit this.

### `av_strdup`'s limit boundary

The `ptr_ret` note said `av_strdup` fails "when the length exceeds
av_max_alloc". `av_realloc` rejects `strlen(s) + 1 > max`, so a string whose
length equals the limit already fails. The new equivalence test
`string_failure_at_the_av_max_alloc_limit_matches_c` pins that boundary (see
`misc/string-max-alloc-limit-docs`).

## Correction

Submitted through `wavefront ... query symbols --name <sym> --file <tu>
--update`:

- `av_escape`, `av_dict_get_string`, `av_opt_serialize`, `av_opt_get`:
  complete `ptr_args`, and `lifetime` = `is_constructor.alloc` on the out
  slot (`scalar.by_ref.owned: true`).
- `av_opt_get_key_value`: the same, with `for: "rkey"`. The schema carries
  one role per symbol, so the `rval` note records that it is a second
  string constructed the same way.
- `av_bprint_finalize`: `lifetime` = `is_disposer` on `buf`, since that is
  `AVBPrint`'s teardown and the type batch needs it. The string output is
  recorded in the `ret_str` block and its note, along with the count-only
  defect above.
- `av_strdup`: `ptr_ret` note corrected to `strlen(s) + 1 > av_max_alloc`.

## Wavefront limitation

After the corrections, `query symbols --lifetime-for string` still lists only
the four return-form constructors. `_lifetime_for` matches the `for` subject on
its own `ptr.string` verdict (`structural_string=False`, `query.py`), and an
out slot is a `char **`, not a string. Its `string` is correctly `false`, as
the previous run recorded for `av_get_token`'s `buf`, and the `by_ref`
inner block has no `string` flag to set. Out-parameter string constructors
are therefore invisible to `--lifetime-for string` whatever is submitted;
this report and the records' notes are where they are listed. A
`by_ref.string` flag (or matching `char **` subjects structurally in
`--lifetime-for`) would close the gap.

Not submitted: the private `get_opt_flags_string` (`opt.c`, static) and
`getenv_dup` (`getenv_utf8.h`) also return `av_free`-released strings, but
they are not part of the public API this wrap campaign targets.
