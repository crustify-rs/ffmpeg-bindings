//! Equivalence of `libavutil/hwcontext.h`'s [`DeviceType`] and
//! [`TransferDirection`] with the C enums, as C observes them.
//!
//! [`DeviceType`] is observed through C's name table
//! (`av_hwdevice_get_type_name`, `av_hwdevice_find_type_by_name`), its backend
//! walk (`av_hwdevice_iterate_types`) and C-allocated storage holding the
//! values. [`TransferDirection`]'s only C reader,
//! `av_hwframe_transfer_get_formats`, needs a hardware frames context, which
//! the campaign builds cannot create (they compile no hwcontext backend), so
//! it is compared with the C enumerators only.

use core::ffi::{CStr, c_uint, c_void};
use core::mem::size_of;

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::hwcontext::{DeviceType, TransferDirection};
use ffmpeg::libavutil::mem::{HeapVec, alloc_array_zeroed};

use ffi::AVHWDeviceType as C;

/// Every named constant beside its C enumerator and the name C gives it.
const NAMED: [(DeviceType, C, Option<&str>); 15] = [
    (DeviceType::NONE, C::AV_HWDEVICE_TYPE_NONE, None),
    (DeviceType::VDPAU, C::AV_HWDEVICE_TYPE_VDPAU, Some("vdpau")),
    (DeviceType::CUDA, C::AV_HWDEVICE_TYPE_CUDA, Some("cuda")),
    (DeviceType::VAAPI, C::AV_HWDEVICE_TYPE_VAAPI, Some("vaapi")),
    (DeviceType::DXVA2, C::AV_HWDEVICE_TYPE_DXVA2, Some("dxva2")),
    (DeviceType::QSV, C::AV_HWDEVICE_TYPE_QSV, Some("qsv")),
    (
        DeviceType::VIDEOTOOLBOX,
        C::AV_HWDEVICE_TYPE_VIDEOTOOLBOX,
        Some("videotoolbox"),
    ),
    (
        DeviceType::D3D11VA,
        C::AV_HWDEVICE_TYPE_D3D11VA,
        Some("d3d11va"),
    ),
    (DeviceType::DRM, C::AV_HWDEVICE_TYPE_DRM, Some("drm")),
    (
        DeviceType::OPENCL,
        C::AV_HWDEVICE_TYPE_OPENCL,
        Some("opencl"),
    ),
    (
        DeviceType::MEDIACODEC,
        C::AV_HWDEVICE_TYPE_MEDIACODEC,
        Some("mediacodec"),
    ),
    (
        DeviceType::VULKAN,
        C::AV_HWDEVICE_TYPE_VULKAN,
        Some("vulkan"),
    ),
    (
        DeviceType::D3D12VA,
        C::AV_HWDEVICE_TYPE_D3D12VA,
        Some("d3d12va"),
    ),
    (DeviceType::AMF, C::AV_HWDEVICE_TYPE_AMF, Some("amf")),
    (
        DeviceType::OHCODEC,
        C::AV_HWDEVICE_TYPE_OHCODEC,
        Some("ohcodec"),
    ),
];

/// Values a user may store in `AVHWDeviceContext.type` though C names none
/// of them: just past the table, and the `unsigned` extremes a negative `int`
/// written there reads back as.
const UNNAMED: [c_uint; 7] = [
    15,
    16,
    64,
    1 << 20,
    i32::MAX as c_uint,
    i32::MIN as c_uint,
    c_uint::MAX,
];

/// What C calls `t`: `av_hwdevice_get_type_name`, NULL as `None`.
fn c_name(t: C) -> Option<&'static str> {
    // SAFETY: `av_hwdevice_get_type_name` takes the enum by value and
    // bounds-checks it against its name table.
    let name = unsafe { ffi::av_hwdevice_get_type_name(t) };
    if name.is_null() {
        return None;
    }
    // SAFETY: a non-NULL result is a string literal in `hwcontext.c`'s static
    // `hw_type_names` table.
    Some(unsafe { CStr::from_ptr(name) }.to_str().unwrap())
}

/// The type C finds for `name`: `av_hwdevice_find_type_by_name`.
fn c_find(name: &CStr) -> DeviceType {
    // SAFETY: `name` is a valid NUL-terminated string for the call, which
    // only compares it with the table's names.
    DeviceType::from_raw(unsafe { ffi::av_hwdevice_find_type_by_name(name.as_ptr()) })
}

/// The next usable type after `prev`: `av_hwdevice_iterate_types`.
fn c_next(prev: C) -> DeviceType {
    // SAFETY: `av_hwdevice_iterate_types` takes the enum by value and only
    // compares it with the compiled-in backends' types.
    DeviceType::from_raw(unsafe { ffi::av_hwdevice_iterate_types(prev) })
}

/// Whether the wrapper names `t`: its `Debug` is a constant's name, not the
/// raw tuple form.
fn rust_names(t: DeviceType) -> bool {
    !format!("{t:?}").starts_with("DeviceType(")
}

#[test]
fn constants_are_the_c_enumerators() {
    for (t, c, name) in NAMED {
        assert_eq!(t.into_raw(), c);
        assert_eq!(DeviceType::from_raw(c), t);
        assert_eq!(DeviceType::from(c), t);
        assert_eq!(C::from(t), c);
        assert_eq!(c_name(t.into_raw()), c_name(c));
        assert_eq!(c_name(t.into_raw()), name, "{t:?}");
    }
    assert_eq!(DeviceType::default().into_raw(), C::AV_HWDEVICE_TYPE_NONE);
}

