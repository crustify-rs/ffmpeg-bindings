//! `CVec::clone` / `try_clone` copy through `CLenClone::c_clone_len` without
//! checking `CLenClone::ALIGN`, which `from_slice` and `to_cvec` do check.
//!
//! Every `unsafe` below honours its contract: `Bytes` is an honest byte
//! allocator that vouches for byte alignment only (the default `ALIGN = 1`),
//! and the adopted buffer is a properly aligned `u32` array from it. The UB is
//! reached by the safe `clone()` + `as_slice()`.
//!
//! Run: `cargo +nightly miri run` (Miri), or `cargo run`, whose debug build
//! trips the standard library's `slice::from_raw_parts` precondition check.

use core::ptr::NonNull;
use std::alloc::{Layout, alloc, dealloc};

use ffibox::{CLenClone, CLenDrop, CVec};

/// Bytes before a block's start: its length at the base, its offset byte
/// right before the start.
const HEADER: usize = 16;

/// Allocate `byte_len` bytes starting `offset` (< 16) bytes past a 16-aligned
/// base, recording what `free` needs to find the base again.
fn allocate(byte_len: usize, offset: usize) -> NonNull<u8> {
    let layout = Layout::from_size_align(HEADER + offset + byte_len, 16).unwrap();
    // SAFETY: `layout` has a non-zero size.
    let base = unsafe { alloc(layout) };
    assert!(!base.is_null());
    // SAFETY: the header's first 8 bytes and the byte before the start lie in
    // the block, and do not overlap for `offset < 8`.
    unsafe {
        base.cast::<usize>().write(byte_len);
        base.add(HEADER + offset - 1).write(offset as u8);
        NonNull::new_unchecked(base.add(HEADER + offset))
    }
}

/// Release a block `allocate` returned.
///
/// # Safety
///
/// `ptr` must be a live block from `allocate`.
unsafe fn free(ptr: *mut u8) {
    // SAFETY: `allocate` wrote the offset right before `ptr`, and the length
    // at the base it addresses.
    unsafe {
        let offset = ptr.sub(1).read() as usize;
        let base = ptr.sub(HEADER + offset);
        let byte_len = base.cast::<usize>().read();
        dealloc(base, Layout::from_size_align(HEADER + offset + byte_len, 16).unwrap());
    }
}

/// A byte allocator whose copies start at an odd address, which byte
/// alignment allows.
#[derive(Clone, Copy, Debug, Default)]
struct Bytes;

// SAFETY: releases an `allocate` block once, whatever its length.
unsafe impl CLenDrop for Bytes {
    unsafe fn c_drop_len(&self, ptr: *mut u8, _byte_len: usize) {
        // SAFETY: the caller hands over a live block it owns.
        unsafe { free(ptr) }
    }
}

// SAFETY: a fresh, uniquely-owned `allocate` block of `byte_len` bytes, a
// byte copy of `ptr`, released by `c_drop_len`; aligned to 1, the default
// `ALIGN` this impl leaves in place.
unsafe impl CLenClone for Bytes {
    unsafe fn c_clone_len(&self, ptr: *mut u8, byte_len: usize) -> Option<NonNull<u8>> {
        let copy = allocate(byte_len, 1);
        // SAFETY: the caller guarantees `byte_len` readable bytes at `ptr`;
        // `copy` is a fresh block of that many.
        unsafe { core::ptr::copy_nonoverlapping(ptr, copy.as_ptr(), byte_len) };
        Some(copy)
    }
}

fn main() {
    // Four `u32` at a 16-aligned start, from the allocator `Bytes` frees.
    let block = allocate(16, 0);
    // SAFETY: `block` is 16 writable bytes aligned to 16.
    unsafe { block.cast::<[u32; 4]>().write([1, 2, 3, 4]) };
    // SAFETY: `block` holds 4 initialised, aligned `u32` from the allocator
    // `Bytes` releases, `byte_len` equals its size, and ownership moves in.
    let v = unsafe { CVec::<u32, Bytes>::from_raw_parts(block.as_ptr().cast(), 4) }.unwrap();
    assert_eq!(v.as_slice(), &[1, 2, 3, 4]);

    // Safe from here on. `from_slice` would refuse `u32` under `Bytes` at
    // compile time; `clone` does not.
    let copy = v.clone();
    println!("copy at {:p}, align_of::<u32>() = 4", copy.as_ptr());
    let s: &[u32] = copy.as_slice(); // a misaligned `&[u32]`: UB
    println!("{s:?}");
}
