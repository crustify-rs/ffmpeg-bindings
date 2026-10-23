//! Safe wrappers over `libavutil/csp.h`.
//!
//! [`TrcFunction`] and [`EotfFunction`] call the transfer functions C hands
//! out for a transfer characteristic (`av_csp_trc_function`,
//! `av_csp_eotf_function`), which the [`ColorTransfer`] getters
//! [`trc_function`](ColorTransfer::trc_function),
//! [`eotf`](ColorTransfer::eotf) and their inverses return, beside its
//! approximate gammas.
//!
//! [`CieXy`] is `AVCIExy` (also spelled `AVWhitepointCoefficients`,
//! [`WhitepointCoefficients`] here): a CIE 1931 chromaticity, two
//! [`Rational`] coordinates. C embeds it by value in its gamut descriptions
//! (a white point, and each of the red, green and blue primaries) and copies
//! it by value; nothing allocates, frees or clones one on its own, so it is a
//! value type with no owner. A field of a wrapped C struct is reached as a
//! [`CieXyRef`] or [`CieXyMut`] over the projected place, and each
//! coordinate as a [`RationalRef`] or [`RationalMut`] over its own.
//!
//! [`LumaCoefficients`] is `AVLumaCoefficients`: the red, green and blue
//! weights of luma as three [`Rational`]s, a value type reached as a
//! [`LumaCoefficientsRef`] or [`LumaCoefficientsMut`], each coefficient as a
//! [`RationalRef`] or [`RationalMut`] over its own.
//!
//! [`PrimaryCoefficients`] is `AVPrimaryCoefficients`: the chromaticities of
//! the red, green and blue primaries of a gamut as three [`CieXy`]s, a value
//! type C embeds in its gamut descriptions (`AVColorPrimariesDesc.prim`),
//! reached as a [`PrimaryCoefficientsRef`] or [`PrimaryCoefficientsMut`],
//! each primary as a [`CieXyRef`] or [`CieXyMut`] over its own.
//!
//! [`ColorPrimariesDescriptor`] is `AVColorPrimariesDesc`: the complete
//! description of a gamut, a white point ([`CieXy`]) and its primaries
//! ([`PrimaryCoefficients`]), a value type C hands out as shared borrows of
//! its static tables and embeds in Dolby Vision metadata, reached as a
//! [`ColorPrimariesDescriptorRef`] or [`ColorPrimariesDescriptorMut`], the
//! white point as a [`CieXyRef`] or [`CieXyMut`] and the primaries as a
//! [`PrimaryCoefficientsRef`] or [`PrimaryCoefficientsMut`] over their own.
//! [`ColorPrimaries::descriptor`] borrows C's description of a gamut, and
//! [`identify`](ColorPrimariesDescriptorRef::identify) names the gamut a
//! description lies within C's tolerance of.

use core::fmt;
use core::ptr::{addr_of, addr_of_mut};

use ffibox::{CZeroable, define_ctype};

use super::ffi;
use super::pixfmt::{ColorPrimaries, ColorSpace, ColorTransfer};
use super::rational::{Rational, RationalMut, RationalRef};

/// Wraps: `av_csp_trc_function`
///
/// A transfer function from `libavutil/csp.c`: an OETF, or an inverse EOTF
/// (for SMPTE ST 2084 / PQ), mapping linear light to gamma-encoded light, or
/// its inverse, over a nominal domain and range of `[0.0, 1.0]`.
///
/// Only C's own functions are held, which are pure arithmetic on their
/// argument, defined (if not meaningful) for every `f64`, NaN and infinities
/// included, and keep no state: a handle is `Copy`, `Send` and `Sync`, and
/// calling it is safe.
#[derive(Clone, Copy, Debug)]
pub struct TrcFunction(unsafe extern "C" fn(f64) -> f64);

impl TrcFunction {
    /// The transfer function applied to `x`.
    #[inline]
    #[must_use]
    pub fn call(self, x: f64) -> f64 {
        // SAFETY: the handle holds one of `csp.c`'s transfer functions,
        // which take any `f64` and touch no memory.
        unsafe { (self.0)(x) }
    }
}

/// Wraps: `av_csp_eotf_function`
///
/// An ITU EOTF from `libavutil/csp.c`, or its inverse, for a reference
/// display with white point luminance `lw` and black point luminance `lb`, in
/// nits (cd/m^2): it maps a signal level in `[0, 1]` to display luminance in
/// nits, or back, per channel of an RGB triple (CIE XYZ for SMPTE ST 428).
///
/// Only C's own functions are held, which read and overwrite the three
/// channels they are given and nothing else, are defined (if not meaningful)
/// for every `f64` input, and keep no state: a handle is `Copy`, `Send` and
/// `Sync`, and calling it is safe.
#[derive(Clone, Copy, Debug)]
pub struct EotfFunction(unsafe extern "C" fn(f64, f64, *mut f64));

impl EotfFunction {
    /// Applies the function to the channels `c` in place, for a display
    /// with white point `lw` and black point `lb`.
    #[inline]
    pub fn call(self, lw: f64, lb: f64, c: &mut [f64; 3]) {
        // SAFETY: the handle holds one of `csp.c`'s EOTFs, which read and
        // write `c[0..3]` only, all inside the exclusively borrowed array,
        // during the call.
        unsafe { (self.0)(lw, lb, c.as_mut_ptr()) }
    }
}

define_ctype!(
    /// Wraps: AVCIExy
    ///
    /// A chromaticity in the CIE 1931 xy plane, as two rationals, neither
    /// reduced nor range-checked. Its getters are on [`CieXyRef`]
    /// (`as_ref()`), its setters on [`CieXyMut`] (`as_mut()`).
    ///
    /// C's own tables hold unreduced fractions such as `3127/10000`, and an
    /// unspecified gamut is all-zero (`0/0` coordinates), so every accessor
    /// takes any pair of rationals.
    #[derive(Clone, Copy)]
    CieXy,
    CieXyRef,
    CieXyMut,
    ffi::AVCIExy
);

/// A white point, as C's `AVWhitepointCoefficients` typedef names a
/// [`CieXy`].
pub type WhitepointCoefficients = CieXy;

// SAFETY: all-zero is two `0/0` coordinates, the value C itself stores for
// the white point and primaries of every gamut its table leaves unspecified
// (and skips on `r.x.num == 0`); every accessor takes it as the rationals it
// is, and the type has no teardown to reject it.
unsafe impl CZeroable for CieXy {}

// SAFETY: an `AVCIExy` is 16 bytes of plain data, two `AVRational`s of two
// `int`s each, with no pointer, sub-allocation or tie to the thread that
// wrote them, so a value, or an exclusive handle to one, moves between
// threads as an `[i32; 4]` does.
unsafe impl Send for CieXy {}
// SAFETY: as for `CieXy`: the handle is the only access to the coordinates,
// and the coordinate handles it lends borrow it.
unsafe impl Send for CieXyMut<'_> {}

// SAFETY: nothing reachable through a shared handle writes: `CieXyRef` has
// getters only, which lend getter-only `RationalRef`s, and the struct has no
// interior mutability, so concurrent shared access reads unchanging bytes, as
// through `&[i32; 4]`.
unsafe impl Sync for CieXy {}
// SAFETY: as for `Sync for CieXy`: copies of a shared handle only read.
unsafe impl Send for CieXyRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for CieXyRef<'_> {}
// SAFETY: `&CieXyMut` reaches the coordinates only through `as_ref`, a
// shared handle; the setters and `x_mut` / `y_mut` need `&mut`.
unsafe impl Sync for CieXyMut<'_> {}

impl CieXy {
    /// The chromaticity `(x, y)`, with both coordinates as given.
    #[inline]
    #[must_use]
    pub const fn new(x: Rational, y: Rational) -> Self {
        Self::from_raw(ffi::AVCIExy {
            x: x.into_raw(),
            y: y.into_raw(),
        })
    }

    /// The chromaticity C encodes as `raw`, whatever its coordinates.
    #[inline]
    #[must_use]
    pub const fn from_raw(raw: ffi::AVCIExy) -> Self {
        Self(raw, core::marker::PhantomData)
    }

    /// The C encoding of this chromaticity, for passing it or storing it in
    /// a C struct by value.
    #[inline]
    #[must_use]
    pub const fn into_raw(self) -> ffi::AVCIExy {
        self.0
    }
}

/// Two `0/0` coordinates: the chromaticity of an unspecified gamut.
impl Default for CieXy {
    #[inline]
    fn default() -> Self {
        Self::zeroed()
    }
}

impl From<ffi::AVCIExy> for CieXy {
    #[inline]
    fn from(raw: ffi::AVCIExy) -> Self {
        Self::from_raw(raw)
    }
}

impl From<CieXy> for ffi::AVCIExy {
    #[inline]
    fn from(xy: CieXy) -> Self {
        xy.into_raw()
    }
}

/// Coordinate by coordinate, each by [`Rational`]'s value equality: `1/2`
/// equals `2/4`, and a `0/0` coordinate equals nothing, itself included.
impl PartialEq for CieXy {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (self.as_ref(), other.as_ref());
        a.x().get() == b.x().get() && a.y().get() == b.y().get()
    }
}

impl fmt::Debug for CieXy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl fmt::Debug for CieXyRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CieXy")
            .field("x", &self.x())
            .field("y", &self.y())
            .finish()
    }
}

impl<'a> CieXyRef<'a> {
    /// Field: AVCIExy.x
    ///
    /// The x coordinate, as a handle over it that lives as long as this one.
    ///
    /// # Panics
    ///
    /// Never: the place of a field of a live struct is not NULL.
    #[must_use]
    pub fn x(&self) -> RationalRef<'a> {
        // SAFETY: the handle's pointer is to a live object, as the operation below relies on,
        // so the place of its `x` field is in bounds; nothing is accessed here.
        let x_ptr = unsafe { addr_of!((*self.as_ptr()).x) };
        // SAFETY: `x` is projected through the shared handle's pointer to a
        // live, initialised `AVCIExy`, so it is non-NULL, aligned and
        // initialised; the struct stays live and unwritten for `'a`, which
        // the returned getter-only handle cannot outlive.
        unsafe { RationalRef::from_ptr(x_ptr.cast_mut()) }
            .expect("a field of a live struct is not NULL")
    }

    /// Field: AVCIExy.y
    ///
    /// The y coordinate, as a handle over it that lives as long as this one.
    ///
    /// # Panics
    ///
    /// Never: the place of a field of a live struct is not NULL.
    #[must_use]
    pub fn y(&self) -> RationalRef<'a> {
        // SAFETY: the handle's pointer is to a live object, as the operation below relies on,
        // so the place of its `y` field is in bounds; nothing is accessed here.
        let y_ptr = unsafe { addr_of!((*self.as_ptr()).y) };
        // SAFETY: as in `x`, for the `y` field.
        unsafe { RationalRef::from_ptr(y_ptr.cast_mut()) }
            .expect("a field of a live struct is not NULL")
    }

    /// A copy of the whole chromaticity, independent of the place it was
    /// read from.
    #[must_use]
    pub fn get(&self) -> CieXy {
        // SAFETY: a read of the whole live, initialised `AVCIExy` through the
        // shared handle's pointer; it is four `int`s, so the byte copy is a
        // complete, independent value.
        CieXy::from_raw(unsafe { self.as_ptr().read() })
    }
}

impl CieXyMut<'_> {
    /// Field: AVCIExy.x
    ///
    /// An exclusive handle over the x coordinate, for writing its parts;
    /// this handle is frozen while it lives.
    ///
    /// # Panics
    ///
    /// Never: the place of a field of a live struct is not NULL.
    #[must_use]
    pub fn x_mut(&mut self) -> RationalMut<'_> {
        // SAFETY: the handle's pointer is to a live object, as the operation below relies on,
        // so the place of its `x` field is in bounds; nothing is accessed here.
        let x_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).x) };
        // SAFETY: `x` is projected through the exclusive handle's pointer to
        // a live `AVCIExy`, so it is non-NULL, aligned and initialised; the
        // result borrows `self` exclusively, so it is the only access to the
        // field while it lives.
        unsafe { RationalMut::from_ptr(x_ptr) }.expect("a field of a live struct is not NULL")
    }

    /// Field: AVCIExy.y
    ///
    /// An exclusive handle over the y coordinate, for writing its parts;
    /// this handle is frozen while it lives.
    ///
    /// # Panics
    ///
    /// Never: the place of a field of a live struct is not NULL.
    #[must_use]
    pub fn y_mut(&mut self) -> RationalMut<'_> {
        // SAFETY: the handle's pointer is to a live object, as the operation below relies on,
        // so the place of its `y` field is in bounds; nothing is accessed here.
        let y_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).y) };
        // SAFETY: as in `x_mut`, for the `y` field.
        unsafe { RationalMut::from_ptr(y_ptr) }.expect("a field of a live struct is not NULL")
    }

    /// Field: AVCIExy.x
    ///
    /// Sets the x coordinate, leaving y as it is.
    pub fn set_x(&mut self, x: Rational) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `x` field is in bounds; nothing is accessed here.
        let x_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).x) };
        // SAFETY: a write of the `x` field projected through the exclusive
        // handle's pointer to a live `AVCIExy`; the old value owns nothing to
        // leak.
        unsafe { x_ptr.write(x.into_raw()) }
    }

    /// Field: AVCIExy.y
    ///
    /// Sets the y coordinate, leaving x as it is.
    pub fn set_y(&mut self, y: Rational) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `y` field is in bounds; nothing is accessed here.
        let y_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).y) };
        // SAFETY: a write of the `y` field projected through the exclusive
        // handle's pointer to a live `AVCIExy`; the old value owns nothing to
        // leak.
        unsafe { y_ptr.write(y.into_raw()) }
    }

    /// Overwrites both coordinates with `xy`'s.
    pub fn set(&mut self, xy: CieXy) {
        // SAFETY: a write of the whole `AVCIExy` through the exclusive
        // handle's pointer to a live one; the old value owns nothing to leak.
        unsafe { self.as_mut_ptr().write(xy.into_raw()) }
    }
}

