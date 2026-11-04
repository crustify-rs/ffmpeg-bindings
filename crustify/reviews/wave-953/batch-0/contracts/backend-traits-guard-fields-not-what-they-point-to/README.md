# The backend traits' no-writer obligation covers the struct's fields, not what they point to

- Objective: `contracts`
- Where: `BackendDeviceContext`'s `# Safety`, second item
  (`src/libavutil/libavutil/hwcontext.rs:2207-2210`), and `StableBackendFramesContext`'s
  (`:3326-3330`), with the `// SAFETY:` comments of `DeviceContextRef::hwctx` (`:2286-2291`) and
  `FramesContextRef::hwctx` (`:3424-3429`) that rely on them. At the wave tip `3ee33a6864f`
  (wave 952 batch 0, commit `1b7312582ee`).
- Reproducer: none. No backend implements the traits yet, so no implementation can break them.
  This is an incomplete contract.
- Fixed: yes, in this review's Rust-tree commit (documentation).

## What is wrong

The shared accessors hand out `T::Ref<'a>` over a context's backend struct, through any
reference to an initialised context. For that to be sound, nothing may write what the handle
reads while it lives. The traits' implementers carry that obligation, but it is worded over the
struct's fields only:

> the backend writes none of its fields once the context is initialised …, except fields
> `Self::Ref`'s getters do not read, or reads through interior-mutable means the backend
> synchronizes with

(`StableBackendFramesContext`: "writes none of the struct's fields … except fields `Self::Ref`'s
getters do not read …"). The `// SAFETY:` comments repeat it: "`T`'s implementation asserts its
backend writes none of it".

The backend structs these traits are for hold pointers to arrays. Their natural getters lend
views of those arrays: `AVVAAPIFramesContext.surface_ids`/`nb_surfaces`,
`AVD3D11VAFramesContext.texture_infos`, `AVDXVA2FramesContext.surfaces`. A backend that writes
into such an array in place after initialisation, leaving the pointer field alone, satisfies the
wording as written. Meanwhile a `CSlice` that `Self::Ref` lent over the array is mutated under a
live shared borrow. D3D11VA, the case the trait docs cite, happens to also reassign the field
(`av_realloc_f`, `hwcontext_d3d11va.c:195`), and so is excluded either way. But it also writes
through the array (`:206-207`), and a backend that preallocates and only fills in would not
reassign. The obligation should be stated over what the getters read, wherever it lives.

## Fix

Both obligations now read over what `Self::Ref`'s getters read: "the backend writes nothing
`Self::Ref`'s getters read once the context is initialised …: neither the fields they read nor the
memory they read or lend views of through those fields, such as an array a field points to, unless
they read it through interior-mutable means the backend synchronizes with". The two
`// SAFETY:` comments say the implementation asserts that "its backend writes nothing the handle
reads, through the struct's fields or what they point to".

## Rerun

```bash
git show 3ee33a6864f:src/libavutil/libavutil/hwcontext.rs | sed -n 2200,2215p
git show 3ee33a6864f:src/libavutil/libavutil/hwcontext.rs | sed -n 3322,3331p
sed -n 190,210p c/libavutil/hwcontext_d3d11va.c
```
