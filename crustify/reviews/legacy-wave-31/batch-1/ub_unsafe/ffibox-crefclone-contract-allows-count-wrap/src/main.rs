//! ffibox's `CRefClone` contract does not forbid a count that wraps: a policy
//! that meets it as written, bound with `impl_crefclone!` to an unchecked
//! up-ref (as RefStruct's `av_refstruct_ref` and `AVBuffer`'s refcount are),
//! lets safe code free a live object by leaking clones.
//!
//! The "C" object is written in Rust so that Miri can run it; its count is
//! eight bits wide so that the wrap takes 255 leaked clones instead of 2^32
//! (a 32-bit `uintptr_t`) or 2^64.

#![deny(unsafe_op_in_unsafe_fn)]

use core::ptr::addr_of;

use ffibox::{CArc, define_ctype, impl_cdrop, impl_crefclone};

mod sys {
    use core::sync::atomic::{AtomicU8, Ordering};

    /// A refcounted C object; the count is incremented unchecked.
    #[repr(C)]
    pub struct obj_st {
        pub count: AtomicU8,
        pub value: u64,
    }

    pub fn obj_new(value: u64) -> *mut obj_st {
        Box::into_raw(Box::new(obj_st {
            count: AtomicU8::new(1),
            value,
        }))
    }

    /// Adds one counted reference to the live object `o`; cannot fail.
    ///
    /// # Safety
    /// `o` is a live object whose reference the caller holds.
    pub unsafe fn obj_up_ref(o: *mut obj_st) {
        // SAFETY: per the contract, `o` is live.
        unsafe { (*o).count.fetch_add(1, Ordering::Relaxed) };
    }

    /// Releases one counted reference; the last one frees the object.
    ///
    /// # Safety
    /// `o` is a live object whose reference the caller hands over.
    pub unsafe fn obj_unref(o: *mut obj_st) {
        // SAFETY: per the contract, `o` is live.
        if unsafe { (*o).count.fetch_sub(1, Ordering::AcqRel) } == 1 {
            // SAFETY: the last reference to a `Box` allocation.
            drop(unsafe { Box::from_raw(o) });
        }
    }
}

define_ctype!(Obj, ObjRef, ObjMut, sys::obj_st);

impl ObjRef<'_> {
    pub fn value(&self) -> u64 {
        // SAFETY: a read of a live object's field through the handle.
        unsafe { addr_of!((*self.as_ptr()).value).read() }
    }
}

/// The down-ref and the up-ref on one policy: everything the documented
/// contract asks holds. `obj_up_ref` increments the count of a live object
/// and changes nothing else; `obj_unref` releases one reference.
#[derive(Clone, Copy, Debug, Default)]
pub struct ObjUnref;
impl_cdrop!(ObjUnref, Obj, sys::obj_unref);
impl_crefclone!(ObjUnref, Obj, sys::obj_up_ref);

fn main() {
    // SAFETY: a fresh object holding one counted reference, which the arc adopts.
    let a = unsafe { CArc::<Obj, ObjUnref>::from_c(sys::obj_new(42)) }.unwrap();
    // Everything below is safe code.
    for _ in 0..255 {
        core::mem::forget(a.clone()); // the count wraps from 255 to 0
    }
    drop(a.clone()); // 0 -> 1 -> 0: the object is freed while `a` holds it
    println!("value = {}", a.as_ref().value()); // use after free
}