define_ctype!(
    /// Wraps: AVLumaCoefficients
    ///
    /// The luma coefficients of a YUV-like colour space, as three
    /// [`Rational`]s: the weights of red, green and blue in luma
    /// (`Y = cr * R + cg * G + cb * B`) for RGB to YUV, YCoCg or similar
    /// conversions. Its getters are on [`LumaCoefficientsRef`]
    /// (`as_ref()`), its setters on [`LumaCoefficientsMut`] (`as_mut()`).
    ///
    /// C hands these out as shared borrows of its static, never-written
    /// table (`av_csp_luma_coeffs_from_avcsp`) and its users read them by
    /// value; nothing allocates, frees or clones one on its own, so it is a
    /// value type with no owner, built by [`LumaCoefficients::new`],
    /// [`from_raw`](LumaCoefficients::from_raw) or `zeroed()`. No field is
    /// checked, by C or here: each takes any pair of `int`s, as a
    /// [`Rational`] does.
    #[derive(Clone, Copy)]
    LumaCoefficients,
    LumaCoefficientsRef,
    LumaCoefficientsMut,
    ffi::AVLumaCoefficients
);

// SAFETY: all-zero is three `0/0` rationals, the entry C's own table holds for
// every colour space it has no coefficients for (which
// `av_csp_luma_coeffs_from_avcsp` recognises by `cr.num == 0`); every
// accessor takes it as the six integers it is, and the type has no teardown
// to reject it.
unsafe impl CZeroable for LumaCoefficients {}

// SAFETY: an `AVLumaCoefficients` is 24 bytes of plain data, six `int`s with
// no pointer, sub-allocation or tie to the thread that wrote them, so a value,
// or an exclusive handle to one, moves between threads as an `[i32; 6]` does.
unsafe impl Send for LumaCoefficients {}
// SAFETY: as for `LumaCoefficients`: the handle is the only access to the
// fields.
unsafe impl Send for LumaCoefficientsMut<'_> {}

// SAFETY: nothing reachable through a shared handle writes:
// `LumaCoefficientsRef` has getters only, whose `RationalRef`s have getters
// only too, and the struct has no interior mutability, so concurrent shared
// access reads unchanging bytes, as through `&[i32; 6]`.
unsafe impl Sync for LumaCoefficients {}
// SAFETY: as for `Sync for LumaCoefficients`: copies of a shared handle only
// read.
unsafe impl Send for LumaCoefficientsRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for LumaCoefficientsRef<'_> {}
// SAFETY: `&LumaCoefficientsMut` reaches the fields only through `as_ref`, a
// shared handle; the setters and the exclusive field handles need `&mut`.
unsafe impl Sync for LumaCoefficientsMut<'_> {}

impl LumaCoefficients {
    /// The coefficients `cr`, `cg` and `cb`, as given.
    #[inline]
    #[must_use]
    pub const fn new(cr: Rational, cg: Rational, cb: Rational) -> Self {
        Self::from_raw(ffi::AVLumaCoefficients {
            cr: cr.into_raw(),
            cg: cg.into_raw(),
            cb: cb.into_raw(),
        })
    }

    /// The coefficients C encodes as `raw`, whatever their values.
    #[inline]
    #[must_use]
    pub const fn from_raw(raw: ffi::AVLumaCoefficients) -> Self {
        Self(raw, core::marker::PhantomData)
    }

    /// The C encoding of these coefficients, for passing them by value.
    #[inline]
    #[must_use]
    pub const fn into_raw(self) -> ffi::AVLumaCoefficients {
        self.0
    }
}

/// Three `0/0` rationals: the entry C's table holds for a colour space with
/// no coefficients.
impl Default for LumaCoefficients {
    #[inline]
    fn default() -> Self {
        Self::zeroed()
    }
}

impl From<ffi::AVLumaCoefficients> for LumaCoefficients {
    #[inline]
    fn from(raw: ffi::AVLumaCoefficients) -> Self {
        Self::from_raw(raw)
    }
}

impl From<LumaCoefficients> for ffi::AVLumaCoefficients {
    #[inline]
    fn from(c: LumaCoefficients) -> Self {
        c.into_raw()
    }
}

/// Coefficient by coefficient, each by [`Rational`]'s value equality: `1/2`
/// equals `2/4`, and a `0/0` coefficient equals nothing, itself included.
impl PartialEq for LumaCoefficients {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (self.as_ref(), other.as_ref());
        a.cr().get() == b.cr().get() && a.cg().get() == b.cg().get() && a.cb().get() == b.cb().get()
    }
}

impl fmt::Debug for LumaCoefficients {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl fmt::Debug for LumaCoefficientsRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LumaCoefficients")
            .field("cr", &self.cr())
            .field("cg", &self.cg())
            .field("cb", &self.cb())
            .finish()
    }
}

impl<'a> LumaCoefficientsRef<'a> {
    /// Field: AVLumaCoefficients.cr
    ///
    /// The red coefficient, read through a shared handle over the field.
    ///
    /// # Panics
    ///
    /// Never: the place of a field of a live struct is not NULL.
    #[must_use]
    pub fn cr(&self) -> RationalRef<'a> {
        // SAFETY: the handle's pointer is to a live object, as the operation below relies on,
        // so the place of its `cr` field is in bounds; nothing is accessed here.
        let cr_ptr = unsafe { addr_of!((*self.as_ptr()).cr) };
        // SAFETY: the `cr` field, projected without a reference through the
        // shared handle's pointer to a live, initialised
        // `AVLumaCoefficients`, is a live, initialised `AVRational` for as
        // long as the coefficients are borrowed (`'a`); `RationalRef` only
        // reads through the pointer, and a field address is never NULL.
        unsafe { RationalRef::from_ptr(cr_ptr.cast_mut()) }.expect("a field address is not NULL")
    }

    /// Field: AVLumaCoefficients.cg
    ///
    /// The green coefficient, read through a shared handle over the field.
    ///
    /// # Panics
    ///
    /// Never: the place of a field of a live struct is not NULL.
    #[must_use]
    pub fn cg(&self) -> RationalRef<'a> {
        // SAFETY: the handle's pointer is to a live object, as the operation below relies on,
        // so the place of its `cg` field is in bounds; nothing is accessed here.
        let cg_ptr = unsafe { addr_of!((*self.as_ptr()).cg) };
        // SAFETY: as for `cr`, for the `cg` field.
        unsafe { RationalRef::from_ptr(cg_ptr.cast_mut()) }.expect("a field address is not NULL")
    }

    /// Field: AVLumaCoefficients.cb
    ///
    /// The blue coefficient, read through a shared handle over the field.
    ///
    /// # Panics
    ///
    /// Never: the place of a field of a live struct is not NULL.
    #[must_use]
    pub fn cb(&self) -> RationalRef<'a> {
        // SAFETY: the handle's pointer is to a live object, as the operation below relies on,
        // so the place of its `cb` field is in bounds; nothing is accessed here.
        let cb_ptr = unsafe { addr_of!((*self.as_ptr()).cb) };
        // SAFETY: as for `cr`, for the `cb` field.
        unsafe { RationalRef::from_ptr(cb_ptr.cast_mut()) }.expect("a field address is not NULL")
    }

    /// A copy of all three coefficients, independent of the place they were
    /// read from.
    #[must_use]
    pub fn get(&self) -> LumaCoefficients {
        // SAFETY: a read of the whole live, initialised `AVLumaCoefficients`
        // through the shared handle's pointer; it is six `int`s, so the byte
        // copy is a complete, independent value.
        LumaCoefficients::from_raw(unsafe { self.as_ptr().read() })
    }
}

impl LumaCoefficientsMut<'_> {
    /// Field: AVLumaCoefficients.cr
    ///
    /// The red coefficient, written through an exclusive handle over the
    /// field, which borrows these coefficients exclusively while it lives.
    ///
    /// # Panics
    ///
    /// Never: the place of a field of a live struct is not NULL.
    #[must_use]
    pub fn cr_mut(&mut self) -> RationalMut<'_> {
        // SAFETY: the handle's pointer is to a live object, as the operation below relies on,
        // so the place of its `cr` field is in bounds; nothing is accessed here.
        let cr_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).cr) };
        // SAFETY: the `cr` field, projected without a reference through the
        // exclusive handle's pointer to a live `AVLumaCoefficients`, is a
        // live `AVRational` that only the result reaches while it borrows
        // `self` exclusively; a field address is never NULL.
        unsafe { RationalMut::from_ptr(cr_ptr) }.expect("a field address is not NULL")
    }

    /// Field: AVLumaCoefficients.cg
    ///
    /// The green coefficient, written through an exclusive handle over the
    /// field, which borrows these coefficients exclusively while it lives.
    ///
    /// # Panics
    ///
    /// Never: the place of a field of a live struct is not NULL.
    #[must_use]
    pub fn cg_mut(&mut self) -> RationalMut<'_> {
        // SAFETY: the handle's pointer is to a live object, as the operation below relies on,
        // so the place of its `cg` field is in bounds; nothing is accessed here.
        let cg_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).cg) };
        // SAFETY: as for `cr_mut`, for the `cg` field.
        unsafe { RationalMut::from_ptr(cg_ptr) }.expect("a field address is not NULL")
    }

    /// Field: AVLumaCoefficients.cb
    ///
    /// The blue coefficient, written through an exclusive handle over the
    /// field, which borrows these coefficients exclusively while it lives.
    ///
    /// # Panics
    ///
    /// Never: the place of a field of a live struct is not NULL.
    #[must_use]
    pub fn cb_mut(&mut self) -> RationalMut<'_> {
        // SAFETY: the handle's pointer is to a live object, as the operation below relies on,
        // so the place of its `cb` field is in bounds; nothing is accessed here.
        let cb_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).cb) };
        // SAFETY: as for `cr_mut`, for the `cb` field.
        unsafe { RationalMut::from_ptr(cb_ptr) }.expect("a field address is not NULL")
    }

    /// Field: AVLumaCoefficients.cr
    ///
    /// Sets the red coefficient, leaving the others as they are.
    pub fn set_cr(&mut self, cr: Rational) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `cr` field is in bounds; nothing is accessed here.
        let cr_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).cr) };
        // SAFETY: a write of the `cr` field projected through the exclusive
        // handle's pointer to a live `AVLumaCoefficients`; the old value owns
        // nothing to leak.
        unsafe { cr_ptr.write(cr.into_raw()) }
    }

    /// Field: AVLumaCoefficients.cg
    ///
    /// Sets the green coefficient, leaving the others as they are.
    pub fn set_cg(&mut self, cg: Rational) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `cg` field is in bounds; nothing is accessed here.
        let cg_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).cg) };
        // SAFETY: as in `set_cr`, for the `cg` field.
        unsafe { cg_ptr.write(cg.into_raw()) }
    }

    /// Field: AVLumaCoefficients.cb
    ///
    /// Sets the blue coefficient, leaving the others as they are.
    pub fn set_cb(&mut self, cb: Rational) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `cb` field is in bounds; nothing is accessed here.
        let cb_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).cb) };
        // SAFETY: as in `set_cr`, for the `cb` field.
        unsafe { cb_ptr.write(cb.into_raw()) }
    }

    /// Overwrites all three coefficients with `c`'s.
    pub fn set(&mut self, c: LumaCoefficients) {
        // SAFETY: a write of the whole `AVLumaCoefficients` through the
        // exclusive handle's pointer to a live one; the old value owns
        // nothing to leak.
        unsafe { self.as_mut_ptr().write(c.into_raw()) }
    }
}

