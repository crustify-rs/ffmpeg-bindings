# Safe boundary: `HashContextFree` went through a private raw-pointer `unsafe fn`

- File: `crustify/rust/ffmpeg/libavutil/hash.rs`
- Affected commit: `5da4d94447` (wave tip `7935c403e0`)
- Item: `AVHashContext` (`HashContextFree`, the `av_hash_freep` policy)

## Finding

`crustify scan-unsafe --sites` reported one `unsafe_fns_priv_smell` and one
`raw_ptr_priv_smell` in the workset, both at `hash.rs:39`: the release
policy bound `av_hash_freep` (which takes `AVHashContext **`) through a
private adapter `unsafe fn free_context(context: *mut ffi::AVHashContext)`
passed to `impl_cdrop!`. The adapter added an `unsafe fn` with its own raw
pointer argument and a `# Safety` contract that merely restated `CDrop`'s,
an extra unsafe surface the policy does not need: the wave's other policies
(`Md5Free`, `RipemdFree`, `Cast5Free`, `AesFree`) implement `CDrop` by hand.

## Fix

`HashContextFree` implements `CDrop<HashContext>` directly; `c_drop` keeps
the local slot and calls `av_hash_freep(&mut slot)` under the same safety
argument. No behaviour change. `scan-unsafe` reports no smell site in the
workset's homes afterwards. Existing coverage (`hash` unit, `ub_safe`,
`equiv` and the drop-parent compile-fail cases) exercises the release.