#[test]
fn the_wrapper_names_exactly_what_c_names_and_none() {
    for raw in (0..=48).chain(UNNAMED) {
        let t = DeviceType::from_raw(C(raw));
        let c_names = c_name(C(raw)).is_some() || C(raw) == C::AV_HWDEVICE_TYPE_NONE;
        assert_eq!(rust_names(t), c_names, "raw {raw}");
        assert_eq!(t.into_raw().0, raw);
    }
}

#[test]
fn c_name_lookups_round_trip_through_the_constants() {
    for (t, _, name) in NAMED {
        let Some(name) = name else { continue };
        let name = std::ffi::CString::new(name).unwrap();
        // C's reverse lookup lands on the wrapper's constant, and the
        // constant's encoding reads back as the same name.
        let found = c_find(&name);
        assert_eq!(found, t);
        assert_eq!(c_name(found.into_raw()), name.to_str().ok());
    }
    for unknown in [c"", c"none", c"CUDA", c"vaapi ", c"vulkan2", c"hw"] {
        assert_eq!(c_find(unknown), DeviceType::NONE, "{unknown:?}");
        assert_eq!(c_find(unknown), DeviceType::default());
    }
}

#[test]
fn backend_walk_agrees_for_rust_and_c_encodings() {
    // Start from every named and unnamed value, then follow C's walk from
    // the Rust side until it reports NONE; every step must be a type the
    // wrapper names, and strictly after the previous one.
    let starts = NAMED
        .iter()
        .map(|&(t, c, _)| (t, c))
        .chain(UNNAMED.map(|r| (DeviceType::from_raw(C(r)), C(r))));
    for (t, c) in starts {
        assert_eq!(c_next(t.into_raw()), c_next(c));
    }
    let mut prev = DeviceType::NONE;
    let mut walked = Vec::new();
    for _ in 0..=NAMED.len() {
        let next = c_next(prev.into_raw());
        if next == DeviceType::NONE {
            break;
        }
        assert!(rust_names(next), "{next:?}");
        assert!(next.into_raw().0 > prev.into_raw().0);
        assert!(c_name(next.into_raw()).is_some());
        walked.push(next);
        prev = next;
    }
    assert!(walked.len() < NAMED.len());
}

/// A C block of `ffi::AVHWDeviceType`, freed with `av_free` on drop.
struct CTypes(*mut C, usize);

impl CTypes {
    fn zeroed(len: usize) -> Self {
        // SAFETY: `av_calloc` takes its sizes by value.
        let ptr = unsafe { ffi::av_calloc(len, size_of::<C>()) };
        assert!(!ptr.is_null());
        Self(ptr.cast(), len)
    }

    fn get(&self, i: usize) -> C {
        assert!(i < self.1);
        // SAFETY: `i` is in bounds of the live, initialised block.
        unsafe { self.0.add(i).read() }
    }

    fn set(&mut self, i: usize, t: C) {
        assert!(i < self.1);
        // SAFETY: `i` is in bounds of the live block, which we own.
        unsafe { self.0.add(i).write(t) }
    }
}

impl Drop for CTypes {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a live `av_calloc` block we own.
        unsafe { ffi::av_free(self.0.cast::<c_void>()) }
    }
}

/// The C values a Rust array holds, read back through its C-typed pointer.
fn rust_as_c(a: &HeapVec<DeviceType>) -> Vec<C> {
    let ptr = a.as_ptr().cast::<C>();
    (0..a.len())
        // SAFETY: `DeviceType` is `#[repr(transparent)]` over
        // `ffi::AVHWDeviceType`, and `i` is in bounds of the live,
        // initialised array `a` borrows.
        .map(|i| unsafe { ptr.add(i).read() })
        .collect()
}

#[test]
fn storage_holds_the_same_values_and_names_after_each_write() {
    let values: Vec<C> = NAMED
        .iter()
        .map(|&(_, c, _)| c)
        .chain(UNNAMED.map(C))
        .collect();
    let len = values.len();
    let mut rust = alloc_array_zeroed::<DeviceType>(len).unwrap();
    let mut c = CTypes::zeroed(len);
    // Zeroed storage reads as NONE on both sides.
    assert_eq!(
        rust_as_c(&rust),
        (0..len).map(|i| c.get(i)).collect::<Vec<_>>()
    );
    assert!(rust.as_slice().iter().all(|&t| t == DeviceType::NONE));
    // Fill both from opposite ends, comparing every slot and its C name
    // after each step.
    for (step, &v) in values.iter().enumerate() {
        let i = (step * 7) % len;
        rust.as_mut_slice()[i] = DeviceType::from_raw(v);
        c.set(i, v);
        let r = rust_as_c(&rust);
        for (j, &rv) in r.iter().enumerate() {
            assert_eq!(rv, c.get(j), "step {step}, slot {j}");
            assert_eq!(c_name(rv), c_name(c.get(j)));
        }
    }
}

#[test]
fn transfer_directions_are_the_c_enumerators() {
    use ffi::AVHWFrameTransferDirection as D;
    assert_eq!(
        TransferDirection::From.into_raw(),
        D::AV_HWFRAME_TRANSFER_DIRECTION_FROM
    );
    assert_eq!(
        TransferDirection::To.into_raw(),
        D::AV_HWFRAME_TRANSFER_DIRECTION_TO
    );
    assert_eq!(
        D::from(TransferDirection::From),
        D::AV_HWFRAME_TRANSFER_DIRECTION_FROM
    );
    assert_eq!(
        D::from(TransferDirection::To),
        D::AV_HWFRAME_TRANSFER_DIRECTION_TO
    );
}
