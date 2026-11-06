# Misc: `RegionOfInterest` misstated how C readers handle `self_size`

- Item: type `AVRegionOfInterest` (`frame.rs`, `RegionOfInterest`)
- Affected commit: `6fd5597711` (buffer-frame wave 1, type batch 0)
- Kind: documentation and safety-justification defect. No Rust UB is
  reachable yet. No safe API in the crate hands a
  `REGIONS_OF_INTEREST` payload to C, because libavfilter and libavcodec are
  not wrapped. But the docs misled exactly the later wrappers that will.

## Finding

The type's docs said that readers of a region array "take [`self_size`] as
its stride and as the divisor of the payload size, and reject when it is 0".
The `CZeroable` SAFETY comment said that "C's readers of side data reject a
zero `self_size` with an error, not by misbehaving". Both statements are wrong
for two inputs, and the safe `RegionOfInterestMut::set_self_size`,
`RegionOfInterest::from_raw` and `zeroed()` produce both.

1. **A nonzero `self_size` below `sizeof(AVRegionOfInterest)` (28).** Every
   reader checks only `self_size != 0 && size % self_size == 0`, then reads a
   whole struct at each stride:

   ```c
   // libavfilter/vf_showinfo.c:141-151, dump_roi
   roi_size = roi->self_size;
   if (!roi_size || sd->size % roi_size != 0) { /* error */ }
   nb_rois = sd->size / roi_size;
   for (int i = 0; i < nb_rois; i++) {
       roi = (const AVRegionOfInterest *)(sd->data + roi_size * i);
       av_log(..., roi->left, roi->top, roi->right, roi->bottom, roi->qoffset.num, roi->qoffset.den);
   ```

   `vf_addroi.c:117-139`, `libx264.c:431-448`, `libx265.c:658`,
   `libvpxenc.c:1589-1604`, `vaapi_encode.c:535`, `qsvenc.c:2159` and
   `d3d12va_encode.c:214` have the same shape. With `self_size = 4` on a
   one-region (28-byte) payload, `nb_rois` is 7, and the seventh read starts
   at byte 24 and runs 24 bytes past the end.
2. **`self_size == 0`.** Most readers log an error, but `vf_addroi` asserts:
   `av_assert0(old_roi_size && sd->size % old_roi_size == 0)`
   (`vf_addroi.c:119`), which aborts the process.

`frame.h` itself requires `self_size` to be `sizeof(AVRegionOfInterest)`
(`frame.h:399-402`).

## Evidence

`roi_filter.c` is a C probe built against the campaign's `asan-ubsan`
libraries. It feeds one 16x16 frame through `buffer -> <filter> ->
buffersink`, with a 28-byte `REGIONS_OF_INTEREST` payload that holds one
region with the given `self_size`:

```sh
B=/target/crustify/builds/asan-ubsan
export LD_LIBRARY_PATH=$(printf '%s:' $B/lib*)
clang -O1 -g -fsanitize=address,undefined -I/target -I$B roi_filter.c \
  $(printf -- '-L%s ' $B/lib*) -lavfilter -lavutil \
  -Wl,-rpath,$LD_LIBRARY_PATH -o roi_filter
./roi_filter 28 showinfo                                  # prints the region, exit 0
./roi_filter 4 showinfo                                   # showinfo-self-size-4.txt
./roi_filter 4 addroi "x=0:y=0:w=8:h=8:qoffset=-0.1"      # addroi-self-size-4.txt
./roi_filter 0 addroi "x=0:y=0:w=8:h=8:qoffset=-0.1"      # addroi-self-size-0.txt
./roi_filter 0 showinfo                                   # "Invalid AVRegionOfInterest.self_size.", exit 0
```

```
==1168236==ERROR: AddressSanitizer: heap-buffer-overflow on address 0x50900000159c
READ of size 4 at 0x50900000159c thread T0
    #0 in dump_roi src/libavfilter/vf_showinfo.c:151:96
    #1 in filter_frame src/libavfilter/vf_showinfo.c:830:13
0x50900000159c is located 0 bytes after 28-byte region [0x509000001580,0x50900000159c)
    #3 in av_frame_new_side_data src/libavutil/frame.c:652:24
```

```
Assertion old_roi_size && sd->size % old_roi_size == 0 failed at src/libavfilter/vf_addroi.c:119
(exit 134, SIGABRT)
```

## Fix

`frame.rs`: the type's docs now state that C requires `self_size ==
SELF_SIZE`. They describe what readers check and what they then read, and
name both hazards. They also say that any wrapper handing a region array to C
must check every element's `self_size` first. The `CZeroable` SAFETY comment
no longer claims that a zero `self_size` is always a C error. It now rests on
the fact that no safe wrapper hands a region to C, and points to the docs.
`set_self_size` warns against undersized values.

The accessors stay as they are. A region read from C side data may carry any
value, and a guard on the setter would not protect side data written as plain
bytes. The guard belongs where a payload reaches C (`av_frame_side_data_*`
stores in buffer-frame wave 3, and the libavfilter/libavcodec sub-campaigns).
The corrected wavefront finding records the hazard for them
(`../../wavefront/region-of-interest-reader-checks-incomplete`). It also
records the encoders' unchecked `qoffset` arithmetic.
