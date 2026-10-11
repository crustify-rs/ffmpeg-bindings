//! Safe wrappers over `libavutil/refstruct.h`.
//!
//! [`RefStructOpaque`] is `AVRefStructOpaque`, the union of a `void *` and a
//! `const void *` that carries a user's pointer to the RefStruct callbacks.
//! Both members are one pointer, so it is a plain `Copy` value: a cookie C
//! stores and hands back, never dereferences and never frees.
//!
//! [`RefStructPool`] is the layout type of `AVRefStructPool`, a thread-safe
//! pool of RefStruct objects whose body is private to `refstruct.c`. The pool
//! is itself a RefStruct object with a counted reference, so its owner is the
//! shared [`RefStructPoolArc`]: [`RefStructPool::new`] (`av_refstruct_pool_alloc`)
//! makes the first reference, `Clone` adds one (`av_refstruct_ref`) and drop
//! releases one (`av_refstruct_pool_uninit`). Entries handed out by the pool
//! keep it alive on their own, so C frees it only after the last reference
//! and the last outstanding entry are both gone.

use core::ffi::{c_uint, c_void};
use core::fmt;
use core::hash::{Hash, Hasher};
use core::ops::{BitOr, BitOrAssign};
use core::ptr::{NonNull, addr_of, addr_of_mut};

use ffibox::{CArc, CDrop, CRefClone, CZeroable, define_ctype};

use super::ffi;

define_ctype!(
    /// Wraps: AVRefStructOpaque
    ///
    /// The user pointer a RefStruct object or pool passes to its callbacks:
    /// C's union of `void *nc` and `const void *c`. Both members are the same
    /// pointer-sized bytes, so either reads what the other last wrote, and
    /// the two only differ in the constness the C caller had in hand. FFmpeg
    /// copies it by value, never dereferences it and never frees what it
    /// points at, so it owns nothing and copies as a value; what it points
    /// at and how long that lives are the user's concern.
    ///
    /// Build one with [`new`](Self::new) (C's `{ .nc = opaque }`, which
    /// `av_refstruct_alloc_ext` and `av_refstruct_pool_alloc_ext` build),
    /// [`from_const`](Self::from_const) (`{ .c = opaque }`) or
    /// [`default`](Self::default) (NULL). Its getters are on
    /// [`RefStructOpaqueRef`] (`as_ref()`), its setters on
    /// [`RefStructOpaqueMut`] (`as_mut()`). Equality and hashing compare the
    /// address.
    #[derive(Clone, Copy)]
    RefStructOpaque,
    RefStructOpaqueRef,
    RefStructOpaqueMut,
    ffi::AVRefStructOpaque
);

// SAFETY: all-zero is the NULL pointer in both members, the
// `(AVRefStructOpaque){.nc = NULL}` that `av_refstruct_allocz` and
// `av_refstruct_pool_alloc` pass; every accessor copies the pointer out
// without dereferencing it, and the union has no teardown to reject it.
unsafe impl CZeroable for RefStructOpaque {}

// SAFETY: an `AVRefStructOpaque` is one pointer that neither FFmpeg (it only
// copies it into the object's header and back to the user's callbacks) nor
// this crate dereferences or frees, so a value, or an exclusive handle to
// one, moves between threads as a `usize` does; whoever dereferences the
// pointer does so in their own `unsafe` code, on their own terms.
unsafe impl Send for RefStructOpaque {}
// SAFETY: as for `RefStructOpaque`: the handle is the only access to the
// bytes.
unsafe impl Send for RefStructOpaqueMut<'_> {}

// SAFETY: nothing reachable through a shared handle writes:
// `RefStructOpaqueRef` has getters only, which copy the pointer out, and the
// union has no interior mutability, so concurrent shared access reads
// unchanging bytes.
unsafe impl Sync for RefStructOpaque {}
// SAFETY: as for `Sync for RefStructOpaque`: copies of a shared handle only
// read.
unsafe impl Send for RefStructOpaqueRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for RefStructOpaqueRef<'_> {}
// SAFETY: `&RefStructOpaqueMut` reaches the bytes only through `as_ref`, a
// shared handle; the setters need `&mut`.
unsafe impl Sync for RefStructOpaqueMut<'_> {}

