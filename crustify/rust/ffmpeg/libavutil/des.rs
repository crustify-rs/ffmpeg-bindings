//! Safe wrappers over `libavutil/des.h`.
//!
//! [`Des`] is resource-free, so it can live inline or behind [`DesBox`].
//! C retains no pointer passed to initialization. All three key schedules
//! are initialized, including the two inactive schedules in single-DES mode.
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
    Des,
    DesRef,
    DesMut,
    ffi::AVDES
);

/// A C-allocated DES context, freed through the existing `av_free` policy.
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

impl DesMut<'_> {
    /// Wraps: av_des_init
    ///
    /// Reinitializes with an 8- or 24-byte key. Invalid lengths return -22
    /// without changing any field. In single-DES mode C leaves the other
    /// two schedules unchanged. The input is read only during this call.
    pub fn init(&mut self, key: &[u8], decrypt: bool) -> Result<(), i32> {
        let bits = key_bits(key.len())?;
        // SAFETY: the handle grants exclusive access to a live AVDES, and
        // bits is 64 or 192 with the corresponding 8 or 24 readable key
        // bytes. C reads the key during the call only and initializes the
        // selected schedules without changing the others.
        let result =
            unsafe { ffi::av_des_init(self.as_mut_ptr(), key.as_ptr(), bits, i32::from(decrypt)) };
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
