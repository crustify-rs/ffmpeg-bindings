//! UB tests for `libavutil/eval.h`'s unsafe API.

/// `Expr::parse_with_context`, each call within its `# Safety` contract: a
/// NULL parent context, a class-less one, and one led by a valid class
/// (freed right after the parse, which keeps no pointer to it), at level
/// offsets up to the overflow bound, on valid and invalid expressions, on
/// several threads sharing one context.
mod parse_with_context {
    use core::ffi::{CStr, c_int, c_void};
    use core::ptr;
    use std::thread;

    use ffmpeg::libavutil::eval::{Expr, UnaryFn};
    use ffmpeg::libavutil::ffi;

    /// `AVERROR(EINVAL)`.
    const EINVAL: c_int = -22;

    /// A logging context: its class first, as every C context has.
    #[repr(C)]
    struct Ctx {
        class: *const ffi::AVClass,
    }

    /// The current `LIBAVUTIL_VERSION_INT`.
    const VERSION: c_int = (ffi::LIBAVUTIL_VERSION_MAJOR << 16
        | ffi::LIBAVUTIL_VERSION_MINOR << 8
        | ffi::LIBAVUTIL_VERSION_MICRO) as c_int;

    /// Whether this run is under BSan, which misreports C's parser: every
    /// primary goes through `av_strtod`, whose end pointer glibc's uninstrumented
    /// `strtod` writes, so the local keeps the provenance an earlier frame left
    /// in its slot (often a freed parse buffer) and `next[0]` can read as a use
    /// after free. Which parse trips depends on the stack layout alone: two
    /// `av_expr_parse` calls from pure C, the second from a deeper frame, trip it
    /// at some depths. The tests that parse return early under it.
    fn under_bsan() -> bool {
        std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan")
    }

    /// A class C's default callback can name a parent by.
    fn class() -> ffi::AVClass {
        ffi::AVClass {
            class_name: c"ub-eval-parent".as_ptr(),
            item_name: Some(ffi::av_default_item_name),
            version: VERSION,
            ..Default::default()
        }
    }

    extern "C" fn twice(_: *mut c_void, x: f64) -> f64 {
        2.0 * x
    }

    /// The parses every context runs: valid and invalid expressions, at the
    /// offsets around the overflow bound. Offsets that keep the error level
    /// at or below the default log level make C log through the context.
    ///
    /// # Safety
    ///
    /// `ctx` must be valid for `parse_with_context` throughout.
    unsafe fn parses(ctx: *mut c_void) {
        let unary: [(&CStr, UnaryFn); 1] = [(c"f", twice)];
        let max = c_int::MAX - ffi::AV_LOG_ERROR as c_int;
        for offset in [c_int::MIN, -8, 0, 8, 40, max] {
            // SAFETY: the caller vouches for `ctx`.
            unsafe {
                let e = Expr::parse_with_context(c"f(X) + 1", &[c"X"], &unary, &[], offset, ctx);
                drop(e.unwrap());
                for s in [c"f(", c"Y", c"nosuch(1)", c"1 )"] {
                    let r = Expr::parse_with_context(s, &[c"X"], &unary, &[], offset, ctx);
                    assert_eq!(r.err(), Some(EINVAL), "{s:?} at {offset}");
                }
            }
        }
        for offset in [max + 1, c_int::MAX] {
            // SAFETY: as above; rejected before C runs.
            unsafe {
                let r = Expr::parse_with_context(c"1", &[], &[], &[], offset, ctx);
                assert_eq!(r.err(), Some(EINVAL));
                let r = Expr::parse_with_context(c"1+", &[], &[], &[], offset, ctx);
                assert_eq!(r.err(), Some(EINVAL));
            }
        }
    }

    #[test]
    fn null_context() {
        if under_bsan() {
            return;
        }
        // SAFETY: NULL is "no parent context".
        unsafe { parses(ptr::null_mut()) };
    }

    #[test]
    fn classless_context() {
        if under_bsan() {
            return;
        }
        let mut ctx = Box::new(Ctx { class: ptr::null() });
        // SAFETY: a live struct led by a NULL class pointer, unwritten while
        // the parses run.
        unsafe { parses((&raw mut *ctx).cast()) };
    }

    #[test]
    fn context_with_a_class() {
        if under_bsan() {
            return;
        }
        let class = Box::new(class());
        let mut ctx = Box::new(Ctx { class: &*class });
        // SAFETY: a live struct led by a pointer to a valid, live class.
        unsafe { parses((&raw mut *ctx).cast()) };
    }

    #[test]
    fn context_freed_right_after_the_parse() {
        if under_bsan() {
            return;
        }
        let tree = {
            let class = Box::new(class());
            let mut ctx = Box::new(Ctx { class: &*class });
            // SAFETY: as above, for this call only.
            let tree = unsafe {
                Expr::parse_with_context(c"2*3", &[], &[], &[], 0, (&raw mut *ctx).cast())
            };
            // SAFETY: as above; this parse fails and logs through `ctx`.
            let failed = unsafe {
                Expr::parse_with_context(c"2*", &[], &[], &[], 0, (&raw mut *ctx).cast())
            };
            assert_eq!(failed.err(), Some(EINVAL));
            tree.unwrap()
        };
        // The tree outlives the context and its class.
        drop(tree);
    }

    #[test]
    fn shared_context_on_many_threads() {
        if under_bsan() {
            return;
        }
        let class = class();
        let ctx = Ctx { class: &class };
        let addr = &raw const ctx as usize;
        thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    // SAFETY: the context and its class outlive the scope and
                    // are only read, which C does too.
                    unsafe { parses(addr as *mut c_void) };
                });
            }
        });
    }
}
