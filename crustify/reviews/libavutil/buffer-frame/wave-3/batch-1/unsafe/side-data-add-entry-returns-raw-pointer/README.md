# Unsafe smell: `add_entry` handed the new entry back as a raw pointer

- Item: `av_frame_side_data_add` (`SideDataArrayMut::add` /
  `SideDataArray::add`, `crustify/rust/ffmpeg/libavutil/frame.rs`). It was
  emitted by the wave-2 type run (`13730d672b`), and translation batch 1 of
  wave 3 (`63d684f623`) reused it.
- Counter: `raw_ptr_priv_smell`, one site, at `frame.rs:2030` on the
  affected commit.
- Status: fixed.

## Finding

The private helper `fn add_entry(&mut self, …) -> Result<*mut
ffi::AVFrameSideData, BufferReferenceBox>` returned the entry C added as a
raw pointer. Each of its three callers then turned that pointer back into
a handle in its own `unsafe { SideDataMut::from_ptr(entry) }`, with a
safety comment restating the lifetime argument. The callers were
`SideDataArrayMut::add`, `SideDataArray::add`, and batch 1's
`into_new_entry`. The raw pointer existed only to dodge E0716 on the
owner's twin. Batch 1's own `into_found` and `into_new_entry` already use
the better shape: a consuming helper on `SideDataArrayMut<'a>` that lends
the handle for `'a`.

## Fix

`add_entry` became `fn into_added(self, …) -> Result<SideDataMut<'a>,
BufferReferenceBox>`. It holds the one `from_ptr` and its safety
argument. `add`, the owner's `add`, and `into_new_entry` now call
`self.as_mut().into_added(…)` or `self.into_added(…)`. This removes the
smell site and two unsafe blocks. Behaviour is unchanged: the existing
unit, ub_safe and equiv tests of `add` pass.
