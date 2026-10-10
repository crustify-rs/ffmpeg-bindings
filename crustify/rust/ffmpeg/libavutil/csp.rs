//! Safe wrappers over `libavutil/csp.h`.
//!
//! [`TrcFunction`] and [`EotfFunction`] call the transfer functions C hands
//! out for a transfer characteristic (`av_csp_trc_function`,
//! `av_csp_eotf_function`).
//!
//! [`CieXy`] is `AVCIExy` (also spelled `AVWhitepointCoefficients`,
//! [`WhitepointCoefficients`] here): a CIE 1931 chromaticity, two
//! [`Rational`] coordinates. C embeds it by value in its gamut descriptions
//! (a white point, and each of the red, green and blue primaries) and copies
//! it by value; nothing allocates, frees or clones one on its own, so it is a
//! value type with no owner. A field of a wrapped C struct is reached as a
//! [`CieXyRef`] or [`CieXyMut`] over the projected place, and each
//! coordinate as a [`RationalRef`] or [`RationalMut`] over its own.

use core::fmt;
use core::ptr::{addr_of, addr_of_mut};

use ffibox::{CZeroable, define_ctype};

use super::ffi;
use super::rational::{Rational, RationalMut, RationalRef};

/// Wraps: av_csp_trc_function
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
    pub fn call(self, x: f64) -> f64 {
        // SAFETY: the handle holds one of `csp.c`'s transfer functions,
        // which take any `f64` and touch no memory.
        unsafe { (self.0)(x) }
    }
}

/// Wraps: av_csp_eotf_function
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
    #[must_use]
    pub fn x(&self) -> RationalRef<'a> {
        // SAFETY: `x` is projected through the shared handle's pointer to a
        // live, initialised `AVCIExy`, so it is non-NULL, aligned and
        // initialised; the struct stays live and unwritten for `'a`, which
        // the returned getter-only handle cannot outlive.
        unsafe { RationalRef::from_ptr(addr_of!((*self.as_ptr()).x).cast_mut()) }
            .expect("a field of a live struct is not NULL")
    }

    /// Field: AVCIExy.y
    ///
    /// The y coordinate, as a handle over it that lives as long as this one.
    #[must_use]
    pub fn y(&self) -> RationalRef<'a> {
        // SAFETY: as in `x`, for the `y` field.
        unsafe { RationalRef::from_ptr(addr_of!((*self.as_ptr()).y).cast_mut()) }
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
    #[must_use]
    pub fn x_mut(&mut self) -> RationalMut<'_> {
        // SAFETY: `x` is projected through the exclusive handle's pointer to
        // a live `AVCIExy`, so it is non-NULL, aligned and initialised; the
        // result borrows `self` exclusively, so it is the only access to the
        // field while it lives.
        unsafe { RationalMut::from_ptr(addr_of_mut!((*self.as_mut_ptr()).x)) }
            .expect("a field of a live struct is not NULL")
    }

    /// Field: AVCIExy.y
    ///
    /// An exclusive handle over the y coordinate, for writing its parts;
    /// this handle is frozen while it lives.
    #[must_use]
    pub fn y_mut(&mut self) -> RationalMut<'_> {
        // SAFETY: as in `x_mut`, for the `y` field.
        unsafe { RationalMut::from_ptr(addr_of_mut!((*self.as_mut_ptr()).y)) }
            .expect("a field of a live struct is not NULL")
    }

    /// Field: AVCIExy.x
    ///
    /// Sets the x coordinate, leaving y as it is.
    pub fn set_x(&mut self, x: Rational) {
        // SAFETY: a write of the `x` field projected through the exclusive
        // handle's pointer to a live `AVCIExy`; the old value owns nothing to
        // leak.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).x).write(x.into_raw()) }
    }

    /// Field: AVCIExy.y
    ///
    /// Sets the y coordinate, leaving x as it is.
    pub fn set_y(&mut self, y: Rational) {
        // SAFETY: a write of the `y` field projected through the exclusive
        // handle's pointer to a live `AVCIExy`; the old value owns nothing to
        // leak.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).y).write(y.into_raw()) }
    }

    /// Overwrites both coordinates with `xy`'s.
    pub fn set(&mut self, xy: CieXy) {
        // SAFETY: a write of the whole `AVCIExy` through the exclusive
        // handle's pointer to a live one; the old value owns nothing to leak.
        unsafe { self.as_mut_ptr().write(xy.into_raw()) }
    }
}

#[cfg(test)]
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
            let fns = unsafe {
                [
                    ffi::av_csp_trc_func_from_id(trc),
                    ffi::av_csp_trc_func_inv_from_id(trc),
                ]
            };
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
            let fns = unsafe { [ffi::av_csp_itu_eotf(trc), ffi::av_csp_itu_eotf_inv(trc)] };
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
        fn assert_traits<T: Copy + Send + Sync + core::fmt::Debug + 'static>() {}
        assert_traits::<TrcFunction>();
        assert_traits::<EotfFunction>();
    }

    #[test]
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
        assert_eq!(core::mem::size_of::<CieXy>(), 16);
        assert_eq!(
            core::mem::align_of::<CieXy>(),
            core::mem::align_of::<ffi::AVCIExy>()
        );
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
        #[allow(clippy::eq_op)] // the reflexive case is the point
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
        let (fx, fy) = unsafe { (addr_of!((*base).x), addr_of!((*base).y)) };
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
        fn assert_traits<T: Copy + Send + Sync + Default + core::fmt::Debug + 'static>() {}
        fn assert_handle<T: Send + Sync>() {}
        assert_traits::<CieXy>();
        assert_handle::<CieXyRef<'static>>();
        assert_handle::<CieXyMut<'static>>();
    }
}