define_ctype!(
    /// Wraps: AVPrimaryCoefficients
    ///
    /// The red, green and blue primaries of a gamut, each a chromaticity in
    /// the CIE 1931 xy plane ([`CieXy`]). Its getters are on
    /// [`PrimaryCoefficientsRef`] (`as_ref()`), its setters on
    /// [`PrimaryCoefficientsMut`] (`as_mut()`).
    ///
    /// C embeds these by value in its gamut descriptions
    /// (`AVColorPrimariesDesc.prim`), which it hands out as shared borrows of
    /// its static, never-written table (`av_csp_primaries_desc_from_id`) and
    /// reads field by field (`av_csp_primaries_id_from_desc`); nothing
    /// allocates, frees or clones one on its own, so it is a value type with
    /// no owner, built by [`PrimaryCoefficients::new`],
    /// [`from_raw`](PrimaryCoefficients::from_raw) or `zeroed()`. No
    /// coordinate is checked, by C or here: each takes any pair of `int`s, as
    /// a [`Rational`] does.
    #[derive(Clone, Copy)]
    PrimaryCoefficients,
    PrimaryCoefficientsRef,
    PrimaryCoefficientsMut,
    ffi::AVPrimaryCoefficients
);

// SAFETY: all-zero is three chromaticities of `0/0` coordinates, the primaries
// C's own table holds for every gamut it leaves unspecified (and skips on
// `r.x.num == 0`); every accessor takes it as the twelve integers it is, and
// the type has no teardown to reject it.
unsafe impl CZeroable for PrimaryCoefficients {}

// SAFETY: an `AVPrimaryCoefficients` is 48 bytes of plain data, three
// `AVCIExy`s of two `AVRational`s each, with no pointer, sub-allocation or tie
// to the thread that wrote them, so a value, or an exclusive handle to one,
// moves between threads as an `[i32; 12]` does.
unsafe impl Send for PrimaryCoefficients {}
// SAFETY: as for `PrimaryCoefficients`: the handle is the only access to the
// primaries, and the primary handles it lends borrow it.
unsafe impl Send for PrimaryCoefficientsMut<'_> {}

// SAFETY: nothing reachable through a shared handle writes:
// `PrimaryCoefficientsRef` has getters only, whose `CieXyRef`s have getters
// only too, and the struct has no interior mutability, so concurrent shared
// access reads unchanging bytes, as through `&[i32; 12]`.
unsafe impl Sync for PrimaryCoefficients {}
// SAFETY: as for `Sync for PrimaryCoefficients`: copies of a shared handle
// only read.
unsafe impl Send for PrimaryCoefficientsRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for PrimaryCoefficientsRef<'_> {}
// SAFETY: `&PrimaryCoefficientsMut` reaches the primaries only through
// `as_ref`, a shared handle; the setters and the exclusive primary handles
// need `&mut`.
unsafe impl Sync for PrimaryCoefficientsMut<'_> {}

impl PrimaryCoefficients {
    /// The primaries `r`, `g` and `b`, as given.
    #[inline]
    #[must_use]
    pub const fn new(r: CieXy, g: CieXy, b: CieXy) -> Self {
        Self::from_raw(ffi::AVPrimaryCoefficients {
            r: r.into_raw(),
            g: g.into_raw(),
            b: b.into_raw(),
        })
    }

    /// The primaries C encodes as `raw`, whatever their coordinates.
    #[inline]
    #[must_use]
    pub const fn from_raw(raw: ffi::AVPrimaryCoefficients) -> Self {
        Self(raw, core::marker::PhantomData)
    }

    /// The C encoding of these primaries, for passing them or storing them
    /// in a C struct by value.
    #[inline]
    #[must_use]
    pub const fn into_raw(self) -> ffi::AVPrimaryCoefficients {
        self.0
    }
}

/// Three chromaticities of `0/0` coordinates: the primaries of an
/// unspecified gamut.
impl Default for PrimaryCoefficients {
    #[inline]
    fn default() -> Self {
        Self::zeroed()
    }
}

impl From<ffi::AVPrimaryCoefficients> for PrimaryCoefficients {
    #[inline]
    fn from(raw: ffi::AVPrimaryCoefficients) -> Self {
        Self::from_raw(raw)
    }
}

impl From<PrimaryCoefficients> for ffi::AVPrimaryCoefficients {
    #[inline]
    fn from(p: PrimaryCoefficients) -> Self {
        p.into_raw()
    }
}

/// Primary by primary, each by [`CieXy`]'s equality: coordinates compare by
/// [`Rational`]'s value equality, so `1/2` equals `2/4`, and a `0/0`
/// coordinate equals nothing, itself included.
impl PartialEq for PrimaryCoefficients {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (self.as_ref(), other.as_ref());
        a.r().get() == b.r().get() && a.g().get() == b.g().get() && a.b().get() == b.b().get()
    }
}

impl fmt::Debug for PrimaryCoefficients {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl fmt::Debug for PrimaryCoefficientsRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PrimaryCoefficients")
            .field("r", &self.r())
            .field("g", &self.g())
            .field("b", &self.b())
            .finish()
    }
}

impl<'a> PrimaryCoefficientsRef<'a> {
    /// Field: AVPrimaryCoefficients.r
    ///
    /// The red primary, read through a shared handle over the field that
    /// lives as long as this one.
    ///
    /// # Panics
    ///
    /// Never: the place of a field of a live struct is not NULL.
    #[must_use]
    pub fn r(&self) -> CieXyRef<'a> {
        // SAFETY: the handle's pointer is to a live object, as the operation below relies on,
        // so the place of its `r` field is in bounds; nothing is accessed here.
        let r_ptr = unsafe { addr_of!((*self.as_ptr()).r) };
        // SAFETY: the `r` field, projected without a reference through the
        // shared handle's pointer to a live, initialised
        // `AVPrimaryCoefficients`, is a live, initialised `AVCIExy` for as
        // long as the primaries are borrowed (`'a`); `CieXyRef` only reads
        // through the pointer, and a field address is never NULL.
        unsafe { CieXyRef::from_ptr(r_ptr.cast_mut()) }.expect("a field address is not NULL")
    }

    /// Field: AVPrimaryCoefficients.g
    ///
    /// The green primary, read through a shared handle over the field that
    /// lives as long as this one.
    ///
    /// # Panics
    ///
    /// Never: the place of a field of a live struct is not NULL.
    #[must_use]
    pub fn g(&self) -> CieXyRef<'a> {
        // SAFETY: the handle's pointer is to a live object, as the operation below relies on,
        // so the place of its `g` field is in bounds; nothing is accessed here.
        let g_ptr = unsafe { addr_of!((*self.as_ptr()).g) };
        // SAFETY: as for `r`, for the `g` field.
        unsafe { CieXyRef::from_ptr(g_ptr.cast_mut()) }.expect("a field address is not NULL")
    }

    /// Field: AVPrimaryCoefficients.b
    ///
    /// The blue primary, read through a shared handle over the field that
    /// lives as long as this one.
    ///
    /// # Panics
    ///
    /// Never: the place of a field of a live struct is not NULL.
    #[must_use]
    pub fn b(&self) -> CieXyRef<'a> {
        // SAFETY: the handle's pointer is to a live object, as the operation below relies on,
        // so the place of its `b` field is in bounds; nothing is accessed here.
        let b_ptr = unsafe { addr_of!((*self.as_ptr()).b) };
        // SAFETY: as for `r`, for the `b` field.
        unsafe { CieXyRef::from_ptr(b_ptr.cast_mut()) }.expect("a field address is not NULL")
    }

    /// A copy of all three primaries, independent of the place they were
    /// read from.
    #[must_use]
    pub fn get(&self) -> PrimaryCoefficients {
        // SAFETY: a read of the whole live, initialised
        // `AVPrimaryCoefficients` through the shared handle's pointer; it is
        // twelve `int`s, so the byte copy is a complete, independent value.
        PrimaryCoefficients::from_raw(unsafe { self.as_ptr().read() })
    }
}

impl PrimaryCoefficientsMut<'_> {
    /// Field: AVPrimaryCoefficients.r
    ///
    /// The red primary, written through an exclusive handle over the field,
    /// which borrows these primaries exclusively while it lives.
    ///
    /// # Panics
    ///
    /// Never: the place of a field of a live struct is not NULL.
    #[must_use]
    pub fn r_mut(&mut self) -> CieXyMut<'_> {
        // SAFETY: the handle's pointer is to a live object, as the operation below relies on,
        // so the place of its `r` field is in bounds; nothing is accessed here.
        let r_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).r) };
        // SAFETY: the `r` field, projected without a reference through the
        // exclusive handle's pointer to a live `AVPrimaryCoefficients`, is a
        // live `AVCIExy` that only the result reaches while it borrows `self`
        // exclusively; a field address is never NULL.
        unsafe { CieXyMut::from_ptr(r_ptr) }.expect("a field address is not NULL")
    }

    /// Field: AVPrimaryCoefficients.g
    ///
    /// The green primary, written through an exclusive handle over the
    /// field, which borrows these primaries exclusively while it lives.
    ///
    /// # Panics
    ///
    /// Never: the place of a field of a live struct is not NULL.
    #[must_use]
    pub fn g_mut(&mut self) -> CieXyMut<'_> {
        // SAFETY: the handle's pointer is to a live object, as the operation below relies on,
        // so the place of its `g` field is in bounds; nothing is accessed here.
        let g_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).g) };
        // SAFETY: as for `r_mut`, for the `g` field.
        unsafe { CieXyMut::from_ptr(g_ptr) }.expect("a field address is not NULL")
    }

    /// Field: AVPrimaryCoefficients.b
    ///
    /// The blue primary, written through an exclusive handle over the field,
    /// which borrows these primaries exclusively while it lives.
    ///
    /// # Panics
    ///
    /// Never: the place of a field of a live struct is not NULL.
    #[must_use]
    pub fn b_mut(&mut self) -> CieXyMut<'_> {
        // SAFETY: the handle's pointer is to a live object, as the operation below relies on,
        // so the place of its `b` field is in bounds; nothing is accessed here.
        let b_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).b) };
        // SAFETY: as for `r_mut`, for the `b` field.
        unsafe { CieXyMut::from_ptr(b_ptr) }.expect("a field address is not NULL")
    }

    /// Field: AVPrimaryCoefficients.r
    ///
    /// Sets the red primary, leaving the others as they are.
    pub fn set_r(&mut self, r: CieXy) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `r` field is in bounds; nothing is accessed here.
        let r_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).r) };
        // SAFETY: a write of the `r` field projected through the exclusive
        // handle's pointer to a live `AVPrimaryCoefficients`; the old value
        // owns nothing to leak.
        unsafe { r_ptr.write(r.into_raw()) }
    }

    /// Field: AVPrimaryCoefficients.g
    ///
    /// Sets the green primary, leaving the others as they are.
    pub fn set_g(&mut self, g: CieXy) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `g` field is in bounds; nothing is accessed here.
        let g_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).g) };
        // SAFETY: as in `set_r`, for the `g` field.
        unsafe { g_ptr.write(g.into_raw()) }
    }

    /// Field: AVPrimaryCoefficients.b
    ///
    /// Sets the blue primary, leaving the others as they are.
    pub fn set_b(&mut self, b: CieXy) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `b` field is in bounds; nothing is accessed here.
        let b_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).b) };
        // SAFETY: as in `set_r`, for the `b` field.
        unsafe { b_ptr.write(b.into_raw()) }
    }

    /// Overwrites all three primaries with `p`'s.
    pub fn set(&mut self, p: PrimaryCoefficients) {
        // SAFETY: a write of the whole `AVPrimaryCoefficients` through the
        // exclusive handle's pointer to a live one; the old value owns
        // nothing to leak.
        unsafe { self.as_mut_ptr().write(p.into_raw()) }
    }
}

define_ctype!(
    /// Wraps: AVColorPrimariesDesc
    ///
    /// The complete description of a colour gamut: its white point and the
    /// chromaticities of its red, green and blue primaries, all in the CIE
    /// 1931 xy plane. Its getters are on [`ColorPrimariesDescriptorRef`]
    /// (`as_ref()`), its setters on [`ColorPrimariesDescriptorMut`]
    /// (`as_mut()`).
    ///
    /// C hands these out as shared borrows of its static, never-written
    /// tables ([`ColorPrimaries::descriptor`]), reads them field by field
    /// ([`identify`](ColorPrimariesDescriptorRef::identify)) and embeds them
    /// by value in Dolby Vision metadata (`AVDOVIDmLevel9`,
    /// `AVDOVIDmLevel10`); nothing allocates, frees or clones one on its own,
    /// so it is a value type with no owner, built by
    /// [`ColorPrimariesDescriptor::new`],
    /// [`from_raw`](ColorPrimariesDescriptor::from_raw) or `zeroed()`. No
    /// coordinate is checked, by C or here: each takes any pair of `int`s, as
    /// a [`Rational`] does.
    #[derive(Clone, Copy)]
    ColorPrimariesDescriptor,
    ColorPrimariesDescriptorRef,
    ColorPrimariesDescriptorMut,
    ffi::AVColorPrimariesDesc
);

