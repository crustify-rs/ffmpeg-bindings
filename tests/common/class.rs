//! Reaches the `ClassRef` of a raw class through the safe API: the class of a
//! logging context, as [`Record::class`] reads it inside a log callback.
//! Shared by the `log` suites of `equiv` and `ub_unsafe`, which include it
//! with `#[path]`.
#![allow(dead_code)]

use core::cell::Cell;
use core::ffi::c_void;

use ffmpeg::libavutil::log::{self, Callback, ClassRef, Level, Record};

/// The visitor [`with_class`] lent this thread's callback, lifetime erased.
type Visitor = *mut (dyn FnMut(Option<ClassRef<'_>>) + 'static);

thread_local! {
    static VISIT: Cell<Option<Visitor>> = const { Cell::new(None) };
}

/// Hands the class of each message logged on a visiting thread to its
/// visitor; other threads' messages are dropped.
struct Visit;

impl Callback for Visit {
    fn log(record: Record<'_>) {
        if let Some(visit) = VISIT.with(Cell::take) {
            // SAFETY: `with_class` lent the visitor on this thread and keeps
            // it alive until the logging call it makes returns.
            unsafe { (*visit)(record.class()) }
        }
    }
}

/// `f` applied to the class of `obj` as a log callback receives it, with
/// `Record::class`. The caller holds its suite's `LogState` lock, as for any
/// change of the process-wide log callback, and the default callback is
/// installed again afterwards.
///
/// # Safety
///
/// `obj` must be a logging context valid for `log_with_context`.
pub unsafe fn with_class<R>(obj: *mut c_void, f: impl FnOnce(Option<ClassRef<'_>>) -> R) -> R {
    let mut f = Some(f);
    let mut out = None;
    let mut visit = |class: Option<ClassRef<'_>>| out = Some((f.take().unwrap())(class));
    let visit: &mut dyn FnMut(Option<ClassRef<'_>>) = &mut visit;
    // SAFETY: only the trait object's lifetime is erased; the callback calls
    // it during `log_with_context` below and the slot is cleared before
    // `visit` goes out of scope.
    let visit: Visitor = unsafe { core::mem::transmute(visit) };
    VISIT.with(|v| v.set(Some(visit)));
    log::set_callback::<Visit>();
    // SAFETY: the caller vouches for `obj`.
    unsafe { log::log_with_context(obj, Level::INFO, c"class\n") };
    log::set_default_callback();
    VISIT.with(|v| v.set(None));
    out.expect("the log callback visited the class")
}
