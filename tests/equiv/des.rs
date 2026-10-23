#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::too_many_lines,
    reason = "each test is one scenario, its helpers and steps kept where they are used"
)]
use ffmpeg_bindings::libavutil::libavutil::aes::Direction;
use ffmpeg_bindings::libavutil::libavutil::des::Des;
use ffmpeg_bindings::libavutil::libavutil::ffi;

/// The safe direction for C's `decrypt` flag.
fn direction(decrypt: bool) -> Direction {
    if decrypt {
        Direction::Decrypt
    } else {
        Direction::Encrypt
    }
}

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
        (unsafe { (*self.0).triple_des }, unsafe {
            (*self.0).round_keys
        })
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
#[cfg_attr(miri, ignore = "calls C")]
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
            // Both contexts are independently owned and keyed above.
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
            }
            safe.as_ref()
                .crypt(&mut output_safe.0, &input.0, None, direction(decrypt))
                .unwrap();
            assert_eq!(output_safe.0, output_raw.0);
        }
    }

    for flag in [i32::MIN, -1, 0, 1, i32::MAX] {
        let keys = [[flag as u64; 16]; 3];
        safe.as_mut().set_triple_des(flag);
        safe.as_mut().set_round_keys(keys);
        // For the unsafe operations below: the fixture owns the entire initialized context
        // exclusively; integers accept these values and the projections create no
        // references.
        {
            // SAFETY: as the comment above this block states.
            unsafe { (*raw.0).triple_des = flag };
            // SAFETY: as the comment above this block states.
            unsafe { (*raw.0).round_keys = keys };
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

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn copies_and_clones_track_c_struct_copies() {
    let key: Vec<u8> = (0..24).map(|i| (i * 29 + 3) as u8).collect();
    let raw = Reference::new();
    // SAFETY: raw owns a live context; the key has the 24 bytes C reads for
    // 192 bits, and C retains no pointer to it.
    let ret = unsafe { ffi::av_des_init(raw.0, key.as_ptr(), 192, 0) };
    assert_eq!(ret, 0);
    let raw_copy = Reference::new();
    // SAFETY: two distinct live AVDES allocations; C code copies the struct
    // by assignment, which this byte copy of its whole size mirrors.
    unsafe { core::ptr::copy_nonoverlapping(raw.0, raw_copy.0, 1) };

    let original = Des::new_boxed(&key, false).unwrap();
    let mut boxed = original.clone();
    let mut inline = Des::new(&key, false).unwrap();
    let inline_copy = inline;
    // Diverge the originals: neither the clone nor any copy may follow.
    drop(original);
    inline.as_mut().init(&[0x11; 8], false).unwrap();
    // SAFETY: raw is live; the 8-byte key matches 64 bits and is not retained.
    let ret = unsafe { ffi::av_des_init(raw.0, [0x11; 8].as_ptr(), 64, 0) };
    assert_eq!(ret, 0);

    let crypt = |ctx: *mut ffi::AVDES, decrypt: i32| {
        let input = Blocks(*b"fedcba9876543210");
        let mut output = Blocks([0; 16]);
        // SAFETY: ctx is a live keyed context; both buffers are 8-byte aligned
        // two-block arrays that do not alias, ECB uses no IV, nothing is retained.
        unsafe {
            ffi::av_des_crypt(
                ctx,
                output.0.as_mut_ptr(),
                input.0.as_ptr(),
                2,
                core::ptr::null_mut(),
                decrypt,
            );
        };
        output.0
    };
    for decrypt in [0, 1] {
        let expected = crypt(raw_copy.0, decrypt);
        let mut actual = [0; 16];
        boxed
            .as_ref()
            .crypt(
                &mut actual,
                b"fedcba9876543210",
                None,
                direction(decrypt != 0),
            )
            .unwrap();
        assert_eq!(actual, expected);
    }
    assert_eq!(boxed.as_ref().round_keys(), raw_copy.fields().1);
    assert_eq!(inline_copy.as_ref().round_keys(), raw_copy.fields().1);
    assert_eq!(inline_copy.as_ref().triple_des(), raw_copy.fields().0);
    assert_eq!(inline.as_ref().round_keys()[0], raw.fields().1[0]);
    boxed.as_mut().set_triple_des(0);
    assert_eq!(inline_copy.as_ref().triple_des(), 1);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn crypt_cbc_in_place_zero_input_and_mac_sequences() {
    for key_len in [8, 24] {
        let key = vec![0x91; key_len];
        let safe = Des::new(&key, false).unwrap();
        let raw = Reference::new();
        // SAFETY: live context, exactly key_len initialized key bytes; no retention.
        let result = unsafe { ffi::av_des_init(raw.0, key.as_ptr(), (key_len * 8) as i32, 0) };
        assert_eq!(result, 0);
        let input: Vec<u8> = (0..81).map(|i| (i * 47) as u8).collect();
        for decrypt in [false, true] {
            for cbc in [false, true] {
                let mut iv_a = [0x35; 8];
                let mut iv_b = iv_a;
                for blocks in [0, 1, 2, 7, 3] {
                    let src = &input[1..=(blocks * 8)];
                    let mut expected = vec![0x99; blocks * 8];
                    let mut actual = expected.clone();
                    // SAFETY: live read-only context, disjoint initialized buffers
                    // of blocks*8 bytes and an eight-byte IV when CBC is selected.
                    unsafe {
                        ffi::av_des_crypt(
                            raw.0,
                            expected.as_mut_ptr(),
                            src.as_ptr(),
                            blocks as i32,
                            if cbc {
                                iv_a.as_mut_ptr()
                            } else {
                                core::ptr::null_mut()
                            },
                            i32::from(decrypt),
                        );
                    };
                    safe.as_ref()
                        .crypt(
                            &mut actual,
                            src,
                            cbc.then_some(&mut iv_b),
                            direction(decrypt),
                        )
                        .unwrap();
                    assert_eq!(actual, expected);
                    assert_eq!(iv_a, iv_b);
                    let mut in_place = src.to_vec();
                    let mut ref_place = src.to_vec();
                    let mut place_iv_a = [0x7B; 8];
                    let mut place_iv_b = place_iv_a;
                    let place = ref_place.as_mut_ptr();
                    // SAFETY: C supports exact aliasing; both pointers name the same
                    // exclusively held buffer of whole blocks. Context is read-only.
                    unsafe {
                        ffi::av_des_crypt(
                            raw.0,
                            place,
                            place,
                            blocks as i32,
                            if cbc {
                                place_iv_a.as_mut_ptr()
                            } else {
                                core::ptr::null_mut()
                            },
                            i32::from(decrypt),
                        );
                    };
                    safe.as_ref()
                        .crypt_in_place(
                            &mut in_place,
                            cbc.then_some(&mut place_iv_b),
                            direction(decrypt),
                        )
                        .unwrap();
                    assert_eq!(in_place, ref_place);
                    assert_eq!(place_iv_a, place_iv_b);
                }
            }
        }
        for blocks in [0, 1, 2, 8] {
            for decrypt in [false, true] {
                let mut zeros = vec![0; blocks * 8];
                let mut expected = zeros.clone();
                let mut iv_a = [7; 8];
                let mut iv_b = iv_a;
                // SAFETY: real zero-filled input avoids C's NULL arithmetic defect.
                // The disjoint buffers contain whole blocks; IV has eight bytes.
                unsafe {
                    ffi::av_des_crypt(
                        raw.0,
                        expected.as_mut_ptr(),
                        zeros.as_ptr(),
                        blocks as i32,
                        iv_a.as_mut_ptr(),
                        i32::from(decrypt),
                    );
                };
                safe.as_ref()
                    .crypt_zeros(&mut zeros, Some(&mut iv_b), direction(decrypt))
                    .unwrap();
                assert_eq!(zeros, expected);
                assert_eq!(iv_a, iv_b);
            }
            let src = &input[1..=(blocks * 8)];
            let mut expected_mac = [0; 8];
            // SAFETY: live context; count input blocks, eight-byte writable result.
            // MAC leaves output unchanged for empty input, initialized above.
            unsafe {
                ffi::av_des_mac(
                    raw.0,
                    expected_mac.as_mut_ptr(),
                    src.as_ptr(),
                    blocks as i32,
                );
            };
            assert_eq!(safe.as_ref().mac(src).unwrap(), expected_mac);
        }
    }
}
