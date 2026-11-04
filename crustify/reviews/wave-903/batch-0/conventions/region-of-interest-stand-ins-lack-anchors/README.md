# The typed region-of-interest store and view carry no anchor

- Objective: `conventions` (Anchors: what is annotated, and several stand-ins)
- Where: wave 902 batch 1, commit `3ad70349610`, at the wave tip `1cc3c27b646`:
  - `FrameMut::add_regions_of_interest`, `src/libavutil/libavutil/frame.rs:5964`;
  - `SideDataArrayMut::add_regions_of_interest`, `src/libavutil/libavutil/side_data.rs:1035`;
  - `SideDataRef::regions_of_interest`, `src/libavutil/libavutil/frame.rs:1509`.
- Fixed: yes, in this review's Rust-tree commit

## What is wrong

The conventions annotate "the function or method the public API exports, which a consumer
imports instead of the FFI function", and "a member: each of its accessors". When one C
function is split into several Rust functions because one signature cannot express every
valid contract, each of them carries the same anchor and alias (conventions, Anchors,
*What is annotated* and *Several stand-ins*).

`av_frame_new_side_data` and `av_frame_side_data_new` return a type-erased payload. The
campaign already splits each of them by how the payload is filled, and every split is
annotated:

| C function | annotated stand-ins at the wave tip |
|---|---|
| `av_frame_new_side_data` | `FrameMut::alloc_side_data_zeroed`, `FrameMut::alloc_side_data_from_slice` (`frame.rs:5893`, `5931`) |
| `av_frame_side_data_new` | `SideDataArrayMut::alloc_zeroed`, `SideDataArrayMut::alloc_from_slice` (`side_data.rs:943`, `974`) |
| `AVFrameSideData.data` | `SideDataRef::data`, `SideDataMut::data_mut` |

Wave 902 adds a third, typed split of each function: `add_regions_of_interest`, which is
what a consumer storing `REGIONS_OF_INTEREST` side data imports instead of either C
function. It also adds a typed accessor of the entry's payload, `regions_of_interest`,
which reads `AVFrameSideData.data` as regions. None of the three carries a `Wraps` anchor
or a `#[doc(alias)]`. The commit message names the C functions, but the conventions make
the annotations "the only record that an item was translated". A consumer who searches
the documentation for `av_frame_new_side_data` therefore finds the byte-level stores and
not the checked one, which is the store the type's docs direct them to.

The owner-level forwarder `SideDataArray::add_regions_of_interest` is left unannotated, as
the owner-level `SideDataArray::alloc_zeroed` and `alloc_from_slice` are.

## Fix

- `FrameMut::add_regions_of_interest`: ``/// Wraps `fn::av_frame_new_side_data` `` and
  `#[doc(alias = "av_frame_new_side_data")]`.
- `SideDataArrayMut::add_regions_of_interest`: ``/// Wraps `fn::av_frame_side_data_new` ``
  and `#[doc(alias = "av_frame_side_data_new")]`.
- `SideDataRef::regions_of_interest`: ``/// Wraps `fld::AVFrameSideData.data` `` and
  `#[doc(alias = "AVFrameSideData.data")]`.

The prose that named the C function in parentheses now opens with the anchor instead.

## Rerun

```bash
git grep -n -B2 'doc(alias = "av_frame_new_side_data")\|doc(alias = "av_frame_side_data_new")' 1cc3c27b646 -- src
git show 1cc3c27b646:src/libavutil/libavutil/frame.rs | sed -n 5946,5964p
git show 1cc3c27b646:src/libavutil/libavutil/side_data.rs | sed -n 990,1035p
git show 1cc3c27b646:src/libavutil/libavutil/frame.rs | sed -n 1479,1509p
```

The first command lists the four byte-level stand-ins. The other three show the new
functions without an anchor.
