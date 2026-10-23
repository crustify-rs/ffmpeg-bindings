//! `base64::encode` and `base64::decode` against `av_base64_encode` and
//! `av_base64_decode` with an output buffer one byte longer than `INT_MAX`.
//! C takes the output size as an `int`, so a C caller with such a buffer
//! passes `INT_MAX` and C succeeds: it writes only the bytes the data needs.
//! On the affected commit the wrappers failed with `AVERROR(EINVAL)` because
//! the buffer's length did not fit an `int`, and this program exits 1 at the
//! first mismatch. With the fix every comparison holds and it exits 0.
//!
//! The 2 GiB buffers are zeroed allocations: only the pages the codecs touch
//! are ever mapped.
use core::ffi::CStr;
use std::process::ExitCode;

use ffmpeg::libavutil::libavutil::{base64, ffi};

const LEN: usize = i32::MAX as usize + 1;

fn main() -> ExitCode {
    let mut status = ExitCode::SUCCESS;
    let mut verdict = |what: &str, c: String, rust: String| {
        let agree = c == rust;
        if !agree {
            status = ExitCode::FAILURE;
        }
        let verdict = if agree { "agree" } else { "DIFFER" };
        println!("{what}: C {c}, Rust {rust}: {verdict}");
    };

    let input = b"crustify";
    let mut c_out = vec![0u8; LEN];
    // SAFETY: `c_out` is writable for `INT_MAX` bytes, `input` readable for
    // its 8 bytes; C writes the 13 encoded bytes and retains neither pointer.
    let ptr = unsafe {
        ffi::av_base64_encode(
            c_out.as_mut_ptr().cast(),
            i32::MAX,
            input.as_ptr(),
            input.len() as i32,
        )
    };
    let c = if ptr.is_null() {
        "NULL".to_owned()
    } else {
        // SAFETY: on success C returns `c_out`, now holding a NUL-terminated
        // string within the buffer.
        format!("{:?}", unsafe { CStr::from_ptr(ptr) })
    };
    drop(c_out);
    let mut rust_out = vec![0u8; LEN];
    let rust = match base64::encode(input, &mut rust_out) {
        Ok(text) => format!("{text:?}"),
        Err(e) => format!("Err({e})"),
    };
    drop(rust_out);
    verdict("encode b\"crustify\" into 2^31 bytes", c, rust);

    let text = c"Y3J1c3RpZnk=";
    let mut c_out = vec![0u8; LEN];
    // SAFETY: `text` is NUL-terminated and `c_out` writable for `INT_MAX`
    // bytes; C writes the 8 decoded bytes and retains neither pointer.
    let ret = unsafe { ffi::av_base64_decode(c_out.as_mut_ptr(), text.as_ptr(), i32::MAX) };
    let c = if ret < 0 {
        format!("Err({ret})")
    } else {
        format!("Ok({ret}) {:?}", &c_out[..ret as usize])
    };
    drop(c_out);
    let mut rust_out = vec![0u8; LEN];
    let rust = match base64::decode(text, &mut rust_out) {
        Ok(n) => format!("Ok({n}) {:?}", &rust_out[..n]),
        Err(e) => format!("Err({e})"),
    };
    verdict("decode c\"Y3J1c3RpZnk=\" into 2^31 bytes", c, rust);
    status
}
