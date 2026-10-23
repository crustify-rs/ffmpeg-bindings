# AVMatrixEncoding's finding named the wrong underlying integer type

Affected base: `17c012afcd0e1e6fab63a97ac3e622e16db2cfba` (finding submitted
by formats wave-0 batch 4, `653bf3f6d0`).

The previous `_comment_agent` for `AVMatrixEncoding` ended with "CPlainElem
(every c_int bit pattern valid)". The enum has no negative enumerator
(`libavutil/channel_layout.h`: `AV_MATRIX_ENCODING_NONE = 0` through
`AV_MATRIX_ENCODING_NB = 7`), so the C compiler picks an unsigned
underlying type and bindgen emits
`pub struct AVMatrixEncoding(pub ::std::os::raw::c_uint);`
(libavutil-sys `bindings.rs`); the wrapper's own `CPlainElem` safety comment
and `MatrixEncoding::index` (`self.0.0 as usize`, no sign test) already
treat it as unsigned. A reader of the finding who trusted "c_int" would add
a `raw >= 0` guard that warns `unused_comparisons` (see the campaign's test
recipes) or expect a negative value to survive a round trip through C
storage, which it does not.

Resubmitted `submission.json` with the encoding corrected to `c_uint`, the
swr option default recorded, and the new `Default = NONE` noted:

```
wavefront <worktree> --config crustify/wavefront/configs/wavefront-config.json \
  query types --name AVMatrixEncoding --update submission.json
```

The other eleven findings of this workset were checked against the headers,
`frame.c`, `channel_layout.c` and `pixdesc.c` and need no correction.