impl RefStructOpaque {
    /// The opaque holding `opaque` in its non-const member, as C's
    /// `(AVRefStructOpaque){ .nc = opaque }`.
    #[inline]
    #[must_use]
    pub fn new(opaque: *mut c_void) -> Self {
        let mut v = Self::zeroed();
        v.as_mut().set_mut_ptr(opaque);
        v
    }

    /// The opaque holding `opaque` in its const member, as C's
    /// `(AVRefStructOpaque){ .c = opaque }`.
    #[inline]
    #[must_use]
    pub fn from_const(opaque: *const c_void) -> Self {
        let mut v = Self::zeroed();
        v.as_mut().set_const_ptr(opaque);
        v
    }

    /// The opaque C passed as `raw`, whatever it holds.
    #[inline]
    #[must_use]
    pub const fn from_raw(raw: ffi::AVRefStructOpaque) -> Self {
        Self(raw, core::marker::PhantomData)
    }

    /// The C encoding of this opaque, for passing it to C by value.
    #[inline]
    #[must_use]
    pub const fn into_raw(self) -> ffi::AVRefStructOpaque {
        self.0
    }
}

impl Default for RefStructOpaque {
    /// The NULL opaque, which C passes when the user gives none.
    #[inline]
    fn default() -> Self {
        Self::zeroed()
    }
}

impl PartialEq for RefStructOpaque {
    /// The same address, whichever member stored it.
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.as_ref().const_ptr() == other.as_ref().const_ptr()
    }
}

impl Eq for RefStructOpaque {}

impl Hash for RefStructOpaque {
    /// Over the address [`PartialEq`] compares.
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().const_ptr().hash(state);
    }
}

impl fmt::Debug for RefStructOpaque {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl fmt::Debug for RefStructOpaqueRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("RefStructOpaque")
            .field(&self.const_ptr())
            .finish()
    }
}

impl RefStructOpaqueRef<'_> {
    /// Field: AVRefStructOpaque.nc
    ///
    /// The user pointer, as the non-const member reads it: the same address
    /// [`const_ptr`](Self::const_ptr) reads, NULL included. FFmpeg never
    /// reads what it points at, so it is handed back as the raw cookie it
    /// is, and dereferencing it is the caller's `unsafe` business.
    #[inline]
    #[must_use]
    pub fn mut_ptr(&self) -> *mut c_void {
        // SAFETY: a read of the `nc` member projected through the shared
        // handle's pointer to a live, initialised `AVRefStructOpaque`. Both
        // members span the same 8 bytes and either one's bytes are a valid
        // pointer of the other's type; the pointer is copied, not
        // dereferenced.
        unsafe { addr_of!((*self.as_ptr()).nc).read() }
    }

    /// Field: AVRefStructOpaque.c
    ///
    /// The user pointer, as the const member reads it: the same address
    /// [`mut_ptr`](Self::mut_ptr) reads, NULL included.
    #[inline]
    #[must_use]
    pub fn const_ptr(&self) -> *const c_void {
        // SAFETY: a read of the `c` member projected through the shared
        // handle's pointer to a live, initialised `AVRefStructOpaque`; as
        // for `nc`, any bytes of either member are a valid `*const c_void`,
        // copied, not dereferenced.
        unsafe { addr_of!((*self.as_ptr()).c).read() }
    }
}

impl RefStructOpaqueMut<'_> {
    /// Field: AVRefStructOpaque.nc
    ///
    /// Stores the user pointer `opaque`, NULL included, through the non-const
    /// member; [`const_ptr`](RefStructOpaqueRef::const_ptr) then reads the
    /// same address. Nothing in FFmpeg or this crate dereferences or frees
    /// it, so any address is safe to store: keeping what it points at alive
    /// for as long as a callback may receive it is the caller's concern.
    // The lint reads the write of `opaque` as a dereference of it: the
    // pointer is the value stored, through the handle's own pointer.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    #[inline]
    pub fn set_mut_ptr(&mut self, opaque: *mut c_void) {
        // SAFETY: a write of the `nc` member projected through the exclusive
        // handle's pointer to a live `AVRefStructOpaque`; it covers the whole
        // union, and the old pointer is a cookie nothing frees.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).nc).write(opaque) }
    }

    /// Field: AVRefStructOpaque.c
    ///
    /// Stores the user pointer `opaque`, NULL included, through the const
    /// member; [`mut_ptr`](RefStructOpaqueRef::mut_ptr) then reads the same
    /// address, as C's `.nc` reads a `.c` write. As for
    /// [`set_mut_ptr`](Self::set_mut_ptr), any address is safe to store.
    // As for `set_mut_ptr`: the pointer is the value stored.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    #[inline]
    pub fn set_const_ptr(&mut self, opaque: *const c_void) {
        // SAFETY: a write of the `c` member projected through the exclusive
        // handle's pointer to a live `AVRefStructOpaque`; it covers the whole
        // union, and the old pointer is a cookie nothing frees.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).c).write(opaque) }
    }
}

