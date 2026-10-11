//! Safe wrappers over `libavutil/des.h`.
//!
//! [`Des`] is resource-free, so it can live inline or behind [`DesBox`].
//! Copies are independent contexts: an inline value is `Copy`, and a box
//! clones through `av_memdup`. C retains no pointer passed to
//! initialization. All three key schedules are initialized, including the
//! two inactive schedules in single-DES mode.
//! DES is obsolete and should not be used for new security protocols.

use core::ptr::{NonNull, addr_of, addr_of_mut};

use ffibox::{CNew, CZeroable, define_ctype};

use super::ffi;
use super::mem::{Heap, HeapBox};

// SAFETY: the generated handles match the public ffi::AVDES layout; raw
// fields are integers or fixed arrays, and borrow lifetimes govern access.
define_ctype!(
    /// Wraps: AVDES
    ///
    /// The public DES layout. Fields are reached through [`DesRef`] and
    /// [`DesMut`], never references to a C-owned object.
    ///
    /// Handles cannot outlive their owner:
    /// ```compile_fail,E0597
    /// use ffmpeg::libavutil::des::Des;
    /// let view;
    /// {
    ///     let owner = Des::new_boxed(&[0; 8], false).unwrap();
    ///     view = owner.as_ref();
    /// }
    /// let _ = view.round_keys();
    /// ```
    #[derive(Clone, Copy)]
    Des,
    DesRef,
    DesMut,
    ffi::AVDES
);

/// A C-allocated DES context, freed through the existing `av_free` policy.
/// Cloning copies the context into a fresh `av_memdup` block.
pub type DesBox = HeapBox<Des>;

// An all-zero context has single-DES mode and zero round keys, accepted by
// every accessor and C operation; the context owns no resources.
// SAFETY: zero initializes the integer flag and every fixed integer array.
unsafe impl CZeroable for Des {}

/// Wraps: av_des_alloc
// av_des_alloc returns a fresh, zero-filled AVDES allocation or NULL.
// SAFETY: zero is a valid Des; Heap frees its av_malloc-family storage.
unsafe impl CNew<Des> for Heap {
    fn c_new(&self) -> Option<NonNull<Des>> {
        // SAFETY: av_des_alloc takes no arguments and retains no Rust state.
        let raw = unsafe { ffi::av_des_alloc() };
        NonNull::new(raw.cast())
    }
}

impl Des {
    /// Initializes an inline context with an 8-byte DES or 24-byte triple-DES
    /// key. Returns `AVERROR(EINVAL)` (-22) for any other key length.
    /// `decrypt` is accepted as in C, but the current C implementation ignores it.
    pub fn new(key: &[u8], decrypt: bool) -> Result<Self, i32> {
        let mut context = Self::zeroed();
        context.as_mut().init(key, decrypt)?;
        Ok(context)
    }

    /// Allocates and initializes a context. Returns -22 for an invalid key
    /// length or `AVERROR(ENOMEM)` (-12) if C cannot allocate its storage.
    pub fn new_boxed(key: &[u8], decrypt: bool) -> Result<DesBox, i32> {
        key_bits(key.len())?;
        let mut context = DesBox::new().ok_or(-12)?;
        context.as_mut().init(key, decrypt)?;
        Ok(context)
    }
}

fn key_bits(len: usize) -> Result<i32, i32> {
    match len {
        8 => Ok(64),
        24 => Ok(192),
        _ => Err(-22),
    }
}

impl DesRef<'_> {
    /// Field: AVDES.triple_des
    ///
    /// The raw mode flag. C treats any nonzero value as triple-DES.
    pub fn triple_des(&self) -> i32 {
        // SAFETY: the shared handle addresses an initialized context; the
        // projection reads only its scalar flag and creates no reference.
        unsafe { addr_of!((*self.as_ptr()).triple_des).read() }
    }

    /// Field: AVDES.round_keys
    ///
    /// A copy of all three 16-round schedules, including inactive schedules.
    pub fn round_keys(&self) -> [[u64; 16]; 3] {
        // SAFETY: safe constructors initialize every schedule, and the
        // shared handle keeps the whole context alive and unwritten.
        unsafe { addr_of!((*self.as_ptr()).round_keys).read() }
    }
}

