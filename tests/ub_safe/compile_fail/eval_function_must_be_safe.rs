// A user function is called with whatever opaque pointer the evaluator gets,
// so an `unsafe` function, which could demand more of it, is refused.
use ffmpeg::libavutil::eval::Expr;
use std::ffi::c_void;

unsafe extern "C" fn deref(opaque: *mut c_void, x: f64) -> f64 {
    // SAFETY: none; the program is rejected before this could run.
    unsafe { *opaque.cast::<f64>() * x }
}

fn main() {
    let _ = Expr::parse(c"f(1)", &[], &[(c"f", deref)], &[], 0);
}