define_ctype!(
    /// Wraps: AVRefStructPool
    ///
    /// A thread-safe pool of RefStruct objects of one size, opaque outside
    /// `refstruct.c`. It is reached only through a pointer: owned as a
    /// [`RefStructPoolArc`], one counted reference to the pool, and used
    /// through the shared [`RefStructPoolRef`] that
    /// [`as_ref`](ffibox::CArc::as_ref) hands out.
    ///
    /// [`RefStructPoolMut`] is never handed out. The pool's own reference
    /// count (the one `av_refstruct_exclusive` reads) does not count the
    /// entries it has handed out, and each of those keeps a pointer back to
    /// the pool whose release locks the pool and relinks its free list from
    /// whichever thread drops the entry: a count of one does not prove
    /// exclusive access, so the owner never claims it.
    ///
    /// Invariant: the pool's callbacks, if it has any, are safe to run on
    /// any thread, concurrently, since C runs them on whichever thread gets
    /// or releases an entry. A pool without callbacks
    /// ([`RefStructPool::new`]) has nothing to run; a constructor taking
    /// callbacks, and code adopting a pool from C, must ensure it.
    RefStructPool,
    RefStructPoolRef,
    RefStructPoolMut,
    ffi::AVRefStructPool
);

// A pool's state: its size, flags, opaque and callbacks are written once by
// the constructor, its free list and `uninited` flag only under its own
// mutex, and its counts atomically.
// SAFETY: a pool has no thread affinity: its counted references are atomic,
// C frees it with `av_free`, which any thread may call, its mutable state is
// synchronized as above, and its callbacks are thread-safe by the type's
// invariant. Moving an owner or a handle to another thread moves that.
unsafe impl Send for RefStructPool {}
// SAFETY: as above, the state shared handles reach is either immutable after
// construction or synchronized by C (atomics and the pool's mutex): getting
// and releasing entries is the concurrent use `refstruct.h` documents as
// thread-safe, given thread-safe callbacks, which the invariant guarantees.
unsafe impl Sync for RefStructPool {}
// SAFETY: as for `Sync for RefStructPool`: a shared handle reaches only
// immutable or C-synchronized state.
unsafe impl Send for RefStructPoolRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for RefStructPoolRef<'_> {}
// SAFETY: as for `Send for RefStructPool`; the exclusive handle is never
// handed out, and adds nothing to what a shared one reaches.
unsafe impl Send for RefStructPoolMut<'_> {}
// SAFETY: `&RefStructPoolMut` reaches the pool only through `as_ref`.
unsafe impl Sync for RefStructPoolMut<'_> {}

/// `av_refstruct_pool_uninit` and `av_refstruct_ref`, as the policy of a
/// counted [`RefStructPool`] reference.
///
/// Thread-agnostic: the count is atomic and C frees the pool through
/// `av_free`, on whichever thread drops its last reference or its last
/// entry, so a [`RefStructPoolArc`] is `Send` and `Sync`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct RefStructPoolUnref;

/// One counted reference to a [`RefStructPool`].
///
/// `Clone` adds a reference to the same pool (`av_refstruct_ref`, which
/// cannot fail); drop releases one (`av_refstruct_pool_uninit`). Dropping the
/// last reference marks the pool for freeing and frees the entries it holds
/// for reuse; entries still in use stay valid and keep the pool's storage
/// alive, and the last of them returned frees it. `get_mut` always returns
/// `None`: see [`RefStructPool`].
pub type RefStructPoolArc = CArc<RefStructPool, RefStructPoolUnref>;

