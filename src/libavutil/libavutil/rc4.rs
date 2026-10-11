//! Safe layouts and lifecycle for `libavutil/rc4.h`.
//!
//! RC4 is obsolete and must not be used for new security-sensitive protocols.
//! The context owns no resources: it may live inline or in an [`Rc4Box`].

use core::ptr::{NonNull, addr_of, addr_of_mut};

use ffibox::{CBox, CNew, CSlice, CSliceMut, CZeroable, define_ctype};

use super::{ffi, mem::Heap};

define_ctype!(
    /// Wraps: AVRC4
    ///
    /// Resource-free RC4 state. Copies are independent stream checkpoints.
    #[derive(Clone, Copy)]
    Rc4, Rc4Ref, Rc4Mut, ffi::AVRC4
);

/// A sole C-allocated RC4 context, released by the existing [`Heap`] policy.
/// No thread-safety assertion is made for the wrapped C type:
///
/// ```compile_fail,E0277
/// use ffmpeg::libavutil::rc4::Rc4Box;
/// fn requires_send<T: Send>() {}
/// requires_send::<Rc4Box>();
/// ```
pub type Rc4Box = CBox<Rc4, Heap>;

// SAFETY: every field is an integer or byte array, and all-zero is accepted
// by every accessor and C operation. `av_rc4_crypt` truncates x/y to bytes;
// no permutation invariant is required for memory safety. Heap frees only storage.
unsafe impl CZeroable for Rc4 {}

/// Wraps: av_rc4_alloc
// SAFETY: av_rc4_alloc returns a fresh, zero-filled AVRC4 (or NULL), with
// exactly Rc4's layout; Heap releases that av_malloc-family allocation.
unsafe impl CNew<Rc4> for Heap {
    fn c_new(&self) -> Option<NonNull<Rc4>> {
        // SAFETY: the allocator has no arguments or preconditions.
        NonNull::new(unsafe { ffi::av_rc4_alloc() }.cast())
    }
}

impl Rc4 {
    /// Allocate and initialize an RC4 stream. Errors are negative AVERROR codes:
    /// EINVAL for an empty/oversized key, ENOMEM if allocation fails.
    /// Encryption and decryption use identical initialization.
    pub fn new(key: &[u8]) -> Result<Rc4Box, i32> {
        key_bits(key.len())?;
        let mut context = Rc4Box::new().ok_or(-12)?;
        context.as_mut().init(key)?;
        Ok(context)
    }

    /// Initialize resource-free, Rust-owned inline storage.
    pub fn from_key(key: &[u8]) -> Result<Self, i32> {
        let mut context = Self::zeroed();
        context.as_mut().init(key)?;
        Ok(context)
    }
}

fn key_bits(len: usize) -> Result<i32, i32> {
    if len == 0 || len > (i32::MAX / 8) as usize {
        return Err(-22);
    }
    Ok((len * 8) as i32)
}

impl<'a> Rc4Ref<'a> {
    /// Field: AVRC4.x
    pub fn x(&self) -> i32 {
        // SAFETY: the shared handle keeps this initialized integer alive.
        unsafe { addr_of!((*self.as_ptr()).x).read() }
    }

    /// Field: AVRC4.y
    pub fn y(&self) -> i32 {
        // SAFETY: the shared handle keeps this initialized integer alive.
        unsafe { addr_of!((*self.as_ptr()).y).read() }
    }

    /// Field: AVRC4.state
    ///
    /// A shared view of all 256 bytes, tied to the context borrow rather than a
    /// Rust reference over C-owned memory.
    pub fn state(&self) -> CSlice<'a, u8> {
        // SAFETY: state is an initialized inline 256-byte array in the live
        // context. Its projected pointer is non-null; the view cannot outlive
        // the context borrow or overlap any safe mutable access.
        unsafe {
            let ptr = addr_of!((*self.as_ptr()).state).cast::<u8>().cast_mut();
            CSlice::from_raw_parts(NonNull::new_unchecked(ptr), 256)
        }
    }
}

