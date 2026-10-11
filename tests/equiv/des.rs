use core::ptr::{addr_of, addr_of_mut};
use ffmpeg::libavutil::{des::Des, ffi};

struct Reference(*mut ffi::AVDES);
impl Reference {
    fn new() -> Self {
        // SAFETY: the allocator takes no arguments and returns a fresh zeroed
        // context. This test fixture releases its allocation exactly once.
        let ptr = unsafe { ffi::av_des_alloc() };
        assert!(!ptr.is_null());
        Self(ptr)
    }
    fn fields(&self) -> (i32, [[u64; 16]; 3]) {
        // SAFETY: this fixture owns a live, fully initialized context; it is
        // not mutated during these raw field projections, which form no references.
        unsafe {
            (
                addr_of!((*self.0).triple_des).read(),
                addr_of!((*self.0).round_keys).read(),
            )
        }
    }
}
impl Drop for Reference {
    fn drop(&mut self) {
        // SAFETY: the fixture uniquely owns this av_des_alloc allocation,
        // which has no owned fields and is released by av_free.
        unsafe { ffi::av_free(self.0.cast()) }
    }
}

#[repr(align(8))]
struct Blocks([u8; 16]);

#[test]
fn lifecycle_fields_and_multicall_state_agree() {
    let raw = Reference::new();
    let mut safe = Des::new_boxed(&[0; 8], false).unwrap();
    for (key, decrypt) in [
        (vec![0; 8], false),
        (vec![0xA5; 24], true),
        ((0..8).collect(), true),
        (vec![0xFF; 24], false),
    ] {
        safe.as_mut().init(&key, decrypt).unwrap();
        let inline = Des::new(&key, decrypt).unwrap();
        // SAFETY: raw owns a live context and key contains the exact 8 or 24
        // readable bytes promised by key_bits; C retains no key pointer.
        let ret = unsafe {
            ffi::av_des_init(
                raw.0,
                key.as_ptr(),
                (key.len() * 8) as i32,
                i32::from(decrypt),
            )
        };
        assert_eq!(ret, 0);
        let (flag, keys) = raw.fields();
        assert_eq!(safe.as_ref().triple_des(), flag);
        assert_eq!(safe.as_ref().round_keys(), keys);
        assert_eq!(inline.as_ref().triple_des(), flag);
        assert_eq!(inline.as_ref().round_keys()[0], keys[0]);
        if key.len() == 24 {
            assert_eq!(inline.as_ref().round_keys(), keys);
        } else {
            assert_eq!(inline.as_ref().round_keys()[1..], [[0; 16]; 2]);
        }

        let input = Blocks(*b"0123456789abcdef");
        let mut output_raw = Blocks([0; 16]);
        let mut output_safe = Blocks([0; 16]);
        for decrypt in [false, true, false] {
            // C crypt is an unwrapped observer of this type run's initialized
            // context. Both contexts are independently owned and keyed above.
            // SAFETY: both pointers are live contexts, buffers are aligned to
            // 8 bytes and hold two blocks, input/output do not alias, and C
            // retains no pointer. No Rust context handle is live during the calls.
            unsafe {
                ffi::av_des_crypt(
                    raw.0,
                    output_raw.0.as_mut_ptr(),
                    input.0.as_ptr(),
                    2,
                    core::ptr::null_mut(),
                    i32::from(decrypt),
                );
                ffi::av_des_crypt(
                    safe.as_c_ptr(),
                    output_safe.0.as_mut_ptr(),
                    input.0.as_ptr(),
                    2,
                    core::ptr::null_mut(),
                    i32::from(decrypt),
                );
            }
            assert_eq!(output_safe.0, output_raw.0);
        }
    }

    for flag in [i32::MIN, -1, 0, 1, i32::MAX] {
        let keys = [[flag as u64; 16]; 3];
        safe.as_mut().set_triple_des(flag);
        safe.as_mut().set_round_keys(keys);
        // SAFETY: the fixture owns the entire initialized context exclusively;
        // integers accept these values and the projections create no references.
        unsafe {
            addr_of_mut!((*raw.0).triple_des).write(flag);
            addr_of_mut!((*raw.0).round_keys).write(keys);
        }
        assert_eq!(safe.as_ref().triple_des(), raw.fields().0);
        assert_eq!(safe.as_ref().round_keys(), raw.fields().1);
    }
    let before = raw.fields();
    assert_eq!(safe.as_mut().init(&[0; 7], false), Err(-22));
    // SAFETY: raw is a live context. C rejects the invalid bit count before
    // inspecting the key pointer or changing any state.
    let result = unsafe { ffi::av_des_init(raw.0, core::ptr::null(), 56, 0) };
    assert_eq!(result, -22);
    assert_eq!(raw.fields(), before);
    assert_eq!(safe.as_ref().round_keys(), before.1);
    assert_eq!(safe.as_ref().triple_des(), before.0);
}