fn block_count(len: usize) -> Result<i32, i32> {
    if !len.is_multiple_of(8) {
        return Err(-22);
    }
    i32::try_from(len / 8).map_err(|_| -22)
}

impl DesRef<'_> {
    /// Wraps: av_des_crypt
    ///
    /// Encrypt or decrypt whole 8-byte blocks. `None` selects ECB; an IV
    /// selects CBC and is updated for the next call. Returns -22 without
    /// changing output or IV for unequal lengths, partial blocks, or a block
    /// count exceeding C's `int`. Unaligned byte buffers are supported by
    /// C's actual AV_RB64/AV_WB64 implementation despite the header wording.
    ///
    /// Partially overlapping input/output cannot be passed through safe Rust:
    /// ```compile_fail,E0502
    /// use ffmpeg::libavutil::des::Des;
    /// let context = Des::new(&[0; 8], false).unwrap();
    /// let mut bytes = [0; 24];
    /// context.as_ref().crypt(&mut bytes[8..], &bytes[..16], None, false).unwrap();
    /// ```
    pub fn crypt(
        &self,
        dst: &mut [u8],
        src: &[u8],
        iv: Option<&mut [u8; 8]>,
        decrypt: bool,
    ) -> Result<(), i32> {
        let count = block_count(src.len())?;
        if dst.len() != src.len() {
            return Err(-22);
        }
        if count == 0 {
            return Ok(());
        }
        let iv = iv.map_or(core::ptr::null_mut(), |iv| iv.as_mut_ptr());
        // SAFETY: the live context is only read by des_encdec and its callers,
        // even though C lacks const. Distinct slices supply count whole blocks;
        // the optional IV has eight exclusively borrowed bytes. C performs
        // unaligned reads/writes and retains no pointer or buffer reference.
        unsafe {
            ffi::av_des_crypt(
                self.as_ptr().cast_mut(),
                dst.as_mut_ptr(),
                src.as_ptr(),
                count,
                iv,
                i32::from(decrypt),
            );
        }
        Ok(())
    }

    /// Wraps: av_des_crypt
    ///
    /// The exact in-place variant of [`Self::crypt`], with the same validation.
    pub fn crypt_in_place(
        &self,
        data: &mut [u8],
        iv: Option<&mut [u8; 8]>,
        decrypt: bool,
    ) -> Result<(), i32> {
        let count = block_count(data.len())?;
        if count == 0 {
            return Ok(());
        }
        let iv = iv.map_or(core::ptr::null_mut(), |iv| iv.as_mut_ptr());
        let ptr = data.as_mut_ptr();
        // SAFETY: C reads each eight-byte block before overwriting it and
        // supports exact src/dst aliasing, including CBC decryption. The
        // context is read-only, data exclusively borrowed, IV disjoint and
        // exactly eight bytes. All lengths were checked; no pointer is kept.
        unsafe {
            ffi::av_des_crypt(
                self.as_ptr().cast_mut(),
                ptr,
                ptr,
                count,
                iv,
                i32::from(decrypt),
            );
        }
        Ok(())
    }

    /// Wraps: av_des_crypt
    ///
    /// Encrypt/decrypt zero blocks into `dst`. Uses real zero-filled input:
    /// C's advertised NULL input is defective for multiple blocks because
    /// it increments NULL and subsequently dereferences the resulting pointer.
    pub fn crypt_zeros(
        &self,
        dst: &mut [u8],
        iv: Option<&mut [u8; 8]>,
        decrypt: bool,
    ) -> Result<(), i32> {
        block_count(dst.len())?;
        dst.fill(0);
        self.crypt_in_place(dst, iv, decrypt)
    }

    /// Wraps: av_des_mac
    ///
    /// CBC-MAC of whole blocks. Empty input returns the initial all-zero MAC.
    /// The context is borrowed read-only; no input or result pointer is retained.
    pub fn mac(&self, src: &[u8]) -> Result<[u8; 8], i32> {
        let count = block_count(src.len())?;
        let mut mac = [0; 8];
        if count != 0 {
            // SAFETY: C reads the context without changing it, reads count
            // whole unaligned-safe blocks, and overwrites the same eight-byte
            // output on each iteration. The buffers cannot overlap and C
            // retains neither. Empty input bypasses C's pointer arithmetic.
            unsafe {
                ffi::av_des_mac(
                    self.as_ptr().cast_mut(),
                    mac.as_mut_ptr(),
                    src.as_ptr(),
                    count,
                )
            };
        }
        Ok(mac)
    }
}

