//! `default_item_name` on memory that satisfies the wrapper's stated contract
//! as written ("readable memory that starts with a non-NULL `const AVClass *`
//! to a live class") but is not aligned for a pointer: C reads the class
//! pointer through `*(AVClass **) ptr`, a misaligned load, which UBSan's
//! alignment check reports inside `av_default_item_name`.
//!
//! Run against the `asan-ubsan` library:
//!
//! ```console
//! CRUSTIFY_FFMPEG_VARIANT=asan-ubsan RUSTFLAGS=-Zsanitizer=address \
//!   cargo +nightly run -Zbuild-std --target x86_64-unknown-linux-gnu --offline
//! ```
//!
//! UBSan recovers by default, so the program re-runs itself with
//! `halt_on_error=1` and exits with the child's status: non-zero on the
//! affected commit.

use core::ffi::c_void;
use std::process::Command;

use ffmpeg::libavutil::libavutil::ffi;
use ffmpeg::libavutil::libavutil::log::default_item_name;

/// A 16-byte buffer aligned for a pointer, so offset 1 is not.
#[repr(C, align(8))]
struct Buf([u8; 16]);

fn main() {
    if std::env::var_os("UBSAN_OPTIONS").is_none() {
        let status = Command::new(std::env::current_exe().unwrap())
            .env("UBSAN_OPTIONS", "halt_on_error=1:print_stacktrace=1")
            .status()
            .unwrap();
        std::process::exit(status.code().unwrap_or(1));
    }

    let class = ffi::AVClass {
        class_name: c"misaligned-object".as_ptr(),
        ..Default::default()
    };
    let class_ptr: *const ffi::AVClass = &class;

    let mut buf = Buf([0; 16]);
    // The object's first member, a pointer to the live class, at offset 1.
    let slot = buf
        .0
        .as_mut_ptr()
        .wrapping_add(1)
        .cast::<*const ffi::AVClass>();
    // SAFETY: `slot` addresses bytes 1..9 of `buf`, inside the buffer.
    unsafe { slot.write_unaligned(class_ptr) };
    println!(
        "object at {slot:p}, which is not aligned to {}",
        align_of::<*const ffi::AVClass>()
    );

    // Satisfies the stated contract as written: readable memory starting with
    // a non-NULL pointer to a live class whose name is a literal, and no
    // other thread writes either.
    // SAFETY: as the contract reads; see above.
    let name = unsafe { default_item_name(slot.cast::<c_void>()) };
    println!("name = {name:?}");
}