// SAFETY: all-zero is a white point and three primaries of `0/0`
// coordinates, the description C's own tables hold for every gamut they leave
// unspecified (and skip on `prim.r.x.num == 0`); every accessor takes it as
// the sixteen integers it is, and the type has no teardown to reject it.
unsafe impl CZeroable for ColorPrimariesDescriptor {}

// SAFETY: an `AVColorPrimariesDesc` is 64 bytes of plain data, an `AVCIExy`
// and an `AVPrimaryCoefficients`, eight `AVRational`s in all, with no
// pointer, sub-allocation or tie to the thread that wrote them, so a value, or
// an exclusive handle to one, moves between threads as an `[i32; 16]` does.
unsafe impl Send for ColorPrimariesDescriptor {}
// SAFETY: as for `ColorPrimariesDescriptor`: the handle is the only access to
// the description, and the white point and primaries handles it lends borrow
// it.
unsafe impl Send for ColorPrimariesDescriptorMut<'_> {}

// SAFETY: nothing reachable through a shared handle writes:
// `ColorPrimariesDescriptorRef` has getters only, whose `CieXyRef` and
// `PrimaryCoefficientsRef` have getters only too, and the struct has no
// interior mutability, so concurrent shared access reads unchanging bytes, as
// through `&[i32; 16]`.
unsafe impl Sync for ColorPrimariesDescriptor {}
// SAFETY: as for `Sync for ColorPrimariesDescriptor`: copies of a shared
// handle only read.
unsafe impl Send for ColorPrimariesDescriptorRef<'_> {}
// SAFETY: as above.
unsafe impl Sync for ColorPrimariesDescriptorRef<'_> {}
// SAFETY: `&ColorPrimariesDescriptorMut` reaches the description only through
// `as_ref`, a shared handle; the setters and the exclusive field handles need
// `&mut`.
unsafe impl Sync for ColorPrimariesDescriptorMut<'_> {}

impl ColorPrimariesDescriptor {
    /// The gamut of white point `white_point` and primaries `primaries`, as
    /// given.
    #[inline]
    #[must_use]
    pub const fn new(white_point: WhitepointCoefficients, primaries: PrimaryCoefficients) -> Self {
        Self::from_raw(ffi::AVColorPrimariesDesc {
            wp: white_point.into_raw(),
            prim: primaries.into_raw(),
        })
    }

    /// The gamut C encodes as `raw`, whatever its coordinates.
    #[inline]
    #[must_use]
    pub const fn from_raw(raw: ffi::AVColorPrimariesDesc) -> Self {
        Self(raw, core::marker::PhantomData)
    }

    /// The C encoding of this gamut, for passing it or storing it in a C
    /// struct by value.
    #[inline]
    #[must_use]
    pub const fn into_raw(self) -> ffi::AVColorPrimariesDesc {
        self.0
    }
}

/// A white point and primaries of `0/0` coordinates: an unspecified gamut.
impl Default for ColorPrimariesDescriptor {
    #[inline]
    fn default() -> Self {
        Self::zeroed()
    }
}

impl From<ffi::AVColorPrimariesDesc> for ColorPrimariesDescriptor {
    #[inline]
    fn from(raw: ffi::AVColorPrimariesDesc) -> Self {
        Self::from_raw(raw)
    }
}

impl From<ColorPrimariesDescriptor> for ffi::AVColorPrimariesDesc {
    #[inline]
    fn from(d: ColorPrimariesDescriptor) -> Self {
        d.into_raw()
    }
}

/// White point and primaries by [`CieXy`]'s and [`PrimaryCoefficients`]'
/// equality: coordinates compare by [`Rational`]'s value equality, so `1/2`
/// equals `2/4`, and a `0/0` coordinate equals nothing, itself included. This
/// is exact equality, not the 1/1000 tolerance within which C identifies a
/// gamut (`av_csp_primaries_id_from_desc`).
impl PartialEq for ColorPrimariesDescriptor {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (self.as_ref(), other.as_ref());
        a.white_point().get() == b.white_point().get() && a.primaries().get() == b.primaries().get()
    }
}

impl fmt::Debug for ColorPrimariesDescriptor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl fmt::Debug for ColorPrimariesDescriptorRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ColorPrimariesDescriptor")
            .field("white_point", &self.white_point())
            .field("primaries", &self.primaries())
            .finish()
    }
}

impl<'a> ColorPrimariesDescriptorRef<'a> {
    /// Field: AVColorPrimariesDesc.wp
    ///
    /// The white point, read through a shared handle over the field that
    /// lives as long as this one.
    ///
    /// # Panics
    ///
    /// Never: the place of a field of a live struct is not NULL.
    #[must_use]
    pub fn white_point(&self) -> CieXyRef<'a> {
        // SAFETY: the handle's pointer is to a live object, as the operation below relies on,
        // so the place of its `wp` field is in bounds; nothing is accessed here.
        let wp_ptr = unsafe { addr_of!((*self.as_ptr()).wp) };
        // SAFETY: the `wp` field, projected without a reference through the
        // shared handle's pointer to a live, initialised
        // `AVColorPrimariesDesc`, is a live, initialised `AVCIExy` for as
        // long as the description is borrowed (`'a`); `CieXyRef` only reads
        // through the pointer, and a field address is never NULL.
        unsafe { CieXyRef::from_ptr(wp_ptr.cast_mut()) }.expect("a field address is not NULL")
    }

    /// Field: AVColorPrimariesDesc.prim
    ///
    /// The red, green and blue primaries, read through a shared handle over
    /// the field that lives as long as this one.
    ///
    /// # Panics
    ///
    /// Never: the place of a field of a live struct is not NULL.
    #[must_use]
    pub fn primaries(&self) -> PrimaryCoefficientsRef<'a> {
        // SAFETY: the handle's pointer is to a live object, as the operation below relies on,
        // so the place of its `prim` field is in bounds; nothing is accessed here.
        let prim_ptr = unsafe { addr_of!((*self.as_ptr()).prim) };
        // SAFETY: as in `white_point`, for the `prim` field, a live,
        // initialised `AVPrimaryCoefficients` that `PrimaryCoefficientsRef`
        // only reads.
        unsafe { PrimaryCoefficientsRef::from_ptr(prim_ptr.cast_mut()) }
            .expect("a field address is not NULL")
    }

    /// A copy of the whole description, independent of the place it was
    /// read from.
    #[must_use]
    pub fn get(&self) -> ColorPrimariesDescriptor {
        // SAFETY: a read of the whole live, initialised
        // `AVColorPrimariesDesc` through the shared handle's pointer; it is
        // sixteen `int`s, so the byte copy is a complete, independent value.
        ColorPrimariesDescriptor::from_raw(unsafe { self.as_ptr().read() })
    }
}

impl ColorPrimariesDescriptorMut<'_> {
    /// Field: AVColorPrimariesDesc.wp
    ///
    /// The white point, written through an exclusive handle over the field,
    /// which borrows this description exclusively while it lives.
    ///
    /// # Panics
    ///
    /// Never: the place of a field of a live struct is not NULL.
    #[must_use]
    pub fn white_point_mut(&mut self) -> CieXyMut<'_> {
        // SAFETY: the handle's pointer is to a live object, as the operation below relies on,
        // so the place of its `wp` field is in bounds; nothing is accessed here.
        let wp_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).wp) };
        // SAFETY: the `wp` field, projected without a reference through the
        // exclusive handle's pointer to a live `AVColorPrimariesDesc`, is a
        // live `AVCIExy` that only the result reaches while it borrows `self`
        // exclusively; a field address is never NULL.
        unsafe { CieXyMut::from_ptr(wp_ptr) }.expect("a field address is not NULL")
    }

    /// Field: AVColorPrimariesDesc.prim
    ///
    /// The primaries, written through an exclusive handle over the field,
    /// which borrows this description exclusively while it lives.
    ///
    /// # Panics
    ///
    /// Never: the place of a field of a live struct is not NULL.
    #[must_use]
    pub fn primaries_mut(&mut self) -> PrimaryCoefficientsMut<'_> {
        // SAFETY: the handle's pointer is to a live object, as the operation below relies on,
        // so the place of its `prim` field is in bounds; nothing is accessed here.
        let prim_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).prim) };
        // SAFETY: as in `white_point_mut`, for the `prim` field, a live
        // `AVPrimaryCoefficients`.
        unsafe { PrimaryCoefficientsMut::from_ptr(prim_ptr) }.expect("a field address is not NULL")
    }

    /// Field: AVColorPrimariesDesc.wp
    ///
    /// Sets the white point, leaving the primaries as they are.
    pub fn set_white_point(&mut self, white_point: WhitepointCoefficients) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `wp` field is in bounds; nothing is accessed here.
        let wp_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).wp) };
        // SAFETY: a write of the `wp` field projected through the exclusive
        // handle's pointer to a live `AVColorPrimariesDesc`; the old value
        // owns nothing to leak.
        unsafe { wp_ptr.write(white_point.into_raw()) }
    }

    /// Field: AVColorPrimariesDesc.prim
    ///
    /// Sets the primaries, leaving the white point as it is.
    pub fn set_primaries(&mut self, primaries: PrimaryCoefficients) {
        // SAFETY: the handle's pointer is to a live object, as the access below relies on,
        // so the place of its `prim` field is in bounds; nothing is accessed here.
        let prim_ptr = unsafe { addr_of_mut!((*self.as_mut_ptr()).prim) };
        // SAFETY: as in `set_white_point`, for the `prim` field.
        unsafe { prim_ptr.write(primaries.into_raw()) }
    }

    /// Overwrites the white point and the primaries with `d`'s.
    pub fn set(&mut self, d: ColorPrimariesDescriptor) {
        // SAFETY: a write of the whole `AVColorPrimariesDesc` through the
        // exclusive handle's pointer to a live one; the old value owns
        // nothing to leak.
        unsafe { self.as_mut_ptr().write(d.into_raw()) }
    }
}

/// A gamma C reports, or `None` for its 0.0, which no characteristic has.
fn gamma(g: f64) -> Option<f64> {
    (g != 0.0).then_some(g)
}

impl ColorTransfer {
    /// Wraps: `av_csp_approximate_trc_gamma`
    ///
    /// The exponent of the pure power function that approximates this
    /// characteristic's OETF, after Apple's TN2257: 2.2 for sRGB (not
    /// 0.45455), 1.961 for BT.709. `None` for a characteristic C cannot
    /// reasonably match, such as the logarithmic and HDR ones.
    #[must_use]
    pub fn approximate_trc_gamma(self) -> Option<f64> {
        // SAFETY: `av_csp_approximate_trc_gamma` takes the enum by value and
        // reads its static table only for an index inside it.
        gamma(unsafe { ffi::av_csp_approximate_trc_gamma(self.into_raw()) })
    }

    /// Wraps: `av_csp_approximate_eotf_gamma`
    ///
    /// The exponent `gamma` of the pure power EOTF approximating this
    /// characteristic, `EOTF(v) = (Lw - Lb) * v^gamma + Lb`: 2.2 for BT.709
    /// and sRGB. `None` for a characteristic C cannot reasonably match.
    #[must_use]
    pub fn approximate_eotf_gamma(self) -> Option<f64> {
        // SAFETY: as in `approximate_trc_gamma`.
        gamma(unsafe { ffi::av_csp_approximate_eotf_gamma(self.into_raw()) })
    }

    /// Wraps: `av_csp_trc_func_from_id`
    ///
    /// The function applying this characteristic to linear light: its OETF,
    /// or its inverse EOTF for SMPTE ST 2084 (PQ). `None` for the
    /// unspecified and reserved values and any C does not know.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::pixfmt::ColorTransfer;
    /// let oetf = ColorTransfer::LINEAR.trc_function().unwrap();
    /// assert_eq!(oetf.call(0.25), 0.25);
    /// assert!(ColorTransfer::UNSPECIFIED.trc_function().is_none());
    /// ```
    #[must_use]
    pub fn trc_function(self) -> Option<TrcFunction> {
        // SAFETY: `av_csp_trc_func_from_id` takes the enum by value and
        // returns NULL or one of `csp.c`'s transfer functions, the only
        // functions a `TrcFunction` may hold.
        unsafe { ffi::av_csp_trc_func_from_id(self.into_raw()) }.map(TrcFunction)
    }

