//! Wave 952's test fixtures implement `BackendDeviceContext` for `DeviceType::VAAPI` twice
//! over: `tests/common/hwdevice.rs`'s `FakeHw` claims that every VAAPI context's `hwctx` is a
//! `FakeHwctx` (at least 48 bytes). But `tests/common/hwbackend.rs`, which the same test
//! crates include (the lib's unit tests, `equiv` and `ub_unsafe`), makes VAAPI contexts
//! (kind A) whose `hwctx` is its 4-byte `DeviceHwctx`. The one `unsafe` call below is the
//! fixtures' own, made exactly as `tests/ub_unsafe/hwcontext.rs` makes it. The read past
//! the 4-byte block happens in safe code: `hwctx::<FakeHw>()` and `value()`.
//!
//! On the affected commit (wave 952's `1b7312582ee`, the wave-953 tip `3ee33a6864f`) the
//! accessor lends the struct: under ASan the read aborts with a heap-buffer-overflow, and
//! without it the program prints what it read and exits 1. Once `FakeHw` claims a type no
//! test backend device has, the accessor lends nothing and the program exits 0.

use ffmpeg_bindings::libavutil::libavutil::hwcontext::{
    BackendDeviceContext, BackendFramesContext, DeviceReferenceBox, DeviceType, FramesContextMut,
    FramesContextRef, StableBackendFramesContext,
};
use libavutil_sys as ffi;

#[allow(dead_code, unfulfilled_lint_expectations)]
#[path = "../../../../../../../tests/common/hwdevice.rs"]
mod hwdevice;

#[allow(dead_code, unfulfilled_lint_expectations)]
#[path = "../../../../../../../tests/common/hwbackend.rs"]
mod hwbackend;

fn main() {
    // As `tests/ub_unsafe/hwcontext.rs`'s `device` makes one, within `from_c`'s contract.
    // SAFETY: the only reference to an initialised test device context, built as
    // `av_hwdevice_ctx_alloc` builds one.
    let device = unsafe { DeviceReferenceBox::from_c(hwbackend::device(hwbackend::Kind::A)) }
        .expect("a kind A device");
    let ctx = device.as_ref().context();
    assert_eq!(ctx.device_type(), DeviceType::VAAPI);
    // Safe from here: the accessor checks the type against `FakeHw::DEVICE_TYPE`, and when they
    // match trusts `FakeHw`'s implementation for the struct's layout.
    match ctx.hwctx::<hwdevice::FakeHw>() {
        None => println!("not lent: FakeHw does not claim this device's type"),
        Some(hw) => {
            println!("FakeHwctx.value read from a 4-byte DeviceHwctx: {}", hw.value());
            std::process::exit(1);
        }
    }
}
