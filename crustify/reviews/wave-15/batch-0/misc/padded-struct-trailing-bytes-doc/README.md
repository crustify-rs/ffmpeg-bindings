# Misc: `PaddedStruct` did not say that C's buffer runs past `reserved_padding`

- File: `crustify/rust/ffmpeg/libavutil/bprint.rs`
- Affected commit: `9de9d5c5ce` (core wave 0, batch 4)
- Item: `FF_PAD_STRUCTURE` (type route, wrap), the `PaddedStruct` trait

## Finding

`PaddedStruct` exists to describe the generator's layout to the instance
wrappers that follow (`AVBPrint`, wave 1). Its docs said C "may use those
bytes (`AVBPrint`'s internal buffer runs from `reserved_internal_buffer`
through the padding to the end)", and the compile-time assertion checks
that `offset_of!(reserved_padding) + PADDING_LEN` rounds up to `SIZE`, but
nothing stated the consequence: `reserved_padding` ends at byte 1020 and the
structure at 1023, and C's automatic buffer (`size_auto = sizeof(*buf) -
offsetof(reserved_internal_buffer)`, `bprint.c:71-72`) writes into those 3
trailing bytes. The equivalence test
`c_fills_the_storage_to_its_last_byte_and_no_further` already proves the
NUL lands at byte `SIZE - 1`.

Rust regards bytes 1021..1024 of `ffi::AVBPrint` as trailing padding, which
a typed copy (`ptr::read`, a by-value move of the struct or of a
`MaybeUninit` holding it) need not preserve. A wave-1 wrapper that moved an
initialised instance by value would lose the tail of a full automatic
buffer silently (besides breaking the self-pointer `str`), so the layout
trait should say that the instance must stay in place. This is a
documentation gap, not a code defect: no current safe path reaches it.

## Fix

- The trait docs now spell out that "the end" is byte `SIZE - 1`, that C
  writes into the bytes past `reserved_padding`, that Rust treats them as
  padding a typed copy need not keep, and that an initialised instance must
  be reached through its address.
- The inline unit test `avbprint_is_padded_to_1024_bytes` asserts the
  padding's end (1021) and the 3-byte distance to `SIZE`, tying the doc
  claim to the binding.