    /// Wraps: `av_csp_trc_func_inv_from_id`
    ///
    /// The mathematical inverse of [`trc_function`](Self::trc_function),
    /// mapping gamma-encoded light back to linear; `None` exactly where that
    /// is.
    #[must_use]
    pub fn inverse_trc_function(self) -> Option<TrcFunction> {
        // SAFETY: as in `trc_function`, for the inverse table.
        unsafe { ffi::av_csp_trc_func_inv_from_id(self.into_raw()) }.map(TrcFunction)
    }

    /// Wraps: `av_csp_itu_eotf`
    ///
    /// The ITU EOTF of this characteristic, mapping a signal level in
    /// `[0, 1]` to display luminance in nits, per RGB channel (CIE XYZ for
    /// SMPTE ST 428). `None` where no EOTF is defined: the unspecified and
    /// reserved values, the logarithmic encodings, the extended range
    /// ([`V_LOG`](Self::V_LOG)) and any value C does not know.
    #[must_use]
    pub fn eotf(self) -> Option<EotfFunction> {
        // SAFETY: `av_csp_itu_eotf` takes the enum by value and returns NULL
        // or one of `csp.c`'s EOTFs, the only functions an `EotfFunction`
        // may hold.
        unsafe { ffi::av_csp_itu_eotf(self.into_raw()) }.map(EotfFunction)
    }

    /// Wraps: `av_csp_itu_eotf_inv`
    ///
    /// The mathematical inverse of [`eotf`](Self::eotf), mapping display
    /// luminance back to a signal level; `None` exactly where that is.
    #[must_use]
    pub fn inverse_eotf(self) -> Option<EotfFunction> {
        // SAFETY: as in `eotf`, for the inverse table.
        unsafe { ffi::av_csp_itu_eotf_inv(self.into_raw()) }.map(EotfFunction)
    }
}

impl ColorSpace {
    /// Wraps: `av_csp_luma_coeffs_from_avcsp`
    ///
    /// The luma coefficients of this colour space, borrowed from C's static,
    /// never-written table for the whole program: `0.2126`, `0.7152` and
    /// `0.0722` for [`BT709`](Self::BT709), all `1` for
    /// [`RGB`](Self::RGB). `None` for a colour space C has none for, such as
    /// the unspecified and reserved values, the chroma-derived and `ICtCp`
    /// spaces, and every value C does not know.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::pixfmt::ColorSpace;
    /// let bt709 = ColorSpace::BT709.luma_coefficients().unwrap();
    /// assert_eq!((bt709.cg().num(), bt709.cg().den()), (71520, 100000));
    /// assert!(ColorSpace::UNSPECIFIED.luma_coefficients().is_none());
    /// ```
    #[must_use]
    pub fn luma_coefficients(self) -> Option<LumaCoefficientsRef<'static>> {
        // For the unsafe operations below: `av_csp_luma_coeffs_from_avcsp` takes the enum by value,
        // checks it against its table, and returns NULL or the address of an
        // entry of `csp.c`'s static `const` table, which lives, unwritten,
        // for the whole program; a shared `'static` handle only reads it.
        {
            // SAFETY: as the comment above this block states.
            let p = unsafe { ffi::av_csp_luma_coeffs_from_avcsp(self.into_raw()) };
            // SAFETY: as the comment above this block states.
            unsafe { LumaCoefficientsRef::from_ptr(p.cast_mut()) }
        }
    }
}

impl ColorPrimaries {
    /// Wraps: `av_csp_primaries_desc_from_id`
    ///
    /// The complete description of this gamut, its white point and the
    /// chromaticities of its primaries, borrowed from C's static,
    /// never-written tables for the whole program: D65 (`0.3127`, `0.3290`)
    /// and red at (`0.64`, `0.33`) for [`BT709`](Self::BT709). `None` for
    /// primaries C has no description of: the unspecified and reserved
    /// values, and every value C does not know.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::pixfmt::ColorPrimaries;
    /// let bt709 = ColorPrimaries::BT709.descriptor().unwrap();
    /// let wp = bt709.white_point();
    /// assert_eq!((wp.x().num(), wp.x().den()), (31270, 100000));
    /// assert_eq!(bt709.identify(), Some(ColorPrimaries::BT709));
    /// assert!(ColorPrimaries::UNSPECIFIED.descriptor().is_none());
    /// ```
    #[must_use]
    pub fn descriptor(self) -> Option<ColorPrimariesDescriptorRef<'static>> {
        // For the unsafe operations below: `av_csp_primaries_desc_from_id` takes the enum by value,
        // checks it against both of its tables, and returns NULL or the
        // address of an entry of `csp.c`'s static `const` tables, which
        // lives, unwritten, for the whole program; a shared `'static` handle
        // only reads it.
        {
            // SAFETY: as the comment above this block states.
            let p = unsafe { ffi::av_csp_primaries_desc_from_id(self.into_raw()) };
            // SAFETY: as the comment above this block states.
            unsafe { ColorPrimariesDescriptorRef::from_ptr(p.cast_mut()) }
        }
    }
}

impl ColorPrimariesDescriptorRef<'_> {
    /// Wraps: `av_csp_primaries_id_from_desc`
    ///
    /// The primaries C describes within 1/1000 of this gamut: the first entry
    /// of C's table, in [`ColorPrimaries`] order, for which the sum of the
    /// absolute differences of all eight coordinates is below 1/1000. `None`
    /// when no entry is that close.
    ///
    /// The sum is C's, in `int` rationals: each difference and each partial
    /// sum is reduced to the closest rational whose parts fit an `int`
    /// (`av_sub_q`, `av_add_q`), which is exact while the denominators stay
    /// small (the tables use 100000 and 3) and approximate, to about
    /// `2e-13`, once a coordinate's denominator makes the exact one exceed
    /// `i32::MAX`. A description that close to the tolerance is judged as C
    /// judges it, not as exact arithmetic would: `311418082/999095547` as
    /// BT.709's white point `x` lies `1e-13` inside 1/1000 of it, yet
    /// reduces to exactly 1/1000 and identifies as nothing.
    ///
    /// SMPTE 240M's description identifies as
    /// [`SMPTE170M`](ColorPrimaries::SMPTE170M), whose entry is identical
    /// and comes first. Only H.273's range is searched, not FFmpeg's
    /// extensions: [`V_GAMUT`](ColorPrimaries::V_GAMUT)'s own description
    /// identifies as nothing.
    ///
    /// A description with a zero denominator is no chromaticity and matches
    /// nothing here. C agrees for a single infinite coordinate, whose
    /// difference stays infinite, but sums a `0/0` coordinate, or two
    /// infinite ones, into a `0/0` difference that it compares unordered
    /// with the tolerance yet reads as within it, and reports
    /// [`BT709`](ColorPrimaries::BT709) for such a description, the
    /// all-zero, unspecified one included; this diverges from C there.
    ///
    /// ```
    /// use ffmpeg_bindings::libavutil::libavutil::csp::ColorPrimariesDescriptor;
    /// use ffmpeg_bindings::libavutil::libavutil::pixfmt::ColorPrimaries;
    /// let p3 = ColorPrimaries::SMPTE432.descriptor().unwrap();
    /// assert_eq!(p3.identify(), Some(ColorPrimaries::SMPTE432));
    /// assert_eq!(
    ///     ColorPrimariesDescriptor::default().as_ref().identify(),
    ///     None
    /// );
    /// ```
    #[must_use]
    pub fn identify(&self) -> Option<ColorPrimaries> {
        let d = self.get().into_raw();
        let points = [d.wp, d.prim.r, d.prim.g, d.prim.b];
        if points.iter().any(|xy| xy.x.den == 0 || xy.y.den == 0) {
            return None;
        }
        // SAFETY: `av_csp_primaries_id_from_desc` only reads the live,
        // initialised `AVColorPrimariesDesc` behind the shared handle's
        // pointer, during the call. Its differences against table
        // coordinates and their sums stay within `av_sub_q`'s and
        // `av_add_q`'s `int64_t` range for every `int`, and `av_reduce`
        // keeps each numerator's magnitude within `INT_MAX` for its `abs`.
        let id = unsafe { ffi::av_csp_primaries_id_from_desc(self.as_ptr()) };
        let id = ColorPrimaries::from_raw(id);
        (id != ColorPrimaries::UNSPECIFIED).then_some(id)
    }
}

#[cfg(test)]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#[expect(
    clippy::items_after_statements,
    reason = "each test is one scenario, its helpers and steps kept where they are used"
)]
mod unit_tests {
    use super::*;

    /// Every transfer characteristic C may hand a function out for, past the
    /// end of both ranges included.
    fn transfers() -> impl Iterator<Item = ffi::AVColorTransferCharacteristic> {
        let nb = ffi::AVColorTransferCharacteristic::AVCOL_TRC_NB.0;
        let ext = ffi::AVColorTransferCharacteristic::AVCOL_TRC_EXT_BASE.0;
        let ext_nb = ffi::AVColorTransferCharacteristic::AVCOL_TRC_EXT_NB.0;
        (0..=nb)
            .chain(ext..=ext_nb)
            .map(ffi::AVColorTransferCharacteristic)
    }

    /// The transfer functions C hands out, with the raw pointers.
    fn trc_functions() -> Vec<(TrcFunction, unsafe extern "C" fn(f64) -> f64)> {
        let mut out = Vec::new();
        for trc in transfers() {
            // SAFETY: both getters take any value and return NULL or a
            // `csp.c` transfer function.
            let fns = [unsafe { ffi::av_csp_trc_func_from_id(trc) }, unsafe {
                ffi::av_csp_trc_func_inv_from_id(trc)
            }];
            out.extend(fns.into_iter().flatten().map(|f| (TrcFunction(f), f)));
        }
        out
    }

    /// The EOTFs C hands out, with the raw pointers.
    fn eotf_functions() -> Vec<(EotfFunction, unsafe extern "C" fn(f64, f64, *mut f64))> {
        let mut out = Vec::new();
        for trc in transfers() {
            // SAFETY: both getters take any value and return NULL or a
            // `csp.c` EOTF.
            let fns = [unsafe { ffi::av_csp_itu_eotf(trc) }, unsafe {
                ffi::av_csp_itu_eotf_inv(trc)
            }];
            out.extend(fns.into_iter().flatten().map(|f| (EotfFunction(f), f)));
        }
        out
    }

    const INPUTS: [f64; 12] = [
        0.0,
        -0.0,
        1.0,
        0.5,
        0.018,
        -0.25,
        1.5,
        1e-300,
        1e300,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
    ];

