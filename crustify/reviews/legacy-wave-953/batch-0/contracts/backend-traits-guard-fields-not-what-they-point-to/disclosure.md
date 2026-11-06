Subject: ffmpeg-bindings: backend hwctx traits guard the struct's fields, not the arrays they point to

Hi Marius,

Wave 952 of `ffmpeg-bindings` added `BackendDeviceContext` and `StableBackendFramesContext`. An
implementation asserts that the backend no longer writes what the shared handle reads once the
context is initialised, which lets `DeviceContextRef::hwctx` and `FramesContextRef::hwctx` lend
it through any reference. The obligation is worded over fields only:

```rust
/// - the backend writes none of its fields once the context is initialised …, except fields
///   `Self::Ref`'s getters do not read, or reads through interior-mutable means …
```

Backend structs hold arrays behind pointer fields (`surface_ids`, `texture_infos`, `surfaces`),
which a getter would lend as a slice. Suppose a backend writes into such an array after init
without reassigning the pointer. It meets the wording, while mutating memory under a live
`&[T]`. D3D11VA reassigns `texture_infos` as it grows it (`av_realloc_f`), so the cited example
is excluded anyway. But the rule as written does not exclude in-place writes.

No reproducer: no backend implements the traits yet. The review's patch states both obligations,
and the matching `// SAFETY:` comments, over everything the getters read, including memory
reached through the fields.

Found by [Crustify](https://github.com/crustify-rs/crustify), an experimental
UB/soundness auditing agent developed at UC Berkeley and running on Claude Opus 5.5, then
manually reviewed and independently reproduced.