impl Rc4Mut<'_> {
    /// Wraps: av_rc4_init
    ///
    /// Reset the stream with a nonempty byte key. Rejects empty keys (C
    /// reads past them) and lengths whose bit count cannot fit a C int,
    /// leaving the old state untouched. The key is not retained.
    pub fn init(&mut self, key: &[u8]) -> Result<(), i32> {
        let bits = key_bits(key.len())?;
        // SAFETY: this handle is exclusive; C writes the complete context,
        // reads only within the nonempty key (at most its first 256 bytes),
        // and retains neither pointer. bits is a positive multiple of eight.
        let result = unsafe { ffi::av_rc4_init(self.as_mut_ptr(), key.as_ptr(), bits, 0) };
        if result < 0 { Err(result) } else { Ok(()) }
    }

    /// Set the C index verbatim; C truncates it to a byte when used.
    pub fn set_x(&mut self, value: i32) {
        // SAFETY: the exclusive handle permits writing this initialized field;
        // every i32 is valid and C indexes with its byte-sized truncation.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).x).write(value) }
    }

    /// Set the C index verbatim; C truncates it to a byte when used.
    pub fn set_y(&mut self, value: i32) {
        // SAFETY: as set_x, for the y field.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).y).write(value) }
    }

    /// Exclusively reborrow the 256 state bytes; arbitrary byte values are
    /// memory-safe, though changing them changes the stream algorithm.
    pub fn state_mut(&mut self) -> CSliceMut<'_, u8> {
        // SAFETY: state is an initialized inline 256-byte array; this handle
        // grants exclusive access, and the view's lifetime freezes the handle
        // until the reborrow ends. No reference covers the C array.
        unsafe {
            let ptr = addr_of_mut!((*self.as_mut_ptr()).state).cast::<u8>();
            CSliceMut::from_raw_parts(NonNull::new_unchecked(ptr), 256)
        }
    }

    /// Replace all state bytes without retaining the input.
    pub fn set_state(&mut self, state: &[u8; 256]) {
        // SAFETY: the exclusive handle permits overwriting this array;
        // all byte values are valid and reading state copies a Rust array.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).state).write(*state) }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn key_length_boundaries() {
        assert_eq!(key_bits(0), Err(-22));
        assert_eq!(key_bits(1), Ok(8));
        let max = (i32::MAX / 8) as usize;
        assert_eq!(key_bits(max), Ok(i32::MAX & !7));
        assert_eq!(key_bits(max + 1), Err(-22));
        assert_eq!(key_bits(usize::MAX), Err(-22));
    }

    #[test]
    fn inline_layout_and_copy_are_ffi_free() {
        assert_eq!(
            core::mem::size_of::<Rc4>(),
            core::mem::size_of::<ffi::AVRC4>()
        );
        assert_eq!(
            core::mem::align_of::<Rc4>(),
            core::mem::align_of::<ffi::AVRC4>()
        );
        let mut context = Rc4::zeroed();
        context.as_mut().set_x(i32::MIN);
        context.as_mut().set_y(i32::MAX);
        context.as_mut().set_state(&[7; 256]);
        let copy = context;
        assert!(context.as_mut().state_mut().set_elem(255, 9));
        assert_eq!(copy.as_ref().x(), i32::MIN);
        assert_eq!(copy.as_ref().y(), i32::MAX);
        assert_eq!(copy.as_ref().state().elem(255), Some(7));
        assert_eq!(context.as_ref().state().elem(255), Some(9));
    }

    #[test]
    fn raw_adoption_round_trip_uses_the_same_heap_policy() {
        let original = Rc4::new(b"key").unwrap();
        let ptr = original.into_c();
        // SAFETY: into_c transfers this initialized, uniquely owned allocation;
        // no intervening access occurred, and Heap releases it through av_free.
        let owner = unsafe { Rc4Box::from_c(ptr) }.unwrap();
        assert_eq!(owner.as_ref().x(), 1);
    }

    #[test]
    fn empty_key_is_rejected_without_mutating_state() {
        let mut context = Rc4::zeroed();
        context.as_mut().set_x(42);
        assert_eq!(context.as_mut().init(&[]), Err(-22));
        assert_eq!(context.as_ref().x(), 42);
        assert!(Rc4::new(&[]).is_err());
        assert!(Rc4::from_key(&[]).is_err());
    }
}