/// Wraps: av_refstruct_pool_uninit
/// Handwritten
///
/// `impl_cdrop!` would need a raw-pointer adapter only to pass the pool's
/// slot; the trait method holds the slot itself.
// SAFETY: `av_refstruct_pool_uninit` (`av_refstruct_unref` on the slot)
// releases one counted reference to a pool `av_refstruct_pool_alloc(_ext_c)`
// made, exactly once; the last one marks the pool uninitialised and leaves
// its storage to the last outstanding entry. It never reads the pool after
// the down-ref unless its own reference was the last.
unsafe impl CDrop<RefStructPool> for RefStructPoolUnref {
    #[inline]
    unsafe fn c_drop(&self, ptr: NonNull<RefStructPool>) {
        let mut slot = ptr.as_ptr().cast::<ffi::AVRefStructPool>();
        // SAFETY: the caller hands over one counted reference to a live
        // pool; C releases it once and NULLs only our local slot.
        unsafe { ffi::av_refstruct_pool_uninit(&mut slot) }
    }
}

/// Wraps: av_refstruct_ref
/// Handwritten
///
/// The up-ref returns its argument, which carries no status: it cannot
/// fail, so `c_up_ref` always reports success. `c_is_sole_owner` keeps the
/// default `false`: `av_refstruct_exclusive` does not count the entries that
/// still reach the pool (see [`RefStructPool`]).
// SAFETY: `av_refstruct_ref` atomically increments the counted references of
// the RefStruct object at `ptr`, which a pool is, and returns `ptr`: one more
// `av_refstruct_pool_uninit` is owed, which the clone's policy settles. The
// `uintptr_t` count cannot overflow while every reference is a live owner.
unsafe impl CRefClone<RefStructPool> for RefStructPoolUnref {
    #[inline]
    unsafe fn c_up_ref(&self, ptr: NonNull<RefStructPool>) -> bool {
        // SAFETY: the caller holds a counted reference to the live pool at
        // `ptr`, so its RefStruct header precedes it; C only bumps the count.
        unsafe { ffi::av_refstruct_ref(ptr.as_ptr().cast()) };
        true
    }
}

impl RefStructPool {
    /// Wraps: av_refstruct_pool_alloc
    ///
    /// A new pool of entries of `size` bytes, with no callbacks, as the first
    /// counted reference to it; `None` when C runs out of memory or cannot
    /// create the pool's mutex. Unless `flags` holds
    /// [`PoolFlags::NO_ZEROING`] or [`PoolFlags::ZERO_EVERY_TIME`], C zeroes
    /// an entry when it first allocates it; with
    /// [`PoolFlags::ZERO_EVERY_TIME`], every time it hands one out. With no
    /// callbacks, [`PoolFlags::RESET_ON_INIT_ERROR`] and
    /// [`PoolFlags::FREE_ON_INIT_ERROR`] have no effect.
    #[must_use]
    pub fn new(size: usize, flags: PoolFlags) -> Option<RefStructPoolArc> {
        // SAFETY: `av_refstruct_pool_alloc` takes any size and flag bits;
        // with no callbacks, the pool's invariant holds trivially.
        let pool = unsafe { ffi::av_refstruct_pool_alloc(size, flags.bits()) };
        // SAFETY: NULL, or a new pool holding exactly one counted
        // reference, which the arc adopts and releases with
        // `av_refstruct_pool_uninit`.
        unsafe { RefStructPoolArc::from_c(pool) }
    }
}

/// The `AV_REFSTRUCT_POOL_FLAG_*` flags of a [`RefStructPool`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PoolFlags(c_uint);

