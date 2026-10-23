# Not every AVClass or class name is static

Affected base: `b2cff27cd691af1482bd36e8c2edab9d5e25fcf8`.

The previous AVClass comment claimed every in-tree instance was file-scope
static const, and `class_name` was always a static literal. At
`libavfilter/vidstabutils.c:vs2av_log`, an automatic `AVClass class` stores
the caller's `tag` and is reached through an automatic `VS2AVLogCtx` during
`av_vlog`. Neither class nor name has a guaranteed static lifetime.

The optional vidstab TU is not enabled in the reusable hermetic build, but
is a concrete counterexample to the repo-wide finding. The wrapper already
uses `ClassRef<'a>` and returns `&'a CStr`; retain that correct design.
Resubmit `submission.json` for AVClass, using `other` for the name's borrow
and documenting the common static case. No missing class destructor exists:
automatic storage expires normally and the tag remains borrowed.
