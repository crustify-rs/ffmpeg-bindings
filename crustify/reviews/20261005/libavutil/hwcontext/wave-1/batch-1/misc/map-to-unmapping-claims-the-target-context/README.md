# Misc: `HardwareFrame::map_to`'s unmapping docs claim the result is in the target context

- Item: `av_hwframe_map` (`HardwareFrame::map_to`), homed in
  `crustify/rust/ffmpeg/libavutil/hwcontext.rs` by translation batch 1
  (`62b80e7a1d`).
- Affected commit: the wave tip `b9dbecd340`.
- Status: documentation and a comment fixed, with a regression unit test.
  This is not a soundness defect: the result is still a hardware frame of
  the context it holds, which is all `HardwareFrame`'s invariant asks.

## Finding

The method docs said: "Maps the frame into another frames context,
`frames`, as a hardware frame of that context's format ... When this frame's
context was derived from `frames`, the result is the frame this one was
mapped from". The comment where the value is built said that it holds
"`frames`' context (or, unmapping, the source frame's, which is `frames`')".

C's unmapping (`hwcontext.c:806-820`) fires whenever the source frame's
context was derived from `frames`. It returns
`av_frame_replace(dst, hwmap->source)`: the frame the source was mapped
*from*, in whichever context that frame is. A derived context D of S holds
two kinds of frame:

- its own allocations (`av_hwframe_get_buffer` on D allocates in S and maps
  into D), whose source is in S;
- frames mapped into D from a third context X, which `map_to(D)` accepts.
  Their source is in X.

So `x.map_to(D)?.map_to(S)` returns `x`'s frame, in X, with X's format.
`frames()` on the result is X, not S.

## Evidence

`hwcontext::unit_tests::contexts::unmapping_returns_the_mapped_frame_in_its_own_context`,
over the test backends: kind A contexts `fa` and `other` on one device, and
`derived = fa.derive(DRM_PRIME, kind B device)`. Then
`other.get_buffer()?.map_to(derived)?.map_to(fa)` holds `other`'s context,
not `fa`'s. A frame `derived` allocated itself unmaps into `fa`. Both
assertions pass, on the wave tip as well, so this is C's behaviour, which
the wrapper preserves.

## Fix

The docs now say that the result is the frame this one was mapped from, in
that frame's own context: `frames` for the derived context's allocations,
and the third context for a frame mapped in from there. The construction
comment was corrected to match.
