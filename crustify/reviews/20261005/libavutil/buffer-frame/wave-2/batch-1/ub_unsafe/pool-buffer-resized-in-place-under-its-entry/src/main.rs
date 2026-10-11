//! A pool whose allocator hands out resizable buffers lets safe code resize
//! a pool buffer in place, behind the pool entry that keeps recycling the
//! old block.
//!
//! `BufferPool::with_allocator`'s contract asks each allocator buffer to
//! hold at least `size` bytes from its start, which
//! `BufferReference::alloc_resizable(size)` does. C's `pool_alloc_buffer`
//! records that buffer's data in its `BufferPoolEntry` and takes over its
//! release, but leaves the buffer's `BUFFER_FLAG_REALLOCATABLE`. The safe
//! `UninitBuffer::resize` (as `BufferReferenceMut::resize`) then sees a sole,
//! writable, reallocatable buffer whose window starts at its data, and C's
//! `av_buffer_realloc` resizes it in place with `av_realloc`, freeing the
//! block the entry still records. The next `get` hands that freed block out
//! again (a use after free when it is filled), and the pool's release frees
//! it a second time.
//!
//! `cargo run` re-executes itself with `ASAN_OPTIONS=halt_on_error=1` and
//! exits with the child's status: non-zero, with an ASan report, on the
//! affected commit.

#![deny(unsafe_op_in_unsafe_fn)]

use ffmpeg::libavutil::buffer::{BufferPool, BufferReference};

const CHILD: &str = "CRUSTIFY_REPRODUCER_CHILD";

fn scenario() {
    // SAFETY: every buffer the allocator returns holds exactly `size` bytes
    // from its start (`alloc_resizable(size)` is a fresh buffer whose window
    // is its data), and neither the allocator nor the buffers' release
    // (`av_free`) calls into the pool.
    let pool = unsafe { BufferPool::with_allocator(16, BufferReference::alloc_resizable) }
        .expect("a pool");
    let mut first = pool.as_ref().get().expect("a pool buffer");
    // Safe: C reallocates the block in place, as the buffer is resizable.
    first.resize(1 << 20).expect("a resize");
    drop(first.fill(1));
    // The entry still records the 16-byte block `av_realloc` freed.
    let again = pool.as_ref().get().expect("a recycled buffer");
    eprintln!("recycled buffer of {} bytes; filling it", again.len());
    drop(again.fill(2));
    drop(pool);
    eprintln!("no report: the pool did not recycle a freed block");
}

fn main() {
    if std::env::var_os(CHILD).is_some() {
        scenario();
        return;
    }
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .env(CHILD, "1")
        .env("ASAN_OPTIONS", "halt_on_error=1:detect_leaks=1")
        .env("UBSAN_OPTIONS", "halt_on_error=1")
        .status()
        .unwrap();
    std::process::exit(status.code().unwrap_or(1));
}
