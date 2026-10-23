# Conventions: `avstring.rs`'s module doc had an ambiguous intra-doc link

- File: `crustify/rust/ffmpeg/libavutil/avstring.rs`, line 3
- Affected commit: `5d0c0b3b31`
- Home of this batch's `av_isgraph`, `av_isspace`, `av_isxdigit`,
  `av_match_name` and `av_match_list`

## Finding

The module doc linked `` [`format`] ``, which rustdoc cannot resolve: the
module has a function `format` (`av_asprintf`, from a sibling batch) and the
prelude has the `format!` macro. `RUSTDOCFLAGS=-D warnings cargo doc -p
ffmpeg` fails on the file:

```text
error: `format` is both a function and a macro
 --> ffmpeg/libavutil/avstring.rs:3:7
  |
3 | //! [`format`] and [`append_path_component`] return a [`HeapString`]: an
  |       ^^^^^^ ambiguous link
```

So the crate's docs did not build under the warning gate, for every batch
homed in `avstring.rs`.

## Fix

The link is now `` [`format()`] ``, the function. `cargo doc -p ffmpeg` is
clean under `-D warnings`.
