# Class documentation overstated static lifetime

Affected base: `b2cff27cd691af1482bd36e8c2edab9d5e25fcf8`.
Home: `crustify/rust/ffmpeg/libavutil/log.rs`, AVClass anchor/module docs.

The docs called every FFmpeg class static const and described Class only as
a static. `libavfilter/vidstabutils.c:vs2av_log` has an automatic class whose
name borrows its caller's tag; `Class::new` can also safely construct a local
Rust value. Clarify immutable-while-borrowed versus static lifetime. The
existing lifetime-bounded handles are correct; retain Send/Sync because
borrows forbid mutation and class callbacks obey their contracts, not because
every descriptor must be static. Add a local-class/thread/reborrow public-API
test and an internal stack-descriptor/heap-name projection test.
