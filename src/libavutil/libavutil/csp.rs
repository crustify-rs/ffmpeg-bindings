//! Safe wrappers over `libavutil/csp.h`.
//!
//! [`TrcFunction`] and [`EotfFunction`] call the transfer functions C hands
//! out for a transfer characteristic (`av_csp_trc_function`,
//! `av_csp_eotf_function`).

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

#[cfg(test)]
mod unit_tests {
    use super::*;
    use crate::libavutil::ffi;

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
}