impl PoolFlags {
    /// Each entry is zeroed when C first allocates it, before the init
    /// callback runs, and then handed out as the last user left it.
    pub const NONE: Self = Self(0);
    /// `AV_REFSTRUCT_POOL_FLAG_NO_ZEROING`: entries are not zeroed when C
    /// allocates them, so a new entry's bytes are uninitialised.
    pub const NO_ZEROING: Self = Self(ffi::AV_REFSTRUCT_POOL_FLAG_NO_ZEROING);
    /// `AV_REFSTRUCT_POOL_FLAG_RESET_ON_INIT_ERROR`: when the init callback
    /// fails, the reset callback runs on the entry; only with both callbacks.
    pub const RESET_ON_INIT_ERROR: Self = Self(ffi::AV_REFSTRUCT_POOL_FLAG_RESET_ON_INIT_ERROR);
    /// `AV_REFSTRUCT_POOL_FLAG_FREE_ON_INIT_ERROR`: when the init callback
    /// fails, the entry-freeing callback runs on the entry, after the reset
    /// callback if that runs too; only with both callbacks.
    pub const FREE_ON_INIT_ERROR: Self = Self(ffi::AV_REFSTRUCT_POOL_FLAG_FREE_ON_INIT_ERROR);
    /// `AV_REFSTRUCT_POOL_FLAG_ZERO_EVERY_TIME`: every entry is zeroed each
    /// time it is handed out, after the init or reset callback; it implies
    /// [`NO_ZEROING`](Self::NO_ZEROING) at allocation.
    pub const ZERO_EVERY_TIME: Self = Self(ffi::AV_REFSTRUCT_POOL_FLAG_ZERO_EVERY_TIME);

    /// The C flag bits.
    #[inline]
    #[must_use]
    pub const fn bits(self) -> c_uint {
        self.0
    }