    #[test]
    fn handles_are_plain_shareable_values() {
        fn assert_traits<T: Copy + Send + Sync + fmt::Debug + 'static>() {}
        assert_traits::<TrcFunction>();
        assert_traits::<EotfFunction>();
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn trc_handles_call_the_c_function() {
        let fns = trc_functions();
        // 16 + 1 forward and 16 + 1 inverse functions, less the
        // unspecified/reserved slots.
        assert!(fns.len() >= 28, "{}", fns.len());
        for (handle, raw) in fns {
            for x in INPUTS {
                // SAFETY: a `csp.c` transfer function takes any `f64`.
                let c = unsafe { raw(x) };
                assert_eq!(handle.call(x).to_bits(), c.to_bits(), "{handle:?} {x}");
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn eotf_handles_call_the_c_function_in_place() {
        let fns = eotf_functions();
        assert!(fns.len() >= 24, "{}", fns.len());
        let displays = [
            (1000.0, 0.005),
            (100.0, 0.0),
            (48.0, 0.02),
            (0.0, 0.0),
            (-1.0, f64::NAN),
        ];
        for (handle, raw) in fns {
            for (lw, lb) in displays {
                for i in 0..INPUTS.len() {
                    let input = [
                        INPUTS[i],
                        INPUTS[(i + 3) % INPUTS.len()],
                        INPUTS[(i + 7) % INPUTS.len()],
                    ];
                    // An exactly sized heap array: an access past it is an
                    // ASan report.
                    let mut rust = Box::new(input);
                    let mut c = input;
                    handle.call(lw, lb, &mut rust);
                    // SAFETY: a `csp.c` EOTF reads and writes `c[0..3]`.
                    unsafe { raw(lw, lb, c.as_mut_ptr()) };
                    assert_eq!(
                        rust.map(f64::to_bits),
                        c.map(f64::to_bits),
                        "{handle:?} {lw} {lb} {input:?}"
                    );
                }
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn handles_cross_threads() {
        let (trc, eotf) = (trc_functions()[0].0, eotf_functions()[0].0);
        let expect = (trc.call(0.5), {
            let mut c = [0.25, 0.5, 0.75];
            eotf.call(100.0, 0.1, &mut c);
            c
        });
        std::thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(move || {
                    let mut c = [0.25, 0.5, 0.75];
                    eotf.call(100.0, 0.1, &mut c);
                    assert_eq!((trc.call(0.5), c), expect);
                });
            }
        });
    }

    fn q(num: i32, den: i32) -> ffi::AVRational {
        ffi::AVRational { num, den }
    }

    /// A rational built without a C call, so the tests using it also run
    /// under Miri.
    fn r(num: i32, den: i32) -> Rational {
        Rational::from_raw(q(num, den))
    }

    fn parts(xy: CieXyRef<'_>) -> [i32; 4] {
        let (x, y) = (xy.x(), xy.y());
        [x.num(), x.den(), y.num(), y.den()]
    }

    #[test]
    fn cie_xy_new_from_raw_and_into_raw_keep_every_part() {
        let raw = ffi::AVCIExy {
            x: q(3127, 10000),
            y: q(i32::MIN, -1),
        };
        let a = CieXy::from_raw(raw);
        let b = CieXy::new(Rational::from_raw(raw.x), Rational::from_raw(raw.y));
        let c: CieXy = raw.into();
        for v in [a, b, c] {
            assert_eq!(parts(v.as_ref()), [3127, 10000, i32::MIN, -1]);
            let back: ffi::AVCIExy = v.into();
            assert_eq!(
                [back.x.num, back.x.den, back.y.num, back.y.den],
                [3127, 10000, i32::MIN, -1]
            );
        }
    }

    #[test]
    fn cie_xy_default_and_zeroed_are_unspecified() {
        assert_eq!(parts(CieXy::default().as_ref()), [0; 4]);
        assert_eq!(parts(CieXy::zeroed().as_ref()), [0; 4]);
        assert_eq!(size_of::<CieXy>(), 16);
        assert_eq!(align_of::<CieXy>(), align_of::<ffi::AVCIExy>());
    }

    #[test]
    fn cie_xy_debug_shows_both_coordinates() {
        let v = CieXy::from_raw(ffi::AVCIExy {
            x: q(1, 3),
            y: q(-1, 0),
        });
        assert_eq!(
            format!("{v:?}"),
            "CieXy { x: Rational { num: 1, den: 3 }, y: Rational { num: -1, den: 0 } }"
        );
        assert_eq!(format!("{:?}", v.as_ref()), format!("{v:?}"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn cie_xy_equality_is_by_value_per_coordinate() {
        let v = |a, b, c, d| {
            CieXy::from_raw(ffi::AVCIExy {
                x: q(a, b),
                y: q(c, d),
            })
        };
        assert_eq!(v(1, 2, 3, 10), v(2, 4, 30, 100));
        assert_ne!(v(1, 2, 3, 10), v(1, 2, 3, 11));
        assert_ne!(v(1, 2, 3, 10), v(1, 3, 3, 10));
        assert_eq!(v(1, 0, -1, 0), v(7, 0, -7, 0));
        // `0/0` is unordered, like NaN, so an unspecified chromaticity
        // equals nothing, itself included.
        let unset = CieXy::default();
        let reflexive = unset == unset;
        assert!(!reflexive);
        assert_ne!(v(0, 0, 1, 2), v(0, 0, 1, 2));
    }

    #[test]
    fn cie_xy_exclusive_handle_writes_each_coordinate() {
        let mut v = CieXy::default();
        let mut m = v.as_mut();
        m.set_x(r(64, 100));
        assert_eq!(parts(m.as_ref()), [64, 100, 0, 0]);
        m.set_y(r(33, 100));
        assert_eq!(parts(m.as_ref()), [64, 100, 33, 100]);
        m.x_mut().set_den(-1);
        m.y_mut().set_num(i32::MAX);
        assert_eq!(parts(m.as_ref()), [64, -1, i32::MAX, 100]);
        m.set(CieXy::new(r(1, 2), r(3, 4)));
        assert_eq!(parts(v.as_ref()), [1, 2, 3, 4]);
    }

    #[test]
    fn cie_xy_coordinate_handles_address_the_fields_in_place() {
        let v = CieXy::new(r(1, 2), r(3, 4));
        let r = v.as_ref();
        let base = r.as_ptr();
        // SAFETY: only addresses are formed, inside the live value.
        let (fx, fy) = (unsafe { &raw const (*base).x }, unsafe {
            &raw const (*base).y
        });
        assert_eq!(r.x().as_ptr(), fx);
        assert_eq!(r.y().as_ptr(), fy);
        // The coordinate handles copy out of the struct handle and keep its
        // lifetime, not the borrow of the handle that produced them.
        let (x, y) = { (v.as_ref().x(), v.as_ref().y()) };
        assert_eq!([x.num(), x.den(), y.num(), y.den()], [1, 2, 3, 4]);
    }

    #[test]
    fn cie_xy_handles_borrow_a_c_owned_struct() {
        // The shape of a white point or primary embedded in a C gamut
        // description: handles adopted over a struct Rust did not build.
        let mut raw = ffi::AVCIExy {
            x: q(3127, 10000),
            y: q(3290, 10000),
        };
        // SAFETY: `raw` is a live, initialised struct outliving the handle,
        // and no other access to it happens while the handle is used.
        let mut m = unsafe { CieXyMut::from_ptr(&raw mut raw) }.unwrap();
        assert_eq!(parts(m.as_ref()), [3127, 10000, 3290, 10000]);
        m.x_mut().set_num(3128);
        m.set_y(r(1, 3));
        assert_eq!(
            [raw.x.num, raw.x.den, raw.y.num, raw.y.den],
            [3128, 10000, 1, 3]
        );
        // SAFETY: `raw` is live and only read while the shared handle lives.
        let r = unsafe { CieXyRef::from_ptr(&raw mut raw) }.unwrap();
        assert_eq!(parts(r.get().as_ref()), [3128, 10000, 1, 3]);
        // SAFETY: NULL is rejected before anything is borrowed.
        assert!(unsafe { CieXyRef::from_ptr(core::ptr::null_mut()) }.is_none());
        // SAFETY: as above.
        assert!(unsafe { CieXyMut::from_ptr(core::ptr::null_mut()) }.is_none());
    }

    #[test]
    fn cie_xy_whitepoint_alias_is_the_same_type() {
        let wp: WhitepointCoefficients = CieXy::new(r(3127, 10000), r(3290, 10000));
        let xy: CieXy = wp;
        assert_eq!(parts(xy.as_ref()), [3127, 10000, 3290, 10000]);
    }

    #[test]
    fn cie_xy_values_are_plain_shareable_data() {
        fn assert_traits<T: Copy + Send + Sync + Default + fmt::Debug + 'static>() {}
        fn assert_handle<T: Send + Sync>() {}
        assert_traits::<CieXy>();
        assert_handle::<CieXyRef<'static>>();
        assert_handle::<CieXyMut<'static>>();
    }

    fn luma_raw(f: [i32; 6]) -> ffi::AVLumaCoefficients {
        ffi::AVLumaCoefficients {
            cr: ffi::AVRational {
                num: f[0],
                den: f[1],
            },
            cg: ffi::AVRational {
                num: f[2],
                den: f[3],
            },
            cb: ffi::AVRational {
                num: f[4],
                den: f[5],
            },
        }
    }

    fn luma_fields(c: LumaCoefficientsRef<'_>) -> [i32; 6] {
        [
            c.cr().num(),
            c.cr().den(),
            c.cg().num(),
            c.cg().den(),
            c.cb().num(),
            c.cb().den(),
        ]
    }

    #[test]
    fn luma_coefficients_layout_is_the_c_struct() {
        use core::mem::{align_of, size_of};
        assert_eq!(
            size_of::<LumaCoefficients>(),
            size_of::<ffi::AVLumaCoefficients>()
        );
        assert_eq!(size_of::<LumaCoefficients>(), 24);
        assert_eq!(
            align_of::<LumaCoefficients>(),
            align_of::<ffi::AVLumaCoefficients>()
        );
    }

    #[test]
    fn luma_coefficients_new_zeroed_default_and_raw_round_trip() {
        let c = LumaCoefficients::new(
            Rational::from_raw(ffi::AVRational {
                num: 2126,
                den: 10000,
            }),
            Rational::from_raw(ffi::AVRational {
                num: 7152,
                den: 10000,
            }),
            Rational::from_raw(ffi::AVRational {
                num: 722,
                den: 10000,
            }),
        );
        assert_eq!(
            luma_fields(c.as_ref()),
            [2126, 10000, 7152, 10000, 722, 10000]
        );
        assert_eq!(luma_fields(LumaCoefficients::zeroed().as_ref()), [0; 6]);
        assert_eq!(luma_fields(LumaCoefficients::default().as_ref()), [0; 6]);
        let f = [i32::MIN, -1, 0, i32::MAX, 1, 0];
        let back: ffi::AVLumaCoefficients = LumaCoefficients::from(luma_raw(f)).into();
        assert_eq!(
            [
                back.cr.num,
                back.cr.den,
                back.cg.num,
                back.cg.den,
                back.cb.num,
                back.cb.den
            ],
            f
        );
        assert_eq!(
            luma_fields(LumaCoefficients::from_raw(luma_raw(f)).as_ref()),
            f
        );
        // `new` is usable in constants.
        const ZERO: LumaCoefficients = LumaCoefficients::new(
            Rational::from_raw(ffi::AVRational { num: 0, den: 0 }),
            Rational::from_raw(ffi::AVRational { num: 0, den: 0 }),
            Rational::from_raw(ffi::AVRational { num: 0, den: 0 }),
        );
        assert_eq!(luma_fields(ZERO.as_ref()), [0; 6]);
    }

    #[test]
    fn luma_coefficients_field_handles_write_their_field_alone() {
        let mut c = LumaCoefficients::from_raw(luma_raw([1, 2, 3, 4, 5, 6]));
        let mut m = c.as_mut();
        m.cr_mut().set_num(-10);
        assert_eq!(luma_fields(m.as_ref()), [-10, 2, 3, 4, 5, 6]);
        m.cg_mut()
            .set(Rational::from_raw(ffi::AVRational { num: 30, den: 40 }));
        assert_eq!(luma_fields(m.as_ref()), [-10, 2, 30, 40, 5, 6]);
        m.cb_mut().set_den(i32::MIN);
        assert_eq!(luma_fields(m.as_ref()), [-10, 2, 30, 40, 5, i32::MIN]);
        m.set(LumaCoefficients::zeroed());
        assert_eq!(luma_fields(c.as_ref()), [0; 6]);
    }

    #[test]
    fn luma_coefficients_field_handles_outlive_the_parent_handle() {
        let c = LumaCoefficients::from_raw(luma_raw([7, 8, 9, 10, 11, 12]));
        // The field handles borrow the value, not the temporary handle.
        let (cr, cb) = {
            let r = c.as_ref();
            (r.cr(), r.cb())
        };
        assert_eq!((cr.get().into_raw().num, cb.den()), (7, 12));
        let copy = c.as_ref().get();
        assert_eq!(luma_fields(copy.as_ref()), [7, 8, 9, 10, 11, 12]);
    }

    #[test]
    fn luma_coefficients_value_setters_write_their_field_alone() {
        let q = |num, den| Rational::from_raw(ffi::AVRational { num, den });
        let mut c = LumaCoefficients::from_raw(luma_raw([1, 2, 3, 4, 5, 6]));
        let mut m = c.as_mut();
        m.set_cr(q(-1, -2));
        assert_eq!(luma_fields(m.as_ref()), [-1, -2, 3, 4, 5, 6]);
        m.set_cg(q(i32::MIN, 0));
        assert_eq!(luma_fields(m.as_ref()), [-1, -2, i32::MIN, 0, 5, 6]);
        m.set_cb(q(0, i32::MAX));
        assert_eq!(luma_fields(c.as_ref()), [-1, -2, i32::MIN, 0, 0, i32::MAX]);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn luma_coefficients_equality_is_by_value() {
        let a = LumaCoefficients::from_raw(luma_raw([1, 2, 1, 3, 1, 6]));
        let b = LumaCoefficients::from_raw(luma_raw([2, 4, 3, 9, 5, 30]));
        assert_eq!(a, b);
        assert_ne!(a, LumaCoefficients::from_raw(luma_raw([1, 2, 1, 3, 1, 7])));
        assert_ne!(a, LumaCoefficients::from_raw(luma_raw([1, 2, 1, 4, 1, 6])));
        assert_ne!(a, LumaCoefficients::from_raw(luma_raw([1, 3, 1, 3, 1, 6])));
        // `0/0` is unordered, like NaN.
        let z = LumaCoefficients::zeroed();
        assert_ne!(z, z);
    }

    #[test]
    fn luma_coefficients_debug_names_every_field() {
        let c = LumaCoefficients::from_raw(luma_raw([1, 2, 3, 4, 5, 6]));
        assert_eq!(
            format!("{c:?}"),
            "LumaCoefficients { cr: Rational { num: 1, den: 2 }, \
             cg: Rational { num: 3, den: 4 }, cb: Rational { num: 5, den: 6 } }"
        );
    }

    #[test]
    fn luma_coefficients_handles_borrow_raw_storage() {
        let mut raw = luma_raw([0; 6]);
        // SAFETY: `raw` is a live, initialised `AVLumaCoefficients` on this
        // frame, outliving the handle, and nothing else touches it meanwhile.
        let mut m = unsafe { LumaCoefficientsMut::from_ptr(&raw mut raw) }.unwrap();
        m.cg_mut().set_num(587);
        assert_eq!(raw.cg.num, 587);
        // SAFETY: NULL is accepted and yields `None`.
        assert!(unsafe { LumaCoefficientsRef::from_ptr(core::ptr::null_mut()) }.is_none());
        // SAFETY: as above.
        assert!(unsafe { LumaCoefficientsMut::from_ptr(core::ptr::null_mut()) }.is_none());
    }

    #[test]
    fn luma_coefficients_are_plain_shareable_values() {
        fn assert_traits<T: Copy + Send + Sync + fmt::Debug + Default + 'static>() {}
        fn assert_handles<T: Send + Sync>() {}
        assert_traits::<LumaCoefficients>();
        assert_handles::<LumaCoefficientsRef<'static>>();
        assert_handles::<LumaCoefficientsMut<'static>>();
    }

    #[test]
    fn gamma_reads_zero_as_none() {
        assert_eq!(gamma(0.0), None);
        assert_eq!(gamma(-0.0), None);
        assert_eq!(gamma(2.2), Some(2.2));
        assert!(gamma(f64::NAN).unwrap().is_nan());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn getters_hand_out_cs_functions() {
        let addr = |f: Option<unsafe extern "C" fn(f64) -> f64>| f.map(|f| f as usize);
        let eaddr = |f: Option<unsafe extern "C" fn(f64, f64, *mut f64)>| f.map(|f| f as usize);
        for trc in transfers() {
            let t = ColorTransfer::from_raw(trc);
            // SAFETY: the getters take any value and return NULL or a
            // `csp.c` function.
            let c = (
                unsafe { ffi::av_csp_trc_func_from_id(trc) },
                unsafe { ffi::av_csp_trc_func_inv_from_id(trc) },
                unsafe { ffi::av_csp_itu_eotf(trc) },
                unsafe { ffi::av_csp_itu_eotf_inv(trc) },
            );
            assert_eq!(t.trc_function().map(|f| f.0 as usize), addr(c.0));
            assert_eq!(t.inverse_trc_function().map(|f| f.0 as usize), addr(c.1));
            assert_eq!(t.eotf().map(|f| f.0 as usize), eaddr(c.2));
            assert_eq!(t.inverse_eotf().map(|f| f.0 as usize), eaddr(c.3));
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn approximate_gammas_follow_cs_tables() {
        assert_eq!(ColorTransfer::BT709.approximate_trc_gamma(), Some(1.961));
        assert_eq!(ColorTransfer::BT709.approximate_eotf_gamma(), Some(2.2));
        assert_eq!(
            ColorTransfer::IEC61966_2_1.approximate_trc_gamma(),
            Some(2.2)
        );
        assert_eq!(ColorTransfer::V_LOG.approximate_eotf_gamma(), Some(2.2));
        assert_eq!(ColorTransfer::SMPTE2084.approximate_trc_gamma(), None);
        assert_eq!(ColorTransfer::UNSPECIFIED.approximate_eotf_gamma(), None);
        let huge = ColorTransfer::from_raw(ffi::AVColorTransferCharacteristic(u32::MAX));
        assert_eq!(
            (huge.approximate_trc_gamma(), huge.approximate_eotf_gamma()),
            (None, None)
        );
        assert!(huge.trc_function().is_none() && huge.eotf().is_none());
        assert!(ColorTransfer::V_LOG.trc_function().is_some());
        assert!(ColorTransfer::V_LOG.eotf().is_none());
        assert!(ColorTransfer::LOG.eotf().is_none() && ColorTransfer::LOG.trc_function().is_some());
    }

    fn primaries_raw(f: [i32; 12]) -> ffi::AVPrimaryCoefficients {
        let xy = |i: usize| ffi::AVCIExy {
            x: q(f[i], f[i + 1]),
            y: q(f[i + 2], f[i + 3]),
        };
        ffi::AVPrimaryCoefficients {
            r: xy(0),
            g: xy(4),
            b: xy(8),
        }
    }

    fn primaries_fields(p: PrimaryCoefficientsRef<'_>) -> [i32; 12] {
        let mut out = [0; 12];
        for (i, xy) in [p.r(), p.g(), p.b()].into_iter().enumerate() {
            out[4 * i..4 * i + 4].copy_from_slice(&parts(xy));
        }
        out
    }

    const PRIMARIES: [i32; 12] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];

    #[test]
    fn primary_coefficients_layout_is_the_c_struct() {
        use core::mem::{align_of, offset_of, size_of};
        assert_eq!(
            size_of::<PrimaryCoefficients>(),
            size_of::<ffi::AVPrimaryCoefficients>()
        );
        assert_eq!(size_of::<PrimaryCoefficients>(), 48);
        assert_eq!(
            align_of::<PrimaryCoefficients>(),
            align_of::<ffi::AVPrimaryCoefficients>()
        );
        assert_eq!(offset_of!(ffi::AVPrimaryCoefficients, g), 16);
        assert_eq!(offset_of!(ffi::AVPrimaryCoefficients, b), 32);
    }

    #[test]
    fn primary_coefficients_new_zeroed_default_and_raw_round_trip() {
        let xy = |a, b, c, d| CieXy::new(r(a, b), r(c, d));
        let p = PrimaryCoefficients::new(
            xy(64, 100, 33, 100),
            xy(30, 100, 60, 100),
            xy(15, 100, 6, 100),
        );
        assert_eq!(
            primaries_fields(p.as_ref()),
            [64, 100, 33, 100, 30, 100, 60, 100, 15, 100, 6, 100]
        );
        assert_eq!(
            primaries_fields(PrimaryCoefficients::zeroed().as_ref()),
            [0; 12]
        );
        assert_eq!(
            primaries_fields(PrimaryCoefficients::default().as_ref()),
            [0; 12]
        );
        let f = [
            i32::MIN,
            -1,
            0,
            i32::MAX,
            1,
            0,
            -7,
            7,
            i32::MAX,
            i32::MIN,
            3,
            -3,
        ];
        let back: ffi::AVPrimaryCoefficients = PrimaryCoefficients::from(primaries_raw(f)).into();
        let back = PrimaryCoefficients::from_raw(back);
        assert_eq!(primaries_fields(back.as_ref()), f);
        assert_eq!(
            primaries_fields(PrimaryCoefficients::from_raw(primaries_raw(f)).as_ref()),
            f
        );
        let raw = PrimaryCoefficients::from_raw(primaries_raw(f)).into_raw();
        assert_eq!(
            (raw.r.x.num, raw.g.y.den, raw.b.x.den),
            (i32::MIN, 7, i32::MIN)
        );
        // `new` is usable in constants.
        const ZERO: PrimaryCoefficients = PrimaryCoefficients::new(
            CieXy::from_raw(ffi::AVCIExy {
                x: ffi::AVRational { num: 0, den: 0 },
                y: ffi::AVRational { num: 0, den: 0 },
            }),
            CieXy::from_raw(ffi::AVCIExy {
                x: ffi::AVRational { num: 0, den: 0 },
                y: ffi::AVRational { num: 0, den: 0 },
            }),
            CieXy::from_raw(ffi::AVCIExy {
                x: ffi::AVRational { num: 0, den: 0 },
                y: ffi::AVRational { num: 0, den: 0 },
            }),
        );
        assert_eq!(primaries_fields(ZERO.as_ref()), [0; 12]);
    }

    #[test]
    fn primary_coefficients_primary_handles_write_their_field_alone() {
        let mut p = PrimaryCoefficients::from_raw(primaries_raw(PRIMARIES));
        let mut m = p.as_mut();
        m.r_mut().x_mut().set_num(-10);
        assert_eq!(
            primaries_fields(m.as_ref()),
            [-10, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]
        );
        m.g_mut().set_y(r(60, 70));
        assert_eq!(
            primaries_fields(m.as_ref()),
            [-10, 2, 3, 4, 5, 6, 60, 70, 9, 10, 11, 12]
        );
        m.b_mut().set(CieXy::zeroed());
        assert_eq!(
            primaries_fields(m.as_ref()),
            [-10, 2, 3, 4, 5, 6, 60, 70, 0, 0, 0, 0]
        );
        m.set(PrimaryCoefficients::zeroed());
        assert_eq!(primaries_fields(p.as_ref()), [0; 12]);
    }

    #[test]
    fn primary_coefficients_value_setters_write_their_field_alone() {
        let mut p = PrimaryCoefficients::from_raw(primaries_raw(PRIMARIES));
        let mut m = p.as_mut();
        m.set_r(CieXy::new(r(-1, -2), r(-3, -4)));
        assert_eq!(
            primaries_fields(m.as_ref()),
            [-1, -2, -3, -4, 5, 6, 7, 8, 9, 10, 11, 12]
        );
        m.set_g(CieXy::new(r(i32::MIN, 0), r(0, i32::MAX)));
        assert_eq!(
            primaries_fields(m.as_ref()),
            [-1, -2, -3, -4, i32::MIN, 0, 0, i32::MAX, 9, 10, 11, 12]
        );
        m.set_b(CieXy::zeroed());
        assert_eq!(
            primaries_fields(p.as_ref()),
            [-1, -2, -3, -4, i32::MIN, 0, 0, i32::MAX, 0, 0, 0, 0]
        );
    }

    #[test]
    fn primary_coefficients_primary_handles_outlive_the_parent_handle() {
        let p = PrimaryCoefficients::from_raw(primaries_raw(PRIMARIES));
        // The primary handles borrow the value, not the temporary handle.
        let (red, blue) = {
            let h = p.as_ref();
            (h.r(), h.b())
        };
        assert_eq!((red.x().num(), blue.y().den()), (1, 12));
        let copy = p.as_ref().get();
        assert_eq!(primaries_fields(copy.as_ref()), PRIMARIES);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn primary_coefficients_equality_is_by_value() {
        let a = PrimaryCoefficients::from_raw(primaries_raw([1, 2, 1, 3, 1, 4, 1, 5, 1, 6, 1, 7]));
        let b =
            PrimaryCoefficients::from_raw(primaries_raw([2, 4, 3, 9, 4, 16, 5, 25, 6, 36, 7, 49]));
        assert_eq!(a, b);
        for i in 0..12 {
            let mut f = [1, 2, 1, 3, 1, 4, 1, 5, 1, 6, 1, 7];
            f[i] += 1;
            assert_ne!(a, PrimaryCoefficients::from_raw(primaries_raw(f)), "{i}");
        }
        // `0/0` is unordered, like NaN.
        let z = PrimaryCoefficients::zeroed();
        assert_ne!(z, z);
    }

    #[test]
    fn primary_coefficients_debug_names_every_primary() {
        let p = PrimaryCoefficients::from_raw(primaries_raw(PRIMARIES));
        let xy = |a, b, c, d| {
            format!(
                "CieXy {{ x: Rational {{ num: {a}, den: {b} }}, y: Rational {{ num: {c}, den: {d} }} }}"
            )
        };
        assert_eq!(
            format!("{p:?}"),
            format!(
                "PrimaryCoefficients {{ r: {}, g: {}, b: {} }}",
                xy(1, 2, 3, 4),
                xy(5, 6, 7, 8),
                xy(9, 10, 11, 12)
            )
        );
        assert_eq!(format!("{:?}", p.as_ref()), format!("{p:?}"));
    }

    #[test]
    fn primary_coefficients_handles_borrow_raw_storage() {
        let mut raw = primaries_raw([0; 12]);
        // SAFETY: `raw` is a live, initialised `AVPrimaryCoefficients` on
        // this frame, outliving the handle, and nothing else touches it
        // meanwhile.
        let mut m = unsafe { PrimaryCoefficientsMut::from_ptr(&raw mut raw) }.unwrap();
        m.g_mut().y_mut().set_num(595);
        m.set_b(CieXy::new(r(155, 1000), r(70, 1000)));
        assert_eq!((raw.g.y.num, raw.b.x.num, raw.b.y.den), (595, 155, 1000));
        // SAFETY: NULL is accepted and yields `None`.
        assert!(unsafe { PrimaryCoefficientsRef::from_ptr(core::ptr::null_mut()) }.is_none());
        // SAFETY: as above.
        assert!(unsafe { PrimaryCoefficientsMut::from_ptr(core::ptr::null_mut()) }.is_none());
    }

    #[test]
    fn primary_coefficients_are_plain_shareable_values() {
        fn assert_traits<T: Copy + Send + Sync + fmt::Debug + Default + 'static>() {}
        fn assert_handle<T: Send + Sync>() {}
        assert_traits::<PrimaryCoefficients>();
        assert_handle::<PrimaryCoefficientsRef<'static>>();
        assert_handle::<PrimaryCoefficientsMut<'static>>();
    }

    fn descriptor_raw(f: [i32; 16]) -> ffi::AVColorPrimariesDesc {
        let mut prim = [0; 12];
        prim.copy_from_slice(&f[4..]);
        ffi::AVColorPrimariesDesc {
            wp: ffi::AVCIExy {
                x: q(f[0], f[1]),
                y: q(f[2], f[3]),
            },
            prim: primaries_raw(prim),
        }
    }

    fn descriptor_fields(d: ColorPrimariesDescriptorRef<'_>) -> [i32; 16] {
        let mut out = [0; 16];
        out[..4].copy_from_slice(&parts(d.white_point()));
        out[4..].copy_from_slice(&primaries_fields(d.primaries()));
        out
    }

    const DESCRIPTOR: [i32; 16] = [21, 22, 23, 24, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];

    #[test]
    fn color_primaries_descriptor_layout_is_the_c_struct() {
        use core::mem::{align_of, offset_of, size_of};
        assert_eq!(
            size_of::<ColorPrimariesDescriptor>(),
            size_of::<ffi::AVColorPrimariesDesc>()
        );
        assert_eq!(size_of::<ColorPrimariesDescriptor>(), 64);
        assert_eq!(
            align_of::<ColorPrimariesDescriptor>(),
            align_of::<ffi::AVColorPrimariesDesc>()
        );
        assert_eq!(offset_of!(ffi::AVColorPrimariesDesc, wp), 0);
        assert_eq!(offset_of!(ffi::AVColorPrimariesDesc, prim), 16);
    }

    #[test]
    fn color_primaries_descriptor_new_zeroed_default_and_raw_round_trip() {
        let wp = CieXy::new(r(3127, 10000), r(3290, 10000));
        let prim = PrimaryCoefficients::from_raw(primaries_raw(PRIMARIES));
        let d = ColorPrimariesDescriptor::new(wp, prim);
        assert_eq!(
            descriptor_fields(d.as_ref()),
            [
                3127, 10000, 3290, 10000, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12
            ]
        );
        assert_eq!(
            descriptor_fields(ColorPrimariesDescriptor::zeroed().as_ref()),
            [0; 16]
        );
        assert_eq!(
            descriptor_fields(ColorPrimariesDescriptor::default().as_ref()),
            [0; 16]
        );
        let mut f = [i32::MIN; 16];
        f[3] = i32::MAX;
        f[15] = -1;
        let back: ffi::AVColorPrimariesDesc =
            ColorPrimariesDescriptor::from(descriptor_raw(f)).into();
        assert_eq!(
            descriptor_fields(ColorPrimariesDescriptor::from_raw(back).as_ref()),
            f
        );
        let raw = ColorPrimariesDescriptor::from_raw(descriptor_raw(f)).into_raw();
        assert_eq!(
            (raw.wp.y.den, raw.prim.r.x.num, raw.prim.b.y.den),
            (i32::MAX, i32::MIN, -1)
        );
        // `new` is usable in constants.
        const ZERO: ColorPrimariesDescriptor = ColorPrimariesDescriptor::new(
            CieXy::from_raw(ffi::AVCIExy {
                x: ffi::AVRational { num: 0, den: 0 },
                y: ffi::AVRational { num: 0, den: 0 },
            }),
            PrimaryCoefficients::from_raw(ffi::AVPrimaryCoefficients {
                r: ffi::AVCIExy {
                    x: ffi::AVRational { num: 0, den: 0 },
                    y: ffi::AVRational { num: 0, den: 0 },
                },
                g: ffi::AVCIExy {
                    x: ffi::AVRational { num: 0, den: 0 },
                    y: ffi::AVRational { num: 0, den: 0 },
                },
                b: ffi::AVCIExy {
                    x: ffi::AVRational { num: 0, den: 0 },
                    y: ffi::AVRational { num: 0, den: 0 },
                },
            }),
        );
        assert_eq!(descriptor_fields(ZERO.as_ref()), [0; 16]);
    }

    #[test]
    fn color_primaries_descriptor_field_handles_write_their_field_alone() {
        let mut d = ColorPrimariesDescriptor::from_raw(descriptor_raw(DESCRIPTOR));
        let mut m = d.as_mut();
        m.white_point_mut().x_mut().set_num(-21);
        let mut expect = DESCRIPTOR;
        expect[0] = -21;
        assert_eq!(descriptor_fields(m.as_ref()), expect);
        m.primaries_mut().g_mut().y_mut().set_den(80);
        expect[11] = 80;
        assert_eq!(descriptor_fields(m.as_ref()), expect);
        m.primaries_mut().set_b(CieXy::zeroed());
        expect[12..].copy_from_slice(&[0; 4]);
        assert_eq!(descriptor_fields(m.as_ref()), expect);
        m.white_point_mut().set(CieXy::zeroed());
        expect[..4].copy_from_slice(&[0; 4]);
        assert_eq!(descriptor_fields(m.as_ref()), expect);
        m.set(ColorPrimariesDescriptor::zeroed());
        assert_eq!(descriptor_fields(d.as_ref()), [0; 16]);
    }

    #[test]
    fn color_primaries_descriptor_value_setters_write_their_field_alone() {
        let mut d = ColorPrimariesDescriptor::from_raw(descriptor_raw(DESCRIPTOR));
        let mut m = d.as_mut();
        m.set_white_point(CieXy::new(r(1, 3), r(i32::MIN, i32::MAX)));
        let mut expect = DESCRIPTOR;
        expect[..4].copy_from_slice(&[1, 3, i32::MIN, i32::MAX]);
        assert_eq!(descriptor_fields(m.as_ref()), expect);
        m.set_primaries(PrimaryCoefficients::zeroed());
        expect[4..].copy_from_slice(&[0; 12]);
        assert_eq!(descriptor_fields(d.as_ref()), expect);
    }

    #[test]
    fn color_primaries_descriptor_field_handles_outlive_the_parent_handle() {
        let d = ColorPrimariesDescriptor::from_raw(descriptor_raw(DESCRIPTOR));
        // The field handles borrow the value, not the temporary handle, and
        // so do the handles they lend in turn.
        let (wp, prim, gx) = {
            let h = d.as_ref();
            (h.white_point(), h.primaries(), h.primaries().g().x())
        };
        assert_eq!((wp.y().num(), prim.b().y().den(), gx.num()), (23, 12, 5));
        let copy = d.as_ref().get();
        assert_eq!(descriptor_fields(copy.as_ref()), DESCRIPTOR);
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn color_primaries_descriptor_equality_is_by_value() {
        let base = [1, 3, 1, 3, 1, 2, 1, 3, 1, 4, 1, 5, 1, 6, 1, 7];
        let mut scaled = base;
        for (i, v) in scaled.iter_mut().enumerate() {
            *v *= (i / 2 + 2) as i32;
        }
        let a = ColorPrimariesDescriptor::from_raw(descriptor_raw(base));
        assert_eq!(
            a,
            ColorPrimariesDescriptor::from_raw(descriptor_raw(scaled))
        );
        for i in 0..16 {
            let mut f = base;
            f[i] += 1;
            assert_ne!(
                a,
                ColorPrimariesDescriptor::from_raw(descriptor_raw(f)),
                "{i}"
            );
        }
        // `0/0` is unordered, like NaN, in the white point alone too.
        let z = ColorPrimariesDescriptor::zeroed();
        assert_ne!(z, z);
        let mut no_wp = a;
        no_wp.as_mut().set_white_point(CieXy::zeroed());
        assert_ne!(no_wp, no_wp);
    }

    #[test]
    fn color_primaries_descriptor_debug_names_every_field() {
        let d = ColorPrimariesDescriptor::from_raw(descriptor_raw(DESCRIPTOR));
        assert_eq!(
            format!("{d:?}"),
            format!(
                "ColorPrimariesDescriptor {{ white_point: {:?}, primaries: {:?} }}",
                d.as_ref().white_point(),
                d.as_ref().primaries()
            )
        );
        assert!(format!("{d:?}").contains("white_point: CieXy { x: Rational { num: 21, den: 22 }"));
        assert_eq!(format!("{:?}", d.as_ref()), format!("{d:?}"));
    }

    #[test]
    fn color_primaries_descriptor_handles_borrow_raw_storage() {
        let mut raw = descriptor_raw([0; 16]);
        // SAFETY: `raw` is a live, initialised `AVColorPrimariesDesc` on
        // this frame, outliving the handle, and nothing else touches it
        // meanwhile.
        let mut m = unsafe { ColorPrimariesDescriptorMut::from_ptr(&raw mut raw) }.unwrap();
        m.white_point_mut().y_mut().set_num(3290);
        m.primaries_mut()
            .set_r(CieXy::new(r(640, 1000), r(330, 1000)));
        assert_eq!(
            (raw.wp.y.num, raw.prim.r.x.num, raw.prim.r.y.den),
            (3290, 640, 1000)
        );
        // SAFETY: NULL is accepted and yields `None`.
        assert!(unsafe { ColorPrimariesDescriptorRef::from_ptr(core::ptr::null_mut()) }.is_none());
        // SAFETY: as above.
        assert!(unsafe { ColorPrimariesDescriptorMut::from_ptr(core::ptr::null_mut()) }.is_none());
    }

    #[test]
    fn color_primaries_descriptors_are_plain_shareable_values() {
        fn assert_traits<T: Copy + Send + Sync + fmt::Debug + Default + 'static>() {}
        fn assert_handle<T: Send + Sync>() {}
        assert_traits::<ColorPrimariesDescriptor>();
        assert_handle::<ColorPrimariesDescriptorRef<'static>>();
        assert_handle::<ColorPrimariesDescriptorMut<'static>>();
    }

    /// The coordinate parts (`descriptor_raw` order) of every denominator.
    const DENOMINATORS: [usize; 8] = [1, 3, 5, 7, 9, 11, 13, 15];

    #[test]
    fn identify_answers_zero_denominators_without_c() {
        // No FFI on this path, so it also runs under Miri.
        assert_eq!(ColorPrimariesDescriptor::zeroed().as_ref().identify(), None);
        for (i, num) in DENOMINATORS
            .into_iter()
            .flat_map(|i| [(i, 0), (i, 7), (i, -1)])
        {
            let mut f = DESCRIPTOR;
            f[i] = 0;
            f[i - 1] = num;
            let d = ColorPrimariesDescriptor::from_raw(descriptor_raw(f));
            assert_eq!(d.as_ref().identify(), None, "{i} {num}");
        }
    }

    /// C's identification of `d`, read in place.
    fn c_identify(d: &ffi::AVColorPrimariesDesc) -> ffi::AVColorPrimaries {
        // SAFETY: `d` is a live, initialised description C only reads.
        unsafe { ffi::av_csp_primaries_id_from_desc(d) }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn identify_corrects_c_matching_unordered_differences_as_bt709() {
        let bt709 = ffi::AVColorPrimaries::AVCOL_PRI_BT709;
        // The all-zero description: C sums `0/0` differences, which
        // `av_cmp_q` reports unordered (`INT_MIN`), read as a match.
        let zero = ffi::AVColorPrimariesDesc::default();
        assert_eq!(c_identify(&zero), bt709);
        assert_eq!(
            ColorPrimariesDescriptor::from_raw(zero).as_ref().identify(),
            None
        );
        let bt2020 = ColorPrimaries::BT2020
            .descriptor()
            .unwrap()
            .get()
            .into_raw();
        let fields = descriptor_fields(ColorPrimariesDescriptor::from_raw(bt2020).as_ref());
        assert_eq!(c_identify(&bt2020), ffi::AVColorPrimaries::AVCOL_PRI_BT2020);
        for &i in &DENOMINATORS {
            // One `0/0` coordinate anywhere.
            let mut f = fields;
            f[i - 1..=i].copy_from_slice(&[0, 0]);
            let d = descriptor_raw(f);
            assert_eq!(c_identify(&d), bt709, "{i}");
            assert_eq!(
                ColorPrimariesDescriptor::from_raw(d).as_ref().identify(),
                None
            );
            for &j in DENOMINATORS.iter().filter(|&&j| j > i) {
                // Two infinite coordinates: `1/0 + 1/0` is `0/0` in C.
                let mut f = fields;
                f[i - 1..=i].copy_from_slice(&[5, 0]);
                f[j - 1..=j].copy_from_slice(&[-3, 0]);
                let d = descriptor_raw(f);
                assert_eq!(c_identify(&d), bt709, "{i} {j}");
                assert_eq!(
                    ColorPrimariesDescriptor::from_raw(d).as_ref().identify(),
                    None
                );
            }
        }
    }
}