impl DesMut<'_> {
    /// Wraps: av_des_init
    ///
    /// Reinitializes with an 8- or 24-byte key. Invalid lengths return -22
    /// without changing any field. In single-DES mode C leaves the other
    /// two schedules unchanged. The input is read only during this call.
    pub fn init(&mut self, key: &[u8], decrypt: bool) -> Result<(), i32> {
        let bits = key_bits(key.len())?;
        let mode = i32::from(decrypt);
        // SAFETY: the handle grants exclusive access to a live AVDES, and
        // bits is 64 or 192 with the corresponding 8 or 24 readable key
        // bytes. C reads the key during the call only and initializes the
        // selected schedules without changing the others.
        let result = unsafe { ffi::av_des_init(self.as_mut_ptr(), key.as_ptr(), bits, mode) };
        if result < 0 { Err(result) } else { Ok(()) }
    }

    /// Sets the raw mode flag; every integer value is valid for C.
    /// Shared and exclusive reborrows cannot overlap:
    /// ```compile_fail,E0502
    /// use ffmpeg::libavutil::des::Des;
    /// let mut owner = Des::new(&[0; 8], false).unwrap();
    /// let mut exclusive = owner.as_mut();
    /// let shared = exclusive.as_ref();
    /// exclusive.set_triple_des(1);
    /// let _ = shared.triple_des();
    /// ```
    /// Nor can two exclusive handles be used at once:
    /// ```compile_fail,E0499
    /// use ffmpeg::libavutil::des::Des;
    /// let mut owner = Des::new(&[0; 8], false).unwrap();
    /// let mut outer = owner.as_mut();
    /// let mut inner = outer.as_mut();
    /// outer.set_triple_des(1);
    /// inner.set_triple_des(0);
    /// ```
    pub fn set_triple_des(&mut self, flag: i32) {
        // SAFETY: the handle exclusively borrows an initialized AVDES; the
        // scalar field accepts any i32 and the projection creates no reference.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).triple_des).write(flag) }
    }

    /// Replaces all three schedules with fully initialized integer arrays.
    pub fn set_round_keys(&mut self, keys: [[u64; 16]; 3]) {
        // SAFETY: the handle grants exclusive access, the fixed array has
        // exactly the field's layout, and all u64 values are valid round keys.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).round_keys).write(keys) }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn crypt_validation_and_unchanged_buffers() {
        let context = Des::zeroed();
        let view = context.as_ref();
        let mut dst = [0xA5; 16];
        let mut iv = [0x35; 8];
        assert_eq!(
            view.crypt(&mut dst, &[0; 8], Some(&mut iv), false),
            Err(-22)
        );
        assert_eq!(
            view.crypt(&mut dst[..7], &[0; 7], Some(&mut iv), true),
            Err(-22)
        );
        assert_eq!(
            view.crypt_in_place(&mut dst[..15], Some(&mut iv), false),
            Err(-22)
        );
        assert_eq!(
            view.crypt_zeros(&mut dst[..15], Some(&mut iv), false),
            Err(-22)
        );
        assert_eq!(view.mac(&[0; 7]), Err(-22));
        assert_eq!(dst, [0xA5; 16]);
        assert_eq!(iv, [0x35; 8]);
        assert_eq!(view.crypt(&mut [], &[], Some(&mut iv), true), Ok(()));
        assert_eq!(view.crypt_in_place(&mut [], Some(&mut iv), false), Ok(()));
        assert_eq!(view.mac(&[]), Ok([0; 8]));
        assert_eq!(block_count(0), Ok(0));
        assert_eq!(block_count(7), Err(-22));
        if let Some(max_len) = (i32::MAX as usize).checked_mul(8) {
            assert_eq!(block_count(max_len), Ok(i32::MAX));
            assert_eq!(block_count(max_len + 8), Err(-22));
        }
    }

    #[test]
    fn invalid_lengths_leave_state_unchanged() {
        let mut value = Des::zeroed();
        value.as_mut().set_triple_des(i32::MIN);
        value.as_mut().set_round_keys([[u64::MAX; 16]; 3]);
        for length in [0, 1, 7, 9, 16, 23, 25, 256] {
            assert!(matches!(Des::new(&vec![0; length], false), Err(-22)));
            assert!(matches!(Des::new_boxed(&vec![0; length], false), Err(-22)));
            assert_eq!(value.as_mut().init(&vec![0; length], false), Err(-22));
            assert_eq!(value.as_ref().triple_des(), i32::MIN);
            assert_eq!(value.as_ref().round_keys(), [[u64::MAX; 16]; 3]);
        }
    }

    #[test]
    fn layout_matches_and_raw_adoption_roundtrips() {
        assert_eq!(
            core::mem::size_of::<Des>(),
            core::mem::size_of::<ffi::AVDES>()
        );
        assert_eq!(
            core::mem::align_of::<Des>(),
            core::mem::align_of::<ffi::AVDES>()
        );
        let owner = Des::new_boxed(&[0; 24], true).unwrap();
        let keys = owner.as_ref().round_keys();
        let raw = owner.into_c();
        // SAFETY: into_c surrendered exactly one live, initialized,
        // av_malloc-family allocation; no other owner or handle exists.
        let restored = unsafe { DesBox::from_c(raw) }.unwrap();
        assert_eq!(restored.as_ref().round_keys(), keys);
    }

    // No FFI: also run under Miri to check the by-value copy strategy.
    #[test]
    fn inline_copies_are_independent_contexts() {
        let mut original = Des::zeroed();
        original.as_mut().set_triple_des(1);
        original.as_mut().set_round_keys([[7; 16]; 3]);
        let copy = original;
        original.as_mut().set_triple_des(0);
        original.as_mut().set_round_keys([[9; 16]; 3]);
        assert_eq!(copy.as_ref().triple_des(), 1);
        assert_eq!(copy.as_ref().round_keys(), [[7; 16]; 3]);
        assert_eq!(original.as_ref().round_keys(), [[9; 16]; 3]);
    }

    #[test]
    fn boxed_clones_are_independent_allocations() {
        let mut original = Des::new_boxed(&[0x5A; 24], false).unwrap();
        let clone = original.clone();
        assert_ne!(clone.as_c_ptr(), original.as_c_ptr());
        assert_eq!(clone.as_ref().triple_des(), 1);
        assert_eq!(clone.as_ref().round_keys(), original.as_ref().round_keys());
        original.as_mut().init(&[0xC3; 8], true).unwrap();
        assert_eq!(original.as_ref().triple_des(), 0);
        assert_eq!(clone.as_ref().triple_des(), 1);
        assert_ne!(
            clone.as_ref().round_keys()[0],
            original.as_ref().round_keys()[0]
        );
        drop(original);
        let again = clone.clone();
        assert_eq!(again.as_ref().round_keys(), clone.as_ref().round_keys());
    }

    #[test]
    fn rust_only_zeroed_fields_and_reborrows() {
        let mut value = Des::zeroed();
        assert_eq!(value.as_ref().round_keys(), [[0; 16]; 3]);
        let mut exclusive = value.as_mut();
        exclusive.as_mut().set_round_keys([[u64::MAX; 16]; 3]);
        for flag in [0, 1, -1, i32::MIN, i32::MAX] {
            exclusive.as_mut().set_triple_des(flag);
            let shared = exclusive.as_ref();
            assert_eq!(shared.triple_des(), flag);
            assert_eq!(shared.round_keys(), [[u64::MAX; 16]; 3]);
        }
    }
}