    /// Whether every flag in `other` is set here.
    #[inline]
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl BitOr for PoolFlags {
    type Output = Self;
    #[inline]
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for PoolFlags {
    #[inline]
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[cfg(test)]
mod unit_tests {
    use core::ffi::c_void;
    use core::ptr::{self, NonNull};
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    use ffibox::{CDrop, CRefClone};

    use super::{
        PoolFlags, RefStructOpaque, RefStructPool, RefStructPoolArc, RefStructPoolUnref, ffi,
    };

    /// Addresses at the edges of a pointer, none of them dereferenced.
    const ADDRS: [usize; 6] = [0, 1, 8, 0x1000, usize::MAX >> 1, usize::MAX];

    fn hash_of(v: &RefStructOpaque) -> u64 {
        let mut h = DefaultHasher::new();
        v.hash(&mut h);
        h.finish()
    }

    #[test]
    fn opaque_members_read_each_others_writes() {
        for a in ADDRS {
            let m = RefStructOpaque::new(ptr::without_provenance_mut(a));
            assert_eq!(m.as_ref().mut_ptr().addr(), a);
            assert_eq!(m.as_ref().const_ptr().addr(), a);
            let c = RefStructOpaque::from_const(ptr::without_provenance(a));
            assert_eq!(c.as_ref().mut_ptr().addr(), a);
            assert_eq!(c.as_ref().const_ptr().addr(), a);
            assert_eq!(m, c);
            assert_eq!(hash_of(&m), hash_of(&c));
        }
    }

    #[test]
    fn opaque_setters_overwrite_the_whole_union() {
        let mut v = RefStructOpaque::new(ptr::without_provenance_mut(0x10));
        let mut m = v.as_mut();
        m.set_const_ptr(ptr::without_provenance(0x20));
        assert_eq!(m.as_ref().mut_ptr().addr(), 0x20);
        m.set_mut_ptr(ptr::null_mut());
        assert!(m.as_ref().const_ptr().is_null());
        assert_eq!(v, RefStructOpaque::default());
    }

    #[test]
    fn opaque_keeps_provenance_for_its_user() {
        let mut slot = 7u32;
        let cookie = (&raw mut slot).cast::<c_void>();
        let v = RefStructOpaque::new(cookie);
        let back = v.as_ref().mut_ptr().cast::<u32>();
        // SAFETY: `back` is `cookie`, a pointer to the live local `slot`
        // derived without an intermediate reference, read back unchanged.
        unsafe { back.write(9) };
        assert_eq!(slot, 9);
    }

    #[test]
    fn opaque_raw_round_trip_and_defaults() {
        assert!(RefStructOpaque::default().as_ref().mut_ptr().is_null());
        for a in ADDRS {
            let v = RefStructOpaque::new(ptr::without_provenance_mut(a));
            let back = RefStructOpaque::from_raw(v.into_raw());
            assert_eq!(back, v);
            let copy = v;
            assert_eq!(copy.as_ref().const_ptr(), v.as_ref().const_ptr());
        }
        assert_ne!(
            RefStructOpaque::new(ptr::without_provenance_mut(1)),
            RefStructOpaque::new(ptr::without_provenance_mut(2))
        );
        let shown = format!(
            "{:?}",
            RefStructOpaque::new(ptr::without_provenance_mut(0x1234))
        );
        assert!(
            shown.starts_with("RefStructOpaque(") && shown.contains("0x1234"),
            "{shown}"
        );
        assert_eq!(
            format!("{:?}", RefStructOpaque::default().as_ref()),
            format!("{:?}", RefStructOpaque::default())
        );
    }

    #[test]
    fn opaque_layout_is_the_unions() {
        assert_eq!(
            size_of::<RefStructOpaque>(),
            size_of::<ffi::AVRefStructOpaque>()
        );
        assert_eq!(
            align_of::<RefStructOpaque>(),
            align_of::<ffi::AVRefStructOpaque>()
        );
        assert_eq!(size_of::<RefStructOpaque>(), size_of::<*mut c_void>());
    }

    #[test]
    fn pool_flags_are_the_c_bits() {
        assert_eq!(PoolFlags::default(), PoolFlags::NONE);
        assert_eq!(PoolFlags::NONE.bits(), 0);
        assert_eq!(PoolFlags::NO_ZEROING.bits(), 1);
        assert_eq!(PoolFlags::RESET_ON_INIT_ERROR.bits(), 1 << 16);
        assert_eq!(PoolFlags::FREE_ON_INIT_ERROR.bits(), 1 << 17);
        assert_eq!(PoolFlags::ZERO_EVERY_TIME.bits(), 1 << 18);
        let mut f = PoolFlags::NO_ZEROING | PoolFlags::ZERO_EVERY_TIME;
        assert!(f.contains(PoolFlags::NO_ZEROING) && f.contains(PoolFlags::ZERO_EVERY_TIME));
        assert!(!f.contains(PoolFlags::FREE_ON_INIT_ERROR));
        assert!(f.contains(PoolFlags::NONE));
        f |= PoolFlags::FREE_ON_INIT_ERROR;
        assert_eq!(f.bits(), 1 | 1 << 17 | 1 << 18);
    }

    fn exclusive(p: *const ffi::AVRefStructPool) -> bool {
        // SAFETY: `p` is a live pool whose reference the caller holds; the
        // call reads its RefStruct header's count.
        unsafe { ffi::av_refstruct_exclusive(p.cast()) == 1 }
    }

    /// The policy's up-ref and down-ref on a pool adopted from C: each
    /// up-ref is one more down-ref owed, and the last one releases it (LSan
    /// reports a leak otherwise).
    #[test]
    fn policy_counts_references_on_a_raw_pool() {
        // SAFETY: no callbacks, any size and flags.
        let raw = unsafe { ffi::av_refstruct_pool_alloc(32, 0) };
        let p = NonNull::new(raw.cast::<RefStructPool>()).expect("pool");
        assert!(exclusive(raw));
        // SAFETY: `p` is live and we hold its one reference.
        assert!(unsafe { RefStructPoolUnref.c_up_ref(p) });
        assert!(!exclusive(raw));
        // SAFETY: as above; this is the owner the up-ref added.
        assert!(!unsafe { RefStructPoolUnref.c_is_sole_owner(p) });
        // SAFETY: releases the reference the up-ref added.
        unsafe { RefStructPoolUnref.c_drop(p) };
        assert!(exclusive(raw));
        // SAFETY: a sole reference is still not claimed as sole.
        assert!(!unsafe { RefStructPoolUnref.c_is_sole_owner(p) });
        // SAFETY: releases the constructor's reference, the last one.
        unsafe { RefStructPoolUnref.c_drop(p) };
    }

    #[test]
    fn adopting_null_is_none() {
        // SAFETY: NULL adopts nothing.
        assert!(unsafe { RefStructPoolArc::from_c(ptr::null_mut()) }.is_none());
    }

    /// A pool's owner never hands out the exclusive handle, sole or not.
    #[test]
    fn arc_never_claims_exclusive_access() {
        let mut a = RefStructPool::new(8, PoolFlags::NONE).expect("pool");
        assert!(exclusive(a.as_c_ptr()));
        assert!(a.get_mut().is_none());
        let a = a.try_into_box().expect_err("never sole");
        let b = a.try_clone().expect("up-ref");
        assert!(RefStructPoolArc::ptr_eq(&a, &b));
        assert!(!exclusive(a.as_c_ptr()));
        drop(a);
        assert!(exclusive(b.as_c_ptr()));
    }
}
