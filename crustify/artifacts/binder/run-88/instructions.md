## Items that have landed (departure)

`AVHWDeviceContext` already has a wrapper, from wave 42, and `AVHWFramesContext` one from
wave 44. Your batch schedules the device context again on purpose: treat its existing
annotations as work to bring to the current specs, not as completed work, and keep the
public API wherever the specs allow.

## The hardware context casts (departure: also `AVHWFramesContext.hwctx`)

`DeviceContextRef::hwctx`, `DeviceContextMut::into_hwctx`, `FramesContextRef::hwctx` and
`FramesContextMut::into_hwctx` are `pub unsafe fn`s generic over any `T: CCell`, whose
obligation is that `T::C` be the hardware context struct `hwcontext_<type>.h` declares for
the context's device type. The wave-43 review kept them unsafe because no backend header
is in the campaign and the builds compile no backend. The device type is known at run time
(C sets `AVHWDeviceContext.type` at allocation and never changes it, and sizes `hwctx` from
it), so the binding can check the obligation instead of passing it on: for instance an
unsafe trait per context kind that a backend's struct implements with its device type as
a constant, the accessors comparing it with the context's and returning `None` on a
mismatch. Make all four safe this way, the frames ones included, although
`AVHWFramesContext` is wave 954's item.

Points the check leaves open:

- some backends write their frames hwctx after init, as they allocate frames through any
  reference (D3D11VA records its textures), so a backend may need the pre-init exclusive
  path without the shared getter;
- what is written before init stays the backend's business: its own field setters, when
  a backend is wrapped, carry whatever their foreign handles need;
- no backend implements the trait yet: the test fixture's fake backend implements it in
  test code, and the trait's docs say what a backend module's implementation asserts.

Update the landed tests and callers that name the casts.
