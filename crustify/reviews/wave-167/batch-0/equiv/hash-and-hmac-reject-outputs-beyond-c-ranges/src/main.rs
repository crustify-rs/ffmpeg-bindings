//! `hash::HashContextMut::finalize_{bin,hex,b64}` and `hmac::HmacMut::{finalize,
//! calculate}` against `av_hash_final_{bin,hex,b64}`, `av_hmac_final` and
//! `av_hmac_calc` with an output buffer longer than the C size parameter can
//! express (`int` for the hash finalizers, `unsigned int` for HMAC). A C caller
//! holding such a buffer passes the largest size it can (`INT_MAX` or
//! `UINT_MAX`) and C succeeds: it writes the digest (and, for `final_bin`,
//! zero padding up to that size). On the affected commit the wrappers failed
//! with `AVERROR(EINVAL)` because the slice length did not fit, and this
//! program exits 1. With the fix every comparison holds and it exits 0.
//!
//! The buffers are zeroed allocations: only the pages C touches are mapped
//! (`final_bin` pads, so it maps its whole 2 GiB).
use std::process::ExitCode;

use ffmpeg::libavutil::libavutil::{
    ffi,
    hash::HashContext,
    hmac::{Hmac, HmacType},
};

const INT_LEN: usize = i32::MAX as usize + 1;
const UINT_LEN: usize = u32::MAX as usize + 1;
const HEAD: usize = 160;

fn buffer(len: usize) -> Vec<u8> {
    let mut buffer = vec![0u8; len];
    buffer[..HEAD].fill(0xAA);
    buffer
}

fn main() -> ExitCode {
    let mut status = ExitCode::SUCCESS;
    let mut verdict = |what: &str, c: String, rust: String| {
        let agree = c == rust;
        if !agree {
            status = ExitCode::FAILURE;
        }
        let verdict = if agree { "agree" } else { "DIFFER" };
        println!("{what}:\n  C    {c}\n  Rust {rust}\n  {verdict}");
    };

    for format in ["bin", "hex", "b64"] {
        let mut context = core::ptr::null_mut();
        // SAFETY: a writable output slot and a NUL-terminated name; C hands
        // over a fresh context, which is initialized and freed below.
        assert_eq!(unsafe { ffi::av_hash_alloc(&mut context, c"SHA256".as_ptr()) }, 0);
        let mut c_out = buffer(INT_LEN);
        // SAFETY: the context is live and initialized; `c_out` is writable for
        // `INT_MAX` bytes and C writes at most that many. Freed exactly once.
        unsafe {
            ffi::av_hash_init(context);
            ffi::av_hash_update(context, b"crustify".as_ptr(), 8);
            match format {
                "bin" => ffi::av_hash_final_bin(context, c_out.as_mut_ptr(), i32::MAX),
                "hex" => ffi::av_hash_final_hex(context, c_out.as_mut_ptr(), i32::MAX),
                _ => ffi::av_hash_final_b64(context, c_out.as_mut_ptr(), i32::MAX),
            }
            ffi::av_hash_freep(&mut context);
        }
        let c = format!("Ok(()) {:02x?}", &c_out[..HEAD]);
        drop(c_out);

        let mut safe = HashContext::new(c"SHA256").unwrap();
        safe.as_mut().update(b"crustify");
        let mut rust_out = buffer(INT_LEN);
        // Debug text, so this builds on both sides of the fix: `finalize_bin`
        // returned `Result<(), i32>` on the affected commit and `()` after it,
        // since it can no longer fail.
        let result = match format {
            "bin" => format!("{:?}", safe.as_mut().finalize_bin(&mut rust_out)),
            "hex" => format!("{:?}", safe.as_mut().finalize_hex(&mut rust_out)),
            _ => format!("{:?}", safe.as_mut().finalize_b64(&mut rust_out)),
        };
        let result = if result == "()" { "Ok(())".to_owned() } else { result };
        let rust = format!("{result} {:02x?}", &rust_out[..HEAD]);
        drop(rust_out);
        verdict(&format!("SHA256 final_{format} into 2^31 bytes"), c, rust);
    }

    for calculate in [false, true] {
        let key = b"key";
        // SAFETY: C returns a fresh context or NULL (asserted); it is keyed,
        // fed and finalized with live buffers, then freed exactly once.
        // `c_out` is writable for `UINT_MAX` bytes; C writes 32 of them.
        let (ret, c_out) = unsafe {
            let context = ffi::av_hmac_alloc(ffi::AVHMACType::AV_HMAC_SHA256);
            assert!(!context.is_null());
            ffi::av_hmac_init(context, key.as_ptr(), 3);
            let mut c_out = buffer(UINT_LEN);
            let ret = if calculate {
                ffi::av_hmac_calc(
                    context,
                    b"crustify".as_ptr(),
                    8,
                    key.as_ptr(),
                    3,
                    c_out.as_mut_ptr(),
                    u32::MAX,
                )
            } else {
                ffi::av_hmac_update(context, b"crustify".as_ptr(), 8);
                ffi::av_hmac_final(context, c_out.as_mut_ptr(), u32::MAX)
            };
            ffi::av_hmac_free(context);
            (ret, c_out)
        };
        let c = format!("Ok({ret}) {:02x?}", &c_out[..HEAD]);
        drop(c_out);

        let mut safe = Hmac::new(HmacType::SHA256, key).unwrap();
        let mut rust_out = buffer(UINT_LEN);
        let result = if calculate {
            safe.as_mut().calculate(b"crustify", key, &mut rust_out)
        } else {
            safe.as_mut().update(b"crustify").unwrap();
            safe.as_mut().finalize(&mut rust_out)
        };
        let rust = format!("{result:?} {:02x?}", &rust_out[..HEAD]);
        drop(rust_out);
        let what = if calculate { "calc" } else { "final" };
        verdict(&format!("HMAC-SHA256 {what} into 2^32 bytes"), c, rust);
    }
    status
}
